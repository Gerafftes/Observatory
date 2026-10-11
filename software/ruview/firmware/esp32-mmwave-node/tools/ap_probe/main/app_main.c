#include <inttypes.h>
#include <string.h>

#include "esp_app_desc.h"
#include "esp_chip_info.h"
#include "esp_event.h"
#include "esp_heap_caps.h"
#include "esp_log.h"
#include "esp_netif.h"
#include "esp_ota_ops.h"
#include "esp_system.h"
#include "esp_timer.h"
#include "esp_wifi.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "nvs_flash.h"

static const char *TAG = "mmwave2_ap_probe";
ESP_EVENT_DEFINE_BASE(PROBE_EVENT);
static portMUX_TYPE s_lock = portMUX_INITIALIZER_UNLOCKED;
static esp_netif_t *s_ap;
typedef struct {
    bool started;
    unsigned starts, stops, joins, leaves, leases, probes, ticks;
    int last_probe_rssi;
    int64_t last_probe_us;
} probe_events_t;
static probe_events_t s_events;

static void event_handler(void *argument, esp_event_base_t base,
                          int32_t id, void *data)
{
    (void)argument;
    portENTER_CRITICAL(&s_lock);
    if (base == PROBE_EVENT) {
        ++s_events.ticks;
    } else if (base == IP_EVENT && id == IP_EVENT_AP_STAIPASSIGNED) {
        ++s_events.leases;
    } else if (base == WIFI_EVENT) {
        switch (id) {
        case WIFI_EVENT_AP_START: s_events.started = true; ++s_events.starts; break;
        case WIFI_EVENT_AP_STOP: s_events.started = false; ++s_events.stops; break;
        case WIFI_EVENT_AP_STACONNECTED: ++s_events.joins; break;
        case WIFI_EVENT_AP_STADISCONNECTED: ++s_events.leaves; break;
        case WIFI_EVENT_AP_PROBEREQRECVED:
            ++s_events.probes;
            s_events.last_probe_rssi = ((wifi_event_ap_probe_req_rx_t *)data)->rssi;
            s_events.last_probe_us = esp_timer_get_time();
            break;
        default: break;
        }
    }
    portEXIT_CRITICAL(&s_lock);

    // Probe traffic is counted, not logged per packet: avoid flooding the event loop.
    if (base == PROBE_EVENT || (base == WIFI_EVENT && id == WIFI_EVENT_AP_PROBEREQRECVED)) {
        return;
    }
    if (base == WIFI_EVENT && id == WIFI_EVENT_AP_STADISCONNECTED) {
        const wifi_event_ap_stadisconnected_t *e = data;
        ESP_LOGW(TAG, "CLIENT_LEAVE aid=%u reason=%u", e->aid, e->reason);
    } else if (base == WIFI_EVENT && id == WIFI_EVENT_AP_STACONNECTED) {
        ESP_LOGI(TAG, "CLIENT_JOIN aid=%u", ((wifi_event_ap_staconnected_t *)data)->aid);
    } else if (base == WIFI_EVENT && id == WIFI_EVENT_AP_STOP) {
        ESP_LOGE(TAG, "AP_STOP unexpected; no automatic restart in diagnostic mode");
    } else if (base == WIFI_EVENT && id == WIFI_EVENT_AP_START) {
        ESP_LOGI(TAG, "AP_START driver event (not proof of over-air beacons)");
    } else if (base == WIFI_EVENT && id == WIFI_EVENT_HOME_CHANNEL_CHANGE) {
        const wifi_event_home_channel_change_t *e = data;
        ESP_LOGW(TAG, "CHANNEL_CHANGE old=%u new=%u", e->old_chan, e->new_chan);
    } else if (base == IP_EVENT && id == IP_EVENT_AP_STAIPASSIGNED) {
        ESP_LOGI(TAG, "DHCP_LEASE ip=" IPSTR, IP2STR(&((ip_event_ap_staipassigned_t *)data)->ip));
    } else {
        ESP_LOGI(TAG, "EVENT base=%s id=%" PRId32, base, id);
    }
}

static void heartbeat(void)
{
    static unsigned previous_ticks;
    static bool tick_posted;
    int64_t now = esp_timer_get_time();
    portENTER_CRITICAL(&s_lock);
    probe_events_t e = s_events;
    portEXIT_CRITICAL(&s_lock);
    ESP_LOGI(TAG, "HEARTBEAT uptime_ms=%" PRId64 " started=%d starts=%u stops=%u joins=%u leaves=%u leases=%u probes=%u probe_rssi=%d probe_age_ms=%" PRId64 " ticks=%u",
             now / 1000, e.started, e.starts, e.stops, e.joins, e.leaves, e.leases,
             e.probes, e.last_probe_rssi, e.probes ? (now - e.last_probe_us) / 1000 : -1, e.ticks);
    wifi_mode_t mode = WIFI_MODE_NULL;
    uint8_t channel = 0;
    wifi_second_chan_t second = WIFI_SECOND_CHAN_NONE;
    int8_t power = 0;
    wifi_sta_list_t stations = {0};
    esp_err_t m = esp_wifi_get_mode(&mode);
    esp_err_t c = esp_wifi_get_channel(&channel, &second);
    esp_err_t p = esp_wifi_get_max_tx_power(&power);
    esp_err_t s = esp_wifi_ap_get_sta_list(&stations);
    esp_netif_dhcp_status_t dhcp = ESP_NETIF_DHCP_INIT;
    esp_err_t d = esp_netif_dhcps_get_status(s_ap, &dhcp);
    ESP_LOGI(TAG, "STATE mode=%d/%s channel=%u/%s tx_qdbm=%d/%s stations=%d/%s netif_up=%d dhcp=%d/%s",
             mode, esp_err_to_name(m), channel, esp_err_to_name(c), power, esp_err_to_name(p),
             stations.num, esp_err_to_name(s), esp_netif_is_netif_up(s_ap), dhcp, esp_err_to_name(d));
    for (int i = 0; s == ESP_OK && i < stations.num; ++i) {
        ESP_LOGI(TAG, "CLIENT index=%d rssi=%d phy_11n=%u", i, stations.sta[i].rssi, stations.sta[i].phy_11n);
    }
    ESP_LOGI(TAG, "MEMORY free=%" PRIu32 " minimum=%" PRIu32 " largest=%u stack_min=%u integrity=%d",
             esp_get_free_heap_size(), esp_get_minimum_free_heap_size(),
             (unsigned)heap_caps_get_largest_free_block(MALLOC_CAP_8BIT),
             (unsigned)uxTaskGetStackHighWaterMark(NULL), heap_caps_check_integrity_all(true));
    esp_err_t tick = esp_event_post(PROBE_EVENT, 0, NULL, 0, 0);
    bool stalled = tick_posted && e.ticks == previous_ticks;
    if (tick != ESP_OK || stalled || !e.started || m != ESP_OK || c != ESP_OK ||
        p != ESP_OK || s != ESP_OK || d != ESP_OK || mode != WIFI_MODE_AP) {
        ESP_LOGE(TAG, "HEALTH_ERROR event_post=%s event_loop_stalled=%d", esp_err_to_name(tick), stalled);
    }
    previous_ticks = e.ticks;
    tick_posted = tick == ESP_OK;
}

void app_main(void)
{
    const esp_app_desc_t *app = esp_app_get_description();
    esp_chip_info_t chip;
    esp_chip_info(&chip);
    const esp_partition_t *running = esp_ota_get_running_partition();
    ESP_LOGI(TAG, "BOOT reset_reason=%d idf=%s app=%s version=%s chip=%d rev=%u cores=%u partition=%s@0x%" PRIx32,
             esp_reset_reason(), esp_get_idf_version(), app->project_name, app->version,
             chip.model, chip.revision, chip.cores, running->label, running->address);
    ESP_LOG_BUFFER_HEX_LEVEL(TAG, app->app_elf_sha256, sizeof(app->app_elf_sha256), ESP_LOG_INFO);
    size_t password_length = strlen(CONFIG_AP_PROBE_PASSWORD);
    if (password_length < 8 || password_length > 63) {
        ESP_LOGE(TAG, "Private WPA2 password must contain 8-63 characters; AP not started");
        return;
    }
    // Preserve provisioned NVS: initialization failures must not trigger erase/reprovision.
    ESP_ERROR_CHECK(nvs_flash_init());
    ESP_ERROR_CHECK(esp_netif_init());
    ESP_ERROR_CHECK(esp_event_loop_create_default());
    s_ap = esp_netif_create_default_wifi_ap();
    ESP_ERROR_CHECK(s_ap ? ESP_OK : ESP_ERR_NO_MEM);
    ESP_ERROR_CHECK(esp_event_handler_register(WIFI_EVENT, ESP_EVENT_ANY_ID, event_handler, NULL));
    ESP_ERROR_CHECK(esp_event_handler_register(IP_EVENT, IP_EVENT_AP_STAIPASSIGNED, event_handler, NULL));
    ESP_ERROR_CHECK(esp_event_handler_register(PROBE_EVENT, ESP_EVENT_ANY_ID, event_handler, NULL));
    wifi_init_config_t init = WIFI_INIT_CONFIG_DEFAULT();
    ESP_ERROR_CHECK(esp_wifi_init(&init));
    ESP_ERROR_CHECK(esp_wifi_set_storage(WIFI_STORAGE_RAM));
    ESP_ERROR_CHECK(esp_wifi_set_mode(WIFI_MODE_AP));
    wifi_config_t config = {.ap = {
        .ssid = "MMWAVE2-RF-TEST", .ssid_len = sizeof("MMWAVE2-RF-TEST") - 1,
        .password = CONFIG_AP_PROBE_PASSWORD, .channel = CONFIG_AP_PROBE_CHANNEL,
        .max_connection = 1, .authmode = WIFI_AUTH_WPA2_PSK,
        .ssid_hidden = 0, .beacon_interval = 100, .pmf_cfg = {.required = true},
    }};
    ESP_ERROR_CHECK(esp_wifi_set_config(WIFI_IF_AP, &config));
    uint32_t mask;
    ESP_ERROR_CHECK(esp_wifi_get_event_mask(&mask));
    ESP_ERROR_CHECK(esp_wifi_set_event_mask(mask & ~WIFI_EVENT_MASK_AP_PROBEREQRECVED));
    ESP_ERROR_CHECK(esp_wifi_start());
    ESP_ERROR_CHECK(esp_wifi_set_max_tx_power(CONFIG_AP_PROBE_TX_POWER));
    ESP_ERROR_CHECK(esp_wifi_get_config(WIFI_IF_AP, &config));
    wifi_bandwidth_t bandwidth;
    wifi_country_t country;
    uint8_t protocol;
    ESP_ERROR_CHECK(esp_wifi_get_bandwidth(WIFI_IF_AP, &bandwidth));
    ESP_ERROR_CHECK(esp_wifi_get_country(&country));
    ESP_ERROR_CHECK(esp_wifi_get_protocol(WIFI_IF_AP, &protocol));
    ESP_LOGI(TAG, "CONFIG channel=%u hidden=%u auth=%d pmf_required=%u beacon_ms=%u max_clients=%u bandwidth=%d protocol=0x%x country=%.2s channels=%u+%u",
             config.ap.channel, config.ap.ssid_hidden, config.ap.authmode, config.ap.pmf_cfg.required,
             config.ap.beacon_interval, config.ap.max_connection, bandwidth, protocol, country.cc, country.schan, country.nchan);
    esp_ota_img_states_t state;
    esp_err_t ota = esp_ota_get_state_partition(running, &state);
    ESP_LOGI(TAG, "OTA state=%d result=%s", ota == ESP_OK ? (int)state : -1, esp_err_to_name(ota));
    if (ota == ESP_OK && state == ESP_OTA_IMG_PENDING_VERIFY) {
        ESP_ERROR_CHECK(esp_ota_mark_app_valid_cancel_rollback());
    }
    while (true) {
        vTaskDelay(pdMS_TO_TICKS(5000));
        heartbeat();
    }
}

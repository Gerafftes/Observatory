#include "wifi_reconnect.h"
#include "nvs_config.h"
#include "csi_peer.h"
#include "status_led.h"
#include "stream_sender.h"
#include "esp_wifi.h"
#include "esp_event.h"
#include "esp_netif.h"
#include "esp_log.h"
#include "esp_timer.h"
#include "freertos/FreeRTOS.h"
#include "freertos/event_groups.h"
#include <string.h>
extern nvs_config_t g_nvs_config;
static const char *TAG = "rx_wifi";
/* Event group bits */
#define WIFI_CONNECTED_BIT BIT0
#define WIFI_FAIL_BIT      BIT1

static EventGroupHandle_t s_wifi_event_group;
static int s_retry_num = 0;
#define MAX_RETRY 10
static esp_timer_handle_t s_wifi_reconnect_timer;
static portMUX_TYPE s_reconnect_lock = portMUX_INITIALIZER_UNLOCKED;
static wifi_reconnect_snapshot_t s_reconnect;
static int64_t s_wifi_log_window_us;
static uint32_t s_wifi_log_lines;
static wifi_reconnect_event_t s_events[WIFI_RECONNECT_EVENT_CAPACITY];
_Static_assert(sizeof(s_reconnect) <= 128, "bounded WiFi reconnect diagnostic");

static void wifi_diag_add(uint32_t *count)
{
    if (*count == UINT32_MAX) s_reconnect.saturated = true;
    else ++*count;
}

/* At most eight fixed numeric lines per 500ms window. Counter/sequence
 * gaps remain visible: suppressed lines are never evidence of event absence. */
static void wifi_reconnect_trace(unsigned event, uint32_t sequence, int64_t when, int32_t rc)
{
    portENTER_CRITICAL(&s_reconnect_lock);
    if (s_events[sequence % WIFI_RECONNECT_EVENT_CAPACITY].sequence<=sequence)
    s_events[sequence % WIFI_RECONNECT_EVENT_CAPACITY] = (wifi_reconnect_event_t){
        .event=event,.sequence=sequence,.time_us=when,.result=rc,
    };
    int64_t now = esp_timer_get_time();
    if (s_wifi_log_lines == 0 || now - s_wifi_log_window_us >= 500000) {
        s_wifi_log_window_us = now;
        s_wifi_log_lines = 0;
    }
    bool emit = s_wifi_log_lines < 8;
    if (emit) ++s_wifi_log_lines;
    else wifi_diag_add(&s_reconnect.log_suppressed);
    portEXIT_CRITICAL(&s_reconnect_lock);
    if (!emit) return;
    wifi_reconnect_snapshot_t sample; wifi_reconnect_snapshot(&sample);
    switch (event) {
    case 1:
        ESP_LOGI(TAG, "wifi-reconnect ev=1 seq=%lu us=%lld r=%lu a=%lu au=%lld ar=%ld ms=%lu sup=%lu ss=%lu su=%lld",
                 (unsigned long)sequence,(long long)when,(unsigned long)(uint32_t)rc,
                 (unsigned long)sample.arm_count,(long long)sample.arm_us,(long)sample.arm_rc,
                 (unsigned long)sample.arm_delay_ms,(unsigned long)sample.log_suppressed,(unsigned long)sample.event_seq,(long long)sample.sample_us);
        break;
    case 2:
        ESP_LOGI(TAG, "wifi-reconnect ev=2 seq=%lu us=%lld n=%lu active=%u connected=%u sup=%lu ss=%lu su=%lld",
                 (unsigned long)sequence,(long long)when,(unsigned long)sample.callback_count,
                 (unsigned)sample.timer_active,(unsigned)sample.connected,(unsigned long)sample.log_suppressed,(unsigned long)sample.event_seq,(long long)sample.sample_us);
        break;
    case 3:
        ESP_LOGI(TAG, "wifi-reconnect ev=3 seq=%lu us=%lld origin=%ld n=%lu sup=%lu ss=%lu su=%lld",
                 (unsigned long)sequence,(long long)when,(long)rc,
                 (unsigned long)sample.connect_started_count,(unsigned long)sample.log_suppressed,(unsigned long)sample.event_seq,(long long)sample.sample_us);
        break;
    case 4:
        ESP_LOGI(TAG, "wifi-reconnect ev=4 seq=%lu us=%lld rc=%ld a=%lu au=%lld ar=%ld ms=%lu sup=%lu ss=%lu su=%lld",
                 (unsigned long)sequence,(long long)when,(long)rc,(unsigned long)sample.arm_count,
                 (long long)sample.arm_us,(long)sample.arm_rc,(unsigned long)sample.arm_delay_ms,
                 (unsigned long)sample.log_suppressed,(unsigned long)sample.event_seq,(long long)sample.sample_us);
        break;
    case 5:
        ESP_LOGI(TAG, "wifi-reconnect ev=5 seq=%lu us=%lld n=%lu active=%u sup=%lu ss=%lu su=%lld",
                 (unsigned long)sequence,(long long)when,(unsigned long)sample.got_ip_count,
                 (unsigned)sample.timer_active,(unsigned long)sample.log_suppressed,(unsigned long)sample.event_seq,(long long)sample.sample_us);
        break;
    }

}

/* Caller owns s_reconnect_lock. Timer operations do not allocate or invoke
 * app callbacks; the SDK releases its timer-list lock before callback entry. */
static uint32_t wifi_reconnect_schedule_locked(bool *failed)
{
    *failed = false;
    if (s_reconnect.connected) return 0;
    if (s_wifi_reconnect_timer == NULL) { *failed = true; return 0; }
    if (esp_timer_is_active(s_wifi_reconnect_timer)) return 0;
    *failed = s_retry_num == MAX_RETRY;
    uint32_t shift = s_retry_num < 6 ? (uint32_t)s_retry_num : 6u;
    uint32_t delay_ms = 500u << shift;
    if (delay_ms > 30000u) delay_ms = 30000u;
    if (s_retry_num < MAX_RETRY) s_retry_num++;
    wifi_diag_add(&s_reconnect.arm_count);
    s_reconnect.arm_us = esp_timer_get_time();
    s_reconnect.arm_delay_ms = delay_ms;
    s_reconnect.arm_rc = esp_timer_start_once(s_wifi_reconnect_timer, (uint64_t)delay_ms * 1000u);
    if (s_reconnect.arm_rc != ESP_OK) *failed = true;
    return s_reconnect.arm_rc == ESP_OK ? delay_ms : 0;
}

static esp_err_t wifi_connect_once(uint32_t origin)
{
    portENTER_CRITICAL(&s_reconnect_lock);
    if (s_reconnect.connected || s_reconnect.connect_in_flight
        || (origin == 2 && s_wifi_reconnect_timer != NULL && esp_timer_is_active(s_wifi_reconnect_timer))) {
        portEXIT_CRITICAL(&s_reconnect_lock);
        return ESP_OK;
    }
    s_reconnect.connect_in_flight = true;
    wifi_diag_add(&s_reconnect.connect_started_count);
    wifi_diag_add(&s_reconnect.event_seq);
    s_reconnect.connect_begin_us = esp_timer_get_time();
    uint32_t begin_seq = s_reconnect.event_seq;
    int64_t begin_us = s_reconnect.connect_begin_us;
    portEXIT_CRITICAL(&s_reconnect_lock);
    wifi_reconnect_trace(3, begin_seq, begin_us, (int32_t)origin);
    /* WiFi calls can block; never execute them in an application critical section. */
    esp_err_t result = esp_wifi_connect();
    portENTER_CRITICAL(&s_reconnect_lock);
    s_reconnect.connect_in_flight = false;
    wifi_diag_add(&s_reconnect.connect_count);
    s_reconnect.connect_us = esp_timer_get_time();
    s_reconnect.connect_rc = result;
    s_reconnect.connect_origin = origin;
    /* GOT_IP may have arrived while connect returned. Its connected state
     * and timer cancellation must win over this failed-call completion. */
    bool failed = false;
    if (result != ESP_OK) (void)wifi_reconnect_schedule_locked(&failed);
    wifi_diag_add(&s_reconnect.event_seq);
    uint32_t end_seq = s_reconnect.event_seq;
    int64_t end_us = s_reconnect.connect_us;
    portEXIT_CRITICAL(&s_reconnect_lock);
    if (failed) xEventGroupSetBits(s_wifi_event_group, WIFI_FAIL_BIT);
    wifi_reconnect_trace(4, end_seq, end_us, result);
    return result;
}

static void wifi_reconnect_timer_cb(void *arg)
{
    (void)arg;
    portENTER_CRITICAL(&s_reconnect_lock);
    wifi_diag_add(&s_reconnect.callback_count);
    s_reconnect.callback_us = esp_timer_get_time();
    wifi_diag_add(&s_reconnect.event_seq);
    uint32_t sequence = s_reconnect.event_seq;
    int64_t when = s_reconnect.callback_us;
    portEXIT_CRITICAL(&s_reconnect_lock);
    wifi_reconnect_trace(2, sequence, when, 0);
    (void)wifi_connect_once(2);
}

void wifi_reconnect_snapshot(wifi_reconnect_snapshot_t *snapshot)
{
    if (snapshot == NULL) return;
    portENTER_CRITICAL(&s_reconnect_lock);
    *snapshot = s_reconnect;
    snapshot->retry_count = (uint32_t)s_retry_num;
    snapshot->timer_active = s_wifi_reconnect_timer != NULL && esp_timer_is_active(s_wifi_reconnect_timer);
    snapshot->sample_us = esp_timer_get_time();
    portEXIT_CRITICAL(&s_reconnect_lock);
}

size_t wifi_reconnect_events(wifi_reconnect_event_t *out, size_t capacity)
{
    if (!out || !capacity) return 0;
    wifi_reconnect_event_t events[WIFI_RECONNECT_EVENT_CAPACITY]; size_t count=0;
    portENTER_CRITICAL(&s_reconnect_lock); memcpy(events,s_events,sizeof(events)); portEXIT_CRITICAL(&s_reconnect_lock);
    for (unsigned i=0;i<WIFI_RECONNECT_EVENT_CAPACITY;++i) {
        if (!events[i].sequence) continue;
        size_t at=count;
        if (at>=capacity) continue;
        while (at>0 && out[at-1].sequence>events[i].sequence) { out[at]=out[at-1]; --at; }
        out[at]=events[i]; ++count;
    }
    return count;
}

static void event_handler(void *arg, esp_event_base_t event_base,
                          int32_t event_id, void *event_data)
{
    if (event_base == WIFI_EVENT && event_id == WIFI_EVENT_STA_START) {
        (void)wifi_connect_once(1);
    } else if (event_base == WIFI_EVENT && event_id == WIFI_EVENT_STA_DISCONNECTED) {
        portENTER_CRITICAL(&s_reconnect_lock);
        s_reconnect.connected = false;
        wifi_diag_add(&s_reconnect.disconnect_count);
        s_reconnect.disconnect_us = esp_timer_get_time();
        s_reconnect.disconnect_reason = event_data != NULL
            ? ((wifi_event_sta_disconnected_t *)event_data)->reason : UINT32_MAX;
        bool failed;
        (void)wifi_reconnect_schedule_locked(&failed);
        wifi_diag_add(&s_reconnect.event_seq);
        uint32_t sequence = s_reconnect.event_seq;
        int64_t when = s_reconnect.disconnect_us;
        uint32_t reason = s_reconnect.disconnect_reason;
        portEXIT_CRITICAL(&s_reconnect_lock);
        if (failed) xEventGroupSetBits(s_wifi_event_group, WIFI_FAIL_BIT);
        status_led_network(false);
        xEventGroupClearBits(s_wifi_event_group, WIFI_CONNECTED_BIT);
        stream_sender_notify_network_change();
        wifi_reconnect_trace(1, sequence, when, (int32_t)reason);
    } else if (event_base == IP_EVENT && event_id == IP_EVENT_STA_GOT_IP) {
        portENTER_CRITICAL(&s_reconnect_lock);
        s_reconnect.connected = true;
        s_retry_num = 0;
        wifi_diag_add(&s_reconnect.got_ip_count);
        s_reconnect.got_ip_us = esp_timer_get_time();
        if (s_wifi_reconnect_timer != NULL && esp_timer_is_active(s_wifi_reconnect_timer))
            (void)esp_timer_stop(s_wifi_reconnect_timer);
        wifi_diag_add(&s_reconnect.event_seq);
        uint32_t sequence = s_reconnect.event_seq;
        int64_t when = s_reconnect.got_ip_us;
        portEXIT_CRITICAL(&s_reconnect_lock);
        stream_sender_notify_network_change();
        status_led_network(true);
        xEventGroupClearBits(s_wifi_event_group, WIFI_FAIL_BIT);
        xEventGroupSetBits(s_wifi_event_group, WIFI_CONNECTED_BIT);
        wifi_reconnect_trace(5, sequence, when, 0);
    }
}

esp_err_t rx_wifi_start(bool *connected)
{
    *connected = false;
    esp_err_t err = ESP_OK;
    bool initialized = false, loop = false, registered_wifi = false, registered_ip = false;
    esp_netif_t *station = NULL;
    esp_event_handler_instance_t instance_any_id = NULL, instance_got_ip = NULL;
    s_wifi_event_group = xEventGroupCreate();
    if (!s_wifi_event_group) { status_led_init_failed(); return ESP_ERR_NO_MEM; }
#define WIFI_INIT(call) do { err = (call); if (err != ESP_OK) goto fail; } while (0)

    WIFI_INIT(esp_netif_init());
    WIFI_INIT(esp_event_loop_create_default()); loop = true;
    station = esp_netif_create_default_wifi_sta();
    if (!station) { err = ESP_ERR_NO_MEM; goto fail; }

    wifi_init_config_t cfg = WIFI_INIT_CONFIG_DEFAULT();
    WIFI_INIT(esp_wifi_init(&cfg)); initialized = true;

    esp_timer_create_args_t reconnect_timer_args = {
        .callback = wifi_reconnect_timer_cb,
        .arg = NULL,
        .name = "wifi_reconnect",
    };
    WIFI_INIT(esp_timer_create(&reconnect_timer_args, &s_wifi_reconnect_timer));

    WIFI_INIT(esp_event_handler_instance_register(
        WIFI_EVENT, ESP_EVENT_ANY_ID, &event_handler, NULL, &instance_any_id)); registered_wifi = true;
    WIFI_INIT(esp_event_handler_instance_register(
        IP_EVENT, IP_EVENT_STA_GOT_IP, &event_handler, NULL, &instance_got_ip)); registered_ip = true;

    wifi_config_t wifi_config = {
        .sta = {
            .threshold.authmode = WIFI_AUTH_WPA2_PSK,
        },
    };

    /* Copy runtime SSID/password from NVS config */
    memcpy(wifi_config.sta.ssid, g_nvs_config.wifi_ssid, strlen(g_nvs_config.wifi_ssid));
    memcpy(wifi_config.sta.password, g_nvs_config.wifi_password, strlen(g_nvs_config.wifi_password));

    /* If password is empty, use open auth */
    if (g_nvs_config.wifi_password[0] == 0) {
        wifi_config.sta.threshold.authmode = WIFI_AUTH_OPEN;
    }

    WIFI_INIT(esp_wifi_set_mode(WIFI_MODE_STA));
    WIFI_INIT(esp_wifi_set_config(WIFI_IF_STA, &wifi_config));

    /* Negotiated HT20 leaves the source CSI layout observable and unchanged. */
    WIFI_INIT(esp_wifi_set_bandwidth(WIFI_IF_STA, WIFI_BW_HT20));
    WIFI_INIT(csi_peer_prepare_radio());
    WIFI_INIT(esp_wifi_start());

    /* Wait for connection */
    EventBits_t bits = xEventGroupWaitBits(s_wifi_event_group,
        WIFI_CONNECTED_BIT | WIFI_FAIL_BIT,
        pdFALSE, pdFALSE, portMAX_DELAY);

    if (bits & WIFI_CONNECTED_BIT) {
        ESP_LOGI(TAG, "Connected to WiFi");
        *connected = true;
    } else if (bits & WIFI_FAIL_BIT) {
        ESP_LOGE(TAG, "Failed to connect to WiFi after %d retries", MAX_RETRY);
    }
    return ESP_OK;
fail:
    status_led_init_failed();
    ESP_LOGE(TAG, "WiFi initialization failed: %s; no reset or retry", esp_err_to_name(err));
    if (registered_wifi) (void)esp_event_handler_instance_unregister(WIFI_EVENT, ESP_EVENT_ANY_ID, instance_any_id);
    if (registered_ip) (void)esp_event_handler_instance_unregister(IP_EVENT, IP_EVENT_STA_GOT_IP, instance_got_ip);
    if (s_wifi_reconnect_timer) {
        (void)esp_timer_stop(s_wifi_reconnect_timer); (void)esp_timer_delete(s_wifi_reconnect_timer);
        s_wifi_reconnect_timer = NULL;
    }
    if (initialized) { (void)esp_wifi_stop(); (void)esp_wifi_deinit(); }
    if (station) esp_netif_destroy_default_wifi(station);
    if (loop) (void)esp_event_loop_delete_default();
    vEventGroupDelete(s_wifi_event_group); s_wifi_event_group = NULL;
    return err;
#undef WIFI_INIT
}


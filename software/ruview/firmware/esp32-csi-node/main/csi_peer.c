/* Generalized from the hash-bound Gen02 peer implementation. The existing
 * transport owns every capture; this module adds no parallel capture queue. */
#include "csi_peer.h"
#include "rx_budget.h"
#include "rx_settings.h"
#include "csi_collector.h"
#include "esp_timer.h"
#include "esp_wifi.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "mbedtls/sha256.h"
#include <string.h>

static portMUX_TYPE lock = portMUX_INITIALIZER_UNLOCKED;
static rx_budget_t budget;
static csi_peer_stats_t stats;
static uint8_t self_mac[6], hashes[4][32];
static bool initialized, stopping, radio_prepared;
static TaskHandle_t task;
static uint32_t send_sequence;

static bool open_locked(int64_t now)
{
    bool open = initialized && !stopping
        && rx_budget_peer_open(&budget, rx_settings_active_profile(), now);
    stats.session_ended = !open;
    return open;
}

int csi_peer_classify(const uint8_t mac[6])
{
    if (!initialized || !mac || !memcmp(mac, self_mac, 6)) return -1;
    return rx_profile_link(rx_settings_active_profile(), mac);
}

bool csi_peer_admit(const uint8_t mac[6], size_t iq_len, int64_t now)
{
    int link = csi_peer_classify(mac);
    if (link < 0) return false;
    const rx_profile_t *p = rx_settings_active_profile();
    portENTER_CRITICAL(&lock);
    /* Order budget time with budget access, not with an earlier callback
     * observation that another core may have overtaken before this lock. */
    int64_t budget_now = esp_timer_get_time();
    csi_peer_link_stats_t *s = &stats.links[link];
    ++s->observed;
    bool valid = now >= 0 && now <= budget_now
        && iq_len > 0 && iq_len <= CSI_MAX_IQ_SIZE && !(iq_len & 1u);
    bool waiting = stats.links[1].queued + stats.links[2].queued + stats.links[3].queued > 0;
    /* Peer captures defer admission to the existing sender. That avoids
     * phase-lock starvation when TX callbacks occupy every arrival slot. */
    bool accepted = valid && (link ? open_locked(budget_now)
        : !stopping && rx_budget_admit(&budget, p, 0, budget_now, waiting));
    if (accepted && link == 0) ++s->admitted;
    if (!accepted) ++s->rejected;
    portEXIT_CRITICAL(&lock);
    return accepted;
}

bool csi_peer_queue_reserve(int link)
{
    if (link < 0 || link > 3) return false;
    const rx_profile_t *p = rx_settings_active_profile();
    portENTER_CRITICAL(&lock);
    unsigned peers = stats.links[1].queued + stats.links[2].queued + stats.links[3].queued;
    bool ok = !stopping && (link == 0
        ? stats.links[0].queued < 8u - p->peer_count
        : peers < p->peer_count && stats.links[link].queued == 0);
    csi_peer_link_stats_t *s = &stats.links[link];
    if (ok) { ++s->captured; ++s->queued; if (s->queued > s->high_watermark) s->high_watermark = s->queued; }
    portEXIT_CRITICAL(&lock);
    return ok;
}

void csi_peer_capture_result(int link, bool accepted)
{
    if (link < 0 || link > 3) return;
    portENTER_CRITICAL(&lock);
    if (!accepted) ++stats.links[link].queue_drops;
    portEXIT_CRITICAL(&lock);
}

bool csi_peer_deferred_admit(int link, int64_t callback_us, int64_t now)
{
    if (link < 1 || link > 3 || callback_us < 0 || now < callback_us || now - callback_us > 100000) return false;
    portENTER_CRITICAL(&lock);
    int64_t budget_now = esp_timer_get_time();
    bool accepted = budget_now >= callback_us && budget_now - callback_us <= 100000
        && open_locked(budget_now)
        && rx_budget_admit(&budget, rx_settings_active_profile(), (unsigned)link, budget_now, true);
    if (accepted) ++stats.links[link].admitted;
    portEXIT_CRITICAL(&lock);
    return accepted;
}

void csi_peer_finish(int link, bool serialized, bool delivered, bool expired)
{
    if (link < 0 || link > 3) return;
    portENTER_CRITICAL(&lock);
    csi_peer_link_stats_t *s = &stats.links[link];
    if (s->queued) --s->queued;
    if (!serialized) ++s->serialize_errors;
    else if (delivered) ++s->delivered;
    else ++s->abandoned;
    if (expired) ++s->expired;
    portEXIT_CRITICAL(&lock);
}

void csi_peer_aborted(int link)
{
    if (link < 0 || link > 3) return;
    portENTER_CRITICAL(&lock);
    if (stats.links[link].queued) --stats.links[link].queued;
    ++stats.links[link].aborted;
    portEXIT_CRITICAL(&lock);
}

static void sender(void *unused)
{
    (void)unused;
    const rx_profile_t *p = rx_settings_active_profile();
    unsigned destination = 0;
    for (;;) {
        portENTER_CRITICAL(&lock);
        bool open = open_locked(esp_timer_get_time());
        stats.task_stack_min_bytes = (uint32_t)uxTaskGetStackHighWaterMark(NULL);
        portEXIT_CRITICAL(&lock);
        if (!open) break;
        wifi_ap_record_t ap;
        if (esp_wifi_sta_get_ap_info(&ap) == ESP_OK) {
            uint8_t frame[48] = {0};
            frame[0] = 0xd0;
            memcpy(frame + 4, p->peers[destination].mac, 6);
            memcpy(frame + 10, self_mac, 6); memcpy(frame + 16, ap.bssid, 6);
            frame[24] = 127; frame[25] = 0x02; frame[26] = 'R'; frame[27] = 'V';
            memcpy(frame + 28, "RVPR", 4); frame[32] = 1; frame[33] = p->node_id;
            uint64_t boot = csi_collector_get_boot_id();
            for (unsigned i = 0; i < 8; ++i) frame[34 + i] = boot >> (8 * i);
            uint32_t sequence = ++send_sequence;
            for (unsigned i = 0; i < 4; ++i) frame[42 + i] = sequence >> (8 * i);
            frame[46] = frame[47] = 0xff;
            esp_err_t result = esp_wifi_80211_tx(WIFI_IF_STA, frame, sizeof(frame), true);
            portENTER_CRITICAL(&lock);
            ++stats.raw_send_attempts;
            if (result != ESP_OK) ++stats.raw_send_errors;
            portEXIT_CRITICAL(&lock);
            destination = (destination + 1) % p->peer_count;
        }
        vTaskDelay(pdMS_TO_TICKS(100)); /* one total sender budget, no catch-up bursts */
    }
    portENTER_CRITICAL(&lock); stats.task_running = false; task = NULL; portEXIT_CRITICAL(&lock);
    vTaskDelete(NULL);
}

esp_err_t csi_peer_start(void)
{
    const rx_profile_t *p = rx_settings_active_profile();
    if (initialized || !rx_profile_valid(p)) return ESP_ERR_INVALID_STATE;
    if (esp_wifi_get_mac(WIFI_IF_STA, self_mac) != ESP_OK || !rx_mac_valid(self_mac)
        || !memcmp(self_mac, p->tx_mac, 6)) return ESP_ERR_INVALID_STATE;
    for (unsigned i = 0; i < p->peer_count; ++i)
        if (!memcmp(self_mac, p->peers[i].mac, 6)) return ESP_ERR_INVALID_ARG;
    for (unsigned i = 0; i <= p->peer_count; ++i)
        if (mbedtls_sha256(i ? p->peers[i - 1].mac : p->tx_mac, 6, hashes[i], 0)) return ESP_FAIL;
    rx_budget_init(&budget); memset(&stats, 0, sizeof(stats)); stopping = false;
    if (p->peer_count && !radio_prepared) return ESP_ERR_INVALID_STATE;
    stats.radio_ready = radio_prepared;
    initialized = true; stats.enabled = p->peer_count != 0;
    if (p->peer_count) {
        stats.task_running = true;
        if (xTaskCreate(sender, "rx_peer", 4096, NULL, 4, &task) != pdPASS) {
            stats.task_running = false; initialized = false; return ESP_ERR_NO_MEM;
        }
    }
    return ESP_OK;
}

void csi_peer_stop(void)
{
    portENTER_CRITICAL(&lock); stopping = true; portEXIT_CRITICAL(&lock);
    for (;;) {
        portENTER_CRITICAL(&lock); bool running = stats.task_running; portEXIT_CRITICAL(&lock);
        if (!running) break;
        vTaskDelay(1);
    }
    initialized = false;
}

void csi_peer_get_stats(csi_peer_stats_t *out)
{
    portENTER_CRITICAL(&lock);
    (void)open_locked(esp_timer_get_time()); *out = stats;
    portEXIT_CRITICAL(&lock);
}
const uint8_t *csi_peer_source_hash(int link) { return link >= 0 && link <= 3 ? hashes[link] : NULL; }
const uint8_t *csi_peer_self_mac(void) { return self_mac; }

esp_err_t csi_peer_prepare_radio(void)
{
    if (!rx_settings_active_profile()->peer_count) return ESP_OK;
    esp_err_t err = esp_wifi_config_80211_tx_rate(WIFI_IF_STA, WIFI_PHY_RATE_6M);
    radio_prepared = err == ESP_OK;
    return err;
}

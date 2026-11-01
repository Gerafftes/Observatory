#include "rx_settings.h"
#include "rx_diag.h"
#include "wifi_reconnect.h"
#include "csi_collector.h"
#include "csi_transport.h"
#include "csi_peer.h"
#include "status_led.h"
#include "stream_sender.h"
#include "ota_update.h"
#include "rx_control.h"
#include "nvs_flash.h"
#include "esp_random.h"
#include "esp_log.h"
#include "esp_wifi.h"
#include <string.h>

nvs_config_t g_nvs_config;

void app_main(void)
{
    if (rx_diag_init() != ESP_OK) return;
    esp_err_t err = nvs_flash_init();
    if (err != ESP_OK) {
        ESP_LOGE("rx", "NVS unavailable (%d); preserved, explicit recovery required", err);
        return;
    }
    nvs_config_load(&g_nvs_config);
    rx_profile_t profile;
    err = rx_settings_load(&g_nvs_config, &profile);
    if (err != ESP_OK) {
        ESP_LOGE("rx", "UNPROVISIONED or invalid RX profile (%d); no CSI or peer start", err);
        return;
    }
    rx_settings_activate(&g_nvs_config, &profile);
    if (!rx_profile_valid(rx_settings_active_profile())) return;
    uint64_t boot = 0;
    while (!boot) esp_fill_random(&boot, sizeof(boot));
    csi_collector_set_node_id(profile.node_id);
    csi_collector_set_boot_id(boot);
    (void)status_led_start(profile.node_id);
    bool connected = false;
    if (rx_wifi_start(&connected) != ESP_OK) return;
    /* Restore the immutable application copy; never silently accept changed
     * node/source identity across WiFi initialization. */
    if (memcmp(&g_nvs_config, rx_settings_active_config(), sizeof(g_nvs_config))) {
        status_led_internal_failed();
        ESP_LOGE("rx", "configuration memory changed during WiFi initialization");
        return;
    }
    (void)stream_sender_init_with(g_nvs_config.target_ip, g_nvs_config.target_port);
    if (csi_peer_start() != ESP_OK || csi_transport_start() != ESP_OK) {
        csi_peer_stop();
        csi_transport_stop();
        stream_sender_deinit();
        status_led_init_failed();
        return;
    }
    bool capture_ready = csi_collector_init() == ESP_OK;
    if (!capture_ready) {
        csi_peer_stop();
        csi_transport_stop();
        status_led_init_failed();
    }
    void *server = NULL;
    bool ready = ota_update_init_ex(&server) == ESP_OK;
    ready = ready && rx_control_register(server) == ESP_OK;
    if (!ready) {
        (void)esp_wifi_set_csi(false);
        csi_collector_notify_network_change(false);
        csi_peer_stop();
        csi_transport_stop();
        stream_sender_deinit();
        status_led_init_failed();
    }
    /* AP reachability is not application integrity. Reconnect remains alive
     * when the AP is absent; qualification still requires fresh CSI and IP. */
    (void)ota_update_complete_boot_self_test(ready && capture_ready);
    ESP_LOGI("rx", "unified RX%u ready=%d connected=%d mode=%s generation=%lu",
             profile.node_id, ready, connected, rx_profile_mode_name(profile.mode),
             (unsigned long)profile.generation);
}

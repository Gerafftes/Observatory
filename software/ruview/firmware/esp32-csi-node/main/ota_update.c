/**
 * @file ota_update.c
 * @brief HTTP OTA firmware update for ESP32-S3 CSI Node.
 *
 * Uses ESP-IDF's native OTA API with rollback support.
 * The HTTP server runs on port 8032 and accepts:
 *   POST /ota — firmware binary payload (application/octet-stream)
 *   GET /ota/status — current firmware version and partition info
 */

#include "ota_update.h"

#include <string.h>
#include "esp_log.h"
#include "esp_ota_ops.h"
#include "esp_http_server.h"
#include "esp_app_desc.h"
#include "nvs_flash.h"
#include "nvs.h"
#include "esp_netif.h"
#include "esp_wifi.h"
#include "esp_system.h"
#include "csi_collector.h"
#include "csi_transport.h"
#include "stream_sender.h"

static const char *TAG = "ota_update";

#define OTA_HTTP_TASK_STACK_BYTES 8192U

/** OTA HTTP server port. */
#define OTA_PORT 8032

/** NVS namespace and key for the OTA pre-shared key. */
#define OTA_NVS_NAMESPACE "security"
#define OTA_NVS_KEY       "ota_psk"

/** Maximum PSK length (hex-encoded SHA-256). */
#define OTA_PSK_MAX_LEN   65

#ifndef CONFIG_CSI_CAPTURE_RATE_LIMIT_HZ
#define CONFIG_CSI_CAPTURE_RATE_LIMIT_HZ 50
#endif

#if defined(CONFIG_CSI_RAW_PROFILE) && CONFIG_CSI_RAW_PROFILE
#define OTA_RAW_PROFILE_JSON "true"
#else
#define OTA_RAW_PROFILE_JSON "false"
#endif

/** Cached PSK loaded from NVS at init time. Empty = reject all protected calls. */
static char s_ota_psk[OTA_PSK_MAX_LEN] = {0};

static const char *ota_state_name(esp_ota_img_states_t state)
{
    switch (state) {
    case ESP_OTA_IMG_NEW: return "NEW";
    case ESP_OTA_IMG_PENDING_VERIFY: return "PENDING_VERIFY";
    case ESP_OTA_IMG_VALID: return "VALID";
    case ESP_OTA_IMG_INVALID: return "INVALID";
    case ESP_OTA_IMG_ABORTED: return "ABORTED";
    case ESP_OTA_IMG_UNDEFINED:
    default: return "UNDEFINED";
    }
}

static void hex_encode(const uint8_t *bytes, size_t len, char *output)
{
    static const char HEX[] = "0123456789abcdef";
    for (size_t index = 0; index < len; ++index) {
        output[index * 2] = HEX[bytes[index] >> 4];
        output[index * 2 + 1] = HEX[bytes[index] & 0x0f];
    }
    output[len * 2] = '\0';
}

/**
 * ADR-050: Verify the Authorization header contains the correct PSK.
 * Returns true only when a PSK is provisioned AND the Bearer token
 * matches it. An unprovisioned node refuses all OTA requests
 * (fail-closed, see RuView#596 audit). The OTA server still starts so
 * the operator can `provision.py --ota-psk <hex>` over USB-CDC without
 * a reflash, but the upload endpoint will reject every request until
 * the PSK is set.
 */
bool ota_check_auth(httpd_req_t *req)
{
    if (s_ota_psk[0] == '\0') {
        /* No PSK provisioned — fail closed. Previously this returned
         * true ("permissive for dev"), which let any host on the WiFi
         * push attacker-controlled firmware to a freshly-flashed node.
         * Plain HTTP transport + no Secure Boot V2 + no signed-image
         * verification meant a single LAN call could brick or back-
         * door a node. Reject until provisioned. */
        ESP_LOGW(TAG, "OTA rejected: no PSK in NVS (run provision.py --ota-psk <hex>)");
        return false;
    }

    char auth_header[128] = {0};
    if (httpd_req_get_hdr_value_str(req, "Authorization", auth_header,
                                     sizeof(auth_header)) != ESP_OK) {
        return false;
    }

    /* Expect "Bearer <psk>" */
    const char *prefix = "Bearer ";
    if (strncmp(auth_header, prefix, strlen(prefix)) != 0) {
        return false;
    }

    const char *token = auth_header + strlen(prefix);
    /* Constant-time comparison to prevent timing attacks. */
    size_t psk_len = strlen(s_ota_psk);
    size_t tok_len = strlen(token);
    if (psk_len != tok_len) return false;
    volatile uint8_t result = 0;
    for (size_t i = 0; i < psk_len; i++) {
        result |= (uint8_t)(s_ota_psk[i] ^ token[i]);
    }
    return result == 0;
}

/**
 * GET /ota/status — return firmware version and partition info.
 */
static esp_err_t ota_status_handler(httpd_req_t *req)
{
    const esp_app_desc_t *app = esp_app_get_description();
    const esp_partition_t *running = esp_ota_get_running_partition();
    const esp_partition_t *update = esp_ota_get_next_update_partition(NULL);
    esp_ota_img_states_t ota_state = ESP_OTA_IMG_UNDEFINED;
    if (running != NULL) {
        (void)esp_ota_get_state_partition(running, &ota_state);
    }
    uint8_t image_sha256[32] = {0};
    char image_sha256_hex[65];
    if (running == NULL || esp_partition_get_sha256(running, image_sha256) != ESP_OK) {
        memset(image_sha256, 0, sizeof(image_sha256));
    }
    hex_encode(image_sha256, sizeof(image_sha256), image_sha256_hex);

    esp_netif_ip_info_t ip_info = {0};
    esp_netif_t *station = esp_netif_get_handle_from_ifkey("WIFI_STA_DEF");
    char ip_address[16] = "0.0.0.0";
    if (station != NULL && esp_netif_get_ip_info(station, &ip_info) == ESP_OK) {
        esp_ip4addr_ntoa(&ip_info.ip, ip_address, sizeof(ip_address));
    }

    csi_collector_stats_t collector;
    csi_ping_stats_t ping;
    csi_transport_stats_t transport;
    stream_sender_stats_t socket_stats;
    csi_collector_get_stats(&collector);
    csi_collector_get_ping_stats(&ping);
    csi_transport_get_stats(&transport);
    stream_sender_get_stats(&socket_stats);
    wifi_ps_type_t wifi_ps_mode = WIFI_PS_NONE;
    esp_err_t wifi_ps_result = esp_wifi_get_ps(&wifi_ps_mode);
    uint8_t current_wifi_channel = 0;
    wifi_second_chan_t secondary_wifi_channel = WIFI_SECOND_CHAN_NONE;
    esp_err_t channel_result = esp_wifi_get_channel(
        &current_wifi_channel,
        &secondary_wifi_channel
    );
    esp_reset_reason_t reset_reason = esp_reset_reason();
    uint32_t cpu_frequency_configured_mhz = 0;
#ifdef CONFIG_ESP_DEFAULT_CPU_FREQ_MHZ
    cpu_frequency_configured_mhz = CONFIG_ESP_DEFAULT_CPU_FREQ_MHZ;
#endif
    uint64_t callback_duration_avg_us = collector.callback_duration_samples == 0
        ? 0
        : collector.callback_duration_total_us / collector.callback_duration_samples;
    uint64_t queue_age_avg_us = transport.queue_age_samples == 0
        ? 0
        : transport.queue_age_total_us / transport.queue_age_samples;
    uint64_t callback_to_dequeue_avg_us = transport.queue_age_samples == 0
        ? 0
        : transport.callback_to_dequeue_total_us / transport.queue_age_samples;
    uint64_t ping_reply_interval_avg_us = ping.reply_interval_samples == 0
        ? 0
        : ping.reply_interval_total_us / ping.reply_interval_samples;
    bool callback_balance_closed = collector.callbacks_received
        == collector.invalid_inputs + collector.foreign_tx
            + collector.rate_limited + collector.admitted;
    bool admitted_balance_closed = collector.admitted
        == collector.capture_handoff_attempts;
    bool handoff_balance_closed = collector.capture_handoff_attempts
        == collector.capture_handoff_accepted + collector.capture_handoff_drops;
    bool capture_balance_closed = transport.captures_enqueued
        == transport.captures_waiting + transport.captures_in_flight
            + transport.captures_aborted + transport.serialization_failures
            + transport.frames_os_accepted + transport.frames_abandoned;

    char response[4096];
    int len = snprintf(response, sizeof(response),
        "{\"node_id\":\"RX%u\",\"boot_id\":\"%016llx\","
        "\"protocol_version\":%u,\"sequence\":%lu,\"rssi_dbm\":%d,"
        "\"ip\":\"%s\",\"version\":\"%s\",\"date\":\"%s\",\"time\":\"%s\","
        "\"image_sha256\":\"%s\",\"ota_state\":\"%s\","
        "\"raw_csi_profile\":%s,\"reset_reason_code\":%u,"
        "\"cpu_frequency_configured_mhz\":%u,"
        "\"cpu_runtime_usage\":\"not_enabled_in_this_build\","
        "\"heap\":{\"free_bytes\":%lu,\"minimum_free_bytes\":%lu},"
        "\"wifi_power_save\":{\"query_ok\":%s,\"mode\":%u},"
        "\"ping\":{\"enabled\":%s,\"running\":%s,"
        "\"start_attempts\":%lu,\"start_failures\":%lu,"
        "\"gateway_unavailable\":%lu,\"restarts\":%lu,"
        "\"replies\":%lu,\"timeouts\":%lu,"
        "\"configured_interval_ms\":%lu,\"configured_timeout_ms\":%lu,"
        "\"reply_interval_avg_us\":%llu,\"reply_interval_last_us\":%lu,"
        "\"reply_interval_max_us\":%lu,\"reply_interval_samples\":%lu},"
        "\"csi_config\":{\"channel_query_ok\":%s,\"channel\":%u,"
        "\"secondary_channel\":%u,\"rate_limit_hz\":%u,\"flags\":%lu},"
        "\"running_partition\":\"%s\",\"next_partition\":\"%s\","
        "\"max_size\":%lu,"
        "\"rx_flow\":{"
        "\"callback\":{\"received\":%lu,\"invalid_inputs\":%lu,"
        "\"foreign_tx\":%lu,\"rate_limited\":%lu,\"admitted\":%lu,"
        "\"handoff_attempts\":%lu,\"handoff_accepted\":%lu,"
        "\"handoff_drops\":%lu},"
        "\"transport\":{\"queue_depth\":%lu,\"queue_high_watermark\":%lu,"
        "\"captures_enqueued\":%lu,\"captures_waiting\":%lu,"
        "\"captures_in_flight\":%lu,\"captures_aborted\":%lu,"
        "\"pool_in_use\":%lu,\"pool_high_watermark\":%lu,"
        "\"start_failures\":%lu,\"not_ready_drops\":%lu,"
        "\"invalid_drops\":%lu,\"pool_exhausted_drops\":%lu,"
        "\"queue_publish_failures\":%lu,\"ownership_errors\":%lu,"
        "\"serialization_failures\":%lu,\"serialized_frames\":%lu,"
        "\"csi_send_calls\":%lu,\"retries\":%lu,\"errors\":%lu,"
        "\"frames_os_accepted\":%lu,\"frames_abandoned\":%lu,"
        "\"frames_expired\":%lu,\"edge_accepted\":%lu,"
        "\"edge_drops\":%lu,\"edge_skipped\":%lu,"
        "\"sync_attempted\":%lu,\"sync_sent\":%lu},"
        "\"udp_sender\":{\"send_calls\":%lu,\"invalid_calls\":%lu,"
        "\"socket_rebuilds\":%lu,\"socket_unavailable\":%lu,"
        "\"backoff_episodes\":%lu,\"backoff_suppressed\":%lu,"
        "\"sendto_attempts\":%lu,\"sendto_failures\":%lu,"
        "\"os_datagrams_accepted\":%lu,\"partial_sends\":%lu,"
        "\"csi_send_calls\":%lu,\"csi_socket_unavailable\":%lu,"
        "\"csi_backoff_suppressed\":%lu,\"csi_sendto_attempts\":%lu,"
        "\"csi_sendto_failures\":%lu,\"csi_os_datagrams_accepted\":%lu},"
        "\"balance\":{\"callbacks_closed\":%s,\"admitted_closed\":%s,"
        "\"handoff_closed\":%s,\"capture_closed\":%s},"
        "\"timing\":{\"clock\":\"device_monotonic_esp_timer_us\","
        "\"device_capture_timestamp_us\":%llu,"
        "\"device_capture_timestamp_status\":\"callback_entry_device_monotonic\","
        "\"callback_time_semantics\":\"callback_entry_device_monotonic\","
        "\"queue_age_semantics\":\"enqueue_attempt_to_dequeue\","
        "\"callback_to_dequeue_semantics\":\"callback_entry_to_dequeue\","
        "\"last_callback_started_us\":%llu,\"callback_duration_avg_us\":%llu,"
        "\"callback_duration_max_us\":%lu,\"callback_duration_samples\":%lu,"
        "\"queue_age_avg_us\":%llu,\"queue_age_last_us\":%lu,"
        "\"queue_age_max_us\":%lu,\"queue_age_samples\":%lu,"
        "\"callback_to_dequeue_avg_us\":%llu,"
        "\"callback_to_dequeue_last_us\":%lu,"
        "\"callback_to_dequeue_max_us\":%lu,"
        "\"callback_stack_free_min_bytes\":%lu,"
        "\"callback_stack_runtime_measured\":%s,"
        "\"processing_stack_configured_bytes\":%lu,"
        "\"processing_stack_free_min_bytes\":%lu,"
        "\"processing_stack_runtime_measured\":%s,"
        "\"last_sendto_attempt_us\":%lld,\"last_os_accept_us\":%lld}}},"
        "\"transport\":{\"queue_depth\":%lu,\"enqueued\":%lu,"
        "\"queue_full_drops\":%lu,\"sent\":%lu,\"retries\":%lu,"
        "\"errors\":%lu,\"abandoned\":%lu,\"socket_rebuilds\":%lu,"
        "\"socket_send_failures\":%lu,\"enomem_backoffs\":%lu}}",
        (unsigned)csi_collector_get_node_id(),
        (unsigned long long)csi_collector_get_boot_id(),
        (unsigned)CSI_PROTOCOL_VERSION,
        (unsigned long)csi_collector_get_sequence(),
        (int)csi_collector_get_rssi(),
        ip_address,
        app->version, app->date, app->time,
        image_sha256_hex, ota_state_name(ota_state),
        OTA_RAW_PROFILE_JSON,
        (unsigned)reset_reason,
        (unsigned)cpu_frequency_configured_mhz,
        (unsigned long)esp_get_free_heap_size(),
        (unsigned long)esp_get_minimum_free_heap_size(),
        wifi_ps_result == ESP_OK ? "true" : "false",
        (unsigned)wifi_ps_mode,
        ping.enabled ? "true" : "false",
        ping.running ? "true" : "false",
        (unsigned long)ping.start_attempts,
        (unsigned long)ping.start_failures,
        (unsigned long)ping.gateway_unavailable,
        (unsigned long)ping.restarts,
        (unsigned long)ping.replies,
        (unsigned long)ping.timeouts,
        (unsigned long)ping.configured_interval_ms,
        (unsigned long)ping.configured_timeout_ms,
        (unsigned long long)ping_reply_interval_avg_us,
        (unsigned long)ping.last_reply_interval_us,
        (unsigned long)ping.reply_interval_max_us,
        (unsigned long)ping.reply_interval_samples,
        channel_result == ESP_OK ? "true" : "false",
        (unsigned)current_wifi_channel,
        (unsigned)secondary_wifi_channel,
        (unsigned)CONFIG_CSI_CAPTURE_RATE_LIMIT_HZ,
        (unsigned long)csi_collector_get_csi_config_flags(),
        running ? running->label : "unknown",
        update ? update->label : "none",
        (unsigned long)(update ? update->size : 0),
        (unsigned long)collector.callbacks_received,
        (unsigned long)collector.invalid_inputs,
        (unsigned long)collector.foreign_tx,
        (unsigned long)collector.rate_limited,
        (unsigned long)collector.admitted,
        (unsigned long)collector.capture_handoff_attempts,
        (unsigned long)collector.capture_handoff_accepted,
        (unsigned long)collector.capture_handoff_drops,
        (unsigned long)transport.queue_depth,
        (unsigned long)transport.queue_high_watermark,
        (unsigned long)transport.captures_enqueued,
        (unsigned long)transport.captures_waiting,
        (unsigned long)transport.captures_in_flight,
        (unsigned long)transport.captures_aborted,
        (unsigned long)transport.pool_in_use,
        (unsigned long)transport.pool_high_watermark,
        (unsigned long)transport.start_failures,
        (unsigned long)transport.not_ready_drops,
        (unsigned long)transport.invalid_drops,
        (unsigned long)transport.pool_exhausted_drops,
        (unsigned long)transport.queue_publish_failures,
        (unsigned long)transport.ownership_errors,
        (unsigned long)transport.serialization_failures,
        (unsigned long)transport.serialized_frames,
        (unsigned long)transport.csi_send_calls,
        (unsigned long)transport.retries,
        (unsigned long)transport.transport_errors,
        (unsigned long)transport.frames_os_accepted,
        (unsigned long)transport.frames_abandoned,
        (unsigned long)transport.frames_expired,
        (unsigned long)transport.edge_enqueue_accepted,
        (unsigned long)transport.edge_enqueue_drops,
        (unsigned long)transport.edge_enqueue_skipped,
        (unsigned long)transport.sync_packets_attempted,
        (unsigned long)transport.sync_packets_sent,
        (unsigned long)socket_stats.send_calls,
        (unsigned long)socket_stats.invalid_calls,
        (unsigned long)socket_stats.socket_rebuilds,
        (unsigned long)socket_stats.socket_unavailable,
        (unsigned long)socket_stats.enomem_backoffs,
        (unsigned long)socket_stats.backoff_suppressed,
        (unsigned long)socket_stats.sendto_attempts,
        (unsigned long)socket_stats.sendto_failures,
        (unsigned long)socket_stats.os_datagrams_accepted,
        (unsigned long)socket_stats.partial_sends,
        (unsigned long)socket_stats.csi_send_calls,
        (unsigned long)socket_stats.csi_socket_unavailable,
        (unsigned long)socket_stats.csi_backoff_suppressed,
        (unsigned long)socket_stats.csi_sendto_attempts,
        (unsigned long)socket_stats.csi_sendto_failures,
        (unsigned long)socket_stats.csi_os_datagrams_accepted,
        callback_balance_closed ? "true" : "false",
        admitted_balance_closed ? "true" : "false",
        handoff_balance_closed ? "true" : "false",
        capture_balance_closed ? "true" : "false",
        (unsigned long long)collector.last_callback_started_us,
        (unsigned long long)collector.last_callback_started_us,
        (unsigned long long)callback_duration_avg_us,
        (unsigned long)collector.callback_duration_max_us,
        (unsigned long)collector.callback_duration_samples,
        (unsigned long long)queue_age_avg_us,
        (unsigned long)transport.queue_age_last_us,
        (unsigned long)transport.queue_age_max_us,
        (unsigned long)transport.queue_age_samples,
        (unsigned long long)callback_to_dequeue_avg_us,
        (unsigned long)transport.callback_to_dequeue_last_us,
        (unsigned long)transport.callback_to_dequeue_max_us,
        (unsigned long)collector.callback_stack_free_min_bytes,
        collector.callback_stack_runtime_measured ? "true" : "false",
        (unsigned long)transport.processing_stack_configured_bytes,
        (unsigned long)transport.processing_stack_free_min_bytes,
        transport.processing_stack_runtime_measured ? "true" : "false",
        (long long)socket_stats.last_sendto_attempt_us,
        (long long)socket_stats.last_os_accept_us,
        (unsigned long)transport.queue_depth,
        (unsigned long)transport.captures_enqueued,
        (unsigned long)transport.pool_exhausted_drops,
        (unsigned long)transport.frames_os_accepted,
        (unsigned long)transport.retries,
        (unsigned long)transport.transport_errors,
        (unsigned long)transport.frames_abandoned,
        (unsigned long)socket_stats.socket_rebuilds,
        (unsigned long)socket_stats.send_failures,
        (unsigned long)socket_stats.enomem_backoffs);

    if (len < 0 || len >= (int)sizeof(response)) {
        httpd_resp_send_err(req, HTTPD_500_INTERNAL_SERVER_ERROR,
                            "Diagnostics response overflow");
        return ESP_FAIL;
    }

    httpd_resp_set_type(req, "application/json");
    httpd_resp_send(req, response, len);
    return ESP_OK;
}

/**
 * POST /ota — receive and flash firmware binary.
 */
static esp_err_t ota_upload_handler(httpd_req_t *req)
{
    /* ADR-050: Authenticate before accepting firmware upload. */
    if (!ota_check_auth(req)) {
        ESP_LOGW(TAG, "OTA upload rejected: authentication failed");
        httpd_resp_send_err(req, HTTPD_403_FORBIDDEN,
                            "Authentication required. Use: Authorization: Bearer <psk>");
        return ESP_FAIL;
    }

    ESP_LOGI(TAG, "OTA update started, content_length=%d", req->content_len);

    const esp_partition_t *update_partition = esp_ota_get_next_update_partition(NULL);
    if (update_partition == NULL) {
        httpd_resp_send_err(req, HTTPD_500_INTERNAL_SERVER_ERROR,
                            "No OTA partition available");
        return ESP_FAIL;
    }

    if (req->content_len <= 0 || (size_t)req->content_len > update_partition->size) {
        ESP_LOGW(TAG, "OTA rejected: content_length=%d does not fit partition '%s' (%lu)",
                 req->content_len, update_partition->label,
                 (unsigned long)update_partition->size);
        httpd_resp_send_err(req, HTTPD_400_BAD_REQUEST,
                            "Invalid firmware size for OTA partition");
        return ESP_FAIL;
    }

    esp_ota_handle_t ota_handle;
    esp_err_t err = esp_ota_begin(update_partition, OTA_WITH_SEQUENTIAL_WRITES, &ota_handle);
    if (err != ESP_OK) {
        ESP_LOGE(TAG, "esp_ota_begin failed: %s", esp_err_to_name(err));
        httpd_resp_send_err(req, HTTPD_500_INTERNAL_SERVER_ERROR,
                            "OTA begin failed");
        return ESP_FAIL;
    }

    /* Read firmware in chunks. */
    char buf[1024];
    int received = 0;
    int total = 0;

    while (total < req->content_len) {
        received = httpd_req_recv(req, buf, sizeof(buf));
        if (received <= 0) {
            if (received == HTTPD_SOCK_ERR_TIMEOUT) {
                continue;  /* Retry on timeout. */
            }
            ESP_LOGE(TAG, "OTA receive error at byte %d", total);
            esp_ota_abort(ota_handle);
            httpd_resp_send_err(req, HTTPD_500_INTERNAL_SERVER_ERROR,
                                "Receive error");
            return ESP_FAIL;
        }

        err = esp_ota_write(ota_handle, buf, received);
        if (err != ESP_OK) {
            ESP_LOGE(TAG, "esp_ota_write failed at byte %d: %s",
                     total, esp_err_to_name(err));
            esp_ota_abort(ota_handle);
            httpd_resp_send_err(req, HTTPD_500_INTERNAL_SERVER_ERROR,
                                "OTA write failed");
            return ESP_FAIL;
        }

        total += received;
        if ((total % (64 * 1024)) == 0) {
            ESP_LOGI(TAG, "OTA progress: %d / %d bytes (%.0f%%)",
                     total, req->content_len,
                     (float)total * 100.0f / (float)req->content_len);
        }
    }

    err = esp_ota_end(ota_handle);
    if (err != ESP_OK) {
        ESP_LOGE(TAG, "esp_ota_end failed: %s", esp_err_to_name(err));
        httpd_resp_send_err(req, HTTPD_500_INTERNAL_SERVER_ERROR,
                            "OTA validation failed");
        return ESP_FAIL;
    }

    err = esp_ota_set_boot_partition(update_partition);
    if (err != ESP_OK) {
        ESP_LOGE(TAG, "esp_ota_set_boot_partition failed: %s", esp_err_to_name(err));
        httpd_resp_send_err(req, HTTPD_500_INTERNAL_SERVER_ERROR,
                            "Set boot partition failed");
        return ESP_FAIL;
    }

    ESP_LOGI(TAG, "OTA update successful! Rebooting to partition '%s'...",
             update_partition->label);

    const char *resp = "{\"status\":\"ok\",\"message\":\"OTA update successful. Rebooting...\"}";
    httpd_resp_set_type(req, "application/json");
    httpd_resp_send(req, resp, strlen(resp));

    /* Delay briefly to let the response flush, then reboot. */
    vTaskDelay(pdMS_TO_TICKS(1000));
    esp_restart();

    return ESP_OK;  /* Never reached. */
}

/** Internal: start the HTTP server and register OTA endpoints. */
static esp_err_t ota_start_server(httpd_handle_t *out_handle)
{
    httpd_config_t config = HTTPD_DEFAULT_CONFIG();
    config.stack_size = OTA_HTTP_TASK_STACK_BYTES;
    config.server_port = OTA_PORT;
    config.max_uri_handlers = 12;  /* Extra slots for WASM endpoints (ADR-040). */
    /* Increase receive timeout for large uploads. */
    config.recv_wait_timeout = 30;

    httpd_handle_t server = NULL;
    esp_err_t err = httpd_start(&server, &config);
    if (err != ESP_OK) {
        ESP_LOGE(TAG, "Failed to start OTA HTTP server on port %d: %s",
                 OTA_PORT, esp_err_to_name(err));
        if (out_handle) *out_handle = NULL;
        return err;
    }

    httpd_uri_t status_uri = {
        .uri      = "/ota/status",
        .method   = HTTP_GET,
        .handler  = ota_status_handler,
        .user_ctx = NULL,
    };
    httpd_register_uri_handler(server, &status_uri);

    httpd_uri_t upload_uri = {
        .uri      = "/ota",
        .method   = HTTP_POST,
        .handler  = ota_upload_handler,
        .user_ctx = NULL,
    };
    httpd_register_uri_handler(server, &upload_uri);

    ESP_LOGI(TAG, "OTA HTTP server started on port %d", OTA_PORT);
    ESP_LOGI(TAG, "  GET  /ota/status — firmware version info");
    ESP_LOGI(TAG, "  POST /ota        — upload new firmware binary");

    if (out_handle) *out_handle = server;
    return ESP_OK;
}

/**
 * Load the OTA PSK from NVS into the module-local s_ota_psk cache and log
 * the resulting posture. Called by both ota_update_init() and
 * ota_update_init_ex() so the per-boot diagnostic prints no matter which
 * entry point main.c uses — historically only ota_update_init() loaded the
 * PSK, which left ota_update_init_ex() with an empty s_ota_psk and an
 * invisible fail-closed posture (RuView#596 follow-up).
 */
static void ota_load_psk_from_nvs(void)
{
    nvs_handle_t nvs;
    if (nvs_open(OTA_NVS_NAMESPACE, NVS_READONLY, &nvs) == ESP_OK) {
        size_t len = sizeof(s_ota_psk);
        if (nvs_get_str(nvs, OTA_NVS_KEY, s_ota_psk, &len) == ESP_OK) {
            ESP_LOGI(TAG, "OTA PSK loaded from NVS (%d chars) — authentication enabled", (int)len - 1);
        } else {
            ESP_LOGW(TAG, "No OTA PSK in NVS — OTA upload endpoint will REJECT all requests until "
                          "provisioned (provision.py --ota-psk <hex>). Fail-closed per RuView#596.");
        }
        nvs_close(nvs);
    } else {
        ESP_LOGW(TAG, "NVS namespace '%s' not found — OTA upload endpoint will REJECT all "
                      "requests until provisioned. Fail-closed per RuView#596.", OTA_NVS_NAMESPACE);
    }
}

esp_err_t ota_update_init(void)
{
    /* ADR-050: Load OTA PSK from NVS if provisioned. */
    ota_load_psk_from_nvs();
    return ota_start_server(NULL);
}

esp_err_t ota_update_init_ex(void **out_server)
{
    /* ADR-050: Load OTA PSK from NVS if provisioned. main.c uses this
     * variant (not ota_update_init), so without this call s_ota_psk
     * stayed empty forever and the fail-closed posture was invisible
     * in serial logs. */
    ota_load_psk_from_nvs();
    return ota_start_server((httpd_handle_t *)out_server);
}

esp_err_t ota_update_complete_boot_self_test(bool essential_runtime_ready)
{
    const esp_partition_t *running = esp_ota_get_running_partition();
    if (running == NULL) {
        return ESP_ERR_NOT_FOUND;
    }

    esp_ota_img_states_t state = ESP_OTA_IMG_UNDEFINED;
    esp_err_t state_result = esp_ota_get_state_partition(running, &state);
    if (state_result != ESP_OK || state != ESP_OTA_IMG_PENDING_VERIFY) {
        return state_result == ESP_OK ? ESP_OK : state_result;
    }

    bool identity_valid = csi_collector_get_node_id() >= 1
        && csi_collector_get_node_id() <= 4
        && csi_collector_get_boot_id() != 0;
    bool heap_valid = esp_get_free_heap_size() >= 32768;
    if (!essential_runtime_ready || !identity_valid || !heap_valid) {
        ESP_LOGE(TAG, "OTA self-test failed (runtime=%d identity=%d heap=%d); rolling back",
                 essential_runtime_ready, identity_valid, heap_valid);
        return esp_ota_mark_app_invalid_rollback_and_reboot();
    }

    ESP_LOGI(TAG, "OTA self-test passed; marking running image VALID");
    return esp_ota_mark_app_valid_cancel_rollback();
}

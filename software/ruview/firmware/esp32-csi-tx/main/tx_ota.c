#include "tx_ota.h"
#include "tx_safety.h"
#include "tx_diag.h"

#include <stdio.h>
#include <stdarg.h>
#include <string.h>

#include "esp_app_desc.h"
#include "esp_http_server.h"
#include "esp_heap_caps.h"
#include "esp_log.h"
#include "esp_ota_ops.h"
#include "esp_partition.h"
#include "esp_system.h"
#include "esp_timer.h"
#include "esp_wifi.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"

#define OTA_PORT 8032
#define HTTP_TASK_STACK_BYTES 8192U
#define OTA_PSK_MAX 65

static const char *TAG = "tx_ota";
static char s_psk[OTA_PSK_MAX];
static uint64_t s_boot_id;
static uint8_t s_channel;
static uint16_t s_rate_hz;
static char s_factory_mac_sha256[65];
static tx_status_provider_t s_provider;
static const esp_partition_t *s_verified_running;
static uint8_t s_image_sha[32];
static int64_t s_image_capture_begin_us, s_image_capture_end_us;

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
    static const char hex[] = "0123456789abcdef";
    for (size_t index = 0; index < len; ++index) {
        output[index * 2] = hex[bytes[index] >> 4];
        output[index * 2 + 1] = hex[bytes[index] & 0x0f];
    }
    output[len * 2] = '\0';
}

bool tx_ota_token_matches(const char *secret, const char *header)
{
    if (strlen(secret) != 64 || strlen(header) != 71 || memcmp(header, "Bearer ", 7) != 0)
        return false;
    volatile uint8_t difference = 0;
    for (size_t i = 0; i < 64; ++i) difference |= (uint8_t)(secret[i] ^ header[i + 7]);
    return difference == 0;
}

static bool authorized(httpd_req_t *request)
{
    char header[96] = {0};
    return httpd_req_get_hdr_value_str(request, "Authorization", header, sizeof(header)) == ESP_OK
        && tx_ota_token_matches(s_psk, header);
}

static bool append_json(char *buffer, size_t capacity, size_t *used, const char *format, ...)
{
    va_list args;
    va_start(args, format);
    int length = vsnprintf(buffer + *used, capacity - *used, format, args);
    va_end(args);
    if (length < 0 || (size_t)length >= capacity - *used) return false;
    *used += (size_t)length;
    return true;
}

static esp_err_t send_diagnostics(httpd_req_t *request, const tx_runtime_status_t *status,
                                  char *buffer, size_t capacity)
{
    const tx_send_error_t *error = &status->last_send_error;
    size_t used = 0;
    if (!append_json(buffer, capacity, &used,
        ",\"send_diagnostics\":{\"scope\":\"local_syscall_only\",\"socket_errors\":%lu,"
        "\"broadcast_errors\":%lu,\"sendto_errors\":%lu,\"short_sends\":%lu,\"last_error\":",
        (unsigned long)status->socket_errors, (unsigned long)status->broadcast_errors,
        (unsigned long)status->sendto_errors, (unsigned long)status->short_sends)) return ESP_FAIL;
    if (error->operation == TX_SEND_OP_NONE) {
        if (!append_json(buffer, capacity, &used, "null}")) return ESP_FAIL;
    } else {
        const char *operation = error->operation == TX_SEND_OP_SOCKET ? "socket"
            : error->operation == TX_SEND_OP_BROADCAST ? "setsockopt_SO_BROADCAST" : "sendto";
        if (!append_json(buffer, capacity, &used,
            "{\"operation\":\"%s\",\"return_value\":%d,\"sequence\":%lu,\"boot_us\":%llu,\"errno\":",
            operation, error->result, (unsigned long)error->sequence,
            (unsigned long long)error->occurred_us)) return ESP_FAIL;
        if (error->errno_valid) {
            if (!append_json(buffer, capacity, &used, "%d}}", error->error_number)) return ESP_FAIL;
        } else if (!append_json(buffer, capacity, &used, "null}}")) return ESP_FAIL;
    }
    /* Sequential status-time observations, not failure-time heap evidence.
       IDF's StackType_t is uint8_t, so the high-watermark unit is bytes. */
    const int64_t resources_begin_us = esp_timer_get_time();
    const uint32_t caps = MALLOC_CAP_INTERNAL | MALLOC_CAP_8BIT;
    const size_t internal_free = heap_caps_get_free_size(caps);
    const size_t internal_largest = heap_caps_get_largest_free_block(caps);
    const size_t internal_minimum = heap_caps_get_minimum_free_size(caps);
    const UBaseType_t http_stack_low = uxTaskGetStackHighWaterMark(NULL);
    const int64_t resources_end_us = esp_timer_get_time();
    if (!append_json(buffer, capacity, &used,
        ",\"status_resources\":{\"scope\":\"non_atomic_http_task_sample\","
        "\"capture_begin_us\":%lld,\"capture_end_us\":%lld,"
        "\"http_task_stack_allocated_bytes\":%lu,\"http_task_stack_low_watermark_bytes\":%lu,"
        "\"heap_internal_8bit_free_bytes\":%lu,\"heap_internal_8bit_largest_block_bytes\":%lu,"
        "\"heap_internal_8bit_minimum_free_bytes\":%lu}",
        (long long)resources_begin_us, (long long)resources_end_us,
        (unsigned long)HTTP_TASK_STACK_BYTES, (unsigned long)http_stack_low,
        (unsigned long)internal_free, (unsigned long)internal_largest,
        (unsigned long)internal_minimum)) return ESP_FAIL;
    return httpd_resp_send_chunk(request, buffer, used);
}

static esp_err_t send_path_diagnostics(httpd_req_t *request, const tx_runtime_status_t *status,
                                       char *buffer, size_t capacity)
{
    const tx_send_path_status_t *path = &status->send_path;
    const tx_send_path_error_t *error = &path->last_enomem;
    size_t used = 0;
    if (!append_json(buffer, capacity, &used,
        ",\"send_path_diagnostics\":{\"scope\":\"own_sounding_local_handoff_only\","
        "\"own_attempts\":%lu,\"own_netbuf_calls\":%lu,\"own_wifi_handoffs\":%lu,"
        "\"enomem_netbuf_allocation\":%lu,\"enomem_wifi_handoff\":%lu,"
        "\"enomem_other_or_unclassified\":%lu,\"last_enomem\":",
        (unsigned long)path->own_attempts, (unsigned long)path->netbuf_calls,
        (unsigned long)path->wifi_handoffs, (unsigned long)path->enomem_netbuf_allocation,
        (unsigned long)path->enomem_wifi_handoff,
        (unsigned long)path->enomem_other_or_unclassified)) return ESP_FAIL;
    if (error->stage == TX_SEND_PATH_NONE) {
        if (!append_json(buffer, capacity, &used, "null}")) return ESP_FAIL;
    } else {
        const char *stage = error->stage == TX_SEND_PATH_NETBUF_ALLOCATION ? "netbuf_allocation"
            : error->stage == TX_SEND_PATH_WIFI_HANDOFF ? "wifi_handoff" : "other_or_unclassified";
        if (!append_json(buffer, capacity, &used,
            "{\"stage\":\"%s\",\"sequence\":%lu,\"capture_begin_us\":%llu,\"capture_end_us\":%llu,"
            "\"netbuf_calls\":%lu,\"netbuf_returned_nonnull\":%s,\"matched_wifi_calls\":%lu,\"wifi_return\":",
            stage, (unsigned long)error->sequence,
            (unsigned long long)error->capture_begin_us, (unsigned long long)error->capture_end_us,
            (unsigned long)error->netbuf_calls,
            error->netbuf_calls == 0 ? "null" : error->netbuf_returned_nonnull ? "true" : "false",
            (unsigned long)error->wifi_calls)) return ESP_FAIL;
        if (error->wifi_calls == 0) {
            if (!append_json(buffer, capacity, &used, "null}}")) return ESP_FAIL;
        } else if (!append_json(buffer, capacity, &used, "%d}}", error->wifi_return)) return ESP_FAIL;
    }
    return httpd_resp_send_chunk(request, buffer, used);
}

static esp_err_t send_association(httpd_req_t *request, char *buffer, size_t capacity, bool request_authorized)
{
    if (!request_authorized) {
        const char *unavailable = ",\"ap_association\":{\"schema\":1,\"status\":\"authorization_required\","
            "\"station_list_rc\":null,\"ap_mac_rc\":null,\"capture_begin_us\":null,\"capture_end_us\":null,"
            "\"generation_before\":null,\"generation_after\":null,\"ap_mac\":null,\"station_count\":null,\"stations\":null}}";
        return httpd_resp_send_chunk(request, unavailable, strlen(unavailable));
    }
    wifi_sta_list_t stations = {0};
    uint8_t ap_mac[6] = {0};
    tx_runtime_status_t before = {0}, after = {0};
    const int64_t begin_us = esp_timer_get_time();
    if (s_provider != NULL) s_provider(&before);
    esp_err_t mac_rc = esp_wifi_get_mac(WIFI_IF_AP, ap_mac);
    esp_err_t list_rc = esp_wifi_ap_get_sta_list(&stations);
    if (s_provider != NULL) s_provider(&after);
    const int64_t end_us = esp_timer_get_time();
    bool list_valid = list_rc == ESP_OK && stations.num >= 0
        && (size_t)stations.num <= sizeof(stations.sta) / sizeof(stations.sta[0]);
    const char *state = !list_valid || mac_rc != ESP_OK || s_provider == NULL || end_us < begin_us
        ? "unavailable" : before.station_generation != after.station_generation ? "changed" : "current";
    char mac_hex[13];
    size_t used = 0;
    if (!append_json(buffer, capacity, &used,
        ",\"ap_association\":{\"schema\":1,\"status\":\"%s\",\"station_list_rc\":%d,\"ap_mac_rc\":%d,"
        "\"capture_begin_us\":%lld,\"capture_end_us\":%lld,\"generation_before\":%llu,\"generation_after\":%llu,\"ap_mac\":",
        state, list_rc, mac_rc, (long long)begin_us, (long long)end_us,
        (unsigned long long)before.station_generation, (unsigned long long)after.station_generation)) return ESP_FAIL;
    if (mac_rc == ESP_OK) {
        hex_encode(ap_mac, sizeof(ap_mac), mac_hex);
        if (!append_json(buffer, capacity, &used, "\"%s\"", mac_hex)) return ESP_FAIL;
    } else if (!append_json(buffer, capacity, &used, "null")) return ESP_FAIL;
    if (!list_valid) {
        if (!append_json(buffer, capacity, &used, ",\"station_count\":null,\"stations\":null}}")) return ESP_FAIL;
    } else {
        if (!append_json(buffer, capacity, &used, ",\"station_count\":%d,\"stations\":[", stations.num)) return ESP_FAIL;
        for (int index = 0; index < stations.num; ++index) {
            hex_encode(stations.sta[index].mac, sizeof(stations.sta[index].mac), mac_hex);
            if (!append_json(buffer, capacity, &used, "%s{\"mac\":\"%s\"}", index ? "," : "", mac_hex)) return ESP_FAIL;
        }
        if (!append_json(buffer, capacity, &used, "]}}")) return ESP_FAIL;
    }
    return httpd_resp_send_chunk(request, buffer, used);
}

static esp_err_t status_handler(httpd_req_t *request)
{
    tx_runtime_status_t status = {0};
    if (s_provider != NULL) {
        s_provider(&status);
    }
    const esp_app_desc_t *app = esp_app_get_description();
    const esp_partition_t *running = esp_ota_get_running_partition();
    const esp_partition_t *next = esp_ota_get_next_update_partition(NULL);
    esp_ota_img_states_t ota_state = ESP_OTA_IMG_UNDEFINED;
    const int64_t sampled_us = esp_timer_get_time();
    esp_err_t state_rc = running == NULL ? ESP_ERR_INVALID_STATE : esp_ota_get_state_partition(running, &ota_state);
    esp_err_t image_rc = running == NULL || running != s_verified_running ? ESP_ERR_INVALID_STATE : ESP_OK;
    /* A successful UNDEFINED state is meaningful. A failed getter is not that
       state, nor is a zero-filled buffer a measured runtime image digest. */
    if (state_rc != ESP_OK || image_rc != ESP_OK) {
        if (state_rc != ESP_OK) {
            ESP_LOGE("tx_boot", "TX_COMPARE op=esp_ota_get_state_partition rc=0x%x field=ota_state expected=available actual=unavailable boot_us=%lld",
                     (unsigned)state_rc, (long long)sampled_us);
            TX_ERROR("STATUS_OTA_STATE", state_rc);
        }
        if (image_rc != ESP_OK) {
            ESP_LOGE("tx_boot", "TX_COMPARE op=verified_running_partition rc=0x%x field=runtime_image_digest expected=verified_partition actual=unavailable boot_us=%lld",
                     (unsigned)image_rc, (long long)sampled_us);
            TX_ERROR("STATUS_IMAGE_DIGEST", image_rc);
        }
        httpd_resp_send_err(request, HTTPD_500_INTERNAL_SERVER_ERROR, "TX identity unavailable");
        return state_rc != ESP_OK ? state_rc : image_rc;
    }
    char image_sha_hex[65];
    hex_encode(s_image_sha, sizeof(s_image_sha), image_sha_hex);
    uint32_t jitter_average = status.jitter_samples == 0
                                  ? 0
                                  : (uint32_t)(status.jitter_sum_us / status.jitter_samples);

    char response[1400];
    int length = snprintf(
        response,
        sizeof(response),
        "{\"node_id\":\"TX1\",\"boot_id\":\"%016llx\",\"protocol_version\":2,"
        "\"boot_check\":%d,\"sequence\":%lu,\"channel\":%u,\"rate_hz\":%u,\"stations\":%lu,"
        "\"version\":\"%s\",\"git_hash\":\"%s\",\"source_sha256\":\"%s\",\"image_sha256\":\"%s\","
        "\"source_mac_sha256\":\"%s\",\"ota_state\":\"%s\","
        "\"running_partition\":\"%s\",\"next_partition\":\"%s\","
        "\"max_ota_size\":%lu,\"free_heap\":%lu,"
        "\"sent_packets\":%lu,\"send_errors\":%lu,\"socket_rebuilds\":%lu,"
        "\"wifi_restarts\":%lu,\"missed_deadlines\":%lu,"
        "\"jitter_max_us\":%lu,\"jitter_average_us\":%lu,"
        "\"image_sha256_scope\":\"verified_once_this_boot\","
        "\"image_sha256_capture_begin_us\":%lld,\"image_sha256_capture_end_us\":%lld",
        (unsigned long long)s_boot_id,
        status.boot_check,
        (unsigned long)status.sequence,
        (unsigned)s_channel,
        (unsigned)s_rate_hz,
        (unsigned long)status.station_count,
        app->version,
        TX_GIT_HASH,
        TX_SOURCE_SHA,
        image_sha_hex,
        s_factory_mac_sha256,
        ota_state_name(ota_state),
        running != NULL ? running->label : "unknown",
        next != NULL ? next->label : "none",
        (unsigned long)(next != NULL ? next->size : 0),
        (unsigned long)esp_get_free_heap_size(),
        (unsigned long)status.sent_packets,
        (unsigned long)status.send_errors,
        (unsigned long)status.socket_rebuilds,
        (unsigned long)status.wifi_restarts,
        (unsigned long)status.missed_deadlines,
        (unsigned long)status.jitter_max_us,
        (unsigned long)jitter_average,
        (long long)s_image_capture_begin_us, (long long)s_image_capture_end_us);
    if (length < 0 || length >= (int)sizeof(response)) {
        httpd_resp_send_err(request, HTTPD_500_INTERNAL_SERVER_ERROR, "status overflow");
        return ESP_FAIL;
    }
    /* ESP-IDF discards request headers when the first response chunk is sent. */
    const bool request_authorized = authorized(request);
    httpd_resp_set_type(request, "application/json");
    esp_err_t result = httpd_resp_send_chunk(request, response, length);
    if (result == ESP_OK) result = send_diagnostics(request, &status, response, sizeof(response));
    if (result == ESP_OK) result = send_path_diagnostics(request, &status, response, sizeof(response));
    if (result == ESP_OK) result = send_association(request, response, sizeof(response), request_authorized);
    if (result == ESP_OK) result = httpd_resp_send_chunk(request, NULL, 0);
    return result;
}

static esp_err_t ota_handler(httpd_req_t *request)
{
    if (!authorized(request)) {
        httpd_resp_send_err(request, HTTPD_403_FORBIDDEN, "OTA authorization required");
        return ESP_FAIL;
    }
    tx_runtime_status_t runtime = {0};
    if (s_provider != NULL) s_provider(&runtime);
    if (runtime.boot_check != 1) {
        httpd_resp_send_err(request, HTTPD_400_BAD_REQUEST, "local boot check incomplete");
        return ESP_FAIL;
    }
    const esp_partition_t *partition = esp_ota_get_next_update_partition(NULL);
    if (partition == NULL || partition == esp_ota_get_running_partition() || request->content_len <= 0
        || (size_t)request->content_len > partition->size) {
        httpd_resp_send_err(request, HTTPD_400_BAD_REQUEST, "invalid OTA image size");
        return ESP_FAIL;
    }
#ifdef CONFIG_TX_PROTECT_ORIGINAL_APP
    if (partition->subtype == ESP_PARTITION_SUBTYPE_APP_OTA_0) {
        httpd_resp_send_err(request, HTTPD_403_FORBIDDEN, "original app0 is protected");
        return ESP_FAIL;
    }
#endif
    esp_ota_handle_t handle;
    esp_err_t error = esp_ota_begin(partition, request->content_len, &handle);
    if (error != ESP_OK) {
        TX_ERROR("OTA_BEGIN", error);
        httpd_resp_send_err(request, HTTPD_500_INTERNAL_SERVER_ERROR, "OTA begin failed");
        return error;
    }
    char buffer[4096];
    int remaining = request->content_len;
    const int64_t deadline = esp_timer_get_time() + 120000000;
    while (remaining > 0) {
        if (esp_timer_get_time() >= deadline) {
            TX_ERROR("OTA_RECEIVE_TIMEOUT", ESP_ERR_TIMEOUT);
            (void)esp_ota_abort(handle);
            httpd_resp_send_err(request, HTTPD_408_REQ_TIMEOUT, "OTA time budget exceeded");
            return ESP_FAIL;
        }
        int received = httpd_req_recv(request, buffer,
                                      remaining < (int)sizeof(buffer) ? remaining : (int)sizeof(buffer));
        if (received <= 0 || received > remaining || esp_ota_write(handle, buffer, received) != ESP_OK) {
            TX_ERROR("OTA_RECEIVE_OR_WRITE", ESP_FAIL);
            (void)esp_ota_abort(handle);
            httpd_resp_send_err(request, HTTPD_500_INTERNAL_SERVER_ERROR, "OTA write failed");
            return ESP_FAIL;
        }
        remaining -= received;
    }
    error = esp_ota_end(handle);
    if (error != ESP_OK || esp_ota_set_boot_partition(partition) != ESP_OK) {
        TX_ERROR("OTA_VALIDATE_OR_SELECT", error != ESP_OK ? error : ESP_FAIL);
        httpd_resp_send_err(request, HTTPD_500_INTERNAL_SERVER_ERROR, "OTA validation failed");
        return ESP_FAIL;
    }
    httpd_resp_sendstr(request, "Update accepted; restarting (boot state depends on verified bootloader)");
    ESP_LOGI(TAG, "OTA image accepted in %s", partition->label);
    TX_CHECKPOINT("OTA_RESTART_ACCEPTED");
    vTaskDelay(pdMS_TO_TICKS(250));
    esp_restart();
    return ESP_OK;
}

esp_err_t tx_ota_start(uint64_t boot_id, const tx_config_t *tx_config,
                       const char *source_mac_sha256, tx_status_provider_t provider)
{
    s_boot_id = boot_id;
    s_channel = tx_config->channel;
    s_rate_hz = tx_config->rate_hz;
    s_provider = provider;
    s_verified_running = NULL;
    const esp_partition_t *running = esp_ota_get_running_partition();
    s_image_capture_begin_us = esp_timer_get_time();
    /* IDF 5.4 hashes the complete running image on every getter call, even
       with an appended hash. Verify once before sender start; OTA only writes
       the next partition. A reboot/start must verify again, never cache errors. */
    esp_err_t image_rc = running == NULL ? ESP_ERR_INVALID_STATE : esp_partition_get_sha256(running, s_image_sha);
    s_image_capture_end_us = esp_timer_get_time();
    if (image_rc != ESP_OK) { TX_ERROR("STATUS_IMAGE_DIGEST", image_rc); return image_rc; }
    s_verified_running = running;
    strlcpy(s_factory_mac_sha256, source_mac_sha256, sizeof(s_factory_mac_sha256));
    strlcpy(s_psk, tx_config->ota_psk, sizeof(s_psk));
    httpd_config_t config = HTTPD_DEFAULT_CONFIG();
    /* Includes Xtensa coprocessor storage and HTTP/printf call depth. */
    config.stack_size = HTTP_TASK_STACK_BYTES;
    config.server_port = OTA_PORT;
    config.recv_wait_timeout = 2;
    config.send_wait_timeout = 2;
    httpd_handle_t server = NULL;
    TX_STEP(httpd_start(&server, &config));
    const httpd_uri_t status_uri = {
        .uri = "/status", .method = HTTP_GET, .handler = status_handler, .user_ctx = NULL,
    };
    const httpd_uri_t ota_uri = {
        .uri = "/ota", .method = HTTP_POST, .handler = ota_handler, .user_ctx = NULL,
    };
    TX_CHECKPOINT("BEGIN STATUS_REGISTER");
    esp_err_t error = httpd_register_uri_handler(server, &status_uri);
    if (error != ESP_OK) TX_ERROR("STATUS_REGISTER", error);
    if (error == ESP_OK) {
        TX_CHECKPOINT("BEGIN OTA_REGISTER");
        error = httpd_register_uri_handler(server, &ota_uri);
        if (error != ESP_OK) TX_ERROR("OTA_REGISTER", error);
    }
    if (error != ESP_OK) (void)httpd_stop(server);
    return error;
}

esp_err_t tx_ota_complete_boot_self_test(bool runtime_healthy)
{
#ifdef CONFIG_TX_VERIFIED_ROLLBACK
    const esp_partition_t *running = esp_ota_get_running_partition();
    esp_ota_img_states_t state = ESP_OTA_IMG_UNDEFINED;
    if (running == NULL) {
        ESP_LOGE("tx_boot", "TX_COMPARE op=esp_ota_get_running_partition rc=0x%x field=running_partition expected=present actual=unavailable boot_us=%lld",
                 (unsigned)ESP_ERR_INVALID_STATE, (long long)esp_timer_get_time());
        TX_ERROR("BOOT_RUNNING_PARTITION", ESP_ERR_INVALID_STATE);
        return ESP_ERR_INVALID_STATE;
    }
    esp_err_t error = esp_ota_get_state_partition(running, &state);
    ESP_LOGI("tx_boot", "TX_BOOT_ACCEPT op=esp_ota_get_state_partition rc=0x%x state=%d unavailable=-1 boot_us=%lld",
             (unsigned)error, error == ESP_OK ? (int)state : -1, (long long)esp_timer_get_time());
    if (error != ESP_OK) { TX_ERROR("esp_ota_get_state_partition", error); return error; }
    if (state != ESP_OTA_IMG_PENDING_VERIFY) return runtime_healthy ? ESP_OK : ESP_FAIL;
    uint32_t heap = esp_get_free_heap_size();
    if (!runtime_healthy || heap < 32768) {
        ESP_LOGE("tx_boot", "TX_COMPARE op=tx_ota_complete_boot_self_test rc=0x%x field=%s expected=%s actual=%lu boot_us=%lld",
                 (unsigned)ESP_ERR_INVALID_STATE, runtime_healthy ? "free_heap_bytes" : "runtime_healthy",
                 runtime_healthy ? ">=32768" : "1", (unsigned long)(runtime_healthy ? heap : 0), (long long)esp_timer_get_time());
        if (esp_ota_check_rollback_is_possible()) {
            TX_ERROR("BOOT_ROLLBACK_RESTART", ESP_ERR_INVALID_STATE);
            return esp_ota_mark_app_invalid_rollback_and_reboot();
        }
        ESP_LOGE("tx_boot", "TX_COMPARE op=esp_ota_check_rollback_is_possible rc=0x%x field=verified_rollback_possible expected=1 actual=0 boot_us=%lld",
                 (unsigned)ESP_ERR_INVALID_STATE, (long long)esp_timer_get_time());
        return ESP_ERR_INVALID_STATE;
    }
    error = esp_ota_mark_app_valid_cancel_rollback();
    if (error != ESP_OK) TX_ERROR("esp_ota_mark_app_valid_cancel_rollback", error);
    return error;
#else
    /* Old bootloader support is unproved: no otadata writes or reboot loop. */
    return runtime_healthy ? ESP_OK : ESP_FAIL;
#endif
}

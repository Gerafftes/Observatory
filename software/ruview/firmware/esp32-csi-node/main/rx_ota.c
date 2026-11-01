#include "rx_diag.h"
#include "rx_control.h"
#include "diag_http.h"
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
#include "esp_system.h"
#include "csi_collector.h"
#include "csi_transport.h"
#include "csi_peer.h"
#include "stream_sender.h"
#include "status_led.h"
#include "esp_timer.h"
#include "esp_wifi.h"
#include "esp_attr.h"
#include "freertos/idf_additions.h"
#include "esp_heap_caps.h"

static const char *TAG = "ota_update";

#define OTA_HTTP_TASK_STACK_BYTES 8192U

/* RX3 sustained 365344 bytes / 25.030 s (14.25 KiB/s) in the preserved
 * deadline failure. Allow 8 KiB/s plus a five-second startup allowance;
 * the socket still rejects five seconds without receive progress. */
#define OTA_MIN_BYTES_PER_SECOND 8192LL
#define OTA_START_ALLOWANCE_US 5000000LL

/** OTA HTTP server port. */
#define OTA_PORT 8032

/** NVS namespace and key for the OTA pre-shared key. */
#define OTA_NVS_NAMESPACE "security"
#define OTA_NVS_KEY       "ota_psk"

/** Maximum PSK length (hex-encoded SHA-256). */
#define OTA_PSK_MAX_LEN   65

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
    RX_HTTP_SCOPE(http_scope, RX_HTTP_STATUS);
    if (!ota_check_auth(req)) return httpd_resp_send_err(req, HTTPD_403_FORBIDDEN, "Authentication required");
    if (diag_http_requested(req)) return diag_http_send(req);
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

    csi_transport_stats_t transport;
    stream_sender_stats_t socket_stats;
    csi_transport_get_stats(&transport);
    stream_sender_get_stats(&socket_stats);

    char response[2048];
    int len = snprintf(response, sizeof(response),
        "{\"node_id\":\"RX%u\",\"boot_id\":\"%016llx\","
        "\"protocol_version\":%u,\"sequence\":%lu,\"rssi_dbm\":%d,"
        "\"ip\":\"%s\",\"version\":\"%s\",\"date\":\"%s\",\"time\":\"%s\","
        "\"image_sha256\":\"%s\",\"ota_state\":\"%s\","
        "\"running_partition\":\"%s\",\"next_partition\":\"%s\","
        "\"max_size\":%lu,"
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
        running ? running->label : "unknown",
        update ? update->label : "none",
        (unsigned long)(update ? update->size : 0),
        (unsigned long)transport.queue_depth,
        (unsigned long)transport.captures_enqueued,
        (unsigned long)transport.queue_publish_failures+transport.pool_exhausted_drops,
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
    return httpd_resp_send(req, response, len);
}

/* Per-boot bounded diagnostics; never stores credentials or image bytes. */
static struct {
    const char *phase;
    int expected, received, written, error, timeouts;
    int64_t started_us, updated_us;
    int64_t receive_us, write_us, integrity_us;
    uint32_t heap_min, stack_min, heap_integrity_checks;
    bool heap_integrity_ok;
    const char *heap_integrity_phase;
} s_upload = {.phase="idle"};

/* Diagnostic-only RTC RAM breadcrumb: survives a watchdog reset without
 * touching NVS/flash. The previous guards produced no panic backtrace. */
static RTC_NOINIT_ATTR volatile struct {
    uint32_t magic, step;
    int expected, received, written;
} s_ota_trace;

static void ota_trace_step(uint32_t step)
{
    s_ota_trace.expected=s_upload.expected;
    s_ota_trace.received=s_upload.received;
    s_ota_trace.written=s_upload.written;
    s_ota_trace.step=step;
    s_ota_trace.magic=0x4f544152U;
}

static void ota_trace_boot(void)
{
    if (s_ota_trace.magic==0x4f544152U && s_ota_trace.step>=1 && s_ota_trace.step<=11 &&
        s_ota_trace.expected>0 && s_ota_trace.expected<=0x200000 &&
        s_ota_trace.written>=0 && s_ota_trace.written<=s_ota_trace.received &&
        s_ota_trace.received<=s_ota_trace.expected)
        ESP_LOGW(TAG,"OTA previous_reset=%d step=%lu received=%d written=%d expected=%d",
            (int)esp_reset_reason(),(unsigned long)s_ota_trace.step,
            s_ota_trace.received,s_ota_trace.written,s_ota_trace.expected);
    s_ota_trace.magic=0;
}

int ota_update_format_progress(char *out, size_t size)
{
    return snprintf(out,size,
        "{\"phase\":\"%s\",\"expected\":%d,\"received\":%d,\"written\":%d,"
        "\"error\":%d,\"timeouts\":%d,\"started_us\":%lld,\"updated_us\":%lld,"
        "\"heap_min_bytes\":%lu,\"stack_min_bytes\":%lu,\"heap_integrity_checks\":%lu,\"heap_integrity_ok\":%s,\"heap_integrity_phase\":\"%s\"}",
        s_upload.phase,s_upload.expected,s_upload.received,s_upload.written,
        s_upload.error,s_upload.timeouts,(long long)s_upload.started_us,
        (long long)s_upload.updated_us,(unsigned long)s_upload.heap_min,
        (unsigned long)s_upload.stack_min,(unsigned long)s_upload.heap_integrity_checks,
        s_upload.heap_integrity_checks?(s_upload.heap_integrity_ok?"true":"false"):"null",
        s_upload.heap_integrity_phase?s_upload.heap_integrity_phase:"not_checked");
}

static bool ota_heap_guard(const char *phase)
{
    int64_t before = esp_timer_get_time();
    bool ok = heap_caps_check_integrity(MALLOC_CAP_INTERNAL|MALLOC_CAP_8BIT,true);
    s_upload.integrity_us += esp_timer_get_time() - before;
    if (!ok) {
        s_upload.heap_integrity_ok=false;
        s_upload.heap_integrity_phase=phase;
        ESP_LOGE(TAG,"OTA first_bad_boundary=%s received=%d written=%d",phase,
            s_upload.received,s_upload.written);
    }
    return ok;
}

static bool ota_heap_check(const char *phase)
{
    s_upload.heap_integrity_phase=phase;
    s_upload.heap_integrity_checks++;
    s_upload.heap_integrity_ok=ota_heap_guard(phase);
    ESP_LOGI(TAG,"OTA heap_check=%s ok=%d written=%d checks=%lu",phase,
        s_upload.heap_integrity_ok,s_upload.written,(unsigned long)s_upload.heap_integrity_checks);
    return s_upload.heap_integrity_ok;
}

static void ota_progress(const char *phase, esp_err_t error)
{
    s_upload.phase=phase;s_upload.error=error;s_upload.updated_us=esp_timer_get_time();
    uint32_t heap=esp_get_free_heap_size(),stack=uxTaskGetStackHighWaterMark(NULL);
    if (heap<s_upload.heap_min) s_upload.heap_min=heap;
    if (stack<s_upload.stack_min) s_upload.stack_min=stack;
    ESP_LOGI(TAG,"OTA phase=%s received=%d written=%d expected=%d error=%d heap=%lu stack=%lu boot_us=%lld",
        phase,s_upload.received,s_upload.written,s_upload.expected,error,
        (unsigned long)heap,(unsigned long)stack,(long long)s_upload.updated_us);
    ESP_LOGI(TAG,"OTA time receive_us=%lld write_us=%lld integrity_us=%lld",
        (long long)s_upload.receive_us,(long long)s_upload.write_us,(long long)s_upload.integrity_us);
#if CONFIG_FREERTOS_GENERATE_RUN_TIME_STATS
    ESP_LOGI(TAG,"OTA idle_us=%llu,%llu",
        (unsigned long long)ulTaskGetIdleRunTimeCounterForCore(0),
        (unsigned long long)ulTaskGetIdleRunTimeCounterForCore(1));
#endif
}

static esp_err_t ota_upload_handler(httpd_req_t *req)
{
    RX_HTTP_SCOPE(http_scope, RX_HTTP_OTA_BEGIN);
    if (!ota_check_auth(req)) {
        httpd_resp_send_err(req,HTTPD_403_FORBIDDEN,"Authentication required");
        return ESP_FAIL;
    }
    if (rx_control_measurement_active())
        return httpd_resp_send_err(req, HTTPD_400_BAD_REQUEST, "Measurement lease active");
    const esp_partition_t *running=esp_ota_get_running_partition();
    const esp_partition_t *update=esp_ota_get_next_update_partition(NULL);
    esp_ota_img_states_t running_state=ESP_OTA_IMG_UNDEFINED;
    if (!running || esp_ota_get_state_partition(running,&running_state)!=ESP_OK || running_state!=ESP_OTA_IMG_VALID)
        return httpd_resp_send_err(req,HTTPD_400_BAD_REQUEST,"Running app must be VALID before another upload");
    if (!running || !update || running->address==update->address ||
        update->type!=ESP_PARTITION_TYPE_APP ||
        (update->subtype!=ESP_PARTITION_SUBTYPE_APP_OTA_0 && update->subtype!=ESP_PARTITION_SUBTYPE_APP_OTA_1)) {
        httpd_resp_send_err(req,HTTPD_500_INTERNAL_SERVER_ERROR,"No inactive OTA app partition");
        return ESP_FAIL;
    }
    if (req->content_len<=0 || (size_t)req->content_len>update->size) {
        httpd_resp_send_err(req,HTTPD_400_BAD_REQUEST,"Invalid firmware size for OTA partition");
        return ESP_FAIL;
    }
    /* No flash access while the driver callback or sender owns captures. */
    esp_err_t stop_error = esp_wifi_set_csi(false);
    if (stop_error != ESP_OK) return httpd_resp_send_err(req, HTTPD_500_INTERNAL_SERVER_ERROR, "CSI stop failed");
    csi_collector_notify_network_change(false);
    csi_peer_stop();
    csi_transport_stop();
    s_upload=(__typeof__(s_upload)){.phase="begin",.expected=req->content_len,
        .started_us=esp_timer_get_time(),.heap_min=esp_get_free_heap_size(),
        .stack_min=uxTaskGetStackHighWaterMark(NULL)};
    status_led_ota(true,false);
    esp_ota_handle_t handle=0;
    esp_err_t err=ESP_ERR_INVALID_STATE;
    bool handle_open=false;
    const char *failure="Heap integrity before begin";
    ota_trace_step(1);
    if (!ota_heap_check("before_begin")) goto fail;
    /* Erase only the image extent before receiving. RX3 repeatedly reset
     * inside sequential writes crossing an erase boundary under CSI load. */
    err=esp_ota_begin(update,req->content_len,&handle);
    handle_open=err==ESP_OK;failure="OTA begin failed";
    if (err!=ESP_OK) goto fail;
    if (!ota_heap_check("after_begin")) {err=ESP_ERR_INVALID_STATE;failure="Heap integrity after begin";goto fail;}
    char buf[1024];
    int next_progress=64*1024;
    const int64_t receive_budget_us = OTA_START_ALLOWANCE_US +
        ((int64_t)req->content_len * 1000000LL + OTA_MIN_BYTES_PER_SECOND - 1) /
        OTA_MIN_BYTES_PER_SECOND;
    while (s_upload.written<req->content_len) {
        /* Five-second no-progress timeout plus size-derived receive/write envelope.
         * No retry of an ambiguous receive; the old app keeps running. */
        if (esp_timer_get_time()-s_upload.started_us>=receive_budget_us) {
            err=ESP_ERR_TIMEOUT;failure="OTA deadline";goto fail;
        }
        s_upload.phase="receiving";
        ota_trace_step(2);
        int remaining=req->content_len-s_upload.received;
        int64_t before = esp_timer_get_time();
        rx_http_change(&http_scope, RX_HTTP_OTA_RECV);
        int n=httpd_req_recv(req,buf,remaining<(int)sizeof(buf)?remaining:(int)sizeof(buf));
        s_upload.receive_us += esp_timer_get_time() - before;
        if (n<=0 || n>remaining || n>(int)sizeof(buf)) {
            if (n==HTTPD_SOCK_ERR_TIMEOUT) s_upload.timeouts++;
            err=n==HTTPD_SOCK_ERR_TIMEOUT?ESP_ERR_TIMEOUT:ESP_FAIL;
            failure="OTA receive failed";goto fail;
        }
        s_upload.received+=n;
        ota_trace_step(3);
        if (!ota_heap_guard("after_receive")) {err=ESP_ERR_INVALID_STATE;failure="Heap integrity after receive";goto fail;}
        s_upload.phase="writing";
        before = esp_timer_get_time();
        ota_trace_step(4);
        rx_http_change(&http_scope, RX_HTTP_OTA_WRITE);
        err=esp_ota_write(handle,buf,n);
        s_upload.write_us += esp_timer_get_time() - before;
        if (err!=ESP_OK) {failure="OTA write failed";goto fail;}
        s_upload.written+=n;
        ota_trace_step(5);
        if (!ota_heap_guard("after_write")) {err=ESP_ERR_INVALID_STATE;failure="Heap integrity after write";goto fail;}
        if (s_upload.written>=next_progress) {
            if (!ota_heap_check("after_write_64K")) {err=ESP_ERR_INVALID_STATE;failure="Heap integrity after write";goto fail;}
            ota_progress("receiving",ESP_OK);next_progress+=64*1024;
        }
    }
    if (esp_timer_get_time()-s_upload.started_us>=receive_budget_us) {
        err=ESP_ERR_TIMEOUT;failure="OTA deadline";goto fail;
    }
    ota_trace_step(6);
    if (!ota_heap_check("before_validation")) {err=ESP_ERR_INVALID_STATE;failure="Heap integrity before validation";goto fail;}
    ota_progress("validating",ESP_OK);
    ota_trace_step(7);
    rx_http_change(&http_scope, RX_HTTP_OTA_END);
        err=esp_ota_end(handle);handle_open=false;
    if (err!=ESP_OK) {failure="OTA validation failed";goto fail;}
    ota_trace_step(8);
    if (!ota_heap_check("after_validation")) {err=ESP_ERR_INVALID_STATE;failure="Heap integrity after validation";goto fail;}
    ota_trace_step(9);
    err=esp_ota_set_boot_partition(update);
    if (err!=ESP_OK) {failure="OTA boot selection failed";goto fail;}
    ota_progress("reboot_pending",ESP_OK);
    char response[512];
    int len=snprintf(response,sizeof(response),
        "{\"status\":\"ok\",\"received\":%d,\"written\":%d,\"elapsed_us\":%lld,\"partition\":\"%s\",\"heap_min_bytes\":%lu,\"stack_min_bytes\":%lu}",
        s_upload.received,s_upload.written,(long long)(s_upload.updated_us-s_upload.started_us),
        update->label,(unsigned long)s_upload.heap_min,(unsigned long)s_upload.stack_min);
    httpd_resp_set_type(req,"application/json");
    /* Boot selection succeeded even if the client has already disconnected.
     * The controller must bind the new boot rather than blindly resend. */
    ota_trace_step(10);
    (void)httpd_resp_send(req,response,len);
    ota_trace_step(11);
    s_ota_trace.magic=0;
    vTaskDelay(pdMS_TO_TICKS(1000));esp_restart();return ESP_OK;
fail:
    rx_http_change(&http_scope, RX_HTTP_OTA_ABORT);
    if (handle_open) (void)esp_ota_abort(handle);
    ota_progress(failure,err);status_led_ota(false,true);
    s_ota_trace.magic=0;
    httpd_resp_send_err(req,HTTPD_500_INTERNAL_SERVER_ERROR,failure);
    return ESP_FAIL;
}

static esp_err_t led_test_handler(httpd_req_t *req)
{
    if (!ota_check_auth(req)) return httpd_resp_send_err(req,HTTPD_403_FORBIDDEN,"Authentication required");
    if (req->content_len!=0 || !status_led_test())
        return httpd_resp_send_err(req,HTTPD_400_BAD_REQUEST,"Empty request and idle LED required");
    httpd_resp_set_type(req,"application/json");
    return httpd_resp_send(req,"{\"status\":\"ok\",\"duration_ms\":38400,\"step_ms\":4800,\"sequence\":[\"START\",\"NORMAL\",\"WIFI_IP\",\"TX1_STALE\",\"CSI_PEER_INIT\",\"LOCAL_TRANSPORT\",\"OTA\",\"INTERNAL\"]}",HTTPD_RESP_USE_STRLEN);
}

/* Controlled app-only restart for the finite peer window; never opens NVS. */
static esp_err_t ota_restart_handler(httpd_req_t *req)
{
    if (!ota_check_auth(req))
        return httpd_resp_send_err(req,HTTPD_403_FORBIDDEN,"Authentication required");
    if (rx_control_measurement_active())
        return httpd_resp_send_err(req, HTTPD_400_BAD_REQUEST, "Measurement lease active");
    const esp_partition_t *running=esp_ota_get_running_partition();
    esp_ota_img_states_t state=ESP_OTA_IMG_UNDEFINED;
    if (req->content_len!=0 || running==NULL ||
        esp_ota_get_state_partition(running,&state)!=ESP_OK || state!=ESP_OTA_IMG_VALID)
        return httpd_resp_send_err(req,HTTPD_400_BAD_REQUEST,"Empty request and VALID app required");
    esp_err_t result=httpd_resp_send(req,"{\"status\":\"ok\",\"restarting\":true}",HTTPD_RESP_USE_STRLEN);
    if(result!=ESP_OK) return result;
    vTaskDelay(pdMS_TO_TICKS(1000));esp_restart();return ESP_OK;
}

/** Internal: start the HTTP server and register OTA endpoints. */
static esp_err_t ota_start_server(httpd_handle_t *out_handle)
{
    httpd_config_t config = HTTPD_DEFAULT_CONFIG();
    config.stack_size = OTA_HTTP_TASK_STACK_BYTES;
    config.server_port = OTA_PORT;
    config.max_uri_handlers = 12;  /* Extra slots for WASM endpoints (ADR-040). */
    /* A stalled upload must return to the old app instead of looping forever. */
    config.recv_wait_timeout = 5;

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
    err=httpd_register_uri_handler(server,&status_uri);
    if(err!=ESP_OK) {httpd_stop(server);if(out_handle)*out_handle=NULL;return err;}

    httpd_uri_t upload_uri = {
        .uri      = "/ota",
        .method   = HTTP_POST,
        .handler  = ota_upload_handler,
        .user_ctx = NULL,
    };
    err=httpd_register_uri_handler(server,&upload_uri);
    if(err!=ESP_OK) {httpd_stop(server);if(out_handle)*out_handle=NULL;return err;}
    httpd_uri_t led_uri={.uri="/led/test",.method=HTTP_POST,.handler=led_test_handler,.user_ctx=NULL};
    err=httpd_register_uri_handler(server,&led_uri);
    if(err!=ESP_OK) {httpd_stop(server);if(out_handle)*out_handle=NULL;return err;}
    httpd_uri_t restart_uri={.uri="/restart",.method=HTTP_POST,.handler=ota_restart_handler,.user_ctx=NULL};
    err=httpd_register_uri_handler(server,&restart_uri);
    if(err!=ESP_OK) {httpd_stop(server);if(out_handle)*out_handle=NULL;return err;}


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
    memset(s_ota_psk, 0, sizeof(s_ota_psk));
    ota_trace_boot();
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

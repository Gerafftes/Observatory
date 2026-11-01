#include "rx_control.h"
#include "rx_settings.h"
#include "rx_diag.h"
#include "diag_http.h"
#include "ota_update.h"
#include "csi_peer.h"
#include "mbedtls/sha256.h"
#include "esp_timer.h"
#include "cJSON.h"
#include <limits.h>
#include <math.h>
#include <stdio.h>
#include <string.h>

static int64_t measurement_until_us;
static char measurement_owner[65];

static bool receiver_id(char out[65])
{
    uint8_t digest[32];
    if (!rx_mac_valid(csi_peer_self_mac()) || mbedtls_sha256(csi_peer_self_mac(), 6, digest, 0)) return false;
    for (unsigned i = 0; i < 32; ++i) snprintf(out + 2 * i, 3, "%02x", digest[i]);
    return true;
}

bool rx_control_measurement_active(void)
{
    return esp_timer_get_time() < measurement_until_us;
}

static esp_err_t json_send(httpd_req_t *req, cJSON *json)
{
    if (!json) return httpd_resp_send_err(req, HTTPD_500_INTERNAL_SERVER_ERROR, "JSON allocation failed");
    char *body = cJSON_PrintUnformatted(json);
    cJSON_Delete(json);
    if (!body) return httpd_resp_send_err(req, HTTPD_500_INTERNAL_SERVER_ERROR, "JSON allocation failed");
    httpd_resp_set_type(req, "application/json");
    esp_err_t err = httpd_resp_send(req, body, HTTPD_RESP_USE_STRLEN);
    cJSON_free(body);
    return err;
}

static cJSON *read_json(httpd_req_t *req)
{
    char body[1024];
    if (req->content_len <= 0 || (size_t)req->content_len >= sizeof(body)) return NULL;
    int received = 0;
    int64_t deadline = esp_timer_get_time() + 5000000;
    while (received < req->content_len && esp_timer_get_time() < deadline) {
        int n = httpd_req_recv(req, body + received, req->content_len - received);
        if (n <= 0) return NULL;
        received += n;
    }
    if (received != req->content_len) return NULL;
    body[received] = 0;
    cJSON *json = cJSON_ParseWithLengthOpts(body, received + 1, NULL, true);
    if (!cJSON_IsObject(json)) { cJSON_Delete(json); return NULL; }
    /* Duplicate keys are ambiguous even when a parser chooses the first. */
    for (cJSON *a = json->child; a; a = a->next)
        for (cJSON *b = a->next; b; b = b->next)
            if (!strcmp(a->string, b->string)) { cJSON_Delete(json); return NULL; }
    return json;
}

static bool number(cJSON *item, uint32_t min, uint32_t max, uint32_t *out)
{
    if (!cJSON_IsNumber(item) || !isfinite(item->valuedouble)
        || item->valuedouble < min || item->valuedouble > max
        || floor(item->valuedouble) != item->valuedouble) return false;
    *out = (uint32_t)item->valuedouble;
    return true;
}

static bool mac(cJSON *item, uint8_t out[6])
{
    if (!cJSON_IsString(item) || strlen(item->valuestring) != 17) return false;
    for (unsigned i = 0; i < 17; ++i)
        if (i % 3 == 2 ? item->valuestring[i] != ':'
            : !strchr("0123456789abcdefABCDEF", item->valuestring[i])) return false;
    unsigned values[6]; char tail;
    if (sscanf(item->valuestring, "%2x:%2x:%2x:%2x:%2x:%2x%c",
        &values[0], &values[1], &values[2], &values[3], &values[4], &values[5], &tail) != 6) return false;
    for (unsigned i = 0; i < 6; ++i) out[i] = values[i];
    return rx_mac_valid(out);
}

static cJSON *profile_json(const nvs_config_t *c, const rx_profile_t *p)
{
    cJSON *j = cJSON_CreateObject();
    if (!j) return NULL;
    cJSON_AddNumberToObject(j, "generation", p->generation);
    cJSON_AddNumberToObject(j, "node_id", p->node_id);
    cJSON_AddNumberToObject(j, "led_gpio", p->led_gpio);
    cJSON_AddStringToObject(j, "mode", rx_profile_mode_name(p->mode));
    cJSON_AddNumberToObject(j, "mode_id", p->mode);
    cJSON_AddNumberToObject(j, "rate_hz", p->rate_hz);
    cJSON_AddNumberToObject(j, "peer_count", p->peer_count);
    cJSON_AddStringToObject(j, "target_ip", c->target_ip);
    cJSON_AddNumberToObject(j, "target_port", c->target_port);
    cJSON_AddNumberToObject(j, "csi_channel", c->csi_channel);
    uint8_t digest[32]; char config_hash[65];
    if (!rx_settings_hash(c, p, digest)) {
        cJSON_Delete(j); return NULL;
    }
    for (unsigned i = 0; i < 32; ++i) snprintf(config_hash + 2 * i, 3, "%02x", digest[i]);
    cJSON_AddStringToObject(j, "config_sha256", config_hash);
    cJSON *peers = cJSON_AddArrayToObject(j, "peer_ids");
    if (!peers) { cJSON_Delete(j); return NULL; }
    for (unsigned i = 0; i < p->peer_count; ++i) cJSON_AddItemToArray(peers, cJSON_CreateNumber(p->peers[i].node_id));
    return j;
}

static esp_err_t config_get(httpd_req_t *req)
{
    RX_HTTP_SCOPE(scope, RX_HTTP_CONFIG);
    if (!ota_check_auth(req)) return httpd_resp_send_err(req, HTTPD_403_FORBIDDEN, "Authentication required");
    nvs_config_t c = *rx_settings_active_config(); rx_profile_t p;
    if (rx_settings_load(&c, &p) != ESP_OK)
        return httpd_resp_send_err(req, HTTPD_500_INTERNAL_SERVER_ERROR, "Stored config invalid");
    cJSON *j = cJSON_CreateObject();
    if (!j) return ESP_ERR_NO_MEM;
    cJSON_AddStringToObject(j, "schema", "observatory.rx-config.v1");
    cJSON_AddItemToObject(j, "active", profile_json(rx_settings_active_config(), rx_settings_active_profile()));
    cJSON_AddItemToObject(j, "stored", profile_json(&c, &p));
    cJSON_AddBoolToObject(j, "reboot_required", p.generation != rx_settings_active_profile()->generation);
    return json_send(req, j);
}

static esp_err_t config_post(httpd_req_t *req)
{
    RX_HTTP_SCOPE(scope, RX_HTTP_CONFIG);
    if (!ota_check_auth(req)) return httpd_resp_send_err(req, HTTPD_403_FORBIDDEN, "Authentication required");
    char expected[65], actual[65];
    if (!receiver_id(actual) || httpd_req_get_hdr_value_str(req, "X-RX-Device-ID", expected, sizeof(expected)) != ESP_OK
        || strcmp(actual, expected)) return httpd_resp_send_err(req, HTTPD_400_BAD_REQUEST, "Physical device identity mismatch");
    if (rx_control_measurement_active()) return httpd_resp_send_err(req, HTTPD_400_BAD_REQUEST, "Measurement lease active");
    cJSON *j = read_json(req);
    if (!j) return httpd_resp_send_err(req, HTTPD_400_BAD_REQUEST, "Invalid bounded JSON");
    nvs_config_t c = *rx_settings_active_config(); rx_profile_t p;
    bool valid = rx_settings_load(&c, &p) == ESP_OK;
    uint32_t value = 0;
    valid = valid && number(cJSON_GetObjectItemCaseSensitive(j, "expected_generation"), 1, UINT32_MAX - 1, &value)
        && value == p.generation;
    for (cJSON *item = j->child; valid && item; item = item->next) {
        const char *key = item->string;
        if (!strcmp(key, "expected_generation")) continue;
        if (!strcmp(key, "node_id")) {
            valid = number(item, 1, 4, &value); p.node_id = c.node_id = value;
        } else if (!strcmp(key, "led_gpio")) {
            valid = number(item, 38, 48, &value); p.led_gpio = value;
        } else if (!strcmp(key, "target_port")) {
            valid = number(item, 1, UINT16_MAX, &value); c.target_port = value;
        } else if (!strcmp(key, "csi_channel")) {
            valid = number(item, 1, 13, &value); c.csi_channel = value;
        } else if (!strcmp(key, "rate_hz")) {
            valid = number(item, 20, 50, &value); p.rate_hz = value;
        } else if (!strcmp(key, "target_ip") || !strcmp(key, "ssid") || !strcmp(key, "password")) {
            char *dest = !strcmp(key, "target_ip") ? c.target_ip : (!strcmp(key, "ssid") ? c.wifi_ssid : c.wifi_password);
            size_t size = !strcmp(key, "target_ip") ? sizeof(c.target_ip) : (!strcmp(key, "ssid") ? sizeof(c.wifi_ssid) : sizeof(c.wifi_password));
            valid = cJSON_IsString(item) && strlen(item->valuestring) < size;
            if (valid) { memset(dest, 0, size); strcpy(dest, item->valuestring); }
        } else if (!strcmp(key, "tx_mac")) {
            valid = mac(item, p.tx_mac); memcpy(c.filter_mac, p.tx_mac, 6);
        } else if (!strcmp(key, "mode")) {
            valid = false;
            for (unsigned i = RX_BASELINE; cJSON_IsString(item) && i <= RX_MESH_CONTINUOUS; ++i)
                if (!strcmp(item->valuestring, rx_profile_mode_name(i))) { p.mode = i; valid = true; break; }
        } else if (!strcmp(key, "peers")) {
            int count = cJSON_GetArraySize(item);
            valid = cJSON_IsArray(item) && count >= 0 && (unsigned)count <= RX_MAX_PEERS;
            if (valid) {
                memset(p.peers, 0, sizeof(p.peers)); p.peer_count = count;
                for (int i = 0; valid && i < count; ++i) {
                    cJSON *peer = cJSON_GetArrayItem(item, i);
                    valid = cJSON_IsObject(peer) && cJSON_GetArraySize(peer) == 2
                        && number(cJSON_GetObjectItemCaseSensitive(peer, "node_id"), 1, 4, &value)
                        && mac(cJSON_GetObjectItemCaseSensitive(peer, "mac"), p.peers[i].mac);
                    p.peers[i].node_id = value;
                }
            }
        } else valid = false;
    }
    if (valid) ++p.generation;
    valid = valid && rx_settings_valid(&c, &p);
    cJSON_Delete(j);
    if (!valid) return httpd_resp_send_err(req, HTTPD_400_BAD_REQUEST, "Invalid complete config or stale generation");
    if (rx_settings_store(&c, &p) != ESP_OK)
        return httpd_resp_send_err(req, HTTPD_500_INTERNAL_SERVER_ERROR, "Config commit/readback failed; inspect stored state");
    return config_get(req);
}

static esp_err_t capabilities(httpd_req_t *req)
{
    if (!ota_check_auth(req)) return httpd_resp_send_err(req, HTTPD_403_FORBIDDEN, "Authentication required");
    cJSON *j = cJSON_CreateObject();
    if (!j) return ESP_ERR_NO_MEM;
    cJSON_AddStringToObject(j, "schema", "observatory.rx-capabilities.v1");
    char device_id[65];
    if (!receiver_id(device_id)) {
        cJSON_Delete(j); return ESP_FAIL;
    }
    cJSON_AddStringToObject(j, "device_id", device_id);
    cJSON_AddNumberToObject(j, "wire_version", 4);
    cJSON_AddNumberToObject(j, "max_peers", RX_MAX_PEERS);
    cJSON_AddNumberToObject(j, "total_hz", 50);
    cJSON_AddNumberToObject(j, "capture_rate_min_hz", 20);
    cJSON_AddNumberToObject(j, "capture_rate_step_hz", 5);
    cJSON_AddNumberToObject(j, "peer_total_hz", 10);
    cJSON_AddNumberToObject(j, "trial_boot_seconds", 900);
    cJSON_AddBoolToObject(j, "gain_available", false);
    cJSON_AddBoolToObject(j, "measurement_active", rx_control_measurement_active());
    cJSON_AddStringToObject(j, "measurement_owner", rx_control_measurement_active() ? measurement_owner : "");
    cJSON_AddNumberToObject(j, "measurement_until_boot_us", (double)measurement_until_us);
    return json_send(req, j);
}

static esp_err_t measurement(httpd_req_t *req)
{
    if (!ota_check_auth(req)) return httpd_resp_send_err(req, HTTPD_403_FORBIDDEN, "Authentication required");
    cJSON *j = read_json(req); uint32_t seconds;
    cJSON *owner = j ? cJSON_GetObjectItemCaseSensitive(j, "owner") : NULL;
    bool valid = j && cJSON_GetArraySize(j) == 2 && cJSON_IsString(owner)
        && strlen(owner->valuestring)>0 && strlen(owner->valuestring)<sizeof(measurement_owner)
        && number(cJSON_GetObjectItemCaseSensitive(j, "seconds"), 0, 3600, &seconds);
    if (valid) for (const char *p=owner->valuestring; *p; ++p)
        if (!strchr("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_.",*p)) valid=false;
    if (valid && ((rx_control_measurement_active() || seconds==0) && measurement_owner[0])
        && strcmp(owner->valuestring,measurement_owner)) valid=false;
    char requested_owner[65]={0};
    if (valid) strcpy(requested_owner,owner->valuestring);
    cJSON_Delete(j);
    if (!valid) return httpd_resp_send_err(req, HTTPD_400_BAD_REQUEST, "Expected owned lease with seconds 0..3600; another active owner cannot be replaced");
    strcpy(measurement_owner, seconds ? requested_owner : "");
    measurement_until_us = esp_timer_get_time() + (int64_t)seconds * 1000000;
    return capabilities(req);
}

static esp_err_t diagnostics(httpd_req_t *req)
{
    if (!ota_check_auth(req)) return httpd_resp_send_err(req, HTTPD_403_FORBIDDEN, "Authentication required");
    return diag_http_send(req);
}

esp_err_t rx_control_register(httpd_handle_t server)
{
    const httpd_uri_t routes[] = {
        {.uri="/config", .method=HTTP_GET, .handler=config_get},
        {.uri="/config", .method=HTTP_POST, .handler=config_post},
        {.uri="/capabilities", .method=HTTP_GET, .handler=capabilities},
        {.uri="/measurement", .method=HTTP_POST, .handler=measurement},
        {.uri="/diag", .method=HTTP_GET, .handler=diagnostics},
    };
    for (unsigned i = 0; i < sizeof(routes) / sizeof(routes[0]); ++i) {
        esp_err_t err = httpd_register_uri_handler(server, &routes[i]);
        if (err != ESP_OK) return err;
    }
    return ESP_OK;
}

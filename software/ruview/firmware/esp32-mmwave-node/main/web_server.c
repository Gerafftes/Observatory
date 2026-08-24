#include "web_server.h"

#include <ctype.h>
#include <stdbool.h>
#include <stdio.h>
#include <string.h>

#include "esp_app_desc.h"
#include "esp_http_server.h"
#include "esp_log.h"
#include "esp_ota_ops.h"
#include "esp_system.h"
#include "cJSON.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "node_diagnostics.h"

static const char *TAG = "web_server";
static app_config_t *s_config;

static bool authorized(httpd_req_t *request)
{
    if (s_config->ota_psk[0] == '\0') {
        return false;
    }
    char header[96];
    if (httpd_req_get_hdr_value_str(request, "Authorization", header,
                                    sizeof(header)) != ESP_OK) {
        return false;
    }
    const char prefix[] = "Bearer ";
    if (strncmp(header, prefix, sizeof(prefix) - 1) != 0) {
        return false;
    }
    const char *token = header + sizeof(prefix) - 1;
    size_t expected_length = strlen(s_config->ota_psk);
    size_t actual_length = strlen(token);
    if (expected_length != actual_length) {
        return false;
    }
    uint8_t difference = 0;
    for (size_t index = 0; index < expected_length; ++index) {
        difference |= (uint8_t)(s_config->ota_psk[index] ^ token[index]);
    }
    return difference == 0;
}

static esp_err_t status_handler(httpd_req_t *request)
{
    app_config_t config;
    app_config_snapshot(s_config, &config);
    const esp_app_desc_t *app = esp_app_get_description();
    const esp_partition_t *running = esp_ota_get_running_partition();
    const esp_partition_t *next = esp_ota_get_next_update_partition(NULL);
    node_diagnostics_snapshot_t diagnostics = node_diagnostics_snapshot();
    char response[896];
    int length = snprintf(response, sizeof(response),
        "{\"node_id\":\"%s\",\"sensor\":\"HLK-LD2450\","
        "\"board\":\"ESP32-C3 Super Mini\",\"version\":\"%s\","
        "\"mode\":\"%s\",\"uart\":{\"rx_gpio\":20,\"tx_gpio\":21,"
        "\"baud\":256000},\"diagnostics\":{"
        "\"uart_bytes_received\":%llu,\"radar_frames_valid\":%llu,"
        "\"udp_packets_sent\":%llu,\"udp_send_failures\":%llu},"
        "\"target\":\"%s:%u\","
        "\"running_partition\":\"%s\",\"next_partition\":\"%s\","
        "\"ota_authenticated\":%s,"
        "\"transform\":{\"origin_x_mm\":%ld,\"origin_z_mm\":%ld,"
        "\"yaw_mdeg\":%ld,\"raw_x_inverted\":%s}}",
        config.node_id, app->version, measurement_mode_name(config.mode),
        (unsigned long long)diagnostics.uart_bytes_received,
        (unsigned long long)diagnostics.radar_frames_valid,
        (unsigned long long)diagnostics.udp_packets_sent,
        (unsigned long long)diagnostics.udp_send_failures,
        config.target_host, config.target_port,
        running ? running->label : "unknown", next ? next->label : "none",
        config.ota_psk[0] != '\0' ? "true" : "false",
        (long)config.origin_x_mm, (long)config.origin_z_mm,
        (long)config.yaw_mdeg, config.invert_raw_x ? "true" : "false");
    httpd_resp_set_type(request, "application/json");
    return httpd_resp_send(request, response, length);
}

static esp_err_t transform_handler(httpd_req_t *request)
{
    if (!authorized(request)) {
        return httpd_resp_send_err(request, HTTPD_403_FORBIDDEN,
                                   "Bearer token required");
    }
    if (request->content_len <= 0 || request->content_len >= 256) {
        return httpd_resp_send_err(request, HTTPD_400_BAD_REQUEST,
                                   "Invalid transform JSON size");
    }
    char body[256] = {0};
    int offset = 0;
    while (offset < request->content_len) {
        int received = httpd_req_recv(request, body + offset,
                                      request->content_len - offset);
        if (received == HTTPD_SOCK_ERR_TIMEOUT) {
            continue;
        }
        if (received <= 0) {
            return ESP_FAIL;
        }
        offset += received;
    }

    cJSON *json = cJSON_ParseWithLength(body, (size_t)offset);
    if (json == NULL) {
        return httpd_resp_send_err(request, HTTPD_400_BAD_REQUEST,
                                   "Invalid transform JSON");
    }
    const cJSON *origin_x = cJSON_GetObjectItemCaseSensitive(json, "origin_x_mm");
    const cJSON *origin_z = cJSON_GetObjectItemCaseSensitive(json, "origin_z_mm");
    const cJSON *yaw = cJSON_GetObjectItemCaseSensitive(json, "yaw_mdeg");
    const cJSON *invert = cJSON_GetObjectItemCaseSensitive(json, "raw_x_inverted");
    bool valid = cJSON_IsNumber(origin_x) && cJSON_IsNumber(origin_z) &&
                 cJSON_IsNumber(yaw) && cJSON_IsBool(invert) &&
                 origin_x->valuedouble == origin_x->valueint &&
                 origin_z->valuedouble == origin_z->valueint &&
                 yaw->valuedouble == yaw->valueint;
    if (!valid || !app_config_set_transform(
            s_config, origin_x ? origin_x->valueint : 0,
            origin_z ? origin_z->valueint : 0, yaw ? yaw->valueint : 0,
            cJSON_IsTrue(invert))) {
        cJSON_Delete(json);
        return httpd_resp_send_err(request, HTTPD_400_BAD_REQUEST,
                                   "Invalid or unpersistable transform");
    }
    cJSON_Delete(json);
    app_config_t config;
    app_config_snapshot(s_config, &config);
    char response[192];
    int length = snprintf(response, sizeof(response),
        "{\"transform\":{\"origin_x_mm\":%ld,\"origin_z_mm\":%ld,"
        "\"yaw_mdeg\":%ld,\"raw_x_inverted\":%s}}",
        (long)config.origin_x_mm, (long)config.origin_z_mm,
        (long)config.yaw_mdeg, config.invert_raw_x ? "true" : "false");
    httpd_resp_set_type(request, "application/json");
    return httpd_resp_send(request, response, length);
}

static esp_err_t mode_handler(httpd_req_t *request)
{
    if (!authorized(request)) {
        return httpd_resp_send_err(request, HTTPD_403_FORBIDDEN,
                                   "Bearer token required");
    }
    if (request->content_len <= 0 || request->content_len >= 32) {
        return httpd_resp_send_err(request, HTTPD_400_BAD_REQUEST,
                                   "Body must be calibration or reference");
    }
    char body[32] = {0};
    int offset = 0;
    while (offset < request->content_len) {
        int received = httpd_req_recv(request, body + offset,
                                      request->content_len - offset);
        if (received == HTTPD_SOCK_ERR_TIMEOUT) {
            continue;
        }
        if (received <= 0) {
            return ESP_FAIL;
        }
        offset += received;
    }
    body[offset] = '\0';
    while (offset > 0 && isspace((unsigned char)body[offset - 1])) {
        body[--offset] = '\0';
    }
    measurement_mode_t mode;
    if (!measurement_mode_parse(body, &mode)) {
        return httpd_resp_send_err(request, HTTPD_400_BAD_REQUEST,
                                   "Body must be calibration or reference");
    }
    if (!app_config_set_mode(s_config, mode)) {
        return httpd_resp_send_err(request, HTTPD_500_INTERNAL_SERVER_ERROR,
                                   "Could not persist mode");
    }
    const char *response = mode == MEASUREMENT_MODE_REFERENCE
        ? "{\"mode\":\"reference\"}"
        : "{\"mode\":\"calibration\"}";
    httpd_resp_set_type(request, "application/json");
    return httpd_resp_sendstr(request, response);
}

static esp_err_t ota_handler(httpd_req_t *request)
{
    if (!authorized(request)) {
        return httpd_resp_send_err(request, HTTPD_403_FORBIDDEN,
                                   "Bearer token required");
    }
    const esp_partition_t *partition = esp_ota_get_next_update_partition(NULL);
    if (partition == NULL || request->content_len <= 0 ||
        (size_t)request->content_len > partition->size) {
        return httpd_resp_send_err(request, HTTPD_400_BAD_REQUEST,
                                   "Invalid firmware size");
    }
    esp_ota_handle_t handle;
    esp_err_t error = esp_ota_begin(partition, OTA_WITH_SEQUENTIAL_WRITES, &handle);
    if (error != ESP_OK) {
        return httpd_resp_send_err(request, HTTPD_500_INTERNAL_SERVER_ERROR,
                                   "OTA begin failed");
    }
    char buffer[1024];
    int total = 0;
    while (total < request->content_len) {
        int received = httpd_req_recv(request, buffer, sizeof(buffer));
        if (received == HTTPD_SOCK_ERR_TIMEOUT) {
            continue;
        }
        if (received <= 0 || esp_ota_write(handle, buffer, received) != ESP_OK) {
            esp_ota_abort(handle);
            return httpd_resp_send_err(request, HTTPD_500_INTERNAL_SERVER_ERROR,
                                       "OTA receive or write failed");
        }
        total += received;
    }
    if (esp_ota_end(handle) != ESP_OK || esp_ota_set_boot_partition(partition) != ESP_OK) {
        return httpd_resp_send_err(request, HTTPD_500_INTERNAL_SERVER_ERROR,
                                   "OTA validation failed");
    }
    httpd_resp_set_type(request, "application/json");
    httpd_resp_sendstr(request, "{\"status\":\"ok\",\"rebooting\":true}");
    vTaskDelay(pdMS_TO_TICKS(500));
    esp_restart();
    return ESP_OK;
}

void web_server_start(app_config_t *config)
{
    s_config = config;
    httpd_config_t server_config = HTTPD_DEFAULT_CONFIG();
    server_config.server_port = 8032;
    server_config.recv_wait_timeout = 30;
    server_config.max_uri_handlers = 7;
    httpd_handle_t server = NULL;
    ESP_ERROR_CHECK(httpd_start(&server, &server_config));
    const httpd_uri_t status = {
        .uri = "/ota/status", .method = HTTP_GET, .handler = status_handler,
    };
    const httpd_uri_t mode = {
        .uri = "/mode", .method = HTTP_PUT, .handler = mode_handler,
    };
    const httpd_uri_t ota = {
        .uri = "/ota", .method = HTTP_POST, .handler = ota_handler,
    };
    const httpd_uri_t transform = {
        .uri = "/transform", .method = HTTP_PUT, .handler = transform_handler,
    };
    ESP_ERROR_CHECK(httpd_register_uri_handler(server, &status));
    ESP_ERROR_CHECK(httpd_register_uri_handler(server, &mode));
    ESP_ERROR_CHECK(httpd_register_uri_handler(server, &ota));
    ESP_ERROR_CHECK(httpd_register_uri_handler(server, &transform));
    ESP_LOGI(TAG, "HTTP status/mode/transform/OTA server listening on port 8032");
}

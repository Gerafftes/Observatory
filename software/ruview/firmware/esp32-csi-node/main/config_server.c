/**
 * @file config_server.c
 * @brief Development HTTP endpoint for updating CSI node NVS settings.
 *
 * This endpoint is intentionally small: it writes selected keys in the
 * existing "csi_cfg" NVS namespace so lab nodes can be retargeted without
 * a USB cable. Most settings are applied on the next boot; pass reboot=1 to
 * restart immediately after a successful write.
 */

#include "config_server.h"

#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "esp_log.h"
#include "esp_system.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "nvs.h"
#include "nvs_config.h"
#include "ota_update.h"

static const char *TAG = "config_server";

static bool parse_bool_value(const char *value)
{
    return value != NULL
        && (strcmp(value, "1") == 0
            || strcmp(value, "true") == 0
            || strcmp(value, "yes") == 0
            || strcmp(value, "on") == 0);
}

static bool parse_u32_range(const char *value, uint32_t min, uint32_t max, uint32_t *out)
{
    if (value == NULL || value[0] == '\0' || out == NULL) {
        return false;
    }

    char *end = NULL;
    unsigned long parsed = strtoul(value, &end, 10);
    if (end == value || *end != '\0' || parsed < min || parsed > max) {
        return false;
    }

    *out = (uint32_t)parsed;
    return true;
}

static bool validate_ipv4(const char *value)
{
    if (value == NULL || value[0] == '\0' || strlen(value) >= NVS_CFG_IP_MAX) {
        return false;
    }

    int octets = 0;
    const char *cursor = value;

    while (*cursor != '\0') {
        if (octets >= 4) {
            return false;
        }

        int digits = 0;
        int number = 0;
        while (*cursor >= '0' && *cursor <= '9') {
            number = number * 10 + (*cursor - '0');
            digits++;
            if (digits > 3 || number > 255) {
                return false;
            }
            cursor++;
        }

        if (digits == 0) {
            return false;
        }

        octets++;
        if (*cursor == '.') {
            cursor++;
            if (*cursor == '\0') {
                return false;
            }
        } else if (*cursor != '\0') {
            return false;
        }
    }

    return octets == 4;
}

static esp_err_t send_json(httpd_req_t *req, const char *json)
{
    httpd_resp_set_type(req, "application/json");
    return httpd_resp_send(req, json, HTTPD_RESP_USE_STRLEN);
}

static esp_err_t config_get_handler(httpd_req_t *req)
{
    if (!ota_check_auth(req)) {
        httpd_resp_send_err(req, HTTPD_403_FORBIDDEN,
                            "Authentication required. Use: Authorization: Bearer <psk>");
        return ESP_FAIL;
    }

    nvs_config_t cfg;
    nvs_config_load(&cfg);

    char response[512];
    snprintf(response, sizeof(response),
             "{"
             "\"target_ip\":\"%s\","
             "\"target_port\":%u,"
             "\"node_id\":%u,"
             "\"csi_channel\":%u,"
             "\"edge_tier\":%u,"
             "\"tdm_slot\":%u,"
             "\"tdm_total\":%u,"
             "\"filter_mac_configured\":%s"
             "}",
             cfg.target_ip,
             (unsigned)cfg.target_port,
             (unsigned)cfg.node_id,
             (unsigned)cfg.csi_channel,
             (unsigned)cfg.edge_tier,
             (unsigned)cfg.tdm_slot_index,
             (unsigned)cfg.tdm_node_count,
             cfg.filter_mac_set ? "true" : "false");

    return send_json(req, response);
}

static esp_err_t config_post_handler(httpd_req_t *req)
{
    if (!ota_check_auth(req)) {
        httpd_resp_send_err(req, HTTPD_403_FORBIDDEN,
                            "Authentication required. Use: Authorization: Bearer <psk>");
        return ESP_FAIL;
    }

    char query[256];
    esp_err_t err = httpd_req_get_url_query_str(req, query, sizeof(query));
    if (err != ESP_OK) {
        httpd_resp_send_err(req, HTTPD_400_BAD_REQUEST,
                            "Missing query string. Example: /config?target_ip=192.0.2.5&reboot=1");
        return ESP_FAIL;
    }

    nvs_handle_t handle;
    err = nvs_open("csi_cfg", NVS_READWRITE, &handle);
    if (err != ESP_OK) {
        ESP_LOGE(TAG, "Failed to open csi_cfg NVS namespace: %s", esp_err_to_name(err));
        httpd_resp_send_err(req, HTTPD_500_INTERNAL_SERVER_ERROR, "Failed to open NVS");
        return ESP_FAIL;
    }

    bool changed = false;
    char value[64];

#define RETURN_ON_NVS_WRITE_ERROR(call, key_name) do {                         \
        err = (call);                                                          \
        if (err != ESP_OK) {                                                   \
            ESP_LOGE(TAG, "Failed to write %s: %s", key_name, esp_err_to_name(err)); \
            nvs_close(handle);                                                 \
            httpd_resp_send_err(req, HTTPD_500_INTERNAL_SERVER_ERROR,          \
                                "Failed to write NVS key");                   \
            return ESP_FAIL;                                                   \
        }                                                                      \
    } while (0)

    if (httpd_query_key_value(query, "target_ip", value, sizeof(value)) == ESP_OK) {
        if (!validate_ipv4(value)) {
            nvs_close(handle);
            httpd_resp_send_err(req, HTTPD_400_BAD_REQUEST, "Invalid target_ip");
            return ESP_FAIL;
        }
        RETURN_ON_NVS_WRITE_ERROR(nvs_set_str(handle, "target_ip", value), "target_ip");
        changed = true;
    }

    if (httpd_query_key_value(query, "target_port", value, sizeof(value)) == ESP_OK) {
        uint32_t port = 0;
        if (!parse_u32_range(value, 1, 65535, &port)) {
            nvs_close(handle);
            httpd_resp_send_err(req, HTTPD_400_BAD_REQUEST, "Invalid target_port");
            return ESP_FAIL;
        }
        RETURN_ON_NVS_WRITE_ERROR(nvs_set_u16(handle, "target_port", (uint16_t)port), "target_port");
        changed = true;
    }

    if (httpd_query_key_value(query, "node_id", value, sizeof(value)) == ESP_OK) {
        uint32_t node_id = 0;
        if (!parse_u32_range(value, 0, 255, &node_id)) {
            nvs_close(handle);
            httpd_resp_send_err(req, HTTPD_400_BAD_REQUEST, "Invalid node_id");
            return ESP_FAIL;
        }
        RETURN_ON_NVS_WRITE_ERROR(nvs_set_u8(handle, "node_id", (uint8_t)node_id), "node_id");
        changed = true;
    }

    if (httpd_query_key_value(query, "csi_channel", value, sizeof(value)) == ESP_OK) {
        uint32_t channel = 0;
        if (!parse_u32_range(value, 0, 177, &channel)) {
            nvs_close(handle);
            httpd_resp_send_err(req, HTTPD_400_BAD_REQUEST, "Invalid csi_channel");
            return ESP_FAIL;
        }
        RETURN_ON_NVS_WRITE_ERROR(nvs_set_u8(handle, "csi_channel", (uint8_t)channel), "csi_channel");
        changed = true;
    }

    if (httpd_query_key_value(query, "edge_tier", value, sizeof(value)) == ESP_OK) {
        uint32_t edge_tier = 0;
        if (!parse_u32_range(value, 0, 2, &edge_tier)) {
            nvs_close(handle);
            httpd_resp_send_err(req, HTTPD_400_BAD_REQUEST, "Invalid edge_tier");
            return ESP_FAIL;
        }
        RETURN_ON_NVS_WRITE_ERROR(nvs_set_u8(handle, "edge_tier", (uint8_t)edge_tier), "edge_tier");
        changed = true;
    }

    if (httpd_query_key_value(query, "tdm_slot", value, sizeof(value)) == ESP_OK) {
        uint32_t tdm_slot = 0;
        if (!parse_u32_range(value, 0, 255, &tdm_slot)) {
            nvs_close(handle);
            httpd_resp_send_err(req, HTTPD_400_BAD_REQUEST, "Invalid tdm_slot");
            return ESP_FAIL;
        }
        RETURN_ON_NVS_WRITE_ERROR(nvs_set_u8(handle, "tdm_slot", (uint8_t)tdm_slot), "tdm_slot");
        changed = true;
    }

    if (httpd_query_key_value(query, "tdm_total", value, sizeof(value)) == ESP_OK) {
        uint32_t tdm_total = 0;
        if (!parse_u32_range(value, 1, 255, &tdm_total)) {
            nvs_close(handle);
            httpd_resp_send_err(req, HTTPD_400_BAD_REQUEST, "Invalid tdm_total");
            return ESP_FAIL;
        }
        RETURN_ON_NVS_WRITE_ERROR(nvs_set_u8(handle, "tdm_nodes", (uint8_t)tdm_total), "tdm_nodes");
        changed = true;
    }

    if (httpd_query_key_value(query, "clear_filter_mac", value, sizeof(value)) == ESP_OK
        && parse_bool_value(value)) {
        esp_err_t erase_err = nvs_erase_key(handle, "filter_mac");
        if (erase_err != ESP_OK && erase_err != ESP_ERR_NVS_NOT_FOUND) {
            ESP_LOGW(TAG, "Failed to erase filter_mac: %s", esp_err_to_name(erase_err));
        }
        changed = true;
    }

    if (!changed) {
        nvs_close(handle);
        httpd_resp_send_err(req, HTTPD_400_BAD_REQUEST,
                            "No supported config keys provided");
        return ESP_FAIL;
    }

    err = nvs_commit(handle);
    nvs_close(handle);
    if (err != ESP_OK) {
        ESP_LOGE(TAG, "NVS commit failed: %s", esp_err_to_name(err));
        httpd_resp_send_err(req, HTTPD_500_INTERNAL_SERVER_ERROR, "NVS commit failed");
        return ESP_FAIL;
    }

    bool reboot = false;
    if (httpd_query_key_value(query, "reboot", value, sizeof(value)) == ESP_OK) {
        reboot = parse_bool_value(value);
    }

    if (reboot) {
        ESP_LOGI(TAG, "Config updated via HTTP; rebooting to apply network settings");
        send_json(req, "{\"status\":\"ok\",\"rebooting\":true}");
        vTaskDelay(pdMS_TO_TICKS(500));
        esp_restart();
        return ESP_OK;
    }

    ESP_LOGI(TAG, "Config updated via HTTP; reboot required to apply network settings");
    return send_json(req, "{\"status\":\"ok\",\"rebooting\":false,\"reboot_required\":true}");

#undef RETURN_ON_NVS_WRITE_ERROR
}

esp_err_t config_server_register(httpd_handle_t server)
{
    if (server == NULL) {
        return ESP_ERR_INVALID_ARG;
    }

    httpd_uri_t get_uri = {
        .uri      = "/config",
        .method   = HTTP_GET,
        .handler  = config_get_handler,
        .user_ctx = NULL,
    };
    esp_err_t err = httpd_register_uri_handler(server, &get_uri);
    if (err != ESP_OK) {
        ESP_LOGE(TAG, "Failed to register GET /config: %s", esp_err_to_name(err));
        return err;
    }

    httpd_uri_t post_uri = {
        .uri      = "/config",
        .method   = HTTP_POST,
        .handler  = config_post_handler,
        .user_ctx = NULL,
    };
    err = httpd_register_uri_handler(server, &post_uri);
    if (err != ESP_OK) {
        ESP_LOGE(TAG, "Failed to register POST /config: %s", esp_err_to_name(err));
        return err;
    }

    ESP_LOGI(TAG, "Config HTTP endpoints registered");
    ESP_LOGI(TAG, "  GET  /config");
    ESP_LOGI(TAG, "  POST /config?target_ip=192.0.2.5&reboot=1");
    return ESP_OK;
}

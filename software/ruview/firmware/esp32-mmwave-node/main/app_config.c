#include "app_config.h"

#include <ctype.h>
#include <string.h>

#include "nvs.h"
#include "sdkconfig.h"
#include "freertos/FreeRTOS.h"

#define CONFIG_NAMESPACE "mmwave"
#define MODE_KEY "mode"
#define ORIGIN_X_KEY "origin_x"
#define ORIGIN_Z_KEY "origin_z"
#define YAW_KEY "yaw_mdeg"
#define INVERT_X_KEY "invert_x"
#define MAX_ROOM_COORDINATE_MM 100000
#define MAX_ABS_YAW_MDEG 360000

static portMUX_TYPE s_config_lock = portMUX_INITIALIZER_UNLOCKED;

static void copy_string(char *destination, size_t capacity, const char *source)
{
    if (capacity == 0) {
        return;
    }
    strncpy(destination, source, capacity - 1);
    destination[capacity - 1] = '\0';
}

static bool public_identifier_valid(const char *value)
{
    if (value == NULL || value[0] == '\0') {
        return false;
    }
    for (const unsigned char *cursor = (const unsigned char *)value;
         *cursor != '\0'; ++cursor) {
        if (!isalnum(*cursor) && *cursor != '-' && *cursor != '_' &&
            *cursor != '.' && *cursor != ':') {
            return false;
        }
    }
    return true;
}

bool app_config_load(app_config_t *config)
{
    memset(config, 0, sizeof(*config));
    copy_string(config->wifi_ssid, sizeof(config->wifi_ssid), CONFIG_MMWAVE_WIFI_SSID);
    copy_string(config->wifi_password, sizeof(config->wifi_password), CONFIG_MMWAVE_WIFI_PASSWORD);
    copy_string(config->target_host, sizeof(config->target_host), CONFIG_MMWAVE_TARGET_HOST);
    copy_string(config->node_id, sizeof(config->node_id), CONFIG_MMWAVE_NODE_ID);
    copy_string(config->ota_psk, sizeof(config->ota_psk), CONFIG_MMWAVE_OTA_PSK);
    config->target_port = CONFIG_MMWAVE_TARGET_PORT;
    config->origin_x_mm = CONFIG_MMWAVE_RADAR_ORIGIN_X_MM;
    config->origin_z_mm = CONFIG_MMWAVE_RADAR_ORIGIN_Z_MM;
    config->yaw_mdeg = CONFIG_MMWAVE_RADAR_YAW_MDEG;
#ifdef CONFIG_MMWAVE_RADAR_INVERT_RAW_X
    config->invert_raw_x = true;
#endif
#ifdef CONFIG_MMWAVE_INITIAL_MODE_CALIBRATION
    config->mode = MEASUREMENT_MODE_CALIBRATION;
#else
    config->mode = MEASUREMENT_MODE_REFERENCE;
#endif

    nvs_handle_t handle;
    if (nvs_open(CONFIG_NAMESPACE, NVS_READONLY, &handle) == ESP_OK) {
        uint8_t persisted_mode;
        if (nvs_get_u8(handle, MODE_KEY, &persisted_mode) == ESP_OK &&
            persisted_mode <= MEASUREMENT_MODE_REFERENCE) {
            config->mode = (measurement_mode_t)persisted_mode;
        }
        int32_t persisted_origin_x;
        int32_t persisted_origin_z;
        int32_t persisted_yaw;
        uint8_t persisted_invert_x;
        if (nvs_get_i32(handle, ORIGIN_X_KEY, &persisted_origin_x) == ESP_OK &&
            nvs_get_i32(handle, ORIGIN_Z_KEY, &persisted_origin_z) == ESP_OK &&
            nvs_get_i32(handle, YAW_KEY, &persisted_yaw) == ESP_OK &&
            nvs_get_u8(handle, INVERT_X_KEY, &persisted_invert_x) == ESP_OK &&
            persisted_invert_x <= 1 &&
            app_config_transform_valid(persisted_origin_x, persisted_origin_z,
                                       persisted_yaw)) {
            config->origin_x_mm = persisted_origin_x;
            config->origin_z_mm = persisted_origin_z;
            config->yaw_mdeg = persisted_yaw;
            config->invert_raw_x = persisted_invert_x != 0;
        }
        nvs_close(handle);
    }
    return config->wifi_ssid[0] != '\0' && config->target_host[0] != '\0' &&
           public_identifier_valid(config->node_id);
}

bool app_config_transform_valid(int32_t origin_x_mm,
                                int32_t origin_z_mm,
                                int32_t yaw_mdeg)
{
    return origin_x_mm >= -MAX_ROOM_COORDINATE_MM &&
           origin_x_mm <= MAX_ROOM_COORDINATE_MM &&
           origin_z_mm >= -MAX_ROOM_COORDINATE_MM &&
           origin_z_mm <= MAX_ROOM_COORDINATE_MM &&
           yaw_mdeg >= -MAX_ABS_YAW_MDEG && yaw_mdeg <= MAX_ABS_YAW_MDEG;
}

bool app_config_set_transform(app_config_t *config,
                              int32_t origin_x_mm,
                              int32_t origin_z_mm,
                              int32_t yaw_mdeg,
                              bool invert_raw_x)
{
    if (config == NULL ||
        !app_config_transform_valid(origin_x_mm, origin_z_mm, yaw_mdeg)) {
        return false;
    }
    nvs_handle_t handle;
    if (nvs_open(CONFIG_NAMESPACE, NVS_READWRITE, &handle) != ESP_OK) {
        return false;
    }
    bool success = nvs_set_i32(handle, ORIGIN_X_KEY, origin_x_mm) == ESP_OK &&
                   nvs_set_i32(handle, ORIGIN_Z_KEY, origin_z_mm) == ESP_OK &&
                   nvs_set_i32(handle, YAW_KEY, yaw_mdeg) == ESP_OK &&
                   nvs_set_u8(handle, INVERT_X_KEY, invert_raw_x ? 1 : 0) == ESP_OK &&
                   nvs_commit(handle) == ESP_OK;
    nvs_close(handle);
    if (success) {
        taskENTER_CRITICAL(&s_config_lock);
        config->origin_x_mm = origin_x_mm;
        config->origin_z_mm = origin_z_mm;
        config->yaw_mdeg = yaw_mdeg;
        config->invert_raw_x = invert_raw_x;
        taskEXIT_CRITICAL(&s_config_lock);
    }
    return success;
}

void app_config_snapshot(const app_config_t *config, app_config_t *snapshot)
{
    taskENTER_CRITICAL(&s_config_lock);
    *snapshot = *config;
    taskEXIT_CRITICAL(&s_config_lock);
}

bool app_config_set_mode(app_config_t *config, measurement_mode_t mode)
{
    if (config == NULL) {
        return false;
    }
    nvs_handle_t handle;
    if (nvs_open(CONFIG_NAMESPACE, NVS_READWRITE, &handle) != ESP_OK) {
        return false;
    }
    bool success = nvs_set_u8(handle, MODE_KEY, (uint8_t)mode) == ESP_OK &&
                   nvs_commit(handle) == ESP_OK;
    nvs_close(handle);
    if (success) {
        taskENTER_CRITICAL(&s_config_lock);
        config->mode = mode;
        taskEXIT_CRITICAL(&s_config_lock);
    }
    return success;
}

const char *measurement_mode_name(measurement_mode_t mode)
{
    return mode == MEASUREMENT_MODE_REFERENCE ? "reference" : "calibration";
}

bool measurement_mode_parse(const char *value, measurement_mode_t *mode)
{
    if (strcmp(value, "calibration") == 0) {
        *mode = MEASUREMENT_MODE_CALIBRATION;
        return true;
    }
    if (strcmp(value, "reference") == 0) {
        *mode = MEASUREMENT_MODE_REFERENCE;
        return true;
    }
    return false;
}

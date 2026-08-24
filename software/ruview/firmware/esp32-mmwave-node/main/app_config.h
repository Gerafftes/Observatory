#pragma once

#include <stdbool.h>
#include <stdint.h>

typedef enum {
    MEASUREMENT_MODE_CALIBRATION,
    MEASUREMENT_MODE_REFERENCE,
} measurement_mode_t;

typedef struct {
    char wifi_ssid[33];
    char wifi_password[65];
    char target_host[16];
    uint16_t target_port;
    char node_id[24];
    char ota_psk[65];
    int32_t origin_x_mm;
    int32_t origin_z_mm;
    int32_t yaw_mdeg;
    bool invert_raw_x;
    measurement_mode_t mode;
} app_config_t;

bool app_config_load(app_config_t *config);
void app_config_snapshot(const app_config_t *config, app_config_t *snapshot);
bool app_config_set_mode(app_config_t *config, measurement_mode_t mode);
bool app_config_transform_valid(int32_t origin_x_mm,
                                int32_t origin_z_mm,
                                int32_t yaw_mdeg);
bool app_config_set_transform(app_config_t *config,
                              int32_t origin_x_mm,
                              int32_t origin_z_mm,
                              int32_t yaw_mdeg,
                              bool invert_raw_x);
const char *measurement_mode_name(measurement_mode_t mode);
bool measurement_mode_parse(const char *value, measurement_mode_t *mode);

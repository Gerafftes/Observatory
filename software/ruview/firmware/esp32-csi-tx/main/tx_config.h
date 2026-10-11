#ifndef CSI_TX_CONFIG_H
#define CSI_TX_CONFIG_H

#include <stdbool.h>
#include <stdint.h>
#include "esp_err.h"

#define TX_CONFIG_SCHEMA 1

typedef struct {
    char node_id[8];
    char wifi_ssid[33];
    char wifi_password[65];
    uint8_t channel;
    uint16_t rate_hz;
    uint8_t schema_version;
    uint8_t base_mac[6];
    uint8_t ap_mac[6];
    uint8_t bandwidth_mhz;
    uint8_t protocol_mask;
    uint8_t secondary_channel;
    uint16_t beacon_interval;
    char country[3];
    uint8_t country_channels;
    char ota_psk[65];
    int8_t led_gpio;
    bool led_active_low;
} tx_config_t;

esp_err_t tx_config_load(tx_config_t *config);
esp_err_t tx_config_init(tx_config_t *config);
bool tx_config_validate(const tx_config_t *config);

#endif

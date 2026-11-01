#ifndef RX_SETTINGS_H
#define RX_SETTINGS_H
#include "esp_err.h"
#include "nvs_config.h"
#include "rx_profile.h"

#define RX_SETTINGS_BYTES 192u

bool rx_settings_valid(const nvs_config_t *config, const rx_profile_t *profile);
bool rx_settings_encode(const nvs_config_t *config, const rx_profile_t *profile,
                        uint8_t out[RX_SETTINGS_BYTES]);
bool rx_settings_decode(const uint8_t *bytes, size_t length,
                        nvs_config_t *config, rx_profile_t *profile);
/* Read legacy keys without writing; a present invalid blob never falls back. */
esp_err_t rx_settings_load(nvs_config_t *config, rx_profile_t *profile);
/* One NVS value, preserving every unrelated key and all authentication state. */
esp_err_t rx_settings_store(const nvs_config_t *config, const rx_profile_t *profile);
/* Public measurement identity excludes password bytes and its storage CRC.
 * Authorized changes, including credentials, advance the profile generation. */
bool rx_settings_hash(const nvs_config_t *config, const rx_profile_t *profile, uint8_t out[32]);
void rx_settings_activate(const nvs_config_t *config, const rx_profile_t *profile);
const rx_profile_t *rx_settings_active_profile(void);
const nvs_config_t *rx_settings_active_config(void);
const uint8_t *rx_settings_active_hash(void);
#endif

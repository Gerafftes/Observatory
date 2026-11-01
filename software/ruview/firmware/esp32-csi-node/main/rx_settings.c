#include "rx_settings.h"
#include "nvs.h"
#include "mbedtls/sha256.h"
#include <string.h>

static nvs_config_t active_config;
static rx_profile_t active_profile;
static uint8_t active_hash[32];

esp_err_t rx_settings_load(nvs_config_t *config, rx_profile_t *profile)
{
    nvs_handle_t handle;
    esp_err_t err = nvs_open("csi_cfg", NVS_READONLY, &handle);
    if (err != ESP_OK) return err;
    uint8_t raw[RX_SETTINGS_BYTES]; size_t size = sizeof(raw);
    err = nvs_get_blob(handle, "rx_settings", raw, &size);
    if (err == ESP_ERR_NVS_NOT_FOUND) {
        /* Migration is read-only and requires an actual provisioned identity. */
        rx_profile_t p = {.generation = 1, .led_gpio = 48, .rate_hz = 50, .mode = RX_BASELINE};
        err = nvs_get_u8(handle, "node_id", &p.node_id);
        if (err == ESP_OK) {
            esp_err_t board = nvs_get_u8(handle, "led_gpio", &p.led_gpio);
            /* Legacy S3 images used GPIO48. Preserve that documented default;
             * board verification and an explicit override precede qualification. */
            if (board != ESP_OK && board != ESP_ERR_NVS_NOT_FOUND) err = board;
        }
        if (err == ESP_OK) {
            memcpy(p.tx_mac, config->filter_mac, 6);
            if (!rx_settings_valid(config, &p)) err = ESP_ERR_INVALID_STATE;
            else *profile = p;
        }
    } else if (err == ESP_OK && !rx_settings_decode(raw, size, config, profile)) {
        err = ESP_ERR_INVALID_STATE;
    }
    nvs_close(handle);
    return err;
}

esp_err_t rx_settings_store(const nvs_config_t *config, const rx_profile_t *profile)
{
    uint8_t raw[RX_SETTINGS_BYTES];
    if (!rx_settings_encode(config, profile, raw)) return ESP_ERR_INVALID_ARG;
    nvs_handle_t handle;
    esp_err_t err = nvs_open("csi_cfg", NVS_READWRITE, &handle);
    if (err != ESP_OK) return err;
    err = nvs_set_blob(handle, "rx_settings", raw, sizeof(raw));
    if (err == ESP_OK) err = nvs_commit(handle);
    nvs_close(handle);
    if (err != ESP_OK) return err;
    nvs_config_t check = *config; rx_profile_t readback;
    err = rx_settings_load(&check, &readback);
    uint8_t observed[RX_SETTINGS_BYTES];
    if (err == ESP_OK && (!rx_settings_encode(&check, &readback, observed)
        || memcmp(raw, observed, sizeof(raw)))) err = ESP_FAIL;
    return err;
}

bool rx_settings_hash(const nvs_config_t *config, const rx_profile_t *profile, uint8_t out[32])
{
    uint8_t raw[RX_SETTINGS_BYTES];
    if (!out || !rx_settings_encode(config, profile, raw)) return false;
    /* Neither a password nor its storage CRC belongs on the measurement wire. */
    memset(raw + 97, 0, 65);
    bool valid = mbedtls_sha256(raw, 188, out, 0) == 0;
    memset(raw, 0, sizeof(raw));
    return valid;
}

void rx_settings_activate(const nvs_config_t *config, const rx_profile_t *profile)
{
    active_config = *config; active_profile = *profile;
    if (!rx_settings_hash(config, profile, active_hash)) {
        memset(&active_profile, 0, sizeof(active_profile));
        memset(active_hash, 0, sizeof(active_hash));
    }
}

const rx_profile_t *rx_settings_active_profile(void) { return &active_profile; }
const nvs_config_t *rx_settings_active_config(void) { return &active_config; }
const uint8_t *rx_settings_active_hash(void) { return active_hash; }

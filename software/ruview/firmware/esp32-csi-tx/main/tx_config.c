#include "tx_config.h"
#include "tx_safety.h"
#include "tx_diag.h"

#include <string.h>
#include "esp_partition.h"
#include "nvs.h"
#include "nvs_flash.h"
#include "esp_timer.h"

static esp_partition_t s_readonly_nvs;

static esp_err_t read_string(nvs_handle_t handle, const char *key, char *target, size_t size)
{
    size_t required = size;
    esp_err_t err = nvs_get_str(handle, key, target, &required);
    if (err != ESP_OK) return err;
    return required > 0 && required <= size && target[required - 1] == '\0'
               && strnlen(target, required) + 1 == required
               ? ESP_OK : ESP_ERR_INVALID_ARG;
}

static esp_err_t read_mac(nvs_handle_t handle, const char *key, uint8_t mac[6])
{
    size_t size = 6;
    esp_err_t err = nvs_get_blob(handle, key, mac, &size);
    return err != ESP_OK ? err : (size == 6 ? ESP_OK : ESP_ERR_INVALID_ARG);
}

esp_err_t tx_config_load(tx_config_t *config)
{
    memset(config, 0, sizeof(*config));
    config->led_gpio = 48;
    nvs_handle_t handle;
    esp_err_t err = nvs_open("tx_cfg", NVS_READONLY, &handle);
    if (err != ESP_OK) { TX_ERROR("PROFILE_OPEN", err); return err; }
    uint8_t led_gpio = 48, active_low = 0;
#define READ(call) do { \
    TX_CHECKPOINT("BEGIN " #call); \
    err = (call); \
    if (err != ESP_OK) { TX_ERROR(#call, err); goto close; } \
} while (0)
    READ(nvs_get_u8(handle, "schema_v", &config->schema_version));
    READ(read_string(handle, "node_id", config->node_id, sizeof(config->node_id)));
    READ(read_string(handle, "ssid", config->wifi_ssid, sizeof(config->wifi_ssid)));
    READ(read_string(handle, "password", config->wifi_password, sizeof(config->wifi_password)));
    READ(read_mac(handle, "base_mac", config->base_mac));
    READ(read_mac(handle, "ap_mac", config->ap_mac));
    READ(nvs_get_u8(handle, "channel", &config->channel));
    READ(nvs_get_u16(handle, "rate_hz", &config->rate_hz));
    READ(nvs_get_u8(handle, "bw_mhz", &config->bandwidth_mhz));
    READ(nvs_get_u8(handle, "protocol", &config->protocol_mask));
    READ(nvs_get_u8(handle, "secondary", &config->secondary_channel));
    READ(nvs_get_u16(handle, "beacon_tu", &config->beacon_interval));
    READ(read_string(handle, "country", config->country, sizeof(config->country)));
    READ(nvs_get_u8(handle, "country_nch", &config->country_channels));
    READ(nvs_get_u8(handle, "led_gpio", &led_gpio));
    READ(nvs_get_u8(handle, "led_active_low", &active_low));
    if ((led_gpio > 48 && led_gpio != UINT8_MAX) || active_low > 1) {
        err = ESP_ERR_INVALID_ARG;
        TX_ERROR("PROFILE_LED_VALUE", err);
        goto close;
    }
    config->led_gpio = led_gpio == UINT8_MAX ? -1 : (int8_t)led_gpio;
    config->led_active_low = active_low != 0;
close:
    nvs_close(handle);
    if (err != ESP_OK) return err;
    err = nvs_open("security", NVS_READONLY, &handle);
    if (err != ESP_OK) { TX_ERROR("SECURITY_OPEN", err); return err; }
    err = read_string(handle, "ota_psk", config->ota_psk, sizeof(config->ota_psk));
    nvs_close(handle);
#undef READ
    if (err != ESP_OK) { TX_ERROR("SECURITY_READ", err); return err; }
    if (!tx_config_validate(config)) {
        TX_ERROR("PROFILE_VALIDATE", ESP_ERR_INVALID_ARG);
        return ESP_ERR_INVALID_ARG;
    }
    return ESP_OK;
}

static bool unicast_mac(const uint8_t mac[6])
{
    static const uint8_t zero[6] = {0};
    return !(mac[0] & 1u) && memcmp(mac, zero, 6) != 0;
}

bool tx_config_validate(const tx_config_t *c)
{
    size_t ssid = strnlen(c->wifi_ssid, sizeof(c->wifi_ssid));
    size_t password = strnlen(c->wifi_password, sizeof(c->wifi_password));
    const int64_t sampled_us = esp_timer_get_time();
#define PROFILE_REQUIRE(field, rule, actual, condition) do { \
    if (!(condition)) { \
        ESP_LOGE("tx_boot", "TX_COMPARE op=tx_config_validate rc=0x%x field=%s expected=%s actual=%lld boot_us=%lld phase=before_wifi", \
                 (unsigned)ESP_ERR_INVALID_ARG, field, rule, (long long)(actual), (long long)sampled_us); \
        return false; \
    } \
} while (0)
    PROFILE_REQUIRE("schema_version", "1", c->schema_version, c->schema_version == TX_CONFIG_SCHEMA);
    PROFILE_REQUIRE("node_id_matches", "1", 0, memcmp(c->node_id, "TX1\0", 4) == 0);
    PROFILE_REQUIRE("ssid_length", "1..32 bytes", ssid, ssid > 0 && ssid <= 32);
    PROFILE_REQUIRE("password_length", "8..63 bytes", password, password >= 8 && password <= 63);
    PROFILE_REQUIRE("base_mac_unicast_nonzero", "1", 0, unicast_mac(c->base_mac));
    PROFILE_REQUIRE("ap_mac_unicast_nonzero", "1", 0, unicast_mac(c->ap_mac));
    PROFILE_REQUIRE("channel", "1..13", c->channel, c->channel >= 1 && c->channel <= 13);
    PROFILE_REQUIRE("rate_hz", "1..100", c->rate_hz, c->rate_hz >= 1 && c->rate_hz <= 100);
    PROFILE_REQUIRE("bandwidth_mhz", "20 or 40", c->bandwidth_mhz, c->bandwidth_mhz == 20 || c->bandwidth_mhz == 40);
    PROFILE_REQUIRE("protocol_mask", "nonzero b/g/n mask (1..7)", c->protocol_mask, c->protocol_mask && !(c->protocol_mask & ~7u));
    PROFILE_REQUIRE("secondary_channel", "0..2", c->secondary_channel, c->secondary_channel <= 2);
    PROFILE_REQUIRE("secondary_for_bandwidth", "NONE for HT20; ABOVE/BELOW for HT40", c->secondary_channel,
                    (c->bandwidth_mhz == 20 && c->secondary_channel == 0) || (c->bandwidth_mhz == 40 && c->secondary_channel != 0));
    PROFILE_REQUIRE("ht40_protocol_11n", "bit 0x04 when HT40", c->protocol_mask, c->bandwidth_mhz != 40 || (c->protocol_mask & 4));
    PROFILE_REQUIRE("beacon_interval_TU", "100..60000, multiple of 100 (1 TU=1024 us)", c->beacon_interval,
                    c->beacon_interval >= 100 && c->beacon_interval <= 60000 && c->beacon_interval % 100 == 0);
    PROFILE_REQUIRE("country[0]", "A..Z", (unsigned char)c->country[0], c->country[0] >= 'A' && c->country[0] <= 'Z');
    PROFILE_REQUIRE("country[1]", "A..Z", (unsigned char)c->country[1], c->country[1] >= 'A' && c->country[1] <= 'Z');
    /* Schema-1 alpha-2 C string; SDK environment semantics are applied at the
       Wi-Fi boundary. This does not add an I/O/X setting to the profile. */
    PROFILE_REQUIRE("country[2]_terminator", "0", (unsigned char)c->country[2], c->country[2] == '\0');
    PROFILE_REQUIRE("country_channels", "channel..13", c->country_channels, c->country_channels >= c->channel && c->country_channels <= 13);
    PROFILE_REQUIRE("secondary_above_range", "primary+4 <= country_channels", c->channel + 4,
                    c->secondary_channel != 1 || c->channel + 4 <= c->country_channels);
    PROFILE_REQUIRE("secondary_below_range", "primary >= 5", c->channel, c->secondary_channel != 2 || c->channel >= 5);
    PROFILE_REQUIRE("led_gpio", "-1..48", c->led_gpio, c->led_gpio >= -1 && c->led_gpio <= 48);
    size_t secret_len = strnlen(c->ota_psk, sizeof(c->ota_psk));
    PROFILE_REQUIRE("ota_psk_length", "64 bytes (redacted)", secret_len, secret_len == 64);
    for (size_t i = 0; i < 64; ++i) {
        PROFILE_REQUIRE("ota_psk_lowercase_hex_valid_redacted", "1", 0,
                        (c->ota_psk[i] >= '0' && c->ota_psk[i] <= '9') || (c->ota_psk[i] >= 'a' && c->ota_psk[i] <= 'f'));
    }
#undef PROFILE_REQUIRE
    return true;
}

esp_err_t tx_config_init(tx_config_t *config)
{
    const esp_partition_t *partition = esp_partition_find_first(
        ESP_PARTITION_TYPE_DATA, ESP_PARTITION_SUBTYPE_DATA_NVS, "nvs");
    if (partition == NULL || partition->address != 0x9000 || partition->size != 0x5000) {
        ESP_LOGE("tx_boot", "TX_COMPARE op=esp_partition_find_first rc=0x%x field=nvs_layout expected_address=0x9000 expected_size=0x5000 actual_address=%lld actual_size=%lld unavailable=-1 boot_us=%lld phase=before_nvs_init",
                 (unsigned)ESP_ERR_INVALID_STATE, partition ? (long long)partition->address : -1,
                 partition ? (long long)partition->size : -1, (long long)esp_timer_get_time());
        TX_ERROR("NVS_LAYOUT", ESP_ERR_INVALID_STATE);
        return ESP_ERR_INVALID_STATE;
    }
    /* IDF rejects writes/erases even during NVS recovery. Keep the descriptor
       alive for the mount; this changes neither the table nor flash data. */
    s_readonly_nvs = *partition;
    s_readonly_nvs.readonly = true;
    TX_STEP(nvs_flash_init_partition_ptr(&s_readonly_nvs));
    TX_CHECKPOINT("NVS_OK");
    return tx_config_load(config);
}

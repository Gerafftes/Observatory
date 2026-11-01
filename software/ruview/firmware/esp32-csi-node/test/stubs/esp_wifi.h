/* Stub: redirect to unified stubs header. */
#ifndef ESP_WIFI_H_STUB
#define ESP_WIFI_H_STUB
#include "esp_stubs.h"
#ifdef RX_WIFI_TEST
typedef struct { int unused; } wifi_init_config_t;
#define WIFI_INIT_CONFIG_DEFAULT() ((wifi_init_config_t){0})
typedef struct { struct { uint8_t ssid[32], password[64]; struct { int authmode; } threshold; } sta; } wifi_config_t;
#define WIFI_AUTH_OPEN 0
#define WIFI_AUTH_WPA2_PSK 2
#define WIFI_MODE_STA 1
#define WIFI_BW_HT20 0
esp_err_t esp_wifi_init(const wifi_init_config_t *);
esp_err_t esp_wifi_deinit(void);
esp_err_t esp_wifi_set_mode(int);
esp_err_t esp_wifi_set_config(int, const wifi_config_t *);
esp_err_t esp_wifi_set_bandwidth(int, int);
esp_err_t esp_wifi_start(void);
esp_err_t esp_wifi_stop(void);
esp_err_t esp_wifi_connect(void);
#endif
#endif

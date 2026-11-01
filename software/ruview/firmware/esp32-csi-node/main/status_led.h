#pragma once
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include "esp_err.h"

typedef enum {
    LED_START, LED_NORMAL, LED_WIFI, LED_STALE, LED_INIT,
    LED_TRANSPORT, LED_OTA, LED_INTERNAL
} status_led_code_t;
typedef struct {
    status_led_code_t code, displayed;
    int64_t sample_us, since_us, last_csi_us;
    uint32_t qualified_frames;
    bool driver_ok, testing;
    const char *reason;
} status_led_snapshot_t;

esp_err_t status_led_start(uint8_t node);
void status_led_network(bool ready);
void status_led_csi(int64_t now_us);
void status_led_init_failed(void);
void status_led_internal_failed(void);
void status_led_ota(bool active, bool failed);
bool status_led_test(void);
void status_led_snapshot(status_led_snapshot_t *out);
const char *status_led_name(status_led_code_t code);

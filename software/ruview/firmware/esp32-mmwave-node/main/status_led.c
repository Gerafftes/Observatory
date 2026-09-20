#include "status_led.h"

#include <stdbool.h>

#include "driver/gpio.h"
#include "esp_err.h"
#include "esp_log.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"

#define STATUS_LED_GPIO GPIO_NUM_8
#define STATUS_LED_ON_LEVEL 0
#define STATUS_LED_OFF_LEVEL 1
#define STATUS_LED_PULSE_MS 150
#define STATUS_LED_PULSE_GAP_MS 150
#define STATUS_LED_CYCLE_MS 3000

static const char *TAG = "status_led";
static uint8_t s_identity_pulse_count;

static void status_led_set(bool enabled)
{
    gpio_set_level(STATUS_LED_GPIO,
                   enabled ? STATUS_LED_ON_LEVEL : STATUS_LED_OFF_LEVEL);
}

static void status_led_task(void *argument)
{
    (void)argument;
    while (true) {
        if (s_identity_pulse_count == 0) {
            status_led_set(true);
            vTaskDelay(pdMS_TO_TICKS(STATUS_LED_PULSE_MS));
            status_led_set(false);
            vTaskDelay(pdMS_TO_TICKS(STATUS_LED_PULSE_GAP_MS));
            continue;
        }

        for (uint8_t pulse = 0; pulse < s_identity_pulse_count; ++pulse) {
            status_led_set(true);
            vTaskDelay(pdMS_TO_TICKS(STATUS_LED_PULSE_MS));
            status_led_set(false);
            if (pulse + 1 < s_identity_pulse_count) {
                vTaskDelay(pdMS_TO_TICKS(STATUS_LED_PULSE_GAP_MS));
            }
        }

        uint32_t active_ms =
            (uint32_t)s_identity_pulse_count * STATUS_LED_PULSE_MS +
            (uint32_t)(s_identity_pulse_count - 1) * STATUS_LED_PULSE_GAP_MS;
        vTaskDelay(pdMS_TO_TICKS(STATUS_LED_CYCLE_MS - active_ms));
    }
}

void status_led_start(const char *node_id)
{
    s_identity_pulse_count = status_led_identity_pulse_count(node_id);
    const gpio_config_t output = {
        .pin_bit_mask = 1ULL << STATUS_LED_GPIO,
        .mode = GPIO_MODE_OUTPUT,
        .pull_up_en = GPIO_PULLUP_DISABLE,
        .pull_down_en = GPIO_PULLDOWN_DISABLE,
        .intr_type = GPIO_INTR_DISABLE,
    };
    esp_err_t error = gpio_config(&output);
    if (error != ESP_OK) {
        ESP_LOGE(TAG, "Cannot configure GPIO 8 identity LED: %s",
                 esp_err_to_name(error));
        return;
    }
    status_led_set(false);

    if (xTaskCreate(status_led_task, "identity_led", 2048, NULL, 1, NULL) !=
        pdPASS) {
        ESP_LOGE(TAG, "Cannot start identity LED task");
        return;
    }

    if (s_identity_pulse_count == 0) {
        ESP_LOGW(TAG, "Unknown node ID '%s'; using warning blink",
                 node_id != NULL ? node_id : "");
    } else {
        ESP_LOGI(TAG, "%s uses %u identity pulse%s per 3-second cycle",
                 node_id, s_identity_pulse_count,
                 s_identity_pulse_count == 1 ? "" : "s");
    }
}

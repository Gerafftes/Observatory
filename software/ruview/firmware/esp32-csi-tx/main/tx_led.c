#include "tx_led.h"
#include "tx_diag.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "led_strip.h"

static portMUX_TYPE s_lock = portMUX_INITIALIZER_UNLOCKED;
static bool s_done;
static esp_err_t s_result;

static esp_err_t show_identity(const tx_config_t *config)
{
    led_strip_handle_t strip = NULL;
    led_strip_config_t strip_config = {
        .strip_gpio_num = config->led_gpio, .max_leds = 1,
        .led_model = LED_MODEL_WS2812,
        .color_component_format = LED_STRIP_COLOR_COMPONENT_FMT_GRB,
        .flags.invert_out = config->led_active_low,
    };
    led_strip_rmt_config_t rmt_config = {
        .resolution_hz = 10 * 1000 * 1000, .flags.with_dma = false,
    };
    TX_STEP(led_strip_new_rmt_device(&strip_config, &rmt_config, &strip));
    /* #ff9f0a scaled by 32/255, rounded: unchanged dim amber/GRB. */
    TX_STEP(led_strip_set_pixel(strip, 0, 32, 20, 1));
    TX_STEP(led_strip_refresh(strip));
    TX_CHECKPOINT("LED_OK");
    return ESP_OK;
}

static void led_task(void *argument)
{
    esp_err_t result = show_identity(argument);
    portENTER_CRITICAL(&s_lock);
    s_result = result;
    s_done = true;
    portEXIT_CRITICAL(&s_lock);
    vTaskDelete(NULL);
}

esp_err_t tx_led_start(const tx_config_t *config)
{
    if (config->led_gpio < 0) {
        TX_CHECKPOINT("LED_DISABLED");
        return ESP_OK;
    }
    TX_CHECKPOINT("BEGIN LED");
    portENTER_CRITICAL(&s_lock);
    s_done = false;
    portEXIT_CRITICAL(&s_lock);
    if (xTaskCreate(led_task, "tx_led", 3072, (void *)config, 1, NULL) != pdPASS) {
        TX_ERROR("LED_TASK", ESP_ERR_NO_MEM);
        return ESP_ERR_NO_MEM;
    }
    const TickType_t started = xTaskGetTickCount();
    do {
        portENTER_CRITICAL(&s_lock);
        bool done = s_done;
        esp_err_t result = s_result;
        portEXIT_CRITICAL(&s_lock);
        if (done) return result;
        vTaskDelay(pdMS_TO_TICKS(10));
    } while ((TickType_t)(xTaskGetTickCount() - started) < pdMS_TO_TICKS(1000));
    /* Do not delete a stuck driver task: it may own driver locks/resources.
       One bounded worker remains; AP/status/sounding continue independently. */
    TX_ERROR("LED_TIMEOUT", ESP_ERR_TIMEOUT);
    return ESP_ERR_TIMEOUT;
}

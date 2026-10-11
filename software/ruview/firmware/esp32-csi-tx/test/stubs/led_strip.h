#include "idf_mock.h"
typedef void *led_strip_handle_t;
typedef struct {
 int strip_gpio_num, max_leds, led_model, color_component_format;
 struct { bool invert_out; } flags;
} led_strip_config_t;
typedef struct { unsigned resolution_hz; struct { bool with_dma; } flags; } led_strip_rmt_config_t;
#define LED_MODEL_WS2812 1
#define LED_STRIP_COLOR_COMPONENT_FMT_GRB 1
esp_err_t led_strip_new_rmt_device(const led_strip_config_t*, const led_strip_rmt_config_t*, led_strip_handle_t*);
esp_err_t led_strip_set_pixel(led_strip_handle_t, unsigned, unsigned, unsigned, unsigned);
esp_err_t led_strip_refresh(led_strip_handle_t);

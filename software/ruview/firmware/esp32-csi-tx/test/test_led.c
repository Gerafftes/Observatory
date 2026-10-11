/* Run the real worker against fake IDF boundaries, including a stuck refresh. */
#include <assert.h>
#include <setjmp.h>
#include <stdarg.h>
#include "tx_led.h"
#include "led_strip.h"
#include "freertos/task.h"

static TickType_t ticks;
static int task_result = pdPASS, fail_step, step;
static bool stuck, enabled, saw_ok, saw_timeout, saw_error, task_deleted;
static jmp_buf worker_yield;

void mock_log(const char *format, ...)
{
    char line[512];
    va_list args;va_start(args,format);vsnprintf(line,sizeof(line),format,args);va_end(args);
    if (strstr(line,"TX_BOOT LED_OK")) saw_ok = true;
    if (strstr(line,"LED_TIMEOUT")) saw_timeout = true;
    if (strstr(line,"TX_BOOT ERROR")) {
        assert(strstr(line,"code=0x"));saw_error = true;
    }
}
int xTaskCreate(void (*fn)(void*), const char *name, unsigned stack, void *arg,
                unsigned priority, TaskHandle_t *handle)
{
    (void)name;assert(stack>=3072 && priority==1 && handle==NULL);
    if (task_result != pdPASS) return task_result;
    enabled = true;
    if (setjmp(worker_yield) == 0) fn(arg);
    return pdPASS;
}
void vTaskDelete(TaskHandle_t task){assert(task==NULL);task_deleted=true;}
TickType_t xTaskGetTickCount(void){return ticks;}
void vTaskDelay(unsigned delay){ticks+=delay;}
esp_err_t led_strip_new_rmt_device(const led_strip_config_t *c,
                                  const led_strip_rmt_config_t *r, led_strip_handle_t *h)
{
    assert(c->strip_gpio_num==48 && c->max_leds==1 && !c->flags.invert_out);
    assert(c->led_model==LED_MODEL_WS2812 && c->color_component_format==LED_STRIP_COLOR_COMPONENT_FMT_GRB);
    assert(r->resolution_hz==10000000 && !r->flags.with_dma);
    *h=(void*)1;return ++step==fail_step?ESP_FAIL:ESP_OK;
}
esp_err_t led_strip_set_pixel(led_strip_handle_t h, unsigned i, unsigned r, unsigned g, unsigned b)
{
    assert(h && i==0 && r==32 && g==20 && b==1);
    return ++step==fail_step?ESP_FAIL:ESP_OK;
}
esp_err_t led_strip_refresh(led_strip_handle_t h)
{
    assert(h);
    if (stuck) longjmp(worker_yield,1); /* worker never completes */
    return ++step==fail_step?ESP_FAIL:ESP_OK;
}
static void reset(void)
{
    ticks=0;step=0;enabled=false;saw_ok=false;saw_timeout=false;saw_error=false;task_deleted=false;
}
int main(void)
{
    tx_config_t config={.led_gpio=-1};
    assert(tx_led_start(&config)==ESP_OK && !enabled && !saw_ok);
    config.led_gpio=48;
    assert(tx_led_start(&config)==ESP_OK && enabled && saw_ok && task_deleted);
    for (fail_step=1;fail_step<=3;fail_step++) {
        reset();assert(tx_led_start(&config)==ESP_FAIL && saw_error && !saw_ok && task_deleted);
    }
    fail_step=0;reset();task_result=0;
    assert(tx_led_start(&config)==ESP_ERR_NO_MEM && saw_error && !enabled);
    task_result=pdPASS;reset();stuck=true;
    assert(tx_led_start(&config)==ESP_ERR_TIMEOUT && ticks==1000 && saw_timeout && !saw_ok && !task_deleted);
    reset();ticks=UINT32_MAX-499;TickType_t start=ticks;
    assert(tx_led_start(&config)==ESP_ERR_TIMEOUT && (TickType_t)(ticks-start)==1000 && saw_timeout);
    puts("TX LED bounded-worker tests passed");
    return 0;
}

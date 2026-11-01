#include "status_led.h"
#include <string.h>
#include "sdkconfig.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "esp_timer.h"
#include "esp_log.h"
#include "led_strip.h"
#include "node_identity.h"
#include "csi_transport.h"
#include "rx_diag.h"
#include "rx_settings.h"
#include "esp_heap_caps.h"

#define START_US 10000000LL
#define STALE_US 3000000LL
#define RECOVER_US 2000000LL
#define TEST_STEP_US 4800000LL
static const char *TAG = "status_led";
static portMUX_TYPE s_lock = portMUX_INITIALIZER_UNLOCKED;
static struct {
    int64_t started, ip_since, last_csi, recovering, since, test_since;
    uint32_t frames;
    bool network, init_failed, internal, ota, ota_failed, healthy, driver_ok, testing;
    status_led_code_t code;
} s;
static led_strip_handle_t s_strip;
static node_identity_t s_identity;

const char *status_led_name(status_led_code_t code)
{
    static const char *names[] = {"START", "NORMAL", "WIFI_IP", "TX1_STALE",
        "CSI_PEER_INIT", "LOCAL_TRANSPORT", "OTA", "INTERNAL"};
    return (unsigned)code < sizeof(names)/sizeof(names[0]) ? names[code] : "UNKNOWN";
}

void status_led_network(bool ready)
{
    int64_t now = esp_timer_get_time();
    portENTER_CRITICAL(&s_lock);
    if (ready != s.network) {
        s.network = ready; s.ip_since = now;
        s.last_csi = 0; s.recovering = 0; s.healthy = false;
    }
    portEXIT_CRITICAL(&s_lock);
}

/* Called only after the collector's source, input and physical-grid guards. */
void status_led_csi(int64_t now)
{
    portENTER_CRITICAL(&s_lock);
    if (s.network && now > 0 && now >= s.last_csi) {
        if (!s.last_csi || now - s.last_csi >= STALE_US) {
            s.recovering = now; s.healthy = false;
        }
        s.last_csi = now;
        if (s.frames != UINT32_MAX) s.frames++;
        if (now - s.recovering >= RECOVER_US) s.healthy = true;
    }
    portEXIT_CRITICAL(&s_lock);
}

void status_led_init_failed(void)
{ portENTER_CRITICAL(&s_lock); s.init_failed = true; portEXIT_CRITICAL(&s_lock); }
void status_led_internal_failed(void)
{ portENTER_CRITICAL(&s_lock); s.internal = true; portEXIT_CRITICAL(&s_lock); }
void status_led_ota(bool active, bool failed)
{
    portENTER_CRITICAL(&s_lock);
    s.ota = active; s.ota_failed = failed;
    if (active || failed) s.testing = false;
    portEXIT_CRITICAL(&s_lock);
}

/* The caller supplies sampled local failures; successful UDP is not an ACK. */
static void select_state(int64_t now, bool transport_failed, bool resources_failed)
{
    status_led_code_t next;
    if (s.internal || s.ota_failed || resources_failed) next = LED_INTERNAL;
    else if (s.ota) next = LED_OTA;
    else if (s.init_failed) next = LED_INIT;
    else if (now - s.started < START_US) next = LED_START;
    else if (!s.network) next = LED_WIFI;
    else if (now - s.ip_since < START_US && !s.healthy) next = LED_START;
    else if (!s.last_csi || now < s.last_csi || now - s.last_csi >= STALE_US || !s.healthy) next = LED_STALE;
    else if (transport_failed) next = LED_TRANSPORT;
    else next = LED_NORMAL;
    if (s.internal || s.ota_failed || resources_failed || s.ota) s.testing=false;
    if (next != s.code) { s.code = next; s.since = now; }
    if (s.testing && (now < s.test_since || now - s.test_since >= 8 * TEST_STEP_US)) s.testing = false;
}

void status_led_snapshot(status_led_snapshot_t *out)
{
    portENTER_CRITICAL(&s_lock);
    int64_t now = esp_timer_get_time();
    *out = (status_led_snapshot_t){.code=s.code, .displayed=s.code,
        .sample_us=now, .since_us=s.since, .last_csi_us=s.last_csi,
        .qualified_frames=s.frames, .driver_ok=s.driver_ok, .testing=s.testing,
        .reason=status_led_name(s.code)};
    if (s.testing && now >= s.test_since && now - s.test_since < 8 * TEST_STEP_US)
        out->displayed = (status_led_code_t)((now - s.test_since) / TEST_STEP_US);
    else out->testing = false;
    portEXIT_CRITICAL(&s_lock);
}

bool status_led_test(void)
{
    int64_t now = esp_timer_get_time();
    portENTER_CRITICAL(&s_lock);
    bool accepted = s.driver_ok && !s.testing && !s.ota && !s.ota_failed && !s.internal && !s.init_failed;
    if (accepted) { s.test_since = now; s.testing = true; }
    portEXIT_CRITICAL(&s_lock);
    if (accepted) ESP_LOGI(TAG, "LED output test: START,NORMAL,WIFI_IP,TX1_STALE,CSI_PEER_INIT,LOCAL_TRANSPORT,OTA,INTERNAL; 4800ms each");
    return accepted;
}

static bool sos_on(uint32_t ms)
{
    /* Morse unit=120ms; 1-unit element gaps, 3-unit letter gaps. */
    static const uint8_t lengths[] = {1,1,1,3,3,3,1,1,1};
    uint32_t cursor = 0;
    for (unsigned i=0; i<9; i++) {
        uint32_t end = cursor + lengths[i] * 120U;
        if (ms >= cursor && ms < end) return true;
        cursor = end + ((i==2 || i==5) ? 360U : 120U);
    }
    return false;
}

static void colour(status_led_code_t code, uint32_t ms, uint8_t rgb[3])
{
    uint8_t level = CONFIG_LED_IDENTITY_BRIGHTNESS;
    bool on = true;
    rgb[0]=rgb[1]=rgb[2]=0;
    if (code == LED_START) {
        unsigned phase = ms % 2400U;
        unsigned ramp = phase < 1200U ? phase : 2400U-phase;
        uint8_t pulse = (uint8_t)(2U + ramp*(level > 2 ? level-2U : 0U)/1200U);
        rgb[0]=rgb[1]=rgb[2]=pulse;
        return;
    }
    if (code == LED_NORMAL) {
        rgb[0]=(uint16_t)s_identity.red*level/255U;
        rgb[1]=(uint16_t)s_identity.green*level/255U;
        rgb[2]=(uint16_t)s_identity.blue*level/255U;
        return;
    }
    if (code == LED_OTA) on = ms % 240U < 120U;
    else if (code == LED_STALE) on = sos_on(ms % 4800U);
    else {
        unsigned count = code==LED_WIFI ? 2 : code==LED_INIT ? 3 : code==LED_TRANSPORT ? 4 : 5;
        unsigned phase = ms % 3000U;
        on = phase < count*240U && phase%240U < 120U;
    }
    if (!on) return;
    if (code==LED_WIFI || code==LED_STALE || code==LED_INIT || code==LED_INTERNAL) rgb[0]=level;
    if (code==LED_WIFI || code==LED_TRANSPORT) rgb[1]=level;
    if (code==LED_INIT || code==LED_TRANSPORT || code==LED_OTA) rgb[2]=level;
}

static void led_task(void *arg)
{
    (void)arg;
    uint32_t previous_errors=0, previous_sent=0;
    int64_t transport_error_until=0;
    uint8_t previous[3]={255,255,255};
    status_led_code_t logged=LED_INTERNAL;
    for (;;) {
        int64_t now=esp_timer_get_time();
        csi_transport_stats_t transport; csi_transport_get_stats(&transport);
        rx_diag_snapshot_t diag; rx_diag_snapshot(&diag);
        uint32_t errors=transport.transport_errors+transport.queue_publish_failures+transport.pool_exhausted_drops+transport.frames_abandoned;
        if (errors != previous_errors) transport_error_until=now+STALE_US;
        else if (transport.frames_os_accepted != previous_sent && now >= transport_error_until) transport_error_until=0;
        previous_errors=errors; previous_sent=transport.frames_os_accepted;
        bool bad_resources=heap_caps_get_free_size(MALLOC_CAP_INTERNAL|MALLOC_CAP_8BIT)<32768U ||
            diag.allocation_failures != 0;
        portENTER_CRITICAL(&s_lock);
        /* Sample after taking the same lock as CSI timestamps. A callback
         * during the stats reads must not appear to come from the future. */
        now=esp_timer_get_time();
        select_state(now,transport_error_until != 0,bad_resources);
        int64_t since=s.testing?s.test_since:s.since;
        portEXIT_CRITICAL(&s_lock);
        status_led_snapshot_t view;status_led_snapshot(&view);
        if (view.code != logged) {
            ESP_LOGI(TAG,"LED state=%s boot_us=%lld",view.reason,(long long)now);
            logged=view.code;
        }
        uint32_t elapsed_ms=(uint32_t)((now-since)/1000);
        if (view.testing) elapsed_ms %= (uint32_t)(TEST_STEP_US/1000);
        uint8_t rgb[3];colour(view.displayed,elapsed_ms,rgb);
        if (memcmp(rgb,previous,3)) {
            esp_err_t err=led_strip_set_pixel(s_strip,0,rgb[0],rgb[1],rgb[2]);
            if (err==ESP_OK) err=led_strip_refresh(s_strip);
            if (err != ESP_OK) {
                portENTER_CRITICAL(&s_lock);s.driver_ok=false;portEXIT_CRITICAL(&s_lock);
                ESP_LOGW(TAG,"Status LED driver failed: %s; CSI/OTA continue",esp_err_to_name(err));
                vTaskDelete(NULL);return;
            }
            memcpy(previous,rgb,3);
        }
        vTaskDelay(pdMS_TO_TICKS(50));
    }
}

esp_err_t status_led_start(uint8_t node)
{
    s=(__typeof__(s)){.started=esp_timer_get_time(),.code=LED_START};
    if (!node_identity_resolve(node,&s_identity)) return ESP_ERR_INVALID_ARG;
#if defined(CONFIG_IDF_TARGET_ESP32C6)
    const int gpio=8;
#else
    const int gpio=rx_settings_active_profile()->led_gpio;
#endif
    led_strip_config_t config={.strip_gpio_num=gpio,.max_leds=1,.led_model=LED_MODEL_WS2812,
        .color_component_format=LED_STRIP_COLOR_COMPONENT_FMT_GRB};
    led_strip_rmt_config_t rmt={.resolution_hz=10000000,.flags.with_dma=false};
    esp_err_t err=led_strip_new_rmt_device(&config,&rmt,&s_strip);
    if (err != ESP_OK) return err;
    s.driver_ok=true;
    if (xTaskCreate(led_task,"status_led",3072,NULL,1,NULL)!=pdPASS) {
        s.driver_ok=false;led_strip_del(s_strip);s_strip=NULL;return ESP_ERR_NO_MEM;
    }
    ESP_LOGI(TAG,"Status LED ready: GPIO %d, brightness=%u, stack=3072B",gpio,(unsigned)CONFIG_LED_IDENTITY_BRIGHTNESS);
    return ESP_OK;
}

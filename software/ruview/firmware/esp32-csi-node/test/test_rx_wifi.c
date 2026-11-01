/* Production reconnect/init code with deterministic SDK effects and reentrant events. */
#include "../main/rx_wifi.c"
#include <assert.h>
#include <stdio.h>

const char RX_TEST_WIFI_EVENT[]="wifi", RX_TEST_IP_EVENT[]="ip";
nvs_config_t g_nvs_config;
static unsigned call, fail_at, groups, loops, stations, drivers, timers, registrations, failures;
static unsigned connects, invalidations, starts;
static bool timer_active, connect_hook_ip, connect_hook_nested, wait_success=true, network;
static esp_err_t connect_result=ESP_OK, arm_result=ESP_OK;
static EventGroup_t group;
static bool step(void) { return ++call != fail_at; }
EventGroupHandle_t xEventGroupCreate(void) { if(!step()) return NULL; ++groups; group.bits=0; return &group; }
void vEventGroupDelete(EventGroupHandle_t h) { assert(h==&group && groups); --groups; }
EventBits_t xEventGroupSetBits(EventGroupHandle_t h, EventBits_t bits) { assert(h); return h->bits|=bits; }
EventBits_t xEventGroupClearBits(EventGroupHandle_t h, EventBits_t bits) { assert(h); return h->bits&=~bits; }
EventBits_t xEventGroupWaitBits(EventGroupHandle_t h, EventBits_t bits, bool clear, bool all, uint32_t timeout)
{
    (void)bits; (void)clear; (void)all; (void)timeout;
    if(wait_success) event_handler(NULL,IP_EVENT,IP_EVENT_STA_GOT_IP,NULL);
    else h->bits|=WIFI_FAIL_BIT;
    return h->bits;
}
esp_err_t esp_netif_init(void) { return step()?ESP_OK:ESP_FAIL; }
esp_netif_t *esp_netif_create_default_wifi_sta(void) { if(!step()) return NULL; ++stations; return (void *)3; }
void esp_netif_destroy_default_wifi(esp_netif_t *s) { assert(s==(void *)3 && stations); --stations; }
esp_err_t esp_event_loop_create_default(void) { if(!step()) return ESP_FAIL; ++loops; return ESP_OK; }
esp_err_t esp_event_loop_delete_default(void) { assert(loops); --loops; return ESP_OK; }
esp_err_t esp_event_handler_instance_register(esp_event_base_t base, int32_t id,
    void (*handler)(void *,esp_event_base_t,int32_t,void *), void *arg, esp_event_handler_instance_t *out)
{ (void)base; (void)id; (void)arg; assert(handler==event_handler); if(!step()) return ESP_FAIL; ++registrations; *out=(void *)5; return ESP_OK; }
esp_err_t esp_event_handler_instance_unregister(esp_event_base_t base, int32_t id, esp_event_handler_instance_t h)
{ (void)base; (void)id; assert(h==(void *)5 && registrations); --registrations; return ESP_OK; }
esp_err_t esp_wifi_init(const wifi_init_config_t *c) { assert(c); if(!step()) return ESP_FAIL; ++drivers; return ESP_OK; }
esp_err_t esp_wifi_deinit(void) { assert(drivers); --drivers; return ESP_OK; }
esp_err_t esp_wifi_stop(void) { return ESP_OK; }
esp_err_t esp_wifi_set_mode(int mode) { assert(mode==WIFI_MODE_STA); return step()?ESP_OK:ESP_FAIL; }
esp_err_t esp_wifi_set_config(int interface, const wifi_config_t *c)
{
    assert(interface==WIFI_IF_STA && c);
    assert(!memcmp(c->sta.ssid,g_nvs_config.wifi_ssid,32));
    assert(!memcmp(c->sta.password,g_nvs_config.wifi_password,64));
    return step()?ESP_OK:ESP_FAIL;
}
esp_err_t esp_wifi_set_bandwidth(int interface,int width)
{ assert(interface==WIFI_IF_STA && width==WIFI_BW_HT20); return step()?ESP_OK:ESP_FAIL; }
esp_err_t csi_peer_prepare_radio(void) { return step()?ESP_OK:ESP_FAIL; }
esp_err_t esp_wifi_start(void)
{ if(!step()) return ESP_FAIL; ++starts; event_handler(NULL,WIFI_EVENT,WIFI_EVENT_STA_START,NULL); return ESP_OK; }
esp_err_t esp_wifi_connect(void)
{
    ++connects;
    if(connect_hook_nested) { connect_hook_nested=false; assert(wifi_connect_once(2)==ESP_OK); }
    if(connect_hook_ip) { connect_hook_ip=false; event_handler(NULL,IP_EVENT,IP_EVENT_STA_GOT_IP,NULL); }
    return connect_result;
}
esp_err_t esp_timer_create(const esp_timer_create_args_t *args, esp_timer_handle_t *out)
{ assert(args->callback==wifi_reconnect_timer_cb); if(!step()) return ESP_FAIL; ++timers; *out=(void *)4; return ESP_OK; }
esp_err_t esp_timer_start_once(esp_timer_handle_t h,uint64_t us)
{ assert(h==(void *)4 && !timer_active && us>=500000 && us<=30000000); if(arm_result==ESP_OK) timer_active=true; return arm_result; }
bool esp_timer_is_active(esp_timer_handle_t h) { assert(h==(void *)4); return timer_active; }
esp_err_t esp_timer_stop(esp_timer_handle_t h) { assert(h==(void *)4); timer_active=false; return ESP_OK; }
esp_err_t esp_timer_delete(esp_timer_handle_t h) { assert(h==(void *)4 && timers); --timers; return ESP_OK; }
void stream_sender_notify_network_change(void) { ++invalidations; }
void status_led_network(bool ready) { network=ready; }
void status_led_init_failed(void) { ++failures; }

static void reset_boot(unsigned fail)
{
    call=0; fail_at=fail; groups=loops=stations=drivers=timers=registrations=failures=0;
    connects=invalidations=starts=0; timer_active=false; network=false; wait_success=true;
    connect_result=arm_result=ESP_OK; connect_hook_ip=connect_hook_nested=false;
    s_wifi_event_group=NULL; s_wifi_reconnect_timer=NULL; s_retry_num=0;
    memset(&s_reconnect,0,sizeof(s_reconnect)); s_wifi_log_window_us=0; s_wifi_log_lines=0;
    memset(s_events,0,sizeof(s_events));
    memset(&g_nvs_config,0,sizeof(g_nvs_config));
    memset(g_nvs_config.wifi_ssid,'s',32); memset(g_nvs_config.wifi_password,'a',64);
    esp_test_set_time_step_us(0); esp_test_set_time_us(1000000);
}
int main(void)
{
    for(unsigned fail=1;fail<=13;++fail) {
        reset_boot(fail); bool connected=true;
        assert(rx_wifi_start(&connected)!=ESP_OK && !connected && failures==1);
        assert(groups+loops+stations+drivers+timers+registrations==0);
    }
    reset_boot(0); bool connected=false;
    assert(rx_wifi_start(&connected)==ESP_OK && connected && network && connects==1);
    event_handler(NULL,WIFI_EVENT,WIFI_EVENT_STA_DISCONNECTED,NULL);
    assert(!network && timer_active && !s_reconnect.connected && s_retry_num==1);
    for(unsigned i=0;i<20;++i) event_handler(NULL,WIFI_EVENT,WIFI_EVENT_STA_DISCONNECTED,NULL);
    assert(s_retry_num==1 && s_reconnect.arm_count==1 && connects==1);
    wifi_reconnect_event_t events[WIFI_RECONNECT_EVENT_CAPACITY];
    assert(wifi_reconnect_events(events,WIFI_RECONNECT_EVENT_CAPACITY)==WIFI_RECONNECT_EVENT_CAPACITY);
    assert(events[0].sequence>1 && events[15].sequence==s_reconnect.event_seq);
    for(unsigned i=1;i<WIFI_RECONNECT_EVENT_CAPACITY;++i) assert(events[i].sequence>events[i-1].sequence);
    uint32_t newest=events[15].sequence;
    wifi_reconnect_trace(1,newest-WIFI_RECONNECT_EVENT_CAPACITY,0,0);
    wifi_reconnect_events(events,WIFI_RECONNECT_EVENT_CAPACITY); assert(events[15].sequence==newest);
    wifi_reconnect_timer_cb(NULL); assert(connects==1); /* stale callback while a newer timer is active */
    timer_active=false; connect_result=ESP_FAIL; connect_hook_ip=connect_hook_nested=true;
    wifi_reconnect_timer_cb(NULL);
    assert(connects==2 && s_reconnect.connected && !timer_active && network);
    assert(s_reconnect.connect_rc==ESP_FAIL && s_retry_num==0); /* GOT_IP wins failed completion */
    event_handler(NULL,WIFI_EVENT,WIFI_EVENT_STA_DISCONNECTED,NULL);
    timer_active=false; connect_result=ESP_FAIL;
    wifi_reconnect_timer_cb(NULL); assert(timer_active && connects==3);
    assert(s_reconnect.arm_delay_ms==1000);
    event_handler(NULL,IP_EVENT,IP_EVENT_STA_GOT_IP,NULL);
    assert(!timer_active && s_reconnect.connected && s_retry_num==0);
    arm_result=ESP_FAIL; event_handler(NULL,WIFI_EVENT,WIFI_EVENT_STA_DISCONNECTED,NULL);
    assert(!timer_active && group.bits&WIFI_FAIL_BIT && s_reconnect.arm_rc==ESP_FAIL);
    reset_boot(0); wait_success=false;
    assert(rx_wifi_start(&connected)==ESP_OK && !connected && starts==1); /* AP absent, no reset */
    assert(failures==0);
    puts("PASS: real WiFi init failure cleanup, full SSID/PSK, AP loss, no overlapping retries, stale timer, GOT_IP race, timer failure, absent AP");
}

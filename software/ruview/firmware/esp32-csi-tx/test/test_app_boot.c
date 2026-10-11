/* Real app_main orchestration; no task, timer, socket or hardware is started. */
#include <assert.h>
#include <stdarg.h>
#include <setjmp.h>
#include <stdlib.h>
#define TX_TEST_CRITICAL_HOOK 1
#define socket mock_socket
#define setsockopt mock_setsockopt
#define sendto mock_sendto
#define close mock_close
#define TX_TEST_TICK_HZ 100
#ifndef TX_APP_SOURCE
#define TX_APP_SOURCE "../main/app_main.c"
#endif
#define tx_boot_poll recorded_boot_poll
#include TX_APP_SOURCE
#undef socket
#undef setsockopt
#undef sendto
#undef close
#undef tx_boot_poll
int tx_boot_poll(tx_boot_check_t *, uint64_t, const tx_runtime_status_t *, bool, uint32_t);

static int fail_at, calls, network_starts, stops, led_calls, boot_accepts;
static bool network_up, status_up, timer_up, sender_progress;
static uint64_t now, anchor, boot_accept_us;
static uint64_t phase_us, network_us, heap_us, log_us, wake_late_us;
static uint64_t probe_time[128], before_sample[128], after_sample[128];
static unsigned delays[128], delay_count, polls, network_checks;
static uint32_t tick_offset;
static bool historical, variable_costs, regress_wait, regress_getter;
static int early_wakes, stop_sender_after, error_after, restart_after;
static uint32_t heap_bytes;
static bool test_sender;
static int sender_case, sender_attempts, sender_limit;
static jmp_buf sender_done;
static TaskHandle_t current_task;
static unsigned critical_depth, real_alloc_calls, real_wifi_calls;
static bool clobber_telemetry_errno;
static int expected_real_errno, returned_real_errno, real_wifi_result, mutate_real_context;
static void *real_alloc_result, *free_on_wifi_return;
static struct netbuf *real_alloc_arg;
static u16_t real_alloc_size;
static uintptr_t real_wifi_arg;
static uint16_t real_wifi_length;
static wifi_interface_t real_wifi_interface;
static int mock_path_send(const void *packet, size_t length);

void tx_test_critical(bool enter)
{
    if (enter) { assert(critical_depth == 0); critical_depth++; }
    else { assert(critical_depth == 1); critical_depth--; }
    if (clobber_telemetry_errno) errno = EIO;
}

TaskHandle_t xTaskGetCurrentTaskHandle(void)
{
    assert(critical_depth == 0);
    if (clobber_telemetry_errno) errno = EIO;
    return current_task;
}

void *__real_netbuf_alloc(struct netbuf *buf, u16_t size)
{
    assert(critical_depth == 0 && errno == expected_real_errno);
    real_alloc_calls++; real_alloc_arg = buf; real_alloc_size = size;
    if (mutate_real_context) {
        portENTER_CRITICAL(&s_status_lock);
        if (mutate_real_context == 1) s_send_attempt.generation++;
        else s_send_attempt.payload_address++;
        portEXIT_CRITICAL(&s_status_lock);
    }
    errno = returned_real_errno;
    return real_alloc_result;
}

int __real_esp_wifi_internal_tx(wifi_interface_t interface, void *buffer, uint16_t length)
{
    assert(critical_depth == 0 && errno == expected_real_errno);
    real_wifi_calls++; real_wifi_interface = interface;
    real_wifi_arg = (uintptr_t)buffer; real_wifi_length = length;
    if (mutate_real_context) {
        portENTER_CRITICAL(&s_status_lock);
        if (mutate_real_context == 1) s_send_attempt.generation++;
        else s_send_attempt.payload_address++;
        portEXIT_CRITICAL(&s_status_lock);
    }
    if (free_on_wifi_return) {
        free(free_on_wifi_return); free_on_wifi_return = NULL;
    }
    errno = returned_real_errno;
    return real_wifi_result;
}

static void advance(uint64_t elapsed)
{
    if (timer_up && sender_progress &&
        (stop_sender_after < 0 || (int)polls < stop_sender_after))
        s_status.sent_packets += (uint32_t)((now + elapsed) / 20000 - now / 20000);
    now += elapsed;
}
int recorded_boot_poll(tx_boot_check_t *c, uint64_t t, const tx_runtime_status_t *s,
                       bool network, uint32_t heap)
{
    assert(polls < 128);
    probe_time[polls] = t; before_sample[polls] = c->last_sample_us;
    int result = tx_boot_poll(c, t, s, network, heap);
    after_sample[polls++] = c->last_sample_us;
    return result;
}
static char log_text[48000];

void mock_log(const char *format, ...)
{
    size_t used=strlen(log_text);va_list args;va_start(args,format);
    vsnprintf(log_text+used,sizeof(log_text)-used,format,args);va_end(args);
    assert(strlen(log_text)+2<sizeof(log_text));strcat(log_text,"\n");
    if (strstr(format, "TX_BOOT_TIMING")) advance(log_us);
    if (test_sender) errno=EIO; /* A later libc/log call may clobber errno. */
}
static esp_err_t step(void){return ++calls==fail_at?ESP_FAIL:ESP_OK;}
esp_err_t tx_config_init(tx_config_t *c)
{
    *c=(tx_config_t){.channel=6,.rate_hz=50,.led_gpio=48};return step();
}
esp_err_t tx_network_start(const tx_config_t *c)
{
    (void)c;network_starts++;esp_err_t e=step();network_up=e==ESP_OK;return e;
}
void tx_network_stop(void){stops++;network_up=false;}
esp_err_t tx_network_check(const tx_config_t *c)
{
    (void)c; network_checks++;
    advance(variable_costs ? (network_checks % 4) * 71351 : network_us);
    if (regress_getter) now -= 1;
    if (error_after >= 0 && (int)polls >= error_after) s_status.send_errors = 1;
    if (restart_after >= 0 && (int)polls >= restart_after) s_status.wifi_restarts = 1;
    return network_up ? ESP_OK : ESP_FAIL;
}
esp_err_t tx_network_restart(const tx_config_t *c){(void)c;assert(false);return ESP_FAIL;}
esp_err_t esp_event_handler_register(esp_event_base_t b,int32_t id,
                                    void (*cb)(void*,esp_event_base_t,int32_t,void*),void *arg)
{
    (void)b;(void)id;(void)cb;(void)arg;return step();
}
esp_err_t tx_ota_start(uint64_t b,const tx_config_t *c,const char *sha,tx_status_provider_t provider)
{
    (void)b;(void)c;(void)sha;assert(provider && network_up);
    esp_err_t e=step();status_up=e==ESP_OK;return e;
}
esp_err_t tx_ota_complete_boot_self_test(bool healthy)
{
    if (healthy) {boot_accept_us=now;boot_accepts++;return step();}return ESP_FAIL;
}
esp_err_t tx_led_start(const tx_config_t *c)
{
    assert(c->led_gpio==48 && network_up && status_up && timer_up);
    led_calls++;now=phase_us;anchor=now;return ESP_ERR_TIMEOUT; /* optional LED cannot stop sounding */
}
uint32_t esp_get_free_heap_size(void){advance(heap_us);return heap_bytes;}
int64_t esp_timer_get_time(void){assert(critical_depth==0);if(clobber_telemetry_errno)errno=EIO;return now;}
int esp_reset_reason(void){return 1;}
void esp_fill_random(void *out,size_t size){memset(out,1,size);}
int mbedtls_sha256(const uint8_t *in,size_t size,uint8_t *out,int mode)
{
    (void)in;(void)size;(void)mode;memset(out,0,32);return step();
}
int xTaskCreate(void (*fn)(void*),const char *name,unsigned stack,void *arg,
                unsigned priority,TaskHandle_t *handle)
{
    (void)fn;(void)name;(void)stack;(void)arg;assert(priority==6);
    if(step()!=ESP_OK)return 0;*handle=(void*)1;return pdPASS;
}
void vTaskDelete(TaskHandle_t h){assert(h==(void*)1);}
void vTaskSuspend(TaskHandle_t h){(void)h;assert(false);}
void vTaskDelay(unsigned delay)
{
    assert(delay > 0 && delay_count < 128); /* never spin/yield with zero ticks */
    delays[delay_count++] = delay;
    if (regress_wait) {now -= 1; return;}
    if (historical && delay_count <= 2) {
        uint64_t wake = delay_count == 1 ? 6316854 : 11516853;
        assert(wake >= now); advance(wake - now); return;
    }
    if (early_wakes > 0) {
        early_wakes--; advance(delay_count == 1 ? 4990000 : 1000); return;
    }
    /* Tick phase and wake latency are independent of the requested duration.
       A tick wait wakes on a boundary, NOT now + ticks * tick_period. */
    uint64_t wake = (now / 10000 + delay) * 10000 + wake_late_us;
    advance(wake - now);
}
TickType_t xTaskGetTickCount(void){return tick_offset + (TickType_t)(now / 10000);}
void xTaskNotifyGive(TaskHandle_t h){(void)h;assert(false);}
uint32_t ulTaskNotifyTake(int clear,TickType_t wait){(void)clear;(void)wait;assert(test_sender);
 if(sender_attempts++==sender_limit)longjmp(sender_done,1);now+=20000;return 1;}
int mock_socket(int domain,int type,int protocol){assert(test_sender && domain==AF_INET && type==SOCK_DGRAM && protocol==IPPROTO_IP);
 if(sender_case==1){errno=EMFILE;return -1;}return 7;}
int mock_setsockopt(int fd,int level,int option,const void *value,socklen_t length){
 assert(fd==7 && level==SOL_SOCKET && option==SO_BROADCAST && length==sizeof(int) && *(const int*)value==1);
 if(sender_case==2){errno=ENOBUFS;return -1;}return 0;}
ssize_t mock_sendto(int fd,const void *packet,size_t length,int flags,const struct sockaddr *destination,socklen_t addrlen){
 (void)packet;assert(fd==7 && flags==0 && addrlen==sizeof(struct sockaddr_in));
 assert(((const struct sockaddr_in*)destination)->sin_port==htons(4210));
 if(sender_case>=5)return mock_path_send(packet,length);
 if(sender_case==3){errno=EAGAIN;return -1;}if(sender_case==4){errno=EDOM;return 3;}return (ssize_t)length;}
int mock_close(int fd){assert(fd==7);errno=EBADF;return 0;}
esp_err_t esp_task_wdt_add(void *h){(void)h;return ESP_OK;}
esp_err_t esp_task_wdt_delete(void *h){(void)h;return ESP_OK;}
esp_err_t esp_task_wdt_reset(void){return ESP_OK;}
esp_err_t esp_timer_create(const esp_timer_create_args_t *a,esp_timer_handle_t *h)
{
    assert(a->callback && !a->skip_unhandled_events);esp_err_t e=step();if(!e)*h=(void*)2;return e;
}
esp_err_t esp_timer_start_periodic(esp_timer_handle_t h,uint64_t interval)
{
    assert(h==(void*)2 && interval==20000);esp_err_t e=step();timer_up=!e;return e;
}
esp_err_t esp_timer_stop(esp_timer_handle_t h){assert(h==(void*)2);timer_up=false;return ESP_OK;}
esp_err_t esp_timer_delete(esp_timer_handle_t h){assert(h==(void*)2);return ESP_OK;}
static void reset(void)
{
    calls=0;network_starts=0;stops=0;led_calls=0;boot_accepts=0;
    network_up=false;status_up=false;timer_up=false;sender_progress=true;
    now=0;anchor=0;boot_accept_us=0;phase_us=0;network_us=0;heap_us=0;log_us=0;
    wake_late_us=0;tick_offset=0;delays[0]=0;delay_count=0;polls=0;network_checks=0;
    historical=false;variable_costs=false;regress_wait=false;regress_getter=false;
    early_wakes=0;stop_sender_after=-1;error_after=-1;restart_after=-1;heap_bytes=64000;
    log_text[0]=0;s_sender_task=NULL;s_status=(tx_runtime_status_t){0};
    s_send_attempt=(tx_send_attempt_t){0};current_task=(void*)1;critical_depth=0;
    clobber_telemetry_errno=false;real_alloc_calls=0;real_wifi_calls=0;mutate_real_context=0;
    expected_real_errno=EDOM;returned_real_errno=EINTR;real_wifi_result=ESP_ERR_NO_MEM;
    real_alloc_result=NULL;free_on_wifi_return=NULL;
}
static void healthy(void)
{
    assert(boot_accepts == 1 && s_status.boot_check == 1 && stops == 0);
    assert(boot_accept_us - anchor >= 60000000 && boot_accept_us < 120000000);
    assert(network_checks == polls);
    unsigned samples = 0;
    for (unsigned i=0; i<polls; i++) {
        if (after_sample[i] == before_sample[i]) continue;
        uint64_t gap = after_sample[i] - before_sample[i];
        assert(gap >= 5000000 && gap <= 6000000);
        assert(after_sample[i] == probe_time[i]); samples++;
    }
    assert(samples >= 10); /* no acceptance after only a shortened observation */
}
static void failed(const char *predicate)
{
    assert(boot_accepts == 0 && s_status.boot_check == -1 && stops == 1);
    assert(strstr(log_text, predicate));
}
static void sender_checks(void)
{
 for(int scenario=0;scenario<=4;scenario++){
  reset();test_sender=true;sender_case=scenario;sender_limit=3;sender_attempts=0;
  s_config=(tx_config_t){.rate_hz=50,.channel=6};s_boot_id=1;
  tx_scheduler_init(&s_scheduler,50,0);
  if(setjmp(sender_done)==0)sender_task(NULL);
  assert(s_status.sent_packets==(scenario?0:3) && s_status.send_errors==(scenario?3:0));
  assert(s_status.wifi_restarts==0);
#ifdef TX_FROZEN_SEND_REGRESSION
  if(scenario==1){
   char expected[96];snprintf(expected,sizeof(expected),"UDP_SEND_ERRNO code=0x%x",EMFILE);
   printf("Frozen socket failure expected original errno=%d; preserved=%d\n",EMFILE,strstr(log_text,expected)!=NULL);
   fflush(stdout);assert(strstr(log_text,expected));return;
  }
#else
  assert(s_status.socket_errors==(scenario==1?3:0));
  assert(s_status.broadcast_errors==(scenario==2?3:0));
  assert(s_status.sendto_errors==(scenario==3?3:0));
  assert(s_status.short_sends==(scenario==4?3:0));
  const tx_send_error_t *failure=&s_status.last_send_error;
  assert(failure->operation==(scenario==1?TX_SEND_OP_SOCKET:scenario==2?TX_SEND_OP_BROADCAST:scenario?TX_SEND_OP_SENDTO:TX_SEND_OP_NONE));
  if(scenario){assert(failure->result==(scenario==4?3:-1) && failure->occurred_us==60000 && failure->sequence==2);
   assert(failure->errno_valid==(scenario!=4));assert(failure->error_number==(scenario==1?EMFILE:scenario==2?ENOBUFS:scenario==3?EAGAIN:0));}
#endif
 }
 #ifndef TX_FROZEN_SEND_REGRESSION
 s_status=(tx_runtime_status_t){0};
 wifi_event(NULL,WIFI_EVENT,WIFI_EVENT_AP_STACONNECTED,NULL);
 wifi_event(NULL,WIFI_EVENT,WIFI_EVENT_AP_STADISCONNECTED,NULL);
 wifi_event(NULL,WIFI_EVENT,WIFI_EVENT_AP_STADISCONNECTED,NULL);
 assert(s_status.station_count==0 && s_status.station_generation==3);
 #endif
 test_sender=false;puts("TX socket/broadcast/sendto/short/local-handoff diagnostics passed");
}

static uint8_t path_wire[128];
static struct netbuf path_netbuf;

static void path_frame(uint8_t *frame, const uint8_t *packet)
{
    memset(frame, 0, 74);
    memset(frame, 255, 6); memcpy(frame + 6, s_config.ap_mac, 6);
    frame[12] = 8; frame[14] = 0x45; frame[17] = 60; frame[23] = 17;
    memcpy(frame + 30, (uint8_t[]){192, 168, 4, 255}, 4);
    frame[36] = 4210 >> 8; frame[37] = 4210 & 255; frame[39] = 40;
    memcpy(frame + 42, packet, 32);
}

static void path_alloc(void *result, u16_t size)
{
    const unsigned before = real_alloc_calls;
    real_alloc_result = result; errno = expected_real_errno;
    assert(__wrap_netbuf_alloc(&path_netbuf, size) == result);
    assert(real_alloc_calls == before + 1 && real_alloc_arg == &path_netbuf && real_alloc_size == size);
    assert(errno == returned_real_errno);
}

static void path_wifi(wifi_interface_t interface, void *frame, uint16_t length)
{
    const unsigned before = real_wifi_calls;
    const uintptr_t address = (uintptr_t)frame;
    errno = expected_real_errno;
    assert(__wrap_esp_wifi_internal_tx(interface, frame, length) == real_wifi_result);
    assert(real_wifi_calls == before + 1 && real_wifi_arg == address &&
           real_wifi_length == length && real_wifi_interface == interface);
    assert(errno == returned_real_errno);
}

static void path_setup(void)
{
    reset(); s_sender_task = current_task; s_boot_id = 7;
    memcpy(s_config.ap_mac, (uint8_t[]){2, 0, 0, 0, 0, 4}, 6);
    uint8_t packet[32]; tx_packet_build(packet, 123, 0, s_boot_id, 6, 50);
    path_frame(path_wire + 1, packet); /* Deliberately unaligned Ethernet/IP headers. */
    clobber_telemetry_errno = true; now = 100; errno = EDOM;
    send_attempt_begin(123, packet); assert(errno == EDOM);
}

static void path_finish(int sent, int error_number, tx_send_path_stage_t stage)
{
    now += 100; errno = EDOM;
    send_attempt_complete(sent, error_number);
    assert(errno == EDOM && !s_send_attempt.active);
    assert(s_status.send_path.last_enomem.stage == stage);
    if (stage != TX_SEND_PATH_NONE) {
        const tx_send_path_error_t *last = &s_status.send_path.last_enomem;
        assert(last->sequence == 123 && last->capture_begin_us == 100 && last->capture_end_us == 200);
    }
}

static int mock_path_send(const void *packet, size_t length)
{
    assert(length == 32);
    path_frame(path_wire + 1, packet);
    expected_real_errno = errno;
    path_alloc(sender_case == 5 ? NULL : path_wire + 43, 32);
    if (sender_case != 5) {
        real_wifi_result = sender_case == 6 ? ESP_ERR_NO_MEM : ESP_OK;
        expected_real_errno = errno;
        path_wifi(WIFI_IF_AP, path_wire + 1, 74);
    }
    errno = ENOMEM;
    return sender_case == 7 ? 32 : -1;
}

static void send_path_checks(void)
{
    path_setup(); path_alloc(NULL, 32);
    path_finish(-1, ENOMEM, TX_SEND_PATH_NETBUF_ALLOCATION);
    assert(s_status.send_path.enomem_netbuf_allocation == 1);

    path_setup(); path_alloc(path_wire + 43, 32); current_task = (void*)2;
    path_wifi(WIFI_IF_AP, path_wire + 1, 74); current_task = s_sender_task;
    path_finish(-1, ENOMEM, TX_SEND_PATH_WIFI_HANDOFF);
    assert(s_status.send_path.enomem_wifi_handoff == 1);

    path_setup(); returned_real_errno = expected_real_errno;
    path_alloc(path_wire + 43, 32); real_wifi_result = ESP_OK;
    path_wifi(WIFI_IF_AP, path_wire + 1, 74);
    path_finish(32, 0, TX_SEND_PATH_NONE);

    path_setup(); path_alloc(path_wire + 43, 32);
    path_finish(-1, ENOMEM, TX_SEND_PATH_UNCLASSIFIED); /* Missing driver observation. */
    path_setup(); path_alloc(path_wire + 43, 32); real_wifi_result = ESP_OK;
    path_wifi(WIFI_IF_AP, path_wire + 1, 74);
    path_finish(-1, ENOMEM, TX_SEND_PATH_UNCLASSIFIED);
    path_setup(); path_alloc(path_wire + 43, 32); real_wifi_result = ESP_ERR_INVALID_ARG;
    path_wifi(WIFI_IF_AP, path_wire + 1, 74);
    path_finish(-1, EINVAL, TX_SEND_PATH_NONE);

    path_setup(); path_alloc(path_wire + 43, 32); path_alloc(path_wire + 43, 32);
    path_wifi(WIFI_IF_AP, path_wire + 1, 74);
    path_finish(-1, ENOMEM, TX_SEND_PATH_UNCLASSIFIED);
    path_setup(); path_alloc(path_wire + 43, 32);
    path_wifi(WIFI_IF_AP, path_wire + 1, 74); path_wifi(WIFI_IF_AP, path_wire + 1, 74);
    path_finish(-1, ENOMEM, TX_SEND_PATH_UNCLASSIFIED);

    for (unsigned size = 31; size <= 32; size++) {
        path_setup(); if (size == 32) current_task = (void*)2;
        path_alloc(path_wire + 43, (u16_t)size);
        assert(s_status.send_path.netbuf_calls == 0);
        path_finish(-1, ENOMEM, TX_SEND_PATH_UNCLASSIFIED);
    }

    for (unsigned length = 0; length < 74; length++) {
        path_setup(); path_alloc(path_wire + 43, 32);
        path_wifi(WIFI_IF_AP, path_wire + 1, (uint16_t)length);
        assert(s_status.send_path.wifi_handoffs == 0);
        path_finish(-1, ENOMEM, TX_SEND_PATH_UNCLASSIFIED);
    }
    const unsigned offsets[] = {0, 6, 12, 14, 17, 20, 21, 23, 30, 36, 39, 42, 50, 62};
    for (unsigned i = 0; i < sizeof(offsets)/sizeof(offsets[0]); i++) {
        path_setup(); path_alloc(path_wire + 43, 32);
        path_wire[1 + offsets[i]] ^= 1;
        path_wifi(WIFI_IF_AP, path_wire + 1, 74);
        assert(s_status.send_path.wifi_handoffs == 0);
        path_finish(-1, ENOMEM, TX_SEND_PATH_UNCLASSIFIED);
    }
    path_setup(); path_alloc(path_wire + 43, 32); path_wire[15] = 0x4f;
    path_wifi(WIFI_IF_AP, path_wire + 1, 74); assert(s_status.send_path.wifi_handoffs == 0);
    const uint16_t foreign_ports[] = {67, 68, 55010};
    for (unsigned i = 0; i < sizeof(foreign_ports)/sizeof(foreign_ports[0]); i++) {
        path_setup(); path_alloc(path_wire + 43, 32);
        path_wire[37] = (uint8_t)(foreign_ports[i] >> 8);
        path_wire[38] = (uint8_t)foreign_ports[i];
        path_wifi(WIFI_IF_AP, path_wire + 1, 74); assert(s_status.send_path.wifi_handoffs == 0);
    }
    path_setup(); path_alloc(path_wire + 43, 32); path_wire[24] = 6; /* HTTP/TCP. */
    path_wifi(WIFI_IF_AP, path_wire + 1, 74); assert(s_status.send_path.wifi_handoffs == 0);
    path_setup(); path_alloc(path_wire + 43, 32); path_wire[21] = 0x20; /* More fragments. */
    path_wifi(WIFI_IF_AP, path_wire + 1, 74); assert(s_status.send_path.wifi_handoffs == 0);
    path_setup(); path_alloc(path_wire + 43, 32); path_wire[21] = 0x80; /* Reserved IPv4 flag. */
    path_wifi(WIFI_IF_AP, path_wire + 1, 74); assert(s_status.send_path.wifi_handoffs == 0);
    path_setup(); path_alloc(path_wire + 43, 32); path_wire[13] = 0x81; path_wire[14] = 0; /* VLAN. */
    path_wifi(WIFI_IF_AP, path_wire + 1, 74); assert(s_status.send_path.wifi_handoffs == 0);
    path_setup(); path_alloc(path_wire + 43, 32);
    path_wifi(0, path_wire + 1, 74); path_wifi(WIFI_IF_AP, NULL, 74);
    uint8_t copied_frame[74]; memcpy(copied_frame, path_wire + 1, 74);
    path_wifi(WIFI_IF_AP, copied_frame, 74); assert(s_status.send_path.wifi_handoffs == 0);

    for (int mutation = 1; mutation <= 2; mutation++) {
        path_setup(); mutate_real_context = mutation; path_alloc(path_wire + 43, 32);
        assert(s_status.send_path.netbuf_calls == 0 && s_send_attempt.netbuf_calls == 0);
        path_setup(); path_alloc(path_wire + 43, 32); mutate_real_context = mutation;
        path_wifi(WIFI_IF_AP, path_wire + 1, 74);
        assert(s_status.send_path.wifi_handoffs == 0 && s_send_attempt.wifi_calls == 0);
        path_finish(-1, ENOMEM, TX_SEND_PATH_UNCLASSIFIED);
    }

    path_setup(); uint8_t *heap_frame = malloc(75); assert(heap_frame);
    memcpy(heap_frame + 1, path_wire + 1, 74); path_alloc(heap_frame + 43, 32);
    free_on_wifi_return = heap_frame; path_wifi(WIFI_IF_AP, heap_frame + 1, 74);
    path_finish(-1, ENOMEM, TX_SEND_PATH_WIFI_HANDOFF); /* No post-real frame access. */
    path_alloc(NULL, 32); path_wifi(WIFI_IF_AP, NULL, 0); /* Inactive paths still preserve errno. */
    assert(s_status.send_path.netbuf_calls == 1 && s_status.send_path.wifi_handoffs == 1);

    for (int scenario = 5; scenario <= 7; scenario++) {
        reset(); test_sender = true; sender_case = scenario; sender_limit = 3; sender_attempts = 0;
        s_sender_task = current_task; clobber_telemetry_errno = true;
        s_config = (tx_config_t){.rate_hz = 50, .channel = 6}; s_boot_id = 1;
        tx_scheduler_init(&s_scheduler, 50, 0);
        if (setjmp(sender_done) == 0) sender_task(NULL);
        assert(s_status.send_errors == (scenario == 7 ? 0U : 3U));
        assert(s_status.sent_packets == (scenario == 7 ? 3U : 0U));
        assert(s_status.send_path.own_attempts == 3 && s_status.send_path.netbuf_calls == 3);
        if (scenario != 7) assert(s_status.last_send_error.error_number == ENOMEM);
        assert(s_status.send_path.enomem_netbuf_allocation == (scenario == 5 ? 3U : 0U));
        assert(s_status.send_path.enomem_wifi_handoff == (scenario == 6 ? 3U : 0U));
    }
    test_sender = false;
    puts("TX send-path ownership, bounds, stale commits, lifetime, errno and real sender checks passed");
}
int main(int argc,char **argv)
{
 if(argc==2 && strcmp(argv[1],"--send-path-tests")==0){send_path_checks();return 0;}
 if(argc==2 && strcmp(argv[1],"--sender-tests")==0){sender_checks();return 0;}
    /* Exact known values, not a claim about the missing historical delay cause. */
    reset();historical=true;phase_us=1318084;network_us=60748;app_main();
#ifdef TX_FROZEN_REGRESSION
    failed("field=sample_gap_us");
    assert(polls == 2 && probe_time[0] == 6316854 && probe_time[1] == 11516853);
    assert(after_sample[0] == 1318084 && before_sample[1] == 1318084);
    assert(probe_time[1] - before_sample[1] == 10198769);
    puts("PASS frozen real caller: early poll skipped; next gap 10198769 > 6000000");
    return 0;
#else
    /* New caller includes getter time and therefore samples the first poll. */
    assert(after_sample[0] == 6377602 && before_sample[1] == 6377602);
    /* Stop the special two-wake trace here; use physical tick simulation below. */
    reset();app_main();healthy();assert(boot_accept_us==60000000 && polls==12);
    for (unsigned phase=0;phase<10000;phase+=137) {
        reset();phase_us=1310000+phase;app_main();healthy();
        assert(delays[0] == 500);
        if (phase) assert(delays[1] == 1 && after_sample[0] == anchor);
    }
    const uint64_t boundary[] = {4999999,5000000,5000001,6000000,6000001};
    for (unsigned i=0;i<sizeof(boundary)/sizeof(boundary[0]);i++) {
        reset();phase_us=1;wake_late_us=boundary[i]-4999999;app_main();
        if (boundary[i] > 6000000) failed("field=sample_gap_us"); else healthy();
        assert(probe_time[0]-anchor==boundary[i]);
    }
    reset();phase_us=8084;variable_costs=true;heap_us=17123;log_us=43491;app_main();healthy();
    assert(probe_time[0] >= 5000000+71351+17123);
    reset();network_us=999999;heap_us=1;app_main();healthy();assert(probe_time[0]==6000000);
    reset();network_us=1000000;heap_us=1;app_main();failed("field=sample_gap_us");
    reset();log_us=6000001;app_main();failed("field=sample_gap_us");
    reset();early_wakes=6;app_main();healthy();
    for(unsigned i=0;i<6;i++)assert(after_sample[i]==anchor);
    assert(delays[1]==1 && polls>12);
    reset();phase_us=8084;tick_offset=UINT32_MAX-300;app_main();healthy();
    assert(xTaskGetTickCount()<tick_offset); /* tick wrap did occur */
    reset();phase_us=8084;regress_wait=true;app_main();failed("clock_order=0");
    reset();regress_getter=true;app_main();failed("clock_order=0");
    reset();phase_us=61000000;app_main();failed("field=boot_elapsed_us");
    reset();stop_sender_after=4;app_main();failed("field=sent_packets_delta");
    reset();error_after=4;app_main();failed("field=send_errors");
    reset();restart_after=4;app_main();failed("field=wifi_restarts");
    reset();heap_bytes=32767;app_main();failed("field=free_heap_bytes");
    reset();heap_bytes=32768;app_main();healthy();
    /* Earlier non-sampling polls must also be monotonic. */
    tx_boot_check_t check;tx_runtime_status_t status={0};
    tx_boot_begin(&check,0,0,&status);
    assert(tx_boot_poll(&check,4999999,&status,true,64000)==0);
    assert(check.last_sample_us==0);
    assert(tx_boot_poll(&check,4999998,&status,true,64000)==-1);
    puts("PASS corrected real caller + bootcheck: 73 tick phases, threshold +/-1us, getter/log costs, real gaps, early repeats, wrap, regression, duration, sender guards");
    reset();app_main();int steps=calls;
    assert(strstr(log_text,"APP_MAIN_ENTER") && strstr(log_text,"PROFILE_OK"));
    assert(strstr(log_text,"STATUS_OK") && strstr(log_text,"BOOT_OK"));
    assert(led_calls==1 && boot_accepts==1 && stops==0 && now==60000000);
    for(fail_at=1;fail_at<=steps;fail_at++) {
        reset();app_main();
        assert(strstr(log_text,"TX_BOOT ERROR stage=") && strstr(log_text,"code=0x"));
        assert(!strstr(log_text,"TX_BOOT BOOT_OK"));
        assert(s_status.boot_check!=1 && !network_up && !timer_up);
        if(fail_at==1)assert(network_starts==0 && stops==0);
        else assert(stops==1 && strstr(log_text,"BOOT_STOP"));
    }
    fail_at=0;reset();sender_progress=false;app_main();
    assert(s_status.boot_check==-1 && strstr(log_text,"stage=BOOT_CHECK") && stops==1);
    sender_checks();send_path_checks();puts("TX app boot orchestration tests passed");return 0;
#endif
}

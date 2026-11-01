/* Execute the production OTA lifecycle without a device, network or flash.
 * SDK validation results are injected; Espressif's image verifier is not
 * reimplemented or claimed tested by these control-flow tests. */
#include "../main/rx_ota.c"
#include <assert.h>
#include "cJSON.h"

static const esp_partition_t running={0x20000,0x200000,0,0x10,"ota_0"};
static const esp_partition_t inactive={0x220000,0x200000,0,0x11,"ota_1"};
static const esp_app_desc_t description={"rx-test","",""};
static bool lease, same_partition, heap_ok=true, disabled, traffic_stopped, peer_stopped, transport_stopped, response_fail;
static unsigned begin_count, write_count, end_count, abort_count, boot_count, restart_count, recv_count;
static unsigned mark_valid, mark_invalid, http_starts, http_stops, registrations, register_fail;
static uint32_t heap_free=64000;
static uint8_t node=1;
static size_t written;
static unsigned heap_checks, heap_fail_at;
static int64_t receive_delay;
static esp_err_t begin_error, write_error, end_error, boot_error, disable_error, server_error;
static esp_ota_img_states_t state=ESP_OTA_IMG_VALID;

const esp_partition_t *esp_ota_get_running_partition(void) { return &running; }
const esp_partition_t *esp_ota_get_next_update_partition(const esp_partition_t *p) { (void)p; return same_partition?&running:&inactive; }
esp_err_t esp_ota_get_state_partition(const esp_partition_t *p,esp_ota_img_states_t *out) { assert(p==&running); *out=state; return ESP_OK; }
esp_err_t esp_partition_get_sha256(const esp_partition_t *p,uint8_t out[32]) { assert(p==&running); memset(out,0xa7,32); return ESP_OK; }
const esp_app_desc_t *esp_app_get_description(void) { return &description; }
esp_err_t esp_ota_begin(const esp_partition_t *p,size_t size,esp_ota_handle_t *out)
{ assert(p==&inactive && size>0 && size<=p->size && disabled && peer_stopped && transport_stopped); ++begin_count; *out=1; return begin_error; }
esp_err_t esp_ota_write(esp_ota_handle_t h,const void *data,size_t size)
{
    assert(h==1 && size>0 && size<=1024); ++write_count;
    /* esp_ota_ops.c at pinned SDK: first magic is checked by esp_ota_write. */
    if(!written && *(const uint8_t *)data!=0xe9) return ESP_ERR_OTA_VALIDATE_FAILED;
    if(write_error) return write_error; written+=size; return ESP_OK;
}
esp_err_t esp_ota_end(esp_ota_handle_t h) { assert(h==1); ++end_count; return end_error; }
esp_err_t esp_ota_abort(esp_ota_handle_t h) { assert(h==1); ++abort_count; return ESP_OK; }
esp_err_t esp_ota_set_boot_partition(const esp_partition_t *p) { assert(p==&inactive && end_count==1); ++boot_count; return boot_error; }
esp_err_t esp_ota_mark_app_invalid_rollback_and_reboot(void) { ++mark_invalid; return ESP_OK; }
esp_err_t esp_ota_mark_app_valid_cancel_rollback(void) { ++mark_valid; return ESP_OK; }
uint32_t esp_get_free_heap_size(void) { return heap_free; }
int esp_reset_reason(void) { return 0; }
void esp_restart(void) { ++restart_count; }
bool heap_caps_check_integrity(uint32_t caps,bool verbose)
{ assert(caps==(MALLOC_CAP_INTERNAL|MALLOC_CAP_8BIT) && verbose); ++heap_checks; return heap_ok && heap_checks!=heap_fail_at; }
esp_err_t esp_wifi_set_csi(bool enabled) { assert(!enabled); if(!disable_error) disabled=true; return disable_error; }
void csi_collector_notify_network_change(bool connected) { assert(!connected && disabled); traffic_stopped=true; }
void csi_peer_stop(void) { assert(disabled && traffic_stopped); peer_stopped=true; }
void csi_transport_stop(void) { assert(disabled && peer_stopped); transport_stopped=true; }
void csi_transport_get_stats(csi_transport_stats_t *out) { memset(out,0,sizeof(*out)); }
void stream_sender_get_stats(stream_sender_stats_t *out) { memset(out,0,sizeof(*out)); }
uint8_t csi_collector_get_node_id(void) { return node; }
uint64_t csi_collector_get_boot_id(void) { return 7; }
uint32_t csi_collector_get_sequence(void) { return 1; }
int8_t csi_collector_get_rssi(void) { return -40; }
void status_led_ota(bool active,bool failed) { assert(active!=failed); }
bool status_led_test(void) { return true; }
bool rx_control_measurement_active(void) { return lease; }
bool diag_http_requested(httpd_req_t *req) { (void)req; return false; }
esp_err_t diag_http_send(httpd_req_t *req) { (void)req; return ESP_OK; }
rx_http_scope_t rx_http_enter(unsigned phase) { return (rx_http_scope_t){phase}; }
void rx_http_leave(rx_http_scope_t *scope) { scope->phase=0; }
void rx_http_change(rx_http_scope_t *scope,unsigned phase) { scope->phase=phase; }
esp_err_t httpd_req_get_hdr_value_str(httpd_req_t *req,const char *name,char *out,size_t size)
{ assert(!strcmp(name,"Authorization") && size>=16); if(!req->authenticated) return ESP_FAIL; strcpy(out,"Bearer test-key"); return ESP_OK; }
int httpd_req_recv(httpd_req_t *req,char *out,size_t size)
{
    ++recv_count; esp_test_advance_time_us(receive_delay);
    if(req->fail_receive) return HTTPD_SOCK_ERR_TIMEOUT;
    size_t remaining=strlen(req->body)-req->received;
    if(size>remaining) size=remaining; memcpy(out,req->body+req->received,size); req->received+=size; return (int)size;
}
esp_err_t httpd_resp_send_err(httpd_req_t *req,int status,const char *reason)
{ assert(reason); req->response_error=status; return ESP_OK; }
esp_err_t httpd_resp_send(httpd_req_t *req,const char *data,int size)
{ (void)req; assert(size>0 || size==HTTPD_RESP_USE_STRLEN); cJSON *json=cJSON_Parse(data); assert(json); cJSON_Delete(json); return response_fail?ESP_FAIL:ESP_OK; }
esp_err_t httpd_resp_set_type(httpd_req_t *req,const char *type) { (void)req; assert(!strcmp(type,"application/json")); return ESP_OK; }
esp_err_t httpd_start(httpd_handle_t *out,const httpd_config_t *config)
{ assert(config->stack_size==8192 && config->server_port==8032 && config->recv_wait_timeout==5); ++http_starts; if(server_error) return server_error; *out=(void *)1; return ESP_OK; }
esp_err_t httpd_stop(httpd_handle_t server) { assert(server==(void *)1); ++http_stops; return ESP_OK; }
esp_err_t httpd_register_uri_handler(httpd_handle_t server,const httpd_uri_t *uri)
{ assert(server==(void *)1 && uri && uri->handler); ++registrations; return registrations==register_fail?ESP_FAIL:ESP_OK; }

static void reset(void)
{
    lease=same_partition=disabled=traffic_stopped=peer_stopped=transport_stopped=response_fail=false;
    begin_count=write_count=end_count=abort_count=boot_count=restart_count=recv_count=0;
    mark_valid=mark_invalid=http_starts=http_stops=registrations=register_fail=0;
    heap_ok=true; heap_free=64000; heap_checks=heap_fail_at=0; node=1; written=0; receive_delay=0;
    begin_error=write_error=end_error=boot_error=disable_error=server_error=ESP_OK;
    state=ESP_OTA_IMG_VALID; strcpy(s_ota_psk,"test-key"); s_upload=(__typeof__(s_upload)){.phase="idle"};
    esp_test_set_time_step_us(0); esp_test_set_time_us(1000000);
}
static httpd_req_t upload(const char *body,int length)
{
    httpd_req_t req={.body=body,.content_len=length,.authenticated=true};
    ota_upload_handler(&req); return req;
}
int main(void)
{
    char image[2501]; memset(image,'x',sizeof(image)-1); image[0]=(char)0xe9; image[2500]=0;
    reset(); httpd_req_t unauth={.body=image,.content_len=2500}; ota_upload_handler(&unauth);
    assert(unauth.response_error==403 && begin_count==0 && !disabled);
    reset(); lease=true; assert(upload(image,2500).response_error==400 && begin_count==0 && !disabled);
    reset(); state=ESP_OTA_IMG_PENDING_VERIFY;
    assert(upload(image,2500).response_error==400 && begin_count==0 && !disabled);
    reset(); assert(upload(image,0).response_error==400 && begin_count==0 && !disabled);
    reset(); assert(upload(image,0x200001).response_error==400 && begin_count==0 && !disabled);
    reset(); same_partition=true; assert(upload(image,2500).response_error==500 && begin_count==0);
    reset(); disable_error=ESP_FAIL; assert(upload(image,2500).response_error==500 && begin_count==0);
    for(unsigned fail=1;fail<=4;++fail) {
        reset(); heap_fail_at=fail; assert(upload(image,2500).response_error==500);
        assert(boot_count==0 && restart_count==0 && abort_count==(fail==1?0:1));
    }
    reset(); begin_error=ESP_FAIL; assert(upload(image,2500).response_error==500 && abort_count==0 && begin_count==1);
    reset(); httpd_req_t req={.body=image,.content_len=2500,.authenticated=true,.fail_receive=true};
    ota_upload_handler(&req); assert(req.response_error==500 && recv_count==1 && write_count==0 && abort_count==1);
    reset(); assert(upload("short",2500).response_error==500 && abort_count==1 && boot_count==0);
    reset(); const char truncated[]={(char)0xe9,'x',0};
    assert(upload(truncated,2500).response_error==500 && written==2 && recv_count==2 && abort_count==1);
    reset(); image[0]='x'; assert(upload(image,2500).response_error==500 && write_count==1 && abort_count==1); image[0]=(char)0xe9;
    reset(); write_error=ESP_FAIL; assert(upload(image,2500).response_error==500 && write_count==1 && abort_count==1 && written==0);
    reset(); end_error=ESP_ERR_OTA_VALIDATE_FAILED;
    assert(upload(image,2500).response_error==500 && end_count==1 && abort_count==0 && boot_count==0);
    reset(); boot_error=ESP_FAIL; assert(upload(image,2500).response_error==500 && boot_count==1 && restart_count==0);
    reset(); receive_delay=10000000; assert(upload(image,2500).response_error==500 && recv_count==1 && end_count==0 && abort_count==1);
    for(unsigned broken_response=0;broken_response<2;++broken_response) {
        reset(); response_fail=broken_response; assert(upload(image,2500).response_error==0);
        assert(begin_count==1 && write_count==3 && written==2500 && end_count==1 && boot_count==1 && restart_count==1 && abort_count==0);
    }
    reset(); state=ESP_OTA_IMG_PENDING_VERIFY; ota_update_complete_boot_self_test(false); assert(mark_invalid==1 && mark_valid==0);
    reset(); state=ESP_OTA_IMG_PENDING_VERIFY; heap_free=32767; ota_update_complete_boot_self_test(true); assert(mark_invalid==1);
    reset(); state=ESP_OTA_IMG_PENDING_VERIFY; node=0; ota_update_complete_boot_self_test(true); assert(mark_invalid==1);
    reset(); state=ESP_OTA_IMG_PENDING_VERIFY; ota_update_complete_boot_self_test(true); assert(mark_valid==1 && mark_invalid==0);
    for(unsigned fail=1;fail<=4;++fail) {
        reset(); register_fail=fail; void *server=(void *)99;
        assert(ota_update_init_ex(&server)!=ESP_OK && server==NULL && http_stops==1 && s_ota_psk[0]==0);
    }
    reset(); server_error=ESP_FAIL; void *server=(void *)99;
    assert(ota_update_init_ex(&server)!=ESP_OK && server==NULL && http_stops==0);
    reset(); httpd_req_t status={.authenticated=true}; assert(ota_status_handler(&status)==ESP_OK);
    puts("PASS: production OTA auth/lease/size/partition, quiescence, heap, timeout/truncation, SDK validation/write/end/boot failures, bounded upload, pending boot, server cleanup");
}

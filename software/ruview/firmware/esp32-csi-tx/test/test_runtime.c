/* Hardware-free IDF boundary tests. All side effects below are in-memory fakes. */
#include <assert.h>
#include <stdarg.h>
#include "esp_heap_caps.h"
#include "tx_config.h"
#include "tx_network.h"
#include "tx_boot.h"
#include "../main/tx_ota.c"

static tx_config_t fixture;
static const char *missing_key;
static int init_error, calls, fail_call, started, storage, dhcp, writes, aborted, selected, restarted;
static int response_code, receive_result = 64, begin_error, end_error, write_error, select_error;
static int mark_valid, mark_invalid, ota_state = ESP_OTA_IMG_PENDING_VERIFY;
static bool rollback_possible, running_missing;
static unsigned ap_ready_after, ap_status_reads;
static bool ap_never_ready;
static bool event_on_start;
static bool normalize_country = true;
/* Independent observations, never derived from the setter's requested power.
   44/20 is the saved hardware tuple; other tuples exercise failure paths. */
static int8_t observed_country_after_set = 20, observed_power_after_set = 44;
static bool change_country_at_ap_ready;
static bool change_power_at_ap_ready;
static unsigned country_reads, fail_country_read;
static int64_t clock_us, clock_step;
static uint32_t heap = 64000;
static UBaseType_t http_stack_low = 3000;
static size_t internal_free = 61000, internal_largest = 50000, internal_minimum = 59000;
static char response_body[4096], auth_header[96];
static wifi_config_t configured_ap;
static wifi_country_t configured_country;
static uint8_t configured_mac[6], hw_mac[6], channel, protocol;
static wifi_second_chan_t secondary;
static wifi_bandwidth_t bandwidth;
static wifi_mode_t wifi_mode;
static int station_count;
static uint8_t station_macs[15][6];
static bool change_generation_during_list;
static unsigned station_list_reads, image_digest_reads;
static int http_chunks;
static bool request_headers_available = true;
/* Each invocation represents a fresh HTTP request; sending a chunk purges it. */
static esp_err_t status_request(httpd_req_t *request)
{
 request_headers_available=true;
 return status_handler(request);
}
static const char *getter_failure;
static esp_err_t getter_error = ESP_FAIL;
static char radio_logs[16000], radio_order[64];
static size_t radio_order_length;
static void (*network_event)(void *, esp_event_base_t, int32_t, void *);
static void order(char operation) {
 assert(radio_order_length+1<sizeof(radio_order));
 radio_order[radio_order_length++]=operation;radio_order[radio_order_length]=0;
}
#define GETTER(name) do { if (getter_failure && strcmp(getter_failure,name)==0) return getter_error; } while(0)
static int8_t power;
static esp_netif_ip_info_t configured_ip;
static dhcps_lease_t configured_lease;
static esp_netif_t netif;
static esp_partition_t partition = {.address=0x9000,.size=0x5000,.label="nvs"};
static esp_partition_t next = {.address=0x10000,.size=0x140000,.subtype=16,.label="app0"};
static esp_partition_t running = {.address=0x150000,.size=0x140000,.subtype=17,.label="app1"};
static tx_runtime_status_t runtime;
static char last_log[512], last_error[512];
static unsigned error_logs;
void mock_log(const char *format, ...) {
 va_list args;va_start(args,format);vsnprintf(last_log,sizeof(last_log),format,args);va_end(args);
 if(strstr(last_log,"TX_BOOT ERROR")) { assert(strstr(last_log,"code=0x"));error_logs++;strcpy(last_error,last_log); }
 if((strstr(last_log,"TX_RADIO") || strstr(last_log,"TX_NET") || strstr(last_log,"TX_COMPARE") || strstr(last_log,"TX_POWER") || strstr(last_log,"TX_BOOT ERROR")) && strlen(radio_logs)+strlen(last_log)+2<sizeof(radio_logs)) {
  strcat(radio_logs,last_log);strcat(radio_logs,"\n");
 }
}
static int step(void) { return ++calls == fail_call ? ESP_FAIL : ESP_OK; }
#define STEP() do {int e=step(); if(e) return e;}while(0)
const esp_partition_t *esp_partition_find_first(int a,int b,const char*c){(void)a;(void)b;(void)c;return &partition;}
esp_err_t nvs_flash_init_partition_ptr(const esp_partition_t*p){assert(p->readonly);return init_error;}
esp_err_t nvs_open(const char*n,int mode,nvs_handle_t*h){assert(mode==NVS_READONLY);*h=strcmp(n,"security")==0;return 0;}
void nvs_close(nvs_handle_t h){(void)h;}
static bool missing(const char *key){return missing_key && strcmp(key,missing_key)==0;}
esp_err_t nvs_get_str(nvs_handle_t h,const char*k,char*out,size_t*size){
 (void)h;if(missing(k))return ESP_ERR_NOT_FOUND;
 const char*v=strcmp(k,"node_id")==0?fixture.node_id:strcmp(k,"ssid")==0?fixture.wifi_ssid:
 strcmp(k,"password")==0?fixture.wifi_password:strcmp(k,"country")==0?fixture.country:fixture.ota_psk;
 size_t n=strlen(v)+1;if(n>*size)return ESP_ERR_INVALID_ARG;memcpy(out,v,n);*size=n;return 0;
}
esp_err_t nvs_get_blob(nvs_handle_t h,const char*k,void*out,size_t*size){(void)h;if(missing(k))return 4;assert(*size==6);memcpy(out,strcmp(k,"base_mac")==0?fixture.base_mac:fixture.ap_mac,6);return 0;}
esp_err_t nvs_get_u8(nvs_handle_t h,const char*k,uint8_t*out){
 (void)h;if(missing(k))return 4;
 *out=strcmp(k,"schema_v")==0?fixture.schema_version:strcmp(k,"channel")==0?fixture.channel:
 strcmp(k,"bw_mhz")==0?fixture.bandwidth_mhz:strcmp(k,"protocol")==0?fixture.protocol_mask:
 strcmp(k,"secondary")==0?fixture.secondary_channel:strcmp(k,"country_nch")==0?fixture.country_channels:
 strcmp(k,"led_gpio")==0?(uint8_t)fixture.led_gpio:fixture.led_active_low;return 0;
}
esp_err_t nvs_get_u16(nvs_handle_t h,const char*k,uint16_t*out){(void)h;if(missing(k))return 4;*out=strcmp(k,"rate_hz")==0?fixture.rate_hz:fixture.beacon_interval;return 0;}
esp_err_t esp_netif_init(void){STEP();return 0;}
esp_err_t esp_event_loop_create_default(void){STEP();return 0;}
esp_err_t esp_event_handler_register(esp_event_base_t b,int32_t id,
 void (*handler)(void*,esp_event_base_t,int32_t,void*),void *arg){
 (void)b;(void)arg;assert(id==ESP_EVENT_ANY_ID);STEP();network_event=handler;return 0;
}
esp_netif_t *esp_netif_new(const esp_netif_config_t*c){(void)c;return step()==ESP_OK?&netif:NULL;}
esp_err_t esp_netif_attach_wifi_ap(esp_netif_t*n){assert(n==&netif);STEP();return 0;}
esp_err_t esp_wifi_set_default_wifi_ap_handlers(void){STEP();return 0;}
esp_err_t esp_netif_dhcps_stop(esp_netif_t*n){(void)n;dhcp=0;return 0;}
esp_err_t esp_netif_dhcps_start(esp_netif_t*n){(void)n;STEP();dhcp=started?1:0;return 0;}
esp_err_t esp_netif_set_ip_info(esp_netif_t*n,const esp_netif_ip_info_t*i){(void)n;STEP();configured_ip=*i;return 0;}
esp_err_t esp_netif_get_ip_info(esp_netif_t*n,esp_netif_ip_info_t*i){(void)n;GETTER("ip");*i=configured_ip;return 0;}
esp_err_t esp_netif_dhcps_option(esp_netif_t*n,int op,int id,void*value,size_t size){
 (void)n;assert(id==ESP_NETIF_REQUESTED_IP_ADDRESS&&size==sizeof(dhcps_lease_t));STEP();
 if(op==ESP_NETIF_OP_GET){GETTER("lease");}if(op==ESP_NETIF_OP_SET){assert(!dhcp);configured_lease=*(dhcps_lease_t*)value;}else *(dhcps_lease_t*)value=configured_lease;return 0;
}
esp_err_t esp_netif_dhcps_get_status(esp_netif_t*n,esp_netif_dhcp_status_t*s){GETTER("dhcp");
 (void)n;if(started&&!ap_never_ready&&++ap_status_reads>ap_ready_after) {
  if(!dhcp&&!event_on_start){order('E');network_event(NULL,WIFI_EVENT,WIFI_EVENT_AP_START,NULL);}
  dhcp=1;
  if(change_country_at_ap_ready)configured_country.max_tx_power=21;
  if(change_power_at_ap_ready)power=52;
 }*s=dhcp;return 0;
}
esp_err_t esp_efuse_mac_get_default(uint8_t*m){memcpy(m,hw_mac,6);return 0;}
esp_err_t esp_wifi_init(const wifi_init_config_t*i){(void)i;STEP();return 0;}
esp_err_t esp_wifi_set_storage(int v){STEP();storage=v;return 0;}
esp_err_t esp_wifi_set_mode(int v){assert(v==WIFI_MODE_AP);STEP();wifi_mode=v;return 0;}
esp_err_t esp_wifi_get_mode(wifi_mode_t*v){GETTER("mode");STEP();*v=wifi_mode;return 0;}
esp_err_t esp_wifi_ap_get_sta_list(wifi_sta_list_t*v){station_list_reads++;GETTER("stations");STEP();v->num=station_count;
 for(int i=0;i<station_count && i<15;i++)memcpy(v->sta[i].mac,station_macs[i],6);
 if(change_generation_during_list)runtime.station_generation++;return 0;}
esp_err_t esp_wifi_set_mac(int i,const uint8_t*m){(void)i;assert(!started);STEP();memcpy(configured_mac,m,6);return 0;}
esp_err_t esp_wifi_get_mac(int i,uint8_t*m){(void)i;GETTER("mac");memcpy(m,configured_mac,6);return 0;}
esp_err_t esp_wifi_set_config(int i,const wifi_config_t*c){(void)i;assert(storage==WIFI_STORAGE_RAM&&!started);radio_order_length=0;order('C');STEP();configured_ap=*c;return 0;}
esp_err_t esp_wifi_get_config(int i,wifi_config_t*c){(void)i;GETTER("config");STEP();*c=configured_ap;return 0;}
esp_err_t esp_wifi_set_country(const wifi_country_t*c){STEP();configured_country=*c; /* IDF 5.4 esp_wifi.h: non I/O/X third octets mean space. */
 country_reads=0;
 configured_country.max_tx_power=observed_country_after_set;
 if(normalize_country&&c->cc[2]!='I'&&c->cc[2]!='O'&&c->cc[2]!='X')configured_country.cc[2]=' ';return 0;}
esp_err_t esp_wifi_get_country(wifi_country_t*c){GETTER("country");if(++country_reads==fail_country_read)return getter_error;*c=configured_country;return 0;}
esp_err_t esp_wifi_set_protocol(int i,uint8_t v){(void)i;STEP();protocol=v;return 0;}
esp_err_t esp_wifi_get_protocol(int i,uint8_t*v){(void)i;GETTER("protocol");*v=protocol;return 0;}
esp_err_t esp_wifi_set_bandwidth(int i,wifi_bandwidth_t v){(void)i;assert(!started);order('B');STEP();bandwidth=v;return 0;}
esp_err_t esp_wifi_get_bandwidth(int i,wifi_bandwidth_t*v){(void)i;GETTER("bandwidth");STEP();*v=bandwidth;return 0;}
esp_err_t esp_wifi_start(void){
 assert(configured_lease.end_ip.addr==inet_addr("192.168.4.49"));order('S');STEP();started=1;ap_status_reads=0;
 if(event_on_start){order('E');network_event(NULL,WIFI_EVENT,WIFI_EVENT_AP_START,NULL);}return 0;
}
esp_err_t esp_wifi_stop(void){started=0;return 0;}
/* This fake echoes the requested state for basic control-flow tests only.
   The independent observations below model permitted inputs, not driver timing. */
esp_err_t esp_wifi_set_channel(uint8_t c,wifi_second_chan_t s){assert(started&&station_count==0);order('H');STEP();channel=c;secondary=s;return 0;}
esp_err_t esp_wifi_get_channel(uint8_t*c,wifi_second_chan_t*s){GETTER("channel");STEP();*c=channel;*s=secondary;return 0;}
esp_err_t esp_wifi_set_max_tx_power(int8_t v){STEP();assert(started&&v==44);power=observed_power_after_set;return 0;}
esp_err_t esp_wifi_get_max_tx_power(int8_t*v){assert(started);GETTER("power");*v=power;return 0;}
const esp_app_desc_t *esp_app_get_description(void){static esp_app_desc_t a={.version="test"};return &a;}
const esp_partition_t *esp_ota_get_running_partition(void){return running_missing?NULL:&running;}
const esp_partition_t *esp_ota_get_next_update_partition(void*p){(void)p;return &next;}
esp_err_t esp_ota_get_state_partition(const esp_partition_t*p,esp_ota_img_states_t*s){(void)p;GETTER("ota_state");*s=ota_state;return 0;}
esp_err_t esp_partition_get_sha256(const esp_partition_t*p,uint8_t*v){(void)p;image_digest_reads++;GETTER("image_digest");memset(v,0,32);return 0;}
esp_err_t esp_ota_begin(const esp_partition_t*p,int n,esp_ota_handle_t*h){(void)p;(void)n;writes++;*h=1;return begin_error;}
esp_err_t esp_ota_write(esp_ota_handle_t h,const void*p,size_t n){(void)h;(void)p;(void)n;return write_error;}
esp_err_t esp_ota_abort(esp_ota_handle_t h){(void)h;aborted++;return 0;}
esp_err_t esp_ota_end(esp_ota_handle_t h){(void)h;return end_error;}
esp_err_t esp_ota_set_boot_partition(const esp_partition_t*p){(void)p;selected++;return select_error;}
bool esp_ota_check_rollback_is_possible(void){return rollback_possible;}
esp_err_t esp_ota_mark_app_invalid_rollback_and_reboot(void){mark_invalid++;return 0;}
esp_err_t esp_ota_mark_app_valid_cancel_rollback(void){mark_valid++;return 0;}
uint32_t esp_get_free_heap_size(void){return heap;}
UBaseType_t uxTaskGetStackHighWaterMark(TaskHandle_t t){assert(t==NULL);return http_stack_low;}
size_t heap_caps_get_free_size(uint32_t c){assert(c==(MALLOC_CAP_INTERNAL|MALLOC_CAP_8BIT));return internal_free;}
size_t heap_caps_get_largest_free_block(uint32_t c){assert(c==(MALLOC_CAP_INTERNAL|MALLOC_CAP_8BIT));return internal_largest;}
size_t heap_caps_get_minimum_free_size(uint32_t c){assert(c==(MALLOC_CAP_INTERNAL|MALLOC_CAP_8BIT));return internal_minimum;}
void esp_restart(void){restarted++;}
int64_t esp_timer_get_time(void){int64_t now=clock_us;clock_us+=clock_step;return now;}
void vTaskDelay(unsigned n){(void)n;}
esp_err_t httpd_req_get_hdr_value_str(httpd_req_t*r,const char*k,char*out,size_t n){(void)r;(void)k;if(!request_headers_available)return ESP_FAIL;if(strlen(auth_header)>=n)return 1;strlcpy(out,auth_header,n);return 0;}
esp_err_t httpd_resp_send_err(httpd_req_t*r,int code,const char*body){(void)r;response_code=code;strlcpy(response_body,body,sizeof(response_body));return 0;}
esp_err_t httpd_resp_set_type(httpd_req_t*r,const char*t){(void)r;(void)t;return 0;}
esp_err_t httpd_resp_send(httpd_req_t*r,const char*b,int n){(void)r;assert(n<(int)sizeof(response_body));memcpy(response_body,b,n);response_body[n]=0;return 0;}
esp_err_t httpd_resp_send_chunk(httpd_req_t*r,const char*b,int n){(void)r;
 request_headers_available=false; /* ESP-IDF httpd_txrx.c: req_hdrs_count = 0. */
 if(!b && !n){http_chunks=0;return ESP_OK;}if(http_chunks++==0)response_body[0]=0;
 size_t used=strlen(response_body);assert(n>=0 && used+(size_t)n<sizeof(response_body));memcpy(response_body+used,b,n);response_body[used+n]=0;return ESP_OK;}
esp_err_t httpd_resp_sendstr(httpd_req_t*r,const char*b){return httpd_resp_send(r,b,strlen(b));}
int httpd_req_recv(httpd_req_t*r,char*b,size_t n){(void)r;memset(b,0,n);return receive_result<0?receive_result:(int)n;}
esp_err_t httpd_start(httpd_handle_t*h,const httpd_config_t*c){assert(c->recv_wait_timeout==2 && c->send_wait_timeout==2 && c->stack_size==8192);STEP();*h=(void*)1;return 0;}
esp_err_t httpd_stop(httpd_handle_t h){(void)h;return 0;}
esp_err_t httpd_register_uri_handler(httpd_handle_t h,const httpd_uri_t*u){(void)h;(void)u;STEP();return 0;}
static void provider(tx_runtime_status_t*s){*s=runtime;}

static void expect_mismatch(const char *field)
{
 radio_logs[0]=0;
 assert(tx_network_check(&fixture)==ESP_ERR_INVALID_STATE);
 assert(strstr(radio_logs,field) && strstr(radio_logs,"TX_COMPARE op=") && strstr(radio_logs,"expected="));
 assert(strstr(radio_logs,"actual=") && strstr(radio_logs,"boot_us=") && strstr(radio_logs,"since_start_us="));
 /* At the first radio/other mismatch all safe read-only values are collected. */
 assert(strstr(radio_logs,"country.cc[2]") && strstr(radio_logs,"esp_netif_dhcps_option(GET)"));
 assert(!strstr(radio_logs,fixture.wifi_ssid) && !strstr(radio_logs,fixture.wifi_password) && !strstr(radio_logs,fixture.ota_psk));
}
static void semantic_checks(void)
{
 /* Regression: SDK-normalized environment differs from the alpha-2 C string. */
 assert(configured_country.cc[2]==' ' && memcmp(configured_country.cc,fixture.country,3)!=0);
 assert(tx_network_check(&fixture)==ESP_OK);
 const unsigned char unspecified[]={0,' ', '?', 0xff};
 for(size_t i=0;i<sizeof(unspecified);i++) {
  configured_country.cc[2]=(char)unspecified[i];radio_logs[0]=0;
  assert(tx_network_check(&fixture)==ESP_OK);
  assert(strstr(radio_logs,"expected_raw=0x00") && strstr(radio_logs,"actual_env=0x20"));
 }
 configured_country.cc[2]=' ';
#define BAD(type, target, value, field) do { \
 type saved=(target);(target)=(value);expect_mismatch(field);(target)=saved; \
} while(0)
 BAD(char,configured_country.cc[0],'U',"country.cc[0]");
 BAD(char,configured_country.cc[1],'S',"country.cc[1]");
 for(const char *e="IOX";*e;e++){BAD(char,configured_country.cc[2],*e,"country.environment");}
 BAD(uint8_t,configured_country.schan,2,"country.schan");
 BAD(uint8_t,configured_country.nchan,12,"country.nchan");
 BAD(uint8_t,configured_country.nchan,14,"country.nchan");
 BAD(int,configured_country.policy,0,"country.policy");
 BAD(int8_t,configured_country.max_tx_power,10,"country.max_tx_power_dBm");
 BAD(int8_t,configured_country.max_tx_power,11,"country.max_tx_power_dBm");
 BAD(int8_t,configured_country.max_tx_power,12,"country.max_tx_power_dBm");
 BAD(int8_t,configured_country.max_tx_power,19,"country.max_tx_power_dBm");
 BAD(int8_t,configured_country.max_tx_power,21,"country.max_tx_power_dBm");
 BAD(int8_t,power,43,"power_quarter_dBm");
 BAD(int8_t,power,45,"power_quarter_dBm");
 BAD(uint8_t,protocol,3,"protocol_mask");
 BAD(uint8_t,configured_mac[5],4,"ap_mac[5]");
 BAD(wifi_mode_t,wifi_mode,WIFI_MODE_APSTA,"field=mode");
 BAD(uint8_t,channel,7,"field=primary");
 BAD(wifi_second_chan_t,secondary,WIFI_SECOND_CHAN_ABOVE,"field=secondary");
 BAD(wifi_bandwidth_t,bandwidth,WIFI_BW_HT20,"interface_bandwidth_enum");
 BAD(uint8_t,configured_ap.ap.channel,7,"ap.channel");
 BAD(uint8_t,configured_ap.ap.max_connection,7,"ap.max_connection");
 BAD(uint8_t,configured_ap.ap.max_connection,9,"ap.max_connection");
 BAD(int,configured_ap.ap.authmode,0,"ap.authmode");
 BAD(uint16_t,configured_ap.ap.beacon_interval,102,"ap.beacon_interval_TU");
 BAD(uint8_t,configured_ap.ap.ssid_len,33,"ap.ssid_representation_valid");
 BAD(uint8_t,configured_ap.ap.ssid_len,3,"ap.ssid_length");
 BAD(uint8_t,configured_ap.ap.ssid[0],'X',"ap.ssid_matches_redacted");
 BAD(uint8_t,configured_ap.ap.password[0],'X',"ap.password_matches_redacted");
 BAD(uint8_t,configured_ap.ap.password[strlen(fixture.wifi_password)],'X',"ap.password_matches_redacted");
 /* Both SDK SSID representations and arbitrary unused padding are valid. */
 configured_ap.ap.ssid_len=0;assert(tx_network_check(&fixture)==ESP_OK);
 memset(configured_ap.ap.ssid+strlen(fixture.wifi_ssid)+1,0xa5,sizeof(configured_ap.ap.ssid)-strlen(fixture.wifi_ssid)-1);
 memset(configured_ap.ap.password+strlen(fixture.wifi_password)+1,0xa5,sizeof(configured_ap.ap.password)-strlen(fixture.wifi_password)-1);
 assert(tx_network_check(&fixture)==ESP_OK);
 wifi_config_t saved_ap=configured_ap;char saved_ssid[33];memcpy(saved_ssid,fixture.wifi_ssid,33);
 memset(fixture.wifi_ssid,'s',32);fixture.wifi_ssid[32]=0;
 memset(configured_ap.ap.ssid,'s',32);configured_ap.ap.ssid_len=32;
 assert(tx_network_check(&fixture)==ESP_OK);
 configured_ap.ap.ssid_len=0;expect_mismatch("ap.ssid_representation_valid");
 configured_ap=saved_ap;memcpy(fixture.wifi_ssid,saved_ssid,33);configured_ap.ap.ssid_len=strlen(fixture.wifi_ssid);
 BAD(uint32_t,configured_ip.ip.addr,inet_addr("192.168.4.2"),"ip.network_order");
 BAD(uint32_t,configured_ip.gw.addr,inet_addr("192.168.4.2"),"gw.network_order");
 BAD(uint32_t,configured_ip.netmask.addr,inet_addr("255.255.0.0"),"netmask.network_order");
 BAD(bool,configured_lease.enable,false,"lease.enable");
 BAD(uint32_t,configured_lease.start_ip.addr,inet_addr("192.168.4.3"),"lease.start.network_order");
 BAD(uint32_t,configured_lease.end_ip.addr,inet_addr("192.168.4.50"),"lease.end.network_order");
 ap_never_ready=true;BAD(int,dhcp,0,"dhcp.status");ap_never_ready=false;
#undef BAD
 const char *all_getters[]={"channel","bandwidth","mode","config","stations","mac","protocol","power","country","ip","dhcp","lease"};
 const char *ops[]={"esp_wifi_get_channel","esp_wifi_get_bandwidth","esp_wifi_get_mode","esp_wifi_get_config","esp_wifi_ap_get_sta_list","esp_wifi_get_mac","esp_wifi_get_protocol","esp_wifi_get_max_tx_power","esp_wifi_get_country","esp_netif_get_ip_info","esp_netif_dhcps_get_status","esp_netif_dhcps_option(GET)"};
 for(size_t i=0;i<sizeof(all_getters)/sizeof(all_getters[0]);i++) {
  getter_failure=all_getters[i];radio_logs[0]=0;
  assert(tx_network_check(&fixture)==ESP_FAIL);
  assert(strstr(last_error,ops[i]) && strstr(radio_logs,"esp_netif_dhcps_option(GET)"));
  assert(strstr(radio_logs,"unavailable") && !strstr(radio_logs,fixture.wifi_password));
  getter_error=ESP_ERR_INVALID_STATE;radio_logs[0]=0;
  assert(tx_network_check(&fixture)==ESP_ERR_INVALID_STATE && strstr(last_error,ops[i]));
  getter_error=ESP_FAIL;
 }
 getter_failure=NULL;
 radio_logs[0]=0;assert(tx_network_check(&fixture)==ESP_OK);
 assert(strstr(radio_logs,"country.cc[0] expected_raw=0x44 actual_raw=0x44"));
 assert(strstr(radio_logs,"country.cc[1] expected_raw=0x45 actual_raw=0x45"));
 assert(strstr(radio_logs,"country.cc[2] expected_raw=0x00 actual_raw=0x20"));
 fixture.country[2]='I';assert(!tx_config_validate(&fixture));fixture.country[2]=0;
 fixture.country[0]='d';assert(!tx_config_validate(&fixture));fixture.country[0]='D';
 fixture.country_channels=9;fixture.secondary_channel=WIFI_SECOND_CHAN_ABOVE;
 assert(!tx_config_validate(&fixture));fixture.country_channels=13;fixture.secondary_channel=WIFI_SECOND_CHAN_BELOW;
 puts("TX semantic country/SSID/field diagnostics and all 12 network getter tests passed (simulated SDK inputs)");
}

static void power_contract_checks(void)
{
 tx_network_stop();radio_logs[0]=0;
 assert(tx_network_start(&fixture)==ESP_OK);
 assert(power==44 && configured_country.max_tx_power==20);
 assert(strstr(radio_logs,"phase=after_country_set") && strstr(radio_logs,"power_read=0"));
 assert(strstr(radio_logs,"phase=after_power_set") && strstr(radio_logs,"country_dBm=20"));
 assert(strstr(radio_logs,"power_read=1 power_rc=0x0 power_quarter_dBm=44"));
 /* A repeated power request must not silently repair/change the country fake. */
 configured_country.max_tx_power=21;
 assert(esp_wifi_set_max_tx_power(44)==ESP_OK && configured_country.max_tx_power==21);
 expect_mismatch("country.max_tx_power_dBm");
 const int8_t bad_power[]={43,45,52,80,-128,127};
 for(size_t i=0;i<sizeof(bad_power);i++) {
  tx_network_stop();observed_power_after_set=bad_power[i];radio_logs[0]=0;
  assert(tx_network_start(&fixture)==ESP_ERR_INVALID_STATE && !started);
  assert(strstr(radio_logs,"field=power_quarter_dBm") && configured_country.max_tx_power==20);
 }
 observed_power_after_set=44;
 const int8_t bad_country[]={11,19,21,-128,127};
 for(size_t i=0;i<sizeof(bad_country);i++) {
  observed_country_after_set=bad_country[i];radio_logs[0]=0;
  assert(tx_network_start(&fixture)==ESP_ERR_INVALID_STATE && !started);
  assert(strstr(radio_logs,"field=country.max_tx_power_dBm") && power==44);
 }
 observed_country_after_set=20;
 const char *getters[]={"country","power"};
 const char *ops[]={"esp_wifi_get_country","esp_wifi_get_max_tx_power"};
 for(size_t i=0;i<2;i++) {
  getter_failure=getters[i];
  for(size_t j=0;j<2;j++) {
   getter_error=j?ESP_ERR_INVALID_STATE:ESP_FAIL;radio_logs[0]=0;
   assert(tx_network_start(&fixture)==getter_error && !started);
   assert(strstr(radio_logs,ops[i]) && strstr(radio_logs,"unavailable=-1"));
   assert(strstr(radio_logs,i?"power_quarter_dBm=-1":"country_dBm=-1"));
  }
 }
 getter_failure=NULL;getter_error=ESP_FAIL;
 for(unsigned failure=2;failure<=3;failure++) {
  fail_country_read=failure;radio_logs[0]=0;
  assert(tx_network_start(&fixture)==ESP_FAIL && !started);
  assert(strstr(radio_logs,"phase=after_country_set") && strstr(radio_logs,"country_dBm=20"));
  assert(strstr(radio_logs,"esp_wifi_get_country") && strstr(radio_logs,"unavailable"));
 }
 fail_country_read=0;
 /* The immediate post-set sample can differ from DHCP-ready state. */
 change_country_at_ap_ready=true;radio_logs[0]=0;clock_step=10;
 assert(tx_network_start(&fixture)==ESP_ERR_INVALID_STATE && !started);
 assert(strstr(radio_logs,"phase=after_power_set") && strstr(radio_logs,"country_dBm=20"));
 assert(strstr(radio_logs,"field=country.max_tx_power_dBm expected=20 actual=21"));
 change_country_at_ap_ready=false;change_power_at_ap_ready=true;radio_logs[0]=0;
 assert(tx_network_start(&fixture)==ESP_ERR_INVALID_STATE && !started);
 assert(strstr(radio_logs,"power_quarter_dBm=44") && strstr(radio_logs,"expected=44 actual=52"));
 change_power_at_ap_ready=false;clock_us=0;clock_step=0;
 /* A different country's table contract is not guessed from the DE result. */
 memcpy(fixture.country,"US",3);radio_logs[0]=0;
 assert(tx_network_start(&fixture)==ESP_ERR_INVALID_STATE && !started);
 assert(strstr(radio_logs,"country.power_contract_DE_1_13"));
 memcpy(fixture.country,"DE",3);
 fixture.country_channels=12;radio_logs[0]=0;
 assert(tx_network_start(&fixture)==ESP_ERR_INVALID_STATE && !started);
 assert(strstr(radio_logs,"country.power_contract_DE_1_13"));fixture.country_channels=13;
 radio_logs[0]=0;assert(tx_network_start(&fixture)==ESP_OK);
 assert(!strstr(radio_logs,fixture.wifi_password) && !strstr(radio_logs,fixture.ota_psk));
 puts("TX independent 44/20 observations, bad power/country, getter errors and temporal changes passed");
}

int main(int argc, char **argv)
{
 fixture=(tx_config_t){.schema_version=1,.node_id="TX1",.wifi_ssid="fixture-network",.wifi_password="12345678",
 .base_mac={2,1,2,3,4,5},.ap_mac={2,9,8,7,6,5},.channel=6,.rate_hz=50,.bandwidth_mhz=40,
 .secondary_channel=WIFI_SECOND_CHAN_BELOW,
 .protocol_mask=7,.beacon_interval=100,.country="DE",.country_channels=13,.led_gpio=48};
 memset(fixture.ota_psk,'a',64);memcpy(hw_mac,fixture.base_mac,6);
 if(argc==2 && strcmp(argv[1],"--status-json")==0) {
  memcpy(configured_mac,fixture.ap_mac,6);station_count=2;runtime.station_generation=5;
  memcpy(station_macs[0],(uint8_t[]){2,0,0,0,0,1},6);memcpy(station_macs[1],(uint8_t[]){2,0,0,0,0,2},6);
  snprintf(auth_header,sizeof(auth_header),"Bearer %s",fixture.ota_psk);clock_step=13;
  assert(tx_ota_start(1,&fixture,"fixture-source",provider)==ESP_OK);
  httpd_req_t request={0};assert(status_request(&request)==ESP_OK);puts(response_body);return 0;
 }
 if(argc==2 && strcmp(argv[1],"--power-regression")==0) {
  esp_err_t result=tx_network_start(&fixture);
  printf("Independent observation: power=%d country=%d result=0x%x\n",power,configured_country.max_tx_power,(unsigned)result);
  fflush(stdout);assert(power==44 && configured_country.max_tx_power==20);
  assert(result==ESP_OK);return 0;
 }
 if(argc==2 && strcmp(argv[1],"--power-tests")==0) {
  power_contract_checks();return 0;
 }
 if(argc==2 && strcmp(argv[1],"--country-regression")==0) {
  esp_err_t result=tx_network_start(&fixture);
  printf("country regression: profile_cc2=0x%02x getter_cc2=0x%02x result=0x%x\n",
         (unsigned char)fixture.country[2],(unsigned char)configured_country.cc[2],(unsigned)result);
  fflush(stdout);assert(memcmp(configured_country.cc,fixture.country,3)!=0);assert(result==ESP_OK);
  return 0;
 }
 if(argc==2 && strcmp(argv[1],"--ssid-regression")==0) {
  normalize_country=false;assert(tx_network_start(&fixture)==ESP_OK);
  configured_ap.ap.ssid_len=0;esp_err_t result=tx_network_check(&fixture);
  printf("SSID regression: explicit=0 effective_length=%zu result=0x%x\n",strlen(fixture.wifi_ssid),(unsigned)result);
  fflush(stdout);assert(result==ESP_OK);return 0;
 }
 if(argc==2 && strcmp(argv[1],"--status-getter-regression")==0) {
  getter_failure="image_digest";
  esp_err_t result=tx_ota_start(1,&fixture,"fixture-source",provider);
  printf("Status regression: failed first image verification; start result=0x%x\n",(unsigned)result);
  fflush(stdout);assert(result==ESP_FAIL && s_verified_running==NULL);return 0;
 }
 tx_config_t loaded;
 assert(tx_config_init(&loaded)==0&&loaded.led_gpio==48);
 init_error=ESP_ERR_NVS_NO_FREE_PAGES;assert(tx_config_init(&loaded)==init_error);
 init_error=ESP_ERR_NVS_NEW_VERSION_FOUND;assert(tx_config_init(&loaded)==init_error);init_error=0;
 const char *keys[]={"schema_v","node_id","ssid","password","base_mac","ap_mac","channel","rate_hz","bw_mhz","protocol","secondary","beacon_tu","country","country_nch","led_gpio","led_active_low","ota_psk"};
 for(size_t i=0;i<sizeof(keys)/sizeof(keys[0]);i++){missing_key=keys[i];assert(tx_config_init(&loaded)!=0);}missing_key=NULL;
 fixture.led_gpio=-1;assert(tx_config_load(&loaded)==0&&loaded.led_gpio==-1);fixture.led_gpio=48;
 fixture.schema_version=2;assert(!tx_config_validate(&fixture));fixture.schema_version=1;
 fixture.wifi_password[0]=0;assert(tx_network_start(&fixture)!=0&&started==0);strcpy(fixture.wifi_password,"12345678");
 hw_mac[5]++;assert(tx_network_start(&fixture)!=0&&started==0);hw_mac[5]--;
 assert(tx_network_start(&fixture)==0&&power==44);
 assert(strcmp(radio_order,"CBSHE")==0);
 assert(tx_network_check(&fixture)==ESP_OK);
 semantic_checks();
 power_contract_checks();
 channel=7;assert(tx_network_check(&fixture)==ESP_ERR_INVALID_STATE&&strstr(last_error,"CHECK_PRIMARY_CHANNEL"));channel=6;
 secondary=WIFI_SECOND_CHAN_ABOVE;
 assert(tx_network_check(&fixture)==ESP_ERR_INVALID_STATE&&strstr(last_error,"CHECK_SECONDARY_CHANNEL"));
 secondary=99;assert(tx_network_check(&fixture)==ESP_ERR_INVALID_STATE);
 /* HT40 interface setting != current device channel width. No client count
    is used to predict what a real driver's getter must return. */
 secondary=WIFI_SECOND_CHAN_NONE;
 for(int n=0;n<=2;n++) {
  station_count=n;radio_logs[0]=0;
  assert(tx_network_check(&fixture)==ESP_OK);
  assert(strstr(radio_logs,"HT20_OBSERVED_HT40_UNPROVEN"));
 }
 station_count=0;secondary=WIFI_SECOND_CHAN_BELOW;
 fixture.secondary_channel=WIFI_SECOND_CHAN_ABOVE;
 assert(tx_network_check(&fixture)==ESP_ERR_INVALID_STATE);
 secondary=WIFI_SECOND_CHAN_ABOVE;assert(tx_network_check(&fixture)==ESP_OK);
 fixture.secondary_channel=WIFI_SECOND_CHAN_BELOW;secondary=WIFI_SECOND_CHAN_BELOW;
 configured_ap.ap.channel=7;assert(tx_network_check(&fixture)==ESP_ERR_INVALID_STATE&&strstr(last_error,"CHECK_CONFIG_CHANNEL"));configured_ap.ap.channel=6;
 bandwidth=WIFI_BW_HT20;assert(tx_network_check(&fixture)==ESP_ERR_INVALID_STATE&&strstr(last_error,"CHECK_BANDWIDTH"));
 fixture.bandwidth_mhz=20;fixture.secondary_channel=WIFI_SECOND_CHAN_NONE;
 assert(tx_network_check(&fixture)==ESP_ERR_INVALID_STATE);secondary=WIFI_SECOND_CHAN_NONE;
 assert(tx_network_check(&fixture)==ESP_OK);
 fixture.bandwidth_mhz=40;fixture.secondary_channel=WIFI_SECOND_CHAN_BELOW;bandwidth=WIFI_BW_HT40;secondary=WIFI_SECOND_CHAN_BELOW;
 wifi_mode=WIFI_MODE_APSTA;assert(tx_network_check(&fixture)==ESP_ERR_INVALID_STATE&&strstr(last_error,"CHECK_MODE"));wifi_mode=WIFI_MODE_AP;
 const char *getters[]={"channel","bandwidth","mode","config","stations"};
 const char *unavailable[]={"primary=-1 secondary=-1","interface_bw=-1","mode=-1","configured_primary=-1 max_clients=-1","clients=-1"};
 for(size_t i=0;i<sizeof(getters)/sizeof(getters[0]);i++) {
  getter_failure=getters[i];radio_logs[0]=0;
  assert(tx_network_check(&fixture)==ESP_FAIL&&strstr(radio_logs,unavailable[i]));
  assert(!strstr(radio_logs,fixture.wifi_password)&&!strstr(radio_logs,fixture.wifi_ssid)&&!strstr(radio_logs,fixture.ota_psk));
 }
 getter_error=ESP_ERR_INVALID_STATE;getter_failure="channel";radio_logs[0]=0;
 assert(tx_network_check(&fixture)==ESP_ERR_INVALID_STATE&&strstr(radio_logs,"primary=-1 secondary=-1"));
 getter_failure=NULL;getter_error=ESP_FAIL;
 clock_us+=1234567;radio_logs[0]=0;assert(tx_network_check(&fixture)==ESP_OK);
 assert(strstr(radio_logs,"since_start_us=1234567"));
 network_event(NULL,WIFI_EVENT,WIFI_EVENT_AP_START,NULL);
 assert(strstr(last_log,"TX_RADIO event=")&&strstr(last_log,"boot_us="));
 assert(configured_lease.start_ip.addr==inet_addr("192.168.4.2"));
 assert(configured_lease.end_ip.addr==inet_addr("192.168.4.49"));
 configured_lease.end_ip.addr=inet_addr("192.168.4.50");assert(tx_network_check(&fixture)!=0);
 assert(tx_network_restart(&fixture)==0);
 assert(strcmp(radio_order,"CBSHESHE")==0); /* set_channel repeated after restart */
 tx_network_stop();station_count=1;radio_logs[0]=0;
 assert(tx_network_start(&fixture)==ESP_ERR_INVALID_STATE&&!started&&strstr(last_log,"SET_CHANNEL_CLIENTS_PRESENT"));
 assert(strstr(radio_logs,"op=esp_wifi_get_channel")&&strstr(radio_logs,"op=esp_wifi_get_mode"));
 assert(strcmp(radio_order,"CBS")==0);station_count=0;
 getter_failure="stations";radio_logs[0]=0;
 assert(tx_network_start(&fixture)==ESP_FAIL&&!started&&strstr(last_log,"esp_wifi_ap_get_sta_list"));
 assert(strstr(radio_logs,"op=esp_wifi_get_channel")&&strstr(radio_logs,"clients=-1"));getter_failure=NULL;
 event_on_start=true;assert(tx_network_start(&fixture)==ESP_OK);
 assert(strcmp(radio_order,"CBSEH")==0);tx_network_stop();event_on_start=false;
 tx_network_stop();calls=0;assert(tx_network_start(&fixture)==0);int successful_calls=calls;tx_network_stop();
 for(int i=1;i<=successful_calls;i++){calls=0;fail_call=i;error_logs=0;assert(tx_network_start(&fixture)!=0);assert(!started);assert(error_logs>0);}
 fail_call=3;calls=0;radio_logs[0]=0;
 assert(tx_network_start(&fixture)==ESP_ERR_NO_MEM && !started);
 assert(strstr(radio_logs,"op=esp_netif_new rc=0x101 field=allocated_ap_netif"));
 fail_call=0;calls=0;
 ap_ready_after=5;assert(tx_network_start(&fixture)==ESP_OK&&ap_status_reads>=6);
 power=0;assert(tx_network_check(&fixture)!=ESP_OK&&strstr(last_error,"CHECK_TX_POWER"));
 power=44;configured_mac[5]++;assert(tx_network_check(&fixture)!=ESP_OK&&strstr(last_error,"CHECK_AP_MAC"));configured_mac[5]--;
 tx_network_stop();ap_never_ready=true;assert(tx_network_start(&fixture)==ESP_ERR_TIMEOUT&&!started);
 assert(strstr(last_log,"AP_START_TIMEOUT")&&ap_status_reads==0);
 ap_never_ready=false;ap_ready_after=0;
 tx_boot_check_t check;runtime=(tx_runtime_status_t){0};tx_boot_begin(&check,0,0,&runtime);
 for(uint64_t t=5000000;t<=60000000;t+=5000000){runtime.sent_packets+=250;assert(tx_boot_poll(&check,t,&runtime,true,64000)==(t==60000000?1:0));}
 tx_boot_begin(&check,0,0,&runtime);assert(tx_boot_poll(&check,5000000,&runtime,true,64000)==-1);
 tx_boot_begin(&check,0,0,&runtime);assert(tx_boot_poll(&check,120000000,&runtime,true,64000)==-1);
 tx_boot_begin(&check,0,0,&runtime);assert(tx_boot_poll(&check,1,&runtime,false,64000)==-1);
 tx_boot_begin(&check,0,0,&runtime);assert(tx_boot_poll(&check,1,&runtime,true,100)==-1);
 runtime.send_errors=1;tx_boot_begin(&check,0,0,&runtime);assert(tx_boot_poll(&check,1,&runtime,true,64000)==-1);runtime.send_errors=0;
 /* Boot predicate boundaries and concrete field diagnostics. */
 runtime=(tx_runtime_status_t){0};tx_boot_begin(&check,10,10,&runtime);radio_logs[0]=0;
 assert(tx_boot_poll(&check,9,&runtime,true,64000)==-1 && strstr(radio_logs,"field=monotonic_time"));
 tx_boot_begin(&check,0,0,&runtime);runtime.sent_packets=1;radio_logs[0]=0;
 assert(tx_boot_poll(&check,6000001,&runtime,true,64000)==-1 && strstr(radio_logs,"field=sample_gap_us"));
 tx_boot_begin(&check,0,0,&runtime);runtime.sent_packets++;radio_logs[0]=0;
 assert(tx_boot_poll(&check,6000000,&runtime,true,32768)==0); /* inclusive limits */
 tx_boot_begin(&check,0,0,&runtime);radio_logs[0]=0;
 assert(tx_boot_poll(&check,1,&runtime,true,32767)==-1 && strstr(radio_logs,"field=free_heap_bytes"));
 runtime.wifi_restarts=1;tx_boot_begin(&check,0,0,&runtime);radio_logs[0]=0;
 assert(tx_boot_poll(&check,1,&runtime,true,64000)==-1 && strstr(radio_logs,"field=wifi_restarts"));runtime.wifi_restarts=0;
 runtime.send_errors=1;tx_boot_begin(&check,0,0,&runtime);radio_logs[0]=0;
 assert(tx_boot_poll(&check,1,&runtime,true,64000)==-1 && strstr(radio_logs,"field=send_errors"));runtime.send_errors=0;
 tx_boot_begin(&check,0,0,&runtime);radio_logs[0]=0;
 assert(tx_boot_poll(&check,5000000,&runtime,true,64000)==-1 && strstr(radio_logs,"field=sent_packets_delta"));
 tx_boot_begin(&check,0,0,&runtime);radio_logs[0]=0;
 assert(tx_boot_poll(&check,1,&runtime,false,64000)==-1 && strstr(radio_logs,"field=network_healthy"));
 assert(tx_ota_start(1,&fixture,"fixture-source",provider)==0);
 httpd_req_t request={.content_len=64};runtime.boot_check=1;
 assert(ota_handler(&request)!=0&&writes==0&&response_code==403);
 snprintf(auth_header,sizeof(auth_header),"Bearer %s",fixture.ota_psk);
 auth_header[70]='b';assert(ota_handler(&request)!=0&&writes==0);
 snprintf(auth_header,sizeof(auth_header),"Bearer %s",fixture.ota_psk);
 runtime.boot_check=0;assert(ota_handler(&request)!=0&&writes==0);runtime.boot_check=1;
 assert(ota_handler(&request)!=0&&writes==0); /* original app0 protected */
 next.subtype=17;request.content_len=next.size+1;assert(ota_handler(&request)!=0&&writes==0);request.content_len=64;
 begin_error=ESP_FAIL;assert(ota_handler(&request)!=0&&selected==0);begin_error=0;
 receive_result=-1;assert(ota_handler(&request)!=0&&aborted==1&&selected==0);receive_result=64;
 write_error=ESP_FAIL;assert(ota_handler(&request)!=0&&aborted==2);write_error=0;
 end_error=ESP_FAIL;assert(ota_handler(&request)!=0&&selected==0);end_error=0;
 clock_step=120000000;assert(ota_handler(&request)!=0&&selected==0);clock_step=0;
 select_error=ESP_FAIL;assert(ota_handler(&request)!=0&&restarted==0);select_error=0;
 assert(ota_handler(&request)==0&&selected==2&&restarted==1);
 assert(status_request(&request)==ESP_OK);assert(!strstr(response_body,fixture.ota_psk)&&!strstr(response_body,fixture.wifi_password));
 assert(strstr(response_body,"\"status_resources\":{\"scope\":\"non_atomic_http_task_sample\""));
 assert(strstr(response_body,"\"http_task_stack_allocated_bytes\":8192"));
 assert(strstr(response_body,"\"http_task_stack_low_watermark_bytes\":3000"));
 assert(strstr(response_body,"\"heap_internal_8bit_free_bytes\":61000"));
 assert(strstr(response_body,"\"heap_internal_8bit_largest_block_bytes\":50000"));
 assert(strstr(response_body,"\"heap_internal_8bit_minimum_free_bytes\":59000"));
 /* Independent resource samples must not reuse old request values. */
 http_stack_low=2500;internal_free=60000;internal_largest=48000;internal_minimum=58000;
 assert(status_request(&request)==ESP_OK);
 assert(strstr(response_body,"\"http_task_stack_low_watermark_bytes\":2500"));
 assert(strstr(response_body,"\"heap_internal_8bit_free_bytes\":60000"));
 assert(strstr(response_body,"\"heap_internal_8bit_largest_block_bytes\":48000"));
 assert(strstr(response_body,"\"heap_internal_8bit_minimum_free_bytes\":58000"));

 /* Status identity getters must not turn failed reads into invented values. */
 const char *identity_getters[]={"ota_state"};
 for(size_t i=0;i<1;i++) {
  getter_failure=identity_getters[i];radio_logs[0]=0;response_code=0;
  assert(status_request(&request)==ESP_FAIL && response_code==500);
  assert(strstr(radio_logs,"actual=unavailable") && !strstr(response_body,"image_sha256"));
 }
 getter_failure=NULL;running_missing=true;response_code=0;
 assert(status_request(&request)==ESP_ERR_INVALID_STATE && response_code==500);running_missing=false;
 ota_state=ESP_OTA_IMG_UNDEFINED;assert(status_request(&request)==ESP_OK && strstr(response_body,"UNDEFINED"));ota_state=ESP_OTA_IMG_PENDING_VERIFY;
 /* The running image is fully verified once before the sender starts, while
    OTA state and the authorized station list remain fresh on every GET. */
 unsigned digest_reads_before=image_digest_reads;
 for(int i=0;i<4;i++)assert(status_request(&request)==ESP_OK);
 assert(image_digest_reads==digest_reads_before);
 assert(strstr(response_body,"verified_once_this_boot"));
 getter_failure="image_digest";assert(status_request(&request)==ESP_OK); /* no repeated flash scan */
 assert(tx_ota_start(2,&fixture,"fixture-source",provider)==ESP_FAIL && s_verified_running==NULL);
 assert(status_request(&request)==ESP_ERR_INVALID_STATE);getter_failure=NULL;
 assert(tx_ota_start(2,&fixture,"fixture-source",provider)==ESP_OK && image_digest_reads==digest_reads_before+2);
 assert(status_request(&request)==ESP_OK && strstr(response_body,"0000000000000002"));
 const esp_partition_t *verified=s_verified_running;s_verified_running=&next;
 assert(status_request(&request)==ESP_ERR_INVALID_STATE);s_verified_running=verified;
 /* Unauthorized status never exposes raw AP/station MACs or queries the list. */
 auth_header[0]=0;unsigned station_reads_before=station_list_reads;
 assert(status_request(&request)==ESP_OK && station_list_reads==station_reads_before);
 assert(strstr(response_body,"authorization_required") && strstr(response_body,"\"stations\":null"));
 assert(strstr(response_body,"\"last_enomem\":null"));
#ifdef TX_TEST_DIAGNOSTIC_JSON
 puts(response_body);
#endif
 const char *denied_status_headers[]={"Bearer wrong","Basic wrong","bearer wrong"};
 for(size_t i=0;i<sizeof(denied_status_headers)/sizeof(denied_status_headers[0]);i++) {
  strlcpy(auth_header,denied_status_headers[i],sizeof(auth_header));
  assert(status_request(&request)==ESP_OK && station_list_reads==station_reads_before);
  assert(strstr(response_body,"authorization_required") && strstr(response_body,"\"ap_mac\":null"));
 }
 assert(!request_headers_available);

 snprintf(auth_header,sizeof(auth_header),"Bearer %s",fixture.ota_psk);
 station_count=0;clock_step=13;runtime.station_generation=10;
 assert(status_request(&request)==ESP_OK && strstr(response_body,"\"status\":\"current\"") && strstr(response_body,"\"station_count\":0,\"stations\":[]"));
 assert(strstr(response_body,"\"generation_before\":10,\"generation_after\":10"));
 station_count=2;memcpy(station_macs[0],(uint8_t[]){2,0,0,0,0,1},6);memcpy(station_macs[1],(uint8_t[]){2,0,0,0,0,2},6);
 assert(status_request(&request)==ESP_OK && strstr(response_body,"020000000001") && strstr(response_body,"020000000002"));
 memcpy(station_macs[1],(uint8_t[]){2,0,0,0,0,3},6);runtime.station_generation++;
 assert(status_request(&request)==ESP_OK && strstr(response_body,"020000000003") && !strstr(response_body,"020000000002"));
 change_generation_during_list=true;
 assert(status_request(&request)==ESP_OK && strstr(response_body,"\"status\":\"changed\""));change_generation_during_list=false;
 getter_failure="stations";
 assert(status_request(&request)==ESP_OK && strstr(response_body,"\"status\":\"unavailable\"") && strstr(response_body,"\"stations\":null"));
 getter_failure="mac";
 assert(status_request(&request)==ESP_OK && strstr(response_body,"\"ap_mac\":null"));getter_failure=NULL;
 station_count=16;assert(status_request(&request)==ESP_OK && strstr(response_body,"\"stations\":null"));
 station_count=15;assert(status_request(&request)==ESP_OK && strstr(response_body,"\"station_count\":15"));station_count=0;clock_step=0;
 runtime.last_send_error=(tx_send_error_t){.operation=TX_SEND_OP_SENDTO,.result=3,.sequence=4,.occurred_us=123,.errno_valid=false};runtime.short_sends=1;
 assert(status_request(&request)==ESP_OK && strstr(response_body,"\"return_value\":3") && strstr(response_body,"\"errno\":null"));
 runtime.last_send_error=(tx_send_error_t){.operation=TX_SEND_OP_SOCKET,.result=-1,.error_number=24,.errno_valid=true};
 assert(status_request(&request)==ESP_OK && strstr(response_body,"\"operation\":\"socket\"") && strstr(response_body,"\"errno\":24"));
 runtime.send_path=(tx_send_path_status_t){.own_attempts=UINT32_MAX,.netbuf_calls=UINT32_MAX,
  .wifi_handoffs=UINT32_MAX,.enomem_netbuf_allocation=UINT32_MAX,.enomem_wifi_handoff=UINT32_MAX,
  .enomem_other_or_unclassified=UINT32_MAX};
 for(int stage=TX_SEND_PATH_NETBUF_ALLOCATION;stage<=TX_SEND_PATH_UNCLASSIFIED;stage++){
  runtime.send_path.last_enomem=(tx_send_path_error_t){.stage=(tx_send_path_stage_t)stage,
   .sequence=UINT32_MAX,.capture_begin_us=UINT64_MAX-1,.capture_end_us=UINT64_MAX,
   .netbuf_calls=stage==TX_SEND_PATH_UNCLASSIFIED?0:1,
   .netbuf_returned_nonnull=stage==TX_SEND_PATH_WIFI_HANDOFF,
   .wifi_calls=stage==TX_SEND_PATH_WIFI_HANDOFF?1:0,.wifi_return=ESP_ERR_NO_MEM};
  assert(status_request(&request)==ESP_OK && strstr(response_body,"\"own_attempts\":4294967295"));
  const char *classification=stage==TX_SEND_PATH_NETBUF_ALLOCATION?"netbuf_allocation":
   stage==TX_SEND_PATH_WIFI_HANDOFF?"wifi_handoff":"other_or_unclassified";
  assert(strstr(response_body,classification));
  assert(strstr(response_body,stage==TX_SEND_PATH_WIFI_HANDOFF?"\"wifi_return\":257":"\"wifi_return\":null"));
  assert(strstr(response_body,"\"http_task_stack_allocated_bytes\":8192"));
#ifdef TX_TEST_DIAGNOSTIC_JSON
  puts(response_body);
#endif
 }
 runtime.send_path=(tx_send_path_status_t){0};
#ifdef CONFIG_TX_VERIFIED_ROLLBACK
 getter_failure="ota_state";radio_logs[0]=0;
 assert(tx_ota_complete_boot_self_test(true)==ESP_FAIL && mark_valid==0 && strstr(last_error,"esp_ota_get_state_partition"));getter_failure=NULL;
 ota_state=ESP_OTA_IMG_VALID;assert(tx_ota_complete_boot_self_test(true)==ESP_OK && mark_valid==0);ota_state=ESP_OTA_IMG_PENDING_VERIFY;
 assert(tx_ota_complete_boot_self_test(false)!=0&&mark_invalid==0);rollback_possible=true;
 assert(tx_ota_complete_boot_self_test(false)==0&&mark_invalid==1);
 assert(tx_ota_complete_boot_self_test(true)==0&&mark_valid==1);
#else
 assert(tx_ota_complete_boot_self_test(false)!=0);
 assert(tx_ota_complete_boot_self_test(true)==0&&mark_valid==0&&mark_invalid==0);
#endif
 puts("TX runtime contract tests passed");
 return 0;
}

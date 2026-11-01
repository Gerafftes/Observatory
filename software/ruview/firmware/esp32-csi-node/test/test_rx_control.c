/* Compile the real HTTP handlers with IDF's actual cJSON parser. */
#include "../main/rx_control.c"
#include "esp_stubs.h"
#include <assert.h>

static nvs_config_t stored_config, active_config;
static rx_profile_t stored_profile, active_profile;
static unsigned stores, routes;
static bool load_failed;
const uint8_t *csi_peer_self_mac(void) { static const uint8_t mac[6]={2,0,0,0,3,1}; return mac; }

const rx_profile_t *rx_settings_active_profile(void) { return &active_profile; }
const nvs_config_t *rx_settings_active_config(void) { return &active_config; }
esp_err_t rx_settings_load(nvs_config_t *c, rx_profile_t *p)
{ if(load_failed) return ESP_FAIL; *c=stored_config; *p=stored_profile; return ESP_OK; }
esp_err_t rx_settings_store(const nvs_config_t *c, const rx_profile_t *p)
{ assert(rx_settings_valid(c,p)); ++stores; stored_config=*c; stored_profile=*p; return ESP_OK; }
bool rx_settings_hash(const nvs_config_t *c, const rx_profile_t *p, uint8_t out[32])
{ assert(rx_settings_valid(c,p)); memset(out,0xa7,32); return true; }
bool ota_check_auth(httpd_req_t *req) { return req->authenticated; }
esp_err_t diag_http_send(httpd_req_t *req) { (void)req; return ESP_OK; }
rx_http_scope_t rx_http_enter(unsigned phase) { return (rx_http_scope_t){phase}; }
void rx_http_leave(rx_http_scope_t *scope) { scope->phase=0; }
int httpd_req_recv(httpd_req_t *req, char *buffer, size_t size)
{
    if(req->fail_receive) return -1;
    size_t remaining=strlen(req->body)-req->received;
    if(size>remaining) size=remaining;
    memcpy(buffer,req->body+req->received,size); req->received+=size; return size;
}
esp_err_t httpd_resp_send_err(httpd_req_t *req, int status, const char *reason)
{ assert(reason); req->response_error=status; return ESP_OK; }
esp_err_t httpd_resp_send(httpd_req_t *req, const char *data, int size)
{
    (void)req; assert(data && (size>0 || size==HTTPD_RESP_USE_STRLEN));
    cJSON *j=cJSON_Parse(data); assert(j); cJSON_Delete(j); return ESP_OK;
}
esp_err_t httpd_resp_set_type(httpd_req_t *req, const char *type)
{ (void)req; assert(!strcmp(type,"application/json")); return ESP_OK; }
esp_err_t httpd_register_uri_handler(httpd_handle_t server, const httpd_uri_t *uri)
{ assert(server && uri && uri->uri && uri->handler); ++routes; return ESP_OK; }
esp_err_t httpd_req_get_hdr_value_str(httpd_req_t *req, const char *name, char *out, size_t size)
{ assert(!strcmp(name,"X-RX-Device-ID") && size==65); assert(receiver_id(out)); if(req->wrong_device) out[0]='z'; return ESP_OK; }

static int post(const char *body, bool auth)
{
    httpd_req_t req={.body=body,.content_len=(int)strlen(body),.authenticated=auth};
    assert(config_post(&req)==ESP_OK); return req.response_error;
}

int main(void)
{
    active_config=(nvs_config_t){.wifi_ssid="test",.wifi_password="test-secret",.target_ip="192.0.2.5",
        .target_port=5005,.node_id=1,.filter_mac_set=1,.filter_mac={2,0,0,0,1,1},.csi_channel=6,.tdm_node_count=1};
    active_profile=(rx_profile_t){.generation=1,.node_id=1,.led_gpio=48,.rate_hz=50,.tx_mac={2,0,0,0,1,1}};
    stored_config=active_config; stored_profile=active_profile;
    esp_test_set_time_step_us(0); esp_test_set_time_us(1000000);
    assert(rx_control_register((void *)1)==ESP_OK && routes==5);
    const char *bad[]={
        "{}", "[]", "{\"expected_generation\":1.5}", "{\"expected_generation\":0}",
        "{\"expected_generation\":1,\"target_port\":6000,\"led_gpio\":47}",
        "{\"expected_generation\":1,\"target_port\":6000,\"target_port\":7000}",
        "{\"expected_generation\":1,\"target_port\":6000,\"unknown\":1}",
        "{\"expected_generation\":1,\"target_ip\":\"1.2.3.999\"}",
        "{\"expected_generation\":1,\"mode\":\"pair-trial\"}",
        "{\"expected_generation\":1,\"mode\":\"tx-only\",\"peers\":[{\"node_id\":2,\"mac\":\"02:00:00:00:02:02\"}]}",
        "{\"expected_generation\":1,\"tx_mac\":\"+2:00:00:00:01:01\"}",
        "{\"expected_generation\":1,\"password\":\"short\"}",
        "{\"expected_generation\":1,\"rate_hz\":21}",
        "{\"expected_generation\":1,\"rate_hz\":51}",
        "{\"expected_generation\":1} trailing",
    };
    for(unsigned i=0;i<sizeof(bad)/sizeof(bad[0]);++i) assert(post(bad[i],true)==400);
    assert(stores==0 && stored_config.target_port==5005 && stored_profile.generation==1);
    httpd_req_t wrong={.authenticated=true,.wrong_device=true};
    assert(config_post(&wrong)==ESP_OK && wrong.response_error==400 && stores==0);
    assert(post("{\"expected_generation\":1,\"target_port\":6000}",false)==403 && stores==0);
    assert(post("{\"expected_generation\":1,\"target_port\":6000}",true)==0 && stores==1);
    assert(stored_profile.generation==2 && stored_config.target_port==6000 && active_config.target_port==5005);
    assert(!strcmp(stored_config.wifi_password,"test-secret"));
    assert(post("{\"expected_generation\":1,\"target_port\":7000}",true)==400 && stores==1);
    assert(post("{\"expected_generation\":2,\"mode\":\"pair-trial\",\"peers\":[{\"node_id\":4,\"mac\":\"02:00:00:00:02:04\"}]}",true)==0);
    assert(stored_profile.generation==3 && stored_profile.mode==RX_PAIR_TRIAL && stores==2);
    httpd_req_t lease={.body="{\"seconds\":60,\"owner\":\"test-run\"}",.authenticated=true};
    lease.content_len=strlen(lease.body); assert(measurement(&lease)==ESP_OK && !lease.response_error);
    assert(rx_control_measurement_active());
    httpd_req_t other={.body="{\"seconds\":0,\"owner\":\"another-run\"}",.authenticated=true};
    other.content_len=strlen(other.body); measurement(&other); assert(other.response_error==400 && rx_control_measurement_active());
    assert(post("{\"expected_generation\":3,\"target_port\":7000}",true)==400 && stores==2);
    esp_test_advance_time_us(60000000); assert(!rx_control_measurement_active());
    load_failed=true; assert(post("{\"expected_generation\":3}",true)==400 && stores==2);
    puts("PASS: actual HTTP parsing/auth, whole-request validation before store, generation conflicts, secret preservation, pending config, measurement lease");
}

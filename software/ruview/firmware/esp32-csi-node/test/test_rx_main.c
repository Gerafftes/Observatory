/* Real app entry point: no device/network API exists in this harness. */
#include "../main/rx_main.c"
#include <assert.h>
#include <stdio.h>
static unsigned fault,peer_stops,transport_stops,socket_closes,failed,internal,self_tests;
static bool peer_running,transport_running,self_test_ready;
static nvs_config_t active;
static rx_profile_t profile;
esp_err_t rx_diag_init(void) {return fault==1?ESP_FAIL:ESP_OK;}
esp_err_t nvs_flash_init(void) {return fault==2?ESP_FAIL:ESP_OK;}
void nvs_config_load(nvs_config_t *out) {memset(out,0,sizeof(*out)); out->node_id=1;}
esp_err_t rx_settings_load(nvs_config_t *config,rx_profile_t *out)
{ (void)config; *out=(rx_profile_t){.generation=1,.node_id=1,.led_gpio=48,.rate_hz=50,.tx_mac={2,0,0,0,1,1}}; return fault==3?ESP_FAIL:ESP_OK; }
void rx_settings_activate(const nvs_config_t *config,const rx_profile_t *p) {active=*config;profile=*p;if(fault==4)profile.node_id=0;}
const rx_profile_t *rx_settings_active_profile(void) {return &profile;}
const nvs_config_t *rx_settings_active_config(void) {return &active;}
void esp_fill_random(void *out,size_t size) {memset(out,1,size);}
void csi_collector_set_node_id(uint8_t id) {assert(id==1);}
void csi_collector_set_boot_id(uint64_t id) {assert(id);}
esp_err_t status_led_start(uint8_t id) {assert(id==1);return ESP_OK;}
void status_led_internal_failed(void) {++internal;}
void status_led_init_failed(void) {++failed;}
esp_err_t rx_wifi_start(bool *connected) {*connected=true;if(fault==6)g_nvs_config.node_id=2;return fault==5?ESP_FAIL:ESP_OK;}
int stream_sender_init_with(const char *ip,uint16_t port) {(void)ip;(void)port;return 0;}
void stream_sender_deinit(void) {++socket_closes;}
esp_err_t csi_peer_start(void) {if(fault==7)return ESP_FAIL;peer_running=true;return ESP_OK;}
esp_err_t csi_transport_start(void) {if(fault==8)return ESP_FAIL;transport_running=true;return ESP_OK;}
void csi_peer_stop(void) {peer_running=false;++peer_stops;}
void csi_transport_stop(void) {transport_running=false;++transport_stops;}
esp_err_t csi_collector_init(void) {return fault==9?ESP_FAIL:ESP_OK;}
void csi_collector_notify_network_change(bool connected) {assert(!connected);}
esp_err_t ota_update_init_ex(void **server) {*server=(void *)1;return fault==10?ESP_FAIL:ESP_OK;}
esp_err_t rx_control_register(httpd_handle_t server) {assert(server==(void *)1);return fault==11?ESP_FAIL:ESP_OK;}
esp_err_t ota_update_complete_boot_self_test(bool ready) {++self_tests;self_test_ready=ready;return ESP_OK;}
int main(void) {
    for(fault=0;fault<=11;++fault) {
        peer_stops=transport_stops=socket_closes=failed=internal=self_tests=0;
        peer_running=transport_running=self_test_ready=false;
        app_main();
        if(fault==0) {assert(peer_running && transport_running && self_tests==1 && self_test_ready);}
        else {
            assert(!peer_running && !transport_running && !self_test_ready);
            if(fault==6) assert(internal==1);
            if(fault==7 || fault==8 || fault==10 || fault==11) assert(peer_stops && transport_stops && socket_closes);
            if(fault>=9) assert(self_tests==1);
        }
    }
    puts("PASS: real RX bootstrap halts invalid state and cleans partial peer/transport/control initialization");
}

#include "rx_settings.h"
#include "esp_stubs.h"
#include <assert.h>
#include <stdio.h>

static uint8_t stored[RX_SETTINGS_BYTES], pending[RX_SETTINGS_BYTES];
static bool present, corrupt, legacy_node=true, legacy_gpio=true;
static unsigned writes, opens_for_write;
static int commit_fault;
static uint8_t hash_input[RX_SETTINGS_BYTES];
static size_t hash_input_size;

esp_err_t nvs_open(const char *ns, int mode, nvs_handle_t *h)
{ assert(!strcmp(ns,"csi_cfg")); if (mode==NVS_READWRITE) ++opens_for_write; *h=1; return ESP_OK; }
void nvs_close(nvs_handle_t h) { assert(h==1); }
esp_err_t nvs_get_u8(nvs_handle_t h, const char *key, uint8_t *v)
{
    assert(h==1);
    if (!strcmp(key,"node_id") && legacy_node) { *v=1; return ESP_OK; }
    if (!strcmp(key,"led_gpio") && legacy_gpio) { *v=48; return ESP_OK; }
    return ESP_ERR_NVS_NOT_FOUND;
}
esp_err_t nvs_get_blob(nvs_handle_t h, const char *key, void *v, size_t *size)
{
    assert(h==1 && !strcmp(key,"rx_settings"));
    if (!present) return ESP_ERR_NVS_NOT_FOUND;
    assert(*size>=sizeof(stored)); memcpy(v,stored,sizeof(stored)); *size=sizeof(stored);
    if (corrupt) ((uint8_t *)v)[0]^=1;
    return ESP_OK;
}
esp_err_t nvs_set_blob(nvs_handle_t h, const char *key, const void *v, size_t size)
{
    /* The real store may only replace its own value. No erase API is linked. */
    assert(h==1 && !strcmp(key,"rx_settings") && size==sizeof(pending));
    ++writes; memcpy(pending,v,size); return ESP_OK;
}
esp_err_t nvs_commit(nvs_handle_t h)
{
    assert(h==1);
    if (commit_fault==1) return ESP_FAIL;
    memcpy(stored,pending,sizeof(stored)); present=true;
    return commit_fault==2 ? ESP_FAIL : ESP_OK;
}
/* Hash implementation is not under test here; firmware links IDF Mbed TLS. */
int mbedtls_sha256(const unsigned char *data, size_t size, unsigned char out[32], int is224)
{ assert(!is224 && size <= sizeof(hash_input)); memcpy(hash_input,data,size); hash_input_size=size; memset(out,1,32); return 0; }

int main(void)
{
    nvs_config_t c={.wifi_ssid="test",.wifi_password="test-secret",.target_ip="192.0.2.5",
        .target_port=5005,.node_id=1,.filter_mac_set=1,.filter_mac={2,0,0,0,1,1},
        .csi_channel=6,.tdm_node_count=1};
    rx_profile_t p;
    assert(rx_settings_load(&c,&p)==ESP_OK && p.generation==1 && p.node_id==1);
    assert(writes==0 && opens_for_write==0); /* Migration never writes. */
    legacy_node=false; assert(rx_settings_load(&c,&p)!=ESP_OK); legacy_node=true;
    legacy_gpio=false; assert(rx_settings_load(&c,&p)==ESP_OK && p.led_gpio==48); legacy_gpio=true;
    p.node_id=0; assert(rx_settings_store(&c,&p)==ESP_ERR_INVALID_ARG);
    assert(writes==0 && opens_for_write==0);
    assert(rx_settings_load(&c,&p)==ESP_OK);
    assert(rx_settings_store(&c,&p)==ESP_OK);
    rx_settings_activate(&c,&p);
    assert(hash_input_size==188);
    for (unsigned i=97;i<162;++i) assert(hash_input[i]==0);
    p.generation=2;
    commit_fault=1; assert(rx_settings_store(&c,&p)!=ESP_OK);
    rx_profile_t loaded; assert(rx_settings_load(&c,&loaded)==ESP_OK && loaded.generation==1);
    commit_fault=2; assert(rx_settings_store(&c,&p)!=ESP_OK);
    assert(rx_settings_load(&c,&loaded)==ESP_OK && loaded.generation==2);
    assert(rx_settings_active_profile()->generation==1); /* Stored is not active. */
    commit_fault=0; corrupt=true;
    assert(rx_settings_load(&c,&loaded)==ESP_ERR_INVALID_STATE); /* Never use legacy fallback. */
    assert(rx_settings_store(&c,&p)!=ESP_OK); /* Failed readback stays failed. */
    corrupt=false; assert(rx_settings_load(&c,&loaded)==ESP_OK && loaded.generation==2);
    puts("PASS: read-only migration, missing identity, legacy board default, validation before writes, commit uncertainty, readback, stored/active separation");
}

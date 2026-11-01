/* Real callback -> static pool -> serializer -> sender, with SDK/network doubles.
 * The raw WiFi sender task is not scheduled by this host harness. */
#include "csi_collector.h"
#include "csi_transport.h"
#include "csi_peer.h"
#include "rx_settings.h"
#include "rx_budget.h"
#include "rx_diag.h"
#include "stream_sender.h"
#include "esp_stubs.h"
#include <assert.h>
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>

nvs_config_t g_nvs_config;
static rx_profile_t profile;
static uint8_t config_hash[32];
static int64_t peer_callback_us;
static void copying_balance(void);

static void newer_peer_admission(void)
{
    esp_test_advance_time_us(1);
    assert(csi_peer_deferred_admit(1,peer_callback_us,esp_timer_get_time()));
    csi_peer_finish(1,true,true,false);
}

static void newer_tx_admission(void)
{
    esp_test_advance_time_us(1);
    (void)csi_peer_admit(profile.tx_mac,256,esp_timer_get_time());
}

static void wait_past_capture_age(void) { esp_test_advance_time_us(100001); }
static void wait_to_capture_age(void) { esp_test_advance_time_us(100000); }

static void reserve_peer(int64_t now)
{
    peer_callback_us=now;
    assert(csi_peer_admit(profile.peers[0].mac,128,now));
    assert(csi_peer_queue_reserve(1));
    csi_peer_capture_result(1,true);
}

static void peer_clock_regressions(void)
{
    csi_peer_stats_t stats;
    /* Another core may advance the budget after this callback samples time,
     * but before it acquires the peer lock. The real clock never goes back. */
    esp_test_set_time_us(3000000); reserve_peer(esp_timer_get_time());
    int64_t callback_snapshot=esp_timer_get_time();
    esp_test_before_critical_once(newer_peer_admission);
    (void)csi_peer_admit(profile.tx_mac,256,callback_snapshot);
    csi_peer_get_stats(&stats); assert(!stats.session_ended);

    esp_test_set_time_us(3200000); callback_snapshot=esp_timer_get_time();
    esp_test_before_critical_once(newer_tx_admission);
    assert(csi_peer_admit(profile.peers[0].mac,128,callback_snapshot));
    csi_peer_get_stats(&stats); assert(!stats.session_ended);

    esp_test_set_time_us(3400000); reserve_peer(esp_timer_get_time());
    int64_t sender_snapshot=esp_timer_get_time();
    esp_test_before_critical_once(newer_tx_admission);
    assert(csi_peer_deferred_admit(1,peer_callback_us,sender_snapshot));
    csi_peer_get_stats(&stats); assert(!stats.session_ended);
    csi_peer_finish(1,true,true,false);

    /* Lock waiting counts toward the unchanged 100-ms capture age. */
    esp_test_set_time_us(3600000); reserve_peer(esp_timer_get_time());
    sender_snapshot=esp_timer_get_time();
    esp_test_before_critical_once(wait_past_capture_age);
    assert(!csi_peer_deferred_admit(1,peer_callback_us,sender_snapshot));
    csi_peer_finish(1,true,false,true);
    esp_test_set_time_us(3900000); reserve_peer(esp_timer_get_time());
    sender_snapshot=esp_timer_get_time();
    esp_test_before_critical_once(wait_to_capture_age);
    assert(csi_peer_deferred_admit(1,peer_callback_us,sender_snapshot));
    csi_peer_finish(1,true,true,false);

    esp_test_set_time_us(RX_TRIAL_US-1);
    csi_peer_get_stats(&stats); assert(!stats.session_ended);
    esp_test_set_time_us(RX_TRIAL_US);
    csi_peer_get_stats(&stats); assert(stats.session_ended);
    copying_balance();
    puts("PASS: real peer admission handles reordered observations, lock-wait age, and exact finite expiry");
}
static void copying_balance(void)
{
    csi_peer_stats_t stats; csi_peer_get_stats(&stats);
    for (unsigned i=0;i<4;++i) {
        csi_peer_link_stats_t *s=&stats.links[i];
        assert(s->captured==s->queued+s->delivered+s->abandoned+s->serialize_errors+s->aborted);
    }
}
const rx_profile_t *rx_settings_active_profile(void) { return &profile; }
const nvs_config_t *rx_settings_active_config(void) { return &g_nvs_config; }
const uint8_t *rx_settings_active_hash(void) { return config_hash; }
void status_led_csi(int64_t now) { assert(now>=0); }

static void balances(void)
{
    csi_transport_stats_t t; csi_transport_get_stats(&t);
    assert(t.captures_enqueued==t.captures_waiting+t.captures_in_flight+t.captures_aborted
        +t.serialization_failures+t.frames_os_accepted+t.frames_abandoned);
    assert(t.pool_in_use==t.captures_waiting+t.captures_in_flight && t.ownership_errors==0);
    csi_collector_stats_t c; csi_collector_get_stats(&c);
    assert(c.callbacks_received==c.invalid_inputs+c.foreign_tx+c.rate_limited+c.admitted);
    assert(c.admitted==c.capture_handoff_accepted+c.capture_handoff_drops);
    csi_peer_stats_t p; csi_peer_get_stats(&p);
    for (unsigned i=0;i<=profile.peer_count;++i) {
        csi_peer_link_stats_t *l=&p.links[i];
        assert(l->captured==l->queued+l->delivered+l->abandoned+l->serialize_errors+l->aborted);
    }
}

int main(int argc, char **argv)
{
    assert(argc==2 || argc==3);
    int scenario=atoi(argv[1]); bool mesh=scenario!=0;
    profile=(rx_profile_t){.generation=7,.node_id=1,.led_gpio=48,
        .mode=scenario==2 || scenario==4 ? RX_PAIR_TRIAL : scenario==3 ? RX_MESH_TRIAL : mesh?RX_MESH_CONTINUOUS:RX_BASELINE,
        .rate_hz=50,.tx_mac={0,0x11,0x22,0x33,0x44,0x55},.peer_count=scenario==2 || scenario==4 ? 1 : mesh?3:0};
    for (unsigned i=0;i<profile.peer_count;++i) {
        profile.peers[i].node_id=i+2; profile.peers[i].mac[0]=2;
        profile.peers[i].mac[4]=2; profile.peers[i].mac[5]=i+2;
    }
    assert(rx_profile_valid(&profile));
    g_nvs_config.node_id=1; g_nvs_config.filter_mac_set=1;
    memcpy(g_nvs_config.filter_mac,profile.tx_mac,6); memset(config_hash,0xa7,sizeof(config_hash));
    esp_test_set_time_step_us(0); esp_test_set_time_us(1000000);
    csi_collector_set_node_id(1); csi_collector_set_boot_id(UINT64_C(0x0102030405060708));
    assert(csi_peer_prepare_radio()==ESP_OK && csi_peer_start()==ESP_OK);
    assert(csi_transport_start()==ESP_OK);
    assert(stream_sender_init_with("192.0.2.5",5005)==0);
    assert(csi_collector_init()==ESP_OK);
    assert(esp_test_promiscuous_mask()==(WIFI_PROMIS_FILTER_MASK_MGMT|WIFI_PROMIS_FILTER_MASK_DATA));
    if (scenario==2 || scenario==3) { peer_clock_regressions(); return 0; }
    if (scenario==4) {
        (void)csi_peer_admit(profile.tx_mac,256,esp_timer_get_time());
        esp_test_set_time_us(999999);
        csi_peer_stats_t stats; csi_peer_get_stats(&stats); assert(stats.session_ended);
        puts("PASS: actual budget clock rollback still ends the session"); return 0;
    }
    int8_t iq[256]; for (unsigned i=0;i<sizeof(iq);++i) iq[i]=(int8_t)i;
    wifi_csi_info_t info={.buf=iq,.len=sizeof(iq),.first_word_invalid=true,
        .rx_ctrl={.channel=6,.sig_mode=1,.rssi=-40,.noise_floor=-92,.timestamp=1234}};
    memcpy(info.mac,profile.tx_mac,6);
    size_t size=CSI_HEADER_SIZE+sizeof(iq)+CSI_SOURCE_BINDING_SIZE+CSI_IDENTITY_TRAILER_SIZE+CSI_PHYSICAL_TRAILER_SIZE;
    stream_sender_test_set_sendto_result(size,0);
    csi_transport_test_after_claim(copying_balance);
    csi_collector_test_invoke_callback(&info);
    csi_transport_test_after_claim(NULL);
    memset(iq,0,sizeof(iq)); /* Driver buffer may immediately be reused. */
    esp_test_advance_time_us(1000);
    assert(csi_transport_test_process_one());
    uint8_t packet[CSI_MAX_FRAME_SIZE]; assert(stream_sender_test_copy_last_payload(packet,sizeof(packet))==size);
    for (unsigned i=0;i<sizeof(iq);++i) assert(packet[CSI_HEADER_SIZE+i]==(uint8_t)i);
    uint8_t *id=packet+CSI_HEADER_SIZE+sizeof(iq)+CSI_SOURCE_BINDING_SIZE;
    assert(id[4]==4 && id[6]==112 && id[28]&CSI_QUALITY_FIRST_WORD_INVALID);
    assert(id[40]==7 && id[44]==profile.mode && id[45]==0 && id[46]==profile.peer_count);
    assert(!memcmp(id+48,config_hash,32) && id[108]==1);
    uint64_t callback,dequeued,sent;
    memcpy(&callback,id+16,8); memcpy(&dequeued,id+80,8); memcpy(&sent,id+88,8);
    assert(callback==1000000 && dequeued>=callback && sent>=dequeued);
    if (argc==3) { FILE *f=fopen(argv[2],"wbx"); assert(f); assert(fwrite(packet,1,size,f)==size); assert(fclose(f)==0); }
    balances();

    /* Foreign and malformed inputs cannot consume a TX quota. */
    esp_test_set_time_us(1100000);
    info.mac[0]=4; csi_collector_test_invoke_callback(&info);
    memcpy(info.mac,profile.tx_mac,6); info.len=255; csi_collector_test_invoke_callback(&info); info.len=256;
    csi_collector_test_invoke_callback(&info); assert(csi_transport_test_process_one()); balances();
    if (mesh) {
        /* Three simultaneous sources must all own a slot, even at one phase. */
        esp_test_set_time_us(1200000);
        for (unsigned i=0;i<3;++i) { memcpy(info.mac,profile.peers[i].mac,6); csi_collector_test_invoke_callback(&info); }
        csi_peer_stats_t p; csi_peer_get_stats(&p);
        for (unsigned i=1;i<=3;++i) assert(p.links[i].queued==1 && p.links[i].queue_drops==0);
        for (unsigned i=0;i<400 && csi_transport_test_process_one();++i) esp_test_advance_time_us(1000);
        balances();
    }
    /* Failed publish, serializer and stale data all close ownership once. */
    memcpy(info.mac,profile.tx_mac,6);
    esp_test_set_time_us(2000000); csi_transport_test_fail_next_publish(); csi_collector_test_invoke_callback(&info); balances();
    esp_test_set_time_us(2100000); csi_collector_test_invoke_callback(&info); csi_transport_test_fail_next_serialization();
    assert(csi_transport_test_process_one()); balances();
    esp_test_set_time_us(2200000); csi_collector_test_invoke_callback(&info); esp_test_advance_time_us(100001);
    assert(csi_transport_test_process_one()); balances();
    esp_test_set_time_us(2400000); csi_collector_test_invoke_callback(&info); csi_transport_stop(); balances();
    assert(csi_transport_start()==ESP_OK); csi_transport_stop(); balances();
    puts("PASS: unified production callback/pool/wire/sender, original IQ, timing, source filtering, three peer slots, failure/stop accounting");
}

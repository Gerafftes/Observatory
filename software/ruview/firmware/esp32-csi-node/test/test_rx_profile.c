/* Exercises the production profile/settings codecs and quota state machine. */
#include "rx_profile.h"
#include "rx_budget.h"
#include "rx_settings.h"
#include <assert.h>
#include <stdio.h>
#include <string.h>

static rx_profile_t profile(unsigned node, unsigned mode)
{
    rx_profile_t p = {.generation=1, .node_id=node, .led_gpio=48, .mode=mode, .rate_hz=50,
        .tx_mac={2,0,0,0,1,1}};
    unsigned count = mode == RX_BASELINE ? 0 : (mode == RX_PAIR_TRIAL || mode == RX_PAIR_CONTINUOUS ? 1 : 3);
    for (unsigned i=1; i<=4 && p.peer_count<count; ++i) {
        if (i == node) continue;
        rx_peer_identity_t *peer = &p.peers[p.peer_count++];
        peer->node_id=i; peer->mac[0]=2; peer->mac[4]=2; peer->mac[5]=i;
    }
    return p;
}

static void codecs(void)
{
    for (unsigned node=1; node<=4; ++node) {
        for (unsigned mode=RX_BASELINE; mode<=RX_MESH_CONTINUOUS; ++mode) {
            rx_profile_t p=profile(node,mode), decoded;
            uint8_t bytes[RX_PROFILE_BYTES], canonical[RX_PROFILE_BYTES];
            assert(rx_profile_encode(&p,bytes));
            assert(rx_profile_decode(bytes,sizeof(bytes),&decoded));
            assert(rx_profile_encode(&decoded,canonical));
            assert(!memcmp(bytes,canonical,sizeof(bytes)));
            assert(rx_profile_link(&p,p.tx_mac)==0);
            for (unsigned i=0; i<p.peer_count; ++i) assert(rx_profile_link(&p,p.peers[i].mac)==(int)i+1);
            for (unsigned i=0; i<sizeof(bytes); ++i) {
                bytes[i]^=1; assert(!rx_profile_decode(bytes,sizeof(bytes),&decoded)); bytes[i]^=1;
            }
        }
    }
    rx_profile_t p=profile(1,RX_MESH_TRIAL);
    p.peers[1]=p.peers[0]; assert(!rx_profile_valid(&p));
    p=profile(1,RX_PAIR_TRIAL); p.peers[0].node_id=1; assert(!rx_profile_valid(&p));
    p=profile(1,RX_BASELINE); p.node_id=0; assert(!rx_profile_valid(&p));
    p=profile(1,RX_BASELINE); p.generation=0; assert(!rx_profile_valid(&p));
    p=profile(1,RX_BASELINE); p.led_gpio=47; assert(!rx_profile_valid(&p));
    p=profile(1,RX_BASELINE); p.tx_mac[0]=1; assert(!rx_profile_valid(&p));

    p=profile(1,RX_BASELINE);
    nvs_config_t c={.wifi_ssid="test",.wifi_password="test-secret",.target_ip="192.0.2.5",
        .target_port=5005,.node_id=1,.filter_mac_set=1,.csi_channel=6,.tdm_node_count=1};
    memcpy(c.filter_mac,p.tx_mac,6);
    uint8_t bytes[RX_SETTINGS_BYTES], canonical[RX_SETTINGS_BYTES];
    assert(rx_settings_encode(&c,&p,bytes));
    nvs_config_t decoded={.wasm_verify=1,.seed_token="unrelated"}; rx_profile_t decoded_p;
    assert(rx_settings_decode(bytes,sizeof(bytes),&decoded,&decoded_p));
    assert(decoded.wasm_verify==1 && !strcmp(decoded.seed_token,"unrelated"));
    assert(!strcmp(decoded.wifi_password,c.wifi_password));
    assert(rx_settings_encode(&decoded,&decoded_p,canonical));
    assert(!memcmp(bytes,canonical,sizeof(bytes)));
    for (unsigned i=0; i<sizeof(bytes); ++i) {
        bytes[i]^=1; assert(!rx_settings_decode(bytes,sizeof(bytes),&decoded,&decoded_p)); bytes[i]^=1;
    }
    c.target_port=0; assert(!rx_settings_valid(&c,&p)); c.target_port=5005;
    strcpy(c.target_ip,"1.2.3.999"); assert(!rx_settings_valid(&c,&p));
    strcpy(c.target_ip,"0.0.0.0"); assert(!rx_settings_valid(&c,&p));
    strcpy(c.target_ip,"224.0.0.1"); assert(!rx_settings_valid(&c,&p));
    strcpy(c.target_ip,"1.2.3."); assert(!rx_settings_valid(&c,&p));
    strcpy(c.target_ip,"192.0.2.5");
    c.tdm_node_count=2; assert(!rx_settings_valid(&c,&p)); c.tdm_node_count=1;
    c.edge_tier=1; assert(!rx_settings_valid(&c,&p)); c.edge_tier=0;
    memset(c.wifi_ssid,'x',sizeof(c.wifi_ssid)); assert(!rx_settings_valid(&c,&p));
}

static void quotas(void)
{
    for (unsigned rate=20;rate<=50;rate+=5) {
        rx_profile_t p=profile(1,RX_MESH_CONTINUOUS); p.rate_hz=rate;
        assert(rx_profile_valid(&p)); rx_budget_t b; rx_budget_init(&b);
        unsigned counts[4]={0};
        for (int64_t now=0;now<1000000;++now) {
            for (unsigned link=1;link<=3;++link) counts[link]+=rx_budget_admit(&b,&p,link,now,true);
            counts[0]+=rx_budget_admit(&b,&p,0,now,true);
        }
        assert(counts[0]==rate-rate/5 && counts[1]+counts[2]+counts[3]==rate/5);
    }
    rx_profile_t invalid_rate=profile(1,RX_BASELINE); invalid_rate.rate_hz=51; assert(!rx_profile_valid(&invalid_rate));
    invalid_rate.rate_hz=21; assert(!rx_profile_valid(&invalid_rate));
    for (unsigned mode=RX_PAIR_TRIAL; mode<=RX_MESH_CONTINUOUS; ++mode) {
        rx_profile_t p=profile(1,mode); rx_budget_t b; rx_budget_init(&b);
        unsigned totals[4]={0};
        for (int64_t second=0; second<6; ++second) {
            unsigned per_second[4]={0};
            for (int64_t now=second*1000000; now<(second+1)*1000000; now+=1000) {
                /* Present every configured source, as a fair deferred queue does. */
                for (unsigned link=1; link<=p.peer_count; ++link)
                    if (rx_budget_admit(&b,&p,link,now,true)) ++per_second[link];
                if (rx_budget_admit(&b,&p,0,now,true)) ++per_second[0];
            }
            assert(per_second[0]==40);
            unsigned peers=0;
            for (unsigned i=1;i<=p.peer_count;++i) {
                peers+=per_second[i]; assert(per_second[i]>=3 && per_second[i]<=10);
            }
            assert(peers==10);
            for (unsigned i=0;i<4;++i) totals[i]+=per_second[i];
        }
        if (p.peer_count==3) assert(totals[1]==20 && totals[2]==20 && totals[3]==20);
        assert(rx_budget_peer_open(&b,&p,RX_TRIAL_US-1));
        assert(rx_budget_peer_open(&b,&p,RX_TRIAL_US)==!rx_profile_trial(&p));
        if (rx_profile_trial(&p)) {
            assert(!rx_budget_admit(&b,&p,1,RX_TRIAL_US,false));
            assert(!rx_budget_peer_open(&b,&p,0)); /* expiry cannot reopen */
        }
    }
    rx_profile_t p=profile(1,RX_MESH_CONTINUOUS); rx_budget_t b; rx_budget_init(&b);
    unsigned one=0;
    for (int64_t now=0;now<1000000;now+=1000) one+=rx_budget_admit(&b,&p,1,now,true);
    assert(one==4); /* Missing links cannot donate their quota. */
    assert(!rx_budget_admit(&b,&p,1,-1,true));
    assert(!rx_budget_peer_open(&b,&p,2000000));
    p=profile(1,RX_BASELINE); rx_budget_init(&b);
    unsigned tx=0;
    for (int64_t now=0;now<1000000;now+=1000) tx+=rx_budget_admit(&b,&p,0,now,false);
    assert(tx==50);
    assert(!rx_budget_peer_open(&b,&p,0));
}

int main(void)
{
    codecs(); quotas();
    puts("PASS: four RX identities, five profiles, canonical codecs, corruption rejection, TX reserve, total peer budget, fairness, expiry");
}

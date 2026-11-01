#include "rx_profile.h"
#include <string.h>

static uint32_t get32(const uint8_t *p)
{
    return (uint32_t)p[0] | (uint32_t)p[1] << 8 | (uint32_t)p[2] << 16 | (uint32_t)p[3] << 24;
}

static void put32(uint8_t *p, uint32_t v)
{
    for (unsigned i = 0; i < 4; ++i) p[i] = (uint8_t)(v >> (8 * i));
}

uint32_t rx_crc32(const uint8_t *data, size_t length)
{
    uint32_t crc = UINT32_MAX;
    for (size_t i = 0; i < length; ++i) {
        crc ^= data[i];
        for (unsigned bit = 0; bit < 8; ++bit)
            crc = (crc >> 1) ^ (UINT32_C(0xedb88320) & (0u - (crc & 1u)));
    }
    return ~crc;
}

bool rx_mac_valid(const uint8_t mac[6])
{
    if (!mac || (mac[0] & 1u)) return false;
    unsigned any = 0;
    for (unsigned i = 0; i < 6; ++i) any |= mac[i];
    return any != 0;
}

bool rx_profile_trial(const rx_profile_t *p)
{
    return p && (p->mode == RX_PAIR_TRIAL || p->mode == RX_MESH_TRIAL);
}

const char *rx_profile_mode_name(uint8_t mode)
{
    static const char *names[] = {
        "tx-only", "pair-trial", "mesh-trial", "pair-continuous", "mesh-continuous"
    };
    return mode <= RX_MESH_CONTINUOUS ? names[mode] : "invalid";
}

bool rx_profile_valid(const rx_profile_t *p)
{
    if (!p || !p->generation || p->node_id < 1 || p->node_id > 4
        || (p->led_gpio != 38 && p->led_gpio != 48)
        || p->mode > RX_MESH_CONTINUOUS || p->rate_hz < 20 || p->rate_hz > 50 || p->rate_hz % 5 != 0
        || !rx_mac_valid(p->tx_mac) || p->peer_count > RX_MAX_PEERS) return false;
    unsigned expected = p->mode == RX_BASELINE ? 0
        : (p->mode == RX_PAIR_TRIAL || p->mode == RX_PAIR_CONTINUOUS ? 1 : 3);
    if (p->peer_count != expected) return false;
    for (unsigned i = 0; i < p->peer_count; ++i) {
        const rx_peer_identity_t *peer = &p->peers[i];
        if (peer->node_id < 1 || peer->node_id > 4 || peer->node_id == p->node_id
            || !rx_mac_valid(peer->mac) || !memcmp(peer->mac, p->tx_mac, 6)) return false;
        for (unsigned j = 0; j < i; ++j)
            if (p->peers[j].node_id >= peer->node_id || !memcmp(p->peers[j].mac, peer->mac, 6)) return false;
    }
    return true;
}

bool rx_profile_encode(const rx_profile_t *p, uint8_t out[RX_PROFILE_BYTES])
{
    if (!out || !rx_profile_valid(p)) return false;
    memset(out, 0, RX_PROFILE_BYTES);
    memcpy(out, "RXP1", 4);
    out[4] = 1; out[5] = p->node_id; out[6] = p->led_gpio; out[7] = p->mode;
    put32(out + 8, p->generation);
    out[12] = p->rate_hz; out[13] = p->peer_count;
    memcpy(out + 16, p->tx_mac, 6);
    for (unsigned i = 0; i < p->peer_count; ++i) {
        out[22 + 7 * i] = p->peers[i].node_id;
        memcpy(out + 23 + 7 * i, p->peers[i].mac, 6);
    }
    put32(out + 60, rx_crc32(out, 60));
    return true;
}

bool rx_profile_decode(const uint8_t *b, size_t length, rx_profile_t *out)
{
    if (!b || !out || length != RX_PROFILE_BYTES || memcmp(b, "RXP1", 4)
        || b[4] != 1 || b[13] > RX_MAX_PEERS || get32(b + 60) != rx_crc32(b, 60)) return false;
    rx_profile_t p = {.generation = get32(b + 8), .node_id = b[5], .led_gpio = b[6],
        .mode = b[7], .rate_hz = b[12], .peer_count = b[13]};
    memcpy(p.tx_mac, b + 16, 6);
    for (unsigned i = 0; i < p.peer_count; ++i) {
        p.peers[i].node_id = b[22 + 7 * i];
        memcpy(p.peers[i].mac, b + 23 + 7 * i, 6);
    }
    uint8_t canonical[RX_PROFILE_BYTES];
    if (!rx_profile_encode(&p, canonical) || memcmp(canonical, b, sizeof(canonical))) return false;
    *out = p;
    return true;
}

int rx_profile_link(const rx_profile_t *p, const uint8_t mac[6])
{
    if (!p || !mac) return -1;
    if (!memcmp(p->tx_mac, mac, 6)) return 0;
    for (unsigned i = 0; i < p->peer_count; ++i)
        if (!memcmp(p->peers[i].mac, mac, 6)) return (int)i + 1;
    return -1;
}

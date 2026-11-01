#include "rx_settings.h"
#include <string.h>

static bool ipv4(const char *s)
{
    for (unsigned part = 0; part < 4; ++part) {
        unsigned value = 0, digits = 0;
        while (*s >= '0' && *s <= '9') {
            value = value * 10 + (unsigned)(*s++ - '0');
            if (++digits > 3 || value > 255) return false;
        }
        if (part==0 && (value==0 || value==127 || value>=224)) return false;
        if (!digits || (part < 3 ? *s++ != '.' : *s != '\0')) return false;
    }
    return true;
}

bool rx_settings_valid(const nvs_config_t *c, const rx_profile_t *p)
{
    if (!c || !rx_profile_valid(p) || c->node_id != p->node_id || !c->filter_mac_set
        || memcmp(c->filter_mac, p->tx_mac, 6) || !memchr(c->wifi_ssid, 0, sizeof(c->wifi_ssid))
        || !c->wifi_ssid[0] || !memchr(c->wifi_password, 0, sizeof(c->wifi_password))
        || !memchr(c->target_ip, 0, sizeof(c->target_ip)) || !ipv4(c->target_ip)
        || !c->target_port || c->csi_channel < 1 || c->csi_channel > 13
        || c->edge_tier != 0 || c->tdm_node_count != 1 || c->tdm_slot_index != 0
        || c->channel_hop_count > 1) return false;
    size_t password_length = strlen(c->wifi_password);
    if (password_length && password_length < 8) return false;
    if (password_length == 64) {
        for (unsigned i = 0; i < password_length; ++i)
            if (!strchr("0123456789abcdefABCDEF", c->wifi_password[i])) return false;
    }
    return true;
}

bool rx_settings_encode(const nvs_config_t *c, const rx_profile_t *p,
                        uint8_t out[RX_SETTINGS_BYTES])
{
    if (!out || !rx_settings_valid(c, p)) return false;
    memset(out, 0, RX_SETTINGS_BYTES);
    if (!rx_profile_encode(p, out)) return false;
    memcpy(out + 64, c->wifi_ssid, strlen(c->wifi_ssid));
    memcpy(out + 97, c->wifi_password, strlen(c->wifi_password));
    memcpy(out + 162, c->target_ip, strlen(c->target_ip));
    out[178] = c->target_port; out[179] = c->target_port >> 8;
    out[180] = c->csi_channel;
    uint32_t crc = rx_crc32(out, 188);
    for (unsigned i = 0; i < 4; ++i) out[188 + i] = crc >> (8 * i);
    return true;
}

bool rx_settings_decode(const uint8_t *b, size_t length, nvs_config_t *config, rx_profile_t *profile)
{
    if (!b || !config || !profile || length != RX_SETTINGS_BYTES) return false;
    uint32_t crc = (uint32_t)b[188] | (uint32_t)b[189] << 8 | (uint32_t)b[190] << 16 | (uint32_t)b[191] << 24;
    rx_profile_t p;
    if (crc != rx_crc32(b, 188) || !rx_profile_decode(b, RX_PROFILE_BYTES, &p)) return false;
    nvs_config_t c = *config;
    memcpy(c.wifi_ssid, b + 64, 33); memcpy(c.wifi_password, b + 97, 65);
    memcpy(c.target_ip, b + 162, 16);
    c.target_port = (uint16_t)b[178] | (uint16_t)b[179] << 8;
    c.csi_channel = b[180]; c.node_id = p.node_id;
    c.filter_mac_set = 1; memcpy(c.filter_mac, p.tx_mac, 6);
    c.edge_tier = 0; c.tdm_slot_index = 0; c.tdm_node_count = 1; c.channel_hop_count = 0;
    uint8_t canonical[RX_SETTINGS_BYTES];
    if (!rx_settings_encode(&c, &p, canonical) || memcmp(canonical, b, length)) return false;
    *config = c; *profile = p;
    return true;
}

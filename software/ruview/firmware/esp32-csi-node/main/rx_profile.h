#ifndef RX_PROFILE_H
#define RX_PROFILE_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#define RX_PROFILE_BYTES 64u
#define RX_MAX_PEERS 3u
#define RX_TRIAL_US INT64_C(900000000)

typedef enum {
    RX_BASELINE,
    RX_PAIR_TRIAL,
    RX_MESH_TRIAL,
    RX_PAIR_CONTINUOUS,
    RX_MESH_CONTINUOUS,
} rx_mode_t;

typedef struct {
    uint8_t node_id;
    uint8_t mac[6];
} rx_peer_identity_t;

typedef struct {
    uint32_t generation;
    uint8_t node_id;
    uint8_t led_gpio;
    uint8_t mode;
    uint8_t rate_hz;
    uint8_t tx_mac[6];
    uint8_t peer_count;
    rx_peer_identity_t peers[RX_MAX_PEERS];
} rx_profile_t;

bool rx_mac_valid(const uint8_t mac[6]);
bool rx_profile_valid(const rx_profile_t *profile);
bool rx_profile_encode(const rx_profile_t *profile, uint8_t out[RX_PROFILE_BYTES]);
bool rx_profile_decode(const uint8_t *bytes, size_t length, rx_profile_t *out);
uint32_t rx_crc32(const uint8_t *data, size_t length);
int rx_profile_link(const rx_profile_t *profile, const uint8_t mac[6]);
bool rx_profile_trial(const rx_profile_t *profile);
const char *rx_profile_mode_name(uint8_t mode);

#endif

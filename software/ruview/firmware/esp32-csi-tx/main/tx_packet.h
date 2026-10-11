#ifndef CSI_TX_PACKET_H
#define CSI_TX_PACKET_H

#include <stddef.h>
#include <stdint.h>

#define TX_SOUNDING_PACKET_SIZE 32U
#define TX_SOUNDING_PROTOCOL_VERSION 2U
#define TX_SOUNDING_ROLE_TX 2U

void tx_packet_build(uint8_t packet[TX_SOUNDING_PACKET_SIZE],
                     uint32_t sequence,
                     uint32_t timestamp_ms,
                     uint64_t boot_id,
                     uint8_t channel,
                     uint8_t rate_hz);

#endif

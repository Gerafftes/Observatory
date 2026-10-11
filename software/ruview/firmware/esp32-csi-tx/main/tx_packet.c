#include "tx_packet.h"

#include <string.h>

static void put_le(uint8_t *output, uint64_t value, unsigned bytes)
{
    for (unsigned i = 0; i < bytes; ++i) output[i] = (uint8_t)(value >> (8 * i));
}

void tx_packet_build(uint8_t packet[TX_SOUNDING_PACKET_SIZE],
                     uint32_t sequence,
                     uint32_t timestamp_ms,
                     uint64_t boot_id,
                     uint8_t channel,
                     uint8_t rate_hz)
{
    memset(packet, 0, TX_SOUNDING_PACKET_SIZE);
    memcpy(packet, "CSI_TX", 6);
    packet[6] = TX_SOUNDING_PROTOCOL_VERSION;
    packet[7] = TX_SOUNDING_ROLE_TX;
    put_le(packet + 8, sequence, 4);
    put_le(packet + 12, timestamp_ms, 4);
    memcpy(packet + 16, "TX1", 3);
    put_le(packet + 20, boot_id, 8);
    packet[28] = channel;
    packet[29] = rate_hz;
}

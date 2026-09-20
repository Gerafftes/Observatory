#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

#include "tx_packet.h"

int main()
{
    uint8_t packet[csi_tx::kSoundingPacketSize];
    const uint32_t sequence = 0x12345678;
    const uint32_t timestamp_ms = 0x90abcdef;

    csi_tx::buildSoundingPacket(packet, sequence, timestamp_ms);

    assert(sizeof(packet) == 32);
    assert(memcmp(packet, "CSI_TX", 6) == 0);
    assert(memcmp(packet + 8, &sequence, sizeof(sequence)) == 0);
    assert(memcmp(packet + 12, &timestamp_ms, sizeof(timestamp_ms)) == 0);
    assert(memcmp(packet + 16, "TX1", 3) == 0);
    for (size_t index = 19; index < sizeof(packet); ++index) {
        assert(packet[index] == 0);
    }

    puts("TX sounding packet tests passed");
    return 0;
}

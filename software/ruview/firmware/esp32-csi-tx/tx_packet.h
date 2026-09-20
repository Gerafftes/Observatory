#pragma once

#include <stddef.h>
#include <stdint.h>
#include <string.h>

namespace csi_tx {

constexpr char kNodeId[] = "TX1";
constexpr size_t kSoundingPacketSize = 32;

inline void buildSoundingPacket(
    uint8_t (&packet)[kSoundingPacketSize],
    uint32_t sequence,
    uint32_t timestampMs) {
  memset(packet, 0, sizeof(packet));
  memcpy(packet, "CSI_TX", 6);
  memcpy(packet + 8, &sequence, sizeof(sequence));
  memcpy(packet + 12, &timestampMs, sizeof(timestampMs));
  memcpy(packet + 16, kNodeId, sizeof(kNodeId) - 1);
}

}  // namespace csi_tx

#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#define MMWAVE_ACK_MAGIC 0x5256414bU
#define MMWAVE_ACK_SIZE 12U

static inline uint32_t mmwave_ack_read_u32(const uint8_t *bytes)
{
    return ((uint32_t)bytes[0] << 24) |
           ((uint32_t)bytes[1] << 16) |
           ((uint32_t)bytes[2] << 8) |
           (uint32_t)bytes[3];
}

static inline bool mmwave_ack_matches(const uint8_t *bytes, size_t length,
                                      uint32_t boot_id, uint32_t sequence)
{
    return length == MMWAVE_ACK_SIZE &&
           mmwave_ack_read_u32(bytes) == MMWAVE_ACK_MAGIC &&
           mmwave_ack_read_u32(bytes + 4) == boot_id &&
           mmwave_ack_read_u32(bytes + 8) == sequence;
}

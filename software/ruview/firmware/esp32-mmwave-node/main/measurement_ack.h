#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#define MMWAVE_ACK_MAGIC 0x5256414bU
#define MMWAVE_ACK_SIZE 12U

typedef struct {
    bool initialized;
    uint64_t smoothed_rtt_us;
    uint64_t rtt_variance_us;
    uint32_t floor_timeout_ms;
    uint32_t timeout_ms;
} mmwave_ack_timing_t;

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

static inline uint32_t mmwave_ack_clamp_timeout_ms(uint64_t timeout_us,
                                                   uint32_t minimum_ms,
                                                   uint32_t maximum_ms)
{
    uint64_t rounded_ms = (timeout_us + 999U) / 1000U;
    if (rounded_ms < minimum_ms) {
        return minimum_ms;
    }
    if (rounded_ms > maximum_ms) {
        return maximum_ms;
    }
    return (uint32_t)rounded_ms;
}

static inline void mmwave_ack_timing_init(mmwave_ack_timing_t *timing,
                                          uint32_t initial_timeout_ms)
{
    *timing = (mmwave_ack_timing_t) {
        .floor_timeout_ms = initial_timeout_ms,
        .timeout_ms = initial_timeout_ms,
    };
}

static inline void mmwave_ack_timing_observe(mmwave_ack_timing_t *timing,
                                             uint64_t round_trip_us,
                                             uint32_t minimum_ms,
                                             uint32_t maximum_ms)
{
    if (!timing->initialized) {
        timing->smoothed_rtt_us = round_trip_us;
        timing->rtt_variance_us = round_trip_us / 2U;
        timing->initialized = true;
    } else {
        uint64_t delta = timing->smoothed_rtt_us > round_trip_us
            ? timing->smoothed_rtt_us - round_trip_us
            : round_trip_us - timing->smoothed_rtt_us;
        timing->rtt_variance_us =
            (3U * timing->rtt_variance_us + delta) / 4U;
        timing->smoothed_rtt_us =
            (7U * timing->smoothed_rtt_us + round_trip_us) / 8U;
    }
    uint32_t effective_minimum_ms = timing->floor_timeout_ms > minimum_ms
        ? timing->floor_timeout_ms : minimum_ms;
    timing->timeout_ms = mmwave_ack_clamp_timeout_ms(
        timing->smoothed_rtt_us + 4U * timing->rtt_variance_us,
        effective_minimum_ms, maximum_ms);
}

static inline uint32_t mmwave_ack_attempt_timeout_ms(uint32_t base_timeout_ms,
                                                     unsigned attempt,
                                                     uint32_t maximum_ms)
{
    uint64_t timeout_ms = base_timeout_ms;
    while (attempt > 0 && timeout_ms < maximum_ms) {
        timeout_ms *= 2U;
        attempt -= 1;
    }
    return timeout_ms > maximum_ms ? maximum_ms : (uint32_t)timeout_ms;
}

static inline void mmwave_ack_timing_backoff(mmwave_ack_timing_t *timing,
                                             uint32_t maximum_ms)
{
    timing->timeout_ms = mmwave_ack_attempt_timeout_ms(
        timing->timeout_ms, 1, maximum_ms);
    timing->floor_timeout_ms = timing->timeout_ms;
}

static inline void mmwave_ack_timing_preserve_retry(
    mmwave_ack_timing_t *timing, uint32_t successful_attempt_timeout_ms)
{
    if (successful_attempt_timeout_ms > timing->floor_timeout_ms) {
        timing->floor_timeout_ms = successful_attempt_timeout_ms;
    }
    if (timing->floor_timeout_ms > timing->timeout_ms) {
        timing->timeout_ms = timing->floor_timeout_ms;
    } else if (successful_attempt_timeout_ms > timing->timeout_ms) {
        timing->timeout_ms = successful_attempt_timeout_ms;
    }
}

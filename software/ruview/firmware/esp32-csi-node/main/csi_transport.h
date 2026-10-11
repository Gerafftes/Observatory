/**
 * @file csi_transport.h
 * @brief Bounded CSI capture pool, processing queue, and sender task.
 */

#ifndef CSI_TRANSPORT_H
#define CSI_TRANSPORT_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#include "esp_err.h"
#include "esp_wifi_types.h"

#ifndef CSI_TRANSPORT_CAPTURE_SLOTS
#ifdef CONFIG_CSI_TRANSPORT_CAPTURE_SLOTS
#define CSI_TRANSPORT_CAPTURE_SLOTS CONFIG_CSI_TRANSPORT_CAPTURE_SLOTS
#else
#define CSI_TRANSPORT_CAPTURE_SLOTS 8u
#endif
#endif

typedef struct {
    uint32_t captures_enqueued;
    uint32_t start_failures;
    uint32_t not_ready_drops;
    uint32_t invalid_drops;
    uint32_t pool_exhausted_drops;
    uint32_t stale_queued_replacements;
    uint32_t queue_publish_failures;
    uint32_t ownership_errors;
    uint32_t captures_waiting;
    uint32_t captures_in_flight;
    uint32_t captures_aborted;
    uint32_t pool_in_use;
    uint32_t pool_high_watermark;
    uint32_t queue_depth;
    uint32_t queue_high_watermark;
    uint32_t serialization_failures;
    uint32_t serialized_frames;
    uint32_t csi_send_calls;
    uint32_t retries;
    uint32_t transport_errors;
    uint32_t frames_os_accepted;
    uint32_t frames_abandoned;
    uint32_t frames_expired;
    uint32_t edge_enqueue_accepted;
    uint32_t edge_enqueue_drops;
    uint32_t edge_enqueue_skipped;
    uint32_t sync_packets_attempted;
    uint32_t sync_packets_sent;
    uint64_t queue_age_total_us;
    uint32_t queue_age_max_us;
    uint32_t queue_age_last_us;
    uint32_t queue_age_samples;
    uint64_t callback_to_dequeue_total_us;
    uint32_t callback_to_dequeue_max_us;
    uint32_t callback_to_dequeue_last_us;
    uint32_t processing_stack_configured_bytes;
    uint32_t processing_stack_free_min_bytes;
    bool processing_stack_runtime_measured;
} csi_transport_stats_t;

/* captures_enqueued = captures_waiting + captures_in_flight
 *                   + captures_aborted + serialization_failures
 *                   + frames_os_accepted + frames_abandoned
 * frames_expired is a subset of frames_abandoned. */

esp_err_t csi_transport_start(void);

/**
 * Stop cooperatively, abort queued captures, and release every static slot.
 * An already processing capture is allowed to reach a terminal outcome.
 * The caller must disable the CSI callback before calling this function.
 */
void csi_transport_stop(void);

/**
 * Copy one driver-owned CSI capture into the fixed pool and publish its slot
 * without waiting. No pointer from @p info is retained after this call.
 */
bool csi_transport_enqueue_capture(
    const wifi_csi_info_t *info,
    int64_t callback_started_at_us
);

void csi_transport_get_stats(csi_transport_stats_t *stats);

#ifdef CSI_DIAGNOSTICS_TEST
void csi_transport_test_reset(void);
bool csi_transport_test_process_one(void);
void csi_transport_test_fail_next_publish(void);
void csi_transport_test_fail_next_serialization(void);
#endif

#endif /* CSI_TRANSPORT_H */

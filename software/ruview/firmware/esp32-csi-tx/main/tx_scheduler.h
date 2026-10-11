#ifndef CSI_TX_SCHEDULER_H
#define CSI_TX_SCHEDULER_H

#include <stdint.h>

typedef struct {
    uint32_t next_sequence;
    uint32_t missed_deadlines;
    uint64_t next_deadline_us;
    uint64_t interval_us;
    uint64_t jitter_sum_us;
    uint32_t jitter_samples;
    uint32_t jitter_max_us;
} tx_scheduler_t;

typedef struct {
    uint32_t sequence;
    uint32_t missed_this_wakeup;
    uint32_t jitter_us;
} tx_schedule_result_t;

void tx_scheduler_init(tx_scheduler_t *scheduler, uint16_t rate_hz, uint64_t now_us);
tx_schedule_result_t tx_scheduler_on_wakeup(tx_scheduler_t *scheduler,
                                             uint32_t notification_count,
                                             uint64_t now_us);

#endif

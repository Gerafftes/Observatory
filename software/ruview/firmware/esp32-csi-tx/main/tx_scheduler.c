#include "tx_scheduler.h"

#include <limits.h>

void tx_scheduler_init(tx_scheduler_t *scheduler, uint16_t rate_hz, uint64_t now_us)
{
    scheduler->next_sequence = 0;
    scheduler->missed_deadlines = 0;
    scheduler->interval_us = 1000000ULL / rate_hz;
    scheduler->next_deadline_us = now_us + scheduler->interval_us;
    scheduler->jitter_sum_us = 0;
    scheduler->jitter_samples = 0;
    scheduler->jitter_max_us = 0;
}

tx_schedule_result_t tx_scheduler_on_wakeup(tx_scheduler_t *scheduler,
                                             uint32_t notification_count,
                                             uint64_t now_us)
{
    if (notification_count == 0) {
        notification_count = 1;
    }
    uint64_t delta = now_us >= scheduler->next_deadline_us
                         ? now_us - scheduler->next_deadline_us
                         : scheduler->next_deadline_us - now_us;
    uint32_t jitter = delta > UINT32_MAX ? UINT32_MAX : (uint32_t)delta;
    uint32_t missed = notification_count - 1;
    tx_schedule_result_t result = {
        .sequence = scheduler->next_sequence++,
        .missed_this_wakeup = missed,
        .jitter_us = jitter,
    };
    scheduler->missed_deadlines += missed;
    scheduler->jitter_sum_us += jitter;
    scheduler->jitter_samples++;
    if (jitter > scheduler->jitter_max_us) {
        scheduler->jitter_max_us = jitter;
    }
    scheduler->next_deadline_us += scheduler->interval_us * notification_count;
    return result;
}

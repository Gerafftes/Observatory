#ifndef RX_BUDGET_H
#define RX_BUDGET_H
#include "rx_profile.h"

/* Caller serializes access. No allocation, SDK calls or sleeping. */
typedef struct {
    int64_t last_us, second, next_total_us, next_peer_us;
    uint32_t used[4];
    bool initialized, trial_ended;
} rx_budget_t;

void rx_budget_init(rx_budget_t *budget);
bool rx_budget_peer_open(rx_budget_t *budget, const rx_profile_t *profile, int64_t now_us);
/* A waiting peer reserves its next eligible opportunity; TX cannot starve it. */
bool rx_budget_admit(rx_budget_t *budget, const rx_profile_t *profile,
                     unsigned link, int64_t now_us, bool peer_waiting);
#endif

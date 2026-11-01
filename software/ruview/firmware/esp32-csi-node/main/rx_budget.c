#include "rx_budget.h"
#include <string.h>

void rx_budget_init(rx_budget_t *b)
{
    memset(b, 0, sizeof(*b));
    b->second = -1;
}

bool rx_budget_peer_open(rx_budget_t *b, const rx_profile_t *p, int64_t now)
{
    if (now < 0 || (b->initialized && now < b->last_us)
        || (rx_profile_trial(p) && now >= RX_TRIAL_US)) b->trial_ended = true;
    return p->peer_count && !b->trial_ended;
}

bool rx_budget_admit(rx_budget_t *b, const rx_profile_t *p,
                     unsigned link, int64_t now, bool peer_waiting)
{
    if (link > p->peer_count || now < 0 || now > INT64_MAX - 100000
        || (b->initialized && now < b->last_us)) {
        b->trial_ended = true;
        return false;
    }
    bool open = rx_budget_peer_open(b, p, now);
    b->last_us = now; b->initialized = true;
    int64_t second = now / 1000000;
    if (second != b->second) {
        b->second = second;
        memset(b->used, 0, sizeof(b->used));
    }
    unsigned peers_used = b->used[1] + b->used[2] + b->used[3];
    unsigned peer_hz = p->rate_hz / 5u;
    if (now < b->next_total_us) return false;
    if (link == 0) {
        if (b->used[0] >= (open ? p->rate_hz - peer_hz : p->rate_hz)
            || (open && peer_waiting && peers_used < peer_hz && now >= b->next_peer_us)) return false;
    } else {
        if (!open || peers_used >= peer_hz || now < b->next_peer_us) return false;
        /* Rotate the remainder: each of three links receives 10 opportunities
         * per three seconds, without letting a missing source hide a gap. */
        unsigned quota = peer_hz / p->peer_count;
        if ((link - 1 + p->peer_count - (unsigned)(second % p->peer_count)) % p->peer_count
            < peer_hz % p->peer_count) ++quota;
        if (b->used[link] >= quota) return false;
        b->next_peer_us = now + (1000000u + peer_hz - 1u) / peer_hz;
    }
    ++b->used[link];
    b->next_total_us = now + (1000000u + p->rate_hz - 1u) / p->rate_hz;
    return true;
}

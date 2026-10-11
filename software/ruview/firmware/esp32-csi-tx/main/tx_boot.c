#include "tx_boot.h"
#include "tx_diag.h"
void tx_boot_begin(tx_boot_check_t *c, uint64_t start, uint64_t now,
                   const tx_runtime_status_t *status)
{
    *c = (tx_boot_check_t){.started_us = start, .observed_us = now,
                           .last_sample_us = now, .last_poll_us = now,
                           .last_sent = status->sent_packets};
}
int tx_boot_poll(tx_boot_check_t *c, uint64_t now, const tx_runtime_status_t *s,
                 bool network_healthy, uint32_t free_heap)
{
    /* Preserve the boot rules; report each failed predicate before cleanup.
       These are local observations, not delivery, RF or external RX proof. */
#define BOOT_REQUIRE(field, rule, actual, condition) do { \
    if (!(condition)) { \
        ESP_LOGE("tx_boot", "TX_COMPARE op=tx_boot_poll rc=0x%x field=%s expected=%s actual=%llu boot_us=%llu observed_us=%llu last_sample_us=%llu", \
                 (unsigned)ESP_ERR_INVALID_STATE, field, rule, (unsigned long long)(actual), \
                 (unsigned long long)now, (unsigned long long)c->observed_us, (unsigned long long)c->last_sample_us); \
        c->failed = true; \
    } \
} while (0)
    BOOT_REQUIRE("monotonic_time", ">= last_poll_us", now, now >= c->last_poll_us);
    if (now >= c->last_poll_us) c->last_poll_us = now;
    BOOT_REQUIRE("boot_elapsed_us", "< 120000000", now - c->started_us, now - c->started_us < TX_BOOT_BUDGET_US);
    BOOT_REQUIRE("network_healthy", "1 (see getter/field diagnostics)", network_healthy, network_healthy);
    BOOT_REQUIRE("free_heap_bytes", ">= 32768", free_heap, free_heap >= 32768);
    BOOT_REQUIRE("send_errors", "0", s->send_errors, s->send_errors == 0);
    BOOT_REQUIRE("wifi_restarts", "0", s->wifi_restarts, s->wifi_restarts == 0);
    if (now >= c->last_sample_us && now - c->last_sample_us >= TX_BOOT_SAMPLE_US) {
        BOOT_REQUIRE("sample_gap_us", "<= 6000000", now - c->last_sample_us,
                     now - c->last_sample_us <= TX_BOOT_SAMPLE_US + UINT64_C(1000000));
        BOOT_REQUIRE("sent_packets_delta", "!= 0 per sample", (uint32_t)(s->sent_packets - c->last_sent), s->sent_packets != c->last_sent);
        c->last_sample_us = now;
        c->last_sent = s->sent_packets;
    }
#undef BOOT_REQUIRE
    if (c->failed) return -1;
    return now - c->observed_us >= TX_BOOT_OBSERVE_US ? 1 : 0;
}

#ifndef TX_BOOT_H
#define TX_BOOT_H
#include "tx_ota.h"
#define TX_BOOT_BUDGET_US UINT64_C(120000000)
#define TX_BOOT_OBSERVE_US UINT64_C(60000000)
#define TX_BOOT_SAMPLE_US UINT64_C(5000000)
typedef struct {
    uint64_t started_us, observed_us, last_sample_us, last_poll_us;
    uint32_t last_sent;
    bool failed;
} tx_boot_check_t;
void tx_boot_begin(tx_boot_check_t *check, uint64_t boot_start, uint64_t now,
                   const tx_runtime_status_t *status);
/* now is the monotonic completion time AFTER network/heap/status collection.
   Only an executed, due probe advances last_sample_us. The initial value is
   the observation anchor; it does not count as a completed network sample.
   0 = pending, 1 = locally healthy, -1 = failed. No flash/OTA side effects. */
int tx_boot_poll(tx_boot_check_t *check, uint64_t now, const tx_runtime_status_t *status,
                 bool network_healthy, uint32_t free_heap);
#endif

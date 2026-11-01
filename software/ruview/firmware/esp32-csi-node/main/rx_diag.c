#include "rx_diag.h"
#include "csi_collector.h"
#include <string.h>
#include "esp_heap_caps.h"
#include "esp_timer.h"
#include "esp_attr.h"
#include "freertos/FreeRTOS.h"
static portMUX_TYPE lock = portMUX_INITIALIZER_UNLOCKED;
static struct {
    uint32_t count[RX_CLASSES][RX_STAGES];
    uint32_t grid_stages[RX_STAGES]; /* own-source, 128 bins, 2437 MHz, PPDU 0, accepted flags */
    uint32_t features[RX_CLASSES][8]; /* sig_mode[0..3], cwb, stbc, first-invalid, rx_state */
    uint32_t last_sequence[RX_CLASSES];
    bool has_sequence[RX_CLASSES];
    bool saturated;
    uint32_t allocation_failures;
    int64_t allocation_counter_start_us;
} state;
_Static_assert(offsetof(rx_diag_snapshot_t, first_failure_state) >= sizeof(state), "counter prefix layout");
static uint32_t allocation_failures;
static int64_t allocation_counter_start_us;
/* Only 32-bit atomics on S3. Payload is immutable after release publication. */
static DRAM_ATTR struct {
    uint32_t publication;
    uint32_t http_phases; /* eight independent 3-bit active-scope counters */
    rx_allocation_failure_t payload;
} first_allocation_failure;
_Static_assert(sizeof(first_allocation_failure) <= 64, "first-failure RAM budget");
_Static_assert(__atomic_always_lock_free(4, 0), "S3 lock-free diagnostic words");

rx_http_scope_t rx_http_enter(unsigned phase) {
    if (phase < RX_HTTP_STATUS || phase > RX_HTTP_OTA_ABORT) return (rx_http_scope_t){0};
    uint32_t step = 1u << ((phase - 1) * 3);
    uint32_t old = __atomic_load_n(&first_allocation_failure.http_phases, __ATOMIC_RELAXED);
    do {
        /* Fail closed before entering a handler if a lane is exhausted. */
        if ((old & (7u * step)) == 7u * step) return (rx_http_scope_t){0};
    } while (!__atomic_compare_exchange_n(&first_allocation_failure.http_phases, &old,
                old + step, false, __ATOMIC_RELAXED, __ATOMIC_RELAXED));
    return (rx_http_scope_t){phase};
}
void rx_http_leave(rx_http_scope_t *scope) {
    if (scope->phase) {
        __atomic_fetch_sub(&first_allocation_failure.http_phases,
                           1u << ((scope->phase - 1) * 3), __ATOMIC_RELAXED);
        scope->phase = 0;
    }
}
void rx_http_change(rx_http_scope_t *scope, unsigned phase) {
    /* Claim new phase before releasing old context; no false idle interval. */
    rx_http_scope_t next = rx_http_enter(phase);
    if (next.phase) { rx_http_leave(scope); *scope = next; }
}
static void IRAM_ATTR allocation_failed(size_t bytes,uint32_t caps,const char *function) {
    uint32_t empty = RX_FAILURE_EMPTY;
    if (__atomic_compare_exchange_n(&first_allocation_failure.publication, &empty,
            RX_FAILURE_WRITING, false, __ATOMIC_RELAXED, __ATOMIC_RELAXED)) {
        /* Pinned systimer time is IRAM, lock/allocation-free; core ID is inline.
         * API address is opaque. Task and ISR context remain explicitly unknown.
         * HTTP activity is simultaneous context, never writer attribution. */
        first_allocation_failure.payload.time_us = esp_timer_get_time();
        first_allocation_failure.payload.requested_bytes = bytes;
        first_allocation_failure.payload.api_id = (uintptr_t)function;
        first_allocation_failure.payload.caps = caps;
        first_allocation_failure.payload.core_id = xPortGetCoreID();
        first_allocation_failure.payload.http_phases =
            __atomic_load_n(&first_allocation_failure.http_phases, __ATOMIC_RELAXED);
        __atomic_store_n(&first_allocation_failure.publication, RX_FAILURE_COMPLETE, __ATOMIC_RELEASE);
    }
    uint32_t value=__atomic_load_n(&allocation_failures,__ATOMIC_RELAXED);
    while(value!=UINT32_MAX && !__atomic_compare_exchange_n(&allocation_failures,&value,value+1,false,__ATOMIC_RELAXED,__ATOMIC_RELAXED)) {}
}
esp_err_t rx_diag_init(void) {
    allocation_counter_start_us=esp_timer_get_time();
    return heap_caps_register_failed_alloc_callback(allocation_failed);
}
static void add(uint32_t *value) {
    if (*value == UINT32_MAX) state.saturated = true;
    else ++*value;
}
unsigned rx_diag_class(int len, bool valid) {
    if (!valid || len <= 0 || (len & 1)) return RX_INVALID;
    return len == 128 ? RX64 : len == 256 ? RX128 : len == 384 ? RX192 : RX_OTHER;
}
unsigned rx_diag_packet(const uint8_t *data, size_t len) {
    if (!data || len < 20 || data[0] != 1 || data[1] != 0 || data[2] != 0x11 || data[3] != 0xc5)
        return RX_OTHER; /* sync packets are not CSI; caller excludes them */
    /* Header dimensions name the IQ payload; additive trailers are not bins. */
    size_t iq_bytes=(size_t)data[5]*((size_t)data[6]|(size_t)data[7]<<8)*2;
    if(!iq_bytes || iq_bytes>CSI_MAX_IQ_SIZE || len != CSI_HEADER_SIZE + iq_bytes
        + CSI_SOURCE_BINDING_SIZE + CSI_IDENTITY_TRAILER_SIZE + CSI_PHYSICAL_TRAILER_SIZE) return RX_INVALID;
    unsigned format=rx_diag_class((int)iq_bytes,true);
    uint32_t frequency = 0;
    memcpy(&frequency, data+8, sizeof(frequency));
    const uint8_t *identity = data + CSI_HEADER_SIZE + iq_bytes + CSI_SOURCE_BINDING_SIZE;
    if (format == RX128 && frequency == 2437 && data[18] == 0 &&
        !(data[19] & ~(0x10u|0x20u)) && (data[20+256+5] & 7u) == 7u
        && identity[45] == 0 && identity[108] == 1) format |= RX_GRID_CANDIDATE;
    return format;
}
void rx_diag_count(unsigned format, unsigned stage) {
    unsigned base = format & 0xffu;
    if (base >= RX_CLASSES || stage >= RX_STAGES) return;
    portENTER_CRITICAL(&lock);
    add(&state.count[base][stage]);
    if (format & RX_GRID_CANDIDATE) add(&state.grid_stages[stage]);
    portEXIT_CRITICAL(&lock);
}
void rx_diag_enter(unsigned format, unsigned features, bool invalid, bool foreign) {
    bool grid = format & RX_GRID_CANDIDATE; format &= 0xffu;
    portENTER_CRITICAL(&lock);
    add(&state.count[format][RX_ENTER]);
    if (grid) add(&state.grid_stages[RX_ENTER]);
    if (invalid) {
        add(&state.count[format][RX_INVALID_DATA]);
        if (grid) add(&state.grid_stages[RX_INVALID_DATA]);
    }
    if (foreign) add(&state.count[format][RX_FOREIGN_OBSERVED]);
    for (unsigned bit = 0; bit < 8; ++bit) if (features & (1u << bit)) add(&state.features[format][bit]);
    portEXIT_CRITICAL(&lock);
}
void rx_diag_serialized(unsigned format, uint32_t sequence) {
    bool grid = format & RX_GRID_CANDIDATE; format &= 0xffu;
    portENTER_CRITICAL(&lock); add(&state.count[format][RX_SERIALIZED]);
    if (grid) add(&state.grid_stages[RX_SERIALIZED]);
    state.last_sequence[format] = sequence; state.has_sequence[format] = true;
    portEXIT_CRITICAL(&lock);
}
void rx_diag_snapshot(rx_diag_snapshot_t *snapshot) {
    portENTER_CRITICAL(&lock); memcpy(snapshot, &state, sizeof(state)); portEXIT_CRITICAL(&lock);
    snapshot->first_failure_state = __atomic_load_n(&first_allocation_failure.publication, __ATOMIC_ACQUIRE);
    if (snapshot->first_failure_state == RX_FAILURE_COMPLETE)
        snapshot->first_failure = first_allocation_failure.payload;
    else memset(&snapshot->first_failure, 0, sizeof(snapshot->first_failure));
    snapshot->allocation_failures=__atomic_load_n(&allocation_failures,__ATOMIC_RELAXED);
    snapshot->allocation_counter_start_us=allocation_counter_start_us;
    if(snapshot->allocation_failures==UINT32_MAX)snapshot->saturated=true;
}

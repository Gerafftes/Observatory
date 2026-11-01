#pragma once
#include <stdint.h>
#include "esp_err.h"
#include <stddef.h>
#include <stdbool.h>
#define RX_GRID_CANDIDATE 0x100u
enum { RX64, RX128, RX192, RX_OTHER, RX_INVALID, RX_CLASSES };
enum { RX_ENTER, RX_INVALID_DATA, RX_FOREIGN_OBSERVED, RX_EARLY_DROP,
       RX_FOREIGN_DROP, RX_SERIALIZED, RX_SERIALIZE_FAIL, RX_LATE_DROP,
       RX_QUEUE_OK, RX_QUEUE_DROP, RX_SEND_ATTEMPT, RX_SEND_OK,
       RX_SEND_ERROR, RX_ABANDONED, RX_STAGES };
enum { RX_HTTP_IDLE, RX_HTTP_STATUS, RX_HTTP_CONFIG, RX_HTTP_DIAG,
       RX_HTTP_OTA_BEGIN, RX_HTTP_OTA_RECV, RX_HTTP_OTA_WRITE,
       RX_HTTP_OTA_END, RX_HTTP_OTA_ABORT };
enum { RX_FAILURE_EMPTY, RX_FAILURE_WRITING, RX_FAILURE_COMPLETE };
typedef struct {
    int64_t time_us;
    size_t requested_bytes;
    uintptr_t api_id; /* opaque address only; never dereference */
    uint32_t caps, core_id, http_phases;
} rx_allocation_failure_t;
typedef struct { unsigned phase; } rx_http_scope_t;
rx_http_scope_t rx_http_enter(unsigned phase);
void rx_http_leave(rx_http_scope_t *scope);
void rx_http_change(rx_http_scope_t *scope, unsigned phase);
/* GCC cleanup closes all early returns; scopes count concurrent/nested handlers. */
#define RX_HTTP_SCOPE(name, phase_id) \
    rx_http_scope_t name __attribute__((cleanup(rx_http_leave))) = rx_http_enter(phase_id); \
    if (!(name).phase) return ESP_FAIL

typedef struct {
    uint32_t count[RX_CLASSES][RX_STAGES];
    uint32_t grid_stages[RX_STAGES]; /* own-source, 128 bins, 2437 MHz, PPDU 0, accepted flags */
    uint32_t features[RX_CLASSES][8]; /* sig_mode[0..3], cwb, stbc, first-invalid, rx_state */
    uint32_t last_sequence[RX_CLASSES];
    bool has_sequence[RX_CLASSES];
    bool saturated;
    uint32_t allocation_failures;
    int64_t allocation_counter_start_us;
    uint32_t first_failure_state;
    rx_allocation_failure_t first_failure;
} rx_diag_snapshot_t;
esp_err_t rx_diag_init(void);
unsigned rx_diag_class(int len, bool valid);
unsigned rx_diag_packet(const uint8_t *data, size_t len);
void rx_diag_count(unsigned format, unsigned stage);
void rx_diag_enter(unsigned format, unsigned features, bool invalid, bool foreign);
void rx_diag_serialized(unsigned format, uint32_t sequence);
void rx_diag_snapshot(rx_diag_snapshot_t *snapshot);

#pragma once
#include <stdbool.h>
#include <stdint.h>
#include <stddef.h>
#include "esp_err.h"
/* Counts saturate; a last-* record exists only when its matching count is >0.
 * Timestamps are app-boot esp_timer microseconds, never wall time. */
typedef struct {
    uint32_t disconnect_count, callback_count, connect_count, arm_count, got_ip_count;
    uint32_t retry_count;
    uint32_t event_seq, connect_started_count, log_suppressed, arm_delay_ms;
    uint32_t disconnect_reason;
    int32_t connect_rc, arm_rc;
    int64_t connect_begin_us;
    int64_t disconnect_us, callback_us, connect_us, arm_us, got_ip_us, sample_us;
    bool connected, timer_active, connect_in_flight, saturated;
    uint32_t connect_origin; /* 1=STA_START, 2=one-shot callback; 0=unobserved */
} wifi_reconnect_snapshot_t;
void wifi_reconnect_snapshot(wifi_reconnect_snapshot_t *snapshot);
#define WIFI_RECONNECT_EVENT_CAPACITY 16u
typedef struct { uint32_t event, sequence; int64_t time_us; int32_t result; } wifi_reconnect_event_t;
/* Fixed per-boot RAM history, including events suppressed in the serial log. */
size_t wifi_reconnect_events(wifi_reconnect_event_t *out, size_t capacity);

esp_err_t rx_wifi_start(bool *connected);

#pragma once

#include <stdbool.h>
#include <stdint.h>

typedef struct {
    uint64_t uart_bytes_received;
    uint64_t radar_frames_valid;
    uint64_t udp_packets_sent;
    uint64_t udp_send_failures;
} node_diagnostics_snapshot_t;

void node_diagnostics_add_uart_bytes(uint32_t count);
void node_diagnostics_record_radar_frame(void);
void node_diagnostics_record_udp_send(bool sent);
node_diagnostics_snapshot_t node_diagnostics_snapshot(void);

#pragma once

#include <stdbool.h>
#include <stdint.h>

typedef struct {
    uint64_t uart_bytes_received;
    uint64_t radar_frames_valid;
    uint64_t udp_packets_sent;
    uint64_t udp_send_failures;
    uint64_t udp_packets_acked;
    uint64_t udp_retransmissions;
    uint64_t udp_ack_timeouts;
} node_diagnostics_snapshot_t;

void node_diagnostics_add_uart_bytes(uint32_t count);
void node_diagnostics_record_radar_frame(void);
void node_diagnostics_record_udp_send(bool sent, bool acknowledged,
                                      uint8_t attempts);
node_diagnostics_snapshot_t node_diagnostics_snapshot(void);

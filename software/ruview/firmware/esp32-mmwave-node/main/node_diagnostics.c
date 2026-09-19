#include "node_diagnostics.h"

#include "freertos/FreeRTOS.h"

static portMUX_TYPE s_lock = portMUX_INITIALIZER_UNLOCKED;
static node_diagnostics_snapshot_t s_diagnostics;

void node_diagnostics_add_uart_bytes(uint32_t count)
{
    portENTER_CRITICAL(&s_lock);
    s_diagnostics.uart_bytes_received += count;
    portEXIT_CRITICAL(&s_lock);
}

void node_diagnostics_record_radar_frame(void)
{
    portENTER_CRITICAL(&s_lock);
    s_diagnostics.radar_frames_valid += 1;
    portEXIT_CRITICAL(&s_lock);
}

void node_diagnostics_record_udp_send(bool sent, bool acknowledged,
                                      uint8_t attempts)
{
    portENTER_CRITICAL(&s_lock);
    if (sent) {
        s_diagnostics.udp_packets_sent += 1;
    } else {
        s_diagnostics.udp_send_failures += 1;
    }
    if (acknowledged) {
        s_diagnostics.udp_packets_acked += 1;
    } else {
        s_diagnostics.udp_ack_timeouts += 1;
    }
    if (attempts > 1) {
        s_diagnostics.udp_retransmissions += attempts - 1;
    }
    portEXIT_CRITICAL(&s_lock);
}

node_diagnostics_snapshot_t node_diagnostics_snapshot(void)
{
    portENTER_CRITICAL(&s_lock);
    node_diagnostics_snapshot_t snapshot = s_diagnostics;
    portEXIT_CRITICAL(&s_lock);
    return snapshot;
}

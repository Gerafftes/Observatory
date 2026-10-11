/**
 * @file stream_sender.h
 * @brief UDP stream sender for CSI frames.
 */

#ifndef STREAM_SENDER_H
#define STREAM_SENDER_H

#include <stdint.h>
#include <stddef.h>
#include <stdbool.h>

typedef struct {
    uint32_t socket_rebuilds;
    uint32_t send_failures;
    uint32_t enomem_backoffs;
    uint32_t send_calls;
    uint32_t invalid_calls;
    uint32_t socket_unavailable;
    uint32_t backoff_suppressed;
    uint32_t sendto_attempts;
    uint32_t sendto_failures;
    uint32_t os_datagrams_accepted;
    uint32_t partial_sends;
    uint32_t csi_send_calls;
    uint32_t csi_socket_unavailable;
    uint32_t csi_backoff_suppressed;
    uint32_t csi_sendto_attempts;
    uint32_t csi_sendto_failures;
    uint32_t csi_os_datagrams_accepted;
    int64_t last_sendto_attempt_us;
    int64_t last_os_accept_us;
} stream_sender_stats_t;

/**
 * Initialize the UDP sender.
 * Creates a UDP socket targeting the configured aggregator.
 *
 * @return 0 on success, -1 on error.
 */
int stream_sender_init(void);

/**
 * Initialize the UDP sender with explicit IP and port.
 * Used when configuration is loaded from NVS at runtime.
 *
 * @param ip   Aggregator IP address string (e.g. "192.168.1.20").
 * @param port Aggregator UDP port.
 * @return 0 on success, -1 on error.
 */
int stream_sender_init_with(const char *ip, uint16_t port);

/** Atomically replace the target; the sender task rebuilds the socket lazily. */
int stream_sender_reconfigure(const char *ip, uint16_t port);

/** Mark the socket stale after a WiFi reconnect or local IP change. */
void stream_sender_notify_network_change(void);

/**
 * Send a serialized CSI frame over UDP.
 *
 * @param data Frame data buffer.
 * @param len  Length of data to send.
 * @return Number of bytes sent, or -1 on error.
 */
int stream_sender_send(const uint8_t *data, size_t len);

/**
 * Send only while the monotonic @p deadline_us has not passed. Socket-lock
 * wait and sendto are both guarded so a CSI retry cannot transmit a stale
 * frame after another stream user held the shared socket.
 */
int stream_sender_send_until(
    const uint8_t *data,
    size_t len,
    int64_t deadline_us,
    bool *deadline_expired
);

void stream_sender_get_stats(stream_sender_stats_t *stats);

#ifdef CSI_DIAGNOSTICS_TEST
void stream_sender_test_reset(void);
void stream_sender_test_set_sendto_result(int result, int error_number);
size_t stream_sender_test_copy_last_payload(uint8_t *buffer, size_t buffer_size);
#endif

/**
 * Close the UDP sender socket.
 */
void stream_sender_deinit(void);

#endif /* STREAM_SENDER_H */

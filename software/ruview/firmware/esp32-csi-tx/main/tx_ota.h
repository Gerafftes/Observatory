#ifndef CSI_TX_OTA_H
#define CSI_TX_OTA_H

#include <stdbool.h>
#include <stdint.h>
#include "tx_config.h"

typedef enum {
    TX_SEND_OP_NONE,
    TX_SEND_OP_SOCKET,
    TX_SEND_OP_BROADCAST,
    TX_SEND_OP_SENDTO,
} tx_send_operation_t;

typedef struct {
    tx_send_operation_t operation;
    int result;
    int error_number;
    bool errno_valid;
    uint32_t sequence;
    uint64_t occurred_us;
} tx_send_error_t;

typedef enum {
    TX_SEND_PATH_NONE,
    TX_SEND_PATH_NETBUF_ALLOCATION,
    TX_SEND_PATH_WIFI_HANDOFF,
    TX_SEND_PATH_UNCLASSIFIED,
} tx_send_path_stage_t;

typedef struct {
    tx_send_path_stage_t stage;
    uint32_t sequence;
    uint64_t capture_begin_us;
    uint64_t capture_end_us;
    uint32_t netbuf_calls;
    bool netbuf_returned_nonnull;
    uint32_t wifi_calls;
    int wifi_return;
} tx_send_path_error_t;

typedef struct {
    uint32_t own_attempts;
    uint32_t netbuf_calls;
    uint32_t wifi_handoffs;
    uint32_t enomem_netbuf_allocation;
    uint32_t enomem_wifi_handoff;
    uint32_t enomem_other_or_unclassified;
    tx_send_path_error_t last_enomem;
} tx_send_path_status_t;

typedef struct {
    int boot_check;
    uint32_t sequence;
    uint32_t sent_packets;
    uint32_t send_errors;
    uint32_t socket_rebuilds;
    uint32_t wifi_restarts;
    uint32_t missed_deadlines;
    uint32_t jitter_max_us;
    uint64_t jitter_sum_us;
    uint32_t jitter_samples;
    uint32_t station_count;
    uint64_t station_generation;
    uint32_t socket_errors;
    uint32_t broadcast_errors;
    uint32_t sendto_errors;
    uint32_t short_sends;
    tx_send_error_t last_send_error;
    tx_send_path_status_t send_path;
} tx_runtime_status_t;

typedef void (*tx_status_provider_t)(tx_runtime_status_t *status);

esp_err_t tx_ota_start(uint64_t boot_id, const tx_config_t *config,
                       const char *source_mac_sha256, tx_status_provider_t provider);
esp_err_t tx_ota_complete_boot_self_test(bool runtime_healthy);
bool tx_ota_token_matches(const char *secret, const char *header);

#endif

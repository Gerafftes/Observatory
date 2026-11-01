#ifndef CSI_PEER_H
#define CSI_PEER_H
#include "esp_err.h"
#include "rx_profile.h"

typedef struct {
    /* captured counts reserved capture lifetimes, including copy/publish
     * failures that end in aborted; it is not an RF/UDP delivery count. */
    uint32_t observed, admitted, rejected, captured, queue_drops, queued, high_watermark;
    uint32_t delivered, abandoned, expired, serialize_errors, aborted;
} csi_peer_link_stats_t;
typedef struct {
    bool enabled, session_ended, task_running, radio_ready;
    uint32_t raw_send_attempts, raw_send_errors, task_stack_min_bytes;
    csi_peer_link_stats_t links[4];
} csi_peer_stats_t;

esp_err_t csi_peer_start(void);
esp_err_t csi_peer_prepare_radio(void);
void csi_peer_stop(void);
int csi_peer_classify(const uint8_t mac[6]);
/* Supplied times describe capture/dispatch observations. The shared admission
 * budget samples its own current clock while holding the peer lock. */
bool csi_peer_admit(const uint8_t mac[6], size_t iq_len, int64_t now);
bool csi_peer_queue_reserve(int link);
void csi_peer_capture_result(int link, bool accepted);
bool csi_peer_deferred_admit(int link, int64_t callback_us, int64_t now);
void csi_peer_finish(int link, bool serialized, bool delivered, bool expired);
void csi_peer_aborted(int link);
void csi_peer_get_stats(csi_peer_stats_t *out);
const uint8_t *csi_peer_source_hash(int link);
const uint8_t *csi_peer_self_mac(void);
#endif

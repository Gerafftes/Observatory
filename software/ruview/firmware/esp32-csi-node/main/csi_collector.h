/**
 * @file csi_collector.h
 * @brief CSI data collection and ADR-018 binary frame serialization.
 */

#ifndef CSI_COLLECTOR_H
#define CSI_COLLECTOR_H

#include <stdint.h>
#include <stddef.h>
#include "esp_err.h"
#include "esp_wifi_types.h"

/** ADR-018 magic number. */
#define CSI_MAGIC 0xC5110001

/** ADR-018 header size in bytes. */
#define CSI_HEADER_SIZE 20

/**
 * Runtime TX source-binding trailer appended after the ADR-018 I/Q payload.
 *
 * Wire layout (all multi-byte integers are little-endian):
 *   [0..3]   Magic 0xC511A060
 *   [4]      Version (1)
 *   [5]      Flags (CSI_SOURCE_BINDING_FLAG_*)
 *   [6..7]   Trailer length (40)
 *   [8..39]  SHA-256 over the six binary runtime filter-MAC bytes
 */
#define CSI_SOURCE_BINDING_MAGIC   0xC511A060u
#define CSI_SOURCE_BINDING_VERSION 1u
#define CSI_SOURCE_BINDING_SIZE    40u

/** Runtime filter is configured and enforced by the CSI callback. */
#define CSI_SOURCE_BINDING_FLAG_FILTER_ENFORCED  (1u << 0)
/** The source MAC on this CSI callback matched the runtime filter. */
#define CSI_SOURCE_BINDING_FLAG_SOURCE_MATCHED   (1u << 1)
/** The 32-byte filter identity digest was computed successfully. */
#define CSI_SOURCE_BINDING_FLAG_IDENTITY_VALID   (1u << 2)
#define CSI_SOURCE_BINDING_FLAGS_MASK             \
    (CSI_SOURCE_BINDING_FLAG_FILTER_ENFORCED      \
     | CSI_SOURCE_BINDING_FLAG_SOURCE_MATCHED     \
     | CSI_SOURCE_BINDING_FLAG_IDENTITY_VALID)

/** Versioned RX boot/timing/quality trailer, appended after source binding. */
#define CSI_IDENTITY_MAGIC        0xC511A061u
#define CSI_PROTOCOL_VERSION      3u
#define CSI_IDENTITY_ROLE_RX      1u
#define CSI_IDENTITY_V2_TRAILER_SIZE 24u
#define CSI_IDENTITY_TRAILER_SIZE 48u

#define CSI_QUALITY_FIRST_WORD_INVALID (1u << 0)
#define CSI_QUALITY_NOISE_FLOOR_VALID  (1u << 1)
#define CSI_QUALITY_RX_STATE_VALID     (1u << 2)
#define CSI_QUALITY_GAIN_AVAILABLE     (1u << 3)
#define CSI_QUALITY_RAW_PROFILE        (1u << 4)
#define CSI_QUALITY_SYNC_VALID         (1u << 5)

/** Capture-configuration flags: ESP32-S3 LLTF/HT-LTF and C6 legacy/HT/HE. */
#define CSI_CONFIG_LEGACY_OR_LLT       (1u << 0)
#define CSI_CONFIG_HT_LTF              (1u << 1)
#define CSI_CONFIG_HE_OR_VHT           (1u << 2)
#define CSI_CONFIG_STBC_HTLTF2         (1u << 3)

/**
 * Maximum I/Q payload and frame buffer size:
 * header + 4 antennas * 256 subcarriers * 2 bytes + binding/identity trailers.
 */
#define CSI_MAX_IQ_SIZE (4 * 256 * 2)
#define CSI_MAX_FRAME_SIZE \
    (CSI_HEADER_SIZE + CSI_MAX_IQ_SIZE + CSI_SOURCE_BINDING_SIZE \
     + CSI_IDENTITY_TRAILER_SIZE)

/** Maximum number of channels in the hop table (ADR-029). */
#define CSI_HOP_CHANNELS_MAX 6

/**
 * Additive receive-path diagnostics. The first three equations define the
 * per-boot callback balance without relying on the wire sequence:
 *
 * callbacks_received = invalid_inputs + foreign_tx + rate_limited + admitted
 * admitted = capture_handoff_attempts
 * capture_handoff_attempts = capture_handoff_accepted + capture_handoff_drops
 */
typedef struct {
    uint32_t callbacks_received;
    uint32_t invalid_inputs;
    uint32_t foreign_tx;
    uint32_t rate_limited;
    uint32_t admitted;
    uint32_t capture_handoff_attempts;
    uint32_t capture_handoff_accepted;
    uint32_t capture_handoff_drops;
    uint64_t last_callback_started_us;
    uint64_t callback_duration_total_us;
    uint32_t callback_duration_max_us;
    uint32_t callback_duration_samples;
    uint32_t callback_stack_free_min_bytes;
    bool callback_stack_runtime_measured;
} csi_collector_stats_t;

typedef struct {
    uint32_t start_attempts;
    uint32_t start_failures;
    uint32_t gateway_unavailable;
    uint32_t restarts;
    uint32_t replies;
    uint32_t timeouts;
    uint32_t last_reply_interval_us;
    uint32_t reply_interval_max_us;
    uint64_t reply_interval_total_us;
    uint32_t reply_interval_samples;
    uint32_t configured_interval_ms;
    uint32_t configured_timeout_ms;
    bool enabled;
    bool running;
} csi_ping_stats_t;

/**
 * Initialize CSI collection.
 * Registers the WiFi CSI callback.
 */
void csi_collector_init(void);

/**
 * Capture node_id BEFORE wifi_init_sta() or any other heavy init.
 *
 * Must be called from app_main() immediately after nvs_config_load().
 * WiFi driver initialization can corrupt g_nvs_config.node_id (confirmed
 * on device 80:b5:4e:c1:be:b8, NVS=3 but post-WiFi reads as 1).
 * This early capture shields s_node_id from that corruption window.
 *
 * @param node_id Value from g_nvs_config.node_id, read right after NVS load.
 */
void csi_collector_set_node_id(uint8_t node_id);

/** Set the per-boot random identity before CSI collection starts. */
void csi_collector_set_boot_id(uint64_t boot_id);

/**
 * Get the runtime node_id (early capture if available, otherwise init-time).
 *
 * Other modules (edge_processing, wasm_runtime, display_ui) should prefer
 * this accessor over reading g_nvs_config.node_id directly.
 *
 * @return Node ID (0-255) as loaded from NVS at boot.
 */
uint8_t csi_collector_get_node_id(void);

/** Return the immutable boot ID emitted in every version-2 CSI frame. */
uint64_t csi_collector_get_boot_id(void);

/** Latest serialized RX-local CSI sequence high-water mark. */
uint32_t csi_collector_get_sequence(void);

/** Latest accepted CSI RSSI, or -128 before the first callback. */
int8_t csi_collector_get_rssi(void);
uint32_t csi_collector_get_csi_config_flags(void);

/** Snapshot callback admission, pre-sequence losses, and callback timing. */
void csi_collector_get_stats(csi_collector_stats_t *stats);

void csi_collector_get_ping_stats(csi_ping_stats_t *stats);

void csi_collector_notify_network_change(bool connected);

/**
 * Serialize CSI data into ADR-018 binary frame format.
 *
 * @param info   WiFi CSI info from the ESP-IDF callback.
 * @param buf    Output buffer (must be at least CSI_MAX_FRAME_SIZE bytes).
 * @param buf_len Size of the output buffer.
 * The returned ADR-018 datagram always includes the 40-byte runtime
 * source-binding trailer after the I/Q bytes.
 *
 * @return Number of bytes written, including the trailer, or 0 on error.
 */
size_t csi_serialize_frame(const wifi_csi_info_t *info, uint8_t *buf, size_t buf_len);

size_t csi_serialize_frame_at(
    const wifi_csi_info_t *info,
    uint64_t callback_started_at_us,
    uint8_t *buf,
    size_t buf_len
);

/**
 * Configure the channel-hop table for multi-band sensing (ADR-029).
 *
 * When hop_count == 1 the collector stays on the single configured channel
 * (backward-compatible with the original single-channel mode).
 *
 * @param channels  Array of WiFi channel numbers (1-14 for 2.4 GHz, 36-177 for 5 GHz).
 * @param hop_count Number of entries in the channels array (1..CSI_HOP_CHANNELS_MAX).
 * @param dwell_ms  Dwell time per channel in milliseconds (>= 10).
 */
void csi_collector_set_hop_table(const uint8_t *channels, uint8_t hop_count, uint32_t dwell_ms);

/**
 * Advance to the next channel in the hop table.
 *
 * Called by the hop timer callback. If hop_count <= 1 this is a no-op.
 * Calls esp_wifi_set_channel() internally.
 */
void csi_hop_next_channel(void);

/**
 * Start the channel-hop timer.
 *
 * Creates an esp_timer periodic callback that fires every dwell_ms
 * milliseconds, calling csi_hop_next_channel(). If hop_count <= 1
 * the timer is not started (single-channel backward-compatible mode).
 */
void csi_collector_start_hop_timer(void);

/**
 * Upgrade the promiscuous filter to capture DATA frames in addition to MGMT
 * (RuView#893/#521).
 *
 * Called on display-less boards: the MGMT-only filter (the #396 display-crash
 * workaround set in csi_collector_init) only fires the CSI callback on sparse
 * management frames, so yield collapses to 0 pps under real traffic and the
 * node looks dead. A board with no AMOLED panel has no QSPI/SPI-flash cache
 * contention, so it can safely capture DATA frames — restoring abundant CSI.
 * Display boards keep MGMT-only to avoid the #396 crash.
 */
void csi_collector_enable_data_capture(void);

/**
 * Inject an NDP (Null Data Packet) frame for sensing.
 *
 * Uses esp_wifi_80211_tx() to send a preamble-only frame (~24 us airtime)
 * that triggers CSI measurement at all receivers. This is the "sensing-first"
 * TX mechanism described in ADR-029.
 *
 * @return ESP_OK on success, or an error code.
 *
 * @note TODO: Full NDP frame construction. Currently sends a minimal
 *       null-data frame as a placeholder.
 */
esp_err_t csi_inject_ndp_frame(void);

/**
 * Get the recent CSI callback rate (per second).
 *
 * Computed as a sliding 1-second window over the internal s_cb_count
 * counter. Used by the ADR-081 radio abstraction layer to fill the
 * pkt_yield_per_sec field of rv_radio_health_t.
 *
 * @return Callbacks observed in the trailing ~1 second.
 */
uint16_t csi_collector_get_pkt_yield_per_sec(void);

/**
 * Get the cumulative UDP send-failure counter since boot.
 *
 * @return Number of stream_sender_send() failures recorded by the
 *         CSI callback path.
 */
uint16_t csi_collector_get_send_fail_count(void);

#ifdef CSI_DIAGNOSTICS_TEST
void csi_collector_test_reset(void);
void csi_collector_test_invoke_callback(wifi_csi_info_t *info);
#endif

#endif /* CSI_COLLECTOR_H */

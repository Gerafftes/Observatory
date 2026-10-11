/**
 * @file csi_collector.c
 * @brief CSI data collection and ADR-018 binary frame serialization.
 *
 * Registers the ESP-IDF WiFi CSI callback and serializes incoming CSI data
 * into the ADR-018 binary frame format for UDP transmission.
 *
 * ADR-029 extensions:
 *   - Channel-hop table for multi-band sensing (channels 1/6/11 by default)
 *   - Timer-driven channel hopping at configurable dwell intervals
 *   - NDP frame injection stub for sensing-first TX
 */

#include "csi_collector.h"
#include "csi_transport.h"
#include "nvs_config.h"
#include "c6_timesync.h"  /* Serializer sync-valid flag on ESP32-C6. */
#include "c6_sync_espnow.h" /* Serializer sync-valid flag on every RX target. */

#include <limits.h>
#include <string.h>
#include "esp_log.h"
#include "esp_wifi.h"
#include "esp_timer.h"
#include "sdkconfig.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "esp_netif.h"          /* #954: STA gateway lookup for self-ping CSI source */
#include "ping/ping_sock.h"     /* #954: esp_ping gateway traffic generator */
#include "lwip/ip_addr.h"       /* #954: ip_addr_t target for esp_ping */
#include "mbedtls/sha256.h"

/* ADR-060: Access the global NVS config for MAC filter and channel override. */
extern nvs_config_t g_nvs_config;

/* Defensive fix (#232, #375, #385, #386, #390): capture NVS config fields into
 * module-local statics BEFORE wifi_init_sta() runs, because WiFi driver init
 * can corrupt g_nvs_config (confirmed on device 80:b5:4e:c1:be:b8).
 * main.c calls csi_collector_set_node_id() immediately after nvs_config_load(),
 * and all runtime paths use the local copies exclusively. */
static uint8_t s_node_id = 1;
static bool s_node_id_early_set = false;

/* Defensive copy of MAC filter config — the CSI callback fires at 100-500 Hz
 * and reads filter_mac_set + filter_mac on every invocation. If wifi_init_sta()
 * corrupts g_nvs_config, the callback would read garbage, potentially causing
 * LoadProhibited panics (observed: Core 0 panic after ~2400 callbacks). */
static uint8_t s_filter_mac[6] = {0};
static bool    s_filter_mac_set = false;
static uint8_t s_filter_identity_sha256[32] = {0};
static bool    s_filter_identity_valid = false;
static uint64_t s_boot_id = 1;
static int8_t s_latest_rssi = INT8_MIN;

/* ADR-057: Build-time guard — fail early if CSI is not enabled in sdkconfig.
 * Without this, the firmware compiles but crashes at runtime with:
 *   "E (xxxx) wifi:CSI not enabled in menuconfig!"
 * which is confusing for users flashing pre-built binaries. */
#ifndef CONFIG_ESP_WIFI_CSI_ENABLED
#error "CONFIG_ESP_WIFI_CSI_ENABLED must be set in sdkconfig. " \
       "Run: idf.py menuconfig -> Component config -> Wi-Fi -> Enable WiFi CSI, " \
       "or copy sdkconfig.defaults.template to sdkconfig.defaults before building."
#endif

static const char *TAG = "csi_collector";

static uint32_t s_sequence = 0;
static uint32_t s_cb_count = 0;
static TaskHandle_t s_callback_task;
static csi_collector_stats_t s_diagnostics;
static portMUX_TYPE s_diagnostics_lock = portMUX_INITIALIZER_UNLOCKED;

typedef enum {
    CSI_CALLBACK_INVALID,
    CSI_CALLBACK_FOREIGN_TX,
    CSI_CALLBACK_RATE_LIMITED,
    CSI_CALLBACK_ADMITTED,
} csi_callback_outcome_t;

static void csi_record_callback_diagnostics(
    csi_callback_outcome_t outcome,
    bool handoff_attempted,
    bool handoff_accepted,
    int64_t started_us
)
{
    int64_t elapsed_us = esp_timer_get_time() - started_us;
    uint32_t duration_us = elapsed_us > 0
        ? (elapsed_us > UINT32_MAX ? UINT32_MAX : (uint32_t)elapsed_us)
        : 0;

    portENTER_CRITICAL(&s_diagnostics_lock);
    s_diagnostics.callbacks_received++;
    switch (outcome) {
    case CSI_CALLBACK_INVALID:
        s_diagnostics.invalid_inputs++;
        break;
    case CSI_CALLBACK_FOREIGN_TX:
        s_diagnostics.foreign_tx++;
        break;
    case CSI_CALLBACK_RATE_LIMITED:
        s_diagnostics.rate_limited++;
        break;
    case CSI_CALLBACK_ADMITTED:
        s_diagnostics.admitted++;
        break;
    }
    if (handoff_attempted) {
        s_diagnostics.capture_handoff_attempts++;
        if (handoff_accepted) {
            s_diagnostics.capture_handoff_accepted++;
        } else {
            s_diagnostics.capture_handoff_drops++;
        }
    }
    s_diagnostics.last_callback_started_us = started_us > 0 ? (uint64_t)started_us : 0;
    s_diagnostics.callback_duration_total_us += duration_us;
    if (duration_us > s_diagnostics.callback_duration_max_us) {
        s_diagnostics.callback_duration_max_us = duration_us;
    }
    s_diagnostics.callback_duration_samples++;
    portEXIT_CRITICAL(&s_diagnostics_lock);
}

/**
 * Single callback-admission schedule for valid frames from the configured TX.
 *
 * CSI callbacks can fire hundreds of times per second in promiscuous mode.
 * A valid frame may arrive up to 500 us before its nominal deadline;
 * accepting it advances the deadline by one configured interval rather than
 * anchoring the next interval to the jittered arrival. A later arrival outside
 * that tolerance rebases the deadline, so missed slots never accumulate into
 * a catch-up burst.
 */
#ifndef CONFIG_CSI_CAPTURE_RATE_LIMIT_HZ
#define CONFIG_CSI_CAPTURE_RATE_LIMIT_HZ 50
#endif
#define CSI_FRAME_INTERVAL_US \
    (1000000LL / CONFIG_CSI_CAPTURE_RATE_LIMIT_HZ)
#define CSI_FRAME_EARLY_TOLERANCE_US 500
static int64_t s_next_frame_deadline_us = 0;

static bool csi_rate_limit_admit(int64_t now_us)
{
    if (s_next_frame_deadline_us == 0) {
        s_next_frame_deadline_us = now_us + CSI_FRAME_INTERVAL_US;
        return true;
    }

    if (now_us < s_next_frame_deadline_us - CSI_FRAME_EARLY_TOLERANCE_US) {
        return false;
    }

    if (now_us > s_next_frame_deadline_us + CSI_FRAME_EARLY_TOLERANCE_US) {
        s_next_frame_deadline_us = now_us + CSI_FRAME_INTERVAL_US;
    } else {
        s_next_frame_deadline_us += CSI_FRAME_INTERVAL_US;
    }
    return true;
}

uint32_t csi_collector_get_csi_config_flags(void)
{
    uint32_t flags = 0;
#if CONFIG_SOC_WIFI_HE_SUPPORT
    flags |= CSI_CONFIG_LEGACY_OR_LLT;
    flags |= CSI_CONFIG_HT_LTF;
    flags |= CSI_CONFIG_HE_OR_VHT;
#else
    flags |= CSI_CONFIG_LEGACY_OR_LLT;
    flags |= CSI_CONFIG_HT_LTF;
    flags |= CSI_CONFIG_STBC_HTLTF2;
#endif
    return flags;
}

/* ---- ADR-029: Channel-hop state ---- */

/** Channel hop table (populated from NVS at boot or via set_hop_table). */
static uint8_t  s_hop_channels[CSI_HOP_CHANNELS_MAX] = {1, 6, 11, 36, 40, 44};

/** Number of active channels in the hop table. 1 = single-channel (no hop). */
static uint8_t  s_hop_count   = 1;

/** Dwell time per channel in milliseconds. */
static uint32_t s_dwell_ms    = 50;

/** Current index into s_hop_channels. */
static uint8_t  s_hop_index   = 0;

/** Handle for the periodic hop timer. NULL when timer is not running. */
static esp_timer_handle_t s_hop_timer = NULL;

/**
 * Serialize CSI data into ADR-018 binary frame format.
 *
 * Layout:
 *   [0..3]   Magic: 0xC5110001 (LE)
 *   [4]      Node ID
 *   [5]      Number of antennas (rx_ctrl.rx_ant + 1 if available, else 1)
 *   [6..7]   Number of subcarriers (LE u16) = len / (2 * n_antennas)
 *   [8..11]  Frequency MHz (LE u32) — derived from channel
 *   [12..15] Sequence number (LE u32)
 *   [16]     RSSI (i8)
 *   [17]     Noise floor (i8)
 *   [18..19] Reserved
 *   [20..]   CSI complex samples (ESP-IDF order: imaginary, then real)
 *   [I/Q end..+39] Runtime TX source-binding trailer (see csi_collector.h)
 *   [binding end..] Versioned RX boot/timing/quality trailer
 */
size_t csi_serialize_frame_at(
    const wifi_csi_info_t *info,
    uint64_t callback_started_at_us,
    uint8_t *buf,
    size_t buf_len
)
{
    if (info == NULL || buf == NULL || info->buf == NULL) {
        return 0;
    }

    uint8_t n_antennas = 1;  /* ESP32-S3 typically reports 1 antenna for CSI */
    uint16_t iq_len = (uint16_t)info->len;
    uint16_t n_subcarriers = iq_len / (2 * n_antennas);

    size_t iq_end = CSI_HEADER_SIZE + iq_len;
    size_t binding_end = iq_end + CSI_SOURCE_BINDING_SIZE;
    size_t frame_size = binding_end + CSI_IDENTITY_TRAILER_SIZE;
    if (frame_size > buf_len) {
        ESP_LOGW(TAG, "Buffer too small: need %u, have %u", (unsigned)frame_size, (unsigned)buf_len);
        return 0;
    }

    /* Derive frequency from channel number */
    uint8_t channel = info->rx_ctrl.channel;
    uint32_t freq_mhz;
    if (channel >= 1 && channel <= 13) {
        freq_mhz = 2412 + (channel - 1) * 5;
    } else if (channel == 14) {
        freq_mhz = 2484;
    } else if (channel >= 36 && channel <= 177) {
        freq_mhz = 5000 + channel * 5;
    } else {
        freq_mhz = 0;
    }

    /* Magic (LE) */
    uint32_t magic = CSI_MAGIC;
    memcpy(&buf[0], &magic, 4);

    /* Node ID (captured at init into s_node_id to survive memory corruption
     * that could clobber g_nvs_config.node_id - see #232/#375/#385/#390). */
    buf[4] = s_node_id;

    /* Number of antennas */
    buf[5] = n_antennas;

    /* Number of subcarriers (LE u16) */
    memcpy(&buf[6], &n_subcarriers, 2);

    /* Frequency MHz (LE u32) */
    memcpy(&buf[8], &freq_mhz, 4);

    /* Sequence number (LE u32) */
    uint32_t seq = s_sequence++;
    memcpy(&buf[12], &seq, 4);

    /* RSSI (i8) */
    buf[16] = (uint8_t)(int8_t)info->rx_ctrl.rssi;

    /* Noise floor (i8) */
    buf[17] = (uint8_t)(int8_t)info->rx_ctrl.noise_floor;

    /* ADR-110: PPDU type (byte 18) + bandwidth/flags (byte 19).
     * Previously reserved-zero, now optionally populated when CONFIG_CSI_FRAME_HE_TAGGING.
     * Readers that don't know about the extension see zeros — backward compatible.
     *
     * The struct that backs info->rx_ctrl is target-conditional in IDF v5.4
     * (esp_wifi/include/local/esp_wifi_types_native.h):
     *
     *   CONFIG_SOC_WIFI_HE_SUPPORT=y  (C6/C5)  →  esp_wifi_rxctrl_t with cur_bb_format, second
     *   otherwise                     (S3 etc) →  legacy struct with sig_mode, cwb, stbc
     *
     * Byte-18 PPDU type encoding stays the same across targets:
     *   0=HT/legacy bucket, 1=HE-SU, 2=HE-MU, 3=HE-TB, 0xFF=unknown
     */
#ifdef CONFIG_CSI_FRAME_HE_TAGGING
    uint8_t ppdu_type = 0xFF;
    uint8_t flags     = 0;
#if CONFIG_SOC_WIFI_HE_SUPPORT
    /* HE-capable chips: read cur_bb_format (0=11b, 1=11g, 2=HT, 3=VHT, 4=HE-SU,
     * 5=HE-MU, 6=HE-ERSU, 7=HE-TB) and 'second' (40 MHz secondary chan offset). */
    switch (info->rx_ctrl.cur_bb_format) {
        case 0:
        case 1:
        case 2:  ppdu_type = 0; break;  /* 11b/g/a/HT bucket */
        case 3:  ppdu_type = 0; break;  /* VHT — rare on 2.4 GHz, HT bucket */
        case 4:  ppdu_type = 1; break;  /* HE-SU */
        case 5:  ppdu_type = 2; break;  /* HE-MU */
        case 6:  ppdu_type = 1; break;  /* HE-ER-SU collapses to HE-SU */
        case 7:  ppdu_type = 3; break;  /* HE-TB */
        default: ppdu_type = 0xFF; break;
    }
    if (info->rx_ctrl.second != 0) flags |= 0x1;  /* bw 40 MHz */
#else
    /* Pre-HE chips (S3 etc): use legacy sig_mode + cwb + stbc fields. */
    switch (info->rx_ctrl.sig_mode) {
        case 0: ppdu_type = 0; break;  /* non-HT (11b/g) */
        case 1: ppdu_type = 0; break;  /* HT (11n) */
        case 3: ppdu_type = 0; break;  /* VHT — bucket as HT for storage */
        default: ppdu_type = 0xFF; break;
    }
    if (info->rx_ctrl.cwb) flags |= 0x1;            /* bw 40 MHz */
    if (info->rx_ctrl.stbc) flags |= (1 << 2);      /* STBC */
#endif  /* CONFIG_SOC_WIFI_HE_SUPPORT */
    /* ADR-018 byte 19 bit 4 = "cross-node sync valid". Two transports can
     * set it: the original 802.15.4 c6_timesync (broken in IDF v5.4 — D1)
     * and the ESP-NOW workaround c6_sync_espnow (measured working in §A0.7-
     * §A0.10). OR them together so frames signal sync from whichever
     * transport is alive on this node. Host can pair against the sync
     * packet (§A0.12) once it sees this bit. */
#if defined(CONFIG_IDF_TARGET_ESP32C6) && defined(CONFIG_C6_TIMESYNC_ENABLE)
    if (c6_timesync_is_valid()) flags |= (1 << 4);  /* 15.4 sync valid */
#endif
    if (c6_sync_espnow_is_valid()) flags |= (1 << 4);  /* ESP-NOW sync valid (D1 workaround) */
    buf[18] = ppdu_type;
    buf[19] = flags;
#else
    buf[18] = 0;
    buf[19] = 0;
#endif

    /* I/Q data. Espressif marks the first four bytes invalid on some frames.
     * Preserve the fixed grid and sanitize those bytes instead of deleting
     * them. Selectively retained from upstream commit dd02efe2. */
    memcpy(&buf[CSI_HEADER_SIZE], info->buf, iq_len);
    if (info->first_word_invalid && iq_len > 0) {
        size_t invalid_len = iq_len < 4 ? iq_len : 4;
        memset(&buf[CSI_HEADER_SIZE], 0, invalid_len);
        buf[19] |= (1u << 5);
    }

    /* Runtime TX source binding. Version 1 hashes exactly the six binary
     * filter-MAC bytes captured before WiFi initialization. The callback
     * independently drops non-matching sources before serialization; the
     * SOURCE_MATCHED bit records the same comparison in every datagram so a
     * setup-bound server can fail closed without receiving a raw MAC address. */
    uint8_t *binding = &buf[iq_end];
    uint32_t binding_magic = CSI_SOURCE_BINDING_MAGIC;
    uint16_t binding_size = CSI_SOURCE_BINDING_SIZE;
    memcpy(&binding[0], &binding_magic, sizeof(binding_magic));
    binding[4] = CSI_SOURCE_BINDING_VERSION;

    uint8_t binding_flags = 0;
    if (s_filter_mac_set) {
        binding_flags |= CSI_SOURCE_BINDING_FLAG_FILTER_ENFORCED;
        if (memcmp(info->mac, s_filter_mac, sizeof(s_filter_mac)) == 0) {
            binding_flags |= CSI_SOURCE_BINDING_FLAG_SOURCE_MATCHED;
        }
        if (s_filter_identity_valid) {
            binding_flags |= CSI_SOURCE_BINDING_FLAG_IDENTITY_VALID;
        }
    }
    binding[5] = binding_flags;
    memcpy(&binding[6], &binding_size, sizeof(binding_size));
    if (s_filter_identity_valid) {
        memcpy(&binding[8], s_filter_identity_sha256,
               sizeof(s_filter_identity_sha256));
    } else {
        memset(&binding[8], 0, sizeof(s_filter_identity_sha256));
    }

    uint8_t *identity = &buf[binding_end];
    uint32_t identity_magic = CSI_IDENTITY_MAGIC;
    uint16_t identity_size = CSI_IDENTITY_TRAILER_SIZE;
    uint32_t driver_rx_timestamp_us = info->rx_ctrl.timestamp;
    uint32_t sync_age_ms = c6_sync_espnow_get_age_ms();
    uint32_t csi_config_flags = csi_collector_get_csi_config_flags();
    uint8_t quality_flags = 0;
    if (info->first_word_invalid) quality_flags |= CSI_QUALITY_FIRST_WORD_INVALID;
    if ((int8_t)info->rx_ctrl.noise_floor < 0) quality_flags |= CSI_QUALITY_NOISE_FLOOR_VALID;
    if (info->rx_ctrl.rx_state == 0) quality_flags |= CSI_QUALITY_RX_STATE_VALID;
#if CONFIG_CSI_RAW_PROFILE
    quality_flags |= CSI_QUALITY_RAW_PROFILE;
#endif
    if (c6_sync_espnow_is_valid()) quality_flags |= CSI_QUALITY_SYNC_VALID;
#if CONFIG_SOC_WIFI_HE_SUPPORT
    uint8_t rx_antenna = 0xFF;
#else
    uint8_t rx_antenna = info->rx_ctrl.ant;
#endif
    memcpy(&identity[0], &identity_magic, sizeof(identity_magic));
    identity[4] = CSI_PROTOCOL_VERSION;
    identity[5] = CSI_IDENTITY_ROLE_RX;
    memcpy(&identity[6], &identity_size, sizeof(identity_size));
    memcpy(&identity[8], &s_boot_id, sizeof(s_boot_id));
    memcpy(&identity[16], &callback_started_at_us, sizeof(callback_started_at_us));
    memcpy(&identity[24], &driver_rx_timestamp_us, sizeof(driver_rx_timestamp_us));
    identity[28] = quality_flags;
    identity[29] = info->rx_ctrl.rx_state;
    identity[30] = rx_antenna;
    identity[31] = (uint8_t)INT8_MIN; /* ESP-IDF 5.4 exposes no public RX gain API. */
    memcpy(&identity[32], &csi_config_flags, sizeof(csi_config_flags));
    memcpy(&identity[36], &sync_age_ms, sizeof(sync_age_ms));
    memset(&identity[40], 0, 8);

    return frame_size;
}

size_t csi_serialize_frame(const wifi_csi_info_t *info, uint8_t *buf, size_t buf_len)
{
    return csi_serialize_frame_at(
        info,
        (uint64_t)esp_timer_get_time(),
        buf,
        buf_len
    );
}

/**
 * WiFi CSI callback — invoked by ESP-IDF when CSI data is available.
 */
static void wifi_csi_callback(void *ctx, wifi_csi_info_t *info)
{
    (void)ctx;

    int64_t callback_started_us = esp_timer_get_time();
    csi_callback_outcome_t outcome = CSI_CALLBACK_INVALID;
    bool handoff_attempted = false;
    bool handoff_accepted = false;

    if (s_callback_task == NULL) {
        TaskHandle_t current_task = xTaskGetCurrentTaskHandle();
        portENTER_CRITICAL(&s_diagnostics_lock);
        if (s_callback_task == NULL) {
            s_callback_task = current_task;
        }
        portEXIT_CRITICAL(&s_diagnostics_lock);
    }

    if (info == NULL || info->buf == NULL || info->len <= 0
        || (size_t)info->len > CSI_MAX_IQ_SIZE
        || info->rx_ctrl.rx_state != 0) {
        goto callback_done;
    }

    /* ADR-060: MAC address filtering — drop frames from non-matching sources.
     * Uses defensively-copied s_filter_mac instead of g_nvs_config (which can
     * be corrupted by wifi_init_sta — same root cause as the node_id clobber). */
    if (s_filter_mac_set) {
        if (memcmp(info->mac, s_filter_mac, 6) != 0) {
            outcome = CSI_CALLBACK_FOREIGN_TX;
            goto callback_done;
        }
    }

    /* Rate-limit only fully valid frames from the configured TX. This single
     * admission decision bounds serialization, edge processing and transport
     * work together; rejected inputs and foreign sources consume no slot. */
    if (!csi_rate_limit_admit(callback_started_us)) {
        outcome = CSI_CALLBACK_RATE_LIMITED;
        goto callback_done;
    }

    outcome = CSI_CALLBACK_ADMITTED;
    s_cb_count++;
    s_latest_rssi = (int8_t)info->rx_ctrl.rssi;
    handoff_attempted = true;
    handoff_accepted = csi_transport_enqueue_capture(info, callback_started_us);

callback_done:
    csi_record_callback_diagnostics(
        outcome,
        handoff_attempted,
        handoff_accepted,
        callback_started_us
    );
}

/**
 * Promiscuous mode callback — required for CSI to fire on all received frames.
 * We don't need the packet content, just the CSI triggered by reception.
 */
static void wifi_promiscuous_cb(void *buf, wifi_promiscuous_pkt_type_t type)
{
    /* No-op: CSI callback is registered separately and fires in parallel. */
    (void)buf;
    (void)type;
}

/* ---- RuView#521/#954: connected-STA CSI traffic source (additive) ----
 *
 * The ESP32 CSI engine only produces CSI for received OFDM frames (L-LTF/HT-LTF).
 * On a quiet network — or on a display-enabled build where the #893 MGMT->MGMT+DATA
 * promiscuous upgrade is skipped (has_display=true) — the only CSI-eligible frames
 * are sparse beacons (often non-OFDM DSSS), so wifi_csi_callback can starve to
 * yield=0pps -> DEGRADED -> motion/presence=0 (#521, #954).
 *
 * This guarantees a ~50 Hz OFDM unicast floor by pinging the STA's own gateway:
 * the router's ICMP echo replies are OFDM frames destined to this station, which
 * drive the CSI engine regardless of promiscuous filter state or ambient traffic.
 * It is ADDITIVE — promiscuous capture (#396/#893) is left fully intact so
 * multistatic/multi-node sensing still hears other stations' frames. Mirrors
 * Espressif's esp-csi csi_recv_router reference.
 */
static esp_ping_handle_t s_self_ping = NULL;
static csi_ping_stats_t s_ping_stats;
static int64_t s_last_ping_reply_us;

#ifndef CONFIG_CSI_SELF_PING_INTERVAL_MS
#define CONFIG_CSI_SELF_PING_INTERVAL_MS 20
#endif
#ifndef CONFIG_CSI_SELF_PING_TIMEOUT_MS
#define CONFIG_CSI_SELF_PING_TIMEOUT_MS 1000
#endif

#if defined(CONFIG_CSI_SELF_PING_ENABLE) && CONFIG_CSI_SELF_PING_ENABLE
static void csi_ping_success(esp_ping_handle_t hdl, void *args)
{
    (void)hdl;
    (void)args;
    int64_t now_us = esp_timer_get_time();
    uint32_t interval_us = 0;
    if (s_last_ping_reply_us > 0 && now_us > s_last_ping_reply_us) {
        int64_t elapsed = now_us - s_last_ping_reply_us;
        interval_us = elapsed > UINT32_MAX ? UINT32_MAX : (uint32_t)elapsed;
    }
    portENTER_CRITICAL(&s_diagnostics_lock);
    s_ping_stats.replies++;
    if (interval_us > 0) {
        s_ping_stats.last_reply_interval_us = interval_us;
        s_ping_stats.reply_interval_total_us += interval_us;
        s_ping_stats.reply_interval_samples++;
        if (interval_us > s_ping_stats.reply_interval_max_us) {
            s_ping_stats.reply_interval_max_us = interval_us;
        }
    }
    portEXIT_CRITICAL(&s_diagnostics_lock);
    s_last_ping_reply_us = now_us;
}

static void csi_ping_timeout(esp_ping_handle_t hdl, void *args)
{
    (void)hdl;
    (void)args;
    portENTER_CRITICAL(&s_diagnostics_lock);
    s_ping_stats.timeouts++;
    portEXIT_CRITICAL(&s_diagnostics_lock);
}

static void csi_ping_end(esp_ping_handle_t hdl, void *args)
{
    (void)hdl;
    (void)args;
    portENTER_CRITICAL(&s_diagnostics_lock);
    s_ping_stats.running = false;
    portEXIT_CRITICAL(&s_diagnostics_lock);
}
#endif

static void csi_stop_self_ping(void)
{
    esp_ping_handle_t ping = s_self_ping;
    s_self_ping = NULL;
    if (ping != NULL) {
        (void)esp_ping_stop(ping);
        (void)esp_ping_delete_session(ping);
    }
    portENTER_CRITICAL(&s_diagnostics_lock);
    s_ping_stats.running = false;
    portEXIT_CRITICAL(&s_diagnostics_lock);
    s_last_ping_reply_us = 0;
}

static void csi_start_self_ping(void)
{
#if !CONFIG_CSI_SELF_PING_ENABLE
    portENTER_CRITICAL(&s_diagnostics_lock);
    s_ping_stats.enabled = false;
    s_ping_stats.running = false;
    portEXIT_CRITICAL(&s_diagnostics_lock);
    return;
#else
    if (s_self_ping != NULL) {
        return;  /* already running */
    }

    portENTER_CRITICAL(&s_diagnostics_lock);
    s_ping_stats.enabled = true;
    s_ping_stats.configured_interval_ms = CONFIG_CSI_SELF_PING_INTERVAL_MS;
    s_ping_stats.configured_timeout_ms = CONFIG_CSI_SELF_PING_TIMEOUT_MS;
    s_ping_stats.start_attempts++;
    portEXIT_CRITICAL(&s_diagnostics_lock);

    esp_netif_t *sta = esp_netif_get_handle_from_ifkey("WIFI_STA_DEF");
    esp_netif_ip_info_t ip;
    if (sta == NULL || esp_netif_get_ip_info(sta, &ip) != ESP_OK || ip.gw.addr == 0) {
        portENTER_CRITICAL(&s_diagnostics_lock);
        s_ping_stats.gateway_unavailable++;
        portEXIT_CRITICAL(&s_diagnostics_lock);
        ESP_LOGW(TAG, "self-ping: no gateway IP yet; CSI relies on ambient frames (#954)");
        return;
    }

    char gw_str[16];
    esp_ip4addr_ntoa(&ip.gw, gw_str, sizeof(gw_str));

    ip_addr_t target;
    memset(&target, 0, sizeof(target));
    ipaddr_aton(gw_str, &target);

    esp_ping_config_t cfg = ESP_PING_DEFAULT_CONFIG();
    cfg.target_addr     = target;
    cfg.count           = ESP_PING_COUNT_INFINITE;
    cfg.interval_ms     = CONFIG_CSI_SELF_PING_INTERVAL_MS;
    cfg.timeout_ms      = CONFIG_CSI_SELF_PING_TIMEOUT_MS;
    cfg.data_size       = 1;
    cfg.task_stack_size = 4096;

    esp_ping_callbacks_t cbs = {
        .cb_args         = NULL,
        .on_ping_success = csi_ping_success,
        .on_ping_timeout = csi_ping_timeout,
        .on_ping_end     = csi_ping_end,
    };

    if (esp_ping_new_session(&cfg, &cbs, &s_self_ping) == ESP_OK && s_self_ping != NULL) {
        if (esp_ping_start(s_self_ping) == ESP_OK) {
            portENTER_CRITICAL(&s_diagnostics_lock);
            s_ping_stats.running = true;
            portEXIT_CRITICAL(&s_diagnostics_lock);
            ESP_LOGI(TAG, "self-ping started -> %s interval=%d ms timeout=%d ms",
                     gw_str, CONFIG_CSI_SELF_PING_INTERVAL_MS,
                     CONFIG_CSI_SELF_PING_TIMEOUT_MS);
        } else {
            csi_stop_self_ping();
            portENTER_CRITICAL(&s_diagnostics_lock);
            s_ping_stats.start_failures++;
            portEXIT_CRITICAL(&s_diagnostics_lock);
        }
    } else {
        ESP_LOGW(TAG, "self-ping: esp_ping_new_session failed");
        s_self_ping = NULL;
        portENTER_CRITICAL(&s_diagnostics_lock);
        s_ping_stats.start_failures++;
        portEXIT_CRITICAL(&s_diagnostics_lock);
    }
#endif
}

void csi_collector_set_node_id(uint8_t node_id)
{
    s_node_id = node_id;
    s_node_id_early_set = true;
    ESP_LOGI(TAG, "Early capture node_id=%u (before WiFi init, #232/#390)",
             (unsigned)node_id);

    /* Also capture MAC filter config now — same struct, same corruption risk.
     * The CSI callback reads filter_mac_set on every invocation (100-500 Hz),
     * so a corrupted value could cause erratic filtering or crash. */
    s_filter_mac_set = (g_nvs_config.filter_mac_set != 0);
    memset(s_filter_mac, 0, sizeof(s_filter_mac));
    memset(s_filter_identity_sha256, 0, sizeof(s_filter_identity_sha256));
    s_filter_identity_valid = false;
    if (s_filter_mac_set) {
        memcpy(s_filter_mac, g_nvs_config.filter_mac, sizeof(s_filter_mac));
        int hash_result = mbedtls_sha256(
            s_filter_mac,
            sizeof(s_filter_mac),
            s_filter_identity_sha256,
            0
        );
        if (hash_result == 0) {
            s_filter_identity_valid = true;
            ESP_LOGI(TAG, "Early capture TX filter identity ready");
        } else {
            memset(s_filter_identity_sha256, 0,
                   sizeof(s_filter_identity_sha256));
            ESP_LOGE(TAG,
                     "TX filter identity SHA-256 failed (%d); "
                     "setup-bound capture will reject this node",
                     hash_result);
        }
    }
}

void csi_collector_set_boot_id(uint64_t boot_id)
{
    s_boot_id = boot_id == 0 ? 1 : boot_id;
}

void csi_collector_init(void)
{
    if (!s_node_id_early_set) {
        /* Fallback: no early capture — use current g_nvs_config (may be clobbered). */
        s_node_id = g_nvs_config.node_id;
        ESP_LOGW(TAG, "Late capture node_id=%u (no early set_node_id call)",
                 (unsigned)s_node_id);
    } else if (g_nvs_config.node_id != s_node_id) {
        /* Canary: early capture disagrees with current g_nvs_config — corruption
         * happened between nvs_config_load() and here (likely wifi_init_sta). */
        ESP_LOGW(TAG, "node_id clobber CONFIRMED: early=%u g_nvs_config=%u "
                 "(WiFi init likely corrupted struct, using early value)",
                 (unsigned)s_node_id, (unsigned)g_nvs_config.node_id);
    } else {
        ESP_LOGI(TAG, "node_id=%u verified (early capture matches g_nvs_config)",
                 (unsigned)s_node_id);
    }

    /* Canary for filter_mac: check if WiFi init corrupted the filter fields. */
    if (s_node_id_early_set) {
        bool mac_set_now = (g_nvs_config.filter_mac_set != 0);
        if (mac_set_now != s_filter_mac_set) {
            ESP_LOGW(TAG, "filter_mac_set clobber CONFIRMED: early=%d g_nvs_config=%d",
                     (int)s_filter_mac_set, (int)mac_set_now);
        } else if (s_filter_mac_set &&
                   memcmp(s_filter_mac, g_nvs_config.filter_mac, 6) != 0) {
            ESP_LOGW(TAG, "filter_mac clobber CONFIRMED: bytes differ after WiFi init");
        }
    } else {
        /* No early capture — grab filter config now (may already be corrupted). */
        s_filter_mac_set = (g_nvs_config.filter_mac_set != 0);
        if (s_filter_mac_set) {
            memcpy(s_filter_mac, g_nvs_config.filter_mac, 6);
        }
    }

    /* ADR-060: Determine the CSI channel.
     * Priority: 1) NVS override (--channel), 2) connected AP channel, 3) Kconfig default. */
    uint8_t csi_channel = (uint8_t)CONFIG_CSI_WIFI_CHANNEL;

    if (g_nvs_config.csi_channel > 0) {
        /* Explicit NVS override via provision.py --channel */
        csi_channel = g_nvs_config.csi_channel;
        ESP_LOGI(TAG, "Using NVS channel override: %u", (unsigned)csi_channel);
    } else {
        /* Auto-detect from connected AP */
        wifi_ap_record_t ap_info;
        if (esp_wifi_sta_get_ap_info(&ap_info) == ESP_OK && ap_info.primary > 0) {
            csi_channel = ap_info.primary;
            ESP_LOGI(TAG, "Auto-detected AP channel: %u", (unsigned)csi_channel);
        } else {
            ESP_LOGW(TAG, "Could not detect AP channel, using Kconfig default: %u",
                     (unsigned)csi_channel);
        }
    }

    /* Update the hop table's first channel to match. */
    s_hop_channels[0] = csi_channel;

    /* Disable WiFi modem sleep — reliable CSI capture needs the radio awake.
     * The ESP-IDF STA default is WIFI_PS_MIN_MODEM, which lets the modem
     * sleep between DTIM beacons; with the MGMT-only promiscuous filter
     * (RuView#396) that starves the CSI callback and the per-second yield
     * collapses toward 0 pps (RuView#521). Operators who want battery
     * duty-cycling opt back in via power_mgmt_init() (provision.py
     * --duty-cycle <N>), which runs after this and re-enables modem sleep. */
    esp_err_t ps_err = esp_wifi_set_ps(WIFI_PS_NONE);
    if (ps_err != ESP_OK) {
        ESP_LOGW(TAG, "esp_wifi_set_ps(WIFI_PS_NONE) failed: %s — CSI yield may be low",
                 esp_err_to_name(ps_err));
    } else {
        ESP_LOGI(TAG, "WiFi modem sleep disabled (WIFI_PS_NONE) for CSI capture");
    }

    /* Enable promiscuous mode — required for reliable CSI callbacks.
     * Without this, CSI only fires on frames destined to this station,
     * which may be very infrequent on a quiet network. */
    ESP_ERROR_CHECK(esp_wifi_set_promiscuous(true));
    ESP_ERROR_CHECK(esp_wifi_set_promiscuous_rx_cb(wifi_promiscuous_cb));

    /* MGMT-only promiscuous filter + active probe injection (RuView#396).
     *
     * DATA frames cause 100-500+ WiFi HW interrupts/sec which crashes Core 0
     * in wDev_ProcessFiq (SPI flash cache race in ESP-IDF WiFi blob).
     * MGMT-only gives ~10 Hz (beacons). Probe request injection at 10 Hz
     * adds ~10 Hz probe responses from APs → ~20 Hz total, matching the
     * edge processing designed sample rate of 20 Hz. */
    wifi_promiscuous_filter_t filt = {
        .filter_mask = WIFI_PROMIS_FILTER_MASK_MGMT,
    };
    ESP_ERROR_CHECK(esp_wifi_set_promiscuous_filter(&filt));

    ESP_LOGI(TAG, "Promiscuous mode enabled (MGMT-only, RuView#396)");

#if CONFIG_SOC_WIFI_HE_SUPPORT
    /* Wi-Fi 6 targets (e.g. ESP32-C6): wifi_csi_config_t is wifi_csi_acquire_config_t
     * (bitfields), not the legacy 802.11n bool layout used on ESP32-S3. */
    wifi_csi_config_t csi_config;
    memset(&csi_config, 0, sizeof(csi_config));
    csi_config.enable = 1U;
    csi_config.acquire_csi_legacy = 1U;
    csi_config.acquire_csi_ht20 = 1U;
    csi_config.acquire_csi_ht40 = 1U;
    csi_config.acquire_csi_su = 1U;
    csi_config.acquire_csi_mu = 1U;
    csi_config.acquire_csi_dcm = 1U;
    csi_config.acquire_csi_beamformed = 1U;
#if CONFIG_SOC_WIFI_MAC_VERSION_NUM >= 3
    csi_config.acquire_csi_force_lltf = 1U;
    csi_config.acquire_csi_vht = 1U;
    csi_config.acquire_csi_he_stbc_mode = ESP_CSI_ACQUIRE_STBC_SAMPLE_HELTFS;
    csi_config.val_scale_cfg = 0U;
#else
    csi_config.acquire_csi_he_stbc = ESP_CSI_ACQUIRE_STBC_SAMPLE_HELTFS;
    csi_config.val_scale_cfg = 0U;
#endif
    csi_config.dump_ack_en = 0U;
#else
    wifi_csi_config_t csi_config = {
        .lltf_en = true,
        .htltf_en = true,
        .stbc_htltf2_en = true,
        .ltf_merge_en = true,
        .channel_filter_en = false,
        .manu_scale = false,
        .shift = false,
    };
#endif

    ESP_ERROR_CHECK(esp_wifi_set_csi_config(&csi_config));
    ESP_ERROR_CHECK(esp_wifi_set_csi_rx_cb(wifi_csi_callback, NULL));
    ESP_ERROR_CHECK(esp_wifi_set_csi(true));

    if (s_filter_mac_set) {
        ESP_LOGI(TAG, "MAC filter active; raw filter value is not logged");
    }

    ESP_LOGI(TAG, "CSI collection initialized (node_id=%u, channel=%u)",
             (unsigned)s_node_id, (unsigned)csi_channel);

    /* RuView#521/#954: start the connected-STA traffic source so the CSI engine
     * receives a guaranteed OFDM unicast floor even when promiscuous capture is
     * starved (display builds / quiet networks). Additive to #396/#893. */
    csi_start_self_ping();
}

/* Accessor for other modules that need the authoritative runtime node_id. */
uint8_t csi_collector_get_node_id(void)
{
    return s_node_id;
}

uint64_t csi_collector_get_boot_id(void)
{
    return s_boot_id;
}

uint32_t csi_collector_get_sequence(void)
{
    return s_sequence;
}

int8_t csi_collector_get_rssi(void)
{
    return s_latest_rssi;
}

void csi_collector_get_stats(csi_collector_stats_t *stats)
{
    if (stats == NULL) {
        return;
    }
    portENTER_CRITICAL(&s_diagnostics_lock);
    *stats = s_diagnostics;
    TaskHandle_t callback_task = s_callback_task;
    portEXIT_CRITICAL(&s_diagnostics_lock);
    if (callback_task != NULL) {
        stats->callback_stack_free_min_bytes =
            (uint32_t)uxTaskGetStackHighWaterMark(callback_task)
                * (uint32_t)sizeof(StackType_t);
        stats->callback_stack_runtime_measured = true;
    }
}

void csi_collector_get_ping_stats(csi_ping_stats_t *stats)
{
    if (stats == NULL) {
        return;
    }
    portENTER_CRITICAL(&s_diagnostics_lock);
    *stats = s_ping_stats;
    portEXIT_CRITICAL(&s_diagnostics_lock);
}

void csi_collector_notify_network_change(bool connected)
{
    bool was_running;
    portENTER_CRITICAL(&s_diagnostics_lock);
    was_running = s_ping_stats.running;
    portEXIT_CRITICAL(&s_diagnostics_lock);

    if (!connected) {
        csi_stop_self_ping();
        return;
    }
    if (!was_running) {
        portENTER_CRITICAL(&s_diagnostics_lock);
        s_ping_stats.restarts++;
        portEXIT_CRITICAL(&s_diagnostics_lock);
        csi_start_self_ping();
    }
}

#ifdef CSI_DIAGNOSTICS_TEST
void csi_collector_test_reset(void)
{
    portENTER_CRITICAL(&s_diagnostics_lock);
    memset(&s_diagnostics, 0, sizeof(s_diagnostics));
    portEXIT_CRITICAL(&s_diagnostics_lock);
    s_sequence = 0;
    s_cb_count = 0;
    s_callback_task = NULL;
    s_next_frame_deadline_us = 0;
    s_latest_rssi = INT8_MIN;
    memset(&s_ping_stats, 0, sizeof(s_ping_stats));
    s_last_ping_reply_us = 0;
}

void csi_collector_test_invoke_callback(wifi_csi_info_t *info)
{
    wifi_csi_callback(NULL, info);
}
#endif

/* ---- ADR-081: packet yield accessor for the radio abstraction layer ---- */

uint16_t csi_collector_get_pkt_yield_per_sec(void)
{
    /* Simple sliding window: record the callback count at ~1 s ago, return
     * the delta. Called from adaptive_controller's fast loop (200 ms), so
     * we update the snapshot every ~5 calls. */
    static int64_t  s_yield_window_start_us = 0;
    static uint32_t s_yield_window_start_cb = 0;
    static uint16_t s_last_yield            = 0;

    int64_t now = esp_timer_get_time();
    if (s_yield_window_start_us == 0) {
        s_yield_window_start_us = now;
        s_yield_window_start_cb = s_cb_count;
        return 0;
    }
    int64_t elapsed = now - s_yield_window_start_us;
    if (elapsed < 1000000LL) {
        return s_last_yield;
    }
    uint32_t delta = s_cb_count - s_yield_window_start_cb;
    /* Scale back to per-second if the window ran long (shouldn't, but be safe). */
    uint64_t per_sec = ((uint64_t)delta * 1000000ULL) / (uint64_t)elapsed;
    if (per_sec > 0xFFFFu) per_sec = 0xFFFFu;
    s_last_yield            = (uint16_t)per_sec;
    s_yield_window_start_us = now;
    s_yield_window_start_cb = s_cb_count;
    return s_last_yield;
}

uint16_t csi_collector_get_send_fail_count(void)
{
    csi_transport_stats_t stats;
    csi_transport_get_stats(&stats);
    uint32_t f = stats.pool_exhausted_drops + stats.queue_publish_failures
        + stats.transport_errors + stats.frames_abandoned;
    return (f > 0xFFFFu) ? 0xFFFFu : (uint16_t)f;
}

/* ---- ADR-029: Channel hopping ---- */

void csi_collector_set_hop_table(const uint8_t *channels, uint8_t hop_count, uint32_t dwell_ms)
{
    if (channels == NULL) {
        ESP_LOGW(TAG, "csi_collector_set_hop_table: channels is NULL");
        return;
    }
    if (hop_count == 0 || hop_count > CSI_HOP_CHANNELS_MAX) {
        ESP_LOGW(TAG, "csi_collector_set_hop_table: invalid hop_count=%u (max=%u)",
                 (unsigned)hop_count, (unsigned)CSI_HOP_CHANNELS_MAX);
        return;
    }
    if (dwell_ms < 10) {
        ESP_LOGW(TAG, "csi_collector_set_hop_table: dwell_ms=%lu too small, clamping to 10",
                 (unsigned long)dwell_ms);
        dwell_ms = 10;
    }

    memcpy(s_hop_channels, channels, hop_count);
    s_hop_count = hop_count;
    s_dwell_ms  = dwell_ms;
    s_hop_index = 0;

    ESP_LOGI(TAG, "Hop table set: %u channels, dwell=%lu ms", (unsigned)hop_count,
             (unsigned long)dwell_ms);
    for (uint8_t i = 0; i < hop_count; i++) {
        ESP_LOGI(TAG, "  hop[%u] = channel %u", (unsigned)i, (unsigned)channels[i]);
    }
}

void csi_hop_next_channel(void)
{
    if (s_hop_count <= 1) {
        /* Single-channel mode: no-op for backward compatibility. */
        return;
    }

    s_hop_index = (s_hop_index + 1) % s_hop_count;
    uint8_t channel = s_hop_channels[s_hop_index];

    /*
     * esp_wifi_set_channel() changes the primary channel.
     * The second parameter is the secondary channel offset for HT40;
     * we use HT20 (no secondary) for sensing.
     */
    esp_err_t err = esp_wifi_set_channel(channel, WIFI_SECOND_CHAN_NONE);
    if (err != ESP_OK) {
        ESP_LOGW(TAG, "Channel hop to %u failed: %s", (unsigned)channel, esp_err_to_name(err));
    } else if ((s_cb_count % 200) == 0) {
        /* Periodic log to confirm hopping is working (not every hop). */
        ESP_LOGI(TAG, "Hopped to channel %u (index %u/%u)",
                 (unsigned)channel, (unsigned)s_hop_index, (unsigned)s_hop_count);
    }
}

/**
 * Timer callback for channel hopping.
 * Called every s_dwell_ms milliseconds from the esp_timer context.
 */
static void hop_timer_cb(void *arg)
{
    (void)arg;
    csi_hop_next_channel();
}

void csi_collector_enable_data_capture(void)
{
    /* MGMT-only (RuView#396) starves the CSI callback on display-less boards
     * (RuView#521/#893): beacons alone are sparse, yield collapses to 0 pps.
     * Without a display there is no QSPI/SPI-flash cache contention with the
     * DATA-frame interrupt load, so capture DATA frames too. */
    wifi_promiscuous_filter_t filt = {
        .filter_mask = WIFI_PROMIS_FILTER_MASK_MGMT | WIFI_PROMIS_FILTER_MASK_DATA,
    };
    esp_err_t err = esp_wifi_set_promiscuous_filter(&filt);
    if (err == ESP_OK) {
        ESP_LOGI(TAG, "CSI filter upgraded to MGMT+DATA (no display, RuView#893)");
    } else {
        ESP_LOGW(TAG, "Failed to enable DATA-frame CSI capture: %s", esp_err_to_name(err));
    }
}

void csi_collector_start_hop_timer(void)
{
    if (s_hop_count <= 1) {
        ESP_LOGI(TAG, "Single-channel mode: hop timer not started");
        return;
    }

    if (s_hop_timer != NULL) {
        ESP_LOGW(TAG, "Hop timer already running");
        return;
    }

    esp_timer_create_args_t timer_args = {
        .callback = hop_timer_cb,
        .arg      = NULL,
        .name     = "csi_hop",
    };

    esp_err_t err = esp_timer_create(&timer_args, &s_hop_timer);
    if (err != ESP_OK) {
        ESP_LOGE(TAG, "Failed to create hop timer: %s", esp_err_to_name(err));
        return;
    }

    uint64_t period_us = (uint64_t)s_dwell_ms * 1000;
    err = esp_timer_start_periodic(s_hop_timer, period_us);
    if (err != ESP_OK) {
        ESP_LOGE(TAG, "Failed to start hop timer: %s", esp_err_to_name(err));
        esp_timer_delete(s_hop_timer);
        s_hop_timer = NULL;
        return;
    }

    ESP_LOGI(TAG, "Hop timer started: period=%lu ms, channels=%u",
             (unsigned long)s_dwell_ms, (unsigned)s_hop_count);
}

/* ---- ADR-029: NDP frame injection stub ---- */

esp_err_t csi_inject_ndp_frame(void)
{
    /*
     * TODO: Construct a proper 802.11 Null Data Packet frame.
     *
     * A real NDP is preamble-only (~24 us airtime, no payload) and is the
     * sensing-first TX mechanism described in ADR-029. For now we send a
     * minimal null-data frame as a placeholder so the API is wired up.
     *
     * Frame structure (IEEE 802.11 Null Data):
     *   FC (2) | Duration (2) | Addr1 (6) | Addr2 (6) | Addr3 (6) | SeqCtl (2)
     *   = 24 bytes total, no body, no FCS (hardware appends FCS).
     */
    uint8_t ndp_frame[24];
    memset(ndp_frame, 0, sizeof(ndp_frame));

    /* Frame Control: Type=Data (0x02), Subtype=Null (0x04) -> 0x0048 */
    ndp_frame[0] = 0x48;
    ndp_frame[1] = 0x00;

    /* Duration: 0 (let hardware fill) */

    /* Addr1 (destination): broadcast */
    memset(&ndp_frame[4], 0xFF, 6);

    /* Addr2 (source): will be overwritten by hardware with own MAC */

    /* Addr3 (BSSID): broadcast */
    memset(&ndp_frame[16], 0xFF, 6);

    esp_err_t err = esp_wifi_80211_tx(WIFI_IF_STA, ndp_frame, sizeof(ndp_frame), false);
    if (err != ESP_OK) {
        ESP_LOGW(TAG, "NDP inject failed: %s", esp_err_to_name(err));
    }

    return err;
}

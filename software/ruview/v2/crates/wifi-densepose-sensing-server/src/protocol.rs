//! Binary packet contracts and parsers for ESP32, edge-vitals, and WASM ingress.

use super::*;

/// ADR-018 ESP32 CSI binary frame header (20 bytes)
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct Esp32Frame {
    pub(crate) magic: u32,
    pub(crate) node_id: u8,
    pub(crate) n_antennas: u8,
    /// u16 since ADR-110 / issue #1005: ESP32-C6 HE-SU frames carry 256
    /// subcarrier bins (242 active HE20 tones). HT frames stay ≤128.
    pub(crate) n_subcarriers: u16,
    pub(crate) freq_mhz: u16,
    pub(crate) sequence: u32,
    pub(crate) rssi: i8,
    pub(crate) noise_floor: i8,
    /// ADR-110 byte 18: PPDU type the CSI was sampled from. Pre-ADR-110
    /// firmware sends 0 ⇒ `PpduType::HtLegacy`.
    pub(crate) ppdu_type: wifi_densepose_hardware::PpduType,
    pub(crate) amplitudes: Vec<f64>,
    pub(crate) phases: Vec<f64>,
}

// ── ESP32 Edge Vitals Packet (ADR-039, magic 0xC511_0002) ────────────────────

/// Decoded vitals packet from ESP32 edge processing pipeline.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct Esp32VitalsPacket {
    pub(crate) node_id: u8,
    pub(crate) presence: bool,
    pub(crate) fall_detected: bool,
    pub(crate) motion: bool,
    pub(crate) breathing_rate_bpm: f64,
    pub(crate) heartrate_bpm: f64,
    pub(crate) rssi: i8,
    pub(crate) n_persons: u8,
    pub(crate) motion_energy: f32,
    pub(crate) presence_score: f32,
    pub(crate) timestamp_ms: u32,
}

/// Parse a 32-byte edge vitals packet (magic 0xC511_0002).
pub(crate) fn parse_esp32_vitals(buf: &[u8]) -> Option<Esp32VitalsPacket> {
    if buf.len() < 32 {
        return None;
    }
    let magic = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
    if magic != 0xC511_0002 {
        return None;
    }

    let node_id = buf[4];
    let flags = buf[5];
    let breathing_raw = u16::from_le_bytes([buf[6], buf[7]]);
    let heartrate_raw = u32::from_le_bytes([buf[8], buf[9], buf[10], buf[11]]);
    let rssi = buf[12] as i8;
    let n_persons = buf[13];
    let motion_energy = f32::from_le_bytes([buf[16], buf[17], buf[18], buf[19]]);
    let presence_score = f32::from_le_bytes([buf[20], buf[21], buf[22], buf[23]]);
    let timestamp_ms = u32::from_le_bytes([buf[24], buf[25], buf[26], buf[27]]);

    Some(Esp32VitalsPacket {
        node_id,
        presence: (flags & 0x01) != 0,
        fall_detected: (flags & 0x02) != 0,
        motion: (flags & 0x04) != 0,
        breathing_rate_bpm: breathing_raw as f64 / 100.0,
        heartrate_bpm: heartrate_raw as f64 / 10000.0,
        rssi,
        n_persons,
        motion_energy,
        presence_score,
        timestamp_ms,
    })
}

// ── ADR-040: WASM Output Packet (magic 0xC511_0007 — reassigned per #928) ─────

/// Single WASM event (type + value).
#[derive(Debug, Clone, Serialize)]
pub(crate) struct WasmEvent {
    pub(crate) event_type: u8,
    pub(crate) value: f32,
}

/// Decoded WASM output packet from ESP32 Tier 3 runtime.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct WasmOutputPacket {
    pub(crate) node_id: u8,
    pub(crate) module_id: u8,
    pub(crate) events: Vec<WasmEvent>,
}

/// Parse a WASM output packet (magic 0xC511_0007 — reassigned per issue #928;
/// the original 0xC511_0004 was a collision with ADR-063 fused vitals).
pub(crate) fn parse_wasm_output(buf: &[u8]) -> Option<WasmOutputPacket> {
    if buf.len() < 8 {
        return None;
    }
    let magic = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
    if magic != 0xC511_0007 {
        return None;
    }

    let node_id = buf[4];
    let module_id = buf[5];
    let event_count = u16::from_le_bytes([buf[6], buf[7]]) as usize;

    let mut events = Vec::with_capacity(event_count);
    let mut offset = 8;
    for _ in 0..event_count {
        if offset + 5 > buf.len() {
            break;
        }
        let event_type = buf[offset];
        let value = f32::from_le_bytes([
            buf[offset + 1],
            buf[offset + 2],
            buf[offset + 3],
            buf[offset + 4],
        ]);
        events.push(WasmEvent { event_type, value });
        offset += 5;
    }

    Some(WasmOutputPacket {
        node_id,
        module_id,
        events,
    })
}

// ── ADR-063: Edge Fused Vitals Packet (magic 0xC511_0004) ─────────────────────
//
// 48-byte packed struct emitted by the ESP32-C6 + MR60BHA2 mmWave config when
// `mmwave_sensor_get_state().detected` is true. Byte layout from
// `firmware/esp32-csi-node/main/edge_processing.h` line 129 — kept in lockstep
// with the firmware's `_Static_assert(sizeof(edge_fused_vitals_pkt_t) == 48)`.
// Issue #928 surfaced that this magic was being parsed as WASM output and the
// fused vitals were silently lost. Adding the proper parser here.

#[derive(Debug, Clone, Serialize)]
pub(crate) struct EdgeFusedVitalsPacket {
    pub(crate) node_id: u8,
    /// Bit0=presence, Bit1=fall, Bit2=motion, Bit3=mmwave_present.
    pub(crate) flags: u8,
    /// Fused breathing rate in BPM (firmware sends BPM*100; we scale here).
    pub(crate) breathing_rate_bpm: f32,
    /// Fused heartrate in BPM (firmware sends BPM*10000; we scale here).
    pub(crate) heartrate_bpm: f32,
    pub(crate) rssi: i8,
    pub(crate) n_persons: u8,
    /// `mmwave_type_t` enum value from firmware.
    pub(crate) mmwave_type: u8,
    /// 0-100 fusion quality score.
    pub(crate) fusion_confidence: u8,
    pub(crate) motion_energy: f32,
    pub(crate) presence_score: f32,
    pub(crate) timestamp_ms: u32,
    /// Raw mmWave heart rate (BPM).
    pub(crate) mmwave_hr_bpm: f32,
    /// Raw mmWave breathing rate (BPM).
    pub(crate) mmwave_br_bpm: f32,
    /// Distance to nearest target (cm).
    pub(crate) mmwave_distance_cm: f32,
    /// Target count from mmWave.
    pub(crate) mmwave_targets: u8,
    /// mmWave signal quality 0-100.
    pub(crate) mmwave_confidence: u8,
}

/// Parse an ADR-063 edge fused vitals packet (magic 0xC511_0004, 48 bytes).
pub(crate) fn parse_edge_fused_vitals(buf: &[u8]) -> Option<EdgeFusedVitalsPacket> {
    if buf.len() < 48 {
        return None;
    }
    let magic = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
    if magic != 0xC511_0004 {
        return None;
    }

    let node_id = buf[4];
    let flags = buf[5];
    let breathing_raw = u16::from_le_bytes([buf[6], buf[7]]);
    let heartrate_raw = u32::from_le_bytes([buf[8], buf[9], buf[10], buf[11]]);
    let rssi = buf[12] as i8;
    let n_persons = buf[13];
    let mmwave_type = buf[14];
    let fusion_confidence = buf[15];
    let motion_energy = f32::from_le_bytes([buf[16], buf[17], buf[18], buf[19]]);
    let presence_score = f32::from_le_bytes([buf[20], buf[21], buf[22], buf[23]]);
    let timestamp_ms = u32::from_le_bytes([buf[24], buf[25], buf[26], buf[27]]);
    let mmwave_hr_bpm = f32::from_le_bytes([buf[28], buf[29], buf[30], buf[31]]);
    let mmwave_br_bpm = f32::from_le_bytes([buf[32], buf[33], buf[34], buf[35]]);
    let mmwave_distance_cm = f32::from_le_bytes([buf[36], buf[37], buf[38], buf[39]]);
    let mmwave_targets = buf[40];
    let mmwave_confidence = buf[41];
    // buf[42..48] are firmware reserved fields (reserved3 u16 + reserved4 u32).

    Some(EdgeFusedVitalsPacket {
        node_id,
        flags,
        breathing_rate_bpm: breathing_raw as f32 / 100.0,
        heartrate_bpm: heartrate_raw as f32 / 10000.0,
        rssi,
        n_persons,
        mmwave_type,
        fusion_confidence,
        motion_energy,
        presence_score,
        timestamp_ms,
        mmwave_hr_bpm,
        mmwave_br_bpm,
        mmwave_distance_cm,
        mmwave_targets,
        mmwave_confidence,
    })
}

#[cfg(test)]
mod issue_928_magic_collision_tests {
    //! Issue #928 — `0xC511_0004` was being parsed as WASM output, eating the
    //! C6+mmWave fused-vitals packets. After this fix, `0xC511_0004` routes to
    //! `parse_edge_fused_vitals` and WASM output owns the freshly-allocated
    //! `0xC511_0007` slot. Tests guard both halves of the swap.
    use super::*;

    /// Build a 48-byte synthetic fused-vitals packet matching the firmware's
    /// `edge_fused_vitals_pkt_t` layout from `edge_processing.h:129`.
    fn build_fused_vitals_packet() -> Vec<u8> {
        let mut buf = vec![0u8; 48];
        buf[0..4].copy_from_slice(&0xC511_0004u32.to_le_bytes());
        buf[4] = 9; // node_id
        buf[5] = 0b0000_1001; // flags: presence | mmwave_present
        buf[6..8].copy_from_slice(&1600u16.to_le_bytes()); // breathing 16.00 BPM
        buf[8..12].copy_from_slice(&720_000u32.to_le_bytes()); // heartrate 72.0 BPM
        buf[12] = (-55i8) as u8; // rssi
        buf[13] = 1; // n_persons
        buf[14] = 2; // mmwave_type
        buf[15] = 85; // fusion_confidence
        buf[16..20].copy_from_slice(&0.42f32.to_le_bytes()); // motion_energy
        buf[20..24].copy_from_slice(&0.95f32.to_le_bytes()); // presence_score
        buf[24..28].copy_from_slice(&1_234_567u32.to_le_bytes()); // timestamp_ms
        buf[28..32].copy_from_slice(&71.5f32.to_le_bytes()); // mmwave_hr_bpm
        buf[32..36].copy_from_slice(&15.8f32.to_le_bytes()); // mmwave_br_bpm
        buf[36..40].copy_from_slice(&182.0f32.to_le_bytes()); // mmwave_distance_cm
        buf[40] = 1; // mmwave_targets
        buf[41] = 90; // mmwave_confidence
                      // bytes 42..48 — firmware reserved fields, left as zero
        buf
    }

    #[test]
    fn parse_edge_fused_vitals_extracts_fields_correctly() {
        let buf = build_fused_vitals_packet();
        let pkt = parse_edge_fused_vitals(&buf).expect("must parse a well-formed packet");
        assert_eq!(pkt.node_id, 9);
        assert_eq!(pkt.flags, 0b0000_1001);
        assert!(
            (pkt.breathing_rate_bpm - 16.0).abs() < 1e-3,
            "breathing scale 100"
        );
        assert!(
            (pkt.heartrate_bpm - 72.0).abs() < 1e-3,
            "heartrate scale 10000"
        );
        assert_eq!(pkt.rssi, -55);
        assert_eq!(pkt.n_persons, 1);
        assert_eq!(pkt.mmwave_type, 2);
        assert_eq!(pkt.fusion_confidence, 85);
        assert!((pkt.motion_energy - 0.42).abs() < 1e-6);
        assert!((pkt.presence_score - 0.95).abs() < 1e-6);
        assert_eq!(pkt.timestamp_ms, 1_234_567);
        assert!((pkt.mmwave_hr_bpm - 71.5).abs() < 1e-6);
        assert!((pkt.mmwave_br_bpm - 15.8).abs() < 1e-3);
        assert!((pkt.mmwave_distance_cm - 182.0).abs() < 1e-6);
        assert_eq!(pkt.mmwave_targets, 1);
        assert_eq!(pkt.mmwave_confidence, 90);
    }

    #[test]
    fn parse_edge_fused_vitals_rejects_short_buffer() {
        let buf = build_fused_vitals_packet();
        // Truncate to 47 bytes — one short of the 48-byte minimum.
        assert!(parse_edge_fused_vitals(&buf[..47]).is_none());
    }

    #[test]
    fn parse_edge_fused_vitals_rejects_wrong_magic() {
        let mut buf = build_fused_vitals_packet();
        buf[0..4].copy_from_slice(&0xC511_0007u32.to_le_bytes()); // WASM magic, not fused
        assert!(parse_edge_fused_vitals(&buf).is_none());
    }

    #[test]
    fn parse_wasm_output_rejects_legacy_0004_magic() {
        // The old WASM magic collided with fused vitals — must no longer be
        // accepted. A real fused-vitals packet starts with 0xC511_0004 and
        // would have been misparsed before this fix.
        let buf = build_fused_vitals_packet();
        assert!(
            parse_wasm_output(&buf).is_none(),
            "issue #928: WASM parser must NOT accept 0xC511_0004"
        );
    }

    #[test]
    fn parse_wasm_output_accepts_new_0007_magic() {
        // Build a tiny well-formed WASM output packet on the new magic.
        let mut buf = vec![0u8; 8];
        buf[0..4].copy_from_slice(&0xC511_0007u32.to_le_bytes());
        buf[4] = 5; // node_id
        buf[5] = 1; // module_id
        buf[6..8].copy_from_slice(&0u16.to_le_bytes()); // event_count = 0
        let pkt = parse_wasm_output(&buf).expect("0xC511_0007 must parse");
        assert_eq!(pkt.node_id, 5);
        assert_eq!(pkt.module_id, 1);
        assert!(pkt.events.is_empty());
    }
}

// ── ESP32 UDP frame parser ───────────────────────────────────────────────────

pub(crate) fn has_esp32_csi_magic(buf: &[u8]) -> bool {
    buf.get(0..4)
        .and_then(|magic| <[u8; 4]>::try_from(magic).ok())
        .is_some_and(|magic| u32::from_le_bytes(magic) == raw_csi_recording::ESP32_CSI_MAGIC)
}

pub(crate) fn esp32_csi_header_rx_id(buf: &[u8]) -> Option<u8> {
    has_esp32_csi_magic(buf)
        .then(|| buf.get(4).copied())
        .flatten()
}

pub(crate) fn parse_esp32_frame(buf: &[u8]) -> Option<Esp32Frame> {
    if buf.len() < 20 {
        return None;
    }

    let magic = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
    if magic != 0xC511_0001 {
        return None;
    }

    // Frame layout (must match firmware csi_collector.c):
    //   [0..3]   magic (u32 LE)
    //   [4]      node_id (u8)
    //   [5]      n_antennas (u8)
    //   [6..7]   n_subcarriers (u16 LE)
    //   [8..11]  freq_mhz (u32 LE)
    //   [12..15] sequence (u32 LE)
    //   [16]     rssi (i8)
    //   [17]     noise_floor (i8)
    //   [18..19] reserved
    //   [20..]   I/Q data
    // Issue #1005: until 2026-06 this code read n_subcarriers from byte 6
    // alone (an ESP32-C6 HE-SU frame's 256 = 0x0100 LE decoded as 0 — the
    // frame parsed with zero subcarriers) and read sequence/rssi/noise at
    // stale offsets 10/14/15. Offsets below match the comment (and firmware).
    let node_id = buf[4];
    let n_antennas = buf[5];
    let n_subcarriers = u16::from_le_bytes([buf[6], buf[7]]);
    let freq_mhz =
        u16::try_from(u32::from_le_bytes([buf[8], buf[9], buf[10], buf[11]])).unwrap_or(0);
    let sequence = u32::from_le_bytes([buf[12], buf[13], buf[14], buf[15]]);
    let rssi_raw = buf[16] as i8;
    // Fix RSSI sign: ensure it's always negative (dBm convention).
    let rssi = if rssi_raw > 0 {
        rssi_raw.saturating_neg()
    } else {
        rssi_raw
    };
    let noise_floor = buf[17] as i8;
    let ppdu_type = wifi_densepose_hardware::PpduType::from_byte(buf[18]);

    let iq_start = 20;
    let n_pairs = n_antennas as usize * n_subcarriers as usize;
    let expected_len = iq_start + n_pairs * 2;

    if buf.len() < expected_len {
        return None;
    }

    let mut amplitudes = Vec::with_capacity(n_pairs);
    let mut phases = Vec::with_capacity(n_pairs);

    for k in 0..n_pairs {
        let i_val = buf[iq_start + k * 2] as i8 as f64;
        let q_val = buf[iq_start + k * 2 + 1] as i8 as f64;
        amplitudes.push((i_val * i_val + q_val * q_val).sqrt());
        phases.push(q_val.atan2(i_val));
    }

    Some(Esp32Frame {
        magic,
        node_id,
        n_antennas,
        n_subcarriers,
        freq_mhz,
        sequence,
        rssi,
        noise_floor,
        ppdu_type,
        amplitudes,
        phases,
    })
}

#[cfg(test)]
mod issue_1009_n_subcarriers_u16_tests {
    //! Issue #1009 §1c — `parse_esp32_frame` must read `n_subcarriers` as a
    //! u16 LE at bytes 6..7 (ADR-018 wire format), not a single byte at 6.
    //!
    //! An ESP32-C6 HE20 frame carries 256 subcarriers → byte 6 = 0x00,
    //! byte 7 = 0x01. The pre-#1005 single-byte read decoded this as 0
    //! subcarriers, silently dropping every real HE20 frame. This was the same
    //! truncation as the CLI parser (`wifi-densepose-cli` calibrate.rs); this
    //! module pins that the sensing-server template stays u16-correct.
    use super::*;

    /// Build an ADR-018 CSI frame (magic 0xC511_0001, 20-byte header).
    fn build_csi_frame(n_subcarriers: u16) -> Vec<u8> {
        let mut buf = vec![0u8; 20 + n_subcarriers as usize * 2];
        buf[0..4].copy_from_slice(&0xC511_0001u32.to_le_bytes());
        buf[4] = 7; // node_id
        buf[5] = 1; // n_antennas
        buf[6..8].copy_from_slice(&n_subcarriers.to_le_bytes()); // u16 LE
        buf[8..12].copy_from_slice(&5180u32.to_le_bytes()); // freq_mhz (5 GHz HE)
        buf[12..16].copy_from_slice(&42u32.to_le_bytes()); // sequence
        buf[16] = (-40i8) as u8; // rssi
        buf[17] = (-90i8) as u8; // noise_floor
        buf[18] = 0; // ppdu_type
        buf[19] = 0;
        for k in 0..n_subcarriers as usize {
            buf[20 + k * 2] = (5 + (k % 40) as i8) as u8; // i
            buf[20 + k * 2 + 1] = (k % 30) as u8; // q
        }
        buf
    }

    #[test]
    fn parse_esp32_frame_he20_256_bins_not_truncated() {
        // 256 = 0x0100 LE: byte6 = 0x00, byte7 = 0x01. A u8 read of byte 6
        // would see 0 subcarriers; a u16 read sees 256.
        let buf = build_csi_frame(256);
        assert_eq!(buf.len(), 532, "256-bin frame wire size = 20 + 256*2");
        let frame = parse_esp32_frame(&buf).expect("256-bin HE20 frame must parse");
        assert_eq!(
            frame.n_subcarriers, 256,
            "n_subcarriers must read as u16 (256), not the byte-6-only 0"
        );
        assert_eq!(frame.amplitudes.len(), 256);
        assert_eq!(frame.node_id, 7);
        assert_eq!(frame.rssi, -40);
        assert_eq!(frame.sequence, 42);
    }

    #[test]
    fn parse_esp32_frame_ht20_64_bins_still_parses() {
        // Regression guard for the common single-byte (≤255) case.
        let buf = build_csi_frame(64);
        let frame = parse_esp32_frame(&buf).expect("64-bin HT20 frame must parse");
        assert_eq!(frame.n_subcarriers, 64);
        assert_eq!(frame.amplitudes.len(), 64);
    }

    #[test]
    fn malformed_csi_magic_still_identifies_safe_rx_header() {
        let mut truncated = build_csi_frame(64);
        truncated.truncate(20);

        assert!(has_esp32_csi_magic(&truncated));
        assert_eq!(esp32_csi_header_rx_id(&truncated), Some(7));
        assert!(parse_esp32_frame(&truncated).is_none());
    }

    #[test]
    fn foreign_or_too_short_datagrams_are_not_csi_ingress() {
        let foreign = 0xC511_0007u32.to_le_bytes();
        assert!(!has_esp32_csi_magic(&foreign));
        assert_eq!(esp32_csi_header_rx_id(&foreign), None);

        let csi_magic_without_rx = raw_csi_recording::ESP32_CSI_MAGIC.to_le_bytes();
        assert!(has_esp32_csi_magic(&csi_magic_without_rx));
        assert_eq!(esp32_csi_header_rx_id(&csi_magic_without_rx), None);
    }
}

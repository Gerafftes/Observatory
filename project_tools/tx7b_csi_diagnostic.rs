//! Offline V2 diagnostics, separate from the frozen Mixed-V2 acceptance gate.
//! Inputs are private length-prefixed UDP payloads and expected source/boot IDs.
#[path = "../software/ruview/v2/crates/wifi-densepose-sensing-server/src/raw_csi_recording.rs"]
mod raw_csi_recording;
#[path = "../software/ruview/v2/crates/wifi-densepose-sensing-server/src/server_clock.rs"]
mod server_clock;

use raw_csi_recording::{RawCsiFrame, RawCsiFrameContext, RawCsiGridIdentity};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use wifi_densepose_hardware::Esp32CsiParser;

#[derive(Deserialize)]
struct Expected {
    source_sha256: String,
    rx_boots: BTreeMap<u8, u64>,
}

#[derive(Default, Serialize)]
struct Classification {
    structurally_complete: bool,
    v2_protocol_semantically_valid: bool,
    bound_to_expected_tx_and_rx_epoch: bool,
    fixed_grid_eligible: bool,
    lossless_roundtrip: bool,
    server_accepted: Option<bool>,
}

fn classify(packet: &[u8], expected: &Expected) -> Classification {
    let mut result = Classification::default();
    if packet.len() < 20 || packet[..4] != 0xc5110001u32.to_le_bytes() {
        return result;
    }
    let carriers = u16::from_le_bytes([packet[6], packet[7]]);
    result.structurally_complete =
        packet.len() == 20 + 2 * packet[5] as usize * carriers as usize + 64;
    if !result.structurally_complete {
        return result;
    }
    let context = RawCsiFrameContext {
        host_timestamp_unix_ns: 1,
        host_monotonic_ns: Some(1),
        clock_epoch_id: Some("offline-diagnostic-not-a-live-epoch".to_string()),
        session_id: None,
        label: None,
        ground_truth: None,
        mesh_timestamp_us: None,
    };
    let Ok(frame) = RawCsiFrame::from_packet(packet, context) else {
        return result;
    };
    let Ok((_, consumed)) = Esp32CsiParser::parse_frame(packet) else {
        return result;
    };
    let Some(identity) = frame.protocol_identity else {
        return result;
    };
    // V2 firmware's known PPDU and flags fields; a PHY diagnosis is separate.
    result.v2_protocol_semantically_valid = consumed == packet.len()
        && identity.protocol_version == 2
        && (1..=4).contains(&frame.rx_id)
        && frame.ppdu_type <= 2
        && frame.flags & !0x3d == 0;
    let grid = RawCsiGridIdentity::from_frame(&frame);
    result.fixed_grid_eligible = grid.center_frequency_mhz == 2437
        && grid.antenna_count == 1
        && grid.subcarrier_count == 128
        && grid.ppdu_type == 0
        && grid.layout_flags == 0;
    result.bound_to_expected_tx_and_rx_epoch =
        frame.source_binding.as_ref().is_some_and(|source| {
            source.has_required_flags() && source.tx_filter_sha256 == expected.source_sha256
        }) && expected.rx_boots.get(&frame.rx_id) == Some(&identity.boot_id);
    result.lossless_roundtrip = frame.to_packet().is_ok_and(|encoded| encoded == packet);
    result
}

fn negative_checks(packet: &[u8], expected: &Expected) -> usize {
    assert!(classify(packet, expected).v2_protocol_semantically_valid);
    let mut cases = Vec::new();
    cases.push(packet[..packet.len() - 1].to_vec());
    for (offset, value) in [
        (5, 0),
        (19, 0x80),
        (packet.len() - 64, 0),
        (packet.len() - 59, 3),
        (packet.len() - 19, 2),
        (packet.len() - 1, 1),
    ] {
        let mut bad = packet.to_vec();
        bad[offset] = value;
        cases.push(bad);
    }
    let mut zero_boot = packet.to_vec();
    zero_boot[packet.len() - 16..packet.len() - 8].fill(0);
    cases.push(zero_boot);
    for bad in &cases {
        assert!(!classify(bad, expected).v2_protocol_semantically_valid);
    }
    let mut wrong_source = packet.to_vec();
    wrong_source[packet.len() - 56] ^= 1;
    let classified = classify(&wrong_source, expected);
    assert!(classified.v2_protocol_semantically_valid);
    assert!(!classified.bound_to_expected_tx_and_rx_epoch);
    cases.len() + 1
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    assert_eq!(
        args.len(),
        3,
        "private payload fixture and expected-identity JSON required"
    );
    let bytes = std::fs::read(&args[1]).unwrap();
    let expected: Expected = serde_json::from_slice(&std::fs::read(&args[2]).unwrap()).unwrap();
    let mut offset = 0;
    let mut counts: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    let mut negatives = 0;
    while offset < bytes.len() {
        let size = u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize;
        offset += 4;
        let packet = &bytes[offset..offset + size];
        offset += size;
        let classification = classify(packet, &expected);
        let carriers = u16::from_le_bytes([packet[6], packet[7]]);
        let entry = counts
            .entry(format!("RX{}/{}", packet[4], carriers))
            .or_default();
        *entry.entry("packets".to_string()).or_default() += 1;
        for (key, value) in serde_json::to_value(&classification)
            .unwrap()
            .as_object()
            .unwrap()
        {
            if value == &serde_json::Value::Bool(true) {
                *entry.entry(key.clone()).or_default() += 1;
            }
        }
        if negatives == 0 {
            negatives = negative_checks(packet, &expected);
        }
        assert!(classification.server_accepted.is_none());
    }
    println!(
        "{}",
        serde_json::json!({"scope":"OFFLINE_V2_DIAGNOSTIC_NOT_ACCEPTANCE",
        "counts":counts,"negative_checks":negatives,"server_packet_join":"UNPROVEN",
        "individual_PHY_or_IQ_quality":"UNPROVEN","live_access":false})
    );
}

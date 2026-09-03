//! Canonical, sealed setup identity for the fixed-room position experiment.
//!
//! The user-authored specification contains only measurement-relevant,
//! path-free and non-secret facts. Creation adds the exact server build
//! identity and seals the complete definition. Loading always recomputes the
//! seal before any field is trusted.

use std::collections::HashSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::position_artifact::{deterministic_pretty_json, sha256_bytes, sha256_file};
use super::raw_csi_recording::{
    RawCsiFrame, SOURCE_BINDING_REQUIRED_FLAGS, TX_SOURCE_BINDING_SCHEME,
};

const MIN_SETUP_SCHEMA_VERSION: u16 = 1;
const SETUP_SCHEMA_VERSION: u16 = 2;
const SETUP_ARTIFACT_KIND: &str = "ruview.position-setup";
const SETUP_HASH_DOMAIN_V1: &[u8] = b"ruview.position-setup.v1\0";
const SETUP_HASH_DOMAIN_V2: &[u8] = b"ruview.position-setup.v2\0";
const COORDINATE_SYSTEM: &str = "x_length_y_height_z_width_lower_left_floor";
const RECORDING_HOST_KIND: &str = "mac";
/// SHA-256 over exactly the six binary MAC bytes written to the RX NVS
/// `filter_mac` blob, in transmitted/network order. Text separators, case,
/// whitespace, and a trailing NUL are never part of the digest.
const TX_FILTER_SCHEME: &str = TX_SOURCE_BINDING_SCHEME;
const EXPECTED_RX_IDS: [u8; 4] = [1, 2, 3, 4];
const POSITION_LAYOUT_FLAGS_MASK: u8 = !0x10;
const SHA256_HEX_LEN: usize = 64;
const OBSERVATORY_SETUP_DRAFT_KIND: &str = "ruview.position-setup-draft";
const OBSERVATORY_SETUP_DRAFT_MISSING_SECTIONS: [&str; 8] = [
    "transmitter.firmware",
    "receivers[*].firmware",
    "receivers[*].expected_grid",
    "recording_host",
    "radio.tx_filter_identity",
    "mmwave.node_id",
    "mmwave.firmware",
    "mmwave.transform",
];

/// Strict input document consumed by `--position-create-setup`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PositionSetupSpec {
    schema_version: u16,
    coordinate_system: String,
    room_dimensions_mm: [u32; 3],
    transmitter: TransmitterDefinition,
    receivers: Vec<ReceiverDefinition>,
    recording_host: RecordingHostDefinition,
    radio: RadioDefinition,
    environment: EnvironmentDefinition,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    mmwave: Option<MmwaveDefinition>,
}

/// Complete setup definition covered by `setup_sha256`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PositionSetupDefinition {
    schema_version: u16,
    coordinate_system: String,
    room_dimensions_mm: [u32; 3],
    transmitter: TransmitterDefinition,
    receivers: Vec<ReceiverDefinition>,
    recording_host: RecordingHostDefinition,
    radio: RadioDefinition,
    environment: EnvironmentDefinition,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    mmwave: Option<MmwaveDefinition>,
    server: ServerBuildIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TransmitterDefinition {
    position_mm: [u32; 3],
    firmware: FirmwareIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReceiverDefinition {
    rx_id: u8,
    position_mm: [u32; 3],
    firmware: FirmwareIdentity,
    expected_grid: ExpectedGridIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RecordingHostDefinition {
    kind: String,
    position_mm: [u32; 3],
    placement_revision: String,
    cable_revision: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RadioDefinition {
    channel: u8,
    tx_filter_identity: TxFilterIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TxFilterIdentity {
    scheme: String,
    sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FirmwareIdentity {
    target: String,
    version: String,
    artifact_sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectedGridIdentity {
    center_frequency_mhz: u32,
    antenna_count: u8,
    subcarrier_count: u16,
    ppdu_type: u8,
    layout_flags: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EnvironmentDefinition {
    layout_revision: String,
    furniture_revision: String,
    door_state_revision: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MmwaveDefinition {
    node_id: String,
    sensor: String,
    firmware: FirmwareIdentity,
    mounting_position_mm: [u32; 3],
    mounting_revision: String,
    transform: MmwaveTransform,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MmwaveTransform {
    origin_x_mm: i32,
    origin_z_mm: i32,
    yaw_mdeg: i32,
    raw_x_inverted: bool,
}

impl MmwaveDefinition {
    pub(crate) fn node_id(&self) -> &str {
        &self.node_id
    }

    pub(crate) fn mounting_position_m(&self) -> [f64; 3] {
        millimetres_to_metres(self.mounting_position_mm)
    }

    pub(crate) fn transform(&self) -> (i32, i32, i32, bool) {
        (
            self.transform.origin_x_mm,
            self.transform.origin_z_mm,
            self.transform.yaw_mdeg,
            self.transform.raw_x_inverted,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ServerBuildIdentity {
    package_version: String,
    executable_sha256: String,
}

/// Path-free sealed artifact written by the setup creation mode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SealedPositionSetup {
    schema_version: u16,
    kind: String,
    setup_id: String,
    setup_sha256: String,
    definition: PositionSetupDefinition,
}

impl SealedPositionSetup {
    pub(crate) fn setup_id(&self) -> &str {
        &self.setup_id
    }

    pub(crate) fn setup_sha256(&self) -> &str {
        &self.setup_sha256
    }

    pub(crate) fn definition(&self) -> &PositionSetupDefinition {
        &self.definition
    }

    pub(crate) fn room_dimensions_m(&self) -> [f64; 3] {
        millimetres_to_metres(self.definition.room_dimensions_mm)
    }

    pub(crate) fn transmitter_position_m(&self) -> [f64; 3] {
        millimetres_to_metres(self.definition.transmitter.position_mm)
    }

    pub(crate) fn receiver_positions_m(&self) -> [[f64; 3]; 4] {
        std::array::from_fn(|index| {
            millimetres_to_metres(self.definition.receivers[index].position_mm)
        })
    }

    pub(crate) fn mmwave(&self) -> Option<&MmwaveDefinition> {
        self.definition.mmwave.as_ref()
    }

    /// Require the overlapping measurement geometry and public deployment
    /// metadata in an Observatory setup profile to match this runtime seal.
    /// Firmware, grid, filter, transform, host, and server identities remain
    /// exclusive to the stricter schema-v2 setup artifact.
    pub(crate) fn validate_observatory_profile(
        &self,
        document: &serde_json::Value,
    ) -> Result<(), String> {
        let room = profile_triplet_mm(document.get("room_dimensions_m"), "room_dimensions_m")?;
        if room != self.definition.room_dimensions_mm {
            return Err(
                "setup profile room dimensions do not match the active sealed setup".to_string(),
            );
        }

        let transmitter = document
            .get("transmitter")
            .and_then(serde_json::Value::as_object)
            .ok_or_else(|| "setup profile transmitter must be an object".to_string())?;
        let transmitter_position = profile_triplet_mm(
            transmitter.get("position_m"),
            "transmitter.position_m",
        )?;
        if transmitter_position != self.definition.transmitter.position_mm {
            return Err(
                "setup profile transmitter position does not match the active sealed setup"
                    .to_string(),
            );
        }

        let receivers = document
            .get("receivers")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| "setup profile receivers must be an array".to_string())?;
        if receivers.len() != self.definition.receivers.len() {
            return Err(
                "setup profile receiver count does not match the active sealed setup".to_string(),
            );
        }
        for (index, (profile_receiver, sealed_receiver)) in receivers
            .iter()
            .zip(&self.definition.receivers)
            .enumerate()
        {
            let expected_id = format!("RX{}", sealed_receiver.rx_id);
            if profile_receiver.get("id").and_then(serde_json::Value::as_str)
                != Some(expected_id.as_str())
            {
                return Err(format!(
                    "setup profile receiver {} does not match {expected_id}",
                    index + 1
                ));
            }
            let field = format!("receivers[{index}].position_m");
            if profile_triplet_mm(profile_receiver.get("position_m"), &field)?
                != sealed_receiver.position_mm
            {
                return Err(format!(
                    "setup profile {expected_id} position does not match the active sealed setup"
                ));
            }
        }

        let radio_channel = document
            .pointer("/radio/channel")
            .and_then(serde_json::Value::as_u64)
            .and_then(|channel| u8::try_from(channel).ok())
            .ok_or_else(|| "setup profile radio.channel must be an integer".to_string())?;
        if radio_channel != self.definition.radio.channel {
            return Err(
                "setup profile radio channel does not match the active sealed setup".to_string(),
            );
        }

        for (field, sealed_value) in [
            (
                "layout_revision",
                self.definition.environment.layout_revision.as_str(),
            ),
            (
                "furniture_revision",
                self.definition.environment.furniture_revision.as_str(),
            ),
            (
                "door_state_revision",
                self.definition.environment.door_state_revision.as_str(),
            ),
        ] {
            let pointer = format!("/environment/{field}");
            if document.pointer(&pointer).and_then(serde_json::Value::as_str)
                != Some(sealed_value)
            {
                return Err(format!(
                    "setup profile environment.{field} does not match the active sealed setup"
                ));
            }
        }

        let profile_mmwave = document
            .get("mmwave")
            .and_then(serde_json::Value::as_object)
            .ok_or_else(|| "setup profile mmwave must be an object".to_string())?;
        let sealed_mmwave = self
            .definition
            .mmwave
            .as_ref()
            .ok_or_else(|| {
                "active sealed setup is not schema v2 with mmWave identity".to_string()
            })?;
        if profile_mmwave
            .get("sensor")
            .and_then(serde_json::Value::as_str)
            != Some(sealed_mmwave.sensor.as_str())
        {
            return Err(
                "setup profile mmWave sensor does not match the active sealed setup".to_string(),
            );
        }
        if profile_triplet_mm(
            profile_mmwave.get("mounting_position_m"),
            "mmwave.mounting_position_m",
        )? != sealed_mmwave.mounting_position_mm
        {
            return Err(
                "setup profile mmWave mounting position does not match the active sealed setup"
                    .to_string(),
            );
        }
        if profile_mmwave
            .get("mounting_revision")
            .and_then(serde_json::Value::as_str)
            != Some(sealed_mmwave.mounting_revision.as_str())
        {
            return Err(
                "setup profile mmWave mounting revision does not match the active sealed setup"
                    .to_string(),
            );
        }
        Ok(())
    }

    /// Require any explicitly repeated runtime geometry to be exactly the
    /// millimetre geometry covered by this setup's seal.
    pub(crate) fn validate_explicit_geometry_mm(
        &self,
        room_dimensions_mm: Option<[u32; 3]>,
        transmitter_position_mm: Option<[u32; 3]>,
        receiver_positions_mm: Option<[[u32; 3]; 4]>,
    ) -> Result<(), String> {
        if room_dimensions_mm.is_some_and(|actual| actual != self.definition.room_dimensions_mm) {
            return Err(format!(
                "explicit room dimensions do not match sealed setup geometry {:?}",
                self.definition.room_dimensions_mm
            ));
        }
        if transmitter_position_mm
            .is_some_and(|actual| actual != self.definition.transmitter.position_mm)
        {
            return Err(format!(
                "explicit transmitter position does not match sealed setup geometry {:?}",
                self.definition.transmitter.position_mm
            ));
        }
        let expected_receivers: [[u32; 3]; 4] =
            std::array::from_fn(|index| self.definition.receivers[index].position_mm);
        if receiver_positions_mm.is_some_and(|actual| actual != expected_receivers) {
            return Err(format!(
                "explicit receiver positions do not match sealed setup geometry {expected_receivers:?}"
            ));
        }
        Ok(())
    }

    fn receiver_for_raw_csi_frame(
        &self,
        frame: &RawCsiFrame,
    ) -> Result<&ReceiverDefinition, String> {
        self.definition
            .receivers
            .get(usize::from(frame.rx_id.saturating_sub(1)))
            .filter(|receiver| receiver.rx_id == frame.rx_id)
            .ok_or_else(|| {
                format!(
                    "raw CSI frame uses RX{}, but sealed setup accepts exactly RX1-RX4",
                    frame.rx_id
                )
            })
    }

    /// Validate the RX and complete runtime TX identity independently of the
    /// CSI symbol grid. A valid controlled transmitter can legitimately
    /// produce more than one CSI grid; callers may filter non-selected grids
    /// without treating their source identity as invalid.
    pub(crate) fn validate_raw_csi_source_identity(
        &self,
        frame: &RawCsiFrame,
    ) -> Result<(), String> {
        self.receiver_for_raw_csi_frame(frame)?;
        let binding = frame.source_binding.as_ref().ok_or_else(|| {
            format!(
                "raw CSI frame from RX{} has no TX-source binding trailer",
                frame.rx_id
            )
        })?;
        binding.validate().map_err(|error| {
            format!(
                "raw CSI frame from RX{} has invalid TX-source binding: {error}",
                frame.rx_id
            )
        })?;
        if binding.flags != SOURCE_BINDING_REQUIRED_FLAGS {
            return Err(format!(
                "raw CSI frame from RX{} has TX-source flags 0x{:02x}; \
                 sealed setup requires exactly 0x{:02x}",
                frame.rx_id, binding.flags, SOURCE_BINDING_REQUIRED_FLAGS
            ));
        }
        if binding.scheme != self.definition.radio.tx_filter_identity.scheme {
            return Err(format!(
                "raw CSI frame from RX{} uses TX-source binding scheme {:?}; \
                 sealed setup requires {:?}",
                frame.rx_id, binding.scheme, self.definition.radio.tx_filter_identity.scheme
            ));
        }
        if binding.tx_filter_sha256 != self.definition.radio.tx_filter_identity.sha256 {
            return Err(format!(
                "raw CSI frame from RX{} TX-filter identity does not match the sealed setup",
                frame.rx_id
            ));
        }
        Ok(())
    }

    /// Return whether this identity-valid frame uses the grid sealed for its
    /// receiver. Mesh-sync bit 0x10 is transient and does not change the grid.
    pub(crate) fn raw_csi_frame_matches_expected_grid(
        &self,
        frame: &RawCsiFrame,
    ) -> Result<bool, String> {
        let receiver = self.receiver_for_raw_csi_frame(frame)?;
        let expected = receiver.expected_grid;
        let actual_flags = frame.flags & POSITION_LAYOUT_FLAGS_MASK;
        let expected_flags = expected.layout_flags & POSITION_LAYOUT_FLAGS_MASK;
        Ok(frame.center_frequency_mhz == expected.center_frequency_mhz
            && frame.antenna_count == expected.antenna_count
            && frame.subcarrier_count == expected.subcarrier_count
            && frame.ppdu_type == expected.ppdu_type
            && actual_flags == expected_flags)
    }

    /// Validate that a captured frame belongs to the complete sealed TX/RX
    /// experiment, including the selected CSI grid.
    pub(crate) fn validate_raw_csi_frame(&self, frame: &RawCsiFrame) -> Result<(), String> {
        self.validate_raw_csi_source_identity(frame)?;
        if !self.raw_csi_frame_matches_expected_grid(frame)? {
            let expected = self.receiver_for_raw_csi_frame(frame)?.expected_grid;
            let actual_flags = frame.flags & POSITION_LAYOUT_FLAGS_MASK;
            let expected_flags = expected.layout_flags & POSITION_LAYOUT_FLAGS_MASK;
            return Err(format!(
                "raw CSI frame from RX{} has grid ({}, {}, {}, {}, 0x{:02x}); \
                 sealed setup requires ({}, {}, {}, {}, 0x{:02x})",
                frame.rx_id,
                frame.center_frequency_mhz,
                frame.antenna_count,
                frame.subcarrier_count,
                frame.ppdu_type,
                actual_flags,
                expected.center_frequency_mhz,
                expected.antenna_count,
                expected.subcarrier_count,
                expected.ppdu_type,
                expected_flags,
            ));
        }
        Ok(())
    }

    /// Recompute and verify the seal before accepting deserialized data.
    pub(crate) fn validate(&self) -> Result<(), String> {
        if !(MIN_SETUP_SCHEMA_VERSION..=SETUP_SCHEMA_VERSION).contains(&self.schema_version) {
            return Err(format!(
                "position setup schema must be {MIN_SETUP_SCHEMA_VERSION} or {SETUP_SCHEMA_VERSION}, got {}",
                self.schema_version
            ));
        }
        if self.schema_version != self.definition.schema_version {
            return Err("position setup and definition schema versions differ".to_string());
        }
        if self.kind != SETUP_ARTIFACT_KIND {
            return Err(format!(
                "position setup kind must be {SETUP_ARTIFACT_KIND:?}, got {:?}",
                self.kind
            ));
        }
        validate_definition(&self.definition)?;
        let expected_sha256 = definition_sha256(&self.definition)?;
        if self.setup_sha256 != expected_sha256 {
            return Err(
                "position setup_sha256 does not match its canonical definition".to_string(),
            );
        }
        let expected_id = setup_id_from_sha256(&expected_sha256);
        if self.setup_id != expected_id {
            return Err(format!(
                "position setup_id must be {expected_id:?} for its setup_sha256"
            ));
        }
        Ok(())
    }
}

/// Build a deliberately incomplete schema-v2 specification from the exact
/// geometry and deployment metadata saved by the Observatory CAD editor.
/// Hardware identities stay null so this draft cannot be mistaken for a
/// sealable setup specification.
pub(crate) fn observatory_profile_setup_draft(
    document: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let room = profile_triplet_mm(document.get("room_dimensions_m"), "room_dimensions_m")?;
    if room.contains(&0) {
        return Err("setup profile room dimensions must all be greater than zero".to_string());
    }

    let transmitter = document
        .get("transmitter")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| "setup profile transmitter must be an object".to_string())?;
    if transmitter.get("id").and_then(serde_json::Value::as_str) != Some("TX") {
        return Err("setup profile transmitter must be named TX".to_string());
    }
    let transmitter_position = profile_triplet_mm(
        transmitter.get("position_m"),
        "transmitter.position_m",
    )?;
    validate_position("transmitter.position_mm", transmitter_position, room)?;

    let receivers = document
        .get("receivers")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "setup profile receivers must be an array".to_string())?;
    if receivers.len() != EXPECTED_RX_IDS.len() {
        return Err("setup profile must contain exactly RX1 through RX4".to_string());
    }
    let receiver_drafts = receivers
        .iter()
        .enumerate()
        .map(|(index, receiver)| {
            let rx_id = EXPECTED_RX_IDS[index];
            let expected_id = format!("RX{rx_id}");
            if receiver.get("id").and_then(serde_json::Value::as_str)
                != Some(expected_id.as_str())
            {
                return Err(format!(
                    "setup profile receiver {} must be named {expected_id}",
                    index + 1
                ));
            }
            let field = format!("receivers[{index}].position_m");
            let position = profile_triplet_mm(receiver.get("position_m"), &field)?;
            validate_position(&format!("{expected_id}.position_mm"), position, room)?;
            Ok(serde_json::json!({
                "rx_id": rx_id,
                "position_mm": position,
                "firmware": null,
                "expected_grid": null,
            }))
        })
        .collect::<Result<Vec<_>, String>>()?;

    let radio_channel = document
        .pointer("/radio/channel")
        .and_then(serde_json::Value::as_u64)
        .and_then(|channel| u8::try_from(channel).ok())
        .filter(|channel| (1..=13).contains(channel))
        .ok_or_else(|| {
            "setup profile radio.channel must be a 2.4 GHz channel from 1 to 13".to_string()
        })?;

    let layout_revision = profile_public_identifier(
        document.pointer("/environment/layout_revision"),
        "environment.layout_revision",
    )?;
    let furniture_revision = profile_public_identifier(
        document.pointer("/environment/furniture_revision"),
        "environment.furniture_revision",
    )?;
    let door_state_revision = profile_public_identifier(
        document.pointer("/environment/door_state_revision"),
        "environment.door_state_revision",
    )?;

    let mmwave = document
        .get("mmwave")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| "setup profile mmwave must be an object".to_string())?;
    if mmwave.get("sensor").and_then(serde_json::Value::as_str) != Some("HLK-LD2450") {
        return Err("setup profile mmwave.sensor must be HLK-LD2450".to_string());
    }
    let mmwave_position = profile_triplet_mm(
        mmwave.get("mounting_position_m"),
        "mmwave.mounting_position_m",
    )?;
    validate_position("mmwave.mounting_position_mm", mmwave_position, room)?;
    let mmwave_mounting_revision = profile_public_identifier(
        mmwave.get("mounting_revision"),
        "mmwave.mounting_revision",
    )?;

    Ok(serde_json::json!({
        "draft_schema_version": 1,
        "kind": OBSERVATORY_SETUP_DRAFT_KIND,
        "ready_to_seal": false,
        "missing_sections": OBSERVATORY_SETUP_DRAFT_MISSING_SECTIONS,
        "spec": {
            "schema_version": 2,
            "coordinate_system": COORDINATE_SYSTEM,
            "room_dimensions_mm": room,
            "transmitter": {
                "position_mm": transmitter_position,
                "firmware": null,
            },
            "receivers": receiver_drafts,
            "recording_host": null,
            "radio": {
                "channel": radio_channel,
                "tx_filter_identity": null,
            },
            "environment": {
                "layout_revision": layout_revision,
                "furniture_revision": furniture_revision,
                "door_state_revision": door_state_revision,
            },
            "mmwave": {
                "node_id": null,
                "sensor": "HLK-LD2450",
                "firmware": null,
                "mounting_position_mm": mmwave_position,
                "mounting_revision": mmwave_mounting_revision,
                "transform": null,
            },
        },
    }))
}

fn profile_triplet_mm(
    value: Option<&serde_json::Value>,
    field: &str,
) -> Result<[u32; 3], String> {
    let coordinates = value
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| format!("setup profile {field} must be an array"))?;
    if coordinates.len() != 3 {
        return Err(format!(
            "setup profile {field} must contain exactly three coordinates"
        ));
    }
    let mut result = [0_u32; 3];
    for (index, coordinate) in coordinates.iter().enumerate() {
        let metres = coordinate
            .as_f64()
            .filter(|value| value.is_finite() && *value >= 0.0)
            .ok_or_else(|| format!("setup profile {field}[{index}] must be non-negative"))?;
        let millimetres = metres * 1_000.0;
        let rounded = millimetres.round();
        if (millimetres - rounded).abs() > 1e-6 || rounded > f64::from(u32::MAX) {
            return Err(format!(
                "setup profile {field}[{index}] must resolve to an exact whole millimetre"
            ));
        }
        result[index] = rounded as u32;
    }
    Ok(result)
}

fn profile_public_identifier(
    value: Option<&serde_json::Value>,
    field: &str,
) -> Result<String, String> {
    let value = value
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| format!("setup profile {field} must be a string"))?;
    validate_public_identifier(field, value)?;
    Ok(value.to_string())
}

fn millimetres_to_metres(value: [u32; 3]) -> [f64; 3] {
    value.map(|coordinate| f64::from(coordinate) / 1_000.0)
}

/// Create and seal a setup from a strict JSON specification.
pub(crate) fn create_position_setup(spec_path: &Path) -> Result<SealedPositionSetup, String> {
    if !spec_path.is_file() {
        return Err(format!(
            "position setup specification {} is not a regular file",
            spec_path.display()
        ));
    }
    let bytes = std::fs::read(spec_path).map_err(|error| {
        format!(
            "could not read position setup specification {}: {error}",
            spec_path.display()
        )
    })?;
    let spec: PositionSetupSpec = serde_json::from_slice(&bytes).map_err(|error| {
        format!(
            "invalid position setup specification {}: {error}",
            spec_path.display()
        )
    })?;
    let executable = std::env::current_exe()
        .map_err(|error| format!("cannot resolve current sensing-server executable: {error}"))?;
    create_position_setup_with_executable(spec, &executable)
}

/// Load a seal and require that it was created for the running server binary.
pub(crate) fn load_position_setup_for_current_executable(
    path: &Path,
) -> Result<SealedPositionSetup, String> {
    let setup = load_position_setup(path)?;
    let executable = std::env::current_exe()
        .map_err(|error| format!("cannot resolve current sensing-server executable: {error}"))?;
    validate_current_server(&setup, &executable)?;
    Ok(setup)
}

fn load_position_setup(path: &Path) -> Result<SealedPositionSetup, String> {
    if !path.is_file() {
        return Err(format!(
            "sealed position setup {} is not a regular file",
            path.display()
        ));
    }
    let bytes = std::fs::read(path)
        .map_err(|error| format!("could not read sealed setup {}: {error}", path.display()))?;
    let setup: SealedPositionSetup = serde_json::from_slice(&bytes)
        .map_err(|error| format!("invalid sealed setup {}: {error}", path.display()))?;
    setup
        .validate()
        .map_err(|error| format!("invalid sealed setup {}: {error}", path.display()))?;
    Ok(setup)
}

fn create_position_setup_with_executable(
    spec: PositionSetupSpec,
    executable: &Path,
) -> Result<SealedPositionSetup, String> {
    validate_spec(&spec)?;
    let executable_sha256 = sha256_file(executable)
        .map_err(|error| format!("cannot hash sensing-server executable: {error}"))?;
    let definition = PositionSetupDefinition {
        schema_version: spec.schema_version,
        coordinate_system: spec.coordinate_system,
        room_dimensions_mm: spec.room_dimensions_mm,
        transmitter: spec.transmitter,
        receivers: spec.receivers,
        recording_host: spec.recording_host,
        radio: spec.radio,
        environment: spec.environment,
        mmwave: spec.mmwave,
        server: ServerBuildIdentity {
            package_version: env!("CARGO_PKG_VERSION").to_string(),
            executable_sha256,
        },
    };
    validate_definition(&definition)?;
    let setup_sha256 = definition_sha256(&definition)?;
    let setup = SealedPositionSetup {
        schema_version: definition.schema_version,
        kind: SETUP_ARTIFACT_KIND.to_string(),
        setup_id: setup_id_from_sha256(&setup_sha256),
        setup_sha256,
        definition,
    };
    setup.validate()?;
    Ok(setup)
}

fn validate_current_server(setup: &SealedPositionSetup, executable: &Path) -> Result<(), String> {
    if setup.definition.server.package_version != env!("CARGO_PKG_VERSION") {
        return Err(format!(
            "sealed setup requires sensing-server package version {:?}, running version is {:?}",
            setup.definition.server.package_version,
            env!("CARGO_PKG_VERSION")
        ));
    }
    let actual_sha256 = sha256_file(executable)
        .map_err(|error| format!("cannot hash sensing-server executable: {error}"))?;
    if setup.definition.server.executable_sha256 != actual_sha256 {
        return Err(
            "sealed setup executable_sha256 does not match the running sensing-server executable"
                .to_string(),
        );
    }
    Ok(())
}

fn validate_spec(spec: &PositionSetupSpec) -> Result<(), String> {
    let definition = PositionSetupDefinition {
        schema_version: spec.schema_version,
        coordinate_system: spec.coordinate_system.clone(),
        room_dimensions_mm: spec.room_dimensions_mm,
        transmitter: spec.transmitter.clone(),
        receivers: spec.receivers.clone(),
        recording_host: spec.recording_host.clone(),
        radio: spec.radio.clone(),
        environment: spec.environment.clone(),
        mmwave: spec.mmwave.clone(),
        server: ServerBuildIdentity {
            package_version: env!("CARGO_PKG_VERSION").to_string(),
            executable_sha256: "0".repeat(SHA256_HEX_LEN),
        },
    };
    validate_definition(&definition)
}

fn validate_definition(definition: &PositionSetupDefinition) -> Result<(), String> {
    if !(MIN_SETUP_SCHEMA_VERSION..=SETUP_SCHEMA_VERSION).contains(&definition.schema_version) {
        return Err(format!(
            "setup definition schema must be {MIN_SETUP_SCHEMA_VERSION} or {SETUP_SCHEMA_VERSION}, got {}",
            definition.schema_version
        ));
    }
    match (definition.schema_version, definition.mmwave.as_ref()) {
        (1, None) => {}
        (1, Some(_)) => return Err("schema v1 must not contain mmwave".to_string()),
        (2, Some(mmwave)) => validate_mmwave(mmwave, definition.room_dimensions_mm)?,
        (2, None) => return Err("schema v2 requires mmwave".to_string()),
        _ => unreachable!("schema range checked above"),
    }
    if definition.coordinate_system != COORDINATE_SYSTEM {
        return Err(format!(
            "coordinate_system must be {COORDINATE_SYSTEM:?}, got {:?}",
            definition.coordinate_system
        ));
    }
    if definition.room_dimensions_mm.contains(&0) {
        return Err("room dimensions must all be greater than zero millimetres".to_string());
    }

    validate_position(
        "transmitter.position_mm",
        definition.transmitter.position_mm,
        definition.room_dimensions_mm,
    )?;
    validate_firmware("transmitter.firmware", &definition.transmitter.firmware)?;

    let receiver_ids: Vec<u8> = definition
        .receivers
        .iter()
        .map(|receiver| receiver.rx_id)
        .collect();
    if receiver_ids != EXPECTED_RX_IDS {
        return Err(format!(
            "receivers must be exactly RX1-RX4 in order, got {receiver_ids:?}"
        ));
    }
    let mut receiver_positions = HashSet::new();
    for receiver in &definition.receivers {
        validate_position(
            &format!("RX{}.position_mm", receiver.rx_id),
            receiver.position_mm,
            definition.room_dimensions_mm,
        )?;
        if !receiver_positions.insert(receiver.position_mm) {
            return Err("receiver positions must be unique".to_string());
        }
        validate_firmware(
            &format!("RX{}.firmware", receiver.rx_id),
            &receiver.firmware,
        )?;
        validate_grid(
            receiver.rx_id,
            definition.radio.channel,
            receiver.expected_grid,
        )?;
    }

    if definition.recording_host.kind != RECORDING_HOST_KIND {
        return Err(format!(
            "recording_host.kind must be {RECORDING_HOST_KIND:?}"
        ));
    }
    validate_position(
        "recording_host.position_mm",
        definition.recording_host.position_mm,
        definition.room_dimensions_mm,
    )?;
    validate_public_identifier(
        "recording_host.placement_revision",
        &definition.recording_host.placement_revision,
    )?;
    validate_public_identifier(
        "recording_host.cable_revision",
        &definition.recording_host.cable_revision,
    )?;

    if !(1..=13).contains(&definition.radio.channel) {
        return Err("radio.channel must be a 2.4 GHz WiFi channel from 1 to 13".to_string());
    }
    if definition.radio.tx_filter_identity.scheme != TX_FILTER_SCHEME {
        return Err(format!(
            "radio.tx_filter_identity.scheme must be {TX_FILTER_SCHEME:?}"
        ));
    }
    validate_sha256(
        "radio.tx_filter_identity.sha256",
        &definition.radio.tx_filter_identity.sha256,
    )?;

    validate_public_identifier(
        "environment.layout_revision",
        &definition.environment.layout_revision,
    )?;
    validate_public_identifier(
        "environment.furniture_revision",
        &definition.environment.furniture_revision,
    )?;
    validate_public_identifier(
        "environment.door_state_revision",
        &definition.environment.door_state_revision,
    )?;
    validate_public_identifier("server.package_version", &definition.server.package_version)?;
    validate_sha256(
        "server.executable_sha256",
        &definition.server.executable_sha256,
    )?;
    Ok(())
}

fn validate_mmwave(mmwave: &MmwaveDefinition, room: [u32; 3]) -> Result<(), String> {
    validate_public_identifier("mmwave.node_id", &mmwave.node_id)?;
    if mmwave.sensor != "HLK-LD2450" {
        return Err("mmwave.sensor must be \"HLK-LD2450\"".to_string());
    }
    validate_firmware("mmwave.firmware", &mmwave.firmware)?;
    validate_position(
        "mmwave.mounting_position_mm",
        mmwave.mounting_position_mm,
        room,
    )?;
    validate_public_identifier("mmwave.mounting_revision", &mmwave.mounting_revision)?;
    if mmwave.transform.origin_x_mm != mmwave.mounting_position_mm[0] as i32
        || mmwave.transform.origin_z_mm != mmwave.mounting_position_mm[2] as i32
    {
        return Err(
            "mmwave transform origin must equal the sealed mounting x/z position".to_string(),
        );
    }
    if mmwave.transform.yaw_mdeg.abs() > 360_000 {
        return Err("mmwave.transform.yaw_mdeg must be between -360000 and 360000".to_string());
    }
    Ok(())
}

fn validate_position(field: &str, position: [u32; 3], room: [u32; 3]) -> Result<(), String> {
    if position[0] > room[0] || position[1] > room[1] || position[2] > room[2] {
        return Err(format!(
            "{field} {position:?} lies outside room_dimensions_mm {room:?}"
        ));
    }
    Ok(())
}

fn validate_firmware(field: &str, firmware: &FirmwareIdentity) -> Result<(), String> {
    validate_public_identifier(&format!("{field}.target"), &firmware.target)?;
    validate_public_identifier(&format!("{field}.version"), &firmware.version)?;
    validate_sha256(
        &format!("{field}.artifact_sha256"),
        &firmware.artifact_sha256,
    )
}

fn validate_grid(rx_id: u8, channel: u8, grid: ExpectedGridIdentity) -> Result<(), String> {
    let expected_frequency_mhz = 2_407_u32 + 5 * u32::from(channel);
    if grid.center_frequency_mhz != expected_frequency_mhz {
        return Err(format!(
            "RX{rx_id} expected grid frequency {} MHz does not match channel {channel} ({expected_frequency_mhz} MHz)",
            grid.center_frequency_mhz
        ));
    }
    if grid.antenna_count == 0 || grid.subcarrier_count == 0 {
        return Err(format!(
            "RX{rx_id} expected grid dimensions must both be greater than zero"
        ));
    }
    if !matches!(grid.ppdu_type, 0..=3 | 0xff) {
        return Err(format!(
            "RX{rx_id} expected grid has unsupported ppdu_type {}",
            grid.ppdu_type
        ));
    }
    if grid.layout_flags & !POSITION_LAYOUT_FLAGS_MASK != 0 {
        return Err(format!(
            "RX{rx_id} expected grid must not contain transient time-sync flag 0x10"
        ));
    }
    Ok(())
}

fn validate_public_identifier(field: &str, value: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > 128 || value.trim() != value {
        return Err(format!(
            "{field} must contain 1-128 characters without surrounding whitespace"
        ));
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b'+'))
    {
        return Err(format!(
            "{field} may contain only ASCII letters, digits, dot, underscore, hyphen, and plus"
        ));
    }
    if looks_like_ipv4(value) || looks_like_hyphenated_mac(value) {
        return Err(format!("{field} must not contain an IP address or raw MAC"));
    }
    Ok(())
}

fn looks_like_ipv4(value: &str) -> bool {
    let parts: Vec<&str> = value.split('.').collect();
    parts.len() == 4
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.parse::<u8>().is_ok())
}

fn looks_like_hyphenated_mac(value: &str) -> bool {
    let parts: Vec<&str> = value.split('-').collect();
    parts.len() == 6
        && parts
            .iter()
            .all(|part| part.len() == 2 && part.bytes().all(|byte| byte.is_ascii_hexdigit()))
}

fn validate_sha256(field: &str, value: &str) -> Result<(), String> {
    if value.len() != SHA256_HEX_LEN
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(format!(
            "{field} must be exactly 64 lowercase hexadecimal characters"
        ));
    }
    Ok(())
}

fn definition_sha256(definition: &PositionSetupDefinition) -> Result<String, String> {
    let canonical = deterministic_pretty_json(definition)
        .map_err(|error| format!("cannot encode canonical setup definition: {error}"))?;
    let domain = match definition.schema_version {
        1 => SETUP_HASH_DOMAIN_V1,
        2 => SETUP_HASH_DOMAIN_V2,
        _ => return Err("unsupported setup schema for hashing".to_string()),
    };
    let mut sealed = Vec::with_capacity(domain.len() + canonical.len());
    sealed.extend_from_slice(domain);
    sealed.extend_from_slice(&canonical);
    Ok(sha256_bytes(&sealed))
}

fn setup_id_from_sha256(setup_sha256: &str) -> String {
    format!("setup-{}", &setup_sha256[..16])
}

fn tx_filter_identity_sha256(filter_mac_bytes: [u8; 6]) -> String {
    sha256_bytes(&filter_mac_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::raw_csi_recording::{
        IqPair, SourceBinding, RAW_CSI_SCHEMA_VERSION, SOURCE_BINDING_FLAG_FILTER_CONFIGURED,
        TX_SOURCE_BINDING_VERSION,
    };
    use serde_json::{json, Value};
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn hash(character: char) -> String {
        std::iter::repeat(character).take(64).collect()
    }

    #[test]
    fn tx_filter_identity_hashes_the_exact_six_nvs_bytes() {
        assert_eq!(
            tx_filter_identity_sha256([0x00, 0x11, 0x22, 0x33, 0x44, 0x55]),
            "48f4634d1002f9f3c7570cb43e00dd869b22c79538e9b4adc7e402de1189cfe1"
        );
    }

    fn valid_spec_value() -> Value {
        let receivers: Vec<Value> = (1_u8..=4)
            .map(|rx_id| {
                json!({
                    "rx_id": rx_id,
                    "position_mm": [u32::from(rx_id) * 100, 500, u32::from(rx_id) * 200],
                    "firmware": {
                        "target": "esp32s3",
                        "version": "d6-v1",
                        "artifact_sha256": hash(char::from(b'a' + rx_id)),
                    },
                    "expected_grid": {
                        "center_frequency_mhz": 2437,
                        "antenna_count": 1,
                        "subcarrier_count": 64,
                        "ppdu_type": 0,
                        "layout_flags": 0
                    }
                })
            })
            .collect();
        json!({
            "schema_version": 1,
            "coordinate_system": COORDINATE_SYSTEM,
            "room_dimensions_mm": [4020, 2590, 3440],
            "transmitter": {
                "position_mm": [1510, 1190, 390],
                "firmware": {
                    "target": "esp32s3",
                    "version": "tx-v1",
                    "artifact_sha256": hash('a')
                }
            },
            "receivers": receivers,
            "recording_host": {
                "kind": "mac",
                "position_mm": [2010, 740, 1720],
                "placement_revision": "final-center-v1",
                "cable_revision": "ethernet-left-v1"
            },
            "radio": {
                "channel": 6,
                "tx_filter_identity": {
                    "scheme": TX_FILTER_SCHEME,
                    "sha256": hash('f')
                }
            },
            "environment": {
                "layout_revision": "fixed-room-v1",
                "furniture_revision": "normal-use-v1",
                "door_state_revision": "closed-v1"
            }
        })
    }

    fn parse_spec(value: Value) -> Result<PositionSetupSpec, serde_json::Error> {
        serde_json::from_value(value)
    }

    fn replace_template_value(template: &mut String, token: &str, json_value: &str) {
        let placeholder = format!("\"{token}\"");
        assert!(
            template.contains(&placeholder),
            "setup template is missing placeholder {token}"
        );
        *template = template.replace(&placeholder, json_value);
    }

    #[test]
    fn public_setup_template_matches_strict_schema() {
        let source = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../scripts/templates/position-setup-spec.template.json"
        ));
        serde_json::from_str::<Value>(source).expect("public setup template must be valid JSON");
        assert!(!source.contains("\"ssid\""));
        assert!(!source.contains("\"password\""));
        assert!(!source.contains("\"filter_mac\""));

        let mut rendered = source.to_string();
        for (token, value) in [
            ("__ROOM_DIMENSIONS_MM__", "[4020,2590,3440]"),
            ("__TX_POSITION_MM__", "[1510,1190,390]"),
            ("__TX_FIRMWARE_TARGET__", "\"esp32s3\""),
            ("__TX_FIRMWARE_VERSION__", "\"sender-test-v1\""),
            (
                "__TX_FIRMWARE_ARTIFACT_SHA256__",
                "\"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"",
            ),
            ("__RX1_POSITION_MM__", "[0,500,280]"),
            ("__RX2_POSITION_MM__", "[4020,870,970]"),
            ("__RX3_POSITION_MM__", "[0,740,2110]"),
            ("__RX4_POSITION_MM__", "[4020,870,2460]"),
            ("__RX1_FIRMWARE_TARGET__", "\"esp32s3\""),
            ("__RX2_FIRMWARE_TARGET__", "\"esp32s3\""),
            ("__RX3_FIRMWARE_TARGET__", "\"esp32s3\""),
            ("__RX4_FIRMWARE_TARGET__", "\"esp32s3\""),
            ("__RX1_FIRMWARE_VERSION__", "\"receiver-test-v1\""),
            ("__RX2_FIRMWARE_VERSION__", "\"receiver-test-v1\""),
            ("__RX3_FIRMWARE_VERSION__", "\"receiver-test-v1\""),
            ("__RX4_FIRMWARE_VERSION__", "\"receiver-test-v1\""),
            (
                "__RX1_FIRMWARE_ARTIFACT_SHA256__",
                "\"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\"",
            ),
            (
                "__RX2_FIRMWARE_ARTIFACT_SHA256__",
                "\"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc\"",
            ),
            (
                "__RX3_FIRMWARE_ARTIFACT_SHA256__",
                "\"dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd\"",
            ),
            (
                "__RX4_FIRMWARE_ARTIFACT_SHA256__",
                "\"eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee\"",
            ),
            ("__RX1_CENTER_FREQUENCY_MHZ__", "2437"),
            ("__RX2_CENTER_FREQUENCY_MHZ__", "2437"),
            ("__RX3_CENTER_FREQUENCY_MHZ__", "2437"),
            ("__RX4_CENTER_FREQUENCY_MHZ__", "2437"),
            ("__RX1_ANTENNA_COUNT__", "1"),
            ("__RX2_ANTENNA_COUNT__", "1"),
            ("__RX3_ANTENNA_COUNT__", "1"),
            ("__RX4_ANTENNA_COUNT__", "1"),
            ("__RX1_SUBCARRIER_COUNT__", "64"),
            ("__RX2_SUBCARRIER_COUNT__", "64"),
            ("__RX3_SUBCARRIER_COUNT__", "64"),
            ("__RX4_SUBCARRIER_COUNT__", "64"),
            ("__RX1_PPDU_TYPE__", "0"),
            ("__RX2_PPDU_TYPE__", "0"),
            ("__RX3_PPDU_TYPE__", "0"),
            ("__RX4_PPDU_TYPE__", "0"),
            ("__RX1_LAYOUT_FLAGS__", "0"),
            ("__RX2_LAYOUT_FLAGS__", "0"),
            ("__RX3_LAYOUT_FLAGS__", "0"),
            ("__RX4_LAYOUT_FLAGS__", "0"),
            ("__RECORDING_HOST_POSITION_MM__", "[2010,740,1720]"),
            (
                "__RECORDING_HOST_PLACEMENT_REVISION__",
                "\"test-placement-v1\"",
            ),
            ("__RECORDING_HOST_CABLE_REVISION__", "\"test-cable-v1\""),
            ("__WIFI_CHANNEL__", "6"),
            (
                "__TX_FILTER_IDENTITY_SHA256__",
                "\"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff\"",
            ),
            ("__LAYOUT_REVISION__", "\"test-layout-v1\""),
            ("__FURNITURE_REVISION__", "\"test-furniture-v1\""),
            ("__DOOR_STATE_REVISION__", "\"test-door-closed-v1\""),
            ("__MMWAVE_NODE_ID__", "\"radar-01\""),
            ("__MMWAVE_FIRMWARE_VERSION__", "\"mmwave-v1\""),
            (
                "__MMWAVE_FIRMWARE_ARTIFACT_SHA256__",
                "\"1111111111111111111111111111111111111111111111111111111111111111\"",
            ),
            ("__MMWAVE_MOUNTING_POSITION_MM__", "[0,1200,1720]"),
            ("__MMWAVE_MOUNTING_REVISION__", "\"wall-center-v1\""),
            ("__MMWAVE_ORIGIN_X_MM__", "0"),
            ("__MMWAVE_ORIGIN_Z_MM__", "1720"),
            ("__MMWAVE_YAW_MDEG__", "0"),
            ("__MMWAVE_RAW_X_INVERTED__", "false"),
        ] {
            replace_template_value(&mut rendered, token, value);
        }
        assert!(
            !rendered.contains("__"),
            "every setup-template placeholder must be covered by this schema test"
        );

        let spec: PositionSetupSpec =
            serde_json::from_str(&rendered).expect("rendered setup template must deserialize");
        validate_spec(&spec).expect("rendered setup template must pass the strict validator");
    }

    #[test]
    fn schema_v1_remains_readable_and_schema_v2_binds_mmwave() {
        let directory = TempDir::new().unwrap();
        let executable = executable_fixture(&directory, b"schema compatibility");
        let v1 = create_position_setup_with_executable(
            parse_spec(valid_spec_value()).unwrap(),
            &executable,
        )
        .unwrap();
        assert_eq!(v1.schema_version, 1);
        assert!(v1.mmwave().is_none());

        let mut value = valid_spec_value();
        value["schema_version"] = json!(2);
        value["mmwave"] = json!({
            "node_id": "radar-01",
            "sensor": "HLK-LD2450",
            "firmware": {
                "target": "esp32c3",
                "version": "mmwave-v1",
                "artifact_sha256": hash('1')
            },
            "mounting_position_mm": [0, 1200, 1720],
            "mounting_revision": "wall-center-v1",
            "transform": {
                "origin_x_mm": 0,
                "origin_z_mm": 1720,
                "yaw_mdeg": 0,
                "raw_x_inverted": false
            }
        });
        let v2 =
            create_position_setup_with_executable(parse_spec(value).unwrap(), &executable).unwrap();
        assert_eq!(v2.schema_version, 2);
        assert_eq!(v2.mmwave().unwrap().node_id(), "radar-01");
        assert_ne!(v1.setup_sha256, v2.setup_sha256);
    }

    fn executable_fixture(directory: &TempDir, bytes: &[u8]) -> PathBuf {
        let path = directory.path().join("sensing-server-fixture");
        std::fs::write(&path, bytes).unwrap();
        path
    }

    fn valid_setup(directory: &TempDir) -> SealedPositionSetup {
        let executable = executable_fixture(directory, b"test executable");
        create_position_setup_with_executable(parse_spec(valid_spec_value()).unwrap(), &executable)
            .unwrap()
    }

    fn valid_v2_setup(directory: &TempDir) -> SealedPositionSetup {
        let executable = executable_fixture(directory, b"test executable v2");
        let mut value = valid_spec_value();
        value["schema_version"] = json!(2);
        value["mmwave"] = json!({
            "node_id": "MMWAVE1",
            "sensor": "HLK-LD2450",
            "firmware": {
                "target": "esp32c3",
                "version": "mmwave-v1",
                "artifact_sha256": hash('1')
            },
            "mounting_position_mm": [0, 1200, 1720],
            "mounting_revision": "wall-center-v1",
            "transform": {
                "origin_x_mm": 0,
                "origin_z_mm": 1720,
                "yaw_mdeg": 0,
                "raw_x_inverted": false
            }
        });
        create_position_setup_with_executable(parse_spec(value).unwrap(), &executable).unwrap()
    }

    fn matching_observatory_profile() -> Value {
        json!({
            "room_dimensions_m": [4.02, 2.59, 3.44],
            "transmitter": { "id": "TX", "position_m": [1.51, 1.19, 0.39] },
            "receivers": [
                { "id": "RX1", "position_m": [0.1, 0.5, 0.2] },
                { "id": "RX2", "position_m": [0.2, 0.5, 0.4] },
                { "id": "RX3", "position_m": [0.3, 0.5, 0.6] },
                { "id": "RX4", "position_m": [0.4, 0.5, 0.8] }
            ],
            "radio": { "channel": 6 },
            "environment": {
                "layout_revision": "fixed-room-v1",
                "furniture_revision": "normal-use-v1",
                "door_state_revision": "closed-v1"
            },
            "mmwave": {
                "sensor": "HLK-LD2450",
                "mounting_position_m": [0, 1.2, 1.72],
                "mounting_revision": "wall-center-v1"
            }
        })
    }

    fn valid_source_binding() -> SourceBinding {
        SourceBinding {
            trailer_version: TX_SOURCE_BINDING_VERSION,
            flags: SOURCE_BINDING_REQUIRED_FLAGS,
            scheme: TX_FILTER_SCHEME.to_string(),
            tx_filter_sha256: hash('f'),
        }
    }

    fn raw_frame(rx_id: u8, flags: u8) -> RawCsiFrame {
        RawCsiFrame {
            schema_version: RAW_CSI_SCHEMA_VERSION,
            host_timestamp_unix_ns: 1,
            host_monotonic_ns: Some(1),
            clock_epoch_id: Some("test-clock".to_string()),
            session_id: None,
            label: None,
            ground_truth: None,
            rx_id,
            antenna_count: 1,
            subcarrier_count: 64,
            center_frequency_mhz: 2437,
            sequence: 1,
            rssi_dbm: -50,
            noise_floor_dbm: -90,
            ppdu_type: 0,
            flags,
            mesh_timestamp_us: None,
            source_binding: Some(valid_source_binding()),
            iq_pairs: vec![IqPair { i: 1, q: -1 }; 64],
        }
    }

    #[test]
    fn seal_is_deterministic_and_measurement_changes_change_the_hash() {
        let directory = TempDir::new().unwrap();
        let executable = executable_fixture(&directory, b"same executable");
        let spec = parse_spec(valid_spec_value()).unwrap();
        let first = create_position_setup_with_executable(spec.clone(), &executable).unwrap();
        let second = create_position_setup_with_executable(spec, &executable).unwrap();
        assert_eq!(first, second);
        assert_eq!(
            deterministic_pretty_json(&first).unwrap(),
            deterministic_pretty_json(&second).unwrap()
        );

        let mut changed = valid_spec_value();
        changed["recording_host"]["position_mm"] = json!([2011, 740, 1720]);
        let changed =
            create_position_setup_with_executable(parse_spec(changed).unwrap(), &executable)
                .unwrap();
        assert_ne!(first.setup_sha256, changed.setup_sha256);
        assert_ne!(first.setup_id, changed.setup_id);
    }

    #[test]
    fn observatory_profile_must_match_the_active_v2_setup() {
        let directory = TempDir::new().unwrap();
        let setup = valid_v2_setup(&directory);
        let profile = matching_observatory_profile();

        setup.validate_observatory_profile(&profile).unwrap();

        let mut wrong_receiver = profile.clone();
        wrong_receiver["receivers"][1]["position_m"] = json!([0.201, 0.5, 0.4]);
        assert!(setup
            .validate_observatory_profile(&wrong_receiver)
            .unwrap_err()
            .contains("RX2 position"));

        let mut wrong_environment = profile;
        wrong_environment["environment"]["layout_revision"] = json!("changed-room-v2");
        assert!(setup
            .validate_observatory_profile(&wrong_environment)
            .unwrap_err()
            .contains("environment.layout_revision"));
    }

    #[test]
    fn observatory_profile_coordinates_must_resolve_to_whole_millimetres() {
        let directory = TempDir::new().unwrap();
        let setup = valid_v2_setup(&directory);
        let mut profile = matching_observatory_profile();
        profile["mmwave"]["mounting_position_m"] = json!([0, 1.2005, 1.72]);

        assert!(setup
            .validate_observatory_profile(&profile)
            .unwrap_err()
            .contains("whole millimetre"));
    }

    #[test]
    fn observatory_setup_draft_reuses_cad_geometry_but_cannot_be_sealed() {
        let profile = matching_observatory_profile();

        let draft = observatory_profile_setup_draft(&profile).unwrap();

        assert_eq!(draft["kind"], OBSERVATORY_SETUP_DRAFT_KIND);
        assert_eq!(draft["ready_to_seal"], false);
        assert_eq!(draft["spec"]["room_dimensions_mm"], json!([4020, 2590, 3440]));
        assert_eq!(
            draft["spec"]["transmitter"]["position_mm"],
            json!([1510, 1190, 390])
        );
        assert_eq!(
            draft["spec"]["receivers"][1]["position_mm"],
            json!([200, 500, 400])
        );
        assert_eq!(
            draft["spec"]["mmwave"]["mounting_position_mm"],
            json!([0, 1200, 1720])
        );
        assert_eq!(draft["spec"]["radio"]["channel"], 6);
        assert_eq!(
            draft["missing_sections"],
            json!(OBSERVATORY_SETUP_DRAFT_MISSING_SECTIONS)
        );
        assert!(parse_spec(draft["spec"].clone()).is_err());
    }

    #[test]
    fn observatory_setup_draft_rejects_geometry_outside_sealable_setup_bounds() {
        let mut profile = matching_observatory_profile();
        profile["mmwave"]["mounting_position_m"] = json!([-0.25, 1.2, 1.72]);

        assert!(observatory_profile_setup_draft(&profile)
            .unwrap_err()
            .contains("non-negative"));
    }

    #[test]
    fn executable_identity_is_in_the_sealed_hash() {
        let directory = TempDir::new().unwrap();
        let first_executable = executable_fixture(&directory, b"first executable");
        let second_executable = directory.path().join("second-sensing-server-fixture");
        std::fs::write(&second_executable, b"second executable").unwrap();
        let spec = parse_spec(valid_spec_value()).unwrap();
        let first = create_position_setup_with_executable(spec.clone(), &first_executable).unwrap();
        let second = create_position_setup_with_executable(spec, &second_executable).unwrap();
        assert_ne!(first.setup_sha256, second.setup_sha256);
    }

    #[test]
    fn manipulated_setup_id_or_hash_is_rejected() {
        let directory = TempDir::new().unwrap();
        let executable = executable_fixture(&directory, b"executable");
        let setup = create_position_setup_with_executable(
            parse_spec(valid_spec_value()).unwrap(),
            &executable,
        )
        .unwrap();

        let mut bad_id = setup.clone();
        bad_id.setup_id = "setup-0000000000000000".to_string();
        assert!(bad_id.validate().is_err());

        let mut bad_hash = setup;
        bad_hash.setup_sha256 = hash('0');
        assert!(bad_hash.validate().is_err());
    }

    #[test]
    fn geometry_getters_convert_sealed_millimetres_to_metres() {
        let directory = TempDir::new().unwrap();
        let setup = valid_setup(&directory);

        assert_eq!(setup.room_dimensions_m(), [4.02, 2.59, 3.44]);
        assert_eq!(setup.transmitter_position_m(), [1.51, 1.19, 0.39]);
        assert_eq!(
            setup.receiver_positions_m(),
            [
                [0.1, 0.5, 0.2],
                [0.2, 0.5, 0.4],
                [0.3, 0.5, 0.6],
                [0.4, 0.5, 0.8],
            ]
        );
    }

    #[test]
    fn explicit_geometry_must_match_the_seal_to_the_millimetre() {
        let directory = TempDir::new().unwrap();
        let setup = valid_setup(&directory);
        let receivers = [
            [100, 500, 200],
            [200, 500, 400],
            [300, 500, 600],
            [400, 500, 800],
        ];

        setup
            .validate_explicit_geometry_mm(
                Some([4020, 2590, 3440]),
                Some([1510, 1190, 390]),
                Some(receivers),
            )
            .unwrap();
        assert!(setup
            .validate_explicit_geometry_mm(Some([4021, 2590, 3440]), None, None)
            .is_err());
        assert!(setup
            .validate_explicit_geometry_mm(None, Some([1511, 1190, 390]), None)
            .is_err());
        let mut changed_receivers = receivers;
        changed_receivers[3][2] += 1;
        assert!(setup
            .validate_explicit_geometry_mm(None, None, Some(changed_receivers))
            .is_err());
    }

    #[test]
    fn expected_grid_and_source_binding_must_match_sealed_setup() {
        let directory = TempDir::new().unwrap();
        let setup = valid_setup(&directory);

        setup.validate_raw_csi_frame(&raw_frame(1, 0)).unwrap();
        setup.validate_raw_csi_frame(&raw_frame(1, 0x10)).unwrap();

        let mut wrong_grid = raw_frame(1, 0);
        wrong_grid.subcarrier_count = 63;
        wrong_grid.iq_pairs.pop();
        setup.validate_raw_csi_source_identity(&wrong_grid).unwrap();
        assert!(!setup
            .raw_csi_frame_matches_expected_grid(&wrong_grid)
            .unwrap());
        assert!(setup.validate_raw_csi_frame(&wrong_grid).is_err());
        assert!(setup
            .validate_raw_csi_source_identity(&raw_frame(5, 0))
            .is_err());
        assert!(setup.validate_raw_csi_frame(&raw_frame(5, 0)).is_err());
    }

    #[test]
    fn sealed_setup_rejects_missing_unsealed_or_wrong_tx_source_binding() {
        let directory = TempDir::new().unwrap();
        let setup = valid_setup(&directory);

        let mut missing = raw_frame(1, 0);
        missing.source_binding = None;
        assert!(setup
            .validate_raw_csi_frame(&missing)
            .unwrap_err()
            .contains("no TX-source binding trailer"));

        let mut unsealed = raw_frame(1, 0);
        unsealed.source_binding = Some(SourceBinding {
            trailer_version: TX_SOURCE_BINDING_VERSION,
            flags: 0,
            scheme: TX_FILTER_SCHEME.to_string(),
            tx_filter_sha256: hash('0'),
        });
        assert!(setup
            .validate_raw_csi_frame(&unsealed)
            .unwrap_err()
            .contains("requires exactly 0x07"));

        let mut partial = raw_frame(1, 0);
        partial.source_binding.as_mut().unwrap().flags = SOURCE_BINDING_FLAG_FILTER_CONFIGURED;
        assert!(setup
            .validate_raw_csi_frame(&partial)
            .unwrap_err()
            .contains("invalid TX-source binding"));

        let mut wrong_scheme = raw_frame(1, 0);
        wrong_scheme.source_binding.as_mut().unwrap().scheme = "sha256-other-v1".to_string();
        assert!(setup
            .validate_raw_csi_frame(&wrong_scheme)
            .unwrap_err()
            .contains("invalid TX-source binding"));

        let mut wrong_hash = raw_frame(1, 0);
        wrong_hash.source_binding.as_mut().unwrap().tx_filter_sha256 = hash('e');
        assert!(setup
            .validate_raw_csi_source_identity(&wrong_hash)
            .unwrap_err()
            .contains("does not match the sealed setup"));
        assert!(setup
            .validate_raw_csi_frame(&wrong_hash)
            .unwrap_err()
            .contains("does not match the sealed setup"));
    }

    #[test]
    fn recording_writer_turns_a_grid_mismatch_into_an_incomplete_result() {
        let directory = TempDir::new().unwrap();
        let setup = valid_setup(&directory);
        let file = std::fs::File::create(directory.path().join("capture.jsonl")).unwrap();
        let mut writer = std::io::BufWriter::new(file);
        let mut result = crate::RecordingWriterResult::default();
        let mut wrong_grid = raw_frame(1, 0);
        wrong_grid.center_frequency_mhz = 2412;

        let error = crate::append_raw_recording_frame(
            &mut writer,
            wrong_grid,
            Some(&setup),
            &None,
            &None,
            &None,
            &mut result,
        )
        .unwrap_err();
        crate::append_recording_writer_error(&mut result, error);

        assert_eq!(result.frames_written, 0);
        assert!(result.incomplete());
        assert!(result
            .error
            .as_deref()
            .is_some_and(|error| error.contains("sealed position setup rejected frame")));
    }

    #[test]
    fn unknown_and_sensitive_fields_are_rejected_by_the_input_schema() {
        for sensitive_field in ["path", "ssid", "ip_address", "password", "tx_filter_mac"] {
            let mut value = valid_spec_value();
            value
                .as_object_mut()
                .unwrap()
                .insert(sensitive_field.to_string(), json!("secret"));
            assert!(
                parse_spec(value).is_err(),
                "{sensitive_field} must be rejected"
            );
        }

        let mut nested_unknown = valid_spec_value();
        nested_unknown["recording_host"]["unknown"] = json!("value");
        assert!(parse_spec(nested_unknown).is_err());
    }

    #[test]
    fn receiver_set_must_be_exactly_rx1_through_rx4_in_order() {
        let directory = TempDir::new().unwrap();
        let executable = executable_fixture(&directory, b"executable");

        let mut unsorted = valid_spec_value();
        unsorted["receivers"].as_array_mut().unwrap().swap(0, 1);
        let error =
            create_position_setup_with_executable(parse_spec(unsorted).unwrap(), &executable)
                .unwrap_err();
        assert!(error.contains("exactly RX1-RX4"));

        let mut missing = valid_spec_value();
        missing["receivers"].as_array_mut().unwrap().pop();
        assert!(
            create_position_setup_with_executable(parse_spec(missing).unwrap(), &executable)
                .unwrap_err()
                .contains("exactly RX1-RX4")
        );
    }

    #[test]
    fn invalid_geometry_grid_hash_and_public_sensitive_value_are_rejected() {
        let directory = TempDir::new().unwrap();
        let executable = executable_fixture(&directory, b"executable");
        let cases = [
            ("geometry", {
                let mut value = valid_spec_value();
                value["receivers"][0]["position_mm"] = json!([4021, 500, 200]);
                value
            }),
            ("grid", {
                let mut value = valid_spec_value();
                value["receivers"][0]["expected_grid"]["center_frequency_mhz"] = json!(2412);
                value
            }),
            ("hash", {
                let mut value = valid_spec_value();
                value["transmitter"]["firmware"]["artifact_sha256"] = json!("ABC");
                value
            }),
            ("raw MAC", {
                let mut value = valid_spec_value();
                value["environment"]["layout_revision"] = json!("aa-bb-cc-dd-ee-ff");
                value
            }),
            ("IP", {
                let mut value = valid_spec_value();
                value["environment"]["layout_revision"] = json!("192.168.1.20");
                value
            }),
        ];
        for (name, value) in cases {
            assert!(
                create_position_setup_with_executable(parse_spec(value).unwrap(), &executable)
                    .is_err(),
                "{name} must be rejected"
            );
        }
    }

    #[test]
    fn sealed_schema_is_strict_and_runtime_binary_is_checked() {
        let directory = TempDir::new().unwrap();
        let executable = executable_fixture(&directory, b"expected executable");
        let other_executable = directory.path().join("other-executable");
        std::fs::write(&other_executable, b"other executable").unwrap();
        let setup = create_position_setup_with_executable(
            parse_spec(valid_spec_value()).unwrap(),
            &executable,
        )
        .unwrap();
        validate_current_server(&setup, &executable).unwrap();
        assert!(validate_current_server(&setup, &other_executable).is_err());

        let mut encoded = serde_json::to_value(setup).unwrap();
        encoded
            .as_object_mut()
            .unwrap()
            .insert("unknown".to_string(), json!(true));
        assert!(serde_json::from_value::<SealedPositionSetup>(encoded).is_err());
    }
}

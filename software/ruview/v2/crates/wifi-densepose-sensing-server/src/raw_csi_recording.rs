//! Lossless, versioned JSONL records for raw ESP32 CSI frames.
//!
//! The live sensing pipeline derives amplitudes and phases from the ESP32 wire
//! packet. Those derived values are not sufficient for later calibration or
//! localization work, so recordings keep every signed I/Q sample together
//! with the complete packet header and the capture context.

use std::path::{Path, PathBuf};
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use wifi_densepose_sensing_server::path_safety::{safe_id, PathSafetyError};

/// Current on-disk schema version.
pub const LEGACY_RAW_CSI_SCHEMA_VERSION: u16 = 1;
pub const RAW_CSI_SCHEMA_VERSION: u16 = 2;

/// Magic value at the start of every supported ESP32 CSI UDP packet.
pub const ESP32_CSI_MAGIC: u32 = 0xC511_0001;

/// Fixed header length of the supported ESP32 CSI UDP packet.
pub const ESP32_CSI_HEADER_LEN: usize = 20;

/// Optional TX-source provenance trailer appended after the exact I/Q payload.
///
/// Wire layout (all multi-byte values little-endian):
/// - `0..4`: magic [`TX_SOURCE_BINDING_MAGIC`]
/// - `4`: version [`TX_SOURCE_BINDING_VERSION`]
/// - `5`: flags
/// - `6..8`: trailer length, always [`TX_SOURCE_BINDING_TRAILER_LEN`]
/// - `8..40`: SHA-256 of the exact six configured TX filter-MAC bytes
pub const TX_SOURCE_BINDING_TRAILER_LEN: usize = 40;
pub const TX_SOURCE_BINDING_MAGIC: u32 = 0xC511_A060;
pub const TX_SOURCE_BINDING_VERSION: u8 = 1;
pub const TX_SOURCE_BINDING_SCHEME: &str = "sha256-ruview-tx-filter-mac-v1";
pub const SOURCE_BINDING_FLAG_FILTER_CONFIGURED: u8 = 0x01;
pub const SOURCE_BINDING_FLAG_SOURCE_MATCHED: u8 = 0x02;
pub const SOURCE_BINDING_FLAG_FILTER_IDENTITY_HASHED: u8 = 0x04;
pub const SOURCE_BINDING_REQUIRED_FLAGS: u8 = SOURCE_BINDING_FLAG_FILTER_CONFIGURED
    | SOURCE_BINDING_FLAG_SOURCE_MATCHED
    | SOURCE_BINDING_FLAG_FILTER_IDENTITY_HASHED;
pub const CSI_IDENTITY_V2_TRAILER_LEN: usize = 24;
pub const CSI_IDENTITY_V3_TRAILER_LEN: usize = 48;
pub const CSI_IDENTITY_V4_TRAILER_LEN: usize = 112;
pub const CSI_PROTOCOL_UNIFIED_RX: u8 = 4;
pub const CSI_IDENTITY_TRAILER_LEN: usize = CSI_IDENTITY_V3_TRAILER_LEN;
pub const CSI_IDENTITY_MAGIC: u32 = 0xC511_A061;
pub const CSI_PROTOCOL_V2: u8 = 2;
pub const CSI_PROTOCOL_VERSION: u8 = 3;
pub const CSI_IDENTITY_ROLE_RX: u8 = 1;
pub const CSI_QUALITY_FIRST_WORD_INVALID: u8 = 1 << 0;
pub const CSI_QUALITY_NOISE_FLOOR_VALID: u8 = 1 << 1;
pub const CSI_QUALITY_RX_STATE_VALID: u8 = 1 << 2;
pub const CSI_QUALITY_GAIN_AVAILABLE: u8 = 1 << 3;
pub const CSI_QUALITY_RAW_PROFILE: u8 = 1 << 4;
pub const CSI_QUALITY_SYNC_VALID: u8 = 1 << 5;
pub const CSI_QUALITY_FLAGS_MASK: u8 = CSI_QUALITY_FIRST_WORD_INVALID
    | CSI_QUALITY_NOISE_FLOOR_VALID
    | CSI_QUALITY_RX_STATE_VALID
    | CSI_QUALITY_GAIN_AVAILABLE
    | CSI_QUALITY_RAW_PROFILE
    | CSI_QUALITY_SYNC_VALID;
const SHA256_BYTE_LEN: usize = 32;
const SHA256_HEX_LEN: usize = SHA256_BYTE_LEN * 2;

/// Filename suffix used for lossless raw CSI JSONL recordings.
pub const RAW_CSI_FILE_SUFFIX: &str = ".raw-csi.v1.jsonl";

/// Mesh-sync freshness is transient transport state, not CSI layout identity.
pub const TRANSIENT_SYNC_FLAG: u8 = 0x10;

/// Optional truth supplied by a controlled measurement run.
///
/// All fields are optional because an unlabelled capture is still useful for
/// diagnosing packet integrity and receiver noise.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroundTruth {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub occupied: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub person_count: Option<u16>,
    /// Known `(x, y, z)` position in metres using the experiment room axes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position_m: Option<[f64; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activity: Option<String>,
}

/// Signed I/Q sample exactly as sent by the ESP32.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IqPair {
    pub i: i8,
    pub q: i8,
}

/// TX-source provenance carried by the optional 40-byte wire trailer.
///
/// This is an additive, optional field in the v1 JSONL schema. Historical v1
/// recordings therefore still decode with `source_binding: None`; an active
/// sealed position setup separately requires a complete binding. The numeric
/// raw-CSI representation and recording filename contract remain unchanged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceBinding {
    pub trailer_version: u8,
    pub flags: u8,
    pub scheme: String,
    pub tx_filter_sha256: String,
}

impl SourceBinding {
    pub fn has_required_flags(&self) -> bool {
        self.flags == SOURCE_BINDING_REQUIRED_FLAGS
    }

    pub fn validate(&self) -> Result<(), RawCsiRecordingError> {
        if self.trailer_version != TX_SOURCE_BINDING_VERSION {
            return Err(RawCsiRecordingError::UnsupportedSourceBindingVersion {
                actual: self.trailer_version,
                expected: TX_SOURCE_BINDING_VERSION,
            });
        }
        if self.scheme != TX_SOURCE_BINDING_SCHEME {
            return Err(RawCsiRecordingError::UnsupportedSourceBindingScheme {
                actual: self.scheme.clone(),
            });
        }
        let digest = decode_sha256_hex(&self.tx_filter_sha256)?;
        match self.flags {
            0 if digest == [0; SHA256_BYTE_LEN] => Ok(()),
            0 => Err(RawCsiRecordingError::InvalidSourceBindingSemantics {
                detail: "unsealed flags require an all-zero digest",
            }),
            SOURCE_BINDING_REQUIRED_FLAGS if digest != [0; SHA256_BYTE_LEN] => Ok(()),
            SOURCE_BINDING_REQUIRED_FLAGS => {
                Err(RawCsiRecordingError::InvalidSourceBindingSemantics {
                    detail: "complete source-binding flags require a non-zero digest",
                })
            }
            flags if flags & !SOURCE_BINDING_REQUIRED_FLAGS != 0 => {
                Err(RawCsiRecordingError::UnknownSourceBindingFlags { actual: flags })
            }
            _ => Err(RawCsiRecordingError::InvalidSourceBindingSemantics {
                detail: "source-binding flags must be either 0x00 or exactly 0x07",
            }),
        }
    }

    fn from_trailer(trailer: &[u8]) -> Result<Self, RawCsiRecordingError> {
        debug_assert_eq!(trailer.len(), TX_SOURCE_BINDING_TRAILER_LEN);
        let magic = u32::from_le_bytes(trailer[0..4].try_into().expect("fixed-size slice"));
        if magic != TX_SOURCE_BINDING_MAGIC {
            return Err(RawCsiRecordingError::WrongSourceBindingMagic { actual: magic });
        }
        let trailer_version = trailer[4];
        if trailer_version != TX_SOURCE_BINDING_VERSION {
            return Err(RawCsiRecordingError::UnsupportedSourceBindingVersion {
                actual: trailer_version,
                expected: TX_SOURCE_BINDING_VERSION,
            });
        }
        let declared_len = u16::from_le_bytes(trailer[6..8].try_into().expect("fixed-size slice"));
        if usize::from(declared_len) != TX_SOURCE_BINDING_TRAILER_LEN {
            return Err(RawCsiRecordingError::InvalidSourceBindingTrailerLength {
                actual: declared_len,
                expected: TX_SOURCE_BINDING_TRAILER_LEN as u16,
            });
        }
        let binding = Self {
            trailer_version,
            flags: trailer[5],
            scheme: TX_SOURCE_BINDING_SCHEME.to_string(),
            tx_filter_sha256: encode_sha256_hex(
                trailer[8..40].try_into().expect("fixed-size slice"),
            ),
        };
        binding.validate()?;
        Ok(binding)
    }

    fn to_trailer(&self) -> Result<[u8; TX_SOURCE_BINDING_TRAILER_LEN], RawCsiRecordingError> {
        self.validate()?;
        let mut trailer = [0_u8; TX_SOURCE_BINDING_TRAILER_LEN];
        trailer[0..4].copy_from_slice(&TX_SOURCE_BINDING_MAGIC.to_le_bytes());
        trailer[4] = self.trailer_version;
        trailer[5] = self.flags;
        trailer[6..8].copy_from_slice(&(TX_SOURCE_BINDING_TRAILER_LEN as u16).to_le_bytes());
        trailer[8..40].copy_from_slice(&decode_sha256_hex(&self.tx_filter_sha256)?);
        Ok(trailer)
    }
}

/// Common RX profile and physical-link identity, present only on protocol v4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnifiedRxIdentity {
    pub generation: u32,
    pub mode: u8,
    /// Zero denotes TX1; 1..4 denote a configured physical peer RX.
    pub source_node_id: u8,
    pub peer_count: u8,
    pub config_sha256: [u8; 32],
    pub dequeued_us: u64,
    pub send_attempt_us: u64,
    pub source_mac: [u8; 6],
    pub receiver_mac: [u8; 6],
    pub sig_mode: u8,
    pub channel_width: u8,
    pub stbc: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RxMeasurementIdentity {
    pub generation: u32,
    pub mode: u8,
    pub source_node_id: u8,
    pub peer_count: u8,
    pub config_sha256: [u8; 32],
    pub source_mac: [u8; 6],
    pub receiver_mac: [u8; 6],
    pub peer_window_open: bool,
}

impl RxMeasurementIdentity {
    pub fn same_receiver_profile(self, other: Self) -> bool {
        self.generation == other.generation && self.mode == other.mode
            && self.peer_count == other.peer_count && self.config_sha256 == other.config_sha256
            && self.receiver_mac == other.receiver_mac && self.peer_window_open == other.peer_window_open
    }
}

#[derive(Debug, Default)]
pub struct RecordingEpochGuard {
    receivers: BTreeMap<u8, (Option<(u8, u64)>, Option<RxMeasurementIdentity>)>,
}

impl RecordingEpochGuard {
    pub fn validate(&self, frame: &RawCsiFrame) -> Result<(), String> {
        if !(1..=4).contains(&frame.rx_id) { return Err("recording accepts exactly RX1-RX4".into()); }
        let current = Self::identity(frame);
        if let Some(previous) = self.receivers.get(&frame.rx_id) {
            let same_profile = match (previous.1, current.1) {
                (Some(a), Some(b)) => a.same_receiver_profile(b), (None, None) => true, _ => false,
            };
            if previous.0 != current.0 || !same_profile {
                return Err(format!("RX{} changed boot/profile during recording", frame.rx_id));
            }
        }
        Ok(())
    }
    fn identity(frame: &RawCsiFrame) -> (Option<(u8, u64)>, Option<RxMeasurementIdentity>) {
        let identity = frame.protocol_identity;
        (identity.map(|i| (i.protocol_version, i.boot_id)), identity.and_then(|i| i.unified_rx)
            .map(|i| i.measurement_identity(identity.and_then(|i| i.callback_started_us).unwrap_or(0))))
    }
    pub fn observe_written(&mut self, frame: &RawCsiFrame) {
        self.receivers.insert(frame.rx_id, Self::identity(frame));
    }
}

impl UnifiedRxIdentity {
    pub fn measurement_identity(self, callback_us: u64) -> RxMeasurementIdentity {
        RxMeasurementIdentity {
            generation: self.generation,
            mode: self.mode,
            source_node_id: self.source_node_id,
            peer_count: self.peer_count,
            config_sha256: self.config_sha256,
            source_mac: self.source_mac,
            receiver_mac: self.receiver_mac,
            peer_window_open: self.peer_count > 0
                && (!matches!(self.mode, 1 | 2) || callback_us < 900_000_000),
        }
    }
    fn valid(self, callback_us: u64) -> bool {
        let peers = match self.mode {
            0 => 0,
            1 | 3 => 1,
            2 | 4 => 3,
            _ => return false,
        };
        let unicast = |mac: [u8; 6]| mac != [0; 6] && mac[0] & 1 == 0;
        self.generation != 0
            && self.peer_count == peers
            && self.source_node_id <= 4
            && (self.source_node_id == 0 || peers > 0)
            && self.config_sha256 != [0; 32]
            && callback_us > 0
            && self.dequeued_us >= callback_us
            && self.send_attempt_us >= self.dequeued_us
            && self.send_attempt_us - callback_us <= 100_000
            && unicast(self.source_mac)
            && unicast(self.receiver_mac)
            && self.source_mac != self.receiver_mac
            && self.sig_mode <= 3
            && self.channel_width <= 1
            && self.stbc <= 3
    }
}

/// Versioned receiver identity carried after the TX-source binding trailer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CsiProtocolIdentity {
    pub protocol_version: u8,
    pub role: u8,
    pub boot_id: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub callback_started_us: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub driver_rx_timestamp_us: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality_flags: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rx_state: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub antenna_index: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gain_db: Option<i8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub csi_config_flags: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sync_age_ms: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unified_rx: Option<UnifiedRxIdentity>,
}

impl CsiProtocolIdentity {
    fn from_trailer(trailer: &[u8]) -> Result<Self, RawCsiRecordingError> {
        if trailer.len() < 8 {
            return Err(RawCsiRecordingError::InvalidCsiIdentityTrailerLength {
                actual: trailer.len().try_into().unwrap_or(u16::MAX),
                expected: CSI_IDENTITY_V2_TRAILER_LEN as u16,
            });
        }
        let magic = u32::from_le_bytes(trailer[0..4].try_into().expect("fixed-size slice"));
        let protocol_version = trailer[4];
        let role = trailer[5];
        let declared_len = u16::from_le_bytes(trailer[6..8].try_into().expect("fixed-size slice"));

        if magic != CSI_IDENTITY_MAGIC {
            return Err(RawCsiRecordingError::WrongCsiIdentityMagic { actual: magic });
        }
        let expected_len = match protocol_version {
            CSI_PROTOCOL_V2 => CSI_IDENTITY_V2_TRAILER_LEN,
            CSI_PROTOCOL_VERSION => CSI_IDENTITY_V3_TRAILER_LEN,
            CSI_PROTOCOL_UNIFIED_RX => CSI_IDENTITY_V4_TRAILER_LEN,
            _ => {
                return Err(RawCsiRecordingError::UnsupportedCsiProtocolVersion {
                    actual: protocol_version,
                });
            }
        };
        if role != CSI_IDENTITY_ROLE_RX {
            return Err(RawCsiRecordingError::UnsupportedCsiIdentityRole { actual: role });
        }
        if usize::from(declared_len) != expected_len || trailer.len() != expected_len {
            return Err(RawCsiRecordingError::InvalidCsiIdentityTrailerLength {
                actual: declared_len,
                expected: expected_len as u16,
            });
        }
        let boot_id = u64::from_le_bytes(trailer[8..16].try_into().expect("fixed-size slice"));
        if boot_id == 0 {
            return Err(RawCsiRecordingError::InvalidCsiIdentitySemantics);
        }
        if protocol_version == CSI_PROTOCOL_V2 {
            if trailer[16..24].iter().any(|byte| *byte != 0) {
                return Err(RawCsiRecordingError::InvalidCsiIdentitySemantics);
            }
            return Ok(Self {
                protocol_version,
                role,
                boot_id,
                callback_started_us: None,
                driver_rx_timestamp_us: None,
                quality_flags: None,
                rx_state: None,
                antenna_index: None,
                gain_db: None,
                csi_config_flags: None,
                sync_age_ms: None,
                unified_rx: None,
            });
        }

        let quality_flags = trailer[28];
        if quality_flags & !CSI_QUALITY_FLAGS_MASK != 0
            || (protocol_version == CSI_PROTOCOL_VERSION
                && trailer[40..48].iter().any(|byte| *byte != 0))
        {
            return Err(RawCsiRecordingError::InvalidCsiIdentitySemantics);
        }
        let encoded_gain = trailer[31] as i8;
        let gain_db = (quality_flags & CSI_QUALITY_GAIN_AVAILABLE != 0).then_some(encoded_gain);
        if gain_db.is_none() && encoded_gain != i8::MIN {
            return Err(RawCsiRecordingError::InvalidCsiIdentitySemantics);
        }
        let unified_rx = if protocol_version == CSI_PROTOCOL_UNIFIED_RX {
            let unified = UnifiedRxIdentity {
                generation: u32::from_le_bytes(trailer[40..44].try_into().unwrap()),
                mode: trailer[44],
                source_node_id: trailer[45],
                peer_count: trailer[46],
                config_sha256: trailer[48..80].try_into().unwrap(),
                dequeued_us: u64::from_le_bytes(trailer[80..88].try_into().unwrap()),
                send_attempt_us: u64::from_le_bytes(trailer[88..96].try_into().unwrap()),
                source_mac: trailer[96..102].try_into().unwrap(),
                receiver_mac: trailer[102..108].try_into().unwrap(),
                sig_mode: trailer[108],
                channel_width: trailer[109],
                stbc: trailer[110],
            };
            let callback_us = u64::from_le_bytes(trailer[16..24].try_into().unwrap());
            if trailer[47] != 0 || trailer[111] != 0 || !unified.valid(callback_us) {
                return Err(RawCsiRecordingError::InvalidCsiIdentitySemantics);
            }
            Some(unified)
        } else {
            None
        };
        Ok(Self {
            protocol_version,
            role,
            boot_id,
            callback_started_us: Some(u64::from_le_bytes(
                trailer[16..24].try_into().expect("fixed-size slice"),
            )),
            driver_rx_timestamp_us: Some(u32::from_le_bytes(
                trailer[24..28].try_into().expect("fixed-size slice"),
            )),
            quality_flags: Some(quality_flags),
            rx_state: Some(trailer[29]),
            antenna_index: Some(trailer[30]),
            gain_db,
            csi_config_flags: Some(u32::from_le_bytes(
                trailer[32..36].try_into().expect("fixed-size slice"),
            )),
            sync_age_ms: Some(u32::from_le_bytes(
                trailer[36..40].try_into().expect("fixed-size slice"),
            )),
            unified_rx,
        })
    }

    fn to_trailer(self) -> Result<Vec<u8>, RawCsiRecordingError> {
        let trailer_len = match self.protocol_version {
            CSI_PROTOCOL_V2 => CSI_IDENTITY_V2_TRAILER_LEN,
            CSI_PROTOCOL_VERSION => CSI_IDENTITY_V3_TRAILER_LEN,
            CSI_PROTOCOL_UNIFIED_RX => CSI_IDENTITY_V4_TRAILER_LEN,
            _ => {
                return Err(RawCsiRecordingError::UnsupportedCsiProtocolVersion {
                    actual: self.protocol_version,
                });
            }
        };
        let mut trailer = vec![0_u8; trailer_len];
        trailer[0..4].copy_from_slice(&CSI_IDENTITY_MAGIC.to_le_bytes());
        trailer[4] = self.protocol_version;
        trailer[5] = self.role;
        trailer[6..8].copy_from_slice(&(trailer_len as u16).to_le_bytes());
        trailer[8..16].copy_from_slice(&self.boot_id.to_le_bytes());
        if self.protocol_version >= CSI_PROTOCOL_VERSION {
            trailer[16..24].copy_from_slice(&self.callback_started_us.unwrap_or(0).to_le_bytes());
            trailer[24..28]
                .copy_from_slice(&self.driver_rx_timestamp_us.unwrap_or(0).to_le_bytes());
            let quality_flags = self.quality_flags.unwrap_or(0);
            trailer[28] = quality_flags;
            trailer[29] = self.rx_state.unwrap_or(0);
            trailer[30] = self.antenna_index.unwrap_or(0xff);
            trailer[31] = self.gain_db.unwrap_or(i8::MIN) as u8;
            trailer[32..36].copy_from_slice(&self.csi_config_flags.unwrap_or(0).to_le_bytes());
            trailer[36..40].copy_from_slice(&self.sync_age_ms.unwrap_or(u32::MAX).to_le_bytes());
        }
        if self.protocol_version == CSI_PROTOCOL_UNIFIED_RX {
            let u = self
                .unified_rx
                .ok_or(RawCsiRecordingError::InvalidCsiIdentitySemantics)?;
            trailer[40..44].copy_from_slice(&u.generation.to_le_bytes());
            trailer[44] = u.mode;
            trailer[45] = u.source_node_id;
            trailer[46] = u.peer_count;
            trailer[48..80].copy_from_slice(&u.config_sha256);
            trailer[80..88].copy_from_slice(&u.dequeued_us.to_le_bytes());
            trailer[88..96].copy_from_slice(&u.send_attempt_us.to_le_bytes());
            trailer[96..102].copy_from_slice(&u.source_mac);
            trailer[102..108].copy_from_slice(&u.receiver_mac);
            trailer[108] = u.sig_mode;
            trailer[109] = u.channel_width;
            trailer[110] = u.stbc;
        } else if self.unified_rx.is_some() {
            return Err(RawCsiRecordingError::InvalidCsiIdentitySemantics);
        }
        Self::from_trailer(&trailer)?;
        Ok(trailer)
    }
}

/// Capture information supplied by the server rather than the ESP32 packet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RawCsiFrameContext {
    /// Wall-clock capture time in Unix nanoseconds.
    pub host_timestamp_unix_ns: u64,
    /// Process-local monotonic receive time. Required by schema v2.
    pub host_monotonic_ns: Option<u64>,
    /// Identifies the process-local monotonic clock origin. Required by v2.
    pub clock_epoch_id: Option<String>,
    /// Optional logical session identifier. If present, it must be path-safe.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Optional measurement label such as `empty`, `still`, or `moving`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ground_truth: Option<GroundTruth>,
    /// Optional mesh-aligned device time in microseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mesh_timestamp_us: Option<u64>,
}

impl Default for RawCsiFrameContext {
    fn default() -> Self {
        let host_time = super::server_clock::now();
        Self {
            host_timestamp_unix_ns: host_time.host_unix_ns,
            host_monotonic_ns: Some(host_time.host_monotonic_ns),
            clock_epoch_id: Some(host_time.clock_epoch_id),
            session_id: None,
            label: None,
            ground_truth: None,
            mesh_timestamp_us: None,
        }
    }
}

/// One lossless raw CSI frame in the version-1 JSONL schema.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawCsiFrame {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub physical_layout: Option<crate::csi_layout::Evidence>,
    pub schema_version: u16,
    pub host_timestamp_unix_ns: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_monotonic_ns: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clock_epoch_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ground_truth: Option<GroundTruth>,
    pub rx_id: u8,
    pub antenna_count: u8,
    pub subcarrier_count: u16,
    pub center_frequency_mhz: u32,
    pub sequence: u32,
    pub rssi_dbm: i8,
    pub noise_floor_dbm: i8,
    /// Exact PPDU byte from offset 18 of the ESP32 packet.
    pub ppdu_type: u8,
    /// Exact flags byte from offset 19 of the ESP32 packet.
    pub flags: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mesh_timestamp_us: Option<u64>,
    /// Optional source provenance parsed from the UDP trailer.
    ///
    /// `None` is the exact legacy wire/JSONL representation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_binding: Option<SourceBinding>,
    /// Versioned receiver boot/timing/quality metadata from the UDP trailer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocol_identity: Option<CsiProtocolIdentity>,
    /// Antenna-major I/Q pairs in the exact order carried by the packet.
    pub iq_pairs: Vec<IqPair>,
}

/// Stable per-RX CSI grid recorded in a capture sidecar and stop response.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawCsiGridIdentity {
    pub center_frequency_mhz: u32,
    pub antenna_count: u8,
    pub subcarrier_count: u16,
    pub ppdu_type: u8,
    pub layout_flags: u8,
}

impl RawCsiGridIdentity {
    pub fn from_frame(frame: &RawCsiFrame) -> Self {
        Self {
            center_frequency_mhz: frame.center_frequency_mhz,
            antenna_count: frame.antenna_count,
            subcarrier_count: frame.subcarrier_count,
            ppdu_type: frame.ppdu_type,
            layout_flags: frame.flags & !(TRANSIENT_SYNC_FLAG | if frame.physical_layout.is_some()
                && frame.protocol_identity.is_some_and(|id|id.protocol_version==CSI_PROTOCOL_UNIFIED_RX) {32} else {0}),
        }
    }
}

/// Durable evidence that one receiver covered the full recording interval.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawCsiRxSummary {
    pub rx_id: u8,
    pub frames_written: u64,
    pub first_host_timestamp_unix_ns: u64,
    pub last_host_timestamp_unix_ns: u64,
    pub grid: RawCsiGridIdentity,
}

impl RawCsiRxSummary {
    pub fn first_written_frame(frame: &RawCsiFrame) -> Self {
        Self {
            rx_id: frame.rx_id,
            frames_written: 1,
            first_host_timestamp_unix_ns: frame.host_timestamp_unix_ns,
            last_host_timestamp_unix_ns: frame.host_timestamp_unix_ns,
            grid: RawCsiGridIdentity::from_frame(frame),
        }
    }

    /// Validate the immutable link identity before a frame is written.
    pub fn validate_next_frame(&self, frame: &RawCsiFrame) -> Result<(), String> {
        if frame.rx_id != self.rx_id {
            return Err(format!(
                "RX summary {} cannot accept a frame from RX{}",
                self.rx_id, frame.rx_id
            ));
        }
        let grid = RawCsiGridIdentity::from_frame(frame);
        if grid != self.grid {
            return Err(format!(
                "RX{} changed CSI grid during recording from {:?} to {:?}",
                self.rx_id, self.grid, grid
            ));
        }
        if frame.host_timestamp_unix_ns < self.last_host_timestamp_unix_ns {
            return Err(format!(
                "RX{} host timestamps moved backwards during recording",
                self.rx_id
            ));
        }
        Ok(())
    }

    /// Update counters only after the corresponding raw line was written.
    pub fn observe_written_frame(&mut self, frame: &RawCsiFrame) {
        debug_assert_eq!(frame.rx_id, self.rx_id);
        self.frames_written = self.frames_written.saturating_add(1);
        self.last_host_timestamp_unix_ns = frame.host_timestamp_unix_ns;
    }
}

/// Source-separated raw evidence. Different observed layouts are retained;
/// these counts do not attest that a physical feature/model accepted them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawCsiLinkSummary {
    pub rx_id: u8,
    pub source_node_id: u8,
    pub generation: Option<u32>,
    pub config_sha256: Option<String>,
    pub source_sha256: Option<String>,
    pub frames_written: u64,
    pub first_host_timestamp_unix_ns: u64,
    pub last_host_timestamp_unix_ns: u64,
    pub grids: Vec<RawCsiGridCount>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawCsiGridCount {
    pub grid: RawCsiGridIdentity,
    pub frames_written: u64,
}

impl RawCsiLinkSummary {
    pub fn key(frame: &RawCsiFrame) -> (u8, u8) {
        (frame.rx_id, frame.protocol_identity.and_then(|i| i.unified_rx).map_or(0, |i| i.source_node_id))
    }
    pub fn first_written(frame: &RawCsiFrame) -> Self {
        Self { rx_id: frame.rx_id, source_node_id: Self::key(frame).1, frames_written: 1,
            generation: frame.protocol_identity.and_then(|i| i.unified_rx).map(|i| i.generation),
            config_sha256: frame.protocol_identity.and_then(|i| i.unified_rx)
                .map(|i| i.config_sha256.iter().map(|v| format!("{v:02x}")).collect()),
            source_sha256: frame.source_binding.as_ref().map(|b| b.tx_filter_sha256.clone()),
            first_host_timestamp_unix_ns: frame.host_timestamp_unix_ns,
            last_host_timestamp_unix_ns: frame.host_timestamp_unix_ns,
            grids: vec![RawCsiGridCount { grid: RawCsiGridIdentity::from_frame(frame), frames_written: 1 }] }
    }
    pub fn validate_next(&self, frame: &RawCsiFrame) -> Result<(), String> {
        if Self::key(frame) != (self.rx_id, self.source_node_id)
            || frame.source_binding.as_ref().map(|b| &b.tx_filter_sha256) != self.source_sha256.as_ref()
            || frame.host_timestamp_unix_ns < self.last_host_timestamp_unix_ns {
            return Err("raw link/source identity or host time changed".into());
        }
        if self.grids.len() == 16 && !self.grids.iter().any(|g| g.grid == RawCsiGridIdentity::from_frame(frame)) {
            return Err("raw link exceeded the bounded 16-layout diagnostic table".into());
        }
        Ok(())
    }
    pub fn observe_written(&mut self, frame: &RawCsiFrame) {
        let grid = RawCsiGridIdentity::from_frame(frame);
        if let Some(count) = self.grids.iter_mut().find(|g| g.grid == grid) { count.frames_written += 1; }
        else { self.grids.push(RawCsiGridCount { grid, frames_written: 1 }); }
        self.frames_written += 1; self.last_host_timestamp_unix_ns = frame.host_timestamp_unix_ns;
    }
}

#[derive(Debug, Error)]
pub enum RawCsiRecordingError {
    #[error("invalid CSI physical evidence: {0}")]
    PhysicalLayout(String),
    #[error("invalid recording ID {id:?}: {source}")]
    InvalidRecordingId {
        id: String,
        #[source]
        source: PathSafetyError,
    },
    #[error("invalid session ID {id:?}: {source}")]
    InvalidSessionId {
        id: String,
        #[source]
        source: PathSafetyError,
    },
    #[error("ESP32 CSI packet is too short: got {actual} bytes, need at least {minimum}")]
    PacketTooShort { actual: usize, minimum: usize },
    #[error("unexpected ESP32 CSI packet magic 0x{actual:08x}")]
    WrongMagic { actual: u32 },
    #[error(
        "invalid ESP32 CSI dimensions: {antenna_count} antenna(s), \
         {subcarrier_count} subcarrier(s)"
    )]
    InvalidDimensions {
        antenna_count: u8,
        subcarrier_count: u16,
    },
    #[error("ESP32 CSI dimensions overflow the supported packet size")]
    DimensionOverflow,
    #[error(
        "ESP32 CSI packet has {actual} bytes; dimensions require exactly \
         {legacy_expected} legacy bytes, {bound_expected} bytes with TX-source binding, \
         {versioned_v2_expected} bytes with protocol-v2 identity, or \
         {versioned_v3_expected} bytes with protocol-v3 timing/quality identity, or \
         {versioned_v4_expected} bytes with unified protocol-v4 identity"
    )]
    PacketLengthMismatch {
        legacy_expected: usize,
        bound_expected: usize,
        versioned_v2_expected: usize,
        versioned_v3_expected: usize,
        versioned_v4_expected: usize,
        actual: usize,
    },
    #[error("unexpected TX-source binding magic 0x{actual:08x}")]
    WrongSourceBindingMagic { actual: u32 },
    #[error("unsupported TX-source binding version {actual}; expected exactly version {expected}")]
    UnsupportedSourceBindingVersion { actual: u8, expected: u8 },
    #[error(
        "TX-source binding trailer declares {actual} bytes; expected exactly {expected} bytes"
    )]
    InvalidSourceBindingTrailerLength { actual: u16, expected: u16 },
    #[error("TX-source binding contains unknown flags 0x{actual:02x}")]
    UnknownSourceBindingFlags { actual: u8 },
    #[error("unsupported TX-source binding scheme {actual:?}")]
    UnsupportedSourceBindingScheme { actual: String },
    #[error("invalid TX-source binding semantics: {detail}")]
    InvalidSourceBindingSemantics { detail: &'static str },
    #[error("unexpected CSI identity magic 0x{actual:08x}")]
    WrongCsiIdentityMagic { actual: u32 },
    #[error("unsupported CSI protocol version {actual}; supported versions are 2, 3 and 4")]
    UnsupportedCsiProtocolVersion { actual: u8 },
    #[error("unsupported CSI identity role {actual}; expected RX role 1")]
    UnsupportedCsiIdentityRole { actual: u8 },
    #[error("CSI identity trailer declares {actual} bytes; expected {expected}")]
    InvalidCsiIdentityTrailerLength { actual: u16, expected: u16 },
    #[error("CSI identity requires a non-zero boot_id and zero reserved bytes")]
    InvalidCsiIdentitySemantics,
    #[error("TX-source binding digest must be exactly 64 lowercase hexadecimal characters")]
    InvalidSourceBindingDigest,
    #[error("unsupported raw CSI schema version {actual}; expected {expected}")]
    UnsupportedSchemaVersion { actual: u16, expected: u16 },
    #[error("raw CSI schema v2 requires host_monotonic_ns and clock_epoch_id")]
    MissingMonotonicTimestamp,
    #[error("raw CSI frame has {actual} I/Q pairs; dimensions require exactly {expected}")]
    IqCountMismatch { expected: usize, actual: usize },
    #[error("ground-truth position contains a non-finite coordinate")]
    NonFiniteGroundTruthPosition,
    #[error("system clock is before the Unix epoch: {0}")]
    SystemClock(#[from] std::time::SystemTimeError),
    #[error("Unix nanosecond timestamp does not fit into u64")]
    HostTimestampOverflow,
    #[error("invalid raw CSI JSON: {0}")]
    Json(#[from] serde_json::Error),
}

fn encode_sha256_hex(digest: &[u8; SHA256_BYTE_LEN]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(SHA256_HEX_LEN);
    for byte in digest {
        encoded.push(HEX[usize::from(byte >> 4)] as char);
        encoded.push(HEX[usize::from(byte & 0x0f)] as char);
    }
    encoded
}

fn decode_sha256_hex(value: &str) -> Result<[u8; SHA256_BYTE_LEN], RawCsiRecordingError> {
    if value.len() != SHA256_HEX_LEN {
        return Err(RawCsiRecordingError::InvalidSourceBindingDigest);
    }
    let mut digest = [0_u8; SHA256_BYTE_LEN];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let high = lowercase_hex_nibble(pair[0])
            .ok_or(RawCsiRecordingError::InvalidSourceBindingDigest)?;
        let low = lowercase_hex_nibble(pair[1])
            .ok_or(RawCsiRecordingError::InvalidSourceBindingDigest)?;
        digest[index] = (high << 4) | low;
    }
    Ok(digest)
}

fn lowercase_hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

impl RawCsiFrame {
    /// Parse and validate an ESP32 CSI UDP packet without discarding samples.
    pub fn from_packet(
        packet: &[u8],
        context: RawCsiFrameContext,
    ) -> Result<Self, RawCsiRecordingError> {
        if packet.len() < ESP32_CSI_HEADER_LEN {
            return Err(RawCsiRecordingError::PacketTooShort {
                actual: packet.len(),
                minimum: ESP32_CSI_HEADER_LEN,
            });
        }

        let magic = u32::from_le_bytes(packet[0..4].try_into().expect("fixed-size slice"));
        if magic != ESP32_CSI_MAGIC {
            return Err(RawCsiRecordingError::WrongMagic { actual: magic });
        }

        let antenna_count = packet[5];
        let subcarrier_count =
            u16::from_le_bytes(packet[6..8].try_into().expect("fixed-size slice"));
        if antenna_count == 0 || subcarrier_count == 0 {
            return Err(RawCsiRecordingError::InvalidDimensions {
                antenna_count,
                subcarrier_count,
            });
        }

        let iq_pair_count = usize::from(antenna_count)
            .checked_mul(usize::from(subcarrier_count))
            .ok_or(RawCsiRecordingError::DimensionOverflow)?;
        let payload_len = iq_pair_count
            .checked_mul(2)
            .ok_or(RawCsiRecordingError::DimensionOverflow)?;
        let legacy_len = ESP32_CSI_HEADER_LEN
            .checked_add(payload_len)
            .ok_or(RawCsiRecordingError::DimensionOverflow)?;
        let bound_len = legacy_len
            .checked_add(TX_SOURCE_BINDING_TRAILER_LEN)
            .ok_or(RawCsiRecordingError::DimensionOverflow)?;
        let versioned_v2_len = bound_len
            .checked_add(CSI_IDENTITY_V2_TRAILER_LEN)
            .ok_or(RawCsiRecordingError::DimensionOverflow)?;
        let versioned_v3_len = bound_len
            .checked_add(CSI_IDENTITY_V3_TRAILER_LEN)
            .ok_or(RawCsiRecordingError::DimensionOverflow)?;
        let versioned_v4_len = bound_len
            .checked_add(CSI_IDENTITY_V4_TRAILER_LEN)
            .ok_or(RawCsiRecordingError::DimensionOverflow)?;
        if packet.len() != legacy_len
            && packet.len() != bound_len
            && packet.len() != versioned_v2_len
            && packet.len() != versioned_v3_len
            && packet.len() != versioned_v4_len
            && ![versioned_v2_len,versioned_v3_len,versioned_v4_len].iter()
                .any(|size|packet.len()==size+crate::csi_layout::WIRE_LEN)
        {
            return Err(RawCsiRecordingError::PacketLengthMismatch {
                legacy_expected: legacy_len,
                bound_expected: bound_len,
                versioned_v2_expected: versioned_v2_len,
                versioned_v3_expected: versioned_v3_len,
                versioned_v4_expected: versioned_v4_len,
                actual: packet.len(),
            });
        }

        let source_binding = (packet.len() >= bound_len)
            .then(|| SourceBinding::from_trailer(&packet[legacy_len..bound_len]))
            .transpose()?;
        let (protocol_identity,physical_layout) = csi_protocol_metadata(packet)?;
        let q_first = protocol_identity
            .as_ref()
            .is_some_and(|identity| identity.protocol_version >= CSI_PROTOCOL_VERSION);
        let iq_pairs = packet[ESP32_CSI_HEADER_LEN..legacy_len]
            .chunks_exact(2)
            .map(|pair| IqPair {
                i: pair[usize::from(q_first)] as i8,
                q: pair[usize::from(!q_first)] as i8,
            })
            .collect();

        let frame = Self {
            schema_version: RAW_CSI_SCHEMA_VERSION,
            host_timestamp_unix_ns: context.host_timestamp_unix_ns,
            host_monotonic_ns: context.host_monotonic_ns,
            clock_epoch_id: context.clock_epoch_id,
            session_id: context.session_id,
            label: context.label,
            ground_truth: context.ground_truth,
            rx_id: packet[4],
            antenna_count,
            subcarrier_count,
            center_frequency_mhz: u32::from_le_bytes(
                packet[8..12].try_into().expect("fixed-size slice"),
            ),
            sequence: u32::from_le_bytes(packet[12..16].try_into().expect("fixed-size slice")),
            rssi_dbm: packet[16] as i8,
            noise_floor_dbm: packet[17] as i8,
            ppdu_type: packet[18],
            flags: packet[19],
            mesh_timestamp_us: context.mesh_timestamp_us,
            source_binding,
            protocol_identity,
            physical_layout,
            iq_pairs,
        };
        frame.validate()?;
        Ok(frame)
    }

    /// Attach the metadata of the active recording to a cloned broadcast frame.
    ///
    /// The UDP ingest path can therefore construct and broadcast a raw frame
    /// without knowing whether a recording is active. The recording writer
    /// clones that frame and enriches only its on-disk copy.
    pub fn with_recording_metadata(
        mut self,
        session_id: Option<String>,
        label: Option<String>,
        ground_truth: Option<GroundTruth>,
    ) -> Result<Self, RawCsiRecordingError> {
        self.session_id = session_id;
        self.label = label;
        self.ground_truth = ground_truth;
        self.validate()?;
        Ok(self)
    }

    /// Validate invariants required by the supported raw CSI schemas.
    pub fn validate(&self) -> Result<(), RawCsiRecordingError> {
        if !matches!(
            self.schema_version,
            LEGACY_RAW_CSI_SCHEMA_VERSION | RAW_CSI_SCHEMA_VERSION
        ) {
            return Err(RawCsiRecordingError::UnsupportedSchemaVersion {
                actual: self.schema_version,
                expected: RAW_CSI_SCHEMA_VERSION,
            });
        }
        if self.schema_version == RAW_CSI_SCHEMA_VERSION
            && (self.host_monotonic_ns.is_none()
                || self.clock_epoch_id.as_deref().is_none_or(str::is_empty))
        {
            return Err(RawCsiRecordingError::MissingMonotonicTimestamp);
        }

        if self.antenna_count == 0 || self.subcarrier_count == 0 {
            return Err(RawCsiRecordingError::InvalidDimensions {
                antenna_count: self.antenna_count,
                subcarrier_count: self.subcarrier_count,
            });
        }

        let expected_iq_pairs = usize::from(self.antenna_count)
            .checked_mul(usize::from(self.subcarrier_count))
            .ok_or(RawCsiRecordingError::DimensionOverflow)?;
        if self.iq_pairs.len() != expected_iq_pairs {
            return Err(RawCsiRecordingError::IqCountMismatch {
                expected: expected_iq_pairs,
                actual: self.iq_pairs.len(),
            });
        }

        if let Some(session_id) = self.session_id.as_deref() {
            safe_id(session_id).map_err(|source| RawCsiRecordingError::InvalidSessionId {
                id: session_id.to_owned(),
                source,
            })?;
        }

        if let Some(position_m) = self
            .ground_truth
            .as_ref()
            .and_then(|ground_truth| ground_truth.position_m)
        {
            if position_m.iter().any(|coordinate| !coordinate.is_finite()) {
                return Err(RawCsiRecordingError::NonFiniteGroundTruthPosition);
            }
        }
        if let Some(binding) = self.source_binding.as_ref() {
            binding.validate()?;
        }
        if self.protocol_identity.is_some() && self.source_binding.is_none() {
            return Err(RawCsiRecordingError::InvalidCsiIdentitySemantics);
        }
        if let Some(identity) = self.protocol_identity {
            identity.to_trailer()?;
            if let Some(unified) = identity.unified_rx {
                let digest: [u8; 32] = Sha256::digest(unified.source_mac).into();
                if !(1..=4).contains(&self.rx_id)
                    || unified.source_node_id == self.rx_id
                    || self.flags & 1 != unified.channel_width
                    || (self.flags & 4 != 0) != (unified.stbc != 0)
                    || self.ppdu_type != if unified.sig_mode == 2 { 0xff } else { 0 }
                    || (self.flags & (1 << 5) != 0)
                        != identity
                            .quality_flags
                            .is_some_and(|flags| flags & CSI_QUALITY_FIRST_WORD_INVALID != 0)
                    || (matches!(unified.mode, 1 | 2)
                        && unified.source_node_id != 0
                        && unified.send_attempt_us >= 900_000_000)
                    || !self.source_binding.as_ref().is_some_and(|binding| {
                        binding.flags == SOURCE_BINDING_REQUIRED_FLAGS
                            && binding.tx_filter_sha256 == encode_sha256_hex(&digest)
                    })
                {
                    return Err(RawCsiRecordingError::InvalidCsiIdentitySemantics);
                }
            }
        }
        if let Some(layout)=self.physical_layout {
            if self.protocol_identity.is_none() {return Err(RawCsiRecordingError::PhysicalLayout("physical evidence needs receiver identity".into()));}
            layout.validate_frame(self).map_err(|error|RawCsiRecordingError::PhysicalLayout(error.into()))?;
            if let Some(unified)=self.protocol_identity.and_then(|identity|identity.unified_rx) {
                if (layout.sig_mode,layout.bandwidth,layout.stbc)!=(unified.sig_mode,unified.channel_width,unified.stbc) {
                    return Err(RawCsiRecordingError::PhysicalLayout("physical and profile metadata contradict".into()));
                }
            }
        }
        Ok(())
    }

    /// Rebuild the exact ESP32 CSI UDP packet represented by this frame.
    ///
    /// Offline replay deliberately feeds this packet back through the live
    /// parser so RSSI normalization, PPDU decoding, grid identity, and I/Q
    /// conversion cannot drift into a second implementation.
    pub fn to_packet(&self) -> Result<Vec<u8>, RawCsiRecordingError> {
        self.validate()?;

        let binding_len = self
            .source_binding
            .as_ref()
            .map(|_| TX_SOURCE_BINDING_TRAILER_LEN)
            .unwrap_or(0);
        let identity_len = self
            .protocol_identity
            .map(|identity| {
                if identity.protocol_version == CSI_PROTOCOL_V2 {
                    CSI_IDENTITY_V2_TRAILER_LEN
                } else if identity.protocol_version == CSI_PROTOCOL_UNIFIED_RX {
                    CSI_IDENTITY_V4_TRAILER_LEN
                } else {
                    CSI_IDENTITY_V3_TRAILER_LEN
                }
            })
            .unwrap_or(0);
        let mut packet = Vec::with_capacity(
            ESP32_CSI_HEADER_LEN
                + self.iq_pairs.len().saturating_mul(2)
                + binding_len
                + identity_len,
        );
        packet.extend_from_slice(&ESP32_CSI_MAGIC.to_le_bytes());
        packet.push(self.rx_id);
        packet.push(self.antenna_count);
        packet.extend_from_slice(&self.subcarrier_count.to_le_bytes());
        packet.extend_from_slice(&self.center_frequency_mhz.to_le_bytes());
        packet.extend_from_slice(&self.sequence.to_le_bytes());
        packet.push(self.rssi_dbm as u8);
        packet.push(self.noise_floor_dbm as u8);
        packet.push(self.ppdu_type);
        packet.push(self.flags);
        let q_first = self
            .protocol_identity
            .is_some_and(|identity| identity.protocol_version >= CSI_PROTOCOL_VERSION);
        for pair in &self.iq_pairs {
            if q_first {
                packet.push(pair.q as u8);
                packet.push(pair.i as u8);
            } else {
                packet.push(pair.i as u8);
                packet.push(pair.q as u8);
            }
        }
        if let Some(binding) = self.source_binding.as_ref() {
            packet.extend_from_slice(&binding.to_trailer()?);
        }
        if let Some(identity) = self.protocol_identity {
            packet.extend_from_slice(&identity.to_trailer()?);
        }
        if let Some(layout)=self.physical_layout {packet.extend_from_slice(&layout.encode());}
        Ok(packet)
    }
}

/// Parse the additive version-2/v3 receiver identity without deriving identity from
/// the UDP source address. Legacy and source-binding-only packets return None.
pub fn csi_protocol_metadata(
    packet: &[u8],
) -> Result<(Option<CsiProtocolIdentity>, Option<crate::csi_layout::Evidence>), RawCsiRecordingError> {
    if packet.len() < ESP32_CSI_HEADER_LEN {
        return Err(RawCsiRecordingError::PacketTooShort {
            actual: packet.len(),
            minimum: ESP32_CSI_HEADER_LEN,
        });
    }
    let antenna_count = packet[5];
    let subcarrier_count = u16::from_le_bytes(packet[6..8].try_into().expect("fixed-size slice"));
    let payload_len = usize::from(antenna_count)
        .checked_mul(usize::from(subcarrier_count))
        .and_then(|pairs| pairs.checked_mul(2))
        .ok_or(RawCsiRecordingError::DimensionOverflow)?;
    let identity_start = ESP32_CSI_HEADER_LEN
        .checked_add(payload_len)
        .and_then(|offset| offset.checked_add(TX_SOURCE_BINDING_TRAILER_LEN))
        .ok_or(RawCsiRecordingError::DimensionOverflow)?;
    if packet.len()<=identity_start {return Ok((None,None));}
    if packet.get(identity_start..identity_start+4)!=Some(CSI_IDENTITY_MAGIC.to_le_bytes().as_slice()) {
        return Ok((None,None));
    }
    let version=*packet.get(identity_start+4).ok_or(RawCsiRecordingError::InvalidCsiIdentitySemantics)?;
    let size=match version {CSI_PROTOCOL_V2=>CSI_IDENTITY_V2_TRAILER_LEN,CSI_PROTOCOL_VERSION=>CSI_IDENTITY_V3_TRAILER_LEN,
        CSI_PROTOCOL_UNIFIED_RX=>CSI_IDENTITY_V4_TRAILER_LEN,_=>return Err(RawCsiRecordingError::UnsupportedCsiProtocolVersion{actual:version})};
    let suffix=&packet[identity_start..];
    if suffix.len()!=size && suffix.len()!=size+crate::csi_layout::WIRE_LEN {
        return Err(RawCsiRecordingError::InvalidCsiIdentitySemantics);
    }
    let identity=CsiProtocolIdentity::from_trailer(&suffix[..size])?;
    let physical=if suffix.len()>size {
        Some(crate::csi_layout::Evidence::decode(&suffix[size..]).map_err(|error|RawCsiRecordingError::PhysicalLayout(error.into()))?)
    } else {None};
    Ok((Some(identity),physical))
}

pub fn csi_protocol_identity(packet:&[u8])->Result<Option<CsiProtocolIdentity>,RawCsiRecordingError> {
    csi_protocol_metadata(packet).map(|metadata|metadata.0)
}

/// Return the current host wall-clock time in Unix nanoseconds.
pub fn now_unix_ns() -> Result<u64, RawCsiRecordingError> {
    let nanoseconds = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    nanoseconds
        .try_into()
        .map_err(|_| RawCsiRecordingError::HostTimestampOverflow)
}

/// Validate a recording ID before it is used in a filename.
pub fn validate_recording_id(id: &str) -> Result<&str, RawCsiRecordingError> {
    safe_id(id).map_err(|source| RawCsiRecordingError::InvalidRecordingId {
        id: id.to_owned(),
        source,
    })
}

/// Build the path for one recording without allowing it to escape `base_dir`.
pub fn recording_path(
    base_dir: &Path,
    recording_id: &str,
) -> Result<PathBuf, RawCsiRecordingError> {
    let recording_id = validate_recording_id(recording_id)?;
    Ok(base_dir.join(format!("{recording_id}{RAW_CSI_FILE_SUFFIX}")))
}

/// Serialize one validated frame, including the terminating JSONL newline.
pub fn encode_json_line(frame: &RawCsiFrame) -> Result<String, RawCsiRecordingError> {
    frame.validate()?;
    let mut line = serde_json::to_string(frame)?;
    line.push('\n');
    Ok(line)
}

/// Deserialize and validate one JSONL frame.
pub fn decode_json_line(line: &str) -> Result<RawCsiFrame, RawCsiRecordingError> {
    let frame: RawCsiFrame = serde_json::from_str(line)?;
    frame.validate()?;
    Ok(frame)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn test_packet() -> Vec<u8> {
        let mut packet = Vec::new();
        packet.extend_from_slice(&ESP32_CSI_MAGIC.to_le_bytes());
        packet.push(3); // RX ID
        packet.push(2); // antennas
        packet.extend_from_slice(&2_u16.to_le_bytes()); // subcarriers
        packet.extend_from_slice(&2_437_u32.to_le_bytes());
        packet.extend_from_slice(&42_u32.to_le_bytes());
        packet.push((-48_i8) as u8);
        packet.push((-92_i8) as u8);
        packet.push(2); // HE-MU PPDU
        packet.push(0b0001_0101);
        packet.extend_from_slice(&[
            (-128_i8) as u8,
            127,
            (-4_i8) as u8,
            5,
            6,
            (-7_i8) as u8,
            0,
            1,
        ]);
        packet
    }

    fn test_digest() -> [u8; SHA256_BYTE_LEN] {
        std::array::from_fn(|index| u8::try_from(index + 1).unwrap())
    }

    fn test_trailer(
        magic: u32,
        version: u8,
        flags: u8,
        declared_len: u16,
        digest: [u8; SHA256_BYTE_LEN],
    ) -> [u8; TX_SOURCE_BINDING_TRAILER_LEN] {
        let mut trailer = [0_u8; TX_SOURCE_BINDING_TRAILER_LEN];
        trailer[0..4].copy_from_slice(&magic.to_le_bytes());
        trailer[4] = version;
        trailer[5] = flags;
        trailer[6..8].copy_from_slice(&declared_len.to_le_bytes());
        trailer[8..40].copy_from_slice(&digest);
        trailer
    }

    fn bound_test_packet() -> Vec<u8> {
        let mut packet = test_packet();
        packet.extend_from_slice(&test_trailer(
            TX_SOURCE_BINDING_MAGIC,
            TX_SOURCE_BINDING_VERSION,
            SOURCE_BINDING_REQUIRED_FLAGS,
            TX_SOURCE_BINDING_TRAILER_LEN as u16,
            test_digest(),
        ));
        packet
    }

    fn versioned_v2_test_packet(boot_id: u64) -> Vec<u8> {
        let mut packet = bound_test_packet();
        packet.extend_from_slice(
            &CsiProtocolIdentity {
                protocol_version: CSI_PROTOCOL_V2,
                role: CSI_IDENTITY_ROLE_RX,
                boot_id,
                callback_started_us: None,
                driver_rx_timestamp_us: None,
                quality_flags: None,
                rx_state: None,
                antenna_index: None,
                gain_db: None,
                csi_config_flags: None,
                sync_age_ms: None,
                unified_rx: None,
            }
            .to_trailer()
            .unwrap(),
        );
        packet
    }

    fn versioned_test_packet(boot_id: u64) -> Vec<u8> {
        let mut packet = bound_test_packet();
        packet.extend_from_slice(
            &CsiProtocolIdentity {
                protocol_version: CSI_PROTOCOL_VERSION,
                role: CSI_IDENTITY_ROLE_RX,
                boot_id,
                callback_started_us: Some(1_234_567),
                driver_rx_timestamp_us: Some(1_234_500),
                quality_flags: Some(CSI_QUALITY_NOISE_FLOOR_VALID | CSI_QUALITY_RX_STATE_VALID),
                rx_state: Some(0),
                antenna_index: Some(1),
                gain_db: None,
                csi_config_flags: Some(0b1011),
                sync_age_ms: Some(u32::MAX),
                unified_rx: None,
            }
            .to_trailer()
            .unwrap(),
        );
        packet
    }

    #[test]
    fn raw_frame_jsonl_roundtrip_is_lossless() {
        let context = RawCsiFrameContext {
            host_timestamp_unix_ns: 1_721_234_567_890_123_456,
            host_monotonic_ns: Some(123_456),
            clock_epoch_id: Some("test-clock".to_string()),
            session_id: Some("d6-empty-001".to_owned()),
            label: Some("empty".to_owned()),
            ground_truth: Some(GroundTruth {
                occupied: Some(false),
                person_count: Some(0),
                position_m: None,
                activity: Some("empty_room".to_owned()),
            }),
            mesh_timestamp_us: Some(987_654_321),
        };

        let frame = RawCsiFrame::from_packet(&test_packet(), context).unwrap();
        let line = encode_json_line(&frame).unwrap();
        let decoded = decode_json_line(&line).unwrap();

        assert!(line.ends_with('\n'));
        assert_eq!(decoded, frame);
        assert_eq!(decoded.rx_id, 3);
        assert_eq!(decoded.center_frequency_mhz, 2_437);
        assert_eq!(decoded.rssi_dbm, -48);
        assert_eq!(decoded.noise_floor_dbm, -92);
        assert_eq!(decoded.ppdu_type, 2);
        assert_eq!(decoded.flags, 0b0001_0101);
        assert!(decoded.source_binding.is_none());
        assert_eq!(
            decoded.iq_pairs,
            vec![
                IqPair { i: -128, q: 127 },
                IqPair { i: -4, q: 5 },
                IqPair { i: 6, q: -7 },
                IqPair { i: 0, q: 1 },
            ]
        );
    }

    #[test]
    fn raw_frame_packet_roundtrip_is_lossless() {
        let packet = test_packet();
        let frame = RawCsiFrame::from_packet(
            &packet,
            RawCsiFrameContext {
                host_timestamp_unix_ns: 123,
                ..Default::default()
            },
        )
        .expect("parse test packet");

        assert_eq!(frame.to_packet().expect("rebuild packet"), packet);
    }

    #[test]
    fn source_bound_packet_and_jsonl_roundtrip_are_lossless() {
        let packet = bound_test_packet();
        let frame = RawCsiFrame::from_packet(
            &packet,
            RawCsiFrameContext {
                host_timestamp_unix_ns: 123,
                ..Default::default()
            },
        )
        .expect("parse bound test packet");
        let binding = frame
            .source_binding
            .as_ref()
            .expect("40-byte trailer must become source_binding");

        assert_eq!(binding.trailer_version, TX_SOURCE_BINDING_VERSION);
        assert_eq!(binding.flags, SOURCE_BINDING_REQUIRED_FLAGS);
        assert_eq!(binding.scheme, TX_SOURCE_BINDING_SCHEME);
        assert_eq!(binding.tx_filter_sha256, encode_sha256_hex(&test_digest()));
        assert_eq!(frame.to_packet().expect("rebuild bound packet"), packet);

        let line = encode_json_line(&frame).expect("encode bound frame");
        assert_eq!(decode_json_line(&line).expect("decode bound frame"), frame);
    }

    #[test]
    fn versioned_packet_preserves_boot_identity_and_existing_binding() {
        let packet = versioned_test_packet(0x0102_0304_0506_0708);
        let frame = RawCsiFrame::from_packet(&packet, RawCsiFrameContext::default())
            .expect("parse versioned packet");
        assert!(frame.source_binding.is_some());
        assert_eq!(
            csi_protocol_identity(&packet).unwrap(),
            Some(CsiProtocolIdentity {
                protocol_version: CSI_PROTOCOL_VERSION,
                role: CSI_IDENTITY_ROLE_RX,
                boot_id: 0x0102_0304_0506_0708,
                callback_started_us: Some(1_234_567),
                driver_rx_timestamp_us: Some(1_234_500),
                quality_flags: Some(CSI_QUALITY_NOISE_FLOOR_VALID | CSI_QUALITY_RX_STATE_VALID,),
                rx_state: Some(0),
                antenna_index: Some(1),
                gain_db: None,
                csi_config_flags: Some(0b1011),
                sync_age_ms: Some(u32::MAX),
                unified_rx: None,
            })
        );
        assert_eq!(
            frame.to_packet().expect("rebuild replay packet"),
            packet,
            "offline replay preserves the complete v3 packet"
        );
        assert_eq!(frame.iq_pairs[0], IqPair { i: 127, q: -128 });
    }

    #[test]
    fn protocol_v2_identity_and_legacy_iq_order_remain_supported() {
        let packet = versioned_v2_test_packet(0x0102_0304_0506_0708);
        let frame = RawCsiFrame::from_packet(&packet, RawCsiFrameContext::default())
            .expect("parse protocol-v2 packet");

        assert_eq!(
            frame.protocol_identity.unwrap().protocol_version,
            CSI_PROTOCOL_V2
        );
        assert_eq!(frame.iq_pairs[0], IqPair { i: -128, q: 127 });
        assert_eq!(frame.to_packet().unwrap(), packet);
    }

    #[test]
    fn packet_length_must_match_dimensions_exactly() {
        let context = RawCsiFrameContext::default();
        let mut truncated = test_packet();
        truncated.pop();
        assert!(matches!(
            RawCsiFrame::from_packet(&truncated, context.clone()),
            Err(RawCsiRecordingError::PacketLengthMismatch { .. })
        ));

        let mut with_trailing_byte = test_packet();
        with_trailing_byte.push(0);
        assert!(matches!(
            RawCsiFrame::from_packet(&with_trailing_byte, context),
            Err(RawCsiRecordingError::PacketLengthMismatch { .. })
        ));

        let mut almost_bound = test_packet();
        almost_bound.resize(almost_bound.len() + TX_SOURCE_BINDING_TRAILER_LEN - 1, 0);
        assert!(matches!(
            RawCsiFrame::from_packet(&almost_bound, RawCsiFrameContext::default()),
            Err(RawCsiRecordingError::PacketLengthMismatch { .. })
        ));

        let mut beyond_bound = bound_test_packet();
        beyond_bound.extend_from_slice(&[0; CSI_IDENTITY_TRAILER_LEN + 1]);
        assert!(matches!(
            RawCsiFrame::from_packet(&beyond_bound, RawCsiFrameContext::default()),
            Err(RawCsiRecordingError::PacketLengthMismatch { .. })
        ));
    }

    #[test]
    fn source_binding_trailer_header_is_strict() {
        let cases = [
            (
                test_trailer(
                    0xC511_A061,
                    TX_SOURCE_BINDING_VERSION,
                    SOURCE_BINDING_REQUIRED_FLAGS,
                    TX_SOURCE_BINDING_TRAILER_LEN as u16,
                    test_digest(),
                ),
                "magic",
            ),
            (
                test_trailer(
                    TX_SOURCE_BINDING_MAGIC,
                    TX_SOURCE_BINDING_VERSION + 1,
                    SOURCE_BINDING_REQUIRED_FLAGS,
                    TX_SOURCE_BINDING_TRAILER_LEN as u16,
                    test_digest(),
                ),
                "version",
            ),
            (
                test_trailer(
                    TX_SOURCE_BINDING_MAGIC,
                    TX_SOURCE_BINDING_VERSION,
                    SOURCE_BINDING_REQUIRED_FLAGS,
                    (TX_SOURCE_BINDING_TRAILER_LEN - 1) as u16,
                    test_digest(),
                ),
                "declared length",
            ),
        ];

        for (trailer, name) in cases {
            let mut packet = test_packet();
            packet.extend_from_slice(&trailer);
            assert!(
                RawCsiFrame::from_packet(&packet, RawCsiFrameContext::default()).is_err(),
                "{name} must be rejected"
            );
        }
    }

    #[test]
    fn source_binding_flags_and_digest_semantics_are_strict() {
        let cases = [
            (0x80, test_digest(), "unknown flag"),
            (
                SOURCE_BINDING_FLAG_FILTER_CONFIGURED,
                test_digest(),
                "partial flags",
            ),
            (0, test_digest(), "unsealed non-zero digest"),
            (
                SOURCE_BINDING_REQUIRED_FLAGS,
                [0; SHA256_BYTE_LEN],
                "sealed zero digest",
            ),
        ];
        for (flags, digest, name) in cases {
            let mut packet = test_packet();
            packet.extend_from_slice(&test_trailer(
                TX_SOURCE_BINDING_MAGIC,
                TX_SOURCE_BINDING_VERSION,
                flags,
                TX_SOURCE_BINDING_TRAILER_LEN as u16,
                digest,
            ));
            assert!(
                RawCsiFrame::from_packet(&packet, RawCsiFrameContext::default()).is_err(),
                "{name} must be rejected"
            );
        }

        let mut unsealed = test_packet();
        unsealed.extend_from_slice(&test_trailer(
            TX_SOURCE_BINDING_MAGIC,
            TX_SOURCE_BINDING_VERSION,
            0,
            TX_SOURCE_BINDING_TRAILER_LEN as u16,
            [0; SHA256_BYTE_LEN],
        ));
        let parsed = RawCsiFrame::from_packet(&unsealed, RawCsiFrameContext::default()).unwrap();
        assert_eq!(parsed.source_binding.unwrap().flags, 0);
    }

    #[test]
    fn recording_id_rejects_traversal_and_unsafe_names() {
        for unsafe_id in [
            "../../etc/passwd",
            "foo/bar",
            "foo\\bar",
            ".hidden",
            "..",
            "contains space",
            "nul\0byte",
        ] {
            assert!(
                validate_recording_id(unsafe_id).is_err(),
                "{unsafe_id:?} must be rejected"
            );
        }

        let base = Path::new("/tmp/recordings");
        assert_eq!(
            recording_path(base, "d6_empty-2026.07.27").unwrap(),
            base.join("d6_empty-2026.07.27.raw-csi.v1.jsonl")
        );
    }

    #[test]
    fn decoded_frames_must_match_current_schema() {
        let frame =
            RawCsiFrame::from_packet(&test_packet(), RawCsiFrameContext::default()).unwrap();
        let mut value = serde_json::to_value(frame).unwrap();
        value["schema_version"] = serde_json::json!(999);

        assert!(matches!(
            decode_json_line(&serde_json::to_string(&value).unwrap()),
            Err(RawCsiRecordingError::UnsupportedSchemaVersion {
                actual: 999,
                expected: RAW_CSI_SCHEMA_VERSION
            })
        ));
    }

    #[test]
    fn historical_v1_json_without_source_binding_still_decodes() {
        let frame =
            RawCsiFrame::from_packet(&test_packet(), RawCsiFrameContext::default()).unwrap();
        let value = serde_json::to_value(&frame).unwrap();

        assert!(value.get("source_binding").is_none());
        assert_eq!(
            decode_json_line(&serde_json::to_string(&value).unwrap()).unwrap(),
            frame
        );
    }

    #[test]
    fn json_source_binding_rejects_unknown_scheme_or_noncanonical_digest() {
        let frame =
            RawCsiFrame::from_packet(&bound_test_packet(), RawCsiFrameContext::default()).unwrap();
        let mut wrong_scheme = serde_json::to_value(&frame).unwrap();
        wrong_scheme["source_binding"]["scheme"] = serde_json::json!("sha256-other-v1");
        assert!(matches!(
            decode_json_line(&serde_json::to_string(&wrong_scheme).unwrap()),
            Err(RawCsiRecordingError::UnsupportedSourceBindingScheme { .. })
        ));

        let mut uppercase_digest = serde_json::to_value(frame).unwrap();
        uppercase_digest["source_binding"]["tx_filter_sha256"] = serde_json::json!("AA".repeat(32));
        assert!(matches!(
            decode_json_line(&serde_json::to_string(&uppercase_digest).unwrap()),
            Err(RawCsiRecordingError::InvalidSourceBindingDigest)
        ));
    }

    #[test]
    fn decoded_frames_reject_hidden_or_future_truth_fields() {
        let frame =
            RawCsiFrame::from_packet(&test_packet(), RawCsiFrameContext::default()).unwrap();
        let mut value = serde_json::to_value(frame).unwrap();
        value["expected_point_id"] = serde_json::json!("P05");

        assert!(matches!(
            decode_json_line(&serde_json::to_string(&value).unwrap()),
            Err(RawCsiRecordingError::Json(_))
        ));
    }

    #[test]
    fn recording_metadata_can_be_attached_after_broadcast() {
        let broadcast_frame =
            RawCsiFrame::from_packet(&test_packet(), RawCsiFrameContext::default()).unwrap();
        let recorded_frame = broadcast_frame
            .clone()
            .with_recording_metadata(
                Some("d6-still-001".to_owned()),
                Some("still".to_owned()),
                Some(GroundTruth {
                    occupied: Some(true),
                    person_count: Some(1),
                    position_m: Some([1.2, 2.3, 1.0]),
                    activity: Some("standing_still".to_owned()),
                }),
            )
            .unwrap();

        assert_eq!(broadcast_frame.session_id, None);
        assert_eq!(recorded_frame.session_id.as_deref(), Some("d6-still-001"));
        assert_eq!(recorded_frame.label.as_deref(), Some("still"));
        assert_eq!(
            recorded_frame
                .ground_truth
                .as_ref()
                .and_then(|truth| truth.position_m),
            Some([1.2, 2.3, 1.0])
        );
    }

    #[test]
    fn per_rx_summary_ignores_only_the_transient_sync_flag() {
        let first = RawCsiFrame::from_packet(
            &test_packet(),
            RawCsiFrameContext {
                host_timestamp_unix_ns: 1_000,
                ..Default::default()
            },
        )
        .unwrap();
        let mut next = first.clone();
        next.host_timestamp_unix_ns = 2_000;
        next.flags &= !TRANSIENT_SYNC_FLAG;

        let mut summary = RawCsiRxSummary::first_written_frame(&first);
        summary.validate_next_frame(&next).unwrap();
        summary.observe_written_frame(&next);

        assert_eq!(summary.frames_written, 2);
        assert_eq!(summary.first_host_timestamp_unix_ns, 1_000);
        assert_eq!(summary.last_host_timestamp_unix_ns, 2_000);
        assert_eq!(
            summary.grid.layout_flags,
            first.flags & !TRANSIENT_SYNC_FLAG
        );
    }

    #[test]
    fn per_rx_summary_rejects_grid_change_or_backward_time() {
        let first = RawCsiFrame::from_packet(
            &test_packet(),
            RawCsiFrameContext {
                host_timestamp_unix_ns: 2_000,
                ..Default::default()
            },
        )
        .unwrap();
        let summary = RawCsiRxSummary::first_written_frame(&first);

        let mut wrong_grid = first.clone();
        wrong_grid.host_timestamp_unix_ns = 3_000;
        wrong_grid.center_frequency_mhz += 5;
        assert!(summary.validate_next_frame(&wrong_grid).is_err());

        let mut backward = first.clone();
        backward.host_timestamp_unix_ns = 1_999;
        assert!(summary.validate_next_frame(&backward).is_err());
    }
    // Emitted by the actual C callback/pool/serializer/sender host test with
    // the existing SHA-256 known-answer MAC vector, not a Rust-built packet.
    pub(crate) fn unified_c_golden() -> Vec<u8> {
        let hex = concat!(
            "010011c5010180008509000000000000d8a40020000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d",
            "1e1f202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f404142434445464748494a4b4c4d4e4f",
            "505152535455565758595a5b5c5d5e5f606162636465666768696a6b6c6d6e6f707172737475767778797a7b7c7d7e7f8081",
            "82838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9fa0a1a2a3a4a5a6a7a8a9aaabacadaeafb0b1b2b3",
            "b4b5b6b7b8b9babbbcbdbebfc0c1c2c3c4c5c6c7c8c9cacbcccdcecfd0d1d2d3d4d5d6d7d8d9dadbdcdddedfe0e1e2e3e4e5",
            "e6e7e8e9eaebecedeeeff0f1f2f3f4f5f6f7f8f9fafbfcfdfeff60a011c50107280048f4634d1002f9f3c7570cb43e00dd86",
            "9b22c79538e9b4adc7e402de1189cfe161a011c504017000080706050403020140420f0000000000d2040000170000800b00",
            "0000ffffffff0700000000000000a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a728460f00",
            "0000000028460f00000000000011223344550200000003010100000063a011c501092000010000000f000101000100000900",
            "040500000000ab2c1b4a",
        );
        (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect()
    }

    #[test]
    fn unified_c_golden_is_lossless_through_record_json_and_replay_packet() {
        let packet = unified_c_golden();
        let frame = RawCsiFrame::from_packet(&packet, RawCsiFrameContext::default()).unwrap();
        let identity = frame.protocol_identity.unwrap();
        assert_eq!(identity.protocol_version, CSI_PROTOCOL_UNIFIED_RX);
        let unified = identity.unified_rx.unwrap();
        assert_eq!(
            (
                unified.generation,
                unified.mode,
                unified.source_node_id,
                unified.peer_count
            ),
            (7, 0, 0, 0)
        );
        assert_eq!(frame.iq_pairs[0], IqPair { i: 1, q: 0 });
        assert_eq!(frame.to_packet().unwrap(), packet);
        let decoded: RawCsiFrame =
            serde_json::from_str(&serde_json::to_string(&frame).unwrap()).unwrap();
        assert_eq!(decoded.to_packet().unwrap(), packet);
        assert!(super::super::protocol::parse_esp32_frame(&packet).is_some());
        assert!(super::super::csi::parse_esp32_frame(&packet).is_some());
    }

    #[test]
    fn unified_rejects_unknown_versions_bad_profile_timing_mac_and_reserved_bytes() {
        let golden = unified_c_golden();
        let start = ESP32_CSI_HEADER_LEN+256+TX_SOURCE_BINDING_TRAILER_LEN;
        for (offset, value) in [
            (4, 5),
            (40, 0),
            (44, 5),
            (46, 1),
            (47, 1),
            (80, 0),
            (96, 3),
            (109, 1),
            (111, 1),
        ] {
            let mut packet = golden.clone();
            if offset == 40 {
                packet[start + 40..start + 44].fill(0);
            } else if offset == 80 {
                packet[start + 80..start + 88].fill(0);
            } else {
                packet[start + offset] = value;
            }
            assert!(
                RawCsiFrame::from_packet(&packet, RawCsiFrameContext::default()).is_err(),
                "offset {offset}"
            );
        }
        let mut unknown = golden;
        unknown[start + 4] = 5;
        assert!(super::super::protocol::parse_esp32_frame(&unknown).is_none());
        assert!(super::super::csi::parse_esp32_frame(&unknown).is_none());
    }

    #[test]
    fn unified_window_identity_separates_generation_and_trial_expiry_not_queue_time() {
        let frame =
            RawCsiFrame::from_packet(&unified_c_golden(), RawCsiFrameContext::default()).unwrap();
        let mut a = frame.protocol_identity.unwrap().unified_rx.unwrap();
        a.mode = 1;
        a.peer_count = 1;
        let key = a.measurement_identity(899_999_999);
        assert_ne!(key, a.measurement_identity(900_000_000));
        a.send_attempt_us += 100;
        a.dequeued_us += 100;
        assert_eq!(key, a.measurement_identity(899_999_999));
        a.generation += 1;
        assert_ne!(key, a.measurement_identity(899_999_999));
    }
    #[test]
    fn future_identity_with_new_length_is_not_silently_legacy() {
        let mut packet=unified_c_golden();
        let start=ESP32_CSI_HEADER_LEN+256+TX_SOURCE_BINDING_TRAILER_LEN;
        packet[start+4]=5; packet.extend_from_slice(&[0;16]);
        packet[start+6..start+8].copy_from_slice(&128u16.to_le_bytes());
        assert!(csi_protocol_identity(&packet).is_err());
        assert!(super::super::protocol::parse_esp32_frame(&packet).is_none());
    }

}

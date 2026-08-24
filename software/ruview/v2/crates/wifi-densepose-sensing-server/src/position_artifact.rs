//! Safe, deterministic infrastructure for offline position artifacts.
//!
//! Artifact identities never contain local I/O paths. Raw-file identity and
//! signal identity are deliberately separate: the former hashes exact on-disk
//! bytes, while the latter hashes only canonical frame timing and wire packets
//! so labels and other recording annotations cannot affect leakage checks.

use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use super::raw_csi_recording::{self, RawCsiFrame};

const SHA256_HEX_LEN: usize = 64;
const HASH_BUFFER_BYTES: usize = 64 * 1024;
const SIGNAL_HASH_DOMAIN: &[u8] = b"ruview.position.feature-signal.v1\0";
const MAX_TEMP_CREATE_ATTEMPTS: usize = 64;

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CaptureSet {
    Training,
    Blind,
}

impl CaptureSet {
    fn as_str(self) -> &'static str {
        match self {
            Self::Training => "training",
            Self::Blind => "blind",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CaptureIdentityField {
    RecordingId,
    RawSha256,
    SignalSha256,
}

impl CaptureIdentityField {
    fn as_str(self) -> &'static str {
        match self {
            Self::RecordingId => "recording_id",
            Self::RawSha256 => "raw_sha256",
            Self::SignalSha256 => "signal_sha256",
        }
    }
}

/// Path-free capture identity suitable for serialized offline artifacts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct CaptureArtifactIdentity {
    pub(crate) recording_id: String,
    pub(crate) raw_sha256: String,
    pub(crate) signal_sha256: String,
}

impl CaptureArtifactIdentity {
    pub(crate) fn new(
        recording_id: impl Into<String>,
        raw_sha256: impl Into<String>,
        signal_sha256: impl Into<String>,
    ) -> Result<Self, PositionArtifactError> {
        let identity = Self {
            recording_id: recording_id.into(),
            raw_sha256: raw_sha256.into(),
            signal_sha256: signal_sha256.into(),
        };
        identity.validate()?;
        Ok(identity)
    }

    pub(crate) fn validate(&self) -> Result<(), PositionArtifactError> {
        raw_csi_recording::validate_recording_id(&self.recording_id).map_err(|source| {
            PositionArtifactError::InvalidRecordingId {
                recording_id: self.recording_id.clone(),
                source,
            }
        })?;
        validate_sha256("raw_sha256", &self.raw_sha256)?;
        validate_sha256("signal_sha256", &self.signal_sha256)?;
        Ok(())
    }
}

#[derive(Debug, Error)]
pub(crate) enum PositionArtifactError {
    #[error("invalid recording ID {recording_id:?}: {source}")]
    InvalidRecordingId {
        recording_id: String,
        #[source]
        source: raw_csi_recording::RawCsiRecordingError,
    },

    #[error("{field} must be exactly 64 lowercase hexadecimal characters")]
    InvalidSha256 { field: &'static str },

    #[error("raw frame {index} is invalid: {source}")]
    InvalidRawFrame {
        index: usize,
        #[source]
        source: raw_csi_recording::RawCsiRecordingError,
    },

    #[error(
        "{set} captures {first_recording_id:?} and {second_recording_id:?} duplicate {field}={value:?}"
    )]
    DuplicateCapture {
        set: &'static str,
        field: &'static str,
        value: String,
        first_recording_id: String,
        second_recording_id: String,
    },

    #[error(
        "training capture {training_recording_id:?} overlaps blind capture {blind_recording_id:?} by {field}={value:?}"
    )]
    CaptureOverlap {
        field: &'static str,
        value: String,
        training_recording_id: String,
        blind_recording_id: String,
    },

    #[error("artifact JSON contains forbidden I/O-path field {json_pointer}")]
    SerializedIoPath { json_pointer: String },

    #[error("artifact JSON serialization failed: {0}")]
    Json(#[from] serde_json::Error),

    #[error("cannot hash file {path}: {source}")]
    FileHashIo {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("artifact target {path} already exists")]
    TargetExists { path: PathBuf },

    #[error("artifact target {path} has no filename")]
    TargetHasNoFilename { path: PathBuf },

    #[error("artifact parent {path} is not a directory")]
    ParentIsNotDirectory { path: PathBuf },

    #[error("could not allocate a private temporary artifact beside {target}")]
    TempNameExhausted { target: PathBuf },

    #[error("artifact {operation} failed for {path}: {source}")]
    ArtifactIo {
        operation: &'static str,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error(
        "artifact was committed to {target}, but its temporary link {temp_path} could not be removed: {source}"
    )]
    TempCleanupAfterCommit {
        target: PathBuf,
        temp_path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error(
        "artifact was committed to {target}, but parent-directory sync failed for {parent}: {source}"
    )]
    ParentSyncAfterCommit {
        target: PathBuf,
        parent: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Lowercase SHA-256 of exact bytes.
pub(crate) fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    digest_hex(hasher.finalize())
}

/// Lowercase SHA-256 of exact file bytes, streamed with bounded memory.
pub(crate) fn sha256_file(path: &Path) -> Result<String, PositionArtifactError> {
    let mut file = File::open(path).map_err(|source| PositionArtifactError::FileHashIo {
        path: path.to_path_buf(),
        source,
    })?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; HASH_BUFFER_BYTES];
    loop {
        let bytes_read =
            file.read(&mut buffer)
                .map_err(|source| PositionArtifactError::FileHashIo {
                    path: path.to_path_buf(),
                    source,
                })?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }
    Ok(digest_hex(hasher.finalize()))
}

/// Canonical identity of the raw information consumed by position features.
///
/// The exact file hash already binds all wire and metadata bytes. This second
/// identity deliberately ignores recording annotations, absolute start time,
/// sequence/noise fields, I/Q phase rotations with identical magnitude, and
/// the transient sync-valid flag. It retains relative frame timing, RX/grid,
/// normalized RSSI, and every CSI magnitude squared. Renaming, renumbering, or
/// shifting a copied capture therefore cannot bypass the train/blind overlap
/// check when its model input is unchanged.
pub(crate) fn signal_sha256(frames: &[RawCsiFrame]) -> Result<String, PositionArtifactError> {
    let first_timestamp = frames
        .iter()
        .map(|frame| frame.host_timestamp_unix_ns)
        .min()
        .unwrap_or(0);
    let mut canonical = Vec::<Vec<u8>>::with_capacity(frames.len());
    for (index, frame) in frames.iter().enumerate() {
        frame
            .validate()
            .map_err(|source| PositionArtifactError::InvalidRawFrame { index, source })?;
        let mut encoded = Vec::with_capacity(20 + frame.iq_pairs.len() * 2);
        encoded.extend_from_slice(
            &frame
                .host_timestamp_unix_ns
                .saturating_sub(first_timestamp)
                .to_le_bytes(),
        );
        encoded.push(frame.rx_id);
        encoded.push(frame.antenna_count);
        encoded.extend_from_slice(&frame.subcarrier_count.to_le_bytes());
        encoded.extend_from_slice(&frame.center_frequency_mhz.to_le_bytes());
        encoded.push(frame.ppdu_type);
        encoded.push(frame.flags & !0x10);
        let normalized_rssi = if frame.rssi_dbm > 0 {
            frame.rssi_dbm.saturating_neg()
        } else {
            frame.rssi_dbm
        };
        encoded.push(normalized_rssi as u8);
        for pair in &frame.iq_pairs {
            let i = i32::from(pair.i);
            let q = i32::from(pair.q);
            let magnitude_squared = (i * i + q * q) as u32;
            encoded.extend_from_slice(&magnitude_squared.to_le_bytes());
        }
        canonical.push(encoded);
    }
    canonical.sort();

    let mut hasher = Sha256::new();
    hasher.update(SIGNAL_HASH_DOMAIN);
    hasher.update((canonical.len() as u64).to_le_bytes());
    for frame in canonical {
        hasher.update((frame.len() as u64).to_le_bytes());
        hasher.update(frame);
    }
    Ok(digest_hex(hasher.finalize()))
}

/// Reject duplicates within either set and overlap between training and blind
/// sets across all three capture identities.
pub(crate) fn check_capture_sets(
    training: &[CaptureArtifactIdentity],
    blind: &[CaptureArtifactIdentity],
) -> Result<(), PositionArtifactError> {
    validate_identity_set(CaptureSet::Training, training)?;
    validate_identity_set(CaptureSet::Blind, blind)?;

    for field in [
        CaptureIdentityField::RecordingId,
        CaptureIdentityField::RawSha256,
        CaptureIdentityField::SignalSha256,
    ] {
        let training_values = identity_map(training, field);
        for blind_capture in blind {
            let value = identity_value(blind_capture, field);
            if let Some(training_recording_id) = training_values.get(value) {
                return Err(PositionArtifactError::CaptureOverlap {
                    field: field.as_str(),
                    value: value.to_string(),
                    training_recording_id: (*training_recording_id).to_string(),
                    blind_recording_id: blind_capture.recording_id.clone(),
                });
            }
        }
    }
    Ok(())
}

/// Canonical deterministic pretty JSON with exactly one final newline.
///
/// Object keys are recursively sorted. Arrays retain their semantic order.
/// Path-shaped field names are rejected so local filesystem topology cannot
/// leak into a durable artifact.
pub(crate) fn deterministic_pretty_json<T: Serialize>(
    value: &T,
) -> Result<Vec<u8>, PositionArtifactError> {
    let value = serde_json::to_value(value)?;
    reject_io_path_fields(&value, "")?;
    let canonical = canonical_json(value);
    let mut encoded = serde_json::to_vec_pretty(&canonical)?;
    while encoded.last() == Some(&b'\n') {
        encoded.pop();
    }
    encoded.push(b'\n');
    Ok(encoded)
}

/// Encode and atomically create a JSON artifact without ever replacing an
/// existing target.
pub(crate) fn write_pretty_json_no_clobber<T: Serialize>(
    target: &Path,
    value: &T,
) -> Result<(), PositionArtifactError> {
    let bytes = deterministic_pretty_json(value)?;
    atomic_write_no_clobber(target, &bytes)
}

/// Atomically create `target` from `bytes` without clobbering.
///
/// The temporary file is created in the same directory with mode `0600` on
/// Unix, fully written and synced, then linked into place. `hard_link` has
/// create-if-absent semantics: if another process wins the target-name race,
/// this call fails without replacing its file. The temporary name is guarded
/// by RAII and removed on every pre-commit error.
pub(crate) fn atomic_write_no_clobber(
    target: &Path,
    bytes: &[u8],
) -> Result<(), PositionArtifactError> {
    atomic_write_no_clobber_impl(target, bytes, |_| Ok(()))
}

fn validate_identity_set(
    set: CaptureSet,
    captures: &[CaptureArtifactIdentity],
) -> Result<(), PositionArtifactError> {
    for capture in captures {
        capture.validate()?;
    }
    for field in [
        CaptureIdentityField::RecordingId,
        CaptureIdentityField::RawSha256,
        CaptureIdentityField::SignalSha256,
    ] {
        let mut seen = BTreeMap::<&str, &str>::new();
        for capture in captures {
            let value = identity_value(capture, field);
            if let Some(first_recording_id) = seen.insert(value, &capture.recording_id) {
                return Err(PositionArtifactError::DuplicateCapture {
                    set: set.as_str(),
                    field: field.as_str(),
                    value: value.to_string(),
                    first_recording_id: first_recording_id.to_string(),
                    second_recording_id: capture.recording_id.clone(),
                });
            }
        }
    }
    Ok(())
}

fn identity_map<'a>(
    captures: &'a [CaptureArtifactIdentity],
    field: CaptureIdentityField,
) -> BTreeMap<&'a str, &'a str> {
    captures
        .iter()
        .map(|capture| {
            (
                identity_value(capture, field),
                capture.recording_id.as_str(),
            )
        })
        .collect()
}

fn identity_value(capture: &CaptureArtifactIdentity, field: CaptureIdentityField) -> &str {
    match field {
        CaptureIdentityField::RecordingId => &capture.recording_id,
        CaptureIdentityField::RawSha256 => &capture.raw_sha256,
        CaptureIdentityField::SignalSha256 => &capture.signal_sha256,
    }
}

fn validate_sha256(field: &'static str, value: &str) -> Result<(), PositionArtifactError> {
    if value.len() != SHA256_HEX_LEN
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(PositionArtifactError::InvalidSha256 { field });
    }
    Ok(())
}

fn digest_hex(digest: impl AsRef<[u8]>) -> String {
    let bytes = digest.as_ref();
    let mut encoded = String::with_capacity(bytes.len() * 2);
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for &byte in bytes {
        encoded.push(HEX[usize::from(byte >> 4)] as char);
        encoded.push(HEX[usize::from(byte & 0x0f)] as char);
    }
    encoded
}

fn reject_io_path_fields(
    value: &serde_json::Value,
    pointer: &str,
) -> Result<(), PositionArtifactError> {
    match value {
        serde_json::Value::Object(object) => {
            for (key, child) in object {
                let child_pointer = format!("{pointer}/{}", escape_json_pointer(key));
                if is_io_path_key(key) {
                    return Err(PositionArtifactError::SerializedIoPath {
                        json_pointer: child_pointer,
                    });
                }
                reject_io_path_fields(child, &child_pointer)?;
            }
        }
        serde_json::Value::Array(values) => {
            for (index, child) in values.iter().enumerate() {
                reject_io_path_fields(child, &format!("{pointer}/{index}"))?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn is_io_path_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "path" | "paths" | "directory" | "directories"
    ) || lower.ends_with("_path")
        || lower.ends_with("_paths")
        || lower.ends_with("_directory")
        || lower.ends_with("_directories")
        || lower.ends_with("_dir")
}

fn escape_json_pointer(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}

fn canonical_json(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(object) => {
            let mut entries: Vec<(String, serde_json::Value)> = object
                .into_iter()
                .map(|(key, value)| (key, canonical_json(value)))
                .collect();
            entries.sort_by(|left, right| left.0.cmp(&right.0));
            let mut canonical = serde_json::Map::new();
            for (key, value) in entries {
                canonical.insert(key, value);
            }
            serde_json::Value::Object(canonical)
        }
        serde_json::Value::Array(values) => {
            serde_json::Value::Array(values.into_iter().map(canonical_json).collect())
        }
        primitive => primitive,
    }
}

fn atomic_write_no_clobber_impl<F>(
    target: &Path,
    bytes: &[u8],
    before_commit: F,
) -> Result<(), PositionArtifactError>
where
    F: FnOnce(&Path) -> Result<(), PositionArtifactError>,
{
    if target.exists() {
        return Err(PositionArtifactError::TargetExists {
            path: target.to_path_buf(),
        });
    }
    let target_name =
        target
            .file_name()
            .ok_or_else(|| PositionArtifactError::TargetHasNoFilename {
                path: target.to_path_buf(),
            })?;
    let parent = artifact_parent(target);
    if !parent.is_dir() {
        return Err(PositionArtifactError::ParentIsNotDirectory {
            path: parent.to_path_buf(),
        });
    }

    let (mut file, mut temp) = create_private_temp(parent, target, target_name)?;
    file.write_all(bytes)
        .map_err(|source| PositionArtifactError::ArtifactIo {
            operation: "temporary write",
            path: temp.path.clone(),
            source,
        })?;
    file.flush()
        .map_err(|source| PositionArtifactError::ArtifactIo {
            operation: "temporary flush",
            path: temp.path.clone(),
            source,
        })?;
    file.sync_all()
        .map_err(|source| PositionArtifactError::ArtifactIo {
            operation: "temporary sync",
            path: temp.path.clone(),
            source,
        })?;
    drop(file);

    before_commit(&temp.path)?;
    match std::fs::hard_link(&temp.path, target) {
        Ok(()) => {}
        Err(source) if source.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err(PositionArtifactError::TargetExists {
                path: target.to_path_buf(),
            });
        }
        Err(source) => {
            return Err(PositionArtifactError::ArtifactIo {
                operation: "atomic no-clobber link",
                path: target.to_path_buf(),
                source,
            });
        }
    }

    if let Err(source) = std::fs::remove_file(&temp.path) {
        return Err(PositionArtifactError::TempCleanupAfterCommit {
            target: target.to_path_buf(),
            temp_path: temp.path.clone(),
            source,
        });
    }
    temp.armed = false;
    sync_parent_after_commit(parent, target)?;
    Ok(())
}

fn artifact_parent(target: &Path) -> &Path {
    target
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

fn create_private_temp(
    parent: &Path,
    target: &Path,
    target_name: &std::ffi::OsStr,
) -> Result<(File, TempFileGuard), PositionArtifactError> {
    for _ in 0..MAX_TEMP_CREATE_ATTEMPTS {
        let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let temp_name = format!(
            ".{}.tmp.{}.{}",
            target_name.to_string_lossy(),
            std::process::id(),
            counter
        );
        let temp_path = parent.join(temp_name);
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&temp_path) {
            Ok(file) => {
                return Ok((
                    file,
                    TempFileGuard {
                        path: temp_path,
                        armed: true,
                    },
                ));
            }
            Err(source) if source.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(source) => {
                return Err(PositionArtifactError::ArtifactIo {
                    operation: "private temporary create",
                    path: temp_path,
                    source,
                });
            }
        }
    }
    Err(PositionArtifactError::TempNameExhausted {
        target: target.to_path_buf(),
    })
}

fn sync_parent_after_commit(parent: &Path, target: &Path) -> Result<(), PositionArtifactError> {
    #[cfg(unix)]
    {
        let directory =
            File::open(parent).map_err(|source| PositionArtifactError::ParentSyncAfterCommit {
                target: target.to_path_buf(),
                parent: parent.to_path_buf(),
                source,
            })?;
        directory
            .sync_all()
            .map_err(|source| PositionArtifactError::ParentSyncAfterCommit {
                target: target.to_path_buf(),
                parent: parent.to_path_buf(),
                source,
            })?;
    }
    #[cfg(not(unix))]
    {
        let _ = (parent, target);
    }
    Ok(())
}

struct TempFileGuard {
    path: PathBuf,
    armed: bool,
}

impl Drop for TempFileGuard {
    fn drop(&mut self) {
        if self.armed {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    use super::super::raw_csi_recording::{GroundTruth, IqPair};

    fn test_frame() -> RawCsiFrame {
        RawCsiFrame {
            schema_version: raw_csi_recording::RAW_CSI_SCHEMA_VERSION,
            host_timestamp_unix_ns: 1_500_000_000,
            host_monotonic_ns: Some(1_500_000_000),
            clock_epoch_id: Some("test-clock".to_string()),
            session_id: Some("capture-a".to_string()),
            label: Some("position-a".to_string()),
            ground_truth: Some(GroundTruth {
                occupied: Some(true),
                person_count: Some(1),
                position_m: Some([1.0, 1.0, 2.0]),
                activity: Some("still".to_string()),
            }),
            rx_id: 1,
            antenna_count: 1,
            subcarrier_count: 4,
            center_frequency_mhz: 2_437,
            sequence: 7,
            rssi_dbm: -48,
            noise_floor_dbm: -92,
            ppdu_type: 0,
            flags: 3,
            mesh_timestamp_us: Some(42),
            source_binding: None,
            iq_pairs: vec![
                IqPair { i: 1, q: 2 },
                IqPair { i: 3, q: 4 },
                IqPair { i: 5, q: 6 },
                IqPair { i: 7, q: 8 },
            ],
        }
    }

    fn digest(seed: u8) -> String {
        format!("{seed:02x}").repeat(32)
    }

    fn identity(recording_id: &str, raw_seed: u8, signal_seed: u8) -> CaptureArtifactIdentity {
        CaptureArtifactIdentity::new(recording_id, digest(raw_seed), digest(signal_seed))
            .expect("test identity")
    }

    fn temp_entries(directory: &Path) -> Vec<String> {
        let mut entries: Vec<String> = std::fs::read_dir(directory)
            .expect("read temp directory")
            .map(|entry| {
                entry
                    .expect("directory entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .filter(|name| name.contains(".tmp."))
            .collect();
        entries.sort();
        entries
    }

    #[test]
    fn signal_hash_ignores_session_label_truth_and_mesh_metadata() {
        let original = test_frame();
        let mut relabeled = original.clone();
        relabeled.session_id = Some("capture-b".to_string());
        relabeled.label = Some("different-label".to_string());
        relabeled.ground_truth = Some(GroundTruth {
            occupied: Some(false),
            person_count: Some(0),
            position_m: None,
            activity: Some("empty".to_string()),
        });
        relabeled.mesh_timestamp_us = Some(9_999);
        relabeled.sequence += 500;
        relabeled.noise_floor_dbm = -80;
        relabeled.flags ^= 0x10;
        for pair in &mut relabeled.iq_pairs {
            (pair.i, pair.q) = (pair.q.saturating_neg(), pair.i);
        }

        assert_eq!(
            signal_sha256(&[original]).expect("original hash"),
            signal_sha256(&[relabeled]).expect("relabeled hash")
        );
    }

    #[test]
    fn signal_hash_changes_with_relative_timing_or_feature_signal() {
        let first = test_frame();
        let mut second = first.clone();
        second.host_timestamp_unix_ns += 100;
        second.sequence += 1;
        let original_hash = signal_sha256(&[first.clone(), second.clone()]).expect("original hash");

        let mut changed_signal = first.clone();
        changed_signal.iq_pairs[0].i += 1;
        assert_ne!(
            original_hash,
            signal_sha256(&[changed_signal, second.clone()]).expect("signal hash")
        );

        let mut changed_time = first;
        changed_time.host_timestamp_unix_ns += 1;
        assert_ne!(
            original_hash,
            signal_sha256(&[changed_time, second]).expect("timestamp hash")
        );
    }

    #[test]
    fn signal_hash_ignores_a_constant_capture_time_shift() {
        let first = test_frame();
        let mut second = first.clone();
        second.host_timestamp_unix_ns += 100;
        second.sequence += 1;
        let mut shifted_first = first.clone();
        shifted_first.host_timestamp_unix_ns += 5_000_000_000;
        let mut shifted_second = second.clone();
        shifted_second.host_timestamp_unix_ns += 5_000_000_000;

        assert_eq!(
            signal_sha256(&[first, second]).expect("original hash"),
            signal_sha256(&[shifted_first, shifted_second]).expect("shifted hash")
        );
    }

    #[test]
    fn signal_hash_accepts_the_full_signed_iq_range() {
        let mut frame = test_frame();
        frame.iq_pairs[0].i = i8::MIN;
        frame.iq_pairs[0].q = i8::MIN;

        let hash = signal_sha256(&[frame]).expect("full-range I/Q hash");
        assert_eq!(hash.len(), SHA256_HEX_LEN);
    }

    #[test]
    fn signal_hash_is_independent_of_input_frame_order() {
        let first = test_frame();
        let mut second = first.clone();
        second.host_timestamp_unix_ns += 100;
        second.sequence += 1;
        assert_eq!(
            signal_sha256(&[first.clone(), second.clone()]).expect("forward hash"),
            signal_sha256(&[second, first]).expect("reverse hash")
        );
    }

    #[test]
    fn duplicate_and_overlap_checks_cover_all_identity_fields() {
        let duplicate_raw = [identity("train-a", 1, 2), identity("train-b", 1, 3)];
        assert!(matches!(
            check_capture_sets(&duplicate_raw, &[]),
            Err(PositionArtifactError::DuplicateCapture {
                field: "raw_sha256",
                ..
            })
        ));

        let duplicate_signal = [identity("train-a", 1, 2), identity("train-b", 3, 2)];
        assert!(matches!(
            check_capture_sets(&duplicate_signal, &[]),
            Err(PositionArtifactError::DuplicateCapture {
                field: "signal_sha256",
                ..
            })
        ));

        let training = [identity("train-a", 1, 2)];
        let blind = [identity("blind-a", 3, 2)];
        assert!(matches!(
            check_capture_sets(&training, &blind),
            Err(PositionArtifactError::CaptureOverlap {
                field: "signal_sha256",
                ..
            })
        ));
    }

    #[test]
    fn pretty_json_is_byte_identical_across_map_insertion_order() {
        let mut first = HashMap::new();
        first.insert("zeta", 1);
        first.insert("alpha", 2);
        let mut second = HashMap::new();
        second.insert("alpha", 2);
        second.insert("zeta", 1);

        let first = deterministic_pretty_json(&first).expect("first JSON");
        let second = deterministic_pretty_json(&second).expect("second JSON");
        assert_eq!(first, second);
        assert_eq!(first.last(), Some(&b'\n'));
        assert_ne!(
            first.get(first.len().saturating_sub(2)),
            Some(&b'\n'),
            "output must have exactly one final newline"
        );
    }

    #[test]
    fn pretty_json_rejects_serialized_io_paths() {
        #[derive(Serialize)]
        struct LeakyArtifact {
            source_path: String,
        }
        let error = deterministic_pretty_json(&LeakyArtifact {
            source_path: "/private/tmp/raw.jsonl".to_string(),
        })
        .expect_err("path field must be rejected");
        assert!(matches!(
            error,
            PositionArtifactError::SerializedIoPath { .. }
        ));
    }

    #[test]
    fn atomic_writer_never_clobbers_an_existing_target() {
        let directory = tempfile::tempdir().expect("tempdir");
        let target = directory.path().join("artifact.json");
        atomic_write_no_clobber(&target, b"first\n").expect("first write");
        let error = atomic_write_no_clobber(&target, b"second\n").expect_err("no clobber");
        assert!(matches!(error, PositionArtifactError::TargetExists { .. }));
        assert_eq!(std::fs::read(&target).expect("target read"), b"first\n");
        assert!(temp_entries(directory.path()).is_empty());

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&target)
                    .expect("target metadata")
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
    }

    #[test]
    fn bare_relative_target_uses_the_current_directory() {
        assert_eq!(artifact_parent(Path::new("index.json")), Path::new("."));
        assert_eq!(
            artifact_parent(Path::new("results/index.json")),
            Path::new("results")
        );
    }

    #[test]
    fn target_race_cleans_the_temporary_file_without_clobbering() {
        let directory = tempfile::tempdir().expect("tempdir");
        let target = directory.path().join("raced.json");
        let error = atomic_write_no_clobber_impl(&target, b"ours\n", |_| {
            std::fs::write(&target, b"winner\n").map_err(|source| {
                PositionArtifactError::ArtifactIo {
                    operation: "test race target create",
                    path: target.clone(),
                    source,
                }
            })
        })
        .expect_err("racing writer must win");
        assert!(matches!(error, PositionArtifactError::TargetExists { .. }));
        assert_eq!(std::fs::read(&target).expect("target read"), b"winner\n");
        assert!(temp_entries(directory.path()).is_empty());
    }
}

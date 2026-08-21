//! Strict offline orchestration for the fixed nine-point position experiment.
//!
//! Training manifests are the only artifact in this module that may contain
//! local file paths. The resulting index and blind-prediction artifact contain
//! only cryptographic capture identities and deterministic model state. Blind
//! prediction never accepts or reads truth.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::position_artifact::{
    check_capture_sets, deterministic_pretty_json, sha256_bytes, sha256_file, signal_sha256,
    CaptureArtifactIdentity,
};
use super::position_capture::{
    build_position_empty_reference, extract_position_feature_blocks, load_position_capture,
    position_source_binding, PositionCapture, PositionCaptureGeometry, PositionEmptyReference,
    PositionFeatureBlock, PositionFeatureExtraction, PositionGridIdentity, POSITION_FEATURE_COUNT,
};
use super::position_evaluation::{
    evaluate, CapturePrediction, CapturePredictionStatus, PositionEvaluationReport,
    PositionPredictionArtifact, PositionTruthManifest,
};
use super::position_fingerprint::{
    FingerprintPosition, PositionFingerprintConfig, PositionFingerprintModel,
    PositionFingerprintPrediction, PositionFingerprintSample, FEATURES_PER_RECEIVER,
    POSITION_COUNT, RECEIVER_COUNT,
};
use super::raw_csi_recording;

const TRAINING_MANIFEST_SCHEMA_VERSION: u16 = 1;
const TRAINING_MANIFEST_KIND: &str = "ruview.position-training-manifest";
const INSPECTION_ARTIFACT_SCHEMA_VERSION: u16 = 1;
const INSPECTION_ARTIFACT_KIND: &str = "ruview.position-capture-inspection";
const INDEX_SCHEMA_VERSION: u16 = 1;
const INDEX_KIND: &str = "ruview.position-index";
const POSITION_ALGORITHM_ID: &str = "d6-nine-point-fingerprint-v1";
const EMPTY_REFERENCE_SCHEMA_VERSION: u16 = 1;
const EMPTY_REFERENCE_ALGORITHM: &str = "d6_empty_projection_v1";
const EXTRACTOR_SCHEMA_VERSION: u16 = 1;
const EXTRACTOR_ALGORITHM: &str = "d6_empty_projection_8band_robust_v1";
const EXPECTED_POSITION_IDS: [&str; POSITION_COUNT] = [
    "P01", "P02", "P03", "P04", "P05", "P06", "P07", "P08", "P09",
];

const SETTLING_NS: u64 = 5_000_000_000;
const WINDOW_NS: u64 = 3_000_000_000;
const WINDOW_STEP_NS: u64 = 1_000_000_000;
const TRAINING_CAPTURE_NS: u64 = 35_000_000_000;
const CALIBRATION_CAPTURE_NS: u64 = 65_000_000_000;
const INDEPENDENT_BLOCK_NS: u64 = 5_000_000_000;
const INDEPENDENT_BLOCK_COUNT: usize = 6;
const WINDOWS_PER_INDEPENDENT_BLOCK: usize = 3;
const COMPLETE_WINDOW_COUNT: usize = 28;
const MINIMUM_ACCEPTED_WINDOWS: usize = 24;
const TEMPORAL_HISTORY_WINDOWS: usize = 5;
const TEMPORAL_AGREEMENT_WINDOWS: usize = 4;
const TEMPORAL_OPPORTUNITY_COUNT: usize = COMPLETE_WINDOW_COUNT - TEMPORAL_HISTORY_WINDOWS + 1;
const MINIMUM_TEMPORAL_COVERAGE_PERCENT: usize = 80;

/// Input-only source descriptor. `path` is resolved relative to the training
/// manifest and is never copied into the durable index.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct TrainingCaptureSource {
    path: PathBuf,
    recording_id: String,
    raw_sha256: String,
    metadata_sha256: String,
    signal_sha256: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct TrainingPointManifest {
    id: String,
    coordinates_m: [f64; 3],
    captures: Vec<TrainingCaptureSource>,
}

/// Typed input manifest for one fixed setup and exactly nine labelled training
/// points. Labels exist only here; raw capture files must remain unlabelled.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct PositionTrainingManifest {
    schema_version: u16,
    kind: String,
    setup_id: String,
    setup_sha256: String,
    geometry: PositionCaptureGeometry,
    calibration: TrainingCaptureSource,
    points: Vec<TrainingPointManifest>,
}

/// Path-free provenance copied into the trained index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PositionCaptureProvenance {
    pub(crate) recording_id: String,
    pub(crate) raw_sha256: String,
    pub(crate) metadata_sha256: String,
    pub(crate) signal_sha256: String,
}

impl PositionCaptureProvenance {
    fn identity(&self) -> Result<CaptureArtifactIdentity, String> {
        validate_sha256("metadata_sha256", &self.metadata_sha256)?;
        CaptureArtifactIdentity::new(
            self.recording_id.clone(),
            self.raw_sha256.clone(),
            self.signal_sha256.clone(),
        )
        .map_err(|error| error.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PositionTrainingProvenance {
    pub(crate) point: FingerprintPosition,
    pub(crate) captures: Vec<PositionTrainingCaptureProvenance>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PositionTrainingCaptureProvenance {
    pub(crate) capture: PositionCaptureProvenance,
    pub(crate) accepted_windows: usize,
    pub(crate) independent_block_samples: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PositionReceiverGrid {
    pub(crate) rx_id: u8,
    pub(crate) grid: PositionGridIdentity,
}

/// The exact feature protocol expected by both training and blind prediction.
///
/// `live_presence_gate_applied` is deliberately false. D6's future live
/// presence gate is a known, explicit gap rather than an implicit claim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PositionFeatureContract {
    pub(crate) extractor_schema_version: u16,
    pub(crate) extractor_algorithm: String,
    pub(crate) settling_ns: u64,
    pub(crate) window_ns: u64,
    pub(crate) window_step_ns: u64,
    pub(crate) feature_count_per_rx: usize,
    pub(crate) receiver_count: usize,
    pub(crate) independent_block_ns: u64,
    pub(crate) windows_per_independent_block: usize,
    pub(crate) receiver_grids: Vec<PositionReceiverGrid>,
    pub(crate) live_presence_gate_applied: bool,
}

/// Protocol span whose signal identity is inspected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PositionInspectionProtocol {
    EmptyCalibration,
    Position,
}

impl PositionInspectionProtocol {
    fn duration_ns(self) -> u64 {
        match self {
            Self::EmptyCalibration => CALIBRATION_CAPTURE_NS,
            Self::Position => TRAINING_CAPTURE_NS,
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::EmptyCalibration => "empty calibration",
            Self::Position => "position",
        }
    }
}

/// Path-free inspection output used to prepare trustworthy training manifests.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PositionCaptureInspectionArtifact {
    pub(crate) schema_version: u16,
    pub(crate) kind: String,
    pub(crate) protocol: PositionInspectionProtocol,
    pub(crate) setup_id: String,
    pub(crate) setup_sha256: String,
    pub(crate) source_binding: raw_csi_recording::SourceBinding,
    pub(crate) server_version: String,
    pub(crate) geometry: PositionCaptureGeometry,
    pub(crate) captures: Vec<PositionCaptureProvenance>,
}

impl PositionCaptureInspectionArtifact {
    fn new(
        protocol: PositionInspectionProtocol,
        setup_id: String,
        setup_sha256: String,
        source_binding: raw_csi_recording::SourceBinding,
        server_version: String,
        geometry: PositionCaptureGeometry,
        mut captures: Vec<PositionCaptureProvenance>,
    ) -> Result<Self, String> {
        captures.sort_by(|left, right| {
            left.recording_id
                .cmp(&right.recording_id)
                .then_with(|| left.raw_sha256.cmp(&right.raw_sha256))
                .then_with(|| left.metadata_sha256.cmp(&right.metadata_sha256))
                .then_with(|| left.signal_sha256.cmp(&right.signal_sha256))
        });
        let artifact = Self {
            schema_version: INSPECTION_ARTIFACT_SCHEMA_VERSION,
            kind: INSPECTION_ARTIFACT_KIND.to_string(),
            protocol,
            setup_id,
            setup_sha256,
            source_binding,
            server_version,
            geometry,
            captures,
        };
        artifact.validate()?;
        Ok(artifact)
    }

    /// Validate a freshly built or deserialized inspection artifact.
    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.schema_version != INSPECTION_ARTIFACT_SCHEMA_VERSION {
            return Err(format!(
                "capture-inspection schema must be {INSPECTION_ARTIFACT_SCHEMA_VERSION}, got {}",
                self.schema_version
            ));
        }
        if self.kind != INSPECTION_ARTIFACT_KIND {
            return Err(format!(
                "capture-inspection kind must be {INSPECTION_ARTIFACT_KIND:?}, got {:?}",
                self.kind
            ));
        }
        raw_csi_recording::validate_recording_id(&self.setup_id)
            .map_err(|error| format!("invalid inspection setup_id: {error}"))?;
        validate_sha256("inspection setup_sha256", &self.setup_sha256)?;
        self.source_binding
            .validate()
            .map_err(|error| format!("invalid inspection TX-source binding: {error}"))?;
        if !self.source_binding.has_required_flags() {
            return Err("inspection TX-source binding is incomplete".to_string());
        }
        validate_nonempty("inspection server_version", &self.server_version)?;
        validate_geometry("capture inspection", &self.geometry)?;
        if self.captures.is_empty() {
            return Err("capture inspection needs at least one capture".to_string());
        }
        for pair in self.captures.windows(2) {
            if pair[0].recording_id >= pair[1].recording_id {
                return Err(
                    "capture-inspection entries must have unique, strictly sorted recording IDs"
                        .to_string(),
                );
            }
        }

        let identities: Vec<CaptureArtifactIdentity> = self
            .captures
            .iter()
            .map(PositionCaptureProvenance::identity)
            .collect::<Result<_, _>>()?;
        check_capture_sets(&identities, &[]).map_err(|error| error.to_string())?;
        reject_duplicate_metadata_hashes(&self.captures)?;
        deterministic_pretty_json(self).map_err(|error| error.to_string())?;
        Ok(())
    }
}

/// Deterministic, path-free position index.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PositionIndexArtifact {
    schema_version: u16,
    kind: String,
    algorithm_id: String,
    setup_id: String,
    setup_sha256: String,
    geometry: PositionCaptureGeometry,
    server_version: String,
    feature_contract: PositionFeatureContract,
    calibration: PositionCaptureProvenance,
    training: Vec<PositionTrainingProvenance>,
    empty_reference: PositionEmptyReference,
    model: PositionFingerprintModel,
    points: Vec<FingerprintPosition>,
}

impl PositionIndexArtifact {
    /// Fail closed before a deserialized index is used for prediction.
    pub(crate) fn validate(&self) -> Result<(), String> {
        validate_index_header(self.schema_version, &self.kind, &self.algorithm_id)?;
        validate_nonempty("setup_id", &self.setup_id)?;
        validate_sha256("setup_sha256", &self.setup_sha256)?;
        validate_geometry("index", &self.geometry)?;
        validate_nonempty("server_version", &self.server_version)?;
        validate_feature_contract(&self.feature_contract)?;

        let calibration_identity = self.calibration.identity()?;
        self.empty_reference.validate()?;
        if self.empty_reference.schema_version != EMPTY_REFERENCE_SCHEMA_VERSION {
            return Err(format!(
                "empty-reference schema must be {EMPTY_REFERENCE_SCHEMA_VERSION}, got {}",
                self.empty_reference.schema_version
            ));
        }
        if self.empty_reference.algorithm != EMPTY_REFERENCE_ALGORITHM {
            return Err(format!(
                "empty-reference algorithm must be {EMPTY_REFERENCE_ALGORITHM:?}, got {:?}",
                self.empty_reference.algorithm
            ));
        }
        if self.empty_reference.calibration_recording_id != self.calibration.recording_id {
            return Err(
                "empty-reference calibration_recording_id does not match calibration provenance"
                    .to_string(),
            );
        }
        if self.empty_reference.setup_id != self.setup_id
            || self.empty_reference.setup_sha256 != self.setup_sha256
            || self.empty_reference.server_version != self.server_version
            || self.empty_reference.geometry != self.geometry
        {
            return Err(
                "empty reference is not bound to the index setup, server version, and geometry"
                    .to_string(),
            );
        }

        if self.points.len() != POSITION_COUNT {
            return Err(format!(
                "index requires exactly {POSITION_COUNT} points, got {}",
                self.points.len()
            ));
        }
        validate_sorted_points(&self.points, &self.geometry)?;
        if self.training.len() != POSITION_COUNT {
            return Err(format!(
                "index requires exactly {POSITION_COUNT} training groups, got {}",
                self.training.len()
            ));
        }

        let mut all_identities = vec![calibration_identity];
        for (index, training) in self.training.iter().enumerate() {
            if training.point != self.points[index] {
                return Err(format!(
                    "training group {} does not match canonical point {}",
                    training.point.id, self.points[index].id
                ));
            }
            if training.captures.is_empty() {
                return Err(format!(
                    "training point {:?} has no capture provenance",
                    training.point.id
                ));
            }
            for pair in training.captures.windows(2) {
                if pair[0].capture.recording_id >= pair[1].capture.recording_id {
                    return Err(format!(
                        "training captures for {:?} are not strictly sorted",
                        training.point.id
                    ));
                }
            }
            for capture in &training.captures {
                if capture.accepted_windows < MINIMUM_ACCEPTED_WINDOWS
                    || capture.accepted_windows > COMPLETE_WINDOW_COUNT
                {
                    return Err(format!(
                        "training capture {:?} has invalid accepted-window count {}",
                        capture.capture.recording_id, capture.accepted_windows
                    ));
                }
                if capture.independent_block_samples != INDEPENDENT_BLOCK_COUNT {
                    return Err(format!(
                        "training capture {:?} must contribute exactly {INDEPENDENT_BLOCK_COUNT} independent block samples",
                        capture.capture.recording_id
                    ));
                }
                all_identities.push(capture.capture.identity()?);
            }
        }
        check_capture_sets(&all_identities, &[]).map_err(|error| error.to_string())?;

        self.model.validate().map_err(|error| error.to_string())?;
        if model_minimum_samples(&self.model)? != INDEPENDENT_BLOCK_COUNT {
            return Err(format!(
                "position model minimum_samples_per_position must be {INDEPENDENT_BLOCK_COUNT}"
            ));
        }
        let model_points: Vec<FingerprintPosition> = self.model.positions().cloned().collect();
        if model_points != self.points {
            return Err("model positions do not match index points".to_string());
        }

        // This also proves that no local path-shaped key leaked into the index.
        deterministic_pretty_json(self).map_err(|error| error.to_string())?;
        Ok(())
    }

    fn training_identities(&self) -> Result<Vec<CaptureArtifactIdentity>, String> {
        let mut identities = vec![self.calibration.identity()?];
        for point in &self.training {
            for capture in &point.captures {
                identities.push(capture.capture.identity()?);
            }
        }
        Ok(identities)
    }

    pub(crate) fn setup_id(&self) -> &str {
        &self.setup_id
    }

    pub(crate) fn setup_sha256(&self) -> &str {
        &self.setup_sha256
    }

    pub(crate) fn server_version(&self) -> &str {
        &self.server_version
    }

    pub(crate) fn geometry(&self) -> &PositionCaptureGeometry {
        &self.geometry
    }

    pub(crate) fn empty_reference(&self) -> &PositionEmptyReference {
        &self.empty_reference
    }

    /// Classify one already quality-gated feature block against this exact
    /// index contract. Live and blind prediction therefore share both grid
    /// validation and the fingerprint model.
    pub(crate) fn predict_feature_block(
        &self,
        block: &PositionFeatureBlock,
    ) -> Result<PositionFingerprintPrediction, String> {
        validate_block_against_grids(block, &self.feature_contract.receiver_grids)?;
        let rx_features = block_feature_matrix(block)?;
        self.model
            .predict(&rx_features)
            .map_err(|error| error.to_string())
    }
}

#[derive(Debug)]
struct LoadedTrainingCapture {
    capture: PositionCapture,
    provenance: PositionCaptureProvenance,
    identity: CaptureArtifactIdentity,
}

#[derive(Debug)]
struct LoadedBlindCapture {
    capture: PositionCapture,
    identity: CaptureArtifactIdentity,
    metadata_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum WindowClassification {
    Matched(String),
    Unknown,
    Ambiguous,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct TemporalConsensus {
    accepted_windows: usize,
    contiguous_opportunities: usize,
    confirmed_by_point: BTreeMap<String, usize>,
    saw_unknown: bool,
    saw_ambiguous: bool,
}

#[derive(Debug)]
struct PositionInspectionContext {
    setup_id: String,
    setup_sha256: String,
    source_binding: raw_csi_recording::SourceBinding,
    server_version: String,
    geometry: PositionCaptureGeometry,
    receiver_grids: Vec<PositionReceiverGrid>,
}

/// Inspect complete raw captures and emit only path-free, manifest-ready
/// identities. Signal hashes cover the canonical protocol prefix, while raw
/// and metadata hashes bind the complete files exactly as recorded.
pub(crate) fn inspect_captures(
    paths: &[PathBuf],
    protocol: PositionInspectionProtocol,
) -> Result<PositionCaptureInspectionArtifact, String> {
    if paths.is_empty() {
        return Err("capture inspection needs at least one input path".to_string());
    }

    let mut context: Option<PositionInspectionContext> = None;
    let mut captures = Vec::with_capacity(paths.len());
    for path in paths {
        let capture = load_position_capture(path)
            .map_err(|error| format!("could not inspect capture {}: {error}", path.display()))?;
        validate_nonempty("capture setup_id", &capture.setup_id)?;
        validate_sha256("capture setup_sha256", &capture.setup_sha256)?;
        let protocol_capture =
            trim_to_protocol_span(&capture, protocol.duration_ns(), protocol.description())?;
        let source_binding = position_source_binding(&protocol_capture)?;
        let receiver_grids = capture_receiver_grids(&protocol_capture)?;

        match &context {
            Some(expected) => {
                validate_inspection_context(expected, &capture, &receiver_grids)?;
            }
            None => {
                context = Some(PositionInspectionContext {
                    setup_id: capture.setup_id.clone(),
                    setup_sha256: capture.setup_sha256.clone(),
                    source_binding,
                    server_version: capture.server_version.clone(),
                    geometry: capture.geometry.clone(),
                    receiver_grids,
                });
            }
        }

        let raw_sha256 = sha256_file(path).map_err(|error| error.to_string())?;
        let metadata_path = sidecar_path(path, &capture.recording_id);
        let metadata_bytes = fs::read(&metadata_path)
            .map_err(|error| format!("could not read {}: {error}", metadata_path.display()))?;
        let metadata_sha256 = sha256_bytes(&metadata_bytes);
        let protocol_signal_sha256 =
            signal_sha256(&protocol_capture.frames).map_err(|error| error.to_string())?;
        captures.push(PositionCaptureProvenance {
            recording_id: capture.recording_id,
            raw_sha256,
            metadata_sha256,
            signal_sha256: protocol_signal_sha256,
        });
    }

    let context = context.expect("non-empty paths installed an inspection context");
    PositionCaptureInspectionArtifact::new(
        protocol,
        context.setup_id,
        context.setup_sha256,
        context.source_binding,
        context.server_version,
        context.geometry,
        captures,
    )
}

/// Build a deterministic nine-point index from one typed training manifest.
pub(crate) fn build_index(training_manifest_path: &Path) -> Result<PositionIndexArtifact, String> {
    let manifest_bytes = fs::read(training_manifest_path).map_err(|error| {
        format!(
            "could not read training manifest {}: {error}",
            training_manifest_path.display()
        )
    })?;
    let manifest: PositionTrainingManifest =
        serde_json::from_slice(&manifest_bytes).map_err(|error| {
            format!(
                "invalid training manifest {}: {error}",
                training_manifest_path.display()
            )
        })?;
    validate_training_manifest(&manifest)?;

    let manifest_dir = training_manifest_path
        .parent()
        .unwrap_or_else(|| Path::new(""));
    let calibration = load_training_source(
        &manifest.calibration,
        manifest_dir,
        CALIBRATION_CAPTURE_NS,
        "calibration",
    )?;
    require_capture_setup(
        &calibration.capture,
        &manifest.setup_id,
        &manifest.setup_sha256,
        &manifest.geometry,
        &calibration.capture.server_version,
        "calibration",
    )?;
    let server_version = calibration.capture.server_version.clone();
    let empty_reference =
        build_position_empty_reference(&calibration.capture, &manifest.setup_sha256)
            .map_err(|error| format!("could not build empty reference: {error}"))?;
    let calibration_grids = capture_receiver_grids(&calibration.capture)?;

    let mut points = manifest.points.clone();
    points.sort_by(|left, right| left.id.cmp(&right.id));

    let mut all_identities = vec![calibration.identity.clone()];
    let mut samples = Vec::new();
    let mut training_provenance = Vec::with_capacity(POSITION_COUNT);
    let mut receiver_grids = Some(calibration_grids);

    for point in points {
        let position = FingerprintPosition {
            id: point.id,
            coordinates_m: point.coordinates_m,
        };
        let mut sources = point.captures;
        sources.sort_by(|left, right| left.recording_id.cmp(&right.recording_id));
        let mut capture_provenance = Vec::with_capacity(sources.len());

        for source in sources {
            let loaded = load_training_source(
                &source,
                manifest_dir,
                TRAINING_CAPTURE_NS,
                &format!("training point {}", position.id),
            )?;
            require_capture_setup(
                &loaded.capture,
                &manifest.setup_id,
                &manifest.setup_sha256,
                &manifest.geometry,
                &server_version,
                &format!("training capture {}", loaded.capture.recording_id),
            )?;
            validate_capture_grids(
                &loaded.capture,
                receiver_grids
                    .as_deref()
                    .expect("calibration grid contract was installed"),
            )?;

            let extraction = extract_position_feature_blocks(&loaded.capture, &empty_reference)
                .map_err(|error| {
                    format!(
                        "could not extract training capture {}: {error}",
                        loaded.capture.recording_id
                    )
                })?;
            validate_extraction_protocol(&extraction)?;
            let accepted_windows = validate_complete_window_accounting(&extraction)?;
            if accepted_windows < MINIMUM_ACCEPTED_WINDOWS {
                return Err(format!(
                    "training capture {:?} has only {accepted_windows} accepted windows; at least {MINIMUM_ACCEPTED_WINDOWS} are required",
                    loaded.capture.recording_id
                ));
            }
            let block_samples = independent_five_second_samples(&extraction, &mut receiver_grids)?;
            for rx_features in block_samples {
                samples.push(PositionFingerprintSample {
                    position: position.clone(),
                    rx_features,
                });
            }

            all_identities.push(loaded.identity);
            capture_provenance.push(PositionTrainingCaptureProvenance {
                capture: loaded.provenance,
                accepted_windows,
                independent_block_samples: INDEPENDENT_BLOCK_COUNT,
            });
        }

        capture_provenance
            .sort_by(|left, right| left.capture.recording_id.cmp(&right.capture.recording_id));
        training_provenance.push(PositionTrainingProvenance {
            point: position,
            captures: capture_provenance,
        });
    }

    check_capture_sets(&all_identities, &[]).map_err(|error| error.to_string())?;
    let model = PositionFingerprintModel::train(
        &samples,
        PositionFingerprintConfig {
            minimum_samples_per_position: INDEPENDENT_BLOCK_COUNT,
        },
    )
    .map_err(|error| error.to_string())?;
    let canonical_points: Vec<FingerprintPosition> = model.positions().cloned().collect();
    let feature_contract = PositionFeatureContract {
        extractor_schema_version: EXTRACTOR_SCHEMA_VERSION,
        extractor_algorithm: EXTRACTOR_ALGORITHM.to_string(),
        settling_ns: SETTLING_NS,
        window_ns: WINDOW_NS,
        window_step_ns: WINDOW_STEP_NS,
        feature_count_per_rx: POSITION_FEATURE_COUNT,
        receiver_count: RECEIVER_COUNT,
        independent_block_ns: INDEPENDENT_BLOCK_NS,
        windows_per_independent_block: WINDOWS_PER_INDEPENDENT_BLOCK,
        receiver_grids: receiver_grids
            .ok_or_else(|| "training produced no receiver-grid contract".to_string())?,
        live_presence_gate_applied: false,
    };

    training_provenance.sort_by(|left, right| left.point.id.cmp(&right.point.id));
    let index = PositionIndexArtifact {
        schema_version: INDEX_SCHEMA_VERSION,
        kind: INDEX_KIND.to_string(),
        algorithm_id: POSITION_ALGORITHM_ID.to_string(),
        setup_id: manifest.setup_id,
        setup_sha256: manifest.setup_sha256,
        geometry: manifest.geometry,
        server_version,
        feature_contract,
        calibration: calibration.provenance,
        training: training_provenance,
        empty_reference,
        model,
        points: canonical_points,
    };
    index.validate()?;
    Ok(index)
}

/// Read exact index bytes, bind their SHA-256 identity, and validate every
/// stored invariant before handing the model to offline or live prediction.
pub(crate) fn load_validated_position_index(
    index_path: &Path,
) -> Result<(PositionIndexArtifact, String), String> {
    let index_bytes = fs::read(index_path)
        .map_err(|error| format!("could not read index {}: {error}", index_path.display()))?;
    let index_sha256 = sha256_bytes(&index_bytes);
    let index: PositionIndexArtifact = serde_json::from_slice(&index_bytes)
        .map_err(|error| format!("invalid position index {}: {error}", index_path.display()))?;
    index.validate()?;
    Ok((index, index_sha256))
}

/// Predict blind, unlabelled captures without opening any truth manifest.
pub(crate) fn predict_blind(
    index_path: &Path,
    capture_paths: &[PathBuf],
) -> Result<PositionPredictionArtifact, String> {
    if capture_paths.is_empty() {
        return Err("blind prediction needs at least one capture".to_string());
    }
    let (index, index_sha256) = load_validated_position_index(index_path)?;

    let mut blind = Vec::with_capacity(capture_paths.len());
    for path in capture_paths {
        let capture = load_position_capture(path)
            .map_err(|error| format!("could not load blind capture {}: {error}", path.display()))?;
        require_capture_setup(
            &capture,
            &index.setup_id,
            &index.setup_sha256,
            &index.geometry,
            &index.server_version,
            &format!("blind capture {}", capture.recording_id),
        )?;
        let protocol_capture =
            trim_to_protocol_span(&capture, TRAINING_CAPTURE_NS, "blind prediction")?;
        validate_capture_grids(&protocol_capture, &index.feature_contract.receiver_grids)?;
        let raw_sha256 = sha256_file(path).map_err(|error| error.to_string())?;
        let metadata_path = sidecar_path(path, &capture.recording_id);
        let metadata_bytes = fs::read(&metadata_path)
            .map_err(|error| format!("could not read {}: {error}", metadata_path.display()))?;
        let metadata_sha256 = sha256_bytes(&metadata_bytes);
        let signal_sha256 =
            signal_sha256(&protocol_capture.frames).map_err(|error| error.to_string())?;
        let identity =
            CaptureArtifactIdentity::new(capture.recording_id.clone(), raw_sha256, signal_sha256)
                .map_err(|error| error.to_string())?;
        blind.push(LoadedBlindCapture {
            capture: protocol_capture,
            identity,
            metadata_sha256,
        });
    }
    blind.sort_by(|left, right| left.capture.recording_id.cmp(&right.capture.recording_id));

    let training_identities = index.training_identities()?;
    let blind_identities: Vec<CaptureArtifactIdentity> =
        blind.iter().map(|loaded| loaded.identity.clone()).collect();
    check_capture_sets(&training_identities, &blind_identities)
        .map_err(|error| error.to_string())?;

    let mut predictions = Vec::with_capacity(blind.len());
    for loaded in blind {
        let extraction = extract_position_feature_blocks(&loaded.capture, &index.empty_reference)
            .map_err(|error| {
            format!(
                "could not extract blind capture {}: {error}",
                loaded.capture.recording_id
            )
        })?;
        validate_extraction_against_contract(&extraction, &index.feature_contract)?;
        validate_complete_window_accounting(&extraction)?;
        let status = predict_capture(&index, &extraction)?;
        predictions.push(CapturePrediction::new(
            loaded.capture.recording_id,
            loaded.identity.raw_sha256,
            loaded.metadata_sha256,
            loaded.identity.signal_sha256,
            status,
        ));
    }

    let artifact = PositionPredictionArtifact::new(
        index.algorithm_id,
        index_sha256,
        index.setup_sha256,
        index.points,
        predictions,
    )
    .map_err(|error| error.to_string())?;
    deterministic_pretty_json(&artifact).map_err(|error| error.to_string())?;
    Ok(artifact)
}

/// Evaluate exact prediction-file bytes against separately supplied truth.
pub(crate) fn evaluate_predictions(
    predictions_path: &Path,
    truth_path: &Path,
) -> Result<PositionEvaluationReport, String> {
    let prediction_bytes = fs::read(predictions_path).map_err(|error| {
        format!(
            "could not read predictions {}: {error}",
            predictions_path.display()
        )
    })?;
    let prediction_sha256 = sha256_bytes(&prediction_bytes);
    let predictions: PositionPredictionArtifact = serde_json::from_slice(&prediction_bytes)
        .map_err(|error| {
            format!(
                "invalid predictions {}: {error}",
                predictions_path.display()
            )
        })?;

    let truth_bytes = fs::read(truth_path)
        .map_err(|error| format!("could not read truth {}: {error}", truth_path.display()))?;
    let truth: PositionTruthManifest = serde_json::from_slice(&truth_bytes)
        .map_err(|error| format!("invalid truth {}: {error}", truth_path.display()))?;
    evaluate(&predictions, &prediction_sha256, &truth).map_err(|error| error.to_string())
}

fn validate_training_manifest(manifest: &PositionTrainingManifest) -> Result<(), String> {
    if manifest.schema_version != TRAINING_MANIFEST_SCHEMA_VERSION {
        return Err(format!(
            "training-manifest schema must be {TRAINING_MANIFEST_SCHEMA_VERSION}, got {}",
            manifest.schema_version
        ));
    }
    if manifest.kind != TRAINING_MANIFEST_KIND {
        return Err(format!(
            "training-manifest kind must be {TRAINING_MANIFEST_KIND:?}, got {:?}",
            manifest.kind
        ));
    }
    validate_nonempty("setup_id", &manifest.setup_id)?;
    validate_sha256("setup_sha256", &manifest.setup_sha256)?;
    validate_geometry("training manifest", &manifest.geometry)?;
    validate_training_source(&manifest.calibration, "calibration")?;
    if manifest.points.len() != POSITION_COUNT {
        return Err(format!(
            "training manifest requires exactly {POSITION_COUNT} points, got {}",
            manifest.points.len()
        ));
    }

    let mut canonical_points = Vec::with_capacity(POSITION_COUNT);
    for point in &manifest.points {
        if point.captures.is_empty() {
            return Err(format!(
                "training point {:?} needs at least one capture",
                point.id
            ));
        }
        for source in &point.captures {
            validate_training_source(source, &format!("training point {}", point.id))?;
        }
        canonical_points.push(FingerprintPosition {
            id: point.id.clone(),
            coordinates_m: point.coordinates_m,
        });
    }
    canonical_points.sort_by(|left, right| left.id.cmp(&right.id));
    validate_sorted_points(&canonical_points, &manifest.geometry)
}

fn validate_training_source(source: &TrainingCaptureSource, context: &str) -> Result<(), String> {
    if source.path.as_os_str().is_empty() {
        return Err(format!("{context} capture path must not be empty"));
    }
    validate_sha256("metadata_sha256", &source.metadata_sha256)?;
    CaptureArtifactIdentity::new(
        source.recording_id.clone(),
        source.raw_sha256.clone(),
        source.signal_sha256.clone(),
    )
    .map_err(|error| format!("{context}: {error}"))?;
    Ok(())
}

fn load_training_source(
    source: &TrainingCaptureSource,
    manifest_dir: &Path,
    protocol_ns: u64,
    context: &str,
) -> Result<LoadedTrainingCapture, String> {
    validate_training_source(source, context)?;
    let raw_path = resolve_source(manifest_dir, &source.path);
    let actual_raw_sha256 = sha256_file(&raw_path).map_err(|error| error.to_string())?;
    if actual_raw_sha256 != source.raw_sha256 {
        return Err(format!(
            "{context} {:?} raw_sha256 does not match exact file bytes",
            source.recording_id
        ));
    }
    let metadata_path = sidecar_path(&raw_path, &source.recording_id);
    let metadata_bytes = fs::read(&metadata_path)
        .map_err(|error| format!("could not read {}: {error}", metadata_path.display()))?;
    let actual_metadata_sha256 = sha256_bytes(&metadata_bytes);
    if actual_metadata_sha256 != source.metadata_sha256 {
        return Err(format!(
            "{context} {:?} metadata_sha256 does not match exact sidecar bytes",
            source.recording_id
        ));
    }

    let capture = load_position_capture(&raw_path)
        .map_err(|error| format!("{context} {:?}: {error}", source.recording_id))?;
    if capture.recording_id != source.recording_id {
        return Err(format!(
            "{context} expected recording_id {:?}, loaded {:?}",
            source.recording_id, capture.recording_id
        ));
    }
    let protocol_capture = trim_to_protocol_span(&capture, protocol_ns, context)?;
    let actual_signal_sha256 =
        signal_sha256(&protocol_capture.frames).map_err(|error| error.to_string())?;
    if actual_signal_sha256 != source.signal_sha256 {
        return Err(format!(
            "{context} {:?} signal_sha256 does not match canonical CSI signal",
            source.recording_id
        ));
    }

    let provenance = PositionCaptureProvenance {
        recording_id: source.recording_id.clone(),
        raw_sha256: actual_raw_sha256.clone(),
        metadata_sha256: actual_metadata_sha256,
        signal_sha256: actual_signal_sha256.clone(),
    };
    let identity = CaptureArtifactIdentity::new(
        source.recording_id.clone(),
        actual_raw_sha256,
        actual_signal_sha256,
    )
    .map_err(|error| error.to_string())?;
    Ok(LoadedTrainingCapture {
        capture: protocol_capture,
        provenance,
        identity,
    })
}

fn resolve_source(manifest_dir: &Path, source: &Path) -> PathBuf {
    if source.is_absolute() {
        source.to_path_buf()
    } else {
        manifest_dir.join(source)
    }
}

fn sidecar_path(raw_path: &Path, recording_id: &str) -> PathBuf {
    raw_path
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .join(format!("{recording_id}.raw-csi.v1.meta.json"))
}

fn require_capture_setup(
    capture: &PositionCapture,
    setup_id: &str,
    setup_sha256: &str,
    geometry: &PositionCaptureGeometry,
    server_version: &str,
    context: &str,
) -> Result<(), String> {
    if capture.setup_id != setup_id || capture.setup_sha256 != setup_sha256 {
        return Err(format!(
            "{context} setup identity does not match the experiment manifest/index"
        ));
    }
    if &capture.geometry != geometry {
        return Err(format!(
            "{context} geometry does not exactly match the fixed setup"
        ));
    }
    if capture.server_version != server_version {
        return Err(format!(
            "{context} server version {:?} does not match {:?}",
            capture.server_version, server_version
        ));
    }
    Ok(())
}

fn validate_inspection_context(
    expected: &PositionInspectionContext,
    capture: &PositionCapture,
    receiver_grids: &[PositionReceiverGrid],
) -> Result<(), String> {
    if capture.setup_id != expected.setup_id
        || capture.setup_sha256 != expected.setup_sha256
        || position_source_binding(capture)? != expected.source_binding
        || capture.server_version != expected.server_version
        || capture.geometry != expected.geometry
    {
        return Err(format!(
            "{} does not share the inspection setup, TX source, server version, and geometry",
            capture.recording_id
        ));
    }
    if receiver_grids != expected.receiver_grids {
        return Err(format!(
            "{} CSI grids do not match the first inspected capture",
            capture.recording_id
        ));
    }
    Ok(())
}

fn trim_to_protocol_span(
    capture: &PositionCapture,
    protocol_ns: u64,
    protocol_name: &str,
) -> Result<PositionCapture, String> {
    let protocol_end = capture
        .started_at_unix_ns
        .checked_add(protocol_ns)
        .ok_or_else(|| format!("{} protocol end overflowed", capture.recording_id))?;
    if capture.ended_at_unix_ns < protocol_end {
        let actual_duration = capture
            .ended_at_unix_ns
            .saturating_sub(capture.started_at_unix_ns);
        return Err(format!(
            "{} is too short for the {protocol_name} protocol: needs at least {protocol_ns} ns, got {actual_duration} ns",
            capture.recording_id
        ));
    }
    let mut protocol_capture = capture.clone();
    protocol_capture.ended_at_unix_ns = protocol_end;
    protocol_capture
        .frames
        .retain(|frame| frame.host_timestamp_unix_ns < protocol_end);
    Ok(protocol_capture)
}

fn reject_duplicate_metadata_hashes(captures: &[PositionCaptureProvenance]) -> Result<(), String> {
    let mut seen = BTreeMap::<&str, &str>::new();
    for capture in captures {
        if let Some(first_recording_id) =
            seen.insert(&capture.metadata_sha256, &capture.recording_id)
        {
            return Err(format!(
                "captures {first_recording_id:?} and {:?} duplicate metadata_sha256={:?}",
                capture.recording_id, capture.metadata_sha256
            ));
        }
    }
    Ok(())
}

fn capture_receiver_grids(capture: &PositionCapture) -> Result<Vec<PositionReceiverGrid>, String> {
    let mut grids_by_rx = BTreeMap::<u8, BTreeSet<PositionGridIdentity>>::new();
    for frame in &capture.frames {
        grids_by_rx
            .entry(frame.rx_id)
            .or_default()
            .insert(PositionGridIdentity::from_frame(frame));
    }
    let mut grids = Vec::with_capacity(RECEIVER_COUNT);
    for rx_id in 1u8..=4 {
        let distinct = grids_by_rx
            .get(&rx_id)
            .ok_or_else(|| format!("{} has no frames for RX{rx_id}", capture.recording_id))?;
        if distinct.len() != 1 {
            return Err(format!(
                "{} RX{rx_id} uses {} CSI grids; exactly one is required",
                capture.recording_id,
                distinct.len()
            ));
        }
        grids.push(PositionReceiverGrid {
            rx_id,
            grid: *distinct
                .first()
                .expect("a non-empty one-element grid set was checked"),
        });
    }
    if grids_by_rx.len() != RECEIVER_COUNT {
        return Err(format!(
            "{} contains an unexpected receiver ID",
            capture.recording_id
        ));
    }
    Ok(grids)
}

fn validate_capture_grids(
    capture: &PositionCapture,
    expected: &[PositionReceiverGrid],
) -> Result<(), String> {
    let actual = capture_receiver_grids(capture)?;
    if actual != expected {
        return Err(format!(
            "{} CSI grids do not exactly match the calibration/index",
            capture.recording_id
        ));
    }
    Ok(())
}

fn validate_extraction_protocol(extraction: &PositionFeatureExtraction) -> Result<(), String> {
    if extraction.schema_version != EXTRACTOR_SCHEMA_VERSION
        || extraction.algorithm != EXTRACTOR_ALGORITHM
        || extraction.settling_ns != SETTLING_NS
        || extraction.window_ns != WINDOW_NS
        || extraction.window_step_ns != WINDOW_STEP_NS
        || extraction.feature_count_per_rx != POSITION_FEATURE_COUNT
    {
        return Err(format!(
            "capture {:?} uses an incompatible position-feature protocol",
            extraction.recording_id
        ));
    }
    let duration = extraction
        .ended_at_unix_ns
        .checked_sub(extraction.started_at_unix_ns)
        .ok_or_else(|| format!("{} has invalid extraction bounds", extraction.recording_id))?;
    if duration != TRAINING_CAPTURE_NS {
        return Err(format!(
            "{} extraction must cover exactly {TRAINING_CAPTURE_NS} ns",
            extraction.recording_id
        ));
    }
    Ok(())
}

fn validate_extraction_against_contract(
    extraction: &PositionFeatureExtraction,
    contract: &PositionFeatureContract,
) -> Result<(), String> {
    validate_extraction_protocol(extraction)?;
    if extraction.schema_version != contract.extractor_schema_version
        || extraction.algorithm != contract.extractor_algorithm
        || extraction.settling_ns != contract.settling_ns
        || extraction.window_ns != contract.window_ns
        || extraction.window_step_ns != contract.window_step_ns
        || extraction.feature_count_per_rx != contract.feature_count_per_rx
    {
        return Err(format!(
            "capture {:?} does not match the index feature contract",
            extraction.recording_id
        ));
    }
    for block in &extraction.accepted_blocks {
        validate_block_against_grids(block, &contract.receiver_grids)?;
    }
    Ok(())
}

fn validate_complete_window_accounting(
    extraction: &PositionFeatureExtraction,
) -> Result<usize, String> {
    let mut starts = BTreeSet::new();
    for block in &extraction.accepted_blocks {
        if !starts.insert(block.window_start_unix_ns) {
            return Err(format!(
                "{} repeats feature window {}",
                extraction.recording_id, block.window_start_unix_ns
            ));
        }
    }
    let mut full_rejections = 0usize;
    for rejected in &extraction.rejected_windows {
        if !starts.insert(rejected.window_start_unix_ns) {
            return Err(format!(
                "{} repeats feature window {}",
                extraction.recording_id, rejected.window_start_unix_ns
            ));
        }
        if rejected.window_end_unix_ns <= extraction.ended_at_unix_ns {
            full_rejections += 1;
        }
    }
    let full_windows = extraction.accepted_blocks.len() + full_rejections;
    if full_windows != COMPLETE_WINDOW_COUNT {
        return Err(format!(
            "{} must account for exactly {COMPLETE_WINDOW_COUNT} complete 3-second windows, got {full_windows}",
            extraction.recording_id
        ));
    }
    Ok(extraction.accepted_blocks.len())
}

fn independent_five_second_samples(
    extraction: &PositionFeatureExtraction,
    expected_grids: &mut Option<Vec<PositionReceiverGrid>>,
) -> Result<Vec<Vec<Vec<f64>>>, String> {
    let by_start: BTreeMap<u64, &PositionFeatureBlock> = extraction
        .accepted_blocks
        .iter()
        .map(|block| (block.window_start_unix_ns, block))
        .collect();
    if by_start.len() != extraction.accepted_blocks.len() {
        return Err(format!(
            "{} has duplicate accepted feature windows",
            extraction.recording_id
        ));
    }

    let first_measured_ns = extraction
        .started_at_unix_ns
        .checked_add(SETTLING_NS)
        .ok_or_else(|| format!("{} timestamp overflow", extraction.recording_id))?;
    let mut samples = Vec::with_capacity(INDEPENDENT_BLOCK_COUNT);
    for block_index in 0..INDEPENDENT_BLOCK_COUNT {
        let block_offset = u64::try_from(block_index)
            .map_err(|_| "training block index does not fit u64".to_string())?
            .checked_mul(INDEPENDENT_BLOCK_NS)
            .ok_or_else(|| "training block offset overflow".to_string())?;
        let block_start = first_measured_ns
            .checked_add(block_offset)
            .ok_or_else(|| "training block timestamp overflow".to_string())?;
        let mut contained = Vec::with_capacity(WINDOWS_PER_INDEPENDENT_BLOCK);
        for window_index in 0..WINDOWS_PER_INDEPENDENT_BLOCK {
            let window_offset = u64::try_from(window_index)
                .map_err(|_| "window index does not fit u64".to_string())?
                .checked_mul(WINDOW_STEP_NS)
                .ok_or_else(|| "window offset overflow".to_string())?;
            let window_start = block_start
                .checked_add(window_offset)
                .ok_or_else(|| "window timestamp overflow".to_string())?;
            let feature_block = by_start.get(&window_start).ok_or_else(|| {
                format!(
                    "{} cannot form independent block {}: fully-contained window at {} was rejected",
                    extraction.recording_id,
                    block_index + 1,
                    window_start
                )
            })?;
            let grids = grids_from_block(feature_block)?;
            match expected_grids {
                Some(expected) => validate_block_against_grids(feature_block, expected)?,
                None => *expected_grids = Some(grids),
            }
            contained.push(*feature_block);
        }
        samples.push(median_feature_sample(&contained)?);
    }
    Ok(samples)
}

fn median_feature_sample(blocks: &[&PositionFeatureBlock]) -> Result<Vec<Vec<f64>>, String> {
    if blocks.len() != WINDOWS_PER_INDEPENDENT_BLOCK {
        return Err(format!(
            "independent sample needs exactly {WINDOWS_PER_INDEPENDENT_BLOCK} windows"
        ));
    }
    let mut sample = Vec::with_capacity(RECEIVER_COUNT);
    for receiver_index in 0..RECEIVER_COUNT {
        let expected_rx_id = u8::try_from(receiver_index + 1)
            .map_err(|_| "receiver index does not fit u8".to_string())?;
        let mut features = Vec::with_capacity(FEATURES_PER_RECEIVER);
        for feature_index in 0..FEATURES_PER_RECEIVER {
            let mut values = [0.0; WINDOWS_PER_INDEPENDENT_BLOCK];
            for (block_index, block) in blocks.iter().enumerate() {
                let receiver = block
                    .receivers
                    .get(receiver_index)
                    .ok_or_else(|| format!("feature block is missing RX{expected_rx_id}"))?;
                if receiver.rx_id != expected_rx_id {
                    return Err(format!(
                        "feature block receiver order is invalid: expected RX{expected_rx_id}, got RX{}",
                        receiver.rx_id
                    ));
                }
                values[block_index] = receiver.features[feature_index];
            }
            values.sort_by(f64::total_cmp);
            let median = values[1];
            if !median.is_finite() {
                return Err(format!(
                    "RX{expected_rx_id} feature {feature_index} median is not finite"
                ));
            }
            features.push(median);
        }
        sample.push(features);
    }
    Ok(sample)
}

fn grids_from_block(block: &PositionFeatureBlock) -> Result<Vec<PositionReceiverGrid>, String> {
    if block.receivers.len() != RECEIVER_COUNT {
        return Err(format!(
            "feature window {} has {} receivers, expected {RECEIVER_COUNT}",
            block.window_start_unix_ns,
            block.receivers.len()
        ));
    }
    let mut grids = Vec::with_capacity(RECEIVER_COUNT);
    for (index, receiver) in block.receivers.iter().enumerate() {
        let expected_rx_id =
            u8::try_from(index + 1).map_err(|_| "receiver index does not fit u8".to_string())?;
        if receiver.rx_id != expected_rx_id {
            return Err(format!(
                "feature window {} expected RX{expected_rx_id}, got RX{}",
                block.window_start_unix_ns, receiver.rx_id
            ));
        }
        if receiver.features.iter().any(|value| !value.is_finite()) {
            return Err(format!(
                "feature window {} RX{} has non-finite features",
                block.window_start_unix_ns, receiver.rx_id
            ));
        }
        grids.push(PositionReceiverGrid {
            rx_id: receiver.rx_id,
            grid: receiver.grid,
        });
    }
    Ok(grids)
}

fn validate_block_against_grids(
    block: &PositionFeatureBlock,
    expected: &[PositionReceiverGrid],
) -> Result<(), String> {
    let actual = grids_from_block(block)?;
    if actual != expected {
        return Err(format!(
            "feature window {} CSI grids do not match the index",
            block.window_start_unix_ns
        ));
    }
    Ok(())
}

fn predict_capture(
    index: &PositionIndexArtifact,
    extraction: &PositionFeatureExtraction,
) -> Result<CapturePredictionStatus, String> {
    let mut classified = Vec::with_capacity(extraction.accepted_blocks.len());
    for block in &extraction.accepted_blocks {
        let classification = match index.predict_feature_block(block)? {
            PositionFingerprintPrediction::Position { position, .. } => {
                WindowClassification::Matched(position.id)
            }
            PositionFingerprintPrediction::Unknown { .. } => WindowClassification::Unknown,
            PositionFingerprintPrediction::Ambiguous { .. } => WindowClassification::Ambiguous,
        };
        classified.push((block.window_start_unix_ns, classification));
    }
    let consensus = temporal_consensus(extraction.started_at_unix_ns, &classified)?;
    Ok(decide_capture(consensus))
}

fn block_feature_matrix(block: &PositionFeatureBlock) -> Result<Vec<Vec<f64>>, String> {
    grids_from_block(block)?;
    Ok(block
        .receivers
        .iter()
        .map(|receiver| receiver.features.to_vec())
        .collect())
}

fn temporal_consensus(
    capture_start_ns: u64,
    classified: &[(u64, WindowClassification)],
) -> Result<TemporalConsensus, String> {
    let mut by_start = BTreeMap::new();
    for (start, classification) in classified {
        if by_start.insert(*start, classification).is_some() {
            return Err(format!("duplicate classified window at {start}"));
        }
    }

    let first_window_start = capture_start_ns
        .checked_add(SETTLING_NS)
        .ok_or_else(|| "capture start timestamp overflow".to_string())?;
    let mut history: VecDeque<&WindowClassification> =
        VecDeque::with_capacity(TEMPORAL_HISTORY_WINDOWS);
    let mut result = TemporalConsensus {
        accepted_windows: classified.len(),
        ..TemporalConsensus::default()
    };

    for index in 0..COMPLETE_WINDOW_COUNT {
        let offset = u64::try_from(index)
            .map_err(|_| "window index does not fit u64".to_string())?
            .checked_mul(WINDOW_STEP_NS)
            .ok_or_else(|| "window offset overflow".to_string())?;
        let start = first_window_start
            .checked_add(offset)
            .ok_or_else(|| "window timestamp overflow".to_string())?;
        let Some(classification) = by_start.get(&start).copied() else {
            history.clear();
            continue;
        };
        match classification {
            WindowClassification::Matched(_) => {}
            WindowClassification::Unknown => result.saw_unknown = true,
            WindowClassification::Ambiguous => result.saw_ambiguous = true,
        }
        history.push_back(classification);
        if history.len() > TEMPORAL_HISTORY_WINDOWS {
            history.pop_front();
        }
        if history.len() == TEMPORAL_HISTORY_WINDOWS {
            result.contiguous_opportunities += 1;
            if let Some(point_id) = confirmed_four_of_five(&history) {
                *result.confirmed_by_point.entry(point_id).or_default() += 1;
            }
        }
    }
    Ok(result)
}

fn confirmed_four_of_five(history: &VecDeque<&WindowClassification>) -> Option<String> {
    if history.len() != TEMPORAL_HISTORY_WINDOWS {
        return None;
    }
    let mut counts = BTreeMap::<&str, usize>::new();
    for classification in history {
        if let WindowClassification::Matched(point_id) = classification {
            *counts.entry(point_id.as_str()).or_default() += 1;
        }
    }
    counts
        .into_iter()
        .filter(|(_, count)| *count >= TEMPORAL_AGREEMENT_WINDOWS)
        .max_by(|left, right| left.1.cmp(&right.1).then_with(|| right.0.cmp(left.0)))
        .map(|(point_id, _)| point_id.to_string())
}

fn decide_capture(consensus: TemporalConsensus) -> CapturePredictionStatus {
    let minimum_contiguous_opportunities = required_coverage_count(TEMPORAL_OPPORTUNITY_COUNT);
    if consensus.accepted_windows < MINIMUM_ACCEPTED_WINDOWS
        || consensus.contiguous_opportunities < minimum_contiguous_opportunities
    {
        return CapturePredictionStatus::InsufficientEvidence;
    }

    let mut ranked: Vec<(&String, &usize)> = consensus.confirmed_by_point.iter().collect();
    ranked.sort_by(|left, right| right.1.cmp(left.1).then_with(|| left.0.cmp(right.0)));
    let Some((winner, winner_count)) = ranked.first().copied() else {
        return if consensus.saw_ambiguous {
            CapturePredictionStatus::Ambiguous
        } else {
            CapturePredictionStatus::Unknown
        };
    };
    let confirmed_total: usize = ranked.iter().map(|(_, count)| **count).sum();
    let runner_up_count = ranked.get(1).map_or(0, |(_, count)| **count);
    let unique_majority =
        *winner_count > runner_up_count && winner_count.saturating_mul(2) > confirmed_total;
    let sufficient_coverage = *winner_count >= required_coverage_count(TEMPORAL_OPPORTUNITY_COUNT);
    if unique_majority && sufficient_coverage {
        return CapturePredictionStatus::Matched {
            point_id: winner.clone(),
        };
    }
    if ranked.len() > 1 || consensus.saw_ambiguous {
        CapturePredictionStatus::Ambiguous
    } else {
        CapturePredictionStatus::Unknown
    }
}

fn required_coverage_count(total: usize) -> usize {
    total
        .saturating_mul(MINIMUM_TEMPORAL_COVERAGE_PERCENT)
        .div_ceil(100)
}

fn validate_index_header(
    schema_version: u16,
    kind: &str,
    algorithm_id: &str,
) -> Result<(), String> {
    if schema_version != INDEX_SCHEMA_VERSION {
        return Err(format!(
            "position-index schema must be {INDEX_SCHEMA_VERSION}, got {schema_version}"
        ));
    }
    if kind != INDEX_KIND {
        return Err(format!(
            "position-index kind must be {INDEX_KIND:?}, got {kind:?}"
        ));
    }
    if algorithm_id != POSITION_ALGORITHM_ID {
        return Err(format!(
            "position-index algorithm must be {POSITION_ALGORITHM_ID:?}, got {algorithm_id:?}"
        ));
    }
    Ok(())
}

fn validate_feature_contract(contract: &PositionFeatureContract) -> Result<(), String> {
    if contract.extractor_schema_version != EXTRACTOR_SCHEMA_VERSION
        || contract.extractor_algorithm != EXTRACTOR_ALGORITHM
        || contract.settling_ns != SETTLING_NS
        || contract.window_ns != WINDOW_NS
        || contract.window_step_ns != WINDOW_STEP_NS
        || contract.feature_count_per_rx != POSITION_FEATURE_COUNT
        || contract.feature_count_per_rx != FEATURES_PER_RECEIVER
        || contract.receiver_count != RECEIVER_COUNT
        || contract.independent_block_ns != INDEPENDENT_BLOCK_NS
        || contract.windows_per_independent_block != WINDOWS_PER_INDEPENDENT_BLOCK
    {
        return Err("stored position feature contract is incompatible".to_string());
    }
    if contract.live_presence_gate_applied {
        return Err(
            "index incorrectly claims that the not-yet-integrated D6 live-presence gate was applied"
                .to_string(),
        );
    }
    if contract.receiver_grids.len() != RECEIVER_COUNT {
        return Err(format!(
            "feature contract requires {RECEIVER_COUNT} receiver grids"
        ));
    }
    for (index, receiver) in contract.receiver_grids.iter().enumerate() {
        let expected =
            u8::try_from(index + 1).map_err(|_| "receiver index does not fit u8".to_string())?;
        if receiver.rx_id != expected {
            return Err(format!(
                "feature contract receiver grids must be sorted RX1..RX4, got RX{} at index {index}",
                receiver.rx_id
            ));
        }
        if receiver.grid.antenna_count == 0
            || receiver.grid.subcarrier_count == 0
            || receiver.grid.layout_flags & 0x10 != 0
        {
            return Err(format!(
                "feature contract RX{} contains an invalid CSI grid",
                receiver.rx_id
            ));
        }
    }
    Ok(())
}

fn validate_geometry(context: &str, geometry: &PositionCaptureGeometry) -> Result<(), String> {
    if geometry
        .room_dimensions_m
        .iter()
        .any(|value| !value.is_finite() || *value <= 0.0)
    {
        return Err(format!(
            "{context} room dimensions must be finite and positive"
        ));
    }
    if geometry.rx_positions_m.len() != RECEIVER_COUNT {
        return Err(format!(
            "{context} requires exactly {RECEIVER_COUNT} RX positions"
        ));
    }
    validate_room_coordinate(
        context,
        "tx_position_m",
        geometry.tx_position_m,
        geometry.room_dimensions_m,
    )?;
    for (index, position) in geometry.rx_positions_m.iter().copied().enumerate() {
        validate_room_coordinate(
            context,
            &format!("rx_positions_m[{index}]"),
            position,
            geometry.room_dimensions_m,
        )?;
    }
    Ok(())
}

fn validate_sorted_points(
    points: &[FingerprintPosition],
    geometry: &PositionCaptureGeometry,
) -> Result<(), String> {
    let actual_ids: Vec<&str> = points.iter().map(|point| point.id.as_str()).collect();
    if actual_ids != EXPECTED_POSITION_IDS {
        return Err(format!(
            "point IDs must be exactly P01 through P09 in canonical order, got {actual_ids:?}"
        ));
    }

    let mut floor_coordinates = BTreeMap::<[u64; 2], &str>::new();
    for (index, point) in points.iter().enumerate() {
        validate_nonempty("point id", &point.id)?;
        if index > 0 && points[index - 1].id >= point.id {
            return Err(format!(
                "point IDs must be unique and strictly sorted, got {:?} before {:?}",
                points[index - 1].id,
                point.id
            ));
        }
        validate_room_coordinate(
            "training point",
            &point.id,
            point.coordinates_m,
            geometry.room_dimensions_m,
        )?;
        let floor_bits = [
            canonical_coordinate_bits(point.coordinates_m[0]),
            canonical_coordinate_bits(point.coordinates_m[2]),
        ];
        if let Some(first_id) = floor_coordinates.insert(floor_bits, &point.id) {
            return Err(format!(
                "points {first_id:?} and {:?} have duplicate floor coordinates (x,z)",
                point.id
            ));
        }
    }
    Ok(())
}

fn canonical_coordinate_bits(value: f64) -> u64 {
    if value == 0.0 {
        0.0f64.to_bits()
    } else {
        value.to_bits()
    }
}

fn validate_room_coordinate(
    context: &str,
    field: &str,
    coordinate: [f64; 3],
    room_dimensions_m: [f64; 3],
) -> Result<(), String> {
    if coordinate.iter().any(|value| !value.is_finite())
        || coordinate
            .iter()
            .zip(room_dimensions_m)
            .any(|(value, maximum)| *value < 0.0 || *value > maximum)
    {
        return Err(format!(
            "{context} {field:?} coordinate {coordinate:?} lies outside room {room_dimensions_m:?}"
        ));
    }
    Ok(())
}

fn model_minimum_samples(model: &PositionFingerprintModel) -> Result<usize, String> {
    serde_json::to_value(model)
        .map_err(|error| format!("could not inspect model configuration: {error}"))?
        .get("config")
        .and_then(|config| config.get("minimum_samples_per_position"))
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| "model has no valid minimum_samples_per_position".to_string())
}

fn validate_nonempty(field: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("{field} must not be empty"));
    }
    Ok(())
}

fn validate_sha256(field: &str, value: &str) -> Result<(), String> {
    if value.len() != 64
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

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::io::{BufWriter, Write};

    use super::*;
    use crate::position_capture::RxPositionFeatures;
    use crate::position_evaluation::PositionTruthItem;
    use crate::raw_csi_recording::{
        IqPair, RawCsiFrame, SourceBinding, RAW_CSI_SCHEMA_VERSION, SOURCE_BINDING_REQUIRED_FLAGS,
        TX_SOURCE_BINDING_SCHEME, TX_SOURCE_BINDING_VERSION,
    };
    use serde_json::{json, Value};

    const SYNTHETIC_FRAME_STEP_NS: u64 = 200_000_000;
    const SYNTHETIC_SETUP_ID: &str = "fixed-position-e2e";
    const SYNTHETIC_SERVER_VERSION: &str = "position-e2e-test";
    const SYNTHETIC_POINT_MASKS: [u8; POSITION_COUNT] = [
        0b0000_1111,
        0b0011_0011,
        0b0101_0101,
        0b0001_0111,
        0b0010_0111,
        0b0100_0111,
        0b1000_0111,
        0b0010_1101,
        0b0100_1110,
    ];

    fn synthetic_source_binding(digest_character: char) -> SourceBinding {
        SourceBinding {
            trailer_version: TX_SOURCE_BINDING_VERSION,
            flags: SOURCE_BINDING_REQUIRED_FLAGS,
            scheme: TX_SOURCE_BINDING_SCHEME.to_string(),
            tx_filter_sha256: digest_character.to_string().repeat(64),
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum SyntheticCaptureKind {
        Empty,
        Training { point_index: usize },
        Blind { point_index: usize },
    }

    impl SyntheticCaptureKind {
        fn point_index(self) -> Option<usize> {
            match self {
                Self::Empty => None,
                Self::Training { point_index } | Self::Blind { point_index } => Some(point_index),
            }
        }

        fn is_blind(self) -> bool {
            matches!(self, Self::Blind { .. })
        }
    }

    fn test_grid(rx_id: u8) -> PositionGridIdentity {
        PositionGridIdentity {
            center_frequency_mhz: 2_437,
            antenna_count: 1,
            subcarrier_count: 64,
            ppdu_type: 0,
            layout_flags: rx_id,
        }
    }

    fn test_feature_block(start: u64, value: f64) -> PositionFeatureBlock {
        PositionFeatureBlock {
            window_start_unix_ns: start,
            window_end_unix_ns: start + WINDOW_NS,
            common_coverage_ns: WINDOW_NS,
            receivers: (1u8..=4)
                .map(|rx_id| RxPositionFeatures {
                    rx_id,
                    grid: test_grid(rx_id),
                    frame_count: 30,
                    observed_rate_millihz: 10_000,
                    coverage_ns: WINDOW_NS,
                    maximum_gap_ns: 100_000_000,
                    features: [value + f64::from(rx_id); POSITION_FEATURE_COUNT],
                })
                .collect(),
        }
    }

    fn test_extraction() -> PositionFeatureExtraction {
        let started_at_unix_ns = 1_000_000_000;
        PositionFeatureExtraction {
            schema_version: EXTRACTOR_SCHEMA_VERSION,
            algorithm: EXTRACTOR_ALGORITHM.to_string(),
            recording_id: "position-test".to_string(),
            started_at_unix_ns,
            ended_at_unix_ns: started_at_unix_ns + TRAINING_CAPTURE_NS,
            settling_ns: SETTLING_NS,
            window_ns: WINDOW_NS,
            window_step_ns: WINDOW_STEP_NS,
            feature_count_per_rx: POSITION_FEATURE_COUNT,
            accepted_blocks: (0..COMPLETE_WINDOW_COUNT)
                .map(|index| {
                    let start = started_at_unix_ns
                        + SETTLING_NS
                        + u64::try_from(index).unwrap() * WINDOW_STEP_NS;
                    test_feature_block(start, index as f64)
                })
                .collect(),
            rejected_windows: Vec::new(),
        }
    }

    fn protocol_test_frame(recording_id: &str, timestamp: u64, sequence: u32) -> RawCsiFrame {
        RawCsiFrame {
            schema_version: RAW_CSI_SCHEMA_VERSION,
            host_timestamp_unix_ns: timestamp,
            host_monotonic_ns: Some(timestamp),
            clock_epoch_id: Some("test-clock".to_string()),
            session_id: Some(recording_id.to_string()),
            label: None,
            ground_truth: None,
            rx_id: 1,
            antenna_count: 1,
            subcarrier_count: 1,
            center_frequency_mhz: 2_437,
            sequence,
            rssi_dbm: -50,
            noise_floor_dbm: -95,
            ppdu_type: 0,
            flags: 0,
            mesh_timestamp_us: None,
            source_binding: Some(synthetic_source_binding('f')),
            iq_pairs: vec![IqPair { i: 1, q: 1 }],
        }
    }

    fn protocol_test_capture(duration_ns: u64) -> PositionCapture {
        let recording_id = "protocol-test";
        let started_at_unix_ns = 1_000_000_000;
        let ended_at_unix_ns = started_at_unix_ns + duration_ns;
        let mut frames = vec![protocol_test_frame(
            recording_id,
            started_at_unix_ns + 34_000_000_000,
            1,
        )];
        if duration_ns > 36_000_000_000 {
            frames.push(protocol_test_frame(
                recording_id,
                started_at_unix_ns + 36_000_000_000,
                2,
            ));
        }
        PositionCapture {
            recording_id: recording_id.to_string(),
            setup_id: "fixed-setup".to_string(),
            setup_sha256: "a".repeat(64),
            server_version: "test".to_string(),
            geometry: test_geometry(),
            started_at_unix_ns,
            ended_at_unix_ns,
            frames,
        }
    }

    fn test_geometry() -> PositionCaptureGeometry {
        PositionCaptureGeometry {
            room_dimensions_m: [4.02, 2.59, 3.44],
            tx_position_m: [1.51, 1.19, 0.39],
            rx_positions_m: vec![
                [0.0, 0.50, 0.28],
                [4.02, 0.87, 0.97],
                [0.0, 0.74, 2.11],
                [4.02, 0.87, 2.46],
            ],
        }
    }

    fn replace_training_template_value(template: &mut String, token: &str, value: &Value) {
        let placeholder = format!("\"{token}\"");
        assert!(
            template.contains(&placeholder),
            "training template is missing placeholder {token}"
        );
        *template = template.replace(&placeholder, &serde_json::to_string(value).unwrap());
    }

    fn synthetic_hash(index: usize) -> String {
        format!("{index:064x}")
    }

    #[test]
    fn public_training_template_matches_strict_schema() {
        let source = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../scripts/templates/position-training-manifest.template.json"
        ));
        serde_json::from_str::<Value>(source)
            .expect("public training-manifest template must be valid JSON");
        assert!(!source.contains("\"ssid\""));
        assert!(!source.contains("\"password\""));
        assert!(!source.contains("\"filter_mac\""));

        let mut rendered = source.to_string();
        let geometry = test_geometry();
        for (token, value) in [
            ("__SETUP_ID__", json!("setup-0123456789abcdef")),
            ("__SETUP_SHA256__", json!(synthetic_hash(1))),
            ("__ROOM_DIMENSIONS_M__", json!(geometry.room_dimensions_m)),
            ("__TX_POSITION_M__", json!(geometry.tx_position_m)),
            ("__RX_POSITIONS_M__", json!(geometry.rx_positions_m)),
            (
                "__CALIBRATION_RAW_PATH__",
                json!("captures/empty-neutral-01.raw-csi.v1.jsonl"),
            ),
            ("__CALIBRATION_RECORDING_ID__", json!("empty-neutral-01")),
            ("__CALIBRATION_RAW_SHA256__", json!(synthetic_hash(2))),
            ("__CALIBRATION_METADATA_SHA256__", json!(synthetic_hash(3))),
            ("__CALIBRATION_SIGNAL_SHA256__", json!(synthetic_hash(4))),
        ] {
            replace_training_template_value(&mut rendered, token, &value);
        }

        for (index, point) in synthetic_points().iter().enumerate() {
            let point_number = index + 1;
            let prefix = format!("P{point_number:02}");
            let hash_base = 5 + index * 3;
            for (suffix, value) in [
                ("COORDINATES_M", json!(point.coordinates_m)),
                (
                    "CAPTURE_RAW_PATH",
                    json!(format!(
                        "captures/train-p{point_number:02}.raw-csi.v1.jsonl"
                    )),
                ),
                (
                    "CAPTURE_RECORDING_ID",
                    json!(format!("train-p{point_number:02}")),
                ),
                ("CAPTURE_RAW_SHA256", json!(synthetic_hash(hash_base))),
                (
                    "CAPTURE_METADATA_SHA256",
                    json!(synthetic_hash(hash_base + 1)),
                ),
                (
                    "CAPTURE_SIGNAL_SHA256",
                    json!(synthetic_hash(hash_base + 2)),
                ),
            ] {
                replace_training_template_value(
                    &mut rendered,
                    &format!("__{prefix}_{suffix}__"),
                    &value,
                );
            }
        }
        assert!(
            !rendered.contains("__"),
            "every training-template placeholder must be covered by this schema test"
        );

        let manifest: PositionTrainingManifest =
            serde_json::from_str(&rendered).expect("rendered training template must deserialize");
        validate_training_manifest(&manifest)
            .expect("rendered training template must pass the strict validator");
    }

    fn inspection_provenance(id: &str, hash_character: char) -> PositionCaptureProvenance {
        PositionCaptureProvenance {
            recording_id: id.to_string(),
            raw_sha256: hash_character.to_string().repeat(64),
            metadata_sha256: char::from_u32(u32::from(hash_character) + 1)
                .unwrap()
                .to_string()
                .repeat(64),
            signal_sha256: char::from_u32(u32::from(hash_character) + 2)
                .unwrap()
                .to_string()
                .repeat(64),
        }
    }

    fn synthetic_points() -> Vec<FingerprintPosition> {
        let columns = [0.75, 2.01, 3.27];
        let rows = [0.75, 1.72, 2.69];
        (0..POSITION_COUNT)
            .map(|index| FingerprintPosition {
                id: format!("P{:02}", index + 1),
                coordinates_m: [columns[index % 3], 0.0, rows[index / 3]],
            })
            .collect()
    }

    fn synthetic_iq_pairs(kind: SyntheticCaptureKind, rx_id: u8, tick_index: u64) -> Vec<IqPair> {
        let Some(point_index) = kind.point_index() else {
            return vec![IqPair { i: 40, q: 0 }; 64];
        };
        let point_mask = SYNTHETIC_POINT_MASKS[point_index];
        let noise_bin = (point_index * 7 + usize::from(rx_id)) % 64;
        (0..64)
            .map(|bin| {
                let band = bin / 8;
                let receiver_band = (band + usize::from(rx_id) - 1) % 8;
                let high = point_mask & (1 << receiver_band) != 0;
                let mut amplitude = if high { 41 } else { 39 };
                if kind.is_blind() && tick_index % 17 == u64::from(rx_id - 1) && bin == noise_bin {
                    amplitude += 1;
                }
                IqPair { i: amplitude, q: 0 }
            })
            .collect()
    }

    fn synthetic_rssi(kind: SyntheticCaptureKind, rx_id: u8) -> i8 {
        let empty_rssi = -60 - i8::try_from(rx_id).unwrap();
        match kind.point_index() {
            None => empty_rssi,
            Some(point_index) => empty_rssi - 8 + i8::try_from(point_index * 2).unwrap(),
        }
    }

    fn write_synthetic_capture(
        directory: &Path,
        recording_id: &str,
        started_at_unix_ns: u64,
        duration_ns: u64,
        kind: SyntheticCaptureKind,
    ) -> PathBuf {
        write_synthetic_capture_with_binding(
            directory,
            recording_id,
            started_at_unix_ns,
            duration_ns,
            kind,
            'f',
        )
    }

    fn write_synthetic_capture_with_binding(
        directory: &Path,
        recording_id: &str,
        started_at_unix_ns: u64,
        duration_ns: u64,
        kind: SyntheticCaptureKind,
        digest_character: char,
    ) -> PathBuf {
        assert_eq!(duration_ns % SYNTHETIC_FRAME_STEP_NS, 0);
        let raw_path = raw_csi_recording::recording_path(directory, recording_id).unwrap();
        let mut writer = BufWriter::new(File::create(&raw_path).unwrap());
        let tick_count = duration_ns / SYNTHETIC_FRAME_STEP_NS;
        let mut frames_written = 0u64;
        let mut rx_summaries = BTreeMap::<u8, raw_csi_recording::RawCsiRxSummary>::new();
        for tick_index in 0..tick_count {
            let timestamp = started_at_unix_ns + tick_index.saturating_mul(SYNTHETIC_FRAME_STEP_NS);
            for rx_id in 1u8..=4 {
                let frame = RawCsiFrame {
                    schema_version: RAW_CSI_SCHEMA_VERSION,
                    host_timestamp_unix_ns: timestamp,
                    host_monotonic_ns: Some(timestamp),
                    clock_epoch_id: Some("test-clock".to_string()),
                    session_id: Some(recording_id.to_string()),
                    label: None,
                    ground_truth: None,
                    rx_id,
                    antenna_count: 1,
                    subcarrier_count: 64,
                    center_frequency_mhz: 2_437,
                    sequence: u32::try_from(tick_index).unwrap(),
                    rssi_dbm: synthetic_rssi(kind, rx_id),
                    noise_floor_dbm: -95,
                    ppdu_type: 0,
                    flags: 0,
                    mesh_timestamp_us: None,
                    source_binding: Some(synthetic_source_binding(digest_character)),
                    iq_pairs: synthetic_iq_pairs(kind, rx_id, tick_index),
                };
                writer
                    .write_all(
                        raw_csi_recording::encode_json_line(&frame)
                            .unwrap()
                            .as_bytes(),
                    )
                    .unwrap();
                match rx_summaries.get_mut(&rx_id) {
                    Some(summary) => {
                        summary.validate_next_frame(&frame).unwrap();
                        summary.observe_written_frame(&frame);
                    }
                    None => {
                        rx_summaries.insert(
                            rx_id,
                            raw_csi_recording::RawCsiRxSummary::first_written_frame(&frame),
                        );
                    }
                }
                frames_written += 1;
            }
        }
        writer.flush().unwrap();

        let ended_at_unix_ns = started_at_unix_ns + duration_ns;
        let rx_summaries: Vec<_> = rx_summaries.into_values().collect();
        let sidecar = serde_json::json!({
            "schema_version": RAW_CSI_SCHEMA_VERSION,
            "recording_id": recording_id,
            "setup_id": SYNTHETIC_SETUP_ID,
            "setup_sha256": "a".repeat(64),
            "server_version": SYNTHETIC_SERVER_VERSION,
            "started_at_unix_seconds": started_at_unix_ns / 1_000_000_000,
            "started_at_unix_ns": started_at_unix_ns,
            "ended_at_unix_seconds": ended_at_unix_ns / 1_000_000_000,
            "ended_at_unix_ns": ended_at_unix_ns,
            "duration_secs": duration_ns / 1_000_000_000,
            "max_duration_seconds": duration_ns / 1_000_000_000,
            "tx_position": test_geometry().tx_position_m,
            "rx_positions": test_geometry().rx_positions_m,
            "room_dimensions": test_geometry().room_dimensions_m,
            "capture_scope": "validated_udp_csi_all_grids",
            "status": "completed",
            "frames_written": frames_written,
            "rx_summaries": rx_summaries,
            "dropped_frames": 0,
            "incomplete": false,
            "writer_error": null,
            "label": null,
            "ground_truth": null
        });
        let metadata_path = directory.join(format!("{recording_id}.raw-csi.v1.meta.json"));
        fs::write(metadata_path, serde_json::to_vec_pretty(&sidecar).unwrap()).unwrap();
        raw_path
    }

    fn manifest_source_json(
        path: &Path,
        provenance: &PositionCaptureProvenance,
    ) -> serde_json::Value {
        serde_json::json!({
            "path": path.file_name().unwrap().to_str().unwrap(),
            "recording_id": provenance.recording_id,
            "raw_sha256": provenance.raw_sha256,
            "metadata_sha256": provenance.metadata_sha256,
            "signal_sha256": provenance.signal_sha256
        })
    }

    fn rewrite_capture_source_binding(path: &Path, source_binding: Option<SourceBinding>) {
        let encoded = fs::read_to_string(path).unwrap();
        let file = File::create(path).unwrap();
        let mut writer = BufWriter::new(file);
        for line in encoded.lines() {
            let mut frame = raw_csi_recording::decode_json_line(line).unwrap();
            frame.source_binding = source_binding.clone();
            writer
                .write_all(
                    raw_csi_recording::encode_json_line(&frame)
                        .unwrap()
                        .as_bytes(),
                )
                .unwrap();
        }
        writer.flush().unwrap();
    }

    #[test]
    fn six_independent_blocks_use_only_the_three_contained_windows() {
        let extraction = test_extraction();
        let mut grids = None;
        let samples = independent_five_second_samples(&extraction, &mut grids).unwrap();

        assert_eq!(samples.len(), 6);
        for (block_index, sample) in samples.iter().enumerate() {
            let expected_window_median = (block_index * 5 + 1) as f64;
            assert_eq!(sample[0][0], expected_window_median + 1.0);
            assert_eq!(sample[3][27], expected_window_median + 4.0);
        }
    }

    #[test]
    fn protocol_span_accepts_a_recorder_tail_and_discards_only_that_tail() {
        let capture = protocol_test_capture(37_000_000_000);
        let trimmed = trim_to_protocol_span(&capture, TRAINING_CAPTURE_NS, "training").unwrap();

        assert_eq!(
            trimmed.ended_at_unix_ns,
            capture.started_at_unix_ns + TRAINING_CAPTURE_NS
        );
        assert_eq!(trimmed.frames.len(), 1);
        assert_eq!(trimmed.frames[0].sequence, 1);
        assert_eq!(capture.frames.len(), 2);
    }

    #[test]
    fn protocol_signal_identity_cannot_be_changed_by_a_different_recorder_tail() {
        let first = protocol_test_capture(37_000_000_000);
        let mut second = first.clone();
        second.frames[1].iq_pairs[0] = IqPair { i: 7, q: 9 };

        assert_ne!(
            signal_sha256(&first.frames).unwrap(),
            signal_sha256(&second.frames).unwrap()
        );
        let first_protocol =
            trim_to_protocol_span(&first, TRAINING_CAPTURE_NS, "training").unwrap();
        let second_protocol =
            trim_to_protocol_span(&second, TRAINING_CAPTURE_NS, "training").unwrap();
        assert_eq!(
            signal_sha256(&first_protocol.frames).unwrap(),
            signal_sha256(&second_protocol.frames).unwrap()
        );
    }

    #[test]
    fn protocol_span_rejects_a_capture_shorter_than_thirty_five_seconds() {
        let capture = protocol_test_capture(TRAINING_CAPTURE_NS - 1);
        let error = trim_to_protocol_span(&capture, TRAINING_CAPTURE_NS, "training").unwrap_err();
        assert!(error.contains("too short"));
        assert!(error.contains("35000000000"));
    }

    #[test]
    fn protocol_signal_hash_is_stable_when_only_the_recorder_tail_changes() {
        let exact = protocol_test_capture(TRAINING_CAPTURE_NS);
        let tailed = protocol_test_capture(TRAINING_CAPTURE_NS + 2_000_000_000);
        let exact_protocol =
            trim_to_protocol_span(&exact, TRAINING_CAPTURE_NS, "position").unwrap();
        let tailed_protocol =
            trim_to_protocol_span(&tailed, TRAINING_CAPTURE_NS, "position").unwrap();

        assert_eq!(
            signal_sha256(&exact_protocol.frames).unwrap(),
            signal_sha256(&tailed_protocol.frames).unwrap()
        );
        assert_ne!(exact.frames.len(), tailed.frames.len());
    }

    #[test]
    fn inspection_context_rejects_mixed_setup_identity() {
        let first = protocol_test_capture(TRAINING_CAPTURE_NS);
        let expected = PositionInspectionContext {
            setup_id: first.setup_id.clone(),
            setup_sha256: first.setup_sha256.clone(),
            source_binding: synthetic_source_binding('f'),
            server_version: first.server_version.clone(),
            geometry: first.geometry.clone(),
            receiver_grids: (1u8..=4)
                .map(|rx_id| PositionReceiverGrid {
                    rx_id,
                    grid: test_grid(rx_id),
                })
                .collect(),
        };
        let mut second = first.clone();
        second.recording_id = "protocol-test-second".to_string();
        second.setup_id = "different-setup".to_string();

        let error =
            validate_inspection_context(&expected, &second, &expected.receiver_grids).unwrap_err();
        assert!(error.contains("does not share"));
    }

    #[test]
    fn inspection_artifact_is_deterministic_across_input_order_and_has_no_paths() {
        let capture_a = inspection_provenance("capture-a", 'a');
        let capture_b = inspection_provenance("capture-b", 'd');
        let build = |captures| {
            PositionCaptureInspectionArtifact::new(
                PositionInspectionProtocol::Position,
                "fixed-setup".to_string(),
                "f".repeat(64),
                synthetic_source_binding('f'),
                "test".to_string(),
                test_geometry(),
                captures,
            )
            .unwrap()
        };
        let forward = build(vec![capture_a.clone(), capture_b.clone()]);
        let reverse = build(vec![capture_b, capture_a]);

        assert_eq!(forward, reverse);
        let forward_json = deterministic_pretty_json(&forward).unwrap();
        let reverse_json = deterministic_pretty_json(&reverse).unwrap();
        assert_eq!(forward_json, reverse_json);
        assert!(!String::from_utf8(forward_json)
            .unwrap()
            .contains("\"path\""));
    }

    #[test]
    fn inspection_rejects_historical_unbound_capture_but_raw_v1_stays_readable() {
        let directory = tempfile::tempdir().unwrap();
        let path = write_synthetic_capture(
            directory.path(),
            "historical-unbound",
            1_700_000_000_000_000_000,
            TRAINING_CAPTURE_NS,
            SyntheticCaptureKind::Blind { point_index: 0 },
        );
        rewrite_capture_source_binding(&path, None);

        let first_line = fs::read_to_string(&path).unwrap();
        let decoded = raw_csi_recording::decode_json_line(first_line.lines().next().unwrap())
            .expect("historical raw-csi-v1 line must remain readable");
        assert!(decoded.source_binding.is_none());

        let error = inspect_captures(&[path], PositionInspectionProtocol::Position)
            .expect_err("unbound raw data cannot prove a sealed position capture");
        assert!(error.contains("has no TX-source binding"));
    }

    #[test]
    fn file_based_position_pipeline_recovers_all_nine_blind_points() {
        let directory = tempfile::tempdir().unwrap();
        let setup_sha256 = "a".repeat(64);
        let first_capture_start_ns = 1_700_000_000_000_000_000u64;
        let points = synthetic_points();

        let calibration_path = write_synthetic_capture(
            directory.path(),
            "empty-e2e",
            first_capture_start_ns,
            CALIBRATION_CAPTURE_NS,
            SyntheticCaptureKind::Empty,
        );
        let mut training_paths = Vec::with_capacity(POSITION_COUNT);
        let mut blind_paths = Vec::with_capacity(POSITION_COUNT);
        let mut expected_by_recording = BTreeMap::new();
        for (point_index, point) in points.iter().enumerate() {
            let training_recording_id = format!("train-p{:02}", point_index + 1);
            let blind_recording_id = format!("blind-p{:02}", point_index + 1);
            let training_start_ns = first_capture_start_ns
                + (u64::try_from(point_index).unwrap() + 1) * 100_000_000_000;
            let blind_start_ns = first_capture_start_ns
                + (u64::try_from(point_index).unwrap() + 20) * 100_000_000_000;
            training_paths.push(write_synthetic_capture(
                directory.path(),
                &training_recording_id,
                training_start_ns,
                TRAINING_CAPTURE_NS,
                SyntheticCaptureKind::Training { point_index },
            ));
            blind_paths.push(write_synthetic_capture(
                directory.path(),
                &blind_recording_id,
                blind_start_ns,
                TRAINING_CAPTURE_NS,
                SyntheticCaptureKind::Blind { point_index },
            ));
            expected_by_recording.insert(blind_recording_id, point.id.clone());
        }

        let empty_inspection = inspect_captures(
            std::slice::from_ref(&calibration_path),
            PositionInspectionProtocol::EmptyCalibration,
        )
        .unwrap();
        let training_inspection =
            inspect_captures(&training_paths, PositionInspectionProtocol::Position).unwrap();
        let blind_inspection =
            inspect_captures(&blind_paths, PositionInspectionProtocol::Position).unwrap();
        assert_eq!(empty_inspection.captures.len(), 1);
        assert_eq!(training_inspection.captures.len(), POSITION_COUNT);
        assert_eq!(blind_inspection.captures.len(), POSITION_COUNT);

        let training_signal_hashes: BTreeSet<&str> = training_inspection
            .captures
            .iter()
            .map(|capture| capture.signal_sha256.as_str())
            .collect();
        let blind_signal_hashes: BTreeSet<&str> = blind_inspection
            .captures
            .iter()
            .map(|capture| capture.signal_sha256.as_str())
            .collect();
        assert!(
            training_signal_hashes.is_disjoint(&blind_signal_hashes),
            "blind CSI signals must not duplicate any training signal"
        );

        let training_manifest_points: Vec<serde_json::Value> = points
            .iter()
            .enumerate()
            .map(|(point_index, point)| {
                let recording_id = format!("train-p{:02}", point_index + 1);
                let provenance = training_inspection
                    .captures
                    .iter()
                    .find(|capture| capture.recording_id == recording_id)
                    .unwrap();
                serde_json::json!({
                    "id": point.id,
                    "coordinates_m": point.coordinates_m,
                    "captures": [
                        manifest_source_json(&training_paths[point_index], provenance)
                    ]
                })
            })
            .collect();
        let training_manifest = serde_json::json!({
            "schema_version": TRAINING_MANIFEST_SCHEMA_VERSION,
            "kind": TRAINING_MANIFEST_KIND,
            "setup_id": SYNTHETIC_SETUP_ID,
            "setup_sha256": setup_sha256.clone(),
            "geometry": test_geometry(),
            "calibration": manifest_source_json(
                &calibration_path,
                &empty_inspection.captures[0]
            ),
            "points": training_manifest_points
        });
        let training_manifest_path = directory.path().join("training-manifest.json");
        fs::write(
            &training_manifest_path,
            serde_json::to_vec_pretty(&training_manifest).unwrap(),
        )
        .unwrap();

        let index = build_index(&training_manifest_path).unwrap();
        index.validate().unwrap();
        let index_path = directory.path().join("position-index.json");
        crate::position_artifact::write_pretty_json_no_clobber(&index_path, &index).unwrap();
        let index_sha256 = sha256_file(&index_path).unwrap();
        let runtime = crate::position_live::PositionIndexRuntime::load(
            &index_path,
            SYNTHETIC_SETUP_ID,
            &setup_sha256,
            Some(&index_sha256),
        )
        .expect("validated index must bind to its exact setup and bytes");
        assert_eq!(runtime.index_sha256(), index_sha256);
        assert_eq!(runtime.setup_id(), SYNTHETIC_SETUP_ID);
        assert_eq!(runtime.setup_sha256(), setup_sha256);
        assert!(crate::position_live::PositionIndexRuntime::load(
            &index_path,
            "different-setup",
            &setup_sha256,
            Some(&index_sha256),
        )
        .is_err());
        assert!(crate::position_live::PositionIndexRuntime::load(
            &index_path,
            SYNTHETIC_SETUP_ID,
            &"b".repeat(64),
            Some(&index_sha256),
        )
        .is_err());
        assert!(crate::position_live::PositionIndexRuntime::load(
            &index_path,
            SYNTHETIC_SETUP_ID,
            &setup_sha256,
            Some(&"c".repeat(64)),
        )
        .is_err());

        rewrite_capture_source_binding(&training_paths[0], Some(synthetic_source_binding('e')));
        let mixed_inspection_error =
            inspect_captures(&training_paths, PositionInspectionProtocol::Position)
                .expect_err("one setup cannot mix two runtime TX identities");
        assert!(mixed_inspection_error.contains("does not share the inspection setup, TX source"));

        let changed_training_inspection = inspect_captures(
            std::slice::from_ref(&training_paths[0]),
            PositionInspectionProtocol::Position,
        )
        .unwrap();
        let mut mismatched_training_manifest = training_manifest.clone();
        mismatched_training_manifest["points"][0]["captures"][0] =
            manifest_source_json(&training_paths[0], &changed_training_inspection.captures[0]);
        let mismatched_training_manifest_path =
            directory.path().join("training-manifest-wrong-tx.json");
        fs::write(
            &mismatched_training_manifest_path,
            serde_json::to_vec_pretty(&mismatched_training_manifest).unwrap(),
        )
        .unwrap();
        let build_error = build_index(&mismatched_training_manifest_path)
            .expect_err("training TX identity must match empty calibration");
        assert!(build_error.contains("TX-source binding differs"));

        let predictions = predict_blind(&index_path, &blind_paths).unwrap();
        assert_eq!(predictions.captures().len(), POSITION_COUNT);
        for capture in predictions.captures() {
            let expected_point_id = expected_by_recording.get(capture.recording_id()).unwrap();
            assert_eq!(
                capture.prediction(),
                &CapturePredictionStatus::Matched {
                    point_id: expected_point_id.clone()
                }
            );
        }

        let wrong_tx_blind = write_synthetic_capture_with_binding(
            directory.path(),
            "blind-wrong-tx",
            first_capture_start_ns + 4_000_000_000_000,
            TRAINING_CAPTURE_NS,
            SyntheticCaptureKind::Blind { point_index: 0 },
            'e',
        );
        let prediction_error = predict_blind(&index_path, &[wrong_tx_blind])
            .expect_err("blind TX identity must match the trained index");
        assert!(prediction_error.contains("TX-source binding differs"));

        let predictions_path = directory.path().join("position-predictions.json");
        crate::position_artifact::write_pretty_json_no_clobber(&predictions_path, &predictions)
            .unwrap();
        let predictions_sha256 = sha256_file(&predictions_path).unwrap();
        let truth_items: Vec<PositionTruthItem> = predictions
            .captures()
            .iter()
            .map(|capture| {
                PositionTruthItem::new(
                    capture.recording_id(),
                    capture.raw_sha256(),
                    capture.metadata_sha256(),
                    capture.signal_sha256(),
                    expected_by_recording.get(capture.recording_id()).unwrap(),
                )
            })
            .collect();
        let truth = PositionTruthManifest::new(
            predictions_sha256,
            predictions.index_sha256(),
            predictions.setup_sha256(),
            truth_items,
        )
        .unwrap();
        let truth_path = directory.path().join("position-truth.json");
        crate::position_artifact::write_pretty_json_no_clobber(&truth_path, &truth).unwrap();

        let report = evaluate_predictions(&predictions_path, &truth_path).unwrap();
        assert_eq!(report.total, POSITION_COUNT as u64);
        assert_eq!(report.matched, POSITION_COUNT as u64);
        assert_eq!(report.correct, POSITION_COUNT as u64);
        assert_eq!(report.unknown, 0);
        assert_eq!(report.ambiguous, 0);
        assert_eq!(report.insufficient_evidence, 0);
        assert_eq!(report.coverage, 1.0);
        assert_eq!(report.accuracy_all, 1.0);
        assert_eq!(report.accuracy_decided, Some(1.0));
        assert_eq!(report.median_floor_error_m, Some(0.0));
        assert_eq!(report.p95_floor_error_m, Some(0.0));
        for (row_index, row) in report.confusion.rows.iter().enumerate() {
            assert_eq!(row.matched_by_point[row_index], 1);
            assert_eq!(row.matched_by_point.iter().sum::<u64>(), 1);
        }

        let blind_raw = fs::read_to_string(&blind_paths[0]).unwrap();
        assert!(!blind_raw.contains("\"label\""));
        assert!(!blind_raw.contains("\"ground_truth\""));
        assert!(!blind_raw.contains("expected_point_id"));
    }

    #[test]
    fn point_validation_rejects_duplicate_floor_coordinates_at_different_heights() {
        let mut points = synthetic_points();
        points[1].coordinates_m = [points[0].coordinates_m[0], 1.5, points[0].coordinates_m[2]];
        let error = validate_sorted_points(&points, &test_geometry()).unwrap_err();
        assert!(error.contains("floor coordinates"));
    }

    #[test]
    fn point_validation_rejects_noncanonical_nine_point_ids() {
        let mut points = synthetic_points();
        points[8].id = "P10".to_string();

        let error = validate_sorted_points(&points, &test_geometry()).unwrap_err();

        assert!(error.contains("exactly P01 through P09"));
    }

    #[test]
    fn four_of_five_equal_windows_confirm_one_point() {
        let a = WindowClassification::Matched("A".to_string());
        let unknown = WindowClassification::Unknown;
        let history = VecDeque::from(vec![&a, &a, &unknown, &a, &a]);
        assert_eq!(confirmed_four_of_five(&history), Some("A".to_string()));
    }

    #[test]
    fn missing_one_second_window_resets_temporal_history() {
        let start = 10_000_000_000;
        let first = start + SETTLING_NS;
        let classified: Vec<(u64, WindowClassification)> = [0u64, 1, 2, 4, 5, 6, 7]
            .into_iter()
            .map(|index| {
                (
                    first + index * WINDOW_STEP_NS,
                    WindowClassification::Matched("A".to_string()),
                )
            })
            .collect();

        let consensus = temporal_consensus(start, &classified).unwrap();
        assert_eq!(consensus.contiguous_opportunities, 0);
        assert!(consensus.confirmed_by_point.is_empty());
    }

    #[test]
    fn matched_run_needs_unique_majority_and_eighty_percent_coverage() {
        let consensus = TemporalConsensus {
            accepted_windows: COMPLETE_WINDOW_COUNT,
            contiguous_opportunities: TEMPORAL_OPPORTUNITY_COUNT,
            confirmed_by_point: BTreeMap::from([("A".to_string(), 20), ("B".to_string(), 4)]),
            saw_unknown: false,
            saw_ambiguous: false,
        };
        assert_eq!(
            decide_capture(consensus),
            CapturePredictionStatus::Matched {
                point_id: "A".to_string()
            }
        );
    }

    #[test]
    fn corrupt_index_header_is_rejected() {
        let error =
            validate_index_header(INDEX_SCHEMA_VERSION + 1, INDEX_KIND, POSITION_ALGORITHM_ID)
                .unwrap_err();
        assert!(error.contains("schema"));
        assert!(validate_index_header(
            INDEX_SCHEMA_VERSION,
            "ruview.wrong-kind",
            POSITION_ALGORITHM_ID
        )
        .is_err());
    }

    #[test]
    fn training_blind_overlap_is_rejected_by_signal_hash() {
        let training =
            CaptureArtifactIdentity::new("train-a", "a".repeat(64), "b".repeat(64)).unwrap();
        let blind =
            CaptureArtifactIdentity::new("blind-a", "c".repeat(64), "b".repeat(64)).unwrap();
        assert!(check_capture_sets(&[training], &[blind]).is_err());
    }

    #[test]
    fn temporal_input_order_does_not_change_consensus() {
        let start = 20_000_000_000;
        let first = start + SETTLING_NS;
        let ordered: Vec<(u64, WindowClassification)> = (0..COMPLETE_WINDOW_COUNT)
            .map(|index| {
                (
                    first + u64::try_from(index).unwrap() * WINDOW_STEP_NS,
                    WindowClassification::Matched("A".to_string()),
                )
            })
            .collect();
        let mut reversed = ordered.clone();
        reversed.reverse();
        assert_eq!(
            temporal_consensus(start, &ordered).unwrap(),
            temporal_consensus(start, &reversed).unwrap()
        );
    }
}

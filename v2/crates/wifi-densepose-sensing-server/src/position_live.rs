//! Fail-closed live position decisions for the fixed-room fingerprint model.
//!
//! This module deliberately owns no CSI feature math. Every live window goes
//! through [`extract_position_feature_window`], which is the same quality and
//! feature boundary used by offline capture extraction. A public position is
//! emitted only after four of five consecutive one-second window decisions
//! agree on one configured point.

use std::collections::{BTreeMap, VecDeque};
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::mmwave_position_index::{load_mmwave_position_index, MmwavePositionIndexArtifact};
use super::position_capture::{
    extract_position_feature_window, PositionCapture, WINDOW_NS, WINDOW_STEP_NS,
};
use super::position_fingerprint::{FingerprintPosition, PositionFingerprintPrediction};
use super::position_offline::{load_validated_position_index, PositionIndexArtifact};
use super::raw_csi_recording::RawCsiFrame;

const LIVE_RECORDING_ID: &str = "position-live-window";
const CONSENSUS_WINDOW_COUNT: usize = 5;
const CONSENSUS_REQUIRED_COUNT: usize = 4;
const DEFAULT_MAX_BUFFERED_FRAMES: usize = 4_096;
const STALE_AFTER_NS: u64 = WINDOW_STEP_NS;

/// Explicit D6/presence readiness supplied by the live sensing pipeline.
///
/// Position inference never invents presence. Only `ready_present` permits a
/// fingerprint window to enter the model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PresenceGate {
    ReadyPresent,
    ReadyAbsent,
    Uncalibrated,
    Insufficient,
    Stale,
}

/// Coordinate-bearing public state for the live position endpoint.
///
/// The five fail-closed states are intentionally fieldless: no stale,
/// ambiguous, or guessed coordinates can leak through serialization.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub(crate) enum LivePositionState {
    Position {
        point_id: String,
        coordinates_m: [f64; 3],
    },
    Unknown,
    Ambiguous,
    Insufficient,
    Uncalibrated,
    Stale,
}

/// Validated immutable position index together with the SHA-256 of the exact
/// bytes loaded from disk.
#[derive(Debug)]
pub(crate) struct PositionIndexRuntime {
    index: RuntimeIndexArtifact,
    index_sha256: String,
}

#[derive(Debug)]
enum RuntimeIndexArtifact {
    Manual(PositionIndexArtifact),
    Mmwave(MmwavePositionIndexArtifact),
}

impl RuntimeIndexArtifact {
    fn setup_id(&self) -> &str {
        match self {
            Self::Manual(index) => index.setup_id(),
            Self::Mmwave(index) => index.setup_id(),
        }
    }

    fn setup_sha256(&self) -> &str {
        match self {
            Self::Manual(index) => index.setup_sha256(),
            Self::Mmwave(index) => index.setup_sha256(),
        }
    }

    fn server_version(&self) -> &str {
        match self {
            Self::Manual(index) => index.server_version(),
            Self::Mmwave(index) => index.server_version(),
        }
    }

    fn geometry(&self) -> &super::position_capture::PositionCaptureGeometry {
        match self {
            Self::Manual(index) => index.geometry(),
            Self::Mmwave(index) => index.geometry(),
        }
    }

    fn empty_reference(&self) -> &super::position_capture::PositionEmptyReference {
        match self {
            Self::Manual(index) => index.empty_reference(),
            Self::Mmwave(index) => index.empty_reference(),
        }
    }

    fn predict_feature_block(
        &self,
        block: &super::position_capture::PositionFeatureBlock,
    ) -> Result<PositionFingerprintPrediction, String> {
        match self {
            Self::Manual(index) => index.predict_feature_block(block),
            Self::Mmwave(index) => index.predict_feature_block(block),
        }
    }
}

impl PositionIndexRuntime {
    /// Load one index and bind it to the active setup.
    ///
    /// `expected_index_sha256` may be supplied by deployment configuration to
    /// pin exact model bytes. Even when it is omitted, the computed hash remains
    /// available through [`Self::index_sha256`] for status/provenance output.
    pub(crate) fn load(
        path: &Path,
        expected_setup_id: &str,
        expected_setup_sha256: &str,
        expected_index_sha256: Option<&str>,
    ) -> Result<Self, String> {
        let kind = std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
            .and_then(|value| {
                value
                    .get("kind")
                    .and_then(|kind| kind.as_str())
                    .map(str::to_owned)
            });
        let (index, index_sha256) = if kind.as_deref() == Some("ruview.mmwave-position-index") {
            let (index, sha256) = load_mmwave_position_index(path)?;
            (RuntimeIndexArtifact::Mmwave(index), sha256)
        } else {
            let (index, sha256) = load_validated_position_index(path)?;
            (RuntimeIndexArtifact::Manual(index), sha256)
        };
        if index.setup_id() != expected_setup_id {
            return Err(format!(
                "position index setup_id {:?} does not match active setup {:?}",
                index.setup_id(),
                expected_setup_id
            ));
        }
        if index.setup_sha256() != expected_setup_sha256 {
            return Err("position index setup_sha256 does not match active setup".to_string());
        }
        if let Some(expected) = expected_index_sha256 {
            if index_sha256 != expected {
                return Err(
                    "position index SHA-256 does not match the configured exact index".to_string(),
                );
            }
        }
        Ok(Self {
            index,
            index_sha256,
        })
    }

    pub(crate) fn index_sha256(&self) -> &str {
        &self.index_sha256
    }

    pub(crate) fn setup_id(&self) -> &str {
        self.index.setup_id()
    }

    pub(crate) fn setup_sha256(&self) -> &str {
        self.index.setup_sha256()
    }

    fn capture_for_window(
        &self,
        started_at_unix_ns: u64,
        ended_at_unix_ns: u64,
        frames: Vec<RawCsiFrame>,
    ) -> PositionCapture {
        PositionCapture {
            recording_id: LIVE_RECORDING_ID.to_string(),
            setup_id: self.index.setup_id().to_string(),
            setup_sha256: self.index.setup_sha256().to_string(),
            server_version: self.index.server_version().to_string(),
            geometry: self.index.geometry().clone(),
            started_at_unix_ns,
            ended_at_unix_ns,
            frames,
        }
    }

    fn predict_window(
        &self,
        capture: &PositionCapture,
        window_start_unix_ns: u64,
    ) -> Result<WindowVote, String> {
        let block = extract_position_feature_window(
            capture,
            self.index.empty_reference(),
            window_start_unix_ns,
        )?;
        match self.index.predict_feature_block(&block)? {
            PositionFingerprintPrediction::Position { position, .. } => {
                Ok(WindowVote::Position(position))
            }
            PositionFingerprintPrediction::Unknown { .. } => Ok(WindowVote::Unknown),
            PositionFingerprintPrediction::Ambiguous { .. } => Ok(WindowVote::Ambiguous),
        }
    }
}

/// Bounded live CSI buffer and temporal position consensus.
#[derive(Debug)]
pub(crate) struct LivePositionTracker {
    runtime: Option<PositionIndexRuntime>,
    frames: VecDeque<RawCsiFrame>,
    max_buffered_frames: usize,
    fresh_buffer_started_at_unix_ns: Option<u64>,
    last_accepted_frame_unix_ns: Option<u64>,
    consensus: PositionConsensus,
    last_evaluation_unix_ns: Option<u64>,
    current: LivePositionState,
    last_error: Option<String>,
}

impl LivePositionTracker {
    pub(crate) fn new(runtime: Option<PositionIndexRuntime>) -> Self {
        Self::with_capacity(runtime, DEFAULT_MAX_BUFFERED_FRAMES)
    }

    fn with_capacity(runtime: Option<PositionIndexRuntime>, max_buffered_frames: usize) -> Self {
        assert!(
            max_buffered_frames > 0,
            "live position buffer capacity must be positive"
        );
        Self {
            current: if runtime.is_some() {
                LivePositionState::Insufficient
            } else {
                LivePositionState::Uncalibrated
            },
            runtime,
            frames: VecDeque::with_capacity(max_buffered_frames.min(256)),
            max_buffered_frames,
            fresh_buffer_started_at_unix_ns: None,
            last_accepted_frame_unix_ns: None,
            consensus: PositionConsensus::default(),
            last_evaluation_unix_ns: None,
            last_error: None,
        }
    }

    /// Replace or remove the loaded index. Buffered frames and consensus are
    /// always discarded so two setup/model identities cannot mix.
    pub(crate) fn install_runtime(&mut self, runtime: Option<PositionIndexRuntime>) {
        self.runtime = runtime;
        self.frames.clear();
        self.fresh_buffer_started_at_unix_ns = None;
        self.last_accepted_frame_unix_ns = None;
        self.consensus.reset();
        self.last_evaluation_unix_ns = None;
        self.last_error = None;
        self.current = if self.runtime.is_some() {
            LivePositionState::Insufficient
        } else {
            LivePositionState::Uncalibrated
        };
    }

    /// Insert one already decoded raw frame while preserving a hard memory
    /// bound. Invalid frames clear temporal confidence immediately.
    pub(crate) fn push_frame(&mut self, frame: RawCsiFrame) -> Result<(), String> {
        if let Err(error) = frame.validate() {
            let message = format!("invalid live CSI frame: {error}");
            self.fail_closed(LivePositionState::Insufficient, Some(message.clone()));
            return Err(message);
        }
        if !(1..=4).contains(&frame.rx_id) {
            let message = format!("live position received unexpected RX{}", frame.rx_id);
            self.fail_closed(LivePositionState::Insufficient, Some(message.clone()));
            return Err(message);
        }
        if self.frames.len() == self.max_buffered_frames {
            self.frames.pop_front();
        }
        self.fresh_buffer_started_at_unix_ns
            .get_or_insert(frame.host_timestamp_unix_ns);
        self.last_accepted_frame_unix_ns = Some(
            self.last_accepted_frame_unix_ns
                .map_or(frame.host_timestamp_unix_ns, |previous| {
                    previous.max(frame.host_timestamp_unix_ns)
                }),
        );
        self.frames.push_back(frame);
        Ok(())
    }

    /// Evaluate at most one newest three-second window per second.
    ///
    /// Calls made before the next one-second boundary return the current state
    /// unchanged. A missed cadence never triggers catch-up predictions; after
    /// two or more seconds the old consensus is discarded before evaluating
    /// one current window.
    pub(crate) fn tick(
        &mut self,
        now_unix_ns: u64,
        presence_gate: PresenceGate,
    ) -> LivePositionState {
        if self.runtime.is_none() {
            return self.fail_closed(LivePositionState::Uncalibrated, None);
        }
        if let Some(state) = self.apply_presence_gate(presence_gate) {
            return state;
        }

        if let Some(previous) = self.last_evaluation_unix_ns {
            let Some(elapsed) = now_unix_ns.checked_sub(previous) else {
                self.last_evaluation_unix_ns = None;
                return self.fail_closed(
                    LivePositionState::Stale,
                    Some("live position clock moved backwards".to_string()),
                );
            };
            if elapsed < WINDOW_STEP_NS {
                return self.current.clone();
            }
            if elapsed >= WINDOW_STEP_NS.saturating_mul(2) {
                self.consensus.reset();
            }
        }
        self.last_evaluation_unix_ns = Some(now_unix_ns);

        let Some(window_start_unix_ns) = now_unix_ns.checked_sub(WINDOW_NS) else {
            return self.fail_closed(
                LivePositionState::Insufficient,
                Some("live position timestamp is shorter than one feature window".to_string()),
            );
        };
        self.frames
            .retain(|frame| frame.host_timestamp_unix_ns >= window_start_unix_ns);

        if latest_eligible_frame_is_stale(&self.frames, now_unix_ns) {
            return self.fail_closed(LivePositionState::Stale, None);
        }

        if !self.has_complete_fresh_window(now_unix_ns) {
            return self.await_fresh_window();
        }

        let window_frames: Vec<RawCsiFrame> = self
            .frames
            .iter()
            .filter(|frame| {
                frame.host_timestamp_unix_ns >= window_start_unix_ns
                    && frame.host_timestamp_unix_ns < now_unix_ns
            })
            .cloned()
            .collect();
        let prediction = {
            let runtime = self
                .runtime
                .as_ref()
                .expect("runtime presence was checked before live extraction");
            let capture =
                runtime.capture_for_window(window_start_unix_ns, now_unix_ns, window_frames);
            runtime.predict_window(&capture, window_start_unix_ns)
        };
        self.apply_window_result(prediction)
    }

    pub(crate) fn current(&self) -> &LivePositionState {
        &self.current
    }

    /// Forward the current decision for an edge-vitals packet without running
    /// fingerprint inference. A raw-CSI gap clears any previous coordinates
    /// even when edge-vitals packets keep the ESP32 source itself online.
    pub(crate) fn expire_if_raw_stale(&mut self, now_unix_ns: u64) -> LivePositionState {
        if matches!(self.current, LivePositionState::Uncalibrated) {
            return self.current.clone();
        }
        let raw_is_stale = self.last_accepted_frame_unix_ns.is_none_or(|timestamp| {
            now_unix_ns
                .checked_sub(timestamp)
                .is_none_or(|age| age >= STALE_AFTER_NS)
        });
        if raw_is_stale {
            return self.fail_closed(LivePositionState::Stale, None);
        }
        self.current.clone()
    }

    /// Clear any previously published coordinates after a structurally or
    /// grid-ineligible live frame. A server without an installed index remains
    /// explicitly uncalibrated rather than implying partial model readiness.
    pub(crate) fn reject_input(&mut self, error: impl Into<String>) -> LivePositionState {
        let state = if self.runtime.is_some() {
            LivePositionState::Insufficient
        } else {
            LivePositionState::Uncalibrated
        };
        self.fail_closed(state, Some(error.into()))
    }

    pub(crate) fn buffered_frame_count(&self) -> usize {
        self.frames.len()
    }

    pub(crate) fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    pub(crate) fn runtime(&self) -> Option<&PositionIndexRuntime> {
        self.runtime.as_ref()
    }

    fn apply_presence_gate(&mut self, presence_gate: PresenceGate) -> Option<LivePositionState> {
        match presence_gate {
            PresenceGate::ReadyPresent => None,
            PresenceGate::ReadyAbsent => Some(self.fail_closed(LivePositionState::Unknown, None)),
            PresenceGate::Uncalibrated => {
                Some(self.fail_closed(LivePositionState::Uncalibrated, None))
            }
            PresenceGate::Insufficient => {
                Some(self.fail_closed(LivePositionState::Insufficient, None))
            }
            PresenceGate::Stale => Some(self.fail_closed(LivePositionState::Stale, None)),
        }
    }

    fn has_complete_fresh_window(&self, now_unix_ns: u64) -> bool {
        self.fresh_buffer_started_at_unix_ns
            .is_some_and(|started_at| now_unix_ns.saturating_sub(started_at) >= WINDOW_NS)
    }

    /// Publish no coordinates while a complete post-reset window is collected.
    ///
    /// This is intentionally not a fail-closed transition: clearing the fresh
    /// frames on every one-second tick would prevent the three-second feature
    /// window from ever becoming complete.
    fn await_fresh_window(&mut self) -> LivePositionState {
        self.consensus.reset();
        self.current = LivePositionState::Insufficient;
        self.last_error = None;
        self.current.clone()
    }

    /// Discard all temporal evidence whenever an input or readiness boundary
    /// fails. A later decision must therefore use a complete, post-transition
    /// CSI window instead of reusing frames recorded under the previous state.
    fn fail_closed(
        &mut self,
        state: LivePositionState,
        error: Option<String>,
    ) -> LivePositionState {
        self.frames.clear();
        self.fresh_buffer_started_at_unix_ns = None;
        self.consensus.reset();
        self.current = state;
        self.last_error = error;
        self.current.clone()
    }

    fn apply_window_result(&mut self, result: Result<WindowVote, String>) -> LivePositionState {
        match result {
            Ok(vote) => {
                self.last_error = None;
                self.current = self.consensus.observe(vote);
                self.current.clone()
            }
            Err(error) => self.fail_closed(LivePositionState::Insufficient, Some(error)),
        }
    }
}

fn latest_eligible_frame_is_stale(frames: &VecDeque<RawCsiFrame>, now_unix_ns: u64) -> bool {
    frames
        .iter()
        .filter(|frame| frame.host_timestamp_unix_ns < now_unix_ns)
        .map(|frame| frame.host_timestamp_unix_ns)
        .max()
        .is_none_or(|timestamp| now_unix_ns.saturating_sub(timestamp) >= STALE_AFTER_NS)
}

#[derive(Debug, Clone, PartialEq)]
enum WindowVote {
    Position(FingerprintPosition),
    Unknown,
    Ambiguous,
}

#[derive(Debug, Default)]
struct PositionConsensus {
    history: VecDeque<WindowVote>,
}

impl PositionConsensus {
    fn observe(&mut self, vote: WindowVote) -> LivePositionState {
        self.history.push_back(vote);
        if self.history.len() > CONSENSUS_WINDOW_COUNT {
            self.history.pop_front();
        }
        if let Some(position) = self.confirmed_position() {
            return LivePositionState::Position {
                point_id: position.id,
                coordinates_m: position.coordinates_m,
            };
        }
        match self.history.back() {
            Some(WindowVote::Unknown) => LivePositionState::Unknown,
            Some(WindowVote::Ambiguous) => LivePositionState::Ambiguous,
            Some(WindowVote::Position(_)) if self.history.len() < CONSENSUS_WINDOW_COUNT => {
                LivePositionState::Insufficient
            }
            Some(WindowVote::Position(_)) => LivePositionState::Ambiguous,
            None => LivePositionState::Insufficient,
        }
    }

    fn confirmed_position(&self) -> Option<FingerprintPosition> {
        if self.history.len() != CONSENSUS_WINDOW_COUNT {
            return None;
        }
        let mut counts = BTreeMap::<&str, (usize, &FingerprintPosition)>::new();
        for vote in &self.history {
            if let WindowVote::Position(position) = vote {
                let entry = counts.entry(position.id.as_str()).or_insert((0, position));
                entry.0 += 1;
            }
        }
        counts
            .into_values()
            .filter(|(count, _)| *count >= CONSENSUS_REQUIRED_COUNT)
            .max_by(|left, right| {
                left.0
                    .cmp(&right.0)
                    .then_with(|| right.1.id.cmp(&left.1.id))
            })
            .map(|(_, position)| position.clone())
    }

    fn reset(&mut self) {
        self.history.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::raw_csi_recording::{IqPair, RAW_CSI_SCHEMA_VERSION};

    fn position(id: &str, x: f64, z: f64) -> FingerprintPosition {
        FingerprintPosition {
            id: id.to_string(),
            coordinates_m: [x, 0.0, z],
        }
    }

    fn frame(timestamp: u64, sequence: u32) -> RawCsiFrame {
        RawCsiFrame {
            schema_version: RAW_CSI_SCHEMA_VERSION,
            host_timestamp_unix_ns: timestamp,
            host_monotonic_ns: Some(timestamp),
            clock_epoch_id: Some("test-clock".to_string()),
            session_id: None,
            label: None,
            ground_truth: None,
            rx_id: 1,
            antenna_count: 1,
            subcarrier_count: 8,
            center_frequency_mhz: 2_437,
            sequence,
            rssi_dbm: -50,
            noise_floor_dbm: -92,
            ppdu_type: 0,
            flags: 0,
            mesh_timestamp_us: None,
            source_binding: None,
            iq_pairs: vec![IqPair { i: 20, q: 0 }; 8],
        }
    }

    #[test]
    fn four_of_five_confirms_but_three_of_five_does_not() {
        let p01 = position("P01", 0.75, 0.75);
        let p02 = position("P02", 2.01, 0.75);
        let mut consensus = PositionConsensus::default();
        let four_of_five = [
            p01.clone(),
            p01.clone(),
            p02.clone(),
            p01.clone(),
            p01.clone(),
        ];
        let mut state = LivePositionState::Insufficient;
        for point in four_of_five {
            state = consensus.observe(WindowVote::Position(point));
        }
        assert_eq!(
            state,
            LivePositionState::Position {
                point_id: "P01".to_string(),
                coordinates_m: [0.75, 0.0, 0.75],
            }
        );

        consensus.reset();
        let three_of_five = [p01.clone(), p01.clone(), p02.clone(), p01, p02];
        for point in three_of_five {
            state = consensus.observe(WindowVote::Position(point));
        }
        assert_eq!(state, LivePositionState::Ambiguous);
    }

    #[test]
    fn every_fail_closed_state_serializes_without_coordinates() {
        let states = [
            LivePositionState::Unknown,
            LivePositionState::Ambiguous,
            LivePositionState::Insufficient,
            LivePositionState::Uncalibrated,
            LivePositionState::Stale,
        ];
        for state in states {
            let encoded = serde_json::to_value(state).expect("serialize public state");
            assert!(encoded.get("point_id").is_none());
            assert!(encoded.get("coordinates_m").is_none());
        }
        let position = serde_json::to_value(LivePositionState::Position {
            point_id: "P09".to_string(),
            coordinates_m: [3.27, 0.0, 2.69],
        })
        .expect("serialize position");
        assert_eq!(position["state"], "position");
        assert_eq!(position["point_id"], "P09");
        assert!(position.get("coordinates_m").is_some());
    }

    #[test]
    fn reset_removes_previous_consensus() {
        let p01 = position("P01", 0.75, 0.75);
        let mut consensus = PositionConsensus::default();
        for _ in 0..4 {
            consensus.observe(WindowVote::Position(p01.clone()));
        }
        consensus.reset();
        let state = consensus.observe(WindowVote::Position(p01));
        assert_eq!(state, LivePositionState::Insufficient);
        assert_eq!(consensus.history.len(), 1);
    }

    #[test]
    fn buffer_is_bounded_and_empty_buffer_is_stale() {
        let mut tracker = LivePositionTracker::with_capacity(None, 3);
        for sequence in 0..5 {
            tracker
                .push_frame(frame(10_000_000_000 + u64::from(sequence), sequence))
                .expect("valid frame");
        }
        assert_eq!(tracker.buffered_frame_count(), 3);
        assert_eq!(
            tracker.tick(20_000_000_000, PresenceGate::ReadyPresent),
            LivePositionState::Uncalibrated
        );
        assert_eq!(tracker.buffered_frame_count(), 0);
        assert_eq!(tracker.fresh_buffer_started_at_unix_ns, None);

        assert!(latest_eligible_frame_is_stale(
            &VecDeque::new(),
            20_000_000_000
        ));
    }

    #[test]
    fn every_presence_failure_discards_pre_transition_frames() {
        let cases = [
            (PresenceGate::ReadyAbsent, LivePositionState::Unknown),
            (PresenceGate::Stale, LivePositionState::Stale),
            (PresenceGate::Uncalibrated, LivePositionState::Uncalibrated),
            (PresenceGate::Insufficient, LivePositionState::Insufficient),
        ];

        for (case_index, (gate, expected)) in cases.into_iter().enumerate() {
            let old_timestamp = 10_000_000_000 + case_index as u64;
            let new_timestamp = 20_000_000_000 + case_index as u64;
            let mut tracker = LivePositionTracker::new(None);
            tracker
                .push_frame(frame(old_timestamp, 1))
                .expect("valid pre-transition frame");
            tracker
                .consensus
                .observe(WindowVote::Position(position("P01", 0.75, 0.75)));
            tracker.current = LivePositionState::Position {
                point_id: "P01".to_string(),
                coordinates_m: [0.75, 0.0, 0.75],
            };

            assert_eq!(tracker.apply_presence_gate(gate), Some(expected));
            assert!(tracker.consensus.history.is_empty());
            assert_eq!(tracker.buffered_frame_count(), 0);
            assert_eq!(tracker.fresh_buffer_started_at_unix_ns, None);

            tracker
                .push_frame(frame(new_timestamp, 2))
                .expect("valid post-transition frame");
            assert_eq!(tracker.buffered_frame_count(), 1);
            assert_eq!(
                tracker
                    .frames
                    .front()
                    .expect("only post-transition frame remains")
                    .host_timestamp_unix_ns,
                new_timestamp
            );
            assert_eq!(tracker.fresh_buffer_started_at_unix_ns, Some(new_timestamp));
        }
    }

    #[test]
    fn rejected_input_clears_old_coordinates_even_without_a_model() {
        let mut tracker = LivePositionTracker::new(None);
        tracker
            .push_frame(frame(10_000_000_000, 1))
            .expect("valid pre-rejection frame");
        tracker.current = LivePositionState::Position {
            point_id: "P01".to_string(),
            coordinates_m: [0.75, 0.0, 0.75],
        };

        assert_eq!(
            tracker.reject_input("wrong grid"),
            LivePositionState::Uncalibrated
        );
        assert!(tracker.consensus.history.is_empty());
        assert_eq!(tracker.buffered_frame_count(), 0);
        assert_eq!(tracker.fresh_buffer_started_at_unix_ns, None);
        assert_eq!(tracker.last_error(), Some("wrong grid"));
    }

    #[test]
    fn edge_vitals_forward_expires_coordinates_when_raw_csi_is_stale() {
        let timestamp = 10_000_000_000;
        let mut tracker = LivePositionTracker::new(None);
        tracker.push_frame(frame(timestamp, 1)).unwrap();
        tracker.current = LivePositionState::Position {
            point_id: "P01".to_string(),
            coordinates_m: [0.75, 0.0, 0.75],
        };

        assert!(matches!(
            tracker.expire_if_raw_stale(timestamp + STALE_AFTER_NS - 1),
            LivePositionState::Position { .. }
        ));
        let expired = tracker.expire_if_raw_stale(timestamp + STALE_AFTER_NS);
        assert_eq!(expired, LivePositionState::Stale);
        assert!(tracker.frames.is_empty());
        let encoded = serde_json::to_value(expired).unwrap();
        assert!(encoded.get("point_id").is_none());
        assert!(encoded.get("coordinates_m").is_none());
    }

    #[test]
    fn edge_vitals_forward_never_runs_or_invents_a_position() {
        let timestamp = 20_000_000_000;
        let mut tracker = LivePositionTracker::new(None);
        tracker.push_frame(frame(timestamp, 1)).unwrap();

        assert_eq!(
            tracker.expire_if_raw_stale(timestamp + 1),
            LivePositionState::Uncalibrated
        );
        assert!(tracker.consensus.history.is_empty());
    }

    #[test]
    fn invalid_frame_discards_preceding_valid_frames() {
        let mut tracker = LivePositionTracker::new(None);
        tracker
            .push_frame(frame(10_000_000_000, 1))
            .expect("valid pre-error frame");
        let mut invalid = frame(11_000_000_000, 2);
        invalid.iq_pairs.pop();

        let error = tracker
            .push_frame(invalid)
            .expect_err("invalid frame must fail closed");

        assert!(error.contains("I/Q pairs"));
        assert_eq!(tracker.current(), &LivePositionState::Insufficient);
        assert_eq!(tracker.buffered_frame_count(), 0);
        assert_eq!(tracker.fresh_buffer_started_at_unix_ns, None);
    }

    #[test]
    fn extraction_or_grid_error_maps_to_insufficient_and_discards_frames() {
        let mut tracker = LivePositionTracker::new(None);
        tracker
            .push_frame(frame(10_000_000_000, 1))
            .expect("valid pre-error frame");
        for _ in 0..4 {
            tracker
                .consensus
                .observe(WindowVote::Position(position("P01", 0.75, 0.75)));
        }
        let state =
            tracker.apply_window_result(Err("rejected position window: MixedGrid RX2".to_string()));
        assert_eq!(state, LivePositionState::Insufficient);
        assert!(tracker.consensus.history.is_empty());
        assert_eq!(tracker.buffered_frame_count(), 0);
        assert_eq!(tracker.fresh_buffer_started_at_unix_ns, None);
        assert!(tracker
            .last_error()
            .expect("diagnostic retained")
            .contains("MixedGrid"));
    }

    #[test]
    fn fresh_window_warmup_preserves_only_post_reset_frames() {
        let mut tracker = LivePositionTracker::new(None);
        tracker
            .push_frame(frame(10_000_000_000, 1))
            .expect("valid pre-reset frame");
        tracker.fail_closed(LivePositionState::Stale, None);

        let fresh_timestamp = 20_000_000_000;
        tracker
            .push_frame(frame(fresh_timestamp, 2))
            .expect("valid post-reset frame");
        assert!(!tracker.has_complete_fresh_window(fresh_timestamp + WINDOW_NS - 1));
        assert_eq!(
            tracker.await_fresh_window(),
            LivePositionState::Insufficient
        );
        assert_eq!(tracker.buffered_frame_count(), 1);
        assert_eq!(
            tracker
                .frames
                .front()
                .expect("warmup frame remains buffered")
                .host_timestamp_unix_ns,
            fresh_timestamp
        );
        assert!(tracker.has_complete_fresh_window(fresh_timestamp + WINDOW_NS));
    }
}

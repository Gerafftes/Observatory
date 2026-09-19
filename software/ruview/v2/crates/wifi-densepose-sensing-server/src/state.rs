//! Shared runtime state for the sensing-server binary.

use super::*;

pub(super) struct NodeState {
    pub(crate) frame_history: VecDeque<Vec<f64>>,
    pub(super) smoothed_person_score: f64,
    pub(crate) prev_person_count: usize,
    pub(super) smoothed_motion: f64,
    pub(super) latest_raw_motion: f64,
    pub(super) motion_confidence: f64,
    pub(super) current_motion_level: String,
    pub(super) debounce_counter: u32,
    pub(super) debounce_candidate: String,
    pub(super) baseline_motion: f64,
    pub(super) baseline_frames: u64,
    /// Experimental D5 empty-room reference and rolling still-presence evidence.
    pub(super) d5_presence: d5_presence::NodePresenceState,
    /// D6 compares the gain-normalized CSI shape with the empty-room
    /// fingerprint. Unlike D5's motion score it remains informative after a
    /// person has stopped moving and also provides the per-link anomaly weight
    /// used by coarse localization.
    pub(super) d6_fingerprint: d6_fingerprint::NodeFingerprintState,
    /// Frames rejected from the current empty-room calibration because D4
    /// observed movement.
    pub(super) calibration_motion_rejected_frames: u64,
    pub(super) smoothed_hr: f64,
    pub(super) smoothed_br: f64,
    pub(super) smoothed_hr_conf: f64,
    pub(super) smoothed_br_conf: f64,
    pub(super) hr_buffer: VecDeque<f64>,
    pub(super) br_buffer: VecDeque<f64>,
    pub(super) rssi_history: VecDeque<f64>,
    pub(super) vital_detector: VitalSignDetector,
    pub(super) latest_vitals: VitalSigns,
    pub(crate) last_frame_time: Option<std::time::Instant>,
    /// Most recent semantically valid TX-source trailer. The private identity
    /// is retained only to compare RX1-RX4 during discovery and is never
    /// serialized or logged.
    pub(super) source_binding_observation: Option<SourceBindingObservation>,
    /// Structurally valid, source-attested CSI frames excluded because they did
    /// not use this receiver's selected fingerprint grid.
    pub(super) skipped_grid_frames: u64,
    /// Mesh-aligned timestamp for the latest accepted CSI frame, recovered from
    /// ADR-110 sync packets when available. Multistatic fusion uses this instead
    /// of UDP arrival time so host/network jitter does not look like sensor
    /// clock spread.
    pub(crate) latest_frame_mesh_time_us: Option<u64>,
    pub(super) edge_vitals: Option<Esp32VitalsPacket>,
    /// ADR-110 §A0.12: Latest sync packet received from this node. When a
    /// CSI frame arrives with byte 19 bit 4 set (`adr018_flags.ieee802154_sync_valid`),
    /// the host can recover a mesh-aligned timestamp via
    /// `latest_sync.epoch_us + (now_local - latest_sync.local_us)`.
    pub(super) latest_sync: Option<wifi_densepose_hardware::SyncPacket>,
    /// Last time a sync packet from this node was received (for staleness).
    pub(super) latest_sync_at: Option<std::time::Instant>,
    /// Last valid sync packet used as the sequence/time anchor for the node's
    /// CSI callback clock
    /// estimation. This stays independent of `latest_sync`, because an invalid
    /// sync packet must remain visible diagnostically without becoming a rate
    /// anchor.
    pub(super) csi_fps_sync_anchor: Option<SyncRateAnchor>,
    /// EMA-tracked node-local CSI sequence-clock rate. Firmware increments the
    /// sequence before its UDP send gate, so this is not a shared transmitter
    /// FPS. The input is `sync_sequence_delta / sync_host_receive_time_delta`,
    /// making it insensitive to CSI datagrams dropped after serialization.
    pub(super) csi_fps_ema: f64,
    /// Number of valid sync-to-sync rate samples observed.
    pub(super) csi_fps_samples: u32,
    /// Latest accepted CSI sequence and diagnostic gap counters.
    pub(super) latest_sequence: Option<u32>,
    pub(super) accepted_csi_frames: u64,
    pub(super) inferred_lost_frames: u64,
    pub(super) sequence_observations: u64,
    /// Process-monotonic host arrival time for the latest accepted live frame.
    /// Offline replay leaves this unset and continues to use `last_frame_time`.
    pub(super) latest_host_monotonic_ns: Option<u64>,
    /// Bounded live-only queue used to select a time-coherent multistatic set.
    /// A single latest frame per receiver is insufficient under packet loss:
    /// it often combines four different transmitter events.
    pub(crate) fusion_frame_history: VecDeque<FusionFrameSample>,
    pub(super) latest_mesh_time_status: MeshTimeStatus,
    pub(super) mesh_time_reset_count: u64,
    /// Latest extracted features for cross-node fusion.
    pub(super) latest_features: Option<FeatureInfo>,
    // ── RuVector Phase 2: Temporal smoothing & coherence gating ──
    /// Previous frame's smoothed keypoint positions for EMA temporal smoothing.
    pub(super) prev_keypoints: Option<Vec<[f64; 3]>>,
    /// Rolling buffer of motion_energy values for coherence scoring (last 20 frames).
    pub(super) motion_energy_history: VecDeque<f64>,
    /// Coherence score [0.0, 1.0]: low variance in motion_energy = high coherence.
    pub(super) coherence_score: f64,
    /// ADR-084 Pass 3 cluster-Pi novelty sensor — per-node sketch bank of
    /// recent CSI feature vectors. Populated by `update_novelty` on each
    /// frame; left `None` to disable the sensor on a per-node basis.
    pub(super) feature_history:
        Option<wifi_densepose_signal::ruvsense::longitudinal::EmbeddingHistory>,
    /// Most recent novelty score in [0.0, 1.0] (0 = exact-match in bank,
    /// 1 = no overlap). Consumed by the model-wake gate downstream.
    pub(crate) last_novelty_score: Option<f32>,
    /// Full CSI identity used by this node's rolling windows and D6 reference.
    pub(super) active_grid: Option<CsiGridKey>,
    /// A replacement grid must remain stable for several consecutive frames
    /// before it can replace `active_grid`.
    pub(super) candidate_grid: Option<CsiGridKey>,
    pub(super) candidate_grid_hits: u8,
}

/// Default EMA alpha for temporal keypoint smoothing (RuVector Phase 2).
/// Lower = smoother (more history, less jitter). 0.15 balances responsiveness
/// with stability for WiFi CSI where per-frame noise is high.
pub(super) const TEMPORAL_EMA_ALPHA_DEFAULT: f64 = 0.15;
/// Reduced EMA alpha when coherence is low (trust measurements less).
pub(super) const TEMPORAL_EMA_ALPHA_LOW_COHERENCE: f64 = 0.05;
/// Coherence threshold below which we reduce EMA alpha.
pub(super) const COHERENCE_LOW_THRESHOLD: f64 = 0.3;
/// Maximum allowed bone-length change ratio between frames (20%).
pub(super) const MAX_BONE_CHANGE_RATIO: f64 = 0.20;
/// Number of motion_energy frames to track for coherence scoring.
pub(super) const COHERENCE_WINDOW: usize = 20;
/// ADR-084 Pass 3 — per-node novelty sketch dimension (56 subcarriers,
/// the dominant ESP32-S3 capture configuration).
pub(super) const NOVELTY_VECTOR_DIM: usize = 56;
/// ADR-084 Pass 3 — number of past sketches retained per-node for
/// novelty comparison. 64 frames ≈ 6.4 s at 10 Hz.
pub(super) const NOVELTY_HISTORY_CAPACITY: usize = 64;
/// About 10-20 seconds at the measured per-RX rates, enough to bridge packet
/// loss without allowing an unbounded live fusion backlog.
pub(super) const FUSION_FRAME_HISTORY_CAPACITY: usize = 128;
/// ADR-084 Pass 3 — feature-vector schema version. Bump on changes to
/// subcarrier ordering / normalisation so banks reject stale data.
pub(super) const NOVELTY_SKETCH_VERSION: u16 = 1;
/// Consecutive frames required before switching a node to another CSI grid.
pub(super) const GRID_SWITCH_CONFIRMATIONS: u8 = 8;

pub(super) const MESH_SYNC_MAX_AGE: std::time::Duration = std::time::Duration::from_secs(9);
pub(super) const MIN_SYNC_RATE_INTERVAL_SECONDS: f64 = 0.5;
pub(super) const MIN_PLAUSIBLE_CSI_FPS: f64 = 1.0;
pub(super) const MAX_PLAUSIBLE_CSI_FPS: f64 = 25.0;
pub(super) const MESH_SEQUENCE_SLACK_FRAMES: u32 = 32;

/// Convert one valid sync-to-sync interval into the node-local sequence rate.
/// Sequence distance, rather than received CSI packet count, makes the sample
/// robust to lost CSI datagrams. Very short intervals and rates outside the
/// firmware's 20 Hz ceiling plus tolerance fail closed.
pub(crate) fn sequence_rate_from_sync_interval(sequence_delta: u32, dt_sec: f64) -> Option<f64> {
    if sequence_delta == 0 || !dt_sec.is_finite() || dt_sec < MIN_SYNC_RATE_INTERVAL_SECONDS {
        return None;
    }
    let fps = f64::from(sequence_delta) / dt_sec;
    (MIN_PLAUSIBLE_CSI_FPS..=MAX_PLAUSIBLE_CSI_FPS)
        .contains(&fps)
        .then_some(fps)
}

#[cfg(test)]
mod fps_ema_tests {
    use super::sequence_rate_from_sync_interval;

    #[test]
    fn packet_loss_uses_sender_sequence_delta() {
        let fps = sequence_rate_from_sync_interval(40, 2.0).unwrap();
        assert!((fps - 20.0).abs() < f64::EPSILON);
    }

    #[test]
    fn different_node_sequence_rates_remain_distinct() {
        assert_eq!(sequence_rate_from_sync_interval(20, 2.0), Some(10.0));
        assert_eq!(sequence_rate_from_sync_interval(40, 2.0), Some(20.0));
    }

    #[test]
    fn nonpositive_or_too_short_interval_is_rejected() {
        assert!(sequence_rate_from_sync_interval(20, 0.0).is_none());
        assert!(sequence_rate_from_sync_interval(20, -1.0).is_none());
        assert!(sequence_rate_from_sync_interval(1, 0.01).is_none());
    }

    #[test]
    fn implausible_sender_rate_is_rejected() {
        assert!(sequence_rate_from_sync_interval(1000, 2.0).is_none());
    }
}

impl NodeState {
    /// ADR-110 §A0.12 timestamp recovery: given a CSI frame's node-local
    /// `esp_timer_get_time()` snapshot, return the mesh-aligned epoch
    /// computed from this node's most recent sync packet — or `None`
    /// if no sync has been received yet, or the last one is too stale
    /// (older than 3 × VALID_WINDOW_MS = 9 s, matching the firmware's own
    /// staleness gate).
    pub(crate) fn mesh_aligned_us(&self, local_at_frame_us: u64) -> Option<u64> {
        let sync = self.latest_sync.as_ref()?;
        let seen_at = self.latest_sync_at?;
        if !sync.flags.is_valid
            || std::time::Instant::now().saturating_duration_since(seen_at) > MESH_SYNC_MAX_AGE
        {
            return None;
        }
        Some(sync.apply_to_local(local_at_frame_us))
    }

    /// ADR-110 §A0.12 sequence-based mesh-time recovery for an in-flight
    /// ADR-018 CSI frame. The frame carries no `local_us` (the wire
    /// format has no slot), but it carries a sequence number that the
    /// sync packet's `sequence` high-water can be paired against. Uses
    /// Returns `None` until a valid sync-to-sync sequence-rate sample exists, or
    /// whenever sync flags, sync age, or sequence direction are unsafe.
    pub(crate) fn mesh_aligned_us_for_csi_frame(&self, frame_sequence: u32) -> Option<u64> {
        self.mesh_time_decision(frame_sequence).0
    }

    fn mesh_time_decision(&self, frame_sequence: u32) -> (Option<u64>, MeshTimeStatus) {
        let Some(sync) = self.latest_sync.as_ref() else {
            return (None, MeshTimeStatus::NoSync);
        };
        if !sync.flags.is_valid {
            return (None, MeshTimeStatus::InvalidSync);
        }
        let Some(seen_at) = self.latest_sync_at else {
            return (None, MeshTimeStatus::NoSync);
        };
        let sync_age = std::time::Instant::now().saturating_duration_since(seen_at);
        if sync_age > MESH_SYNC_MAX_AGE {
            return (None, MeshTimeStatus::StaleSync);
        }
        if self.csi_fps_samples == 0 {
            return (None, MeshTimeStatus::FpsWarmingUp);
        }
        if !(MIN_PLAUSIBLE_CSI_FPS..=MAX_PLAUSIBLE_CSI_FPS).contains(&self.csi_fps_ema) {
            return (None, MeshTimeStatus::ImplausibleFps);
        }
        if self
            .latest_sequence
            .is_some_and(|latest| frame_sequence.wrapping_sub(latest) >= 0x8000_0000)
        {
            return (None, MeshTimeStatus::SequenceRegression);
        }

        let sequence_delta = frame_sequence.wrapping_sub(sync.sequence);
        if sequence_delta >= 0x8000_0000 {
            return (None, MeshTimeStatus::SequenceBeforeSync);
        }
        let age_frames = (sync_age.as_secs_f64() * MAX_PLAUSIBLE_CSI_FPS).ceil() as u32;
        let max_forward_frames = age_frames.saturating_add(MESH_SEQUENCE_SLACK_FRAMES);
        if sequence_delta > max_forward_frames {
            return (None, MeshTimeStatus::SequenceJump);
        }

        (
            Some(sync.mesh_aligned_us_for_sequence(frame_sequence, self.csi_fps_ema)),
            MeshTimeStatus::Valid,
        )
    }

    fn reset_mesh_rate(&mut self, status: MeshTimeStatus) {
        self.csi_fps_ema = 0.0;
        self.csi_fps_samples = 0;
        self.latest_frame_mesh_time_us = None;
        // A rate reset marks a discontinuity (reboot or an implausible
        // sequence interval). Do not let pre-reset frames be paired with
        // post-reset arrivals in the host-time synchronizer.
        self.fusion_frame_history.clear();
        self.latest_mesh_time_status = status;
        self.mesh_time_reset_count = self.mesh_time_reset_count.saturating_add(1);
    }

    /// ADR-110 iter 32 — apply a freshly-decoded sync packet to this node.
    /// Overwrites `latest_sync` with the new packet and stamps
    /// `latest_sync_at` so the staleness gate in `mesh_aligned_us_for_csi_frame`
    /// can age it out after 9 s. Used by `udp_receiver_task` on every
    /// successful magic-dispatched sync datagram; extracted so the dispatch
    /// path is testable without spinning up the tokio UDP socket.
    pub(crate) fn apply_sync_packet(
        &mut self,
        pkt: wifi_densepose_hardware::SyncPacket,
        now: std::time::Instant,
    ) {
        self.latest_frame_mesh_time_us = None;
        if pkt.flags.is_valid {
            if let Some(anchor) = self.csi_fps_sync_anchor {
                let sequence_delta = pkt.sequence.wrapping_sub(anchor.sequence);
                let rebooted = pkt.local_us < anchor.local_us || sequence_delta >= 0x8000_0000;
                if rebooted {
                    self.reset_mesh_rate(MeshTimeStatus::NodeReboot);
                    self.latest_sequence = None;
                } else {
                    let dt_sec = now
                        .saturating_duration_since(anchor.observed_at)
                        .as_secs_f64();
                    match sequence_rate_from_sync_interval(sequence_delta, dt_sec) {
                        Some(sample) => {
                            self.csi_fps_ema = if self.csi_fps_samples == 0 {
                                sample
                            } else {
                                self.csi_fps_ema + (sample - self.csi_fps_ema) / 8.0
                            };
                            self.csi_fps_samples = self.csi_fps_samples.saturating_add(1);
                            self.latest_mesh_time_status = MeshTimeStatus::AwaitingCsiFrame;
                        }
                        None if dt_sec >= MIN_SYNC_RATE_INTERVAL_SECONDS => {
                            self.reset_mesh_rate(MeshTimeStatus::ImplausibleFps);
                        }
                        None => {
                            self.latest_mesh_time_status = MeshTimeStatus::FpsWarmingUp;
                        }
                    }
                }
            } else {
                self.latest_mesh_time_status = MeshTimeStatus::FpsWarmingUp;
            }
            self.csi_fps_sync_anchor = Some(SyncRateAnchor {
                sequence: pkt.sequence,
                local_us: pkt.local_us,
                observed_at: now,
            });
        } else {
            self.latest_mesh_time_status = MeshTimeStatus::InvalidSync;
        }
        self.latest_sync = Some(pkt);
        self.latest_sync_at = Some(now);
    }

    /// ADR-110 iter 30 — pure snapshot of this node's mesh-sync state.
    /// Returns `None` when no sync packet has been observed. Used by both
    /// the WebSocket broadcaster (iter 23) and the REST handlers (iter 29);
    /// extracted here so tests can build a `NodeState`, populate
    /// `latest_sync`, and assert the snapshot shape without spinning up
    /// the axum router.
    pub(crate) fn sync_snapshot(&self) -> Option<NodeSyncSnapshot> {
        let sync = self.latest_sync.as_ref()?;
        Some(NodeSyncSnapshot {
            offset_us: sync.local_minus_epoch_us(),
            is_leader: sync.flags.is_leader,
            is_valid: sync.flags.is_valid,
            smoothed: sync.flags.smoothed_used,
            sequence: sync.sequence,
            csi_fps_ema: self.csi_fps_ema,
            csi_fps_samples: self.csi_fps_samples,
            staleness_ms: self.latest_sync_at.map(|t| t.elapsed().as_millis() as u64),
        })
    }

    pub(crate) fn observe_csi_frame_arrival(&mut self, now: std::time::Instant) {
        self.last_frame_time = Some(now);
    }

    pub(crate) fn observe_accepted_csi_frame(
        &mut self,
        frame_sequence: u32,
        now: std::time::Instant,
    ) {
        self.accepted_csi_frames = self.accepted_csi_frames.saturating_add(1);
        let mut sequence_regressed = false;
        if let Some(previous) = self.latest_sequence {
            let delta = frame_sequence.wrapping_sub(previous);
            if (1..0x8000_0000).contains(&delta) {
                self.inferred_lost_frames = self
                    .inferred_lost_frames
                    .saturating_add(u64::from(delta.saturating_sub(1)));
                self.sequence_observations = self.sequence_observations.saturating_add(1);
            } else if delta >= 0x8000_0000 {
                sequence_regressed = true;
            }
        }
        if !sequence_regressed {
            self.latest_sequence = Some(frame_sequence);
        }
        self.observe_csi_frame_arrival(now);
        if sequence_regressed {
            self.latest_frame_mesh_time_us = None;
            // The regressing frame is not a safe continuation of this node's
            // timeline. Drop the queued pre-regression candidates so a later
            // host-coherent quartet cannot bridge the discontinuity.
            self.fusion_frame_history.clear();
            self.latest_mesh_time_status = MeshTimeStatus::SequenceRegression;
        } else {
            let (timestamp, status) = self.mesh_time_decision(frame_sequence);
            self.latest_frame_mesh_time_us = timestamp;
            self.latest_mesh_time_status = status;
        }
    }

    pub(crate) fn observe_live_fusion_frame(&mut self, amplitude: &[f64], host_monotonic_ns: u64) {
        self.latest_host_monotonic_ns = Some(host_monotonic_ns);
        if self.latest_mesh_time_status == MeshTimeStatus::SequenceRegression {
            // Keep the frame available to per-node diagnostics/classification,
            // but never admit the regressing sequence into the fusion queue.
            return;
        }
        self.fusion_frame_history.push_back(FusionFrameSample {
            amplitude: amplitude.to_vec(),
            host_monotonic_us: host_monotonic_ns / 1_000,
            mesh_timestamp_us: self.latest_frame_mesh_time_us,
        });
        if self.fusion_frame_history.len() > FUSION_FRAME_HISTORY_CAPACITY {
            self.fusion_frame_history.pop_front();
        }
    }

    pub(super) fn observe_source_binding(&mut self, observation: Option<SourceBindingObservation>) {
        self.source_binding_observation = observation;
    }

    pub(super) fn invalidate_source_binding_attestation(&mut self) {
        self.source_binding_observation = None;
    }

    pub(crate) fn new() -> Self {
        Self {
            frame_history: VecDeque::new(),
            smoothed_person_score: 0.0,
            prev_person_count: 0,
            smoothed_motion: 0.0,
            latest_raw_motion: 0.0,
            motion_confidence: 0.0,
            current_motion_level: "absent".to_string(),
            debounce_counter: 0,
            debounce_candidate: "absent".to_string(),
            baseline_motion: 0.0,
            baseline_frames: 0,
            d5_presence: d5_presence::NodePresenceState::default(),
            d6_fingerprint: d6_fingerprint::NodeFingerprintState::default(),
            calibration_motion_rejected_frames: 0,
            smoothed_hr: 0.0,
            smoothed_br: 0.0,
            smoothed_hr_conf: 0.0,
            smoothed_br_conf: 0.0,
            hr_buffer: VecDeque::with_capacity(8),
            br_buffer: VecDeque::with_capacity(8),
            rssi_history: VecDeque::new(),
            vital_detector: VitalSignDetector::new(10.0),
            latest_vitals: VitalSigns::default(),
            last_frame_time: None,
            source_binding_observation: None,
            skipped_grid_frames: 0,
            latest_frame_mesh_time_us: None,
            edge_vitals: None,
            latest_sync: None,
            latest_sync_at: None,
            csi_fps_sync_anchor: None,
            csi_fps_ema: 0.0,
            csi_fps_samples: 0,
            latest_sequence: None,
            accepted_csi_frames: 0,
            inferred_lost_frames: 0,
            sequence_observations: 0,
            latest_host_monotonic_ns: None,
            fusion_frame_history: VecDeque::with_capacity(FUSION_FRAME_HISTORY_CAPACITY),
            latest_mesh_time_status: MeshTimeStatus::NoSync,
            mesh_time_reset_count: 0,
            latest_features: None,
            prev_keypoints: None,
            motion_energy_history: VecDeque::with_capacity(COHERENCE_WINDOW),
            coherence_score: 1.0, // assume stable initially
            feature_history: Some(
                wifi_densepose_signal::ruvsense::longitudinal::EmbeddingHistory::with_sketch(
                    NOVELTY_VECTOR_DIM,
                    NOVELTY_HISTORY_CAPACITY,
                    NOVELTY_SKETCH_VERSION,
                ),
            ),
            last_novelty_score: None,
            active_grid: None,
            candidate_grid: None,
            candidate_grid_hits: 0,
        }
    }

    pub(super) fn new_with_calibration(
        node_id: u8,
        bundle: Option<&calibration_persistence::CalibrationBundle>,
    ) -> Self {
        let mut state = Self::new();
        if let Some(calibration) = bundle.and_then(|bundle| bundle.node(node_id)) {
            if let Some(reference) = calibration.d5 {
                state.d5_presence.install_reference(reference);
            }
            if let Some(reference) = &calibration.d6 {
                state.d6_fingerprint.install_reference(reference.clone());
            }
        }
        state
    }

    /// ADR-110 / issue #1005 grid gate: decide whether a frame on `grid`
    /// may enter this node's feature path, and update `active_grid`.
    ///
    /// Returns `true` to accept. A different grid must be observed for
    /// `GRID_SWITCH_CONFIRMATIONS` consecutive frames before it replaces the
    /// active grid. The rolling history and motion baseline are then cleared,
    /// so symbol grids are never mixed while occasional outliers cannot poison
    /// a stable ESP32-S3 or ESP32-C6 stream. Rejected arrivals still count for
    /// fps/liveness in the caller.
    pub(super) fn accept_grid(&mut self, grid: CsiGridKey) -> bool {
        match self.active_grid {
            None => {
                self.active_grid = Some(grid);
                self.candidate_grid = None;
                self.candidate_grid_hits = 0;
                true
            }
            Some(active) if active == grid => {
                self.candidate_grid = None;
                self.candidate_grid_hits = 0;
                true
            }
            Some(_) => {
                if self.candidate_grid == Some(grid) {
                    self.candidate_grid_hits = self.candidate_grid_hits.saturating_add(1);
                } else {
                    self.candidate_grid = Some(grid);
                    self.candidate_grid_hits = 1;
                }

                if self.candidate_grid_hits < GRID_SWITCH_CONFIRMATIONS {
                    return false;
                }

                self.active_grid = Some(grid);
                self.candidate_grid = None;
                self.candidate_grid_hits = 0;
                self.frame_history.clear();
                self.fusion_frame_history.clear();
                self.smoothed_motion = 0.0;
                self.latest_raw_motion = 0.0;
                self.motion_confidence = 0.0;
                self.current_motion_level = "absent".to_string();
                self.debounce_counter = 0;
                self.debounce_candidate = "absent".to_string();
                self.baseline_motion = 0.0;
                self.baseline_frames = 0;
                self.d5_presence.invalidate_reference();
                self.d6_fingerprint.invalidate_reference();
                true
            }
        }
    }

    /// Preserve fresh source attestation for every complete controlled-TX
    /// frame, while admitting only the selected CSI grid to sensing.
    pub(super) fn observe_validated_grid(
        &mut self,
        observation: SourceBindingObservation,
        grid: CsiGridKey,
        matches_sealed_grid: bool,
    ) -> bool {
        self.observe_source_binding(Some(observation));
        if matches_sealed_grid && self.accept_grid(grid) {
            return true;
        }
        self.skipped_grid_frames = self.skipped_grid_frames.saturating_add(1);
        false
    }

    /// ADR-084 cluster-Pi novelty step. Truncates / zero-pads the
    /// incoming amplitude vector to `NOVELTY_VECTOR_DIM`, scores its
    /// novelty against the per-node bank, then inserts it. The novelty
    /// score is computed *before* the insert so a frame doesn't see
    /// itself in the bank.
    pub(crate) fn update_novelty(&mut self, amplitudes: &[f64]) {
        let history = match &mut self.feature_history {
            Some(h) => h,
            None => return,
        };
        let mut feature: Vec<f32> = amplitudes
            .iter()
            .take(NOVELTY_VECTOR_DIM)
            .map(|&v| v as f32)
            .collect();
        feature.resize(NOVELTY_VECTOR_DIM, 0.0);

        // Score before insert so a query doesn't see itself.
        self.last_novelty_score = history.novelty(&feature);

        let _ = history.push(
            wifi_densepose_signal::ruvsense::longitudinal::EmbeddingEntry {
                person_id: 0,
                day_us: 0,
                embedding: feature,
            },
        );
    }

    /// Update the coherence score from the latest motion_energy value.
    ///
    /// Coherence is computed as 1.0 / (1.0 + running_variance) so that
    /// low motion-energy variance maps to high coherence ([0, 1]).
    fn update_coherence(&mut self, motion_energy: f64) {
        if self.motion_energy_history.len() >= COHERENCE_WINDOW {
            self.motion_energy_history.pop_front();
        }
        self.motion_energy_history.push_back(motion_energy);

        let n = self.motion_energy_history.len();
        if n < 2 {
            self.coherence_score = 1.0;
            return;
        }

        let mean: f64 = self.motion_energy_history.iter().sum::<f64>() / n as f64;
        let variance: f64 = self
            .motion_energy_history
            .iter()
            .map(|v| (v - mean) * (v - mean))
            .sum::<f64>()
            / (n - 1) as f64;

        // Map variance to [0, 1] coherence: higher variance = lower coherence.
        self.coherence_score = (1.0 / (1.0 + variance)).clamp(0.0, 1.0);
    }

    /// Choose the EMA alpha based on current coherence score.
    pub(super) fn ema_alpha(&self) -> f64 {
        if self.coherence_score < COHERENCE_LOW_THRESHOLD {
            TEMPORAL_EMA_ALPHA_LOW_COHERENCE
        } else {
            TEMPORAL_EMA_ALPHA_DEFAULT
        }
    }
}

#[cfg(test)]
mod grid_gate_tests {
    use super::*;
    use wifi_densepose_hardware::PpduType;

    const STABLE_GRID: CsiGridKey = (2437, 1, 128, PpduType::HtLegacy);
    const REPLACEMENT_GRID: CsiGridKey = (2437, 1, 192, PpduType::HtLegacy);

    fn complete_binding(now: std::time::Instant) -> SourceBindingObservation {
        SourceBindingObservation::validated(
            &raw_csi_recording::SourceBinding {
                trailer_version: raw_csi_recording::TX_SOURCE_BINDING_VERSION,
                flags: raw_csi_recording::SOURCE_BINDING_REQUIRED_FLAGS,
                scheme: raw_csi_recording::TX_SOURCE_BINDING_SCHEME.to_string(),
                tx_filter_sha256: "a".repeat(64),
            },
            now,
            false,
        )
        .unwrap()
    }

    fn source_binding(flags: u8, digest_character: char) -> raw_csi_recording::SourceBinding {
        raw_csi_recording::SourceBinding {
            trailer_version: raw_csi_recording::TX_SOURCE_BINDING_VERSION,
            flags,
            scheme: raw_csi_recording::TX_SOURCE_BINDING_SCHEME.to_string(),
            tx_filter_sha256: digest_character.to_string().repeat(64),
        }
    }

    fn raw_frame(subcarrier_count: u16, flags: u8) -> raw_csi_recording::RawCsiFrame {
        raw_csi_recording::RawCsiFrame {
            schema_version: raw_csi_recording::RAW_CSI_SCHEMA_VERSION,
            host_timestamp_unix_ns: 1,
            host_monotonic_ns: Some(1),
            clock_epoch_id: Some("test-clock".to_string()),
            session_id: None,
            label: None,
            ground_truth: None,
            rx_id: 1,
            antenna_count: 1,
            subcarrier_count,
            center_frequency_mhz: 2437,
            sequence: 1,
            rssi_dbm: -50,
            noise_floor_dbm: -90,
            ppdu_type: 0,
            flags,
            mesh_timestamp_us: None,
            source_binding: None,
            iq_pairs: Vec::new(),
        }
    }

    #[test]
    fn exact_grid_pin_filters_subcarrier_variants_but_ignores_sync_flag() {
        let pin: CsiGridPin = "2437,1,64,0,0x00".parse().unwrap();

        assert!(pin.matches(&raw_frame(64, 0)));
        assert!(pin.matches(&raw_frame(64, raw_csi_recording::TRANSIENT_SYNC_FLAG)));
        assert!(!pin.matches(&raw_frame(128, 0)));
    }

    #[test]
    fn grid_pin_rejects_incomplete_or_transient_identities() {
        assert!("2437,1,64,0".parse::<CsiGridPin>().is_err());
        assert!("2437,1,64,9,0".parse::<CsiGridPin>().is_err());
        assert!("2437,1,64,0,0x10".parse::<CsiGridPin>().is_err());
    }

    #[test]
    fn accepts_the_active_grid_continuously() {
        let mut state = NodeState::new();

        for _ in 0..20 {
            assert!(state.accept_grid(STABLE_GRID));
        }
        assert_eq!(state.active_grid, Some(STABLE_GRID));
        assert_eq!(state.candidate_grid, None);
    }

    #[test]
    fn rejects_a_single_grid_outlier_without_poisoning_the_stream() {
        let mut state = NodeState::new();
        assert!(state.accept_grid(STABLE_GRID));

        assert!(!state.accept_grid(REPLACEMENT_GRID));
        assert_eq!(state.active_grid, Some(STABLE_GRID));
        assert!(state.accept_grid(STABLE_GRID));
        assert_eq!(state.candidate_grid, None);
        assert_eq!(state.candidate_grid_hits, 0);
    }

    #[test]
    fn valid_binding_stays_attested_when_an_off_grid_frame_is_filtered() {
        let now = std::time::Instant::now();
        let later = now + std::time::Duration::from_millis(50);
        let mut state = NodeState::new();

        assert!(state.observe_validated_grid(complete_binding(now), STABLE_GRID, true));
        assert!(!state.observe_validated_grid(complete_binding(later), REPLACEMENT_GRID, true,));

        let binding = state.source_binding_observation.as_ref().unwrap();
        assert!(binding.complete);
        assert_eq!(binding.observed_at, later);
        assert_eq!(state.active_grid, Some(STABLE_GRID));
        assert_eq!(state.skipped_grid_frames, 1);
    }

    #[test]
    fn sealed_off_grid_frame_refreshes_binding_without_selecting_its_grid() {
        let now = std::time::Instant::now();
        let mut state = NodeState::new();

        assert!(!state.observe_validated_grid(complete_binding(now), REPLACEMENT_GRID, false,));

        assert!(state.source_binding_observation.is_some());
        assert_eq!(state.active_grid, None);
        assert_eq!(state.skipped_grid_frames, 1);
    }

    #[test]
    fn missing_incomplete_and_malformed_bindings_remain_fatal() {
        let now = std::time::Instant::now();
        assert!(validated_complete_source_binding(None, now, false).is_err());

        let incomplete = source_binding(0, '0');
        assert!(validated_complete_source_binding(Some(&incomplete), now, false).is_err());

        let malformed = source_binding(
            raw_csi_recording::SOURCE_BINDING_FLAG_FILTER_CONFIGURED,
            'a',
        );
        assert!(validated_complete_source_binding(Some(&malformed), now, false).is_err());

        let complete = source_binding(raw_csi_recording::SOURCE_BINDING_REQUIRED_FLAGS, 'a');
        assert!(validated_complete_source_binding(Some(&complete), now, false).is_ok());
    }

    #[test]
    fn switches_after_a_sustained_replacement_grid() {
        let mut state = NodeState::new();
        assert!(state.accept_grid(STABLE_GRID));
        state.frame_history.push_back(vec![1.0, 2.0]);
        state.baseline_motion = 4.2;
        state.baseline_frames = 17;
        state
            .d5_presence
            .install_reference_for_test(0.02, d5_presence::ROBUST_SCALE_FLOOR);
        state
            .d6_fingerprint
            .install_reference_for_test(&[1.0, 2.0, 1.0, 2.0])
            .unwrap();

        for _ in 1..GRID_SWITCH_CONFIRMATIONS {
            assert!(!state.accept_grid(REPLACEMENT_GRID));
        }
        assert!(state.accept_grid(REPLACEMENT_GRID));

        assert_eq!(state.active_grid, Some(REPLACEMENT_GRID));
        assert!(state.frame_history.is_empty());
        assert_eq!(state.baseline_motion, 0.0);
        assert_eq!(state.baseline_frames, 0);
        assert!(!state.d5_presence.reference_ready());
        assert!(!state.d6_fingerprint.reference_ready());
    }

    #[test]
    fn active_grid_resets_a_partial_replacement_candidate() {
        let mut state = NodeState::new();
        assert!(state.accept_grid(STABLE_GRID));
        for _ in 0..3 {
            assert!(!state.accept_grid(REPLACEMENT_GRID));
        }

        assert!(state.accept_grid(STABLE_GRID));
        assert_eq!(state.candidate_grid, None);
        assert_eq!(state.candidate_grid_hits, 0);
    }

    #[test]
    fn frequency_and_antenna_layout_are_part_of_the_grid_identity() {
        let mut channel_change = NodeState::new();
        assert!(channel_change.accept_grid(STABLE_GRID));
        assert!(!channel_change.accept_grid((2462, STABLE_GRID.1, STABLE_GRID.2, STABLE_GRID.3,)));

        let mut antenna_change = NodeState::new();
        assert!(antenna_change.accept_grid(STABLE_GRID));
        assert!(!antenna_change.accept_grid((STABLE_GRID.0, 2, STABLE_GRID.2, STABLE_GRID.3,)));
    }
}

/// Per-node feature info for WebSocket broadcasts (multi-node support).

pub(super) const MMWAVE_NODE_DIAGNOSTICS_POLL_INTERVAL: Duration = Duration::from_secs(5);
pub(super) const MMWAVE_NODE_DIAGNOSTICS_MAX_AGE: Duration = Duration::from_secs(15);

#[derive(Debug, Default)]
pub(super) struct MmwaveNodeDiagnosticsCache {
    pub(super) last_success: Option<(
        std::time::Instant,
        mmwave_calibration::NodeDiagnosticsWindow,
    )>,
    pub(super) previous: Option<mmwave_calibration::NodeDiagnostics>,
    pub(super) last_error: Option<String>,
    pub(super) last_error_kind: Option<String>,
}

impl MmwaveNodeDiagnosticsCache {
    pub(super) fn record(&mut self, result: Result<mmwave_calibration::NodeDiagnostics, String>) {
        match result {
            Ok(diagnostics) => {
                let window = mmwave_calibration::NodeDiagnosticsWindow {
                    uart_bytes_delta: self.previous.as_ref().map_or(0, |previous| {
                        diagnostics
                            .uart_bytes_received
                            .saturating_sub(previous.uart_bytes_received)
                    }),
                    radar_frames_delta: self.previous.as_ref().map_or(0, |previous| {
                        diagnostics
                            .radar_frames_valid
                            .saturating_sub(previous.radar_frames_valid)
                    }),
                    udp_packets_sent_delta: self.previous.as_ref().map_or(0, |previous| {
                        diagnostics
                            .udp_packets_sent
                            .saturating_sub(previous.udp_packets_sent)
                    }),
                    udp_send_failures_delta: self.previous.as_ref().map_or(0, |previous| {
                        diagnostics
                            .udp_send_failures
                            .saturating_sub(previous.udp_send_failures)
                    }),
                    diagnostics: diagnostics.clone(),
                };
                self.previous = Some(diagnostics);
                self.last_success = Some((std::time::Instant::now(), window));
                self.last_error = None;
                self.last_error_kind = None;
            }
            Err(error) => {
                self.last_error_kind =
                    Some(mmwave_calibration::classify_node_status_error(&error).to_string());
                self.last_error = Some(error);
            }
        }
    }

    pub(super) fn status(
        &self,
        url_configured: bool,
        token_configured: bool,
    ) -> mmwave_calibration::NodeControlStatus {
        let last_success_age_ms = self
            .last_success
            .as_ref()
            .map(|(updated_at, _)| updated_at.elapsed().as_millis() as u64);
        let reachable = last_success_age_ms
            .map(|age| age <= MMWAVE_NODE_DIAGNOSTICS_MAX_AGE.as_millis() as u64)
            .or_else(|| self.last_error.as_ref().map(|_| false));
        mmwave_calibration::NodeControlStatus {
            url_configured,
            token_configured,
            reachable,
            last_success_age_ms,
            last_error_kind: self.last_error_kind.clone(),
            last_error: self.last_error.clone(),
        }
    }

    pub(super) fn snapshot(&self) -> Result<mmwave_calibration::NodeDiagnosticsWindow, String> {
        if let Some((updated_at, diagnostics)) = &self.last_success {
            if updated_at.elapsed() <= MMWAVE_NODE_DIAGNOSTICS_MAX_AGE {
                return Ok(diagnostics.clone());
            }
        }
        Err(self
            .last_error
            .clone()
            .unwrap_or_else(|| "mmWave node diagnostics are not available yet".to_string()))
    }
}

#[cfg(test)]
mod mmwave_node_diagnostics_cache_tests {
    use super::*;

    fn diagnostics() -> mmwave_calibration::NodeDiagnostics {
        mmwave_calibration::NodeDiagnostics {
            uart_bytes_received: 1,
            radar_frames_valid: 1,
            udp_packets_sent: 1,
            udp_send_failures: 0,
            ..Default::default()
        }
    }

    #[test]
    fn recent_success_survives_one_transient_poll_failure() {
        let mut cache = MmwaveNodeDiagnosticsCache::default();
        cache.record(Ok(diagnostics()));
        cache.record(Err("temporary timeout".to_string()));

        assert_eq!(cache.snapshot().unwrap().diagnostics.udp_packets_sent, 1);
    }

    #[test]
    fn cumulative_counters_are_evaluated_as_current_window_deltas() {
        let mut cache = MmwaveNodeDiagnosticsCache::default();
        cache.record(Ok(mmwave_calibration::NodeDiagnostics {
            uart_bytes_received: 10,
            radar_frames_valid: 10,
            udp_packets_sent: 10,
            udp_send_failures: 2_733,
            ..Default::default()
        }));
        assert_eq!(cache.snapshot().unwrap().udp_send_failures_delta, 0);

        cache.record(Ok(mmwave_calibration::NodeDiagnostics {
            uart_bytes_received: 20,
            radar_frames_valid: 20,
            udp_packets_sent: 20,
            udp_send_failures: 2_733,
            ..Default::default()
        }));
        let window = cache.snapshot().unwrap();
        assert_eq!(window.uart_bytes_delta, 10);
        assert_eq!(window.radar_frames_delta, 10);
        assert_eq!(window.udp_packets_sent_delta, 10);
        assert_eq!(window.udp_send_failures_delta, 0);

        cache.record(Ok(mmwave_calibration::NodeDiagnostics {
            uart_bytes_received: 30,
            radar_frames_valid: 30,
            udp_packets_sent: 30,
            udp_send_failures: 2_734,
            ..Default::default()
        }));
        assert_eq!(cache.snapshot().unwrap().udp_send_failures_delta, 1);
    }

    #[test]
    fn stale_success_fails_closed_with_the_latest_error() {
        let mut cache = MmwaveNodeDiagnosticsCache::default();
        cache.last_success = Some((
            std::time::Instant::now() - MMWAVE_NODE_DIAGNOSTICS_MAX_AGE - Duration::from_secs(1),
            mmwave_calibration::NodeDiagnosticsWindow {
                diagnostics: diagnostics(),
                uart_bytes_delta: 1,
                radar_frames_delta: 1,
                udp_packets_sent_delta: 1,
                udp_send_failures_delta: 0,
            },
        ));
        cache.record(Err("node unavailable".to_string()));

        assert_eq!(cache.snapshot().unwrap_err(), "node unavailable");
    }

    #[test]
    fn status_exposes_configuration_and_classified_poll_failure() {
        let mut cache = MmwaveNodeDiagnosticsCache::default();
        cache.record(Err(
            "mmWave node status request returned HTTP 401".to_string()
        ));

        let status = cache.status(true, false);
        assert!(status.url_configured);
        assert!(!status.token_configured);
        assert_eq!(status.reachable, Some(false));
        assert_eq!(status.last_error_kind.as_deref(), Some("http_error"));
        assert!(status.last_error.as_deref().unwrap().contains("HTTP 401"));
    }
}

pub(super) struct AppStateInner {
    pub(super) latest_update: Option<SensingUpdate>,
    pub(super) rssi_history: VecDeque<f64>,
    /// Circular buffer of recent CSI amplitude vectors for temporal analysis.
    /// Each entry is the full subcarrier amplitude vector for one frame.
    /// Capacity: FRAME_HISTORY_CAPACITY frames.
    pub(super) frame_history: VecDeque<Vec<f64>>,
    pub(super) tick: u64,
    pub(super) source: String,
    /// Optional transmitter marker position for the live room visualization.
    pub(super) tx_position: Option<[f64; 3]>,
    /// Optional physical room dimensions `[length, height, width]`.
    pub(super) room_dimensions: Option<[f64; 3]>,
    /// Validated setup seal governing runtime geometry and raw recordings.
    pub(super) position_setup: Option<Arc<position_setup::SealedPositionSetup>>,
    /// Explicit pre-setup grid filter. A sealed setup supersedes this mechanism
    /// and the CLI rejects configuring both at once.
    pub(super) csi_grid_pin: Option<CsiGridPin>,
    /// Independent mmWave teacher/reference state. It is never read by the
    /// WiFi position predictor.
    pub(super) mmwave: mmwave_calibration::MmwaveManager,
    /// Cached read-only ESP counters. Polling the ESP independently avoids
    /// coupling the one-second UI refresh cadence to the shared experiment WLAN.
    pub(super) mmwave_node_diagnostics: MmwaveNodeDiagnosticsCache,
    pub(super) mmwave_connection: mmwave_connection::ConnectionStatus,
    /// Fail-closed discrete live-position inference and temporal consensus.
    pub(super) live_position_tracker: position_live::LivePositionTracker,
    /// Instant of the last ESP32 UDP frame received (for offline detection).
    pub(super) last_esp32_frame: Option<std::time::Instant>,
    /// Instant of the last raw CSI frame accepted by the discrete position path.
    pub(super) last_raw_csi_frame: Option<std::time::Instant>,
    pub(super) tx: broadcast::Sender<String>,
    /// Private, server-local stream of lossless accepted ESP32 CSI frames.
    /// This is intentionally separate from the privacy-gated WebSocket output:
    /// recording and later offline validation need the exact I/Q samples.
    pub(super) raw_csi_tx: broadcast::Sender<RawCsiIngress>,
    // ADR-099 D2/D3/D4: real-time CSI introspection tap. Per-frame state +
    // a parallel broadcast topic (`/ws/introspection`) running alongside
    // (not replacing) the window-aggregated `tx` / `/ws/sensing` pipeline.
    pub(super) intro: wifi_densepose_sensing_server::introspection::IntrospectionState,
    pub(super) intro_tx: broadcast::Sender<String>,
    pub(super) total_detections: u64,
    pub(super) start_time: std::time::Instant,
    /// Vital sign detector (processes CSI frames to estimate HR/RR).
    pub(super) vital_detector: VitalSignDetector,
    /// Most recent vital sign reading for the REST endpoint.
    pub(super) latest_vitals: VitalSigns,
    /// RVF container info if a model was loaded via `--load-rvf`.
    pub(super) rvf_info: Option<RvfContainerInfo>,
    /// Path to save RVF container on shutdown (set via `--save-rvf`).
    pub(super) save_rvf_path: Option<PathBuf>,
    /// Progressive loader for a trained model (set via `--model`).
    pub(super) progressive_loader: Option<ProgressiveLoader>,
    /// Active SONA profile name.
    pub(super) active_sona_profile: Option<String>,
    /// Whether a trained model is loaded.
    pub(super) model_loaded: bool,
    /// Smoothed person count (EMA) for hysteresis — prevents frame-to-frame jumping.
    pub(super) smoothed_person_score: f64,
    /// Previous person count for hysteresis (asymmetric up/down thresholds).
    pub(super) prev_person_count: usize,
    // ── Motion smoothing & adaptive baseline (ADR-047 tuning) ────────────
    /// EMA-smoothed motion score (alpha ~0.15 for ~10 FPS → ~1s time constant).
    pub(super) smoothed_motion: f64,
    /// Current classification state for hysteresis debounce.
    pub(super) current_motion_level: String,
    /// How many consecutive frames the *raw* classification has agreed with a
    /// *candidate* new level.  State only changes after DEBOUNCE_FRAMES.
    pub(super) debounce_counter: u32,
    /// The candidate motion level that the debounce counter is tracking.
    pub(super) debounce_candidate: String,
    /// Adaptive baseline: EMA of motion score when room is "quiet" (low motion).
    /// Subtracted from raw score so slow environmental drift doesn't inflate readings.
    pub(super) baseline_motion: f64,
    /// Number of frames processed so far (for baseline warm-up).
    pub(super) baseline_frames: u64,
    // ── Vital signs smoothing ────────────────────────────────────────────
    /// EMA-smoothed heart rate (BPM).
    pub(super) smoothed_hr: f64,
    /// EMA-smoothed breathing rate (BPM).
    pub(super) smoothed_br: f64,
    /// EMA-smoothed HR confidence.
    pub(super) smoothed_hr_conf: f64,
    /// EMA-smoothed BR confidence.
    pub(super) smoothed_br_conf: f64,
    /// Median filter buffer for HR (last N raw values for outlier rejection).
    pub(super) hr_buffer: VecDeque<f64>,
    /// Median filter buffer for BR.
    pub(super) br_buffer: VecDeque<f64>,
    /// ADR-039: Latest edge vitals packet from ESP32.
    pub(super) edge_vitals: Option<Esp32VitalsPacket>,
    /// ADR-040: Latest WASM output packet from ESP32.
    pub(super) latest_wasm_events: Option<WasmOutputPacket>,
    // ── Model management fields ─────────────────────────────────────────────
    /// Discovered RVF model files from `data/models/`.
    pub(super) discovered_models: Vec<serde_json::Value>,
    /// ID of the currently loaded model, if any.
    pub(super) active_model_id: Option<String>,
    // ── Recording fields ────────────────────────────────────────────────────
    /// Metadata for recorded CSI data files.
    pub(super) recordings: Vec<serde_json::Value>,
    /// Serializes file creation, finalization, and deletion so a recording ID
    /// cannot be deleted while another request is starting or finalizing it.
    pub(super) recording_lifecycle: Arc<Mutex<()>>,
    /// Explicit recorder lifecycle. `recording_active` remains the hot-path
    /// producer gate; this phase also represents the finalization interval.
    pub(super) recording_phase: RecordingLifecyclePhase,
    /// Whether CSI recording is currently in progress.
    pub(super) recording_active: bool,
    /// When the current recording started.
    pub(super) recording_start_time: Option<std::time::Instant>,
    /// ID of the current recording (used for filename).
    pub(super) recording_current_id: Option<String>,
    /// Shutdown signal for the recording writer task.
    pub(super) recording_stop_tx: Option<tokio::sync::watch::Sender<bool>>,
    /// Completion result from the writer. Stop and shutdown must await this
    /// before reporting the recording as durable or allowing delete/reuse.
    pub(super) recording_done_rx: Option<tokio::sync::oneshot::Receiver<RecordingWriterResult>>,
    // ── Adaptive classifier (environment-tuned) ──────────────────────────
    /// Trained adaptive model (loaded from data/adaptive_model.json or trained at runtime).
    pub(super) adaptive_model: Option<adaptive_classifier::AdaptiveModel>,
    // ── Per-node state (issue #249) ─────────────────────────────────────
    /// Per-node sensing state for multi-node deployments.
    /// Keyed by `node_id` from the ESP32 frame header.
    pub(super) node_states: HashMap<u8, NodeState>,
    /// Experimental D5 calibration/fusion state. It remains uncalibrated until
    /// explicitly activated through the classification-calibration API.
    pub(super) d5_presence: d5_presence::PresenceFusionState,
    /// Active D5/D6 references. Keeping the bundle in memory also lets a node
    /// that appears after startup receive the restored reference immediately.
    pub(super) active_calibration_bundle: Option<Arc<calibration_persistence::CalibrationBundle>>,
    pub(super) active_calibration_source: Option<String>,
    pub(super) calibration_context: Option<calibration_persistence::CalibrationContext>,
    // ── Accuracy sprint: Kalman tracker, multistatic fusion, eigenvalue counting ──
    /// Global Kalman-based pose tracker for stable person IDs and smoothed keypoints.
    pub(super) pose_tracker: PoseTracker,
    /// Instant of last tracker update (for computing dt).
    pub(super) last_tracker_instant: Option<std::time::Instant>,
    /// Attention-weighted multi-node CSI fusion engine.
    pub(super) multistatic_fuser: MultistaticFuser,
    /// Governed trust-path bridge (ADR-135..146): runs the same live frames
    /// through the privacy/provenance/witness control plane. Does not alter
    /// person-count behavior; its trust state (witness, effective class,
    /// recalibration flag, error count) is recorded on the bridge itself and
    /// exposed via `GET /api/v1/status`, and a Restricted-class cycle strips
    /// per-node raw amplitudes from the live publish (review finding 1).
    pub(super) engine_bridge: engine_bridge::EngineBridge,
    /// SVD-based room field model for eigenvalue person counting (None until calibration).
    pub(super) field_model: Option<FieldModel>,
    // ── ADR-044 §5.2: adaptive rolling-p95 normalization ─────────────────────
    /// Rolling P95 of `FeatureInfo.variance` over the last ~30 s (600 frames @ 20 Hz).
    pub(crate) p95_variance: RollingP95,
    /// Rolling P95 of `FeatureInfo.motion_band_power` over the last ~30 s.
    pub(crate) p95_motion_band_power: RollingP95,
    /// Rolling P95 of `FeatureInfo.spectral_power` over the last ~30 s.
    pub(crate) p95_spectral_power: RollingP95,
    // ── ADR-044 §5.3: runtime-configurable dedup factor ───────────────────────
    /// Divisor for multi-node person-count deduplication (sum / factor).
    /// Default 3.0 (one body visible to ~3 nodes on average).
    /// Configurable at runtime via `POST /api/v1/config/dedup-factor` and
    /// `POST /api/v1/config/ground-truth`. Persisted across restarts.
    pub(crate) dedup_factor: f64,
    /// Data directory for persisting runtime config (parent of `firmware_dir`).
    pub(crate) data_dir: std::path::PathBuf,
    /// Optional local Observatory experiment catalogue. A database failure
    /// must not take the live sensing/read-only surface down with it.
    pub(super) experiment_store: Option<Arc<experiment::ExperimentStore>>,
    /// ADR-262 P3: the live RuField surface. Holds the dedicated ed25519 signer
    /// + a bounded ring of recent signed `FieldEvent`s + the `/ws/field`
    /// broadcast topic. The governed sensing cycle calls `emit()` on it once per
    /// cycle (joining `SensingUpdate` features/classification/signal_field with
    /// the `TrustedOutput` trust class); `/api/field` + `/ws/field` read it.
    /// Held behind its own `Arc<RwLock<_>>` so the additive field router can
    /// take it as state without re-locking `AppStateInner`.
    pub(super) field_surface: rufield_surface::FieldState,
}

/// If no ESP32 frame arrives within this duration, source reverts to offline.
pub(super) const ESP32_OFFLINE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
/// A sealed preflight may only trust TX-source evidence from the current live
/// stream. Kept equal to the capture runner's hard maximum.
pub(super) const SOURCE_BINDING_FRESHNESS_TIMEOUT: std::time::Duration =
    std::time::Duration::from_secs(2);
/// A measured position must not outlive its raw CSI input.
pub(super) const POSITION_RAW_STALE_TIMEOUT: std::time::Duration =
    std::time::Duration::from_secs(1);

impl AppStateInner {
    /// Return the effective data source, accounting for ESP32 frame timeout.
    /// If the source is "esp32" but no frame has arrived in 5 seconds, returns
    /// "esp32:offline" so the UI can distinguish active vs stale connections.
    /// Person count: eigenvalue-based if field model is calibrated, else heuristic.
    /// Uses global frame_history if populated, otherwise the freshest per-node history.
    pub(super) fn person_count(&self) -> usize {
        match self.field_model.as_ref() {
            Some(fm) => {
                // Prefer global frame_history (populated by wifi/simulate paths).
                // Fall back to freshest per-node history (populated by ESP32 paths).
                let history = if !self.frame_history.is_empty() {
                    &self.frame_history
                } else {
                    // Find the node with the most recent frame
                    self.node_states
                        .values()
                        .filter(|ns| !ns.frame_history.is_empty())
                        .max_by_key(|ns| ns.last_frame_time)
                        .map(|ns| &ns.frame_history)
                        .unwrap_or(&self.frame_history)
                };
                field_bridge::occupancy_or_fallback(
                    fm,
                    history,
                    self.smoothed_person_score,
                    self.prev_person_count,
                )
            }
            None => score_to_person_count(self.smoothed_person_score, self.prev_person_count),
        }
    }

    pub(super) fn effective_source(&self) -> String {
        if self.source == "esp32" {
            match self.last_esp32_frame {
                Some(last) if last.elapsed() <= ESP32_OFFLINE_TIMEOUT => {}
                // A configured ESP32 source is not evidence of a connected
                // device. Until the first real frame arrives the server is
                // explicitly offline, so the UI must not claim LIVE.
                _ => return "esp32:offline".to_string(),
            }
        }
        self.source.clone()
    }
}

/// Number of frames retained in `frame_history` for temporal analysis.
/// At 500 ms ticks this covers ~50 seconds; at 100 ms ticks ~10 seconds.
pub(super) const FRAME_HISTORY_CAPACITY: usize = 100;

pub(super) type SharedState = Arc<RwLock<AppStateInner>>;

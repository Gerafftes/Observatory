//! Bridge between sensing-server per-node state and the signal crate's
//! `MultistaticFuser` for attention-weighted CSI fusion across ESP32 nodes.
//!
//! This module converts the server's `NodeState` (f64 amplitude history) into
//! `MultiBandCsiFrame`s that the multistatic fusion pipeline expects, then
//! drives `MultistaticFuser::fuse` with a graceful fallback when fusion fails
//! (e.g. insufficient nodes or timestamp spread).

use std::collections::HashMap;
use std::time::{Duration, Instant};

use wifi_densepose_signal::hardware_norm::{CanonicalCsiFrame, HardwareType};
use wifi_densepose_signal::ruvsense::multiband::MultiBandCsiFrame;
use wifi_densepose_signal::ruvsense::multistatic::{
    FusedSensingFrame, MultistaticConfig, MultistaticFuser,
};

use super::{FusionFrameSample, NodeState};

/// Maximum age for a node frame to be considered active (10 seconds).
const STALE_THRESHOLD: Duration = Duration::from_secs(10);
const STALE_THRESHOLD_US: u64 = STALE_THRESHOLD.as_micros() as u64;

/// Default WiFi channel frequency (MHz) used for single-channel frames.
const DEFAULT_FREQ_MHZ: u32 = 2437; // Channel 6

const HOST_FALLBACK_REFERENCE_US: u64 = 1_000_000_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FusionTimeBasis {
    Mesh,
    HostMonotonic,
}

impl FusionTimeBasis {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mesh => "mesh",
            Self::HostMonotonic => "host_monotonic",
        }
    }
}

#[derive(Debug)]
pub(crate) struct CoherentFrameSelection {
    pub(crate) frames: Vec<MultiBandCsiFrame>,
    pub(crate) basis: FusionTimeBasis,
    pub(crate) host_spread_us: u64,
    pub(crate) mesh_spread_us: Option<u64>,
    pub(crate) selected_host_monotonic_us: HashMap<u8, u64>,
    pub(crate) selected_mesh_timestamp_us: HashMap<u8, Option<u64>>,
}

fn frame_from_sample(
    node_id: u8,
    sample: &FusionFrameSample,
    basis: FusionTimeBasis,
) -> MultiBandCsiFrame {
    let amplitude: Vec<f32> = sample.amplitude.iter().map(|&value| value as f32).collect();
    let phase = vec![0.0_f32; amplitude.len()];
    let timestamp_us = match basis {
        FusionTimeBasis::Mesh => sample
            .mesh_timestamp_us
            .expect("mesh basis requires a timestamp for every selected frame"),
        FusionTimeBasis::HostMonotonic => sample.host_monotonic_us,
    };

    MultiBandCsiFrame {
        node_id,
        timestamp_us,
        channel_frames: vec![CanonicalCsiFrame {
            amplitude,
            phase,
            hardware_type: HardwareType::Esp32S3,
        }],
        frequencies_mhz: vec![DEFAULT_FREQ_MHZ],
        coherence: 1.0,
    }
}

#[derive(Debug, Clone, Copy)]
struct HostWindow {
    earliest_candidate: usize,
    latest_candidate: usize,
    spread_us: u64,
}

fn host_window(candidates: &[(u8, Vec<&FusionFrameSample>)], indices: &[usize]) -> HostWindow {
    let mut earliest_candidate = 0;
    let mut latest_candidate = 0;
    let mut min_host_us = u64::MAX;
    let mut max_host_us = 0;

    for (candidate_index, ((_, frames), frame_index)) in candidates.iter().zip(indices).enumerate()
    {
        let host_us = frames[*frame_index].host_monotonic_us;
        if host_us < min_host_us {
            min_host_us = host_us;
            earliest_candidate = candidate_index;
        }
        if host_us >= max_host_us {
            max_host_us = host_us;
            latest_candidate = candidate_index;
        }
    }

    HostWindow {
        earliest_candidate,
        latest_candidate,
        spread_us: max_host_us.saturating_sub(min_host_us),
    }
}

fn build_selection(
    candidates: &[(u8, Vec<&FusionFrameSample>)],
    indices: &[usize],
    host_spread_us: u64,
    guard_interval_us: u64,
) -> CoherentFrameSelection {
    let selected: Vec<_> = candidates
        .iter()
        .zip(indices)
        .map(|((node_id, frames), frame_index)| (*node_id, frames[*frame_index]))
        .collect();
    let mesh_times: Option<Vec<u64>> = selected
        .iter()
        .map(|(_, sample)| sample.mesh_timestamp_us)
        .collect();
    let mesh_spread_us = mesh_times.as_ref().and_then(|timestamps| {
        let min = timestamps.iter().copied().min()?;
        let max = timestamps.iter().copied().max()?;
        Some(max.saturating_sub(min))
    });
    let basis = if mesh_spread_us.is_some_and(|spread| spread <= guard_interval_us) {
        FusionTimeBasis::Mesh
    } else {
        FusionTimeBasis::HostMonotonic
    };
    let frames = selected
        .iter()
        .map(|(node_id, sample)| frame_from_sample(*node_id, sample, basis))
        .collect();
    let selected_host_monotonic_us = selected
        .iter()
        .map(|(node_id, sample)| (*node_id, sample.host_monotonic_us))
        .collect();
    let selected_mesh_timestamp_us = selected
        .into_iter()
        .map(|(node_id, sample)| (node_id, sample.mesh_timestamp_us))
        .collect();

    CoherentFrameSelection {
        frames,
        basis,
        host_spread_us,
        mesh_spread_us,
        selected_host_monotonic_us,
        selected_mesh_timestamp_us,
    }
}

/// Select one disjoint, host-coherent frame per active receiver.
///
/// Receiver-local CSI sequences cannot identify a shared transmitter event,
/// and packet loss means each node's latest frame often belongs to a different
/// event. This approximate-time synchronizer walks the bounded per-node queues
/// until all host arrivals fit the existing hard guard. Mesh remains preferred
/// only when those same frames are also mesh-coherent; otherwise the complete
/// set uses host-monotonic time. No mixed clock domains and no wider guard.
pub(crate) fn select_coherent_frames(
    node_states: &HashMap<u8, NodeState>,
    consumed_host_monotonic_us: &HashMap<u8, u64>,
    guard_interval_us: u64,
) -> Option<CoherentFrameSelection> {
    let now = Instant::now();
    let mut candidates: Vec<(u8, Vec<&FusionFrameSample>)> = node_states
        .iter()
        .filter_map(|(&node_id, state)| {
            let last_frame_time = state.last_frame_time?;
            if now.saturating_duration_since(last_frame_time) > STALE_THRESHOLD {
                return None;
            }
            // A node can go offline and later return with its pre-outage
            // bounded queue still populated.  The node itself is fresh at
            // that point, but those old samples are not.  Use the same
            // process-monotonic host clock recorded on the latest accepted
            // frame to discard queue entries older than the active window.
            let latest_host_us = state.latest_host_monotonic_ns.map(|ns| ns / 1_000);
            let consumed_through = consumed_host_monotonic_us
                .get(&node_id)
                .copied()
                .unwrap_or(0);
            let frames: Vec<_> = state
                .fusion_frame_history
                .iter()
                .filter(|sample| {
                    sample.host_monotonic_us > consumed_through
                        && !sample.amplitude.is_empty()
                        && latest_host_us.is_none_or(|latest| {
                            latest.saturating_sub(sample.host_monotonic_us) <= STALE_THRESHOLD_US
                        })
                })
                .collect();
            Some((node_id, frames))
        })
        .collect();
    candidates.sort_by_key(|(node_id, _)| *node_id);

    if candidates.len() < 2 || candidates.iter().any(|(_, frames)| frames.is_empty()) {
        return None;
    }

    let mut indices = vec![0_usize; candidates.len()];
    loop {
        if candidates
            .iter()
            .zip(indices.iter())
            .any(|((_, frames), index)| *index >= frames.len())
        {
            return None;
        }

        let window = host_window(&candidates, &indices);
        if window.spread_us > guard_interval_us {
            indices[window.earliest_candidate] =
                indices[window.earliest_candidate].saturating_add(1);
            continue;
        }

        return Some(build_selection(
            &candidates,
            &indices,
            window.spread_us,
            guard_interval_us,
        ));
    }
}

/// Select the newest host-coherent set for the legacy person-count path.
///
/// The governed [`EngineBridge`] consumes queue entries one cycle at a time,
/// while the person-count path is a read-only snapshot. It therefore needs
/// the same coherence rule without consuming state: start at each RX's newest
/// sample and walk older only until the common host spread fits the existing
/// hard guard. A missing coherent set returns `None` instead of reviving the
/// old latest-only cross-event selection.
fn select_latest_coherent_frames(
    node_states: &HashMap<u8, NodeState>,
    guard_interval_us: u64,
) -> Option<CoherentFrameSelection> {
    let now = Instant::now();
    let mut candidates: Vec<(u8, Vec<&FusionFrameSample>)> = node_states
        .iter()
        .filter_map(|(&node_id, state)| {
            let last_frame_time = state.last_frame_time?;
            if now.saturating_duration_since(last_frame_time) > STALE_THRESHOLD {
                return None;
            }
            let latest_host_us = state.latest_host_monotonic_ns.map(|ns| ns / 1_000);
            let frames: Vec<_> = state
                .fusion_frame_history
                .iter()
                .filter(|sample| {
                    !sample.amplitude.is_empty()
                        && latest_host_us.is_none_or(|latest| {
                            latest.saturating_sub(sample.host_monotonic_us) <= STALE_THRESHOLD_US
                        })
                })
                .collect();
            Some((node_id, frames))
        })
        .collect();
    candidates.sort_by_key(|(node_id, _)| *node_id);

    if candidates.len() < 2 || candidates.iter().any(|(_, frames)| frames.is_empty()) {
        return None;
    }

    let mut indices: Vec<_> = candidates
        .iter()
        .map(|(_, frames)| frames.len().saturating_sub(1))
        .collect();
    loop {
        let window = host_window(&candidates, &indices);
        if window.spread_us > guard_interval_us {
            if indices[window.latest_candidate] == 0 {
                return None;
            }
            indices[window.latest_candidate] -= 1;
            continue;
        }

        return Some(build_selection(
            &candidates,
            &indices,
            window.spread_us,
            guard_interval_us,
        ));
    }
}

/// Convert a single `NodeState` into a `MultiBandCsiFrame` suitable for
/// multistatic fusion.
///
/// Returns `None` when the node has no frame history or no recorded
/// `last_frame_time`.
pub fn node_frame_from_state(node_id: u8, ns: &NodeState) -> Option<MultiBandCsiFrame> {
    let basis = if ns.latest_frame_mesh_time_us.is_some() {
        FusionTimeBasis::Mesh
    } else {
        FusionTimeBasis::HostMonotonic
    };
    node_frame_from_state_with_basis(
        node_id,
        ns,
        basis,
        Instant::now(),
        ns.latest_host_monotonic_ns.is_some(),
    )
}

fn node_frame_from_state_with_basis(
    node_id: u8,
    ns: &NodeState,
    basis: FusionTimeBasis,
    now: Instant,
    use_recorded_host_clock: bool,
) -> Option<MultiBandCsiFrame> {
    let last_time = ns.last_frame_time.as_ref()?;
    let latest = ns.frame_history.back()?;
    if latest.is_empty() {
        return None;
    }

    let amplitude: Vec<f32> = latest.iter().map(|&v| v as f32).collect();
    let n_sub = amplitude.len();
    let phase = vec![0.0_f32; n_sub];

    let timestamp_us = match basis {
        FusionTimeBasis::Mesh => ns.latest_frame_mesh_time_us?,
        FusionTimeBasis::HostMonotonic if use_recorded_host_clock => {
            ns.latest_host_monotonic_ns? / 1_000
        }
        FusionTimeBasis::HostMonotonic => HOST_FALLBACK_REFERENCE_US
            .saturating_sub(now.saturating_duration_since(*last_time).as_micros() as u64),
    };

    let canonical = CanonicalCsiFrame {
        amplitude,
        phase,
        hardware_type: HardwareType::Esp32S3,
    };

    Some(MultiBandCsiFrame {
        node_id,
        timestamp_us,
        channel_frames: vec![canonical],
        frequencies_mhz: vec![DEFAULT_FREQ_MHZ],
        coherence: 1.0, // single-channel, perfect self-coherence
    })
}

/// Collect `MultiBandCsiFrame`s from all active nodes.
///
/// A node is considered active if its `last_frame_time` is within
/// [`STALE_THRESHOLD`] of `now`.
pub fn node_frames_from_states(node_states: &HashMap<u8, NodeState>) -> Vec<MultiBandCsiFrame> {
    node_frames_from_states_with_basis(node_states).0
}

pub fn node_frames_from_states_with_basis(
    node_states: &HashMap<u8, NodeState>,
) -> (Vec<MultiBandCsiFrame>, Option<FusionTimeBasis>) {
    let now = Instant::now();
    let active: Vec<(u8, &NodeState)> = node_states
        .iter()
        .filter_map(|(&node_id, ns)| {
            ns.last_frame_time
                .filter(|time| now.saturating_duration_since(*time) <= STALE_THRESHOLD)
                .and_then(|_| ns.frame_history.back().filter(|frame| !frame.is_empty()))
                .map(|_| (node_id, ns))
        })
        .collect();
    if active.is_empty() {
        return (Vec::new(), None);
    }

    // A cycle must use one clock domain. Per-node fallback would combine the
    // mesh epoch with a process-relative host clock and manufacture a huge
    // timestamp spread. If any active node lacks a usable mesh timestamp, the
    // complete cycle falls back to host-monotonic arrival time.
    let basis = if active
        .iter()
        .all(|(_, ns)| ns.latest_frame_mesh_time_us.is_some())
    {
        FusionTimeBasis::Mesh
    } else {
        FusionTimeBasis::HostMonotonic
    };
    let use_recorded_host_clock = basis == FusionTimeBasis::HostMonotonic
        && active
            .iter()
            .all(|(_, ns)| ns.latest_host_monotonic_ns.is_some());

    let mut frames = Vec::with_capacity(active.len());
    for (node_id, ns) in active {
        if let Some(frame) =
            node_frame_from_state_with_basis(node_id, ns, basis, now, use_recorded_host_clock)
        {
            frames.push(frame);
        }
    }

    (frames, Some(basis))
}

/// Attempt multistatic fusion; fall back to max per-node person count on failure.
///
/// Returns `(fused_frame, fallback_person_count)`. When fusion succeeds,
/// `fallback_person_count` is `None` — the caller must compute count from
/// the fused amplitudes. On failure, returns the maximum per-node count
/// (not the sum, to avoid double-counting overlapping coverage).
pub fn fuse_or_fallback(
    fuser: &MultistaticFuser,
    node_states: &HashMap<u8, NodeState>,
    dedup_factor: f64,
) -> (Option<FusedSensingFrame>, Option<usize>) {
    // Once any raw CSI queue is live, never return to the latest-only frame
    // conversion: that was the cross-event source of the timestamp spread.
    // The person-count path is snapshot-based, so select the newest coherent
    // quartet without consuming the queue. Edge-vitals-only states retain the
    // legacy conversion because they have no timed CSI samples to align.
    let frames = if node_states
        .values()
        .any(|state| !state.fusion_frame_history.is_empty())
    {
        select_latest_coherent_frames(node_states, MultistaticConfig::default().guard_interval_us)
            .map(|selection| selection.frames)
            .unwrap_or_default()
    } else {
        node_frames_from_states(node_states)
    };
    if frames.is_empty() {
        return (None, Some(fallback_person_count(node_states, dedup_factor)));
    }

    match fuser.fuse(&frames) {
        Ok(fused) => {
            // Caller must compute person count from fused amplitudes.
            (Some(fused), None)
        }
        Err(e) => {
            tracing::debug!("Multistatic fusion failed ({e}), using per-node sum/dedup fallback");
            (None, Some(fallback_person_count(node_states, dedup_factor)))
        }
    }
}

fn fallback_person_count(node_states: &HashMap<u8, NodeState>, dedup_factor: f64) -> usize {
    // Sum per-node counts then divide by dedup_factor (assumed average
    // visibility per body across nodes). ADR-044 §5.1.
    // dedup_factor is runtime-configurable; default 3.0.
    let total: usize = node_states
        .values()
        .filter(|ns| {
            ns.last_frame_time
                .map(|t| t.elapsed() <= STALE_THRESHOLD)
                .unwrap_or(false)
        })
        .map(|ns| ns.prev_person_count)
        .sum();
    ((total as f64) / dedup_factor).ceil() as usize
}

/// Compute a person-presence score from fused amplitude data.
///
/// Uses the squared coefficient of variation (variance / mean^2) as a
/// lightweight proxy for body-induced CSI perturbation. A flat amplitude
/// vector (no person) yields a score near zero; a vector with high variance
/// relative to its mean (person moving) yields a score approaching 1.0.
pub fn compute_person_score_from_amplitudes(amplitudes: &[f32]) -> f64 {
    if amplitudes.is_empty() {
        return 0.0;
    }

    let n = amplitudes.len() as f64;
    let sum: f64 = amplitudes.iter().map(|&a| a as f64).sum();
    let mean = sum / n;

    let variance: f64 = amplitudes
        .iter()
        .map(|&a| {
            let diff = (a as f64) - mean;
            diff * diff
        })
        .sum::<f64>()
        / n;

    let score = variance / (mean * mean + 1e-10);
    score.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    /// Helper: build a minimal NodeState for testing. Uses `NodeState::new()`
    /// then mutates the `pub(crate)` fields the bridge needs.
    fn make_node_state(
        frame_history: VecDeque<Vec<f64>>,
        last_frame_time: Option<Instant>,
        prev_person_count: usize,
    ) -> NodeState {
        let mut ns = NodeState::new();
        ns.frame_history = frame_history;
        ns.last_frame_time = last_frame_time;
        ns.prev_person_count = prev_person_count;
        ns
    }

    #[test]
    fn test_node_frame_from_empty_state() {
        let ns = make_node_state(VecDeque::new(), Some(Instant::now()), 0);
        assert!(node_frame_from_state(1, &ns).is_none());
    }

    #[test]
    fn test_node_frame_from_state_no_time() {
        let mut history = VecDeque::new();
        history.push_back(vec![1.0, 2.0, 3.0]);
        let ns = make_node_state(history, None, 0);
        assert!(node_frame_from_state(1, &ns).is_none());
    }

    #[test]
    fn test_node_frame_conversion() {
        let mut history = VecDeque::new();
        history.push_back(vec![10.0, 20.0, 30.5]);
        let ns = make_node_state(history, Some(Instant::now()), 0);

        let frame = node_frame_from_state(42, &ns).expect("should produce a frame");
        assert_eq!(frame.node_id, 42);
        assert_eq!(frame.channel_frames.len(), 1);

        let ch = &frame.channel_frames[0];
        assert_eq!(ch.amplitude.len(), 3);
        assert!((ch.amplitude[0] - 10.0_f32).abs() < f32::EPSILON);
        assert!((ch.amplitude[1] - 20.0_f32).abs() < f32::EPSILON);
        assert!((ch.amplitude[2] - 30.5_f32).abs() < f32::EPSILON);
        // Phase should be all zeros
        assert!(ch.phase.iter().all(|&p| p == 0.0));
        assert_eq!(ch.hardware_type, HardwareType::Esp32S3);
    }

    #[test]
    fn test_node_frame_prefers_mesh_timestamp() {
        let mut history = VecDeque::new();
        history.push_back(vec![10.0, 20.0, 30.0]);
        let mut ns = make_node_state(history, Some(Instant::now()), 0);
        ns.latest_frame_mesh_time_us = Some(123_456);

        let frame = node_frame_from_state(7, &ns).expect("should produce a frame");
        assert_eq!(frame.timestamp_us, 123_456);
    }

    #[test]
    fn test_stale_node_excluded() {
        let mut states: HashMap<u8, NodeState> = HashMap::new();

        // Active node: frame just received
        let mut active_history = VecDeque::new();
        active_history.push_back(vec![1.0, 2.0]);
        states.insert(1, make_node_state(active_history, Some(Instant::now()), 1));

        // Stale node: frame 20 seconds ago
        let mut stale_history = VecDeque::new();
        stale_history.push_back(vec![3.0, 4.0]);
        let stale_time = Instant::now() - Duration::from_secs(20);
        states.insert(2, make_node_state(stale_history, Some(stale_time), 1));

        let frames = node_frames_from_states(&states);
        assert_eq!(frames.len(), 1, "stale node should be excluded");
        assert_eq!(frames[0].node_id, 1);
    }

    #[test]
    fn test_compute_person_score_empty() {
        assert!((compute_person_score_from_amplitudes(&[]) - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_compute_person_score_flat() {
        // Constant amplitude => variance = 0 => score ~ 0
        let flat = vec![5.0_f32; 64];
        let score = compute_person_score_from_amplitudes(&flat);
        assert!(
            score < 0.001,
            "flat signal should have near-zero score, got {score}"
        );
    }

    #[test]
    fn test_compute_person_score_varied() {
        // High variance relative to mean should produce a positive score
        let varied: Vec<f32> = (0..64)
            .map(|i| if i % 2 == 0 { 1.0 } else { 10.0 })
            .collect();
        let score = compute_person_score_from_amplitudes(&varied);
        assert!(
            score > 0.1,
            "varied signal should have positive score, got {score}"
        );
        assert!(score <= 1.0, "score should be clamped to 1.0, got {score}");
    }

    #[test]
    fn test_compute_person_score_clamped() {
        // Near-zero mean with non-zero variance => would blow up without clamp
        let vals = vec![0.0_f32, 0.0, 0.0, 0.001];
        let score = compute_person_score_from_amplitudes(&vals);
        assert!(score <= 1.0, "score must be clamped to 1.0");
    }

    #[test]
    fn test_fuse_uses_mesh_time_to_ignore_udp_arrival_jitter() {
        let mut states: HashMap<u8, NodeState> = HashMap::new();

        let mut first_history = VecDeque::new();
        first_history.push_back(vec![1.0; 64]);
        let mut first = make_node_state(first_history, Some(Instant::now()), 1);
        first.latest_frame_mesh_time_us = Some(1_000_000);
        states.insert(1, first);

        let mut second_history = VecDeque::new();
        second_history.push_back(vec![1.1; 64]);
        let mut second = make_node_state(
            second_history,
            Some(Instant::now() - Duration::from_millis(210)),
            1,
        );
        second.latest_frame_mesh_time_us = Some(1_018_000);
        states.insert(2, second);

        let fuser = MultistaticFuser::new();
        let (fused, fallback_count) = fuse_or_fallback(&fuser, &states, 3.0);
        assert!(
            fused.is_some(),
            "18 ms mesh spread should fuse even when UDP arrivals are 210 ms apart"
        );
        assert_eq!(fallback_count, None);
    }

    #[test]
    fn test_partial_mesh_availability_uses_host_time_for_every_node() {
        let now = Instant::now();
        let mut states: HashMap<u8, NodeState> = HashMap::new();

        let mut first_history = VecDeque::new();
        first_history.push_back(vec![1.0; 64]);
        let mut first = make_node_state(first_history, Some(now), 1);
        first.latest_frame_mesh_time_us = Some(8_000_000_000);
        states.insert(1, first);

        let mut second_history = VecDeque::new();
        second_history.push_back(vec![1.1; 64]);
        states.insert(
            2,
            make_node_state(second_history, Some(now - Duration::from_millis(10)), 1),
        );

        let frames = node_frames_from_states(&states);
        let min = frames.iter().map(|frame| frame.timestamp_us).min().unwrap();
        let max = frames.iter().map(|frame| frame.timestamp_us).max().unwrap();
        assert!(
            max - min <= 20_000,
            "a cycle must never mix mesh epoch and host-monotonic time"
        );
    }

    #[test]
    fn test_fuse_or_fallback_empty() {
        let fuser = MultistaticFuser::new();
        let states: HashMap<u8, NodeState> = HashMap::new();
        let (fused, count) = fuse_or_fallback(&fuser, &states, 3.0);
        assert!(fused.is_none());
        assert_eq!(count, Some(0));
    }

    fn timed_state(host_us: u64, mesh_us: Option<u64>) -> NodeState {
        let mut ns = NodeState::new();
        ns.last_frame_time = Some(Instant::now());
        ns.fusion_frame_history.push_back(FusionFrameSample {
            amplitude: vec![1.0; 64],
            host_monotonic_us: host_us,
            mesh_timestamp_us: mesh_us,
        });
        ns
    }

    fn timestamp_spread_us(frames: &[MultiBandCsiFrame]) -> Option<u64> {
        let min = frames.iter().map(|frame| frame.timestamp_us).min()?;
        let max = frames.iter().map(|frame| frame.timestamp_us).max()?;
        Some(max.saturating_sub(min))
    }

    #[test]
    fn coherent_host_quartet_uses_mesh_only_when_mesh_is_also_coherent() {
        let host_base = 1_000_000;
        let mesh_base = 8_000_000;
        let mut states = HashMap::new();
        states.insert(1, timed_state(host_base, Some(mesh_base)));
        states.insert(2, timed_state(host_base + 5_000, Some(mesh_base + 4_000)));
        states.insert(3, timed_state(host_base + 11_000, Some(mesh_base + 9_000)));
        states.insert(4, timed_state(host_base + 18_000, Some(mesh_base + 12_000)));

        let selection = select_coherent_frames(&states, &HashMap::new(), 60_000)
            .expect("four coherent receivers should be selected");

        assert_eq!(selection.basis, FusionTimeBasis::Mesh);
        assert_eq!(selection.host_spread_us, 18_000);
        assert_eq!(selection.mesh_spread_us, Some(12_000));
        assert_eq!(timestamp_spread_us(&selection.frames), Some(12_000));
        assert_eq!(selection.selected_host_monotonic_us[&4], host_base + 18_000);
        assert_eq!(
            selection.selected_mesh_timestamp_us[&4],
            Some(mesh_base + 12_000)
        );
    }

    #[test]
    fn mesh_divergence_falls_back_to_same_host_coherent_quartet() {
        let host_base = 1_000_000;
        let mesh_base = 8_000_000;
        let mut states = HashMap::new();
        states.insert(1, timed_state(host_base, Some(mesh_base)));
        states.insert(2, timed_state(host_base + 5_000, Some(mesh_base + 200_000)));
        states.insert(
            3,
            timed_state(host_base + 11_000, Some(mesh_base + 400_000)),
        );
        states.insert(
            4,
            timed_state(host_base + 18_000, Some(mesh_base + 600_000)),
        );

        let selection = select_coherent_frames(&states, &HashMap::new(), 60_000)
            .expect("host-coherent receivers must remain usable");

        assert_eq!(selection.basis, FusionTimeBasis::HostMonotonic);
        assert_eq!(selection.host_spread_us, 18_000);
        assert_eq!(selection.mesh_spread_us, Some(600_000));
        assert_eq!(timestamp_spread_us(&selection.frames), Some(18_000));
        assert_eq!(selection.selected_host_monotonic_us.len(), 4);
        assert_eq!(selection.selected_mesh_timestamp_us.len(), 4);
    }

    #[test]
    fn incoherent_host_frames_are_rejected_without_widening_guard() {
        let mut states = HashMap::new();
        states.insert(1, timed_state(1_000_000, Some(8_000_000)));
        states.insert(2, timed_state(1_070_000, Some(8_070_000)));
        states.insert(3, timed_state(1_140_000, Some(8_140_000)));
        states.insert(4, timed_state(1_210_000, Some(8_210_000)));

        assert!(select_coherent_frames(&states, &HashMap::new(), 60_000).is_none());
    }

    #[test]
    fn stale_queue_entries_are_not_replayed_after_an_outage() {
        let old_host_base = 1_000_000;
        let fresh_host_base = old_host_base + STALE_THRESHOLD_US + 1_000;
        let mut states = HashMap::new();
        for (node_id, host_offset_us, mesh_offset_us) in [
            (1, 0, 0),
            (2, 5_000, 4_000),
            (3, 11_000, 9_000),
            (4, 18_000, 12_000),
        ] {
            let mut state = timed_state(fresh_host_base + host_offset_us, None);
            state.latest_host_monotonic_ns = Some((fresh_host_base + host_offset_us) * 1_000);
            state.fusion_frame_history.clear();
            state.fusion_frame_history.push_back(FusionFrameSample {
                amplitude: vec![0.5; 64],
                host_monotonic_us: old_host_base + host_offset_us,
                mesh_timestamp_us: Some(7_000_000 + mesh_offset_us),
            });
            state.fusion_frame_history.push_back(FusionFrameSample {
                amplitude: vec![1.0; 64],
                host_monotonic_us: fresh_host_base + host_offset_us,
                mesh_timestamp_us: Some(8_000_000 + mesh_offset_us),
            });
            states.insert(node_id, state);
        }

        let selection = select_coherent_frames(&states, &HashMap::new(), 60_000)
            .expect("fresh quartet should remain selectable");
        assert_eq!(selection.host_spread_us, 18_000);
        assert_eq!(selection.mesh_spread_us, Some(12_000));
        assert!(selection
            .selected_host_monotonic_us
            .values()
            .all(|host_us| *host_us >= fresh_host_base));
        assert!(selection.frames.iter().all(|frame| {
            frame
                .channel_frames
                .first()
                .and_then(|channel| channel.amplitude.first())
                .is_some_and(|amplitude| (*amplitude - 1.0).abs() < f32::EPSILON)
        }));
    }

    #[test]
    fn live_person_fuser_uses_newest_coherent_queue_set() {
        let old_host_base = 1_000_000;
        let fresh_host_base = old_host_base + STALE_THRESHOLD_US + 1_000;
        let mut states = HashMap::new();
        for (node_id, host_offset_us, mesh_offset_us) in [
            (1, 0, 0),
            (2, 5_000, 4_000),
            (3, 11_000, 9_000),
            (4, 18_000, 12_000),
        ] {
            let mut state = timed_state(fresh_host_base + host_offset_us, None);
            state.latest_host_monotonic_ns = Some((fresh_host_base + host_offset_us) * 1_000);
            state.fusion_frame_history.clear();
            state.fusion_frame_history.push_back(FusionFrameSample {
                amplitude: vec![0.5; 64],
                host_monotonic_us: old_host_base + host_offset_us,
                mesh_timestamp_us: Some(7_000_000 + mesh_offset_us),
            });
            state.fusion_frame_history.push_back(FusionFrameSample {
                amplitude: vec![1.0; 64],
                host_monotonic_us: fresh_host_base + host_offset_us,
                mesh_timestamp_us: Some(8_000_000 + mesh_offset_us),
            });
            states.insert(node_id, state);
        }

        let fuser = MultistaticFuser::new();
        let (fused, fallback) = fuse_or_fallback(&fuser, &states, 3.0);
        let fused = fused.expect("newest coherent quartet should fuse");
        assert_eq!(fallback, None);
        assert_eq!(fused.node_frames.len(), 4);
        assert!(fused.node_frames.iter().all(|frame| {
            frame
                .channel_frames
                .first()
                .and_then(|channel| channel.amplitude.first())
                .is_some_and(|amplitude| (*amplitude - 1.0).abs() < f32::EPSILON)
        }));
    }
}

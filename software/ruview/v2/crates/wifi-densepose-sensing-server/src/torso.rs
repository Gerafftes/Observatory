//! Offline-only torso-v1 contracts and fall-candidate state tracking.
//!
//! This module deliberately produces no production fall alarm. A torso model
//! must carry complete, blind-test-approved metadata before it can enter live
//! inference, and even then alarm integration remains a separate opt-in gate.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const TORSO_KEYPOINT_NAMES: [&str; 4] =
    ["left_shoulder", "right_shoulder", "left_hip", "right_hip"];
pub const RX_ORDER: [&str; 4] = ["RX1", "RX2", "RX3", "RX4"];

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
pub struct TorsoKeypoint {
    pub x: f64,
    pub y: f64,
    pub confidence: f64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TorsoOutput {
    pub left_shoulder: TorsoKeypoint,
    pub right_shoulder: TorsoKeypoint,
    pub left_hip: TorsoKeypoint,
    pub right_hip: TorsoKeypoint,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TorsoKinematics {
    pub midpoint: [f64; 2],
    /// Signed angle in degrees from image-up. Positive tilts toward +x.
    pub angle_degrees: f64,
    pub height: f64,
    /// Positive values move downward in image coordinates.
    pub vertical_velocity: f64,
    pub confidence: f64,
}

impl TorsoOutput {
    pub fn kinematics(
        &self,
        previous: Option<([f64; 2], f64)>,
        timestamp_seconds: f64,
    ) -> Result<TorsoKinematics, String> {
        let points = [
            self.left_shoulder,
            self.right_shoulder,
            self.left_hip,
            self.right_hip,
        ];
        if points.iter().any(|point| {
            !point.x.is_finite()
                || !point.y.is_finite()
                || !point.confidence.is_finite()
                || !(0.0..=1.0).contains(&point.confidence)
        }) {
            return Err(
                "torso output contains non-finite coordinates or invalid confidence".into(),
            );
        }

        let shoulder = [
            (self.left_shoulder.x + self.right_shoulder.x) / 2.0,
            (self.left_shoulder.y + self.right_shoulder.y) / 2.0,
        ];
        let hip = [
            (self.left_hip.x + self.right_hip.x) / 2.0,
            (self.left_hip.y + self.right_hip.y) / 2.0,
        ];
        let midpoint = [(shoulder[0] + hip[0]) / 2.0, (shoulder[1] + hip[1]) / 2.0];
        let dx = shoulder[0] - hip[0];
        let dy = shoulder[1] - hip[1];
        let height = dx.hypot(dy);
        let angle_degrees = dx.atan2(-dy).to_degrees();
        let vertical_velocity = previous.map_or(0.0, |(previous_midpoint, previous_time)| {
            let elapsed = timestamp_seconds - previous_time;
            if elapsed > 0.0 {
                (midpoint[1] - previous_midpoint[1]) / elapsed
            } else {
                0.0
            }
        });
        let confidence = points.iter().map(|point| point.confidence).sum::<f64>() / 4.0;
        Ok(TorsoKinematics {
            midpoint,
            angle_degrees,
            height,
            vertical_velocity,
            confidence,
        })
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
pub struct FallThresholds {
    pub min_confidence: f64,
    pub rapid_descent_velocity: f64,
    pub floor_midpoint_y: f64,
    pub max_floor_height: f64,
    pub remains_down_seconds: f64,
    pub recovery_midpoint_y: f64,
    pub recovery_hold_seconds: f64,
}

impl FallThresholds {
    fn validate(self) -> Result<Self, String> {
        let values = [
            self.min_confidence,
            self.rapid_descent_velocity,
            self.floor_midpoint_y,
            self.max_floor_height,
            self.remains_down_seconds,
            self.recovery_midpoint_y,
            self.recovery_hold_seconds,
        ];
        if values
            .iter()
            .any(|value| !value.is_finite() || *value < 0.0)
        {
            return Err("fall thresholds must be finite non-negative values".into());
        }
        if !(0.0..=1.0).contains(&self.min_confidence) {
            return Err("min_confidence must be within [0, 1]".into());
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FallState {
    Upright,
    RapidDescent,
    FloorLevel,
    RemainsDown,
    Recovered,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct FallDecision {
    pub state: FallState,
    pub candidate_fall: bool,
    /// Hard-disabled for the new torso path pending separate alarm approval.
    pub fall_detected: bool,
    pub events_enabled: bool,
}

pub struct FallStateMachine {
    thresholds: FallThresholds,
    state: FallState,
    state_since: f64,
}

impl FallStateMachine {
    pub fn new(thresholds: FallThresholds) -> Result<Self, String> {
        Ok(Self {
            thresholds: thresholds.validate()?,
            state: FallState::Upright,
            state_since: 0.0,
        })
    }

    pub fn update(
        &mut self,
        kinematics: Option<TorsoKinematics>,
        timestamp_seconds: f64,
    ) -> FallDecision {
        let Some(value) =
            kinematics.filter(|value| value.confidence >= self.thresholds.min_confidence)
        else {
            return self.decision();
        };
        let floor_level = value.midpoint[1] >= self.thresholds.floor_midpoint_y
            && value.height <= self.thresholds.max_floor_height;
        let next = match self.state {
            FallState::Upright
                if value.vertical_velocity >= self.thresholds.rapid_descent_velocity =>
            {
                FallState::RapidDescent
            }
            FallState::RapidDescent if floor_level => FallState::FloorLevel,
            FallState::RapidDescent if value.midpoint[1] <= self.thresholds.recovery_midpoint_y => {
                FallState::Upright
            }
            FallState::FloorLevel
                if floor_level
                    && timestamp_seconds - self.state_since
                        >= self.thresholds.remains_down_seconds =>
            {
                FallState::RemainsDown
            }
            FallState::FloorLevel if !floor_level => FallState::Recovered,
            FallState::RemainsDown if value.midpoint[1] <= self.thresholds.recovery_midpoint_y => {
                FallState::Recovered
            }
            FallState::Recovered
                if timestamp_seconds - self.state_since
                    >= self.thresholds.recovery_hold_seconds =>
            {
                FallState::Upright
            }
            _ => self.state,
        };
        if next != self.state {
            self.state = next;
            self.state_since = timestamp_seconds;
        }
        self.decision()
    }

    fn decision(&self) -> FallDecision {
        FallDecision {
            state: self.state,
            candidate_fall: self.state == FallState::RemainsDown,
            fall_detected: false,
            events_enabled: false,
        }
    }
}

pub fn validate_live_torso_manifest(
    manifest: &serde_json::Value,
    weights: &[f32],
) -> Result<FallThresholds, String> {
    if manifest.get("task").and_then(|value| value.as_str()) != Some("torso") {
        return Err("manifest task must be torso".into());
    }
    if manifest
        .get("schema_version")
        .and_then(|value| value.as_str())
        != Some("torso-v1")
    {
        return Err("manifest schema_version must be torso-v1".into());
    }
    if manifest
        .get("validation_status")
        .and_then(|value| value.as_str())
        != Some("VALIDATED")
        || manifest
            .get("blind_test_status")
            .and_then(|value| value.as_str())
            != Some("PASS")
    {
        return Err("torso model is UNVALIDATED or has no blind-test PASS".into());
    }
    let input = manifest.get("input").ok_or("manifest input is missing")?;
    if input.get("time_points").and_then(|value| value.as_u64()) != Some(20)
        || input.get("subcarriers").and_then(|value| value.as_u64()) != Some(64)
        || input.get("rx_order") != Some(&serde_json::json!(RX_ORDER))
    {
        return Err("torso input must be 20 time points, 64 subcarriers, RX1-RX4".into());
    }
    let channels = input
        .get("channels")
        .and_then(|value| value.as_array())
        .ok_or("manifest input.channels is missing")?;
    let channel_names: Vec<&str> = channels.iter().filter_map(|value| value.as_str()).collect();
    if channel_names != ["amplitude"] && channel_names != ["amplitude", "phase"] {
        return Err("torso channels must be amplitude or amplitude plus phase".into());
    }
    let expected_phase_status = if channel_names.contains(&"phase") {
        "sanitized"
    } else {
        "unavailable"
    };
    if manifest
        .get("phase_status")
        .and_then(|value| value.as_str())
        != Some(expected_phase_status)
    {
        return Err("phase_status does not match torso input channels".into());
    }
    let normalization = manifest
        .get("normalization")
        .and_then(|value| value.as_object())
        .ok_or("manifest normalization is missing")?;
    for channel in channels {
        let name = channel
            .as_str()
            .ok_or("input channel names must be strings")?;
        let stats = normalization
            .get(name)
            .ok_or("normalization channel is missing")?;
        let mean = stats.get("mean").and_then(|value| value.as_f64());
        let std = stats.get("std").and_then(|value| value.as_f64());
        if !mean.is_some_and(f64::is_finite)
            || !std.is_some_and(|value| value.is_finite() && value > 0.0)
        {
            return Err("normalization mean/std must be finite and std positive".into());
        }
    }
    if manifest.pointer("/output/keypoints") != Some(&serde_json::json!(TORSO_KEYPOINT_NAMES)) {
        return Err("torso output keypoint order is invalid".into());
    }
    if manifest.pointer("/output/fields") != Some(&serde_json::json!(["x", "y", "confidence"])) {
        return Err("torso output fields must be x, y, confidence".into());
    }
    let expected_sha = manifest
        .get("sha256")
        .and_then(|value| value.as_str())
        .ok_or("manifest sha256 is missing")?;
    let mut hasher = Sha256::new();
    for weight in weights {
        hasher.update(weight.to_le_bytes());
    }
    if format!("{:x}", hasher.finalize()) != expected_sha {
        return Err("torso model weights do not match manifest sha256".into());
    }
    serde_json::from_value::<FallThresholds>(
        manifest
            .get("fall_thresholds")
            .cloned()
            .ok_or("fall_thresholds are missing")?,
    )
    .map_err(|error| format!("invalid fall_thresholds: {error}"))?
    .validate()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(weights: &[f32]) -> serde_json::Value {
        let mut hasher = Sha256::new();
        for weight in weights {
            hasher.update(weight.to_le_bytes());
        }
        serde_json::json!({
            "task": "torso",
            "schema_version": "torso-v1",
            "validation_status": "VALIDATED",
            "blind_test_status": "PASS",
            "input": {
                "time_points": 20,
                "subcarriers": 64,
                "rx_order": RX_ORDER,
                "channels": ["amplitude", "phase"]
            },
            "phase_status": "sanitized",
            "normalization": {
                "amplitude": { "mean": 1.0, "std": 2.0 },
                "phase": { "mean": 0.0, "std": 0.5 }
            },
            "output": {
                "keypoints": TORSO_KEYPOINT_NAMES,
                "fields": ["x", "y", "confidence"]
            },
            "sha256": format!("{:x}", hasher.finalize()),
            "fall_thresholds": thresholds()
        })
    }

    fn output(mid_y: f64, height: f64, confidence: f64) -> TorsoOutput {
        TorsoOutput {
            left_shoulder: TorsoKeypoint {
                x: 0.4,
                y: mid_y - height / 2.0,
                confidence,
            },
            right_shoulder: TorsoKeypoint {
                x: 0.6,
                y: mid_y - height / 2.0,
                confidence,
            },
            left_hip: TorsoKeypoint {
                x: 0.4,
                y: mid_y + height / 2.0,
                confidence,
            },
            right_hip: TorsoKeypoint {
                x: 0.6,
                y: mid_y + height / 2.0,
                confidence,
            },
        }
    }

    fn thresholds() -> FallThresholds {
        FallThresholds {
            min_confidence: 0.8,
            rapid_descent_velocity: 0.5,
            floor_midpoint_y: 0.75,
            max_floor_height: 0.2,
            remains_down_seconds: 2.0,
            recovery_midpoint_y: 0.55,
            recovery_hold_seconds: 1.0,
        }
    }

    #[test]
    fn derives_midpoint_angle_height_and_velocity() {
        let current = output(0.6, 0.4, 0.9)
            .kinematics(Some(([0.5, 0.3], 1.0)), 1.5)
            .unwrap();
        assert_eq!(current.midpoint, [0.5, 0.6]);
        assert!(current.angle_degrees.abs() < 1e-9);
        assert!((current.height - 0.4).abs() < 1e-9);
        assert!((current.vertical_velocity - 0.6).abs() < 1e-9);
    }

    #[test]
    fn fall_sequence_stays_an_unpublished_candidate() {
        let mut machine = FallStateMachine::new(thresholds()).unwrap();
        let descent = output(0.6, 0.4, 0.9)
            .kinematics(Some(([0.5, 0.2], 0.0)), 0.5)
            .unwrap();
        assert_eq!(
            machine.update(Some(descent), 0.5).state,
            FallState::RapidDescent
        );
        let floor = output(0.8, 0.15, 0.9).kinematics(None, 1.0).unwrap();
        assert_eq!(
            machine.update(Some(floor), 1.0).state,
            FallState::FloorLevel
        );
        let decision = machine.update(Some(floor), 3.1);
        assert_eq!(decision.state, FallState::RemainsDown);
        assert!(decision.candidate_fall);
        assert!(!decision.fall_detected);
        assert!(!decision.events_enabled);
    }

    #[test]
    fn bending_sitting_and_short_dropout_do_not_form_a_fall() {
        let mut machine = FallStateMachine::new(thresholds()).unwrap();
        let bend = output(0.55, 0.3, 0.9).kinematics(None, 1.0).unwrap();
        assert_eq!(machine.update(Some(bend), 1.0).state, FallState::Upright);
        assert_eq!(machine.update(None, 2.0).state, FallState::Upright);
        let low_confidence = output(0.9, 0.1, 0.2).kinematics(None, 3.0).unwrap();
        assert_eq!(
            machine.update(Some(low_confidence), 3.0).state,
            FallState::Upright
        );
    }

    #[test]
    fn recovery_returns_to_upright() {
        let mut machine = FallStateMachine::new(thresholds()).unwrap();
        let descent = output(0.6, 0.4, 0.9)
            .kinematics(Some(([0.5, 0.2], 0.0)), 0.5)
            .unwrap();
        machine.update(Some(descent), 0.5);
        let floor = output(0.8, 0.15, 0.9).kinematics(None, 1.0).unwrap();
        machine.update(Some(floor), 1.0);
        machine.update(Some(floor), 3.1);
        let upright = output(0.4, 0.4, 0.9).kinematics(None, 4.0).unwrap();
        assert_eq!(
            machine.update(Some(upright), 4.0).state,
            FallState::Recovered
        );
        assert_eq!(machine.update(Some(upright), 5.1).state, FallState::Upright);
    }

    #[test]
    fn complete_validated_manifest_accepts_matching_weights() {
        let weights = [1.0_f32, -2.0, 3.5];
        assert!(validate_live_torso_manifest(&manifest(&weights), &weights).is_ok());
    }

    #[test]
    fn incomplete_or_mismatched_manifests_are_rejected() {
        let weights = [1.0_f32, 2.0];
        let mut value = manifest(&weights);
        value["validation_status"] = serde_json::json!("UNVALIDATED");
        assert!(validate_live_torso_manifest(&value, &weights).is_err());

        let mut value = manifest(&weights);
        value.as_object_mut().unwrap().remove("normalization");
        assert!(validate_live_torso_manifest(&value, &weights).is_err());

        let value = manifest(&weights);
        assert!(validate_live_torso_manifest(&value, &[9.0]).is_err());
    }
}

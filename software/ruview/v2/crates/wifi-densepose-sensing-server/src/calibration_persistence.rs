//! Durable, context-bound D5/D6 empty-room calibration bundles.
//!
//! A calibration is reusable only when the profile-relevant room context and
//! the sealed runtime setup are both identical. The bundle contains detector
//! references, never raw CSI, and is therefore safe to keep next to the
//! experiment catalogue as a small JSON artifact.

use crate::{
    d5_presence::PresenceReference,
    d6_fingerprint::FingerprintReference,
    position_artifact::{deterministic_pretty_json, sha256_bytes},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};

pub(crate) const CALIBRATION_SCHEMA_VERSION: u16 = 1;
pub(crate) const CALIBRATION_ALGORITHM_VERSION: &str = "wifi-d5-d6-empty-room-v1";

static CALIBRATION_COUNTER: AtomicU64 = AtomicU64::new(0);

/// The part of a setup profile that can change the WiFi empty-room baseline.
/// Labels, P01-P09 teaching points, and mmWave connection status are
/// intentionally excluded. The sealed setup hash binds firmware, exact CSI
/// grids, TX-source identity, and other runtime facts separately.
pub(crate) fn profile_context_sha256(document: &Value) -> Result<String, String> {
    let object = document
        .as_object()
        .ok_or_else(|| "setup profile document must be a JSON object".to_string())?;
    let receivers = object
        .get("receivers")
        .and_then(Value::as_array)
        .ok_or_else(|| "setup profile receivers are missing".to_string())?;
    let receiver_context: Vec<Value> = receivers
        .iter()
        .map(|receiver| {
            json!({
                "id": receiver.get("id"),
                "position_m": receiver.get("position_m"),
            })
        })
        .collect();
    let transmitter = object
        .get("transmitter")
        .ok_or_else(|| "setup profile transmitter is missing".to_string())?;
    let context = json!({
        "schema_version": CALIBRATION_SCHEMA_VERSION,
        "algorithm_version": CALIBRATION_ALGORITHM_VERSION,
        "room_dimensions_m": object.get("room_dimensions_m"),
        "sensor_mount_radius_m": object
            .get("sensor_mount_radius_m")
            .cloned()
            .unwrap_or_else(|| json!(0.5)),
        "transmitter": {
            "id": transmitter.get("id"),
            "position_m": transmitter.get("position_m"),
        },
        "receivers": receiver_context,
        "mmwave": object.get("mmwave").map(|mmwave| json!({
            "sensor": mmwave.get("sensor"),
            "mounting_position_m": mmwave.get("mounting_position_m"),
            "mounting_revision": mmwave.get("mounting_revision"),
        })),
        "radio": object.get("radio"),
        "environment": object.get("environment"),
    });
    let bytes = deterministic_pretty_json(&context).map_err(|error| error.to_string())?;
    Ok(sha256_bytes(&bytes))
}

pub(crate) fn calibration_context_sha256(
    profile_context_sha256: &str,
    setup_id: &str,
    setup_sha256: &str,
) -> Result<String, String> {
    for (field, value) in [
        ("profile_context_sha256", profile_context_sha256),
        ("setup_id", setup_id),
        ("setup_sha256", setup_sha256),
    ] {
        if value.trim().is_empty() || value.chars().any(char::is_control) {
            return Err(format!("{field} must be a non-empty printable identity"));
        }
    }
    let context = json!({
        "schema_version": CALIBRATION_SCHEMA_VERSION,
        "algorithm_version": CALIBRATION_ALGORITHM_VERSION,
        "profile_context_sha256": profile_context_sha256,
        "setup_id": setup_id,
        "setup_sha256": setup_sha256,
    });
    let bytes = deterministic_pretty_json(&context).map_err(|error| error.to_string())?;
    Ok(sha256_bytes(&bytes))
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CalibrationContext {
    pub(crate) profile_id: String,
    pub(crate) profile_revision_id: String,
    pub(crate) profile_sha256: String,
    pub(crate) profile_context_sha256: String,
    pub(crate) setup_id: String,
    pub(crate) setup_sha256: String,
    pub(crate) calibration_context_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CalibrationNodeBundle {
    pub(crate) node_id: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) d5: Option<PresenceReference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) d6: Option<FingerprintReference>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CalibrationBundle {
    pub(crate) schema_version: u16,
    pub(crate) calibration_id: String,
    pub(crate) profile_id: String,
    pub(crate) profile_revision_id: String,
    pub(crate) profile_sha256: String,
    pub(crate) profile_context_sha256: String,
    pub(crate) setup_id: String,
    pub(crate) setup_sha256: String,
    pub(crate) calibration_context_sha256: String,
    pub(crate) algorithm_version: String,
    pub(crate) captured_at: String,
    pub(crate) nodes: Vec<CalibrationNodeBundle>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CalibrationSummary {
    pub(crate) calibration_id: String,
    pub(crate) profile_id: String,
    pub(crate) profile_revision_id: String,
    pub(crate) profile_sha256: String,
    pub(crate) profile_context_sha256: String,
    pub(crate) setup_id: String,
    pub(crate) setup_sha256: String,
    pub(crate) calibration_context_sha256: String,
    pub(crate) algorithm_version: String,
    pub(crate) captured_at: String,
    pub(crate) node_count: usize,
}

impl CalibrationBundle {
    pub(crate) fn new(
        calibration_id: String,
        context: &CalibrationContext,
        captured_at: String,
        nodes: Vec<CalibrationNodeBundle>,
    ) -> Result<Self, String> {
        let bundle = Self {
            schema_version: CALIBRATION_SCHEMA_VERSION,
            calibration_id,
            profile_id: context.profile_id.clone(),
            profile_revision_id: context.profile_revision_id.clone(),
            profile_sha256: context.profile_sha256.clone(),
            profile_context_sha256: context.profile_context_sha256.clone(),
            setup_id: context.setup_id.clone(),
            setup_sha256: context.setup_sha256.clone(),
            calibration_context_sha256: context.calibration_context_sha256.clone(),
            algorithm_version: CALIBRATION_ALGORITHM_VERSION.to_string(),
            captured_at,
            nodes,
        };
        bundle.validate()?;
        Ok(bundle)
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.schema_version != CALIBRATION_SCHEMA_VERSION {
            return Err(format!(
                "unsupported calibration bundle schema version {}",
                self.schema_version
            ));
        }
        if self.algorithm_version != CALIBRATION_ALGORITHM_VERSION {
            return Err(format!(
                "unsupported calibration algorithm {}",
                self.algorithm_version
            ));
        }
        for (field, value) in [
            ("calibration_id", self.calibration_id.as_str()),
            ("profile_id", self.profile_id.as_str()),
            ("profile_revision_id", self.profile_revision_id.as_str()),
            ("profile_sha256", self.profile_sha256.as_str()),
            (
                "profile_context_sha256",
                self.profile_context_sha256.as_str(),
            ),
            ("setup_id", self.setup_id.as_str()),
            ("setup_sha256", self.setup_sha256.as_str()),
            (
                "calibration_context_sha256",
                self.calibration_context_sha256.as_str(),
            ),
            ("captured_at", self.captured_at.as_str()),
        ] {
            if value.trim().is_empty() || value.chars().any(char::is_control) {
                return Err(format!("calibration bundle {field} is invalid"));
            }
        }
        let expected_context_sha256 = calibration_context_sha256(
            &self.profile_context_sha256,
            &self.setup_id,
            &self.setup_sha256,
        )?;
        if self.calibration_context_sha256 != expected_context_sha256 {
            return Err(
                "calibration bundle context hash does not match its identities".to_string(),
            );
        }
        let mut node_ids = HashSet::new();
        let d6_ready = self.nodes.iter().filter(|node| node.d6.is_some()).count();
        if d6_ready < crate::d5_presence::MIN_FRESH_REFERENCES {
            return Err(format!(
                "calibration bundle needs at least {} D6 node references",
                crate::d5_presence::MIN_FRESH_REFERENCES
            ));
        }
        for node in &self.nodes {
            if !node_ids.insert(node.node_id) {
                return Err(format!("calibration bundle repeats node {}", node.node_id));
            }
            if node.d5.is_none() && node.d6.is_none() {
                return Err(format!(
                    "calibration bundle node {} has no reference",
                    node.node_id
                ));
            }
            if let Some(reference) = node.d5 {
                reference.validate()?;
            }
            if let Some(reference) = &node.d6 {
                reference.validate()?;
            }
        }
        Ok(())
    }

    pub(crate) fn node(&self, node_id: u8) -> Option<&CalibrationNodeBundle> {
        self.nodes.iter().find(|node| node.node_id == node_id)
    }

    pub(crate) fn summary(&self) -> CalibrationSummary {
        CalibrationSummary {
            calibration_id: self.calibration_id.clone(),
            profile_id: self.profile_id.clone(),
            profile_revision_id: self.profile_revision_id.clone(),
            profile_sha256: self.profile_sha256.clone(),
            profile_context_sha256: self.profile_context_sha256.clone(),
            setup_id: self.setup_id.clone(),
            setup_sha256: self.setup_sha256.clone(),
            calibration_context_sha256: self.calibration_context_sha256.clone(),
            algorithm_version: self.algorithm_version.clone(),
            captured_at: self.captured_at.clone(),
            node_count: self.nodes.len(),
        }
    }

    pub(crate) fn context(&self) -> CalibrationContext {
        CalibrationContext {
            profile_id: self.profile_id.clone(),
            profile_revision_id: self.profile_revision_id.clone(),
            profile_sha256: self.profile_sha256.clone(),
            profile_context_sha256: self.profile_context_sha256.clone(),
            setup_id: self.setup_id.clone(),
            setup_sha256: self.setup_sha256.clone(),
            calibration_context_sha256: self.calibration_context_sha256.clone(),
        }
    }
}

pub(crate) fn new_calibration_id() -> String {
    let counter = CALIBRATION_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!(
        "calibration-{}-{}-{}",
        chrono::Utc::now().timestamp_millis(),
        std::process::id(),
        counter
    )
}

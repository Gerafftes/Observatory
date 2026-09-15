//! Shared private contracts used by multiple HTTP route domains.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};
use serde::Deserialize;

use super::{calibration_persistence, experiment};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CalibrationContextRequest {
    pub(crate) profile_id: String,
    #[serde(default)]
    pub(crate) profile_revision_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CalibrationAvailabilityQuery {
    pub(crate) profile_id: String,
    #[serde(default)]
    pub(crate) profile_revision_id: Option<String>,
}

pub(crate) fn api_error(
    status: StatusCode,
    code: impl Into<String>,
    message: impl Into<String>,
) -> Response {
    (
        status,
        Json(serde_json::json!({
            "error": code.into(),
            "message": message.into(),
        })),
    )
        .into_response()
}

pub(crate) async fn resolve_calibration_context(
    store: &experiment::ExperimentStore,
    setup_identity: Option<(&str, &str)>,
    request: &CalibrationContextRequest,
) -> Result<calibration_persistence::CalibrationContext, String> {
    let (setup_id, setup_sha256) = setup_identity.ok_or_else(|| {
        "an active sealed position setup is required for reusable calibration".to_string()
    })?;
    let profile = match request.profile_revision_id.as_deref() {
        Some(revision_id) => store
            .get_profile_revision(&request.profile_id, revision_id)
            .await?
            .ok_or_else(|| "setup profile revision not found".to_string())?,
        None => store
            .get_profile(&request.profile_id)
            .await?
            .ok_or_else(|| "setup profile not found".to_string())?,
    };
    let profile_context_sha256 =
        calibration_persistence::profile_context_sha256(&profile.document)?;
    let calibration_context_sha256 = calibration_persistence::calibration_context_sha256(
        &profile_context_sha256,
        setup_id,
        setup_sha256,
    )?;
    Ok(calibration_persistence::CalibrationContext {
        profile_id: profile.id,
        profile_revision_id: profile.revision_id,
        profile_sha256: profile.profile_sha256,
        profile_context_sha256,
        setup_id: setup_id.to_string(),
        setup_sha256: setup_sha256.to_string(),
        calibration_context_sha256,
    })
}

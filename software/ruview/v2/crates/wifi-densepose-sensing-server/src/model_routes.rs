//! Model and LoRA HTTP routes used by the sensing-server binary.

use std::path::PathBuf;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use tracing::info;
use wifi_densepose_sensing_server::error_response;

use super::SharedState;

pub(crate) fn routes() -> Router<SharedState> {
    Router::new()
        .route("/api/v1/models", get(list_models))
        .route("/api/v1/models/active", get(get_active_model))
        .route("/api/v1/models/load", post(load_model))
        .route("/api/v1/models/unload", post(unload_model))
        .route("/api/v1/models/:id", get(get_model).delete(delete_model))
        .route("/api/v1/models/lora/profiles", get(list_lora_profiles))
        .route("/api/v1/models/lora/activate", post(activate_lora_profile))
}

/// GET /api/v1/models — list discovered RVF model files.
async fn list_models(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let models = scan_model_files();
    let total = models.len();
    {
        let mut state = state.write().await;
        state.discovered_models = models.clone();
    }
    Json(serde_json::json!({ "models": models, "total": total }))
}

/// GET /api/v1/models/:id — return metadata for one discovered RVF model.
async fn get_model(Path(id): Path<String>) -> (StatusCode, Json<serde_json::Value>) {
    model_detail_response(scan_model_files(), &id)
}

fn model_detail_response(
    models: Vec<serde_json::Value>,
    id: &str,
) -> (StatusCode, Json<serde_json::Value>) {
    if wifi_densepose_sensing_server::path_safety::safe_id(id).is_err() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "invalid model id",
                "success": false,
            })),
        );
    }

    match find_model_metadata(models, id) {
        Some(model) => (StatusCode::OK, Json(model)),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "model not found",
                "success": false,
            })),
        ),
    }
}

fn find_model_metadata(models: Vec<serde_json::Value>, id: &str) -> Option<serde_json::Value> {
    models
        .into_iter()
        .find(|model| model.get("id").and_then(serde_json::Value::as_str) == Some(id))
}

/// GET /api/v1/models/active — return currently loaded model or null.
async fn get_active_model(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let state = state.read().await;
    match &state.active_model_id {
        Some(id) => {
            let model = state
                .discovered_models
                .iter()
                .find(|model| model.get("id").and_then(|value| value.as_str()) == Some(id));
            Json(serde_json::json!({
                "active": model.cloned().unwrap_or_else(|| serde_json::json!({ "id": id })),
            }))
        }
        None => Json(serde_json::json!({ "active": serde_json::Value::Null })),
    }
}

/// POST /api/v1/models/load — load a model by ID.
async fn load_model(
    State(state): State<SharedState>,
    Json(body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let model_id = body
        .get("id")
        .or_else(|| body.get("model_id"))
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string();
    if model_id.is_empty() {
        return Json(serde_json::json!({ "error": "missing 'id' field", "success": false }));
    }
    let mut state = state.write().await;
    state.active_model_id = Some(model_id.clone());
    state.model_loaded = true;
    info!("Model loaded: {model_id}");
    Json(serde_json::json!({ "success": true, "model_id": model_id }))
}

/// POST /api/v1/models/unload — unload the current model.
async fn unload_model(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let mut state = state.write().await;
    let previous = state.active_model_id.take();
    state.model_loaded = false;
    info!("Model unloaded (was: {:?})", previous);
    Json(serde_json::json!({ "success": true, "previous": previous }))
}

/// DELETE /api/v1/models/:id — delete a model file.
async fn delete_model(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    let safe_id = std::path::Path::new(&id)
        .file_name()
        .and_then(|filename| filename.to_str())
        .unwrap_or("");
    if safe_id.is_empty() || safe_id != id {
        return Json(serde_json::json!({ "error": "invalid model id", "success": false }));
    }
    let path = effective_models_dir().join(format!("{safe_id}.rvf"));
    if !path.exists() {
        return Json(serde_json::json!({ "error": "model not found", "success": false }));
    }
    if let Err(error) = std::fs::remove_file(&path) {
        return error_response::internal_error_json("model delete", error);
    }

    let mut state = state.write().await;
    if state.active_model_id.as_deref() == Some(id.as_str()) {
        state.active_model_id = None;
        state.model_loaded = false;
    }
    state
        .discovered_models
        .retain(|model| model.get("id").and_then(|value| value.as_str()) != Some(id.as_str()));
    info!("Model deleted: {id}");
    Json(serde_json::json!({ "success": true, "deleted": id }))
}

/// GET /api/v1/models/lora/profiles — list LoRA adapter profiles.
async fn list_lora_profiles() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "profiles": scan_lora_profiles() }))
}

/// POST /api/v1/models/lora/activate — activate a LoRA adapter profile.
async fn activate_lora_profile(Json(body): Json<serde_json::Value>) -> Json<serde_json::Value> {
    let profile = body
        .get("profile")
        .or_else(|| body.get("name"))
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string();
    if profile.is_empty() {
        return Json(serde_json::json!({ "error": "missing 'profile' field", "success": false }));
    }
    info!("LoRA profile activated: {profile}");
    Json(serde_json::json!({ "success": true, "profile": profile }))
}

pub(crate) fn effective_models_dir() -> PathBuf {
    PathBuf::from(std::env::var("MODELS_DIR").unwrap_or_else(|_| "data/models".to_string()))
}

pub(crate) fn scan_model_files() -> Vec<serde_json::Value> {
    let dir = effective_models_dir();
    let mut models = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("rvf") {
                continue;
            }
            let name = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("unknown")
                .to_string();
            let size = entry.metadata().map(|metadata| metadata.len()).unwrap_or(0);
            let modified = entry
                .metadata()
                .ok()
                .and_then(|metadata| metadata.modified().ok())
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|duration| duration.as_secs())
                .unwrap_or(0);
            models.push(serde_json::json!({
                "id": name,
                "name": name,
                "path": path.display().to_string(),
                "size_bytes": size,
                "format": "rvf",
                "modified_epoch": modified,
            }));
        }
    }
    models
}

fn scan_lora_profiles() -> Vec<serde_json::Value> {
    let dir = effective_models_dir();
    let mut profiles = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path
                .file_name()
                .and_then(|filename| filename.to_str())
                .unwrap_or("");
            if !name.ends_with(".lora.json") {
                continue;
            }
            let profile_name = name.trim_end_matches(".lora.json").to_string();
            let config = std::fs::read_to_string(&path)
                .ok()
                .and_then(|contents| serde_json::from_str::<serde_json::Value>(&contents).ok())
                .unwrap_or_else(|| serde_json::json!({}));
            profiles.push(serde_json::json!({
                "name": profile_name,
                "path": path.display().to_string(),
                "config": config,
            }));
        }
    }
    profiles
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(id: &str) -> serde_json::Value {
        serde_json::json!({
            "id": id,
            "name": id,
            "format": "rvf",
            "size_bytes": 42,
        })
    }

    #[test]
    fn model_detail_uses_the_same_metadata_shape_as_the_model_list() {
        let expected = model("room-v1");
        let (status, Json(body)) =
            model_detail_response(vec![expected.clone(), model("other")], "room-v1");

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, expected);
    }

    #[test]
    fn model_detail_rejects_unsafe_ids_before_lookup() {
        let (status, Json(body)) = model_detail_response(vec![model("room-v1")], "../room-v1");

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["success"], false);
    }

    #[test]
    fn model_detail_returns_not_found_for_an_unknown_safe_id() {
        let (status, Json(body)) = model_detail_response(vec![model("room-v1")], "missing");

        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["success"], false);
    }
}

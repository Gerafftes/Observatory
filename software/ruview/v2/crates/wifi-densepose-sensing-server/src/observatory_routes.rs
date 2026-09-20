//! Observatory experiment, setup-profile, workflow, and aggregate status routes.

use super::route_support::api_error as experiment_api_error;
use super::system_routes::public_node_summaries;
use super::*;

pub(crate) fn routes() -> Router<SharedState> {
    Router::new()
        .route("/api/v1/experiments/status", get(experiments_status))
        .route(
            "/api/v1/experiments/runs",
            get(experiments_list).post(experiments_create),
        )
        .route("/api/v1/experiments/runs/:id", get(experiment_get))
        .route(
            "/api/v1/experiments/runs/:id/replay",
            post(experiment_replay),
        )
        .route(
            "/api/v1/experiments/setup-profiles",
            get(setup_profiles_list).post(setup_profile_create),
        )
        .route(
            "/api/v1/experiments/setup-profiles/:id",
            put(setup_profile_update),
        )
        .route(
            "/api/v1/experiments/setup-profiles/:id/setup-v2-draft",
            get(setup_profile_v2_draft),
        )
        .route("/api/v1/experiments/workflows", post(workflow_create))
        .route("/api/v1/experiments/runs/:id/phase", post(workflow_advance))
        .route(
            "/api/v1/experiments/runs/:id/artifacts",
            post(workflow_artifact_register),
        )
        .route(
            "/api/v1/experiments/runs/:id/report",
            get(experiment_report).post(workflow_report),
        )
        .route(
            "/api/v1/experiments/runs/:id/export",
            get(experiment_export),
        )
        .route("/api/v1/control-center/status", get(control_center_status))
        .route("/api/v1/benchmarks/catalog", get(benchmark_catalog))
}

#[derive(Debug, Deserialize)]
struct CreateExperimentRequest {
    label: Option<String>,
    fixture_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SetupProfileRequest {
    label: String,
    document: serde_json::Value,
}

async fn map_saved_mmwave_profile(state: &SharedState) -> serde_json::Value {
    let state = state.read().await;
    if !state.mmwave.transform_reconfiguration_allowed() {
        return serde_json::json!({
            "status": "skipped",
            "reason": "an active sealed setup or calibration session keeps its immutable geometry; create a new setup for this profile",
        });
    }
    serde_json::json!({
        "status": "server_mapped",
        "node_ids": state.mmwave.node_ids(),
        "reason": "the server applies the saved room transform by packet node_id; no ESP transform write is required",
    })
}

fn setup_profile_response(
    profile: &experiment::SetupProfile,
    mmwave_transform_sync: serde_json::Value,
) -> Json<serde_json::Value> {
    let mut response = serde_json::to_value(profile).expect("setup profile is serializable");
    response["mmwave_transform_sync"] = mmwave_transform_sync;
    Json(response)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SetupProfileDraftQuery {
    #[serde(default)]
    revision_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateWorkflowRequest {
    label: String,
    profile_id: String,
    #[serde(default)]
    profile_revision_id: Option<String>,
    #[serde(default = "default_dataset_version")]
    dataset_version: String,
    #[serde(default = "default_firmware_version")]
    firmware_version: String,
    #[serde(default)]
    blind_seed: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkflowPhaseRequest {
    phase: String,
    status: String,
    #[serde(default)]
    payload: serde_json::Value,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkflowArtifactRequest {
    kind: String,
    relative_path: String,
}

fn default_dataset_version() -> String {
    "unassigned".to_string()
}

fn default_firmware_version() -> String {
    "unassigned".to_string()
}

async fn experiments_status(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let store = state.read().await.experiment_store.clone();
    let Some(store) = store else {
        return Json(serde_json::json!({
            "available": false,
            "status": "PERSISTENCE_UNAVAILABLE",
            "schema_version": experiment::SCHEMA_VERSION,
            "message": "SQLite persistence is unavailable; the Control Center is locked.",
            "control_center_locked": true,
        }));
    };

    match store.run_count().await {
        Ok(run_count) => Json(serde_json::json!({
            "available": true,
            "status": "READY",
            "schema_version": experiment::SCHEMA_VERSION,
            "database_path": store.db_path().display().to_string(),
            "run_count": run_count,
            "supported_fixture_ids": [experiment::SUPPORTED_FIXTURE_ID],
            "control_center_locked": false,
        })),
        Err(error) => Json(serde_json::json!({
            "available": false,
            "status": "PERSISTENCE_UNAVAILABLE",
            "schema_version": experiment::SCHEMA_VERSION,
            "message": error,
            "control_center_locked": true,
        })),
    }
}

async fn experiments_list(
    State(state): State<SharedState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let store = state.read().await.experiment_store.clone();
    let Some(store) = store else {
        return experiment_api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "PERSISTENCE_UNAVAILABLE",
            "SQLite persistence is unavailable; experiment runs cannot be listed.",
        );
    };

    let limit = params
        .get("limit")
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(50)
        .clamp(1, 100);
    match store.list_runs(limit).await {
        Ok(runs) => Json(serde_json::json!({ "runs": runs, "limit": limit })).into_response(),
        Err(error) => {
            experiment_api_error(StatusCode::SERVICE_UNAVAILABLE, "DB_READ_FAILED", error)
        }
    }
}

async fn experiments_create(
    State(state): State<SharedState>,
    Json(request): Json<CreateExperimentRequest>,
) -> Response {
    let store = state.read().await.experiment_store.clone();
    let Some(store) = store else {
        return experiment_api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "PERSISTENCE_UNAVAILABLE",
            "SQLite persistence is unavailable; the Control Center is locked.",
        );
    };

    let Some(label) = request.label else {
        return experiment_api_error(
            StatusCode::BAD_REQUEST,
            "INVALID_REQUEST",
            "label is required",
        );
    };
    let Some(fixture_id) = request.fixture_id else {
        return experiment_api_error(
            StatusCode::BAD_REQUEST,
            "INVALID_REQUEST",
            "fixture_id is required",
        );
    };

    match store.create_run(&label, &fixture_id).await {
        Ok(run) => (StatusCode::CREATED, Json(serde_json::json!(run))).into_response(),
        Err(error) if error.starts_with("unsupported fixture_id") => {
            experiment_api_error(StatusCode::BAD_REQUEST, "UNSUPPORTED_FIXTURE", error)
        }
        Err(error) if error.starts_with("label must") => {
            experiment_api_error(StatusCode::BAD_REQUEST, "INVALID_LABEL", error)
        }
        Err(error) => {
            experiment_api_error(StatusCode::SERVICE_UNAVAILABLE, "DB_WRITE_FAILED", error)
        }
    }
}

async fn experiment_get(State(state): State<SharedState>, Path(id): Path<String>) -> Response {
    let store = state.read().await.experiment_store.clone();
    let Some(store) = store else {
        return experiment_api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "PERSISTENCE_UNAVAILABLE",
            "SQLite persistence is unavailable; the Control Center is locked.",
        );
    };

    match store.get_run(&id).await {
        Ok(Some(run)) => Json(serde_json::json!(run)).into_response(),
        Ok(None) => experiment_api_error(
            StatusCode::NOT_FOUND,
            "RUN_NOT_FOUND",
            "experiment run not found",
        ),
        Err(error) => {
            experiment_api_error(StatusCode::SERVICE_UNAVAILABLE, "DB_READ_FAILED", error)
        }
    }
}

async fn experiment_replay(State(state): State<SharedState>, Path(id): Path<String>) -> Response {
    let store = state.read().await.experiment_store.clone();
    let Some(store) = store else {
        return experiment_api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "PERSISTENCE_UNAVAILABLE",
            "SQLite persistence is unavailable; the Control Center is locked.",
        );
    };

    match store.get_run(&id).await {
        Ok(Some(run)) if run.state == "completed" => experiment_api_error(
            StatusCode::CONFLICT,
            "RUN_COMPLETED",
            "completed experiment runs cannot be replayed again",
        ),
        Ok(Some(run)) if run.state == "running" => experiment_api_error(
            StatusCode::CONFLICT,
            "RUN_RUNNING",
            "experiment run is already running",
        ),
        Ok(Some(_)) => match experiment::replay_run(&store, &id).await {
            Ok(run) => Json(serde_json::json!(run)).into_response(),
            Err(error) => {
                experiment_api_error(StatusCode::INTERNAL_SERVER_ERROR, "REPLAY_FAILED", error)
            }
        },
        Ok(None) => experiment_api_error(
            StatusCode::NOT_FOUND,
            "RUN_NOT_FOUND",
            "experiment run not found",
        ),
        Err(error) => {
            experiment_api_error(StatusCode::SERVICE_UNAVAILABLE, "DB_READ_FAILED", error)
        }
    }
}

async fn setup_profiles_list(State(state): State<SharedState>) -> Response {
    let store = state.read().await.experiment_store.clone();
    let Some(store) = store else {
        return experiment_api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "PERSISTENCE_UNAVAILABLE",
            "SQLite persistence is unavailable; setup profiles cannot be listed.",
        );
    };
    match store.list_profiles().await {
        Ok(profiles) => Json(serde_json::json!({
            "profiles": profiles,
            "schema_version": experiment::PROFILE_SCHEMA_VERSION
        }))
        .into_response(),
        Err(error) => {
            experiment_api_error(StatusCode::SERVICE_UNAVAILABLE, "DB_READ_FAILED", error)
        }
    }
}

async fn setup_profile_create(
    State(state): State<SharedState>,
    Json(request): Json<SetupProfileRequest>,
) -> Response {
    let store = state.read().await.experiment_store.clone();
    let Some(store) = store else {
        return experiment_api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "PERSISTENCE_UNAVAILABLE",
            "SQLite persistence is unavailable; setup profiles cannot be saved.",
        );
    };
    let profile = match store
        .create_profile(&request.label, &request.document)
        .await
    {
        Ok(profile) => profile,
        Err(error) => {
            return experiment_api_error(StatusCode::BAD_REQUEST, "INVALID_PROFILE", error)
        }
    };
    state.write().await.mmwave.apply_cad_profile(&profile);
    let mmwave_transform_sync = map_saved_mmwave_profile(&state).await;
    (
        StatusCode::CREATED,
        setup_profile_response(&profile, mmwave_transform_sync),
    )
        .into_response()
}

async fn setup_profile_update(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    Json(request): Json<SetupProfileRequest>,
) -> Response {
    let store = state.read().await.experiment_store.clone();
    let Some(store) = store else {
        return experiment_api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "PERSISTENCE_UNAVAILABLE",
            "SQLite persistence is unavailable; setup profiles cannot be saved.",
        );
    };
    let profile = match store
        .update_profile(&id, &request.label, &request.document)
        .await
    {
        Ok(profile) => profile,
        Err(error) if error == "setup profile not found" => {
            return experiment_api_error(StatusCode::NOT_FOUND, "PROFILE_NOT_FOUND", error)
        }
        Err(error) => {
            return experiment_api_error(StatusCode::BAD_REQUEST, "INVALID_PROFILE", error)
        }
    };
    state.write().await.mmwave.apply_cad_profile(&profile);
    let mmwave_transform_sync = map_saved_mmwave_profile(&state).await;
    setup_profile_response(&profile, mmwave_transform_sync).into_response()
}

async fn setup_profile_v2_draft(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    Query(query): Query<SetupProfileDraftQuery>,
) -> Response {
    let store = state.read().await.experiment_store.clone();
    let Some(store) = store else {
        return experiment_api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "PERSISTENCE_UNAVAILABLE",
            "SQLite persistence is unavailable; setup drafts cannot be generated.",
        );
    };
    let profile = match query.revision_id.as_deref() {
        Some(revision_id) => store.get_profile_revision(&id, revision_id).await,
        None => store.get_profile(&id).await,
    };
    let profile = match profile {
        Ok(Some(profile)) => profile,
        Ok(None) => {
            return experiment_api_error(
                StatusCode::NOT_FOUND,
                "PROFILE_NOT_FOUND",
                "setup profile or requested revision not found",
            )
        }
        Err(error) => {
            return experiment_api_error(StatusCode::SERVICE_UNAVAILABLE, "DB_READ_FAILED", error)
        }
    };
    let mut draft = match position_setup::observatory_profile_setup_draft(&profile.document) {
        Ok(draft) => draft,
        Err(error) => {
            return experiment_api_error(StatusCode::CONFLICT, "PROFILE_NOT_SEALABLE", error)
        }
    };
    if let Some(object) = draft.as_object_mut() {
        object.insert(
            "source_profile".to_string(),
            serde_json::json!({
                "id": profile.id,
                "revision_id": profile.revision_id,
                "version": profile.version,
                "profile_sha256": profile.profile_sha256,
            }),
        );
    }
    match serde_json::to_string_pretty(&draft) {
        Ok(body) => (
            StatusCode::OK,
            [(
                axum::http::header::CONTENT_TYPE,
                "application/json; charset=utf-8",
            )],
            body,
        )
            .into_response(),
        Err(error) => experiment_api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "DRAFT_SERIALIZATION_FAILED",
            error.to_string(),
        ),
    }
}

async fn workflow_create(
    State(state): State<SharedState>,
    Json(request): Json<CreateWorkflowRequest>,
) -> Response {
    let store = state.read().await.experiment_store.clone();
    let Some(store) = store else {
        return experiment_api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "PERSISTENCE_UNAVAILABLE",
            "SQLite persistence is unavailable; workflow runs cannot be created.",
        );
    };
    let seed = request.blind_seed.unwrap_or_else(|| {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        now.as_nanos() as u64
    });
    match store
        .create_workflow_run(
            &request.label,
            &request.profile_id,
            request.profile_revision_id.as_deref(),
            &request.dataset_version,
            &request.firmware_version,
            seed,
        )
        .await
    {
        Ok(run) => (StatusCode::CREATED, Json(serde_json::json!(run))).into_response(),
        Err(error) if error == "setup profile not found" => {
            experiment_api_error(StatusCode::NOT_FOUND, "PROFILE_NOT_FOUND", error)
        }
        Err(error) => experiment_api_error(StatusCode::BAD_REQUEST, "INVALID_WORKFLOW", error),
    }
}

async fn workflow_advance(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    Json(request): Json<WorkflowPhaseRequest>,
) -> Response {
    let (store, position_setup) = {
        let state = state.read().await;
        (state.experiment_store.clone(), state.position_setup.clone())
    };
    let Some(store) = store else {
        return experiment_api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "PERSISTENCE_UNAVAILABLE",
            "SQLite persistence is unavailable; workflow phases cannot be recorded.",
        );
    };
    let requested_phase_index = experiment::WORKFLOW_PHASES
        .iter()
        .position(|phase| *phase == request.phase);
    let workflow = if requested_phase_index.is_some_and(|index| index > 0) {
        let run = match store.get_run(&id).await {
            Ok(Some(run)) => run,
            Ok(None) => {
                return experiment_api_error(
                    StatusCode::NOT_FOUND,
                    "RUN_NOT_FOUND",
                    "workflow run not found",
                )
            }
            Err(error) => {
                return experiment_api_error(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "DB_READ_FAILED",
                    error,
                )
            }
        };
        let Some(workflow) = run.workflow else {
            return experiment_api_error(
                StatusCode::CONFLICT,
                "INVALID_PHASE",
                "run is not a position workflow",
            );
        };
        Some(workflow)
    } else {
        None
    };
    let software_only_phase = workflow
        .as_ref()
        .is_some_and(|workflow| workflow_allows_software_only_phase(workflow, &request.payload));
    if requested_phase_index.is_some_and(|index| index > 1) && !software_only_phase {
        let Some(setup) = position_setup.as_deref() else {
            return experiment_api_error(
                StatusCode::CONFLICT,
                "POSITION_SETUP_REQUIRED",
                "Workflow bleibt gesperrt: Der Server läuft ohne aktives --position-setup.",
            );
        };
        let workflow = workflow
            .as_ref()
            .expect("later workflow phases loaded the workflow above");
        if let Err(error) =
            validate_workflow_runtime_seal(workflow, setup.setup_id(), setup.setup_sha256())
        {
            return experiment_api_error(StatusCode::CONFLICT, "POSITION_SETUP_REQUIRED", error);
        }
    }
    let payload = if request.phase == "seal_setup" {
        if request.status != "PASS" {
            return experiment_api_error(
                StatusCode::CONFLICT,
                "POSITION_SETUP_REQUIRED",
                "Die Setup-Phase darf nur mit einem validierten Runtime-Seal als PASS betreten werden.",
            );
        }
        if software_only_phase {
            request.payload
        } else {
            let Some(setup) = position_setup else {
                return experiment_api_error(
                    StatusCode::CONFLICT,
                    "POSITION_SETUP_REQUIRED",
                    "Setup kann nicht versiegelt werden: Der Server läuft ohne aktives --position-setup.",
                );
            };
            let workflow = workflow
                .as_ref()
                .expect("setup phase loaded the workflow above");
            let profile = match workflow.profile_revision_id.as_deref() {
                Some(revision_id) => {
                    store
                        .get_profile_revision(&workflow.profile_id, revision_id)
                        .await
                }
                None => store.get_profile(&workflow.profile_id).await,
            };
            let profile = match profile {
                Ok(Some(profile)) => profile,
                Ok(None) => {
                    return experiment_api_error(
                        StatusCode::CONFLICT,
                        "PROFILE_REVISION_MISSING",
                        "the profile revision bound to this run no longer exists",
                    )
                }
                Err(error) => {
                    return experiment_api_error(
                        StatusCode::SERVICE_UNAVAILABLE,
                        "DB_READ_FAILED",
                        error,
                    )
                }
            };
            if profile.profile_sha256 != workflow.profile_sha256 {
                return experiment_api_error(
                    StatusCode::CONFLICT,
                    "PROFILE_REVISION_MISMATCH",
                    "the profile revision no longer matches the hash bound to this run",
                );
            }
            if let Err(error) = setup.validate_observatory_profile(&profile.document) {
                return experiment_api_error(
                    StatusCode::CONFLICT,
                    "POSITION_SETUP_MISMATCH",
                    error,
                );
            }
            match bind_runtime_position_setup(
                &request.payload,
                &workflow.profile_sha256,
                setup.as_ref(),
            ) {
                Ok(payload) => payload,
                Err(error) => {
                    return experiment_api_error(
                        StatusCode::CONFLICT,
                        "POSITION_SETUP_MISMATCH",
                        error,
                    )
                }
            }
        }
    } else {
        request.payload
    };
    match store
        .advance_workflow(&id, &request.phase, &request.status, &payload)
        .await
    {
        Ok(run) => Json(serde_json::json!(run)).into_response(),
        Err(error) if error == "workflow run not found" => {
            experiment_api_error(StatusCode::NOT_FOUND, "RUN_NOT_FOUND", error)
        }
        Err(error) => experiment_api_error(StatusCode::CONFLICT, "INVALID_PHASE", error),
    }
}

fn workflow_allows_software_only_phase(
    workflow: &experiment::ExperimentWorkflow,
    payload: &serde_json::Value,
) -> bool {
    let request_is_software_only = payload
        .get("software_only")
        .and_then(serde_json::Value::as_bool)
        == Some(true);
    request_is_software_only
        && workflow.events.iter().any(|event| {
            event.phase == "create_experiment"
                && event.status == "PASS"
                && event
                    .payload
                    .get("software_only")
                    .and_then(serde_json::Value::as_bool)
                    == Some(true)
        })
}

fn validate_workflow_runtime_seal(
    workflow: &experiment::ExperimentWorkflow,
    setup_id: &str,
    setup_sha256: &str,
) -> Result<(), String> {
    let valid = workflow.events.iter().any(|event| {
        event.phase == "seal_setup"
            && event.status == "PASS"
            && event
                .payload
                .get("seal_kind")
                .and_then(serde_json::Value::as_str)
                == Some("active_position_setup_v2")
            && event
                .payload
                .get("profile_sha256")
                .and_then(serde_json::Value::as_str)
                == Some(workflow.profile_sha256.as_str())
            && event
                .payload
                .get("setup_id")
                .and_then(serde_json::Value::as_str)
                == Some(setup_id)
            && event
                .payload
                .get("setup_sha256")
                .and_then(serde_json::Value::as_str)
                == Some(setup_sha256)
    });
    valid.then_some(()).ok_or_else(|| {
        "Workflow bleibt gesperrt: Es fehlt ein zum aktiven Setup-v2 passendes Runtime-Seal. Bitte einen neuen Run anlegen."
            .to_string()
    })
}

#[cfg(test)]
mod workflow_runtime_seal_tests {
    use super::*;

    fn workflow_with_seal(payload: serde_json::Value) -> experiment::ExperimentWorkflow {
        experiment::ExperimentWorkflow {
            profile_id: "profile-test".to_string(),
            profile_revision_id: Some("profile-test-v1".to_string()),
            profile_sha256: "a".repeat(64),
            profile_context_sha256: Some("b".repeat(64)),
            dataset_version: "dataset-test".to_string(),
            firmware_version: "firmware-test".to_string(),
            calibration_id: None,
            calibration_source: None,
            calibration_context_sha256: None,
            blind_seed: 7,
            current_phase: "seal_setup".to_string(),
            current_status: "PASS".to_string(),
            events: vec![experiment::WorkflowPhaseEvent {
                id: 1,
                phase: "seal_setup".to_string(),
                status: "PASS".to_string(),
                payload,
                created_at: "2026-08-29T00:00:00Z".to_string(),
            }],
        }
    }

    #[test]
    fn exact_runtime_seal_allows_later_workflow_phases() {
        let workflow = workflow_with_seal(serde_json::json!({
            "seal_kind": "active_position_setup_v2",
            "profile_sha256": "a".repeat(64),
            "setup_id": "setup-runtime-1",
            "setup_sha256": "c".repeat(64),
        }));

        validate_workflow_runtime_seal(&workflow, "setup-runtime-1", &"c".repeat(64)).unwrap();
    }

    #[test]
    fn software_only_gate_requires_the_initial_marker_and_marks_every_phase() {
        let mut demo = workflow_with_seal(serde_json::json!({"software_only": true}));
        demo.events[0].phase = "create_experiment".to_string();

        assert!(workflow_allows_software_only_phase(
            &demo,
            &serde_json::json!({"software_only": true}),
        ));
        assert!(!workflow_allows_software_only_phase(
            &demo,
            &serde_json::json!({}),
        ));

        demo.events[0].status = "READY".to_string();
        assert!(!workflow_allows_software_only_phase(
            &demo,
            &serde_json::json!({"software_only": true}),
        ));

        let late_marker = workflow_with_seal(serde_json::json!({"software_only": true}));
        assert!(!workflow_allows_software_only_phase(
            &late_marker,
            &serde_json::json!({"software_only": true}),
        ));
    }

    #[test]
    fn legacy_or_changed_runtime_seal_blocks_later_workflow_phases() {
        let legacy = workflow_with_seal(serde_json::json!({
            "profile_sha256": "a".repeat(64),
        }));
        assert!(
            validate_workflow_runtime_seal(&legacy, "setup-runtime-1", &"c".repeat(64),).is_err()
        );

        let changed = workflow_with_seal(serde_json::json!({
            "seal_kind": "active_position_setup_v2",
            "profile_sha256": "a".repeat(64),
            "setup_id": "setup-runtime-1",
            "setup_sha256": "d".repeat(64),
        }));
        assert!(
            validate_workflow_runtime_seal(&changed, "setup-runtime-1", &"c".repeat(64),).is_err()
        );
    }
}

fn bind_runtime_position_setup(
    payload: &serde_json::Value,
    profile_sha256: &str,
    setup: &position_setup::SealedPositionSetup,
) -> Result<serde_json::Value, String> {
    let mut payload = payload
        .as_object()
        .cloned()
        .ok_or_else(|| "setup seal payload must be a JSON object".to_string())?;
    for (field, expected) in [
        ("setup_id", setup.setup_id()),
        ("setup_sha256", setup.setup_sha256()),
    ] {
        if payload
            .get(field)
            .is_some_and(|actual| actual.as_str() != Some(expected))
        {
            return Err(format!(
                "requested {field} does not match the active sealed setup"
            ));
        }
    }
    if payload
        .get("profile_sha256")
        .is_some_and(|actual| actual.as_str() != Some(profile_sha256))
    {
        return Err(
            "requested profile_sha256 does not match the profile revision bound to this run"
                .to_string(),
        );
    }
    payload.insert(
        "profile_sha256".to_string(),
        serde_json::json!(profile_sha256),
    );
    payload.insert("setup_id".to_string(), serde_json::json!(setup.setup_id()));
    payload.insert(
        "setup_sha256".to_string(),
        serde_json::json!(setup.setup_sha256()),
    );
    payload.insert(
        "seal_kind".to_string(),
        serde_json::json!("active_position_setup_v2"),
    );
    Ok(serde_json::Value::Object(payload))
}

async fn workflow_artifact_register(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    Json(request): Json<WorkflowArtifactRequest>,
) -> Response {
    let store = state.read().await.experiment_store.clone();
    let Some(store) = store else {
        return experiment_api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "PERSISTENCE_UNAVAILABLE",
            "SQLite persistence is unavailable; artifacts cannot be registered.",
        );
    };
    match store
        .register_workflow_artifact(&id, &request.kind, &request.relative_path)
        .await
    {
        Ok(run) => Json(serde_json::json!(run)).into_response(),
        Err(error) if error == "workflow run not found" => {
            experiment_api_error(StatusCode::NOT_FOUND, "RUN_NOT_FOUND", error)
        }
        Err(error) => experiment_api_error(StatusCode::BAD_REQUEST, "INVALID_ARTIFACT", error),
    }
}

async fn workflow_report(State(state): State<SharedState>, Path(id): Path<String>) -> Response {
    let store = state.read().await.experiment_store.clone();
    let Some(store) = store else {
        return experiment_api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "PERSISTENCE_UNAVAILABLE",
            "SQLite persistence is unavailable; reports cannot be written.",
        );
    };
    match store.write_workflow_report(&id).await {
        Ok(run) => Json(serde_json::json!(run)).into_response(),
        Err(error) if error == "workflow run not found" => {
            experiment_api_error(StatusCode::NOT_FOUND, "RUN_NOT_FOUND", error)
        }
        Err(error) => experiment_api_error(StatusCode::CONFLICT, "REPORT_NOT_READY", error),
    }
}

async fn experiment_report(State(state): State<SharedState>, Path(id): Path<String>) -> Response {
    let store = state.read().await.experiment_store.clone();
    let Some(store) = store else {
        return experiment_api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "PERSISTENCE_UNAVAILABLE",
            "SQLite persistence is unavailable; reports cannot be read.",
        );
    };
    match store.report_json(&id).await {
        Ok(Some(report)) => Json(report).into_response(),
        Ok(None) => experiment_api_error(
            StatusCode::NOT_FOUND,
            "REPORT_NOT_FOUND",
            "no report artifact exists for this run",
        ),
        Err(error) => {
            experiment_api_error(StatusCode::SERVICE_UNAVAILABLE, "REPORT_READ_FAILED", error)
        }
    }
}

async fn experiment_export(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let store = state.read().await.experiment_store.clone();
    let Some(store) = store else {
        return experiment_api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "PERSISTENCE_UNAVAILABLE",
            "SQLite persistence is unavailable; exports cannot be generated.",
        );
    };
    let Some(run) = (match store.get_run(&id).await {
        Ok(run) => run,
        Err(error) => {
            return experiment_api_error(StatusCode::SERVICE_UNAVAILABLE, "DB_READ_FAILED", error)
        }
    }) else {
        return experiment_api_error(
            StatusCode::NOT_FOUND,
            "RUN_NOT_FOUND",
            "experiment run not found",
        );
    };
    if params.get("format").is_some_and(|format| format == "csv") {
        let mut csv = String::from("event_id,phase,status,created_at,payload_json\n");
        if let Some(workflow) = &run.workflow {
            for event in &workflow.events {
                let payload =
                    serde_json::to_string(&event.payload).unwrap_or_else(|_| "{}".to_string());
                csv.push_str(&format!(
                    "{},{},{},{},\"{}\"\n",
                    event.id,
                    event.phase,
                    event.status,
                    event.created_at,
                    payload.replace('"', "\"\"")
                ));
            }
        }
        return (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "text/csv; charset=utf-8")],
            csv,
        )
            .into_response();
    }
    let report = store.report_json(&id).await.ok().flatten();
    Json(serde_json::json!({ "run": run, "report": report })).into_response()
}

async fn control_center_status(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let (
        nodes,
        recording,
        training,
        calibration,
        position_setup,
        csi_grid_pin,
        active_model,
        intro,
        mmwave,
    ) = {
        let s = state.read().await;
        let now = std::time::Instant::now();
        let recording = s.recording_current_id.as_ref().map(|id| {
            serde_json::json!({
                "id": id,
                "phase": match s.recording_phase {
                    RecordingLifecyclePhase::Idle => "idle",
                    RecordingLifecyclePhase::Recording => "recording",
                    RecordingLifecyclePhase::Finalizing => "finalizing",
                }
            })
        });
        (
            public_node_summaries(
                &s.node_states,
                now,
                s.d5_presence.phase(),
                s.position_setup.is_some(),
            ),
            recording,
            training_routes::training_status_payload(),
            serde_json::json!({
                "phase": s.d5_presence.phase().as_str(),
                "position_setup_active": s.position_setup.is_some(),
                "calibration_id": s
                    .active_calibration_bundle
                    .as_ref()
                    .map(|bundle| bundle.calibration_id.clone()),
                "calibration_source": s.active_calibration_source.clone(),
                "calibration_context_sha256": s
                    .active_calibration_bundle
                    .as_ref()
                    .map(|bundle| bundle.calibration_context_sha256.clone()),
                "reuse_available": s.active_calibration_bundle.is_some(),
            }),
            s.position_setup.as_ref().map(|setup| {
                serde_json::json!({
                    "active": true,
                    "setup_id": setup.setup_id(),
                    "setup_sha256": setup.setup_sha256(),
                    "room_dimensions_m": setup.room_dimensions_m(),
                })
            }),
            s.csi_grid_pin,
            s.active_model_id.clone(),
            serde_json::to_value(s.intro.snapshot()).unwrap_or_else(|_| serde_json::json!({})),
            serde_json::to_value(s.mmwave.status(server_clock::now().host_monotonic_ns))
                .unwrap_or_else(|_| serde_json::json!({})),
        )
    };
    Json(serde_json::json!({
        "nodes": nodes,
        "recording": recording,
        "training": training,
        "classification_calibration": calibration,
        "position_setup": position_setup,
        "csi_grid_pin": csi_grid_pin,
        "active_model_id": active_model,
        "signal_diagnostics": intro,
        "mmwave": mmwave,
        "mmwave_control": "read_only_until_sensor_validation",
        "capabilities": {
            "setup_v2_draft_export": true
        }
    }))
}

async fn benchmark_catalog() -> Json<serde_json::Value> {
    Json(benchmark::catalog())
}

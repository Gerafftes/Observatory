//! Adaptive classification and calibration HTTP routes.

use super::route_support::{
    api_error as experiment_api_error, resolve_calibration_context, CalibrationAvailabilityQuery,
    CalibrationContextRequest,
};
use super::*;

pub(crate) fn routes() -> Router<SharedState> {
    Router::new()
        .route("/api/v1/adaptive/train", post(adaptive_train))
        .route("/api/v1/adaptive/status", get(adaptive_status))
        .route("/api/v1/adaptive/unload", post(adaptive_unload))
        .route(
            "/api/v1/classification/calibration/start",
            post(classification_calibration_start),
        )
        .route(
            "/api/v1/classification/calibration/stop",
            post(classification_calibration_stop),
        )
        .route(
            "/api/v1/classification/calibration/status",
            get(classification_calibration_status),
        )
        .route(
            "/api/v1/classification/calibration/availability",
            get(classification_calibration_availability),
        )
        .route(
            "/api/v1/classification/calibration/reuse",
            post(classification_calibration_reuse),
        )
        .route("/api/v1/calibration/start", post(calibration_start))
        .route("/api/v1/calibration/stop", post(calibration_stop))
        .route("/api/v1/calibration/status", get(calibration_status))
}

// ── Adaptive classifier endpoints ────────────────────────────────────────────

/// POST /api/v1/adaptive/train — train the adaptive classifier from recordings.
async fn adaptive_train(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let rec_dir = PathBuf::from("data/recordings");
    eprintln!("=== Adaptive Classifier Training ===");
    match adaptive_classifier::train_from_recordings(&rec_dir) {
        Ok(model) => {
            let accuracy = model.training_accuracy;
            let frames = model.trained_frames;
            let stats: Vec<_> = model
                .class_stats
                .iter()
                .map(|cs| {
                    serde_json::json!({
                        "class": cs.label,
                        "samples": cs.count,
                        "feature_means": cs.mean,
                    })
                })
                .collect();

            // Save to disk.
            if let Err(e) = model.save(&adaptive_classifier::model_path()) {
                warn!("Failed to save adaptive model: {e}");
            } else {
                info!(
                    "Adaptive model saved to {}",
                    adaptive_classifier::model_path().display()
                );
            }

            // Load into runtime state.
            let mut s = state.write().await;
            s.adaptive_model = Some(model);

            Json(serde_json::json!({
                "success": true,
                "trained_frames": frames,
                "accuracy": accuracy,
                "class_stats": stats,
            }))
        }
        Err(e) => Json(serde_json::json!({
            "success": false,
            "error": e,
        })),
    }
}

/// GET /api/v1/adaptive/status — check adaptive model status.
async fn adaptive_status(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let s = state.read().await;
    match &s.adaptive_model {
        Some(model) => Json(serde_json::json!({
            "loaded": true,
            "trained_frames": model.trained_frames,
            "accuracy": model.training_accuracy,
            "version": model.version,
            "classes": model.class_names,
            "class_stats": model.class_stats,
        })),
        None => Json(serde_json::json!({
            "loaded": false,
            "message": "No adaptive model. POST /api/v1/adaptive/train to train one.",
        })),
    }
}

/// POST /api/v1/adaptive/unload — unload the adaptive model (revert to thresholds).
async fn adaptive_unload(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let mut s = state.write().await;
    s.adaptive_model = None;
    Json(serde_json::json!({ "success": true, "message": "Adaptive model unloaded." }))
}

// ── D5 classification calibration (multi-RX still presence) ─────────────────

fn install_calibration_bundle(
    node_states: &mut HashMap<u8, NodeState>,
    bundle: &calibration_persistence::CalibrationBundle,
) {
    for node in node_states.values_mut() {
        node.d5_presence.reset_for_calibration();
        node.d6_fingerprint.reset_for_calibration();
        node.calibration_motion_rejected_frames = 0;
    }
    for calibration_node in &bundle.nodes {
        let Some(node) = node_states.get_mut(&calibration_node.node_id) else {
            continue;
        };
        if let Some(reference) = calibration_node.d5 {
            node.d5_presence.install_reference(reference);
        }
        if let Some(reference) = &calibration_node.d6 {
            node.d6_fingerprint.install_reference(reference.clone());
        }
    }
}

async fn classification_calibration_start(
    State(state): State<SharedState>,
    Json(request): Json<CalibrationContextRequest>,
) -> Json<serde_json::Value> {
    let (store, setup_identity) = {
        let s = state.read().await;
        (
            s.experiment_store.clone(),
            s.position_setup.as_ref().map(|setup| {
                (
                    setup.setup_id().to_string(),
                    setup.setup_sha256().to_string(),
                )
            }),
        )
    };
    let Some(store) = store else {
        return Json(serde_json::json!({
            "success": false,
            "error": "SQLite persistence is unavailable; reusable calibration cannot be recorded.",
        }));
    };
    let context = match resolve_calibration_context(
        &store,
        setup_identity
            .as_ref()
            .map(|(setup_id, setup_sha256)| (setup_id.as_str(), setup_sha256.as_str())),
        &request,
    )
    .await
    {
        Ok(context) => context,
        Err(error) => {
            return Json(serde_json::json!({
                "success": false,
                "error": error,
            }));
        }
    };

    let mut s = state.write().await;
    if s.position_setup
        .as_ref()
        .is_none_or(|setup| setup.setup_sha256() != context.setup_sha256)
    {
        return Json(serde_json::json!({
            "success": false,
            "error": "the sealed setup changed while calibration was being prepared",
        }));
    }
    let now = std::time::Instant::now();
    if let Err(error) = s.d5_presence.start_calibration(now) {
        return Json(serde_json::json!({
            "success": false,
            "error": error,
        }));
    }

    for node in s.node_states.values_mut() {
        node.d5_presence.reset_for_calibration();
        node.d6_fingerprint.reset_for_calibration();
        node.calibration_motion_rejected_frames = 0;
    }
    s.active_calibration_bundle = None;
    s.active_calibration_source = None;
    s.calibration_context = Some(context.clone());

    Json(serde_json::json!({
        "success": true,
        "status": "collecting",
        "message": "D5/D6 empty-room calibration started. Keep the room empty and keep the final physical setup unchanged.",
        "recommended_seconds": d5_presence::RECOMMENDED_CALIBRATION_SECONDS,
        "minimum_complete_blocks": d5_presence::MIN_CALIBRATION_BLOCKS,
        "minimum_samples_per_block": d5_presence::MIN_CALIBRATION_SAMPLES_PER_BLOCK,
        "block_seconds": d5_presence::CALIBRATION_BLOCK.as_secs(),
        "profile_id": context.profile_id,
        "profile_revision_id": context.profile_revision_id,
        "profile_context_sha256": context.profile_context_sha256,
        "calibration_context_sha256": context.calibration_context_sha256,
    }))
}

async fn classification_calibration_stop(
    State(state): State<SharedState>,
) -> Json<serde_json::Value> {
    let mut s = state.write().await;
    if s.d5_presence.phase() != d5_presence::CalibrationPhase::Collecting {
        return Json(serde_json::json!({
            "success": false,
            "error": "No D5/D6 classification calibration is collecting.",
        }));
    }
    let Some(started_at) = s.d5_presence.calibration_started_at() else {
        return Json(serde_json::json!({
            "success": false,
            "error": "D5/D6 calibration start time is missing.",
        }));
    };
    let now = std::time::Instant::now();

    let mut candidates: Vec<(
        u8,
        Result<d5_presence::PresenceReference, String>,
        Result<d6_fingerprint::FingerprintReference, String>,
    )> = s
        .node_states
        .iter()
        .map(|(&node_id, node)| {
            let d5_reference = if node.d5_presence.observation_ready(now) {
                node.d5_presence.build_reference(started_at, now)
            } else {
                Err(format!(
                    "accepted D5 calibration input is stale or below {:.1} Hz",
                    d5_presence::MIN_FRAME_RATE_HZ
                ))
            };
            let d6_reference = if node.d6_fingerprint.observation_ready(now) {
                node.d6_fingerprint.build_reference(started_at, now)
            } else {
                Err(format!(
                    "accepted D6 fingerprint input is stale or below {:.1} Hz",
                    d6_fingerprint::MIN_FRAME_RATE_HZ
                ))
            };
            (node_id, d5_reference, d6_reference)
        })
        .collect();
    candidates.sort_by_key(|(node_id, _, _)| *node_id);

    let ready_count = candidates
        .iter()
        .filter(|(_, _, reference)| reference.is_ok())
        .count();
    let node_results: Vec<serde_json::Value> = candidates
        .iter()
        .map(|(node_id, d5_result, d6_result)| {
            serde_json::json!({
                "node_id": node_id,
                "ready": d6_result.is_ok(),
                "d5_ready": d5_result.is_ok(),
                "d5_error": d5_result.as_ref().err(),
                "d6_ready": d6_result.is_ok(),
                "d6_error": d6_result.as_ref().err(),
            })
        })
        .collect();

    if ready_count < d5_presence::MIN_FRESH_REFERENCES {
        return Json(serde_json::json!({
            "success": false,
            "status": "collecting",
            "error": format!(
                "Only {ready_count} D6 RX fingerprints are usable; at least {} are required. Keep the room empty and continue collecting.",
                d5_presence::MIN_FRESH_REFERENCES
            ),
            "elapsed_seconds": now.saturating_duration_since(started_at).as_secs_f64(),
            "nodes": node_results,
        }));
    }

    let Some(context) = s.calibration_context.clone() else {
        return Json(serde_json::json!({
            "success": false,
            "status": "collecting",
            "error": "calibration context is missing; start it from a selected sealed setup profile",
            "nodes": node_results,
        }));
    };
    let Some(store) = s.experiment_store.clone() else {
        return Json(serde_json::json!({
            "success": false,
            "status": "collecting",
            "error": "SQLite persistence is unavailable; calibration was not stored",
            "nodes": node_results,
        }));
    };
    let calibration_nodes = candidates
        .iter()
        .filter_map(|(node_id, d5_result, d6_result)| {
            d6_result.as_ref().ok().map(|d6_reference| {
                calibration_persistence::CalibrationNodeBundle {
                    node_id: *node_id,
                    d5: d5_result.as_ref().ok().cloned(),
                    d6: Some(d6_reference.clone()),
                }
            })
        })
        .collect();
    let bundle = match calibration_persistence::CalibrationBundle::new(
        calibration_persistence::new_calibration_id(),
        &context,
        chrono::Utc::now().to_rfc3339(),
        calibration_nodes,
    ) {
        Ok(bundle) => bundle,
        Err(error) => {
            return Json(serde_json::json!({
                "success": false,
                "status": "collecting",
                "error": format!("calibration bundle is invalid: {error}"),
                "nodes": node_results,
            }));
        }
    };
    let summary = match store.persist_calibration_bundle(&bundle).await {
        Ok(summary) => summary,
        Err(error) => {
            return Json(serde_json::json!({
                "success": false,
                "status": "collecting",
                "error": format!("calibration could not be persisted: {error}"),
                "nodes": node_results,
            }));
        }
    };

    for (node_id, d5_result, d6_result) in candidates {
        let node = s
            .node_states
            .get_mut(&node_id)
            .expect("candidate node must still exist while state is write-locked");
        match d5_result {
            Ok(reference) => node.d5_presence.install_reference(reference),
            Err(_) => node.d5_presence.invalidate_reference(),
        }
        match d6_result {
            Ok(reference) => node.d6_fingerprint.install_reference(reference),
            Err(_) => node.d6_fingerprint.invalidate_reference(),
        }
    }
    s.d5_presence.finish_calibration(now);
    s.active_calibration_bundle = Some(Arc::new(bundle));
    s.active_calibration_source = Some("captured".to_string());
    let calibration_id = summary.calibration_id.clone();
    let calibration_context_sha256 = summary.calibration_context_sha256.clone();

    Json(serde_json::json!({
        "success": true,
        "status": "ready",
        "message": "D5 diagnostics and D6 static CSI fingerprints installed. The first D6 live decision needs a complete 3-second window.",
        "elapsed_seconds": now.saturating_duration_since(started_at).as_secs_f64(),
        "ready_nodes": ready_count,
        "required_votes": d5_presence::REQUIRED_VOTES,
        "minimum_fresh_references": d5_presence::MIN_FRESH_REFERENCES,
        "minimum_frame_rate_hz": d5_presence::MIN_FRAME_RATE_HZ,
        "calibration_source": "captured",
        "calibration": summary,
        "calibration_id": calibration_id,
        "calibration_context_sha256": calibration_context_sha256,
        "nodes": node_results,
    }))
}

async fn classification_calibration_status(
    State(state): State<SharedState>,
) -> Json<serde_json::Value> {
    let s = state.read().await;
    let now = std::time::Instant::now();
    let phase = s.d5_presence.phase();
    let mut nodes: Vec<serde_json::Value> = s
        .node_states
        .iter()
        .map(|(&node_id, node)| {
            let last_seen_ms = node
                .last_frame_time
                .map(|seen| now.saturating_duration_since(seen).as_millis() as u64);
            let fresh = node
                .last_frame_time
                .is_some_and(|seen| now.saturating_duration_since(seen) <= ESP32_OFFLINE_TIMEOUT);
            let d5_snapshot = node.d5_presence.snapshot(now);
            let d6_snapshot = node.d6_fingerprint.snapshot(now);
            serde_json::json!({
                "node_id": node_id,
                "fresh": fresh,
                "last_seen_ms": last_seen_ms,
                "frame_rate_hz": if node.csi_fps_samples > 0 {
                    node.csi_fps_ema
                } else {
                    0.0
                },
                "d5": d5_snapshot,
                "d6": d6_snapshot,
                "motion_rejected_frames": node.calibration_motion_rejected_frames,
            })
        })
        .collect();
    nodes.sort_by_key(|node| {
        node.get("node_id")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(u64::MAX)
    });

    let fresh_reference_nodes = s
        .node_states
        .values()
        .filter(|node| {
            node.d6_fingerprint.reference_ready() && node.d6_fingerprint.observation_fresh(now)
        })
        .count();
    let usable_live_nodes = s
        .node_states
        .values()
        .filter(|node| node.d6_fingerprint.evidence_ready(now))
        .count();
    let votes = s
        .node_states
        .values()
        .filter(|node| node.d6_fingerprint.evidence_ready(now) && node.d6_fingerprint.vote())
        .count();
    let decision_status =
        classification_decision_status(phase, s.position_setup.is_some(), usable_live_nodes);
    let operational = phase == d5_presence::CalibrationPhase::Ready
        && usable_live_nodes >= d5_presence::MIN_FRESH_REFERENCES;
    let active_calibration = s.active_calibration_bundle.as_ref().map(|bundle| {
        serde_json::json!({
            "calibration_id": bundle.calibration_id.clone(),
            "source": s.active_calibration_source.clone(),
            "profile_id": bundle.profile_id.clone(),
            "profile_revision_id": bundle.profile_revision_id.clone(),
            "profile_context_sha256": bundle.profile_context_sha256.clone(),
            "calibration_context_sha256": bundle.calibration_context_sha256.clone(),
            "setup_id": bundle.setup_id.clone(),
            "setup_sha256": bundle.setup_sha256.clone(),
            "captured_at": bundle.captured_at.clone(),
            "node_count": bundle.nodes.len(),
        })
    });

    Json(serde_json::json!({
        "success": true,
        "phase": phase.as_str(),
        "decision_status": decision_status,
        "position_setup_active": s.position_setup.is_some(),
        "collecting_seconds": s
            .d5_presence
            .calibration_started_at()
            .map(|started| now.saturating_duration_since(started).as_secs_f64()),
        "calibrated_seconds_ago": s
            .d5_presence
            .calibrated_at()
            .map(|calibrated| now.saturating_duration_since(calibrated).as_secs_f64()),
        "present": operational && s.d5_presence.present(),
        "detector": "d6_static_csi_fingerprint",
        "fresh_reference_nodes": fresh_reference_nodes,
        "usable_live_nodes": usable_live_nodes,
        "votes": votes,
        "required_votes": d5_presence::REQUIRED_VOTES,
        "minimum_fresh_references": d5_presence::MIN_FRESH_REFERENCES,
        "minimum_frame_rate_hz": d5_presence::MIN_FRAME_RATE_HZ,
        "operational": operational,
        "calibration": active_calibration,
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
        "nodes": nodes,
    }))
}

async fn classification_calibration_availability(
    State(state): State<SharedState>,
    Query(request): Query<CalibrationAvailabilityQuery>,
) -> Json<serde_json::Value> {
    let (store, setup_identity) = {
        let s = state.read().await;
        (
            s.experiment_store.clone(),
            s.position_setup.as_ref().map(|setup| {
                (
                    setup.setup_id().to_string(),
                    setup.setup_sha256().to_string(),
                )
            }),
        )
    };
    let Some(store) = store else {
        return Json(serde_json::json!({
            "success": true,
            "available": false,
            "reason": "SQLite persistence is unavailable",
        }));
    };
    let context_request = CalibrationContextRequest {
        profile_id: request.profile_id,
        profile_revision_id: request.profile_revision_id,
    };
    let context = match resolve_calibration_context(
        &store,
        setup_identity
            .as_ref()
            .map(|(setup_id, setup_sha256)| (setup_id.as_str(), setup_sha256.as_str())),
        &context_request,
    )
    .await
    {
        Ok(context) => context,
        Err(error) => {
            return Json(serde_json::json!({
                "success": true,
                "available": false,
                "reason": error,
            }));
        }
    };
    match store
        .find_compatible_calibration(
            &context.profile_id,
            &context.profile_context_sha256,
            &context.setup_id,
            &context.setup_sha256,
        )
        .await
    {
        Ok(Some(bundle)) => Json(serde_json::json!({
            "success": true,
            "available": true,
            "calibration": bundle.summary(),
        })),
        Ok(None) => Json(serde_json::json!({
            "success": true,
            "available": false,
            "reason": "no compatible empty-room calibration exists for this profile and sealed setup",
            "profile_context_sha256": context.profile_context_sha256,
            "calibration_context_sha256": context.calibration_context_sha256,
        })),
        Err(error) => Json(serde_json::json!({
            "success": true,
            "available": false,
            "reason": format!("stored calibration is unusable: {error}"),
        })),
    }
}

async fn classification_calibration_reuse(
    State(state): State<SharedState>,
    Json(request): Json<CalibrationContextRequest>,
) -> Response {
    let (store, setup_identity) = {
        let s = state.read().await;
        (
            s.experiment_store.clone(),
            s.position_setup.as_ref().map(|setup| {
                (
                    setup.setup_id().to_string(),
                    setup.setup_sha256().to_string(),
                )
            }),
        )
    };
    let Some(store) = store else {
        return experiment_api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "PERSISTENCE_UNAVAILABLE",
            "SQLite persistence is unavailable; calibration reuse is unavailable.",
        );
    };
    let context = match resolve_calibration_context(
        &store,
        setup_identity
            .as_ref()
            .map(|(setup_id, setup_sha256)| (setup_id.as_str(), setup_sha256.as_str())),
        &request,
    )
    .await
    {
        Ok(context) => context,
        Err(error) => {
            return experiment_api_error(StatusCode::CONFLICT, "CALIBRATION_CONTEXT_INVALID", error)
        }
    };
    let bundle = match store
        .find_compatible_calibration(
            &context.profile_id,
            &context.profile_context_sha256,
            &context.setup_id,
            &context.setup_sha256,
        )
        .await
    {
        Ok(Some(bundle)) => bundle,
        Ok(None) => {
            return experiment_api_error(
                StatusCode::CONFLICT,
                "CALIBRATION_NOT_AVAILABLE",
                "no compatible empty-room calibration exists for this profile and sealed setup",
            )
        }
        Err(error) => {
            return experiment_api_error(StatusCode::CONFLICT, "CALIBRATION_BUNDLE_INVALID", error)
        }
    };

    let mut s = state.write().await;
    if s.d5_presence.phase() == d5_presence::CalibrationPhase::Collecting {
        return experiment_api_error(
            StatusCode::CONFLICT,
            "CALIBRATION_RUNNING",
            "finish or cancel the active empty-room calibration before reusing a stored one",
        );
    }
    if s.position_setup.as_ref().is_none_or(|setup| {
        setup.setup_id() != context.setup_id || setup.setup_sha256() != context.setup_sha256
    }) {
        return experiment_api_error(
            StatusCode::CONFLICT,
            "CALIBRATION_CONTEXT_CHANGED",
            "the sealed setup changed while calibration reuse was being prepared",
        );
    }
    install_calibration_bundle(&mut s.node_states, &bundle);
    let now = std::time::Instant::now();
    s.d5_presence.restore_ready(now);
    s.active_calibration_bundle = Some(Arc::new(bundle.clone()));
    s.active_calibration_source = Some("reused".to_string());
    s.calibration_context = Some(context);

    Json(serde_json::json!({
        "success": true,
        "status": "ready",
        "calibration_source": "reused",
        "calibration_id": bundle.calibration_id.clone(),
        "calibration_context_sha256": bundle.calibration_context_sha256.clone(),
        "calibration": bundle.summary(),
        "message": "Stored D5/D6 empty-room references restored; no new empty-room measurement was required.",
    }))
    .into_response()
}

pub(crate) fn classification_decision_status(
    phase: d5_presence::CalibrationPhase,
    position_setup_active: bool,
    usable_live_nodes: usize,
) -> &'static str {
    match phase {
        d5_presence::CalibrationPhase::Uncalibrated if position_setup_active => "uncalibrated",
        d5_presence::CalibrationPhase::Uncalibrated => "legacy_d4",
        d5_presence::CalibrationPhase::Collecting => "calibrating",
        d5_presence::CalibrationPhase::Ready
            if usable_live_nodes >= d5_presence::MIN_FRESH_REFERENCES =>
        {
            "operational"
        }
        d5_presence::CalibrationPhase::Ready => "degraded_unknown",
    }
}

// ── Field model calibration endpoints (eigenvalue person counting) ──────────

async fn calibration_start(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let mut s = state.write().await;
    // Guard: don't discard an in-progress or fresh calibration
    if let Some(ref fm) = s.field_model {
        match fm.status() {
            CalibrationStatus::Collecting => {
                return Json(serde_json::json!({
                    "success": false,
                    "error": "Calibration already in progress. Call /calibration/stop first.",
                    "frame_count": fm.calibration_frame_count(),
                }));
            }
            CalibrationStatus::Fresh => {
                return Json(serde_json::json!({
                    "success": false,
                    "error": "A fresh calibration already exists. Call /calibration/stop or wait for expiry.",
                }));
            }
            _ => {} // Stale/Expired/Uncalibrated — ok to recalibrate
        }
    }
    match FieldModel::new(field_bridge::single_link_config()) {
        Ok(fm) => {
            s.field_model = Some(fm);
            Json(serde_json::json!({
                "success": true,
                "message": "Calibration started — keep room empty while frames accumulate.",
            }))
        }
        // ADR-080 #2: FieldModel init error chain stays server-side only.
        Err(e) => error_response::internal_error_json("calibration start", e),
    }
}

async fn calibration_stop(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let mut s = state.write().await;
    if let Some(ref mut fm) = s.field_model {
        let ts = chrono::Utc::now().timestamp_micros() as u64;
        match fm.finalize_calibration(ts, 0) {
            Ok(modes) => {
                let baseline = modes.baseline_eigenvalue_count;
                let variance_explained = modes.variance_explained;
                info!("Field model calibrated: baseline_eigenvalues={baseline}, variance_explained={variance_explained:.2}");
                Json(serde_json::json!({
                    "success": true,
                    "baseline_eigenvalue_count": baseline,
                    "variance_explained": variance_explained,
                    "frame_count": fm.calibration_frame_count(),
                }))
            }
            // ADR-080 #2: finalize error chain stays server-side only.
            Err(e) => error_response::internal_error_json("calibration stop", e),
        }
    } else {
        Json(serde_json::json!({
            "success": false,
            "error": "No field model active — call /calibration/start first.",
        }))
    }
}

async fn calibration_status(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let s = state.read().await;
    match s.field_model.as_ref() {
        Some(fm) => Json(serde_json::json!({
            "active": true,
            "status": format!("{:?}", fm.status()),
            "frame_count": fm.calibration_frame_count(),
        })),
        None => Json(serde_json::json!({
            "active": false,
            "status": "none",
        })),
    }
}

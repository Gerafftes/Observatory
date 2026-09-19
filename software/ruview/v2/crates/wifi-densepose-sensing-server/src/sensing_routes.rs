//! Live sensing, pose, stream, and introspection routes.

use super::*;

pub(crate) fn routes() -> Router<SharedState> {
    Router::new()
        .route("/api/v1/sensing/latest", get(latest))
        .route("/api/v1/pose/current", get(pose_current))
        .route("/api/v1/pose/stats", get(pose_stats))
        .route("/api/v1/pose/zones/summary", get(pose_zones_summary))
        .route("/api/v1/stream/status", get(stream_status))
        .route("/api/v1/stream/pose", get(ws_pose_handler))
        .route("/ws/sensing", get(ws_sensing_handler))
        .route("/ws/introspection", get(ws_introspection_handler))
        .route(
            "/api/v1/introspection/snapshot",
            get(api_introspection_snapshot),
        )
}

pub(crate) fn dedicated_websocket_routes() -> Router<SharedState> {
    Router::new().route("/ws/sensing", get(ws_sensing_handler))
}

// ── WebSocket handler ────────────────────────────────────────────────────────

async fn ws_sensing_handler(
    ws: WebSocketUpgrade,
    State(state): State<SharedState>,
) -> impl IntoResponse {
    ws.protocols([wifi_densepose_sensing_server::bearer_auth::WS_PROTOCOL])
        .on_upgrade(|socket| handle_ws_client(socket, state))
}

async fn handle_ws_client(mut socket: WebSocket, state: SharedState) {
    let mut rx = {
        let s = state.read().await;
        s.tx.subscribe()
    };

    info!("WebSocket client connected (sensing)");

    // ADR-044/045: ping/pong keepalive to prevent proxy idle timeouts.
    let mut ping_interval = tokio::time::interval(std::time::Duration::from_secs(30));
    ping_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            msg = rx.recv() => {
                match msg {
                    Ok(json) => {
                        if socket.send(Message::Text(json)).await.is_err() {
                            break;
                        }
                    }
                    // Lagged: client fell behind — skip missed frames, don't disconnect.
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::debug!("WS client lagged by {n} frames, skipping");
                        continue;
                    }
                    Err(_) => break, // channel closed
                }
            }
            _ = ping_interval.tick() => {
                if socket.send(Message::Ping(vec![])).await.is_err() {
                    break;
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(Message::Pong(_))) => {} // keepalive response
                    _ => {} // ignore other client messages
                }
            }
        }
    }

    info!("WebSocket client disconnected (sensing)");
}

// ── ADR-099: real-time CSI introspection — WS topic + REST snapshot ──────────
//
// Parallel to the window-aggregated `/ws/sensing` topic. Subscribers see a
// fresh `IntrospectionSnapshot` JSON frame on every accepted CSI frame
// (regime / Lyapunov exponent / top-k DTW similarity), no window-close delay.

async fn ws_introspection_handler(
    ws: WebSocketUpgrade,
    State(state): State<SharedState>,
) -> impl IntoResponse {
    ws.protocols([wifi_densepose_sensing_server::bearer_auth::WS_PROTOCOL])
        .on_upgrade(|socket| handle_ws_introspection_client(socket, state))
}

async fn handle_ws_introspection_client(mut socket: WebSocket, state: SharedState) {
    let mut rx = {
        let s = state.read().await;
        s.intro_tx.subscribe()
    };

    info!("WebSocket client connected (introspection)");

    loop {
        tokio::select! {
            msg = rx.recv() => {
                match msg {
                    Ok(json) => {
                        if socket.send(Message::Text(json)).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {} // ignore client messages
                }
            }
        }
    }

    info!("WebSocket client disconnected (introspection)");
}

/// `GET /api/v1/introspection/snapshot` — one-shot poll for the latest
/// per-frame snapshot (regime, Lyapunov, top-k similarity). Mirrors the shape
/// of `/api/v1/sensing/latest` for the dashboard one-shot path.
async fn api_introspection_snapshot(State(state): State<SharedState>) -> impl IntoResponse {
    let s = state.read().await;
    Json(s.intro.snapshot().clone())
}

// ── Pose WebSocket handler (sends pose_data messages for Live Demo) ──────────

async fn ws_pose_handler(
    ws: WebSocketUpgrade,
    State(state): State<SharedState>,
) -> impl IntoResponse {
    ws.protocols([wifi_densepose_sensing_server::bearer_auth::WS_PROTOCOL])
        .on_upgrade(|socket| handle_ws_pose_client(socket, state))
}

fn pose_source_for_frame(model_loaded: bool, model_output_present: bool) -> &'static str {
    if model_loaded && model_output_present {
        "model_inference"
    } else {
        "signal_derived"
    }
}

#[cfg(test)]
mod torso_live_gating_tests {
    use super::pose_source_for_frame;

    #[test]
    fn loaded_model_without_output_never_claims_model_inference() {
        assert_eq!(pose_source_for_frame(true, false), "signal_derived");
        assert_eq!(pose_source_for_frame(false, false), "signal_derived");
        assert_eq!(pose_source_for_frame(false, true), "signal_derived");
        assert_eq!(pose_source_for_frame(true, true), "model_inference");
    }
}

async fn handle_ws_pose_client(mut socket: WebSocket, state: SharedState) {
    let mut rx = {
        let s = state.read().await;
        s.tx.subscribe()
    };

    info!("WebSocket client connected (pose)");

    // Send connection established message
    let conn_msg = serde_json::json!({
        "type": "connection_established",
        "payload": { "status": "connected", "backend": "rust+ruvector" }
    });
    let _ = socket.send(Message::Text(conn_msg.to_string())).await;

    loop {
        tokio::select! {
            msg = rx.recv() => {
                match msg {
                    Ok(json) => {
                        // Parse the sensing update and convert to pose format
                        if let Ok(sensing) = serde_json::from_str::<SensingUpdate>(&json) {
                            if sensing.msg_type == "sensing_update" {
                                // Determine pose estimation mode for the UI indicator.
                                // "model_inference"    — this frame contains model-produced keypoints.
                                // "signal_derived"     — keypoints estimated from raw CSI features.
                                let model_loaded = {
                                    let s = state.read().await;
                                    s.model_loaded
                                };
                                let model_inference = model_loaded && sensing.pose_keypoints.is_some();
                                let pose_source = pose_source_for_frame(
                                    model_loaded,
                                    sensing.pose_keypoints.is_some(),
                                );

                                let source_offline = sensing.source.ends_with(":offline");
                                let persons = if source_offline {
                                    // Offline rebroadcasts contain only the server's
                                    // liveness state. Never derive a person from stale
                                    // features while the source is offline.
                                    Vec::new()
                                } else if model_inference {
                                    // When a trained model is loaded, prefer its keypoints if present.
                                    sensing.pose_keypoints.as_ref().map(|kps| {
                                        let kp_names = [
                                            "nose","left_eye","right_eye","left_ear","right_ear",
                                            "left_shoulder","right_shoulder","left_elbow","right_elbow",
                                            "left_wrist","right_wrist","left_hip","right_hip",
                                            "left_knee","right_knee","left_ankle","right_ankle",
                                        ];
                                        let keypoints: Vec<PoseKeypoint> = kps.iter()
                                            .enumerate()
                                            .map(|(i, kp)| PoseKeypoint {
                                                name: kp_names.get(i).unwrap_or(&"unknown").to_string(),
                                                x: kp[0], y: kp[1], z: kp[2], confidence: kp[3],
                                            })
                                            .collect();
                                        let [nx, _ny, nz] = sensing.signal_field.grid_size;
                                        let peak = field_localize::extract_peaks(
                                            &sensing.signal_field.values, nx, nz, 1, 3.0,
                                        ).into_iter().next();
                                        vec![PersonDetection {
                                            id: 1,
                                            confidence: sensing.classification.confidence,
                                            bbox: BoundingBox { x: 260.0, y: 150.0, width: 120.0, height: 220.0 },
                                            keypoints,
                                            zone: "zone_1".into(),
                                            position: peak.map_or([0.0, 0.0, 0.0], |p| p.position),
                                            motion_score: field_localize::motion_score_from_power(
                                                sensing.features.motion_band_power,
                                            ),
                                            pose: sensing.posture.clone(),
                                        }]
                                    }).unwrap_or_default()
                                } else {
                                    // Prefer tracked persons from broadcast if available
                                    sensing.persons.clone().unwrap_or_else(|| derive_pose_from_sensing(&sensing))
                                };

                                let pose_msg = serde_json::json!({
                                    "type": "pose_data",
                                    "zone_id": "zone_1",
                                    "timestamp": sensing.timestamp,
                                    "payload": {
                                        "pose": {
                                            "persons": persons,
                                        },
                                        "confidence": if sensing.classification.presence { sensing.classification.confidence } else { 0.0 },
                                        "activity": sensing.classification.motion_level,
                                        // pose_source tells the UI which estimation mode is active.
                                        "pose_source": pose_source,
                                        "metadata": {
                                            "frame_id": format!("rust_frame_{}", sensing.tick),
                                            "processing_time_ms": 1,
                                            "source": sensing.source,
                                            "tick": sensing.tick,
                                            "signal_strength": sensing.features.mean_rssi,
                                            "motion_band_power": sensing.features.motion_band_power,
                                            "breathing_band_power": sensing.features.breathing_band_power,
                                            "estimated_persons": persons.len(),
                                        }
                                    }
                                });
                                if socket.send(Message::Text(pose_msg.to_string())).await.is_err() {
                                    break;
                                }
                            }
                        }
                    }
                    // Lagged: skip missed frames, don't disconnect.
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::debug!("WS pose client lagged by {n} frames, skipping");
                        continue;
                    }
                    Err(_) => break, // channel closed
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        // Handle ping/pong
                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
                            if v.get("type").and_then(|t| t.as_str()) == Some("ping") {
                                let pong = serde_json::json!({"type": "pong"});
                                let _ = socket.send(Message::Text(pong.to_string())).await;
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(Message::Pong(_))) => {} // keepalive response
                    _ => {}
                }
            }
        }
    }

    info!("WebSocket client disconnected (pose)");
}

async fn latest(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let s = state.read().await;
    match &s.latest_update {
        Some(update) => {
            let effective_source = s.effective_source();
            let public = public_sensing_update(update, &effective_source);
            Json(serde_json::to_value(public).unwrap_or_default())
        }
        None => Json(serde_json::json!({"status": "no data yet"})),
    }
}

async fn pose_current(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let s = state.read().await;
    let effective_source = s.effective_source();
    let persons = match &s.latest_update {
        Some(update) => {
            let public = public_sensing_update(update, &effective_source);
            public
                .persons
                .clone()
                .unwrap_or_else(|| derive_pose_from_sensing(&public))
        }
        None => vec![],
    };
    Json(serde_json::json!({
        "timestamp": chrono::Utc::now().timestamp_millis() as f64 / 1000.0,
        "persons": persons,
        "total_persons": persons.len(),
        "source": effective_source,
    }))
}

async fn pose_stats(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let s = state.read().await;
    let average_confidence = s.latest_update.as_ref().and_then(|update| {
        let public = public_sensing_update(update, &s.effective_source());
        let persons = public.persons?;
        (!persons.is_empty()).then(|| {
            persons.iter().map(|person| person.confidence).sum::<f64>() / persons.len() as f64
        })
    });
    Json(serde_json::json!({
        "total_detections": s.total_detections,
        "average_confidence": average_confidence,
        "frames_processed": s.tick,
        "source": s.effective_source(),
    }))
}

async fn pose_zones_summary(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let s = state.read().await;
    let effective_source = s.effective_source();
    let mut counts = std::collections::BTreeMap::<String, usize>::new();
    if let Some(update) = s.latest_update.as_ref() {
        if let Some(persons) = public_sensing_update(update, &effective_source).persons {
            for person in persons {
                *counts.entry(person.zone).or_default() += 1;
            }
        }
    }
    let zones = counts
        .into_iter()
        .map(|(zone, person_count)| {
            (
                zone,
                serde_json::json!({
                    "person_count": person_count,
                    "status": if person_count > 0 { "occupied" } else { "clear" },
                }),
            )
        })
        .collect::<serde_json::Map<String, serde_json::Value>>();
    Json(serde_json::json!({
        "zones": zones,
    }))
}

async fn stream_status(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let s = state.read().await;
    let clients = s.tx.receiver_count();
    Json(serde_json::json!({
        "active": clients > 0,
        "clients": clients,
        "source": s.effective_source(),
    }))
}

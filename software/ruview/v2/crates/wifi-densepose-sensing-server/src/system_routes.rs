//! Node, mesh, vital-sign, model-inspection, and runtime configuration routes.

use super::*;

pub(crate) fn routes() -> Router<SharedState> {
    Router::new()
        .route("/api/v1/nodes", get(nodes_endpoint))
        .route("/api/v1/nodes/:id/sync", get(node_sync_endpoint))
        .route("/api/v1/mesh", get(mesh_endpoint))
        .route("/api/v1/mesh/metrics", get(mesh_metrics_endpoint))
        .route("/api/v1/vital-signs", get(vital_signs_endpoint))
        .route("/api/v1/edge-vitals", get(edge_vitals_endpoint))
        .route("/api/v1/edge/registry", get(edge_registry_endpoint))
        .route("/api/v1/wasm-events", get(wasm_events_endpoint))
        .route("/api/v1/model/info", get(model_info))
        .route("/api/v1/model/layers", get(model_layers))
        .route("/api/v1/model/segments", get(model_segments))
        .route("/api/v1/model/sona/profiles", get(sona_profiles))
        .route("/api/v1/model/sona/activate", post(sona_activate))
        .route(
            "/api/v1/config/dedup-factor",
            get(config_get_dedup_factor).post(config_set_dedup_factor),
        )
        .route("/api/v1/config/ground-truth", post(config_set_ground_truth))
}

async fn vital_signs_endpoint(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let s = state.read().await;
    let vs = &s.latest_vitals;
    let (br_len, br_cap, hb_len, hb_cap) = s.vital_detector.buffer_status();
    Json(serde_json::json!({
        "vital_signs": {
            "breathing_rate_bpm": vs.breathing_rate_bpm,
            "heart_rate_bpm": vs.heart_rate_bpm,
            "breathing_confidence": vs.breathing_confidence,
            "heartbeat_confidence": vs.heartbeat_confidence,
            "signal_quality": vs.signal_quality,
        },
        "buffer_status": {
            "breathing_samples": br_len,
            "breathing_capacity": br_cap,
            "heartbeat_samples": hb_len,
            "heartbeat_capacity": hb_cap,
        },
        "source": s.effective_source(),
        "tick": s.tick,
    }))
}

/// Query params for `GET /api/v1/edge/registry`.
#[derive(Debug, Deserialize)]
struct EdgeRegistryParams {
    /// `?refresh=1` bypasses the in-process cache. Logged at debug for
    /// abuse visibility. ADR-102 §"Cache semantics".
    #[serde(default)]
    refresh: Option<String>,
}

/// GET /api/v1/edge/registry — surfaces the canonical Cognitum cog catalog.
///
/// See ADR-102 (`docs/adr/ADR-102-edge-module-registry.md`) for the design
/// + trust model + security review.
async fn edge_registry_endpoint(
    Extension(reg): Extension<
        Option<Arc<wifi_densepose_sensing_server::edge_registry::EdgeRegistry>>,
    >,
    Query(params): Query<EdgeRegistryParams>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let Some(reg) = reg else {
        // --no-edge-registry, or upstream URL empty.
        return Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "edge_registry_disabled",
                "detail": "This sensing-server was started with --no-edge-registry."
            })),
        ));
    };
    let force_refresh = matches!(params.refresh.as_deref(), Some("1") | Some("true"));
    if force_refresh {
        tracing::debug!(
            event = "edge_registry.refresh_requested",
            "?refresh=1 bypassed the cache; verify this isn't being abused"
        );
    }
    match tokio::task::spawn_blocking(move || reg.get(force_refresh)).await {
        Ok(Ok(resp)) => Ok(Json(
            serde_json::to_value(resp).unwrap_or(serde_json::json!({})),
        )),
        // ADR-080 #2: the upstream error can carry an internal URL/connection
        // detail — log it server-side only and return a generic 503.
        Ok(Err(err)) => Err(error_response::upstream_unavailable("edge_registry", err)),
        // ADR-080 #2: a panicked spawn_blocking surfaces "task … panicked" via
        // JoinError::Display — never ship that to the client. Generic 500 +
        // correlation id; the panic detail is logged server-side.
        Err(join_err) => Err(error_response::internal_error("edge_registry", join_err)),
    }
}

/// GET /api/v1/edge-vitals — latest edge vitals from ESP32 (ADR-039).
async fn edge_vitals_endpoint(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let s = state.read().await;
    match &s.edge_vitals {
        Some(vitals) => {
            let classification = s
                .latest_update
                .as_ref()
                .filter(|update| update.source == "esp32")
                .map(|update| public_sensing_update(update, &s.effective_source()).classification)
                .unwrap_or_else(|| {
                    apply_position_setup_classification_gate(
                        s.position_setup.is_some(),
                        s.d5_presence.phase(),
                        edge_vitals_classification(vitals),
                    )
                });
            let public_vitals = public_edge_vitals_packet(vitals, &classification);
            Json(serde_json::json!({
                "status": "ok",
                "edge_vitals": public_vitals,
            }))
        }
        None => Json(serde_json::json!({
            "status": "no_data",
            "edge_vitals": null,
            "message": "No edge vitals packet received yet. Ensure ESP32 edge_tier >= 1.",
        })),
    }
}

/// GET /api/v1/wasm-events — latest WASM events from ESP32 (ADR-040).
async fn wasm_events_endpoint(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let s = state.read().await;
    match &s.latest_wasm_events {
        Some(w) => Json(serde_json::json!({
            "status": "ok",
            "wasm_events": w,
        })),
        None => Json(serde_json::json!({
            "status": "no_data",
            "wasm_events": null,
            "message": "No WASM output packet received yet. Upload and start a .wasm module on the ESP32.",
        })),
    }
}

async fn model_info(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let s = state.read().await;
    match &s.rvf_info {
        Some(info) => Json(serde_json::json!({
            "status": "loaded",
            "container": info,
        })),
        None => Json(serde_json::json!({
            "status": "no_model",
            "message": "No RVF container loaded. Use --load-rvf <path> to load one.",
        })),
    }
}

async fn model_layers(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let s = state.read().await;
    match &s.progressive_loader {
        Some(loader) => {
            let (a, b, c) = loader.layer_status();
            Json(serde_json::json!({
                "layer_a": a,
                "layer_b": b,
                "layer_c": c,
                "progress": loader.loading_progress(),
            }))
        }
        None => Json(serde_json::json!({
            "layer_a": false,
            "layer_b": false,
            "layer_c": false,
            "progress": 0.0,
            "message": "No model loaded with progressive loading",
        })),
    }
}

async fn model_segments(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let s = state.read().await;
    match &s.progressive_loader {
        Some(loader) => Json(serde_json::json!({ "segments": loader.segment_list() })),
        None => Json(serde_json::json!({ "segments": [] })),
    }
}

async fn sona_profiles(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let s = state.read().await;
    let names = s
        .progressive_loader
        .as_ref()
        .map(|l| l.sona_profile_names())
        .unwrap_or_default();
    let active = s.active_sona_profile.clone().unwrap_or_default();
    Json(serde_json::json!({ "profiles": names, "active": active }))
}

async fn sona_activate(
    State(state): State<SharedState>,
    Json(body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let profile = body
        .get("profile")
        .and_then(|p| p.as_str())
        .unwrap_or("")
        .to_string();

    let mut s = state.write().await;
    let available = s
        .progressive_loader
        .as_ref()
        .map(|l| l.sona_profile_names())
        .unwrap_or_default();

    if available.contains(&profile) {
        s.active_sona_profile = Some(profile.clone());
        Json(serde_json::json!({ "status": "activated", "profile": profile }))
    } else {
        Json(serde_json::json!({
            "status": "error",
            "message": format!("Profile '{}' not found. Available: {:?}", profile, available),
        }))
    }
}

/// GET /api/v1/nodes — per-node health and feature info.
/// ADR-110 iter 29 — per-node mesh sync snapshot via HTTP.
///
/// GET /api/v1/nodes/:id/sync
///   200 → Json(NodeSyncSnapshot) when latest_sync is present
///   404 → {"error": "no_sync", "node_id": N} otherwise
///
/// Complements the WebSocket `sync` field (iter 23) for clients that
/// can't hold a streaming connection (curl scripts, Home Assistant REST
/// sensors, automation rule probes).
async fn node_sync_endpoint(
    State(state): State<SharedState>,
    Path(id): Path<u8>,
) -> Result<Json<NodeSyncSnapshot>, (StatusCode, Json<serde_json::Value>)> {
    let s = state.read().await;
    let ns = s.node_states.get(&id).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "unknown_node", "node_id": id,
            })),
        )
    })?;
    ns.sync_snapshot().map(Json).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "no_sync", "node_id": id,
                "hint": "node hasn't emitted a sync packet yet (no mesh peer or not v0.6.9+)",
            })),
        )
    })
}

/// ADR-110 iter 29 — fleet-wide mesh state via HTTP.
///
/// GET /api/v1/mesh
///   200 → { "nodes": { "<id>": NodeSyncSnapshot, ... }, "total": N }
///   Nodes without a recent sync are omitted from the map; an empty
///   `nodes` object means no mesh peers reachable.
/// ADR-110 iter 36 — Prometheus exposition format for mesh state.
///
/// GET /api/v1/mesh/metrics → text/plain
///   wifi_densepose_mesh_offset_us{node="N"} <signed-int>
///   wifi_densepose_mesh_is_leader{node="N"} 0|1
///   wifi_densepose_mesh_is_valid{node="N"} 0|1
///   wifi_densepose_mesh_smoothed{node="N"} 0|1
///   wifi_densepose_mesh_sequence{node="N"} <u32>
///   wifi_densepose_mesh_csi_fps{node="N"} <float>
///   wifi_densepose_mesh_csi_fps_samples{node="N"} <u32>
///   wifi_densepose_mesh_staleness_ms{node="N"} <u64>
///
/// Spec: <https://prometheus.io/docs/instrumenting/exposition_formats/>.
/// Each metric is a gauge labeled by node_id. Nodes without a fresh sync
/// are simply absent from the output (Prometheus handles missing series
/// natively — the scrape just reports them as stale after the configured
/// staleness duration).
async fn mesh_metrics_endpoint(State(state): State<SharedState>) -> impl IntoResponse {
    use std::fmt::Write;
    let s = state.read().await;
    let mut body = String::with_capacity(1024);

    // Each metric: HELP + TYPE header + one line per node that has a snapshot.
    let metrics: &[(&str, &str, &str)] = &[
        (
            "wifi_densepose_mesh_offset_us",
            "Cross-board mesh-aligned offset, microseconds (signed)",
            "gauge",
        ),
        (
            "wifi_densepose_mesh_is_leader",
            "1 if this node is the elected mesh leader, else 0",
            "gauge",
        ),
        (
            "wifi_densepose_mesh_is_valid",
            "1 if this node has heard a fresh leader beacon, else 0",
            "gauge",
        ),
        (
            "wifi_densepose_mesh_smoothed",
            "1 once the firmware-side EMA filter has seeded, else 0",
            "gauge",
        ),
        (
            "wifi_densepose_mesh_sequence",
            "High-water CSI sequence at sync emit time",
            "gauge",
        ),
        (
            "wifi_densepose_mesh_csi_fps",
            "Per-node measured CSI frame rate (Hz)",
            "gauge",
        ),
        (
            "wifi_densepose_mesh_csi_fps_samples",
            "How many inter-frame deltas the fps EMA has seen",
            "gauge",
        ),
        (
            "wifi_densepose_mesh_staleness_ms",
            "Milliseconds since the host last received this node's sync packet",
            "gauge",
        ),
    ];

    // Collect (id, snapshot) pairs once so each metric loop reads the same set.
    let snaps: Vec<(u8, NodeSyncSnapshot)> = s
        .node_states
        .iter()
        .filter_map(|(&id, ns)| ns.sync_snapshot().map(|snap| (id, snap)))
        .collect();

    // Iter 37: fleet cardinality summary — Ops dashboards want the
    // "how many leaders / followers / no-sync" tally at a glance
    // without scraping every per-node series and counting.
    let (leaders, followers) = fleet_role_counts(&snaps);
    let no_sync = s.node_states.len().saturating_sub(snaps.len()) as u64;
    let _ = writeln!(
        body,
        "# HELP wifi_densepose_mesh_node_total Per-state node count across the fleet"
    );
    let _ = writeln!(body, "# TYPE wifi_densepose_mesh_node_total gauge");
    let _ = writeln!(
        body,
        "wifi_densepose_mesh_node_total{{state=\"leader\"}} {leaders}"
    );
    let _ = writeln!(
        body,
        "wifi_densepose_mesh_node_total{{state=\"follower\"}} {followers}"
    );
    let _ = writeln!(
        body,
        "wifi_densepose_mesh_node_total{{state=\"no_sync\"}} {no_sync}"
    );

    for (name, help, kind) in metrics {
        let _ = writeln!(body, "# HELP {name} {help}");
        let _ = writeln!(body, "# TYPE {name} {kind}");
        for (id, snap) in &snaps {
            let value = match *name {
                "wifi_densepose_mesh_offset_us" => snap.offset_us.to_string(),
                "wifi_densepose_mesh_is_leader" => bool_metric(snap.is_leader),
                "wifi_densepose_mesh_is_valid" => bool_metric(snap.is_valid),
                "wifi_densepose_mesh_smoothed" => bool_metric(snap.smoothed),
                "wifi_densepose_mesh_sequence" => snap.sequence.to_string(),
                "wifi_densepose_mesh_csi_fps" => format!("{:.3}", snap.csi_fps_ema),
                "wifi_densepose_mesh_csi_fps_samples" => snap.csi_fps_samples.to_string(),
                "wifi_densepose_mesh_staleness_ms" => snap
                    .staleness_ms
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| "0".into()),
                _ => continue,
            };
            let _ = writeln!(body, "{name}{{node=\"{id}\"}} {value}");
        }
    }
    (
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4",
        )],
        body,
    )
}

pub(crate) fn bool_metric(b: bool) -> String {
    (if b { 1 } else { 0 }).to_string()
}

/// ADR-110 iter 37 — count (leaders, followers) in a populated snapshot set.
/// Free function for testability — same pattern as iter 18's `update_csi_fps_ema`.
pub(crate) fn fleet_role_counts(snaps: &[(u8, NodeSyncSnapshot)]) -> (u64, u64) {
    let leaders = snaps.iter().filter(|(_, s)| s.is_leader).count() as u64;
    let followers = (snaps.len() as u64).saturating_sub(leaders);
    (leaders, followers)
}

async fn mesh_endpoint(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let s = state.read().await;
    let mut nodes = serde_json::Map::new();
    for (&id, ns) in s.node_states.iter() {
        if let Some(snap) = ns.sync_snapshot() {
            nodes.insert(id.to_string(), serde_json::to_value(snap).unwrap());
        }
    }
    let total = nodes.len();
    Json(serde_json::json!({
        "nodes": serde_json::Value::Object(nodes),
        "total": total,
    }))
}

pub(crate) fn source_binding_consistent_across_nodes(
    node_states: &HashMap<u8, NodeState>,
    now: std::time::Instant,
) -> bool {
    const EXPECTED_RX_IDS: [u8; 4] = [1, 2, 3, 4];
    let mut active: Vec<(u8, &NodeState)> = node_states
        .iter()
        .filter(|(_, node)| {
            node.last_frame_time
                .is_some_and(|seen| now.saturating_duration_since(seen) <= ESP32_OFFLINE_TIMEOUT)
        })
        .map(|(&id, node)| (id, node))
        .collect();
    active.sort_by_key(|(id, _)| *id);
    if active.iter().map(|(id, _)| *id).collect::<Vec<_>>() != EXPECTED_RX_IDS {
        return false;
    }

    let Some(expected_identity) = active.first().and_then(|(_, node)| {
        node.source_binding_observation
            .as_ref()
            .filter(|binding| binding.complete && binding.is_fresh(now))
            .map(|binding| binding.tx_filter_identity.as_str())
    }) else {
        return false;
    };
    active.iter().all(|(_, node)| {
        node.source_binding_observation
            .as_ref()
            .is_some_and(|binding| {
                binding.complete
                    && binding.is_fresh(now)
                    && binding.tx_filter_identity == expected_identity
            })
    })
}

async fn nodes_endpoint(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let s = state.read().await;
    let now = std::time::Instant::now();
    let nodes = public_node_summaries(
        &s.node_states,
        now,
        s.d5_presence.phase(),
        s.position_setup.is_some(),
    );
    Json(serde_json::json!({
        "nodes": nodes,
        "total": nodes.len(),
        "source_binding_consistent_across_nodes": source_binding_consistent_across_nodes(
            &s.node_states,
            now,
        ),
    }))
}

pub(crate) fn public_node_summaries(
    node_states: &HashMap<u8, NodeState>,
    now: std::time::Instant,
    phase: d5_presence::CalibrationPhase,
    position_setup_active: bool,
) -> Vec<serde_json::Value> {
    let classifications: HashMap<u8, ClassificationInfo> =
        build_node_features(node_states, now, phase, position_setup_active)
            .unwrap_or_default()
            .into_iter()
            .map(|entry| (entry.node_id, entry.classification))
            .collect();
    let mut nodes: Vec<serde_json::Value> = node_states
        .iter()
        .map(|(&id, ns)| {
            let elapsed_ms = ns
                .last_frame_time
                .map(|t| now.saturating_duration_since(t).as_millis() as u64)
                .unwrap_or(999999);
            let stale = elapsed_ms > 5000;
            let status = if stale { "stale" } else { "active" };
            let binding_last_seen_ms = ns.source_binding_observation.as_ref().map(|binding| {
                now.saturating_duration_since(binding.observed_at)
                    .as_millis() as u64
            });
            // Source attestation is independent of the selected CSI grid. A
            // complete identity-valid off-grid frame refreshes this evidence
            // but is still excluded from every sensing and recording path.
            // These booleans expose no configured MAC or private digest.
            let source_binding_attested = ns
                .source_binding_observation
                .as_ref()
                .is_some_and(|binding| binding.complete && binding.is_fresh(now));
            let identity_matches_setup = source_binding_attested
                && position_setup_active
                && ns
                    .source_binding_observation
                    .as_ref()
                    .is_some_and(|binding| binding.matches_setup);
            let rssi = ns.rssi_history.back().copied().unwrap_or(-90.0);
            let classification = classifications.get(&id);
            let motion_level = classification
                .map(|classification| classification.motion_level.as_str())
                .unwrap_or("unknown");
            let person_count =
                if classification.is_some_and(|classification| classification.presence) {
                    ns.prev_person_count
                } else {
                    0
                };
            let sequence_total = ns
                .sequence_observations
                .saturating_add(ns.inferred_lost_frames);
            serde_json::json!({
                "node_id": id,
                "display_name": format!("RX{id}"),
                "role": "receiver",
                "status": status,
                "last_seen_ms": elapsed_ms,
                "rssi_dbm": rssi,
                "frame_rate_hz": (ns.csi_fps_samples > 0).then_some(ns.csi_fps_ema),
                "frame_rate_samples": ns.csi_fps_samples,
                "latest_sequence": ns.latest_sequence,
                "accepted_csi_frames": ns.accepted_csi_frames,
                "inferred_lost_frames": ns.inferred_lost_frames,
                "sequence_observations": ns.sequence_observations,
                "packet_loss_percent": (sequence_total > 0).then_some(
                    (ns.inferred_lost_frames as f64 / sequence_total as f64) * 100.0,
                ),
                "sync": ns.sync_snapshot(),
                "time_quality": {
                    "host_arrival_monotonic_ns": ns.latest_host_monotonic_ns,
                    "host_arrival_age_ms": elapsed_ms,
                    "mesh_timestamp_us": ns.latest_frame_mesh_time_us,
                    "mesh_status": ns.latest_mesh_time_status,
                    "fusion_queue_depth": ns.fusion_frame_history.len(),
                    "candidate_time_source": if ns.latest_frame_mesh_time_us.is_some() {
                        "mesh"
                    } else {
                        "host_monotonic"
                    },
                    "mesh_time_reset_count": ns.mesh_time_reset_count,
                },
                "motion_level": motion_level,
                "person_count": person_count,
                "source_binding_attested": source_binding_attested,
                "filter_enforced": source_binding_attested,
                "source_matched_filter": source_binding_attested,
                "identity_valid": source_binding_attested,
                "identity_matches_setup": identity_matches_setup,
                "binding_last_seen_ms": binding_last_seen_ms,
                "skipped_grid_frames": ns.skipped_grid_frames,
            })
        })
        .collect();
    nodes.sort_by_key(|node| {
        node.get("node_id")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(u64::MAX)
    });
    nodes
}

// ── ADR-044 §5.3: dedup_factor runtime configuration endpoints ────────────────

/// `GET /api/v1/config/dedup-factor` — read the current dedup factor.
async fn config_get_dedup_factor(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let s = state.read().await;
    Json(serde_json::json!({
        "dedup_factor": s.dedup_factor,
        "description": "Divisor for multi-node person count deduplication (sum / factor). Range: 1.0–10.0."
    }))
}

/// `POST /api/v1/config/dedup-factor` — set the dedup factor (clamped 1.0–10.0).
///
/// Body: `{ "value": <f64> }`
async fn config_set_dedup_factor(
    State(state): State<SharedState>,
    Json(body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let value = body.get("value").and_then(|v| v.as_f64()).unwrap_or(3.0);
    let clamped = value.clamp(1.0, 10.0);
    let mut s = state.write().await;
    s.dedup_factor = clamped;
    let data_dir = s.data_dir.clone();
    drop(s);
    save_runtime_config(
        &data_dir,
        &RuntimeConfig {
            dedup_factor: clamped,
        },
    );
    Json(serde_json::json!({
        "status": "ok",
        "dedup_factor": clamped,
    }))
}

/// `POST /api/v1/config/ground-truth` — auto-tune dedup factor from a known person count.
///
/// Derives `dedup_factor = raw_node_sum / ground_truth_count` from the current
/// per-node person counts, clamped to [1.0, 10.0].  Persisted immediately.
///
/// Body: `{ "count": <u64> }`
async fn config_set_ground_truth(
    State(state): State<SharedState>,
    Json(body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let ground_truth = match body.get("count").and_then(|v| v.as_u64()) {
        Some(n) if n > 0 => n as usize,
        _ => return Json(serde_json::json!({"error": "count must be a positive integer"})),
    };
    let mut s = state.write().await;
    let raw_sum: usize = s
        .node_states
        .values()
        .filter(|ns| {
            ns.last_frame_time
                .map(|t| t.elapsed() < std::time::Duration::from_secs(10))
                .unwrap_or(false)
        })
        .map(|ns| ns.prev_person_count)
        .sum();
    let optimal = if raw_sum > 0 {
        (raw_sum as f64) / (ground_truth as f64)
    } else {
        3.0
    };
    let clamped = optimal.clamp(1.0, 10.0);
    s.dedup_factor = clamped;
    let data_dir = s.data_dir.clone();
    drop(s);
    save_runtime_config(
        &data_dir,
        &RuntimeConfig {
            dedup_factor: clamped,
        },
    );
    Json(serde_json::json!({
        "status": "ok",
        "ground_truth": ground_truth,
        "raw_sum": raw_sum,
        "computed_dedup_factor": clamped,
    }))
}

//! HTTP contract for the independent mmWave reference and calibration teacher.

use super::route_support::{resolve_calibration_context, CalibrationContextRequest};
use super::*;

pub(crate) fn routes() -> Router<SharedState> {
    Router::new()
        .route("/api/v1/mmwave/status", get(mmwave_status_endpoint))
        .route("/api/v1/mmwave/mode", put(mmwave_mode_endpoint))
        .route("/api/v1/mmwave/transform", put(mmwave_transform_endpoint))
        .route(
            "/api/v1/mmwave/session/start",
            post(mmwave_session_start_endpoint),
        )
        .route("/api/v1/mmwave/session/status", get(mmwave_status_endpoint))
        .route(
            "/api/v1/mmwave/session/stop",
            post(mmwave_session_stop_endpoint),
        )
        .route(
            "/api/v1/mmwave/known-point/check",
            post(mmwave_known_point_check_endpoint),
        )
        .route(
            "/api/v1/mmwave/fixed-points/start",
            post(mmwave_fixed_points_start_endpoint),
        )
        .route(
            "/api/v1/mmwave/fixed-points/check",
            post(mmwave_fixed_points_check_endpoint),
        )
        .route(
            "/api/v1/mmwave/fixed-points/cancel",
            post(mmwave_fixed_points_cancel_endpoint),
        )
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MmwaveModeRequest {
    mode: mmwave_calibration::MeasurementMode,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MmwaveSessionStartRequest {
    kind: mmwave_calibration::SessionKind,
    #[serde(default)]
    policy: Option<mmwave_calibration::CalibrationPolicy>,
    #[serde(default)]
    calibration_context: Option<CalibrationContextRequest>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct KnownPointCheckRequest {
    /// Expected floor position in the room's X/Z axes, in metres.
    expected_position_m: [f64; 2],
    #[serde(default = "default_known_point_tolerance_mm")]
    tolerance_mm: u32,
    #[serde(default = "default_known_point_window_ms")]
    window_ms: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FixedPointCheckRequest {
    #[serde(default = "default_known_point_tolerance_mm")]
    tolerance_mm: u32,
    #[serde(default = "default_known_point_window_ms")]
    window_ms: u64,
}

fn default_known_point_tolerance_mm() -> u32 {
    350
}

fn default_known_point_window_ms() -> u64 {
    1_500
}

fn validate_mmwave_calibration_context(
    kind: mmwave_calibration::SessionKind,
    context: Option<CalibrationContextRequest>,
) -> Result<Option<CalibrationContextRequest>, String> {
    match (kind, context) {
        (mmwave_calibration::SessionKind::Calibration, Some(context)) => Ok(Some(context)),
        (mmwave_calibration::SessionKind::Calibration, None) => Err(
            "calibration sessions require calibration_context with a persisted setup profile"
                .to_string(),
        ),
        (mmwave_calibration::SessionKind::Blind, Some(_)) => {
            Err("blind sessions must not provide calibration_context".to_string())
        }
        (mmwave_calibration::SessionKind::Blind, None) => Ok(None),
        (mmwave_calibration::SessionKind::MmwaveOnly, Some(_)) => {
            Err("mmWave-only sessions use the active CAD profile and must not provide calibration_context".to_string())
        }
        (mmwave_calibration::SessionKind::MmwaveOnly, None) => Ok(None),
    }
}

#[cfg(test)]
mod mmwave_session_context_tests {
    use super::{
        mmwave_calibration, mode_change_required, validate_mmwave_calibration_context,
        MmwaveSessionStartRequest,
    };

    #[test]
    fn fixed_point_repeat_skips_redundant_calibration_mode_request() {
        assert!(!mode_change_required(
            Some(mmwave_calibration::MeasurementMode::Calibration),
            mmwave_calibration::MeasurementMode::Calibration,
        ));
        assert!(mode_change_required(
            Some(mmwave_calibration::MeasurementMode::Reference),
            mmwave_calibration::MeasurementMode::Calibration,
        ));
    }

    #[test]
    fn calibration_session_requires_a_persisted_profile_context() {
        let request: MmwaveSessionStartRequest = serde_json::from_value(serde_json::json!({
            "kind": "calibration"
        }))
        .expect("parse calibration request");

        let error = validate_mmwave_calibration_context(request.kind, request.calibration_context)
            .expect_err("contextless calibration must fail closed");

        assert!(error.contains("require calibration_context"));
    }

    #[test]
    fn calibration_session_preserves_exact_profile_identity() {
        let request: MmwaveSessionStartRequest = serde_json::from_value(serde_json::json!({
            "kind": "calibration",
            "calibration_context": {
                "profile_id": "profile-fixed-room",
                "profile_revision_id": "profile-fixed-room-v7"
            }
        }))
        .expect("parse calibration request");

        let context =
            validate_mmwave_calibration_context(request.kind, request.calibration_context)
                .expect("context-bound calibration must be accepted")
                .expect("calibration context must be returned");

        assert_eq!(context.profile_id, "profile-fixed-room");
        assert_eq!(
            context.profile_revision_id.as_deref(),
            Some("profile-fixed-room-v7")
        );
    }

    #[test]
    fn blind_session_rejects_a_calibration_context() {
        let request: MmwaveSessionStartRequest = serde_json::from_value(serde_json::json!({
            "kind": "blind",
            "calibration_context": {
                "profile_id": "profile-fixed-room"
            }
        }))
        .expect("parse blind request");

        let error = validate_mmwave_calibration_context(request.kind, request.calibration_context)
            .expect_err("blind request must not accept calibration context");

        assert!(error.contains("must not provide calibration_context"));
        assert_eq!(request.kind, mmwave_calibration::SessionKind::Blind);
    }
}

type MmwaveApiResult = Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)>;

const MMWAVE_SESSION_START_SETTLE_TIMEOUT: Duration = Duration::from_secs(8);
const MMWAVE_NODE_STREAM_ADVANCE_TIMEOUT: Duration = Duration::from_secs(2);
const MMWAVE_NODE_STREAM_ADVANCE_POLL_INTERVAL: Duration = Duration::from_millis(100);

fn expected_mmwave_mode(
    kind: mmwave_calibration::SessionKind,
) -> mmwave_calibration::MeasurementMode {
    match kind {
        mmwave_calibration::SessionKind::Calibration => {
            mmwave_calibration::MeasurementMode::Calibration
        }
        mmwave_calibration::SessionKind::Blind => mmwave_calibration::MeasurementMode::Reference,
        mmwave_calibration::SessionKind::MmwaveOnly => {
            mmwave_calibration::MeasurementMode::Calibration
        }
    }
}

fn mode_change_required(
    current: Option<mmwave_calibration::MeasurementMode>,
    expected: mmwave_calibration::MeasurementMode,
) -> bool {
    current != Some(expected)
}

fn node_diagnostics_show_streaming(
    before: &mmwave_calibration::NodeDiagnostics,
    after: &mmwave_calibration::NodeDiagnostics,
) -> bool {
    after.uart_bytes_received > 0
        && after.radar_frames_valid > 0
        && after.udp_packets_sent > 0
        && after
            .udp_send_failures
            .saturating_sub(before.udp_send_failures)
            == 0
        && (after.radar_frames_valid > before.radar_frames_valid
            || after.udp_packets_sent > before.udp_packets_sent)
}

async fn wait_for_mmwave_node_stream_advance(
    control: &mmwave_calibration::NodeControl,
    before: &mmwave_calibration::NodeDiagnostics,
) -> Result<mmwave_calibration::NodeDiagnostics, String> {
    let deadline = std::time::Instant::now() + MMWAVE_NODE_STREAM_ADVANCE_TIMEOUT;
    let mut latest = before.clone();

    loop {
        let diagnostics_control = control.clone();
        let last_request_error = match tokio::task::spawn_blocking(move || {
            mmwave_calibration::get_node_diagnostics(&diagnostics_control)
        })
        .await
        {
            Ok(Ok(diagnostics)) => {
                latest = diagnostics;
                if node_diagnostics_show_streaming(before, &latest) {
                    return Ok(latest);
                }
                None
            }
            Ok(Err(error)) => Some(error),
            Err(error) => Some(error.to_string()),
        };

        if std::time::Instant::now() >= deadline {
            let request_error = last_request_error
                .map(|error| format!(", last_status_error={error}"))
                .unwrap_or_default();
            return Err(format!(
                "mmWave node diagnostics did not advance cleanly within {} ms (radar_frames {} -> {}, udp_sent {} -> {}, udp_failures {} -> {}{})",
                MMWAVE_NODE_STREAM_ADVANCE_TIMEOUT.as_millis(),
                before.radar_frames_valid,
                latest.radar_frames_valid,
                before.udp_packets_sent,
                latest.udp_packets_sent,
                before.udp_send_failures,
                latest.udp_send_failures,
                request_error,
            ));
        }

        tokio::time::sleep(MMWAVE_NODE_STREAM_ADVANCE_POLL_INTERVAL).await;
    }
}

#[cfg(test)]
mod mmwave_node_stream_advance_tests {
    use super::{mmwave_calibration, node_diagnostics_show_streaming};

    fn diagnostics(
        radar_frames: u64,
        udp_sent: u64,
        udp_failures: u64,
    ) -> mmwave_calibration::NodeDiagnostics {
        mmwave_calibration::NodeDiagnostics {
            uart_bytes_received: 100,
            radar_frames_valid: radar_frames,
            udp_packets_sent: udp_sent,
            udp_send_failures: udp_failures,
            ..Default::default()
        }
    }

    #[test]
    fn unchanged_snapshot_is_not_stream_progress() {
        let before = diagnostics(20, 10, 3);

        assert!(!node_diagnostics_show_streaming(&before, &before));
    }

    #[test]
    fn later_radar_frame_is_stream_progress() {
        let before = diagnostics(20, 10, 3);
        let after = diagnostics(21, 10, 3);

        assert!(node_diagnostics_show_streaming(&before, &after));
    }

    #[test]
    fn new_udp_failure_keeps_start_fail_closed() {
        let before = diagnostics(20, 10, 3);
        let after = diagnostics(21, 11, 4);

        assert!(!node_diagnostics_show_streaming(&before, &after));
    }
}

async fn wait_for_mmwave_mode_and_preflight(
    state: &SharedState,
    kind: mmwave_calibration::SessionKind,
    expected_mode: mmwave_calibration::MeasurementMode,
) -> Result<(), String> {
    let deadline = std::time::Instant::now() + MMWAVE_SESSION_START_SETTLE_TIMEOUT;
    loop {
        let status = {
            let state = state.read().await;
            state.mmwave.status(server_clock::now().host_monotonic_ns)
        };
        let preflight_ready = match kind {
            mmwave_calibration::SessionKind::MmwaveOnly => status.radar_preflight_ready(),
            mmwave_calibration::SessionKind::Calibration
            | mmwave_calibration::SessionKind::Blind => status.preflight_ready(),
        };
        if status.mode == Some(expected_mode) && preflight_ready {
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            return Err(format!(
                "mmWave start preflight did not recover within {} seconds (mode={:?}, expected={:?}, ready={})",
                MMWAVE_SESSION_START_SETTLE_TIMEOUT.as_secs(),
                status.mode,
                expected_mode,
                preflight_ready
            ));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn wait_for_mmwave_mode_and_radar_preflight(
    state: &SharedState,
    expected_mode: mmwave_calibration::MeasurementMode,
) -> Result<(), String> {
    let deadline = std::time::Instant::now() + MMWAVE_SESSION_START_SETTLE_TIMEOUT;
    loop {
        let status = {
            let state = state.read().await;
            state.mmwave.status(server_clock::now().host_monotonic_ns)
        };
        if status.mode == Some(expected_mode) && status.radar_preflight_ready() {
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            return Err(format!(
                "mmWave fixed-point preflight did not recover within {} seconds (mode={:?}, expected={:?}, ready={})",
                MMWAVE_SESSION_START_SETTLE_TIMEOUT.as_secs(),
                status.mode,
                expected_mode,
                status.radar_preflight_ready()
            ));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn mmwave_api_error(
    status: StatusCode,
    message: impl Into<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    (status, Json(serde_json::json!({ "error": message.into() })))
}

async fn mmwave_status_endpoint(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let (mut status, diagnostics, node_control, connection) = {
        let state = state.read().await;
        let status = state.mmwave.status(server_clock::now().host_monotonic_ns);
        let node_control = state.mmwave_node_diagnostics.status(
            status.node_control.url_configured,
            status.node_control.token_configured,
        );
        (
            status,
            state
                .mmwave_connection
                .reachable
                .then(|| state.mmwave_node_diagnostics.snapshot()),
            node_control,
            state.mmwave_connection.clone(),
        )
    };
    if let Some(diagnostics) = diagnostics {
        status.attach_node_diagnostics_window(diagnostics);
    }
    status.node_control = node_control;
    // HTTP reachability is independent of optional firmware diagnostic counters.
    status.node_control.reachable = Some(connection.reachable);
    if connection.reachable && status.node_status_error.is_some() {
        status.node_control.last_error_kind = Some("diagnostics_unavailable".to_string());
    }
    status.node_control.url_configured =
        connection.node_url.is_some() || status.node_control.url_configured;
    let mut response = serde_json::to_value(status).expect("mmWave status is serializable");
    response["connection"] =
        serde_json::to_value(connection).expect("connection status is serializable");
    Json(response)
}

async fn mmwave_mode_endpoint(
    State(state): State<SharedState>,
    Json(request): Json<MmwaveModeRequest>,
) -> MmwaveApiResult {
    let control = state.read().await.mmwave.control().ok_or_else(|| {
        mmwave_api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "mmWave node control is not configured",
        )
    })?;
    tokio::task::spawn_blocking(move || mmwave_calibration::set_node_mode(&control, request.mode))
        .await
        .map_err(|error| mmwave_api_error(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?
        .map_err(|error| mmwave_api_error(StatusCode::BAD_GATEWAY, error))?;
    Ok(Json(serde_json::json!({ "mode": request.mode })))
}

async fn mmwave_transform_endpoint(
    State(state): State<SharedState>,
    Json(request): Json<mmwave_calibration::TransformRequest>,
) -> MmwaveApiResult {
    let control = {
        let state = state.read().await;
        if !state.mmwave.transform_reconfiguration_allowed() {
            return Err(mmwave_api_error(
                StatusCode::CONFLICT,
                "the transform cannot change during a session or after setup sealing",
            ));
        }
        state.mmwave.control().ok_or_else(|| {
            mmwave_api_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "mmWave node control is not configured",
            )
        })?
    };
    let response = request.clone();
    tokio::task::spawn_blocking(move || mmwave_calibration::set_node_transform(&control, &request))
        .await
        .map_err(|error| mmwave_api_error(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?
        .map_err(|error| mmwave_api_error(StatusCode::BAD_GATEWAY, error))?;
    state.write().await.mmwave.clear_observed_transform();
    Ok(Json(serde_json::json!({ "transform": response })))
}

async fn mmwave_session_start_endpoint(
    State(state): State<SharedState>,
    Json(request): Json<MmwaveSessionStartRequest>,
) -> MmwaveApiResult {
    let MmwaveSessionStartRequest {
        kind,
        policy,
        calibration_context,
    } = request;
    let policy = policy
        .unwrap_or_default()
        .validate()
        .map_err(|error| mmwave_api_error(StatusCode::BAD_REQUEST, error))?;
    let calibration_context_request =
        validate_mmwave_calibration_context(kind, calibration_context)
            .map_err(|error| mmwave_api_error(StatusCode::BAD_REQUEST, error))?;
    let calibration_context = if let Some(request) = calibration_context_request.as_ref() {
        let (store, setup_identity) = {
            let state = state.read().await;
            (
                state.experiment_store.clone(),
                state.position_setup.as_ref().map(|setup| {
                    (
                        setup.setup_id().to_string(),
                        setup.setup_sha256().to_string(),
                    )
                }),
            )
        };
        let store = store.ok_or_else(|| {
            mmwave_api_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "SQLite persistence is unavailable; calibration context cannot be resolved",
            )
        })?;
        Some(
            resolve_calibration_context(
                &store,
                setup_identity
                    .as_ref()
                    .map(|(setup_id, setup_sha256)| (setup_id.as_str(), setup_sha256.as_str())),
                request,
            )
            .await
            .map_err(|error| mmwave_api_error(StatusCode::BAD_REQUEST, error))?,
        )
    } else {
        None
    };
    let (control, data_dir) = {
        let state = state.read().await;
        state
            .mmwave
            .validate_session_start(kind)
            .map_err(|error| mmwave_api_error(StatusCode::CONFLICT, error))?;
        if kind == mmwave_calibration::SessionKind::Calibration {
            state
                .mmwave
                .validate_calibration_phase_two_start()
                .map_err(|error| mmwave_api_error(StatusCode::CONFLICT, error))?;
        }
        if kind == mmwave_calibration::SessionKind::Calibration
            && state.d5_presence.phase() == d5_presence::CalibrationPhase::Collecting
        {
            return Err(mmwave_api_error(
                StatusCode::CONFLICT,
                "classification calibration is already collecting",
            ));
        }
        let control = state.mmwave.control().ok_or_else(|| {
            mmwave_api_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "automatic sessions require configured mmWave node control",
            )
        })?;
        (control, state.data_dir.clone())
    };
    let expected_mode = expected_mmwave_mode(kind);
    let diagnostics_control = control.clone();
    let diagnostics_before = tokio::task::spawn_blocking(move || {
        mmwave_calibration::get_node_diagnostics(&diagnostics_control)
    })
    .await
    .map_err(|error| mmwave_api_error(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?
    .map_err(|error| mmwave_api_error(StatusCode::BAD_GATEWAY, error))?;
    if diagnostics_before.uart_bytes_received == 0
        || diagnostics_before.radar_frames_valid == 0
        || diagnostics_before.udp_packets_sent == 0
    {
        return Err(mmwave_api_error(
            StatusCode::CONFLICT,
            "mmWave node diagnostics show no radar streaming yet",
        ));
    }

    {
        let mut state = state.write().await;
        state
            .mmwave
            .prepare_session_start(kind)
            .map_err(|error| mmwave_api_error(StatusCode::CONFLICT, error))?;
    }

    let mode_control = control.clone();
    let mode_join = tokio::task::spawn_blocking(move || {
        mmwave_calibration::set_node_mode(&mode_control, expected_mode)
    })
    .await;
    let mode_result = match mode_join {
        Ok(result) => result.map_err(|error| mmwave_api_error(StatusCode::BAD_GATEWAY, error)),
        Err(error) => {
            state.write().await.mmwave.cancel_prepared_session_start();
            return Err(mmwave_api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                error.to_string(),
            ));
        }
    };
    if let Err(error) = mode_result {
        state.write().await.mmwave.cancel_prepared_session_start();
        return Err(error);
    }

    if let Err(error) = wait_for_mmwave_mode_and_preflight(&state, kind, expected_mode).await {
        state.write().await.mmwave.cancel_prepared_session_start();
        return Err(mmwave_api_error(StatusCode::CONFLICT, error));
    }

    if let Err(error) = wait_for_mmwave_node_stream_advance(&control, &diagnostics_before).await {
        state.write().await.mmwave.cancel_prepared_session_start();
        return Err(mmwave_api_error(StatusCode::CONFLICT, error));
    }

    let now = server_clock::now();
    let mut state = state.write().await;
    if let Some(context) = calibration_context.as_ref() {
        if state
            .position_setup
            .as_ref()
            .is_none_or(|setup| setup.setup_sha256() != context.setup_sha256)
        {
            state.mmwave.cancel_prepared_session_start();
            return Err(mmwave_api_error(
                StatusCode::CONFLICT,
                "the sealed setup changed while calibration was being prepared",
            ));
        }
    }
    if kind == mmwave_calibration::SessionKind::Calibration {
        state
            .mmwave
            .validate_calibration_phase_two_start()
            .map_err(|error| mmwave_api_error(StatusCode::CONFLICT, error))?;
    }
    if let Err(error) = state
        .mmwave
        .start_session(kind, &data_dir, now.clone(), policy)
    {
        state.mmwave.cancel_prepared_session_start();
        return Err(mmwave_api_error(StatusCode::CONFLICT, error));
    }
    if kind == mmwave_calibration::SessionKind::Calibration {
        if let Err(error) = state
            .d5_presence
            .start_calibration(std::time::Instant::now())
        {
            let _ = state.mmwave.stop_session();
            return Err(mmwave_api_error(StatusCode::CONFLICT, error));
        }
        for node in state.node_states.values_mut() {
            node.d5_presence.reset_for_calibration();
            node.d6_fingerprint.reset_for_calibration();
            node.calibration_motion_rejected_frames = 0;
        }
        state.active_calibration_bundle = None;
        state.active_calibration_source = None;
        state.calibration_context = calibration_context;
    }
    let status = state.mmwave.status(now.host_monotonic_ns);
    Ok(Json(
        serde_json::to_value(status).expect("mmWave status is serializable"),
    ))
}

async fn mmwave_fixed_points_start_endpoint(State(state): State<SharedState>) -> MmwaveApiResult {
    let (control, current_mode) = {
        let state = state.read().await;
        state
            .mmwave
            .validate_fixed_point_start(server_clock::now().host_monotonic_ns)
            .map_err(|error| mmwave_api_error(StatusCode::CONFLICT, error))?;
        let status = state.mmwave.status(server_clock::now().host_monotonic_ns);
        (state.mmwave.control(), status.mode)
    };
    let expected_mode = mmwave_calibration::MeasurementMode::Calibration;
    if mode_change_required(current_mode, expected_mode) {
        let mode_control = control.ok_or_else(|| {
            mmwave_api_error(
                StatusCode::CONFLICT,
                "mmWave is not in calibration mode and node control is unavailable",
            )
        })?;
        tokio::task::spawn_blocking(move || {
            mmwave_calibration::set_node_mode(&mode_control, expected_mode)
        })
        .await
        .map_err(|error| mmwave_api_error(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?
        .map_err(|error| mmwave_api_error(StatusCode::BAD_GATEWAY, error))?;
        wait_for_mmwave_mode_and_radar_preflight(&state, expected_mode)
            .await
            .map_err(|error| mmwave_api_error(StatusCode::CONFLICT, error))?;
    }
    let mut state = state.write().await;
    let fixed_points = state
        .mmwave
        .start_fixed_point_calibration(server_clock::now().host_monotonic_ns)
        .map_err(|error| mmwave_api_error(StatusCode::CONFLICT, error))?;
    Ok(Json(
        serde_json::json!({ "fixed_point_calibration": fixed_points }),
    ))
}

async fn mmwave_fixed_points_check_endpoint(
    State(state): State<SharedState>,
    Json(request): Json<FixedPointCheckRequest>,
) -> MmwaveApiResult {
    if !(50..=2_000).contains(&request.tolerance_mm) {
        return Err(mmwave_api_error(
            StatusCode::BAD_REQUEST,
            "tolerance_mm must be between 50 and 2000",
        ));
    }
    if !(250..=10_000).contains(&request.window_ms) {
        return Err(mmwave_api_error(
            StatusCode::BAD_REQUEST,
            "window_ms must be between 250 and 10000",
        ));
    }
    let fixed_points = {
        let mut state = state.write().await;
        let fixed_points = state
            .mmwave
            .check_current_fixed_point(
                request.tolerance_mm,
                request.window_ms,
                server_clock::now().host_monotonic_ns,
            )
            .map_err(|error| mmwave_api_error(StatusCode::CONFLICT, error))?;
        if fixed_points.state == "complete" {
            let data_dir = state.data_dir.clone();
            state
                .mmwave
                .persist_yaw_calibration(&data_dir)
                .map_err(|error| mmwave_api_error(StatusCode::INTERNAL_SERVER_ERROR, error))?;
        }
        state
            .mmwave
            .status(server_clock::now().host_monotonic_ns)
            .fixed_point_calibration
            .ok_or_else(|| {
                mmwave_api_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "fixed-point calibration disappeared",
                )
            })?
    };
    Ok(Json(
        serde_json::json!({ "fixed_point_calibration": fixed_points }),
    ))
}

async fn mmwave_fixed_points_cancel_endpoint(State(state): State<SharedState>) -> MmwaveApiResult {
    let mut state = state.write().await;
    state.mmwave.cancel_fixed_point_calibration();
    let status = state.mmwave.status(server_clock::now().host_monotonic_ns);
    Ok(Json(
        serde_json::to_value(status).expect("mmWave status is serializable"),
    ))
}

async fn mmwave_known_point_check_endpoint(
    State(state): State<SharedState>,
    Json(request): Json<KnownPointCheckRequest>,
) -> MmwaveApiResult {
    if !request
        .expected_position_m
        .iter()
        .all(|value| value.is_finite() && value.abs() <= 100.0)
    {
        return Err(mmwave_api_error(
            StatusCode::BAD_REQUEST,
            "expected_position_m must contain finite room coordinates in metres",
        ));
    }
    if !(50..=2_000).contains(&request.tolerance_mm) {
        return Err(mmwave_api_error(
            StatusCode::BAD_REQUEST,
            "tolerance_mm must be between 50 and 2000",
        ));
    }
    if !(250..=5_000).contains(&request.window_ms) {
        return Err(mmwave_api_error(
            StatusCode::BAD_REQUEST,
            "window_ms must be between 250 and 5000",
        ));
    }
    let expected_position_mm = [
        (request.expected_position_m[0] * 1_000.0).round() as i32,
        (request.expected_position_m[1] * 1_000.0).round() as i32,
    ];
    let deadline = std::time::Instant::now() + Duration::from_millis(request.window_ms.min(5_000));
    loop {
        let now_ns = server_clock::now().host_monotonic_ns;
        let result = {
            let state = state.read().await;
            state.mmwave.check_known_point(
                expected_position_mm,
                request.tolerance_mm,
                request.window_ms,
                now_ns,
            )
        };
        match result {
            Ok(check) => {
                return Ok(Json(
                    serde_json::to_value(check).expect("known point check is serializable"),
                ));
            }
            Err(error) if std::time::Instant::now() < deadline => {
                let _ = error;
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            Err(error) => return Err(mmwave_api_error(StatusCode::CONFLICT, error)),
        }
    }
}

async fn mmwave_session_stop_endpoint(State(state): State<SharedState>) -> MmwaveApiResult {
    let mut state = state.write().await;
    state
        .mmwave
        .stop_session()
        .map_err(|error| mmwave_api_error(StatusCode::CONFLICT, error))?;
    let status = state.mmwave.status(server_clock::now().host_monotonic_ns);
    Ok(Json(
        serde_json::to_value(status).expect("mmWave status is serializable"),
    ))
}

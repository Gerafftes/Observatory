//! Recording HTTP routes and raw-capture contract helpers.

use axum::routing::delete;

use super::runtime_tasks::finalize_active_recording;
use super::*;

pub(crate) fn routes() -> Router<SharedState> {
    Router::new()
        .route("/api/v1/recording/list", get(list_recordings))
        .route("/api/v1/recording/start", post(start_recording))
        .route("/api/v1/recording/stop", post(stop_recording))
        .route("/api/v1/recording/:id", delete(delete_recording))
}

async fn list_recordings(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let current = {
        let s = state.read().await;
        s.recording_current_id
            .clone()
            .map(|id| (id, s.recording_phase))
    };
    let mut recordings = scan_recording_files();
    if let Some((current_id, phase)) = current {
        let live_status = match phase {
            RecordingLifecyclePhase::Recording => "recording",
            RecordingLifecyclePhase::Finalizing => "finalizing",
            RecordingLifecyclePhase::Idle => "incomplete",
        };
        for recording in &mut recordings {
            if recording.get("id").and_then(serde_json::Value::as_str) == Some(current_id.as_str())
            {
                recording["status"] = serde_json::json!(live_status);
                recording["incomplete"] = serde_json::json!(phase == RecordingLifecyclePhase::Idle);
            }
        }
    }
    Json(serde_json::json!({ "recordings": recordings }))
}

/// POST /api/v1/recording/start — start recording CSI data.
async fn start_recording(
    State(state): State<SharedState>,
    Json(body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let lifecycle = {
        let s = state.read().await;
        s.recording_lifecycle.clone()
    };
    let _lifecycle_guard = lifecycle.lock().await;
    let mut s = state.write().await;
    if s.recording_phase != RecordingLifecyclePhase::Idle
        || s.recording_active
        || s.recording_current_id.is_some()
    {
        return Json(serde_json::json!({
            "error": "recording already in progress or finalizing",
            "success": false,
            "recording_id": s.recording_current_id,
        }));
    }
    let id = body
        .get("id")
        .and_then(|v| v.as_str())
        .or_else(|| body.get("session_name").and_then(|v| v.as_str()))
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("rec_{}", chrono_timestamp()));
    if let Err(error) = raw_csi_recording::validate_recording_id(&id) {
        return Json(serde_json::json!({
            "success": false,
            "error": error.to_string(),
        }));
    }
    let max_duration_seconds = match body.get("max_duration_seconds") {
        None => None,
        Some(value) => match value.as_u64() {
            Some(seconds @ 1..=3_600) => Some(seconds),
            _ => {
                return Json(serde_json::json!({
                    "success": false,
                    "error": "max_duration_seconds must be an integer from 1 to 3600",
                }));
            }
        },
    };
    let label = body
        .get("label")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    let ground_truth = match body.get("ground_truth").cloned() {
        Some(value) => match serde_json::from_value::<raw_csi_recording::GroundTruth>(value) {
            Ok(ground_truth) => Some(ground_truth),
            Err(error) => {
                return Json(serde_json::json!({
                    "success": false,
                    "error": format!("invalid ground_truth: {error}"),
                }));
            }
        },
        None => None,
    };
    let loaded_setup_identity = s
        .position_setup
        .as_deref()
        .map(|setup| (setup.setup_id(), setup.setup_sha256()));
    let (setup_id, setup_sha256) =
        match resolve_recording_setup_identity(&body, loaded_setup_identity) {
            Ok(identity) => identity,
            Err(error) => {
                return Json(serde_json::json!({
                    "success": false,
                    "error": error,
                }));
            }
        };

    // Create the lossless server-local recording and an immutable setup
    // sidecar. Raw I/Q is never routed through the privacy-gated UI payload.
    let recordings_dir = PathBuf::from("data/recordings");
    if let Err(error) = std::fs::create_dir_all(&recordings_dir) {
        return error_response::internal_error_json("recording directory create", error);
    }
    let rec_path = match raw_csi_recording::recording_path(&recordings_dir, &id) {
        Ok(path) => path,
        Err(error) => {
            return Json(serde_json::json!({
                "success": false,
                "error": error.to_string(),
            }));
        }
    };
    let file = match create_private_recording_file(&rec_path) {
        Ok(f) => f,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            return Json(serde_json::json!({
                "success": false,
                "error": "recording ID already exists; choose a new ID",
                "recording_id": id,
            }));
        }
        Err(e) => {
            // ADR-080 #2: the OS error can carry the recordings path; log it
            // server-side only and return a generic body + correlation id.
            return error_response::internal_error_json("recording create", e);
        }
    };
    let metadata_path = recordings_dir.join(format!("{id}.raw-csi.v1.meta.json"));
    let started_at_unix_ns = match raw_csi_recording::now_unix_ns() {
        Ok(timestamp) => timestamp,
        Err(error) => {
            drop(file);
            let _ = std::fs::remove_file(&rec_path);
            return error_response::internal_error_json("recording start timestamp", error);
        }
    };
    let recording_rx_positions: Vec<[f64; 3]> = s
        .position_setup
        .as_deref()
        .map(|setup| setup.receiver_positions_m().into_iter().collect())
        .unwrap_or_else(|| {
            s.multistatic_fuser
                .node_positions()
                .iter()
                .map(|position| position.map(f64::from))
                .collect()
        });
    let metadata = serde_json::json!({
        "schema_version": raw_csi_recording::RAW_CSI_SCHEMA_VERSION,
        "recording_id": id,
        "label": label.clone(),
        "ground_truth": ground_truth.clone(),
        "setup_id": setup_id,
        "setup_sha256": setup_sha256,
        "server_version": env!("CARGO_PKG_VERSION"),
        "started_at_unix_seconds": chrono_timestamp(),
        "started_at_unix_ns": started_at_unix_ns,
        "tx_position": s.tx_position,
        "rx_positions": recording_rx_positions,
        "room_dimensions": s.room_dimensions,
        "capture_scope": "validated_udp_csi_all_grids",
        "max_duration_seconds": max_duration_seconds,
        "status": "recording",
    });
    let metadata_bytes = match serde_json::to_vec_pretty(&metadata) {
        Ok(bytes) => bytes,
        Err(error) => {
            drop(file);
            let _ = std::fs::remove_file(&rec_path);
            return error_response::internal_error_json("recording metadata encode", error);
        }
    };
    let mut metadata_file = match create_private_recording_file(&metadata_path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            drop(file);
            let _ = std::fs::remove_file(&rec_path);
            return Json(serde_json::json!({
                "success": false,
                "error": "recording metadata already exists; choose a new ID",
                "recording_id": id,
            }));
        }
        Err(error) => {
            drop(file);
            let _ = std::fs::remove_file(&rec_path);
            return error_response::internal_error_json("recording metadata create", error);
        }
    };
    {
        use std::io::Write;
        if let Err(error) = metadata_file
            .write_all(&metadata_bytes)
            .and_then(|_| metadata_file.flush())
            .and_then(|_| metadata_file.sync_all())
        {
            drop(metadata_file);
            drop(file);
            let _ = std::fs::remove_file(&metadata_path);
            let _ = std::fs::remove_file(&rec_path);
            return error_response::internal_error_json("recording metadata write", error);
        }
    }
    drop(metadata_file);

    // Create a stop signal channel
    let (stop_tx, mut stop_rx) = tokio::sync::watch::channel(false);
    let (done_tx, done_rx) = tokio::sync::oneshot::channel();
    s.recording_phase = RecordingLifecyclePhase::Recording;
    s.recording_active = true;
    s.recording_start_time = Some(std::time::Instant::now());
    s.recording_current_id = Some(id.clone());
    s.recording_stop_tx = Some(stop_tx);
    s.recording_done_rx = Some(done_rx);

    // Subscribe to the private lossless CSI channel, not the derived public
    // sensing-update channel.
    let mut rx = s.raw_csi_tx.subscribe();
    let recording_position_setup = s.position_setup.clone();

    // Add initial recording entry
    s.recordings.push(serde_json::json!({
        "id": id,
        "path": rec_path.display().to_string(),
        "metadata_path": metadata_path.display().to_string(),
        "format": "raw-csi-v1-jsonl",
        "label": label.clone(),
        "status": "recording",
        "started_at": chrono_timestamp(),
        "frames": 0,
        "frame_count": 0,
        "setup_id": setup_id,
        "setup_sha256": setup_sha256,
        "rx_summaries": [],
    }));

    let rec_id = id.clone();
    let session_id = Some(id.clone());
    let writer_state = state.clone();
    let watchdog_state = state.clone();
    let watchdog_recording_id = id.clone();

    // Spawn writer task in background
    tokio::spawn(async move {
        use std::io::Write;
        let mut writer = std::io::BufWriter::new(file);
        let mut result = RecordingWriterResult::default();
        let mut stop_requested = false;

        while !stop_requested && result.error.is_none() {
            tokio::select! {
                received = rx.recv() => {
                    match received {
                        Ok(RawCsiIngress::Frame(raw_frame)) => {
                            if let Err(error) = append_raw_recording_frame(
                                &mut writer,
                                raw_frame,
                                recording_position_setup.as_deref(),
                                &session_id,
                                &label,
                                &ground_truth,
                                &mut result,
                            ) {
                                warn!("Recording {rec_id}: {error}");
                                result.error = Some(error);
                            }
                        }
                        Ok(RawCsiIngress::Rejected { rx_id, reason }) => {
                            mark_recording_rejected(&mut result, rx_id, &reason);
                        }
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            result.dropped_frames =
                                result.dropped_frames.saturating_add(n);
                            warn!(
                                "Recording {rec_id}: lagged {n} frames; capture will be marked incomplete"
                            );
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            result.error =
                                Some("raw CSI broadcast closed before stop".to_string());
                        }
                    }
                }
                changed = stop_rx.changed() => {
                    match changed {
                        Ok(()) if *stop_rx.borrow() => {
                            stop_requested = true;
                            info!(
                                "Recording {rec_id}: stop signal received ({} frames)",
                                result.frames_written
                            );
                        }
                        Ok(()) => {}
                        Err(_) => {
                            result.error =
                                Some("recording stop channel closed unexpectedly".to_string());
                        }
                    }
                }
            }
        }

        // The producer gate was closed under AppState's write lock before the
        // stop signal was sent. The queue is therefore finite and can be
        // drained to establish a durable, exact stop boundary.
        if stop_requested && result.error.is_none() {
            loop {
                match rx.try_recv() {
                    Ok(RawCsiIngress::Frame(raw_frame)) => {
                        if let Err(error) = append_raw_recording_frame(
                            &mut writer,
                            raw_frame,
                            recording_position_setup.as_deref(),
                            &session_id,
                            &label,
                            &ground_truth,
                            &mut result,
                        ) {
                            warn!("Recording {rec_id}: {error}");
                            result.error = Some(error);
                            break;
                        }
                    }
                    Ok(RawCsiIngress::Rejected { rx_id, reason }) => {
                        mark_recording_rejected(&mut result, rx_id, &reason);
                        break;
                    }
                    Err(broadcast::error::TryRecvError::Lagged(n)) => {
                        result.dropped_frames = result.dropped_frames.saturating_add(n);
                        warn!(
                            "Recording {rec_id}: lagged {n} queued frames during drain; capture is incomplete"
                        );
                    }
                    Err(broadcast::error::TryRecvError::Empty)
                    | Err(broadcast::error::TryRecvError::Closed) => break,
                }
            }
        }

        match writer.flush() {
            Ok(()) => {
                if let Err(error) = writer.get_ref().sync_all() {
                    let sync_error = format!("raw recording sync error: {error}");
                    warn!("Recording {rec_id}: {sync_error}");
                    append_recording_writer_error(&mut result, sync_error);
                }
            }
            Err(error) => {
                let flush_error = format!("final flush error: {error}");
                warn!("Recording {rec_id}: {flush_error}");
                append_recording_writer_error(&mut result, flush_error);
            }
        }
        info!(
            "Recording {rec_id} finished: {} frames written, {} dropped, incomplete={}",
            result.frames_written,
            result.dropped_frames,
            result.incomplete()
        );
        let writer_failed = result.error.is_some();
        let _ = done_tx.send(result);
        if writer_failed {
            match finalize_active_recording(&writer_state).await {
                Ok((recording_id, _, result)) => {
                    warn!(
                        "Recording {recording_id} was stopped automatically after a writer error: {:?}",
                        result.error
                    );
                }
                Err(error) => {
                    // A concurrent explicit stop may have completed first.
                    debug!("Recording {rec_id} auto-finalization skipped: {error}");
                }
            }
        }
    });

    if let Some(max_duration_seconds) = max_duration_seconds {
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(max_duration_seconds)).await;
            let should_finalize = {
                let state = watchdog_state.read().await;
                state.recording_phase == RecordingLifecyclePhase::Recording
                    && state.recording_current_id.as_deref() == Some(watchdog_recording_id.as_str())
            };
            if should_finalize {
                match finalize_active_recording(&watchdog_state).await {
                    Ok((recording_id, _, result)) => {
                        warn!(
                            "Recording {recording_id} reached its server-side maximum duration; \
                             finalized with incomplete={}",
                            result.incomplete()
                        );
                    }
                    Err(error) => {
                        debug!(
                            "Recording {watchdog_recording_id} watchdog finalization skipped: {error}"
                        );
                    }
                }
            }
        });
    }

    info!("Recording started: {id}");
    Json(serde_json::json!({
        "success": true,
        "recording_id": id,
        "max_duration_seconds": max_duration_seconds,
    }))
}

/// POST /api/v1/recording/stop — stop recording CSI data.
async fn stop_recording(State(state): State<SharedState>) -> Json<serde_json::Value> {
    match finalize_active_recording(&state).await {
        Ok((recording_id, duration_secs, result)) => {
            let incomplete = result.incomplete();
            if incomplete {
                warn!(
                    "Recording {recording_id} stopped incomplete: dropped={}, error={:?}",
                    result.dropped_frames, result.error
                );
            } else {
                info!("Recording stopped: {recording_id} ({duration_secs}s)");
            }
            Json(serde_json::json!({
                "success": !incomplete,
                "stopped": true,
                "recording_id": recording_id,
                "duration_secs": duration_secs,
                "frames_written": result.frames_written,
                "rx_summaries": result.rx_summaries.values().collect::<Vec<_>>(),
                "dropped_frames": result.dropped_frames,
                "incomplete": incomplete,
                "writer_error": result.error,
            }))
        }
        Err(error) => Json(serde_json::json!({
            "error": error,
            "success": false,
        })),
    }
}

/// DELETE /api/v1/recording/:id — delete a recording file.
async fn delete_recording(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    if let Err(error) = raw_csi_recording::validate_recording_id(&id) {
        return Json(serde_json::json!({
            "error": error.to_string(),
            "success": false
        }));
    }
    let lifecycle = {
        let s = state.read().await;
        s.recording_lifecycle.clone()
    };
    let _lifecycle_guard = lifecycle.lock().await;
    {
        let s = state.read().await;
        if s.recording_phase != RecordingLifecyclePhase::Idle
            && s.recording_current_id.as_deref() == Some(id.as_str())
        {
            return Json(serde_json::json!({
                "error": "active or finalizing recording cannot be deleted",
                "success": false,
                "recording_id": id,
            }));
        }
    }

    let recordings_dir = PathBuf::from("data/recordings");
    let raw_path = raw_csi_recording::recording_path(&recordings_dir, &id)
        .expect("recording ID was validated above");
    let data_paths = [
        raw_path,
        recordings_dir.join(format!("{id}.csi.jsonl")),
        recordings_dir.join(format!("{id}.jsonl")),
    ];
    let metadata_paths = [
        recordings_dir.join(format!("{id}.raw-csi.v1.meta.json")),
        recordings_dir.join(format!("{id}.csi.meta.json")),
        recordings_dir.join(format!("{id}.meta.json")),
    ];

    let mut removed_any = false;
    for path in data_paths.iter().chain(metadata_paths.iter()) {
        if !path.exists() {
            continue;
        }
        if let Err(error) = std::fs::remove_file(path) {
            // ADR-080 #2: log the OS error (incl. path) server-side only.
            return error_response::internal_error_json("recording delete", error);
        }
        removed_any = true;
    }

    if removed_any {
        let mut s = state.write().await;
        s.recordings
            .retain(|r| r.get("id").and_then(|v| v.as_str()) != Some(id.as_str()));
        info!("Recording deleted: {id}");
        Json(serde_json::json!({ "success": true, "deleted": id }))
    } else {
        Json(serde_json::json!({ "error": "recording not found", "success": false }))
    }
}

/// Scan `data/recordings/` for lossless raw recordings and legacy derived
/// `.jsonl` files.
#[derive(Debug, PartialEq, Eq)]
struct ScannedRecordingIntegrity {
    status: String,
    incomplete: bool,
    dropped_frames: u64,
    metadata_valid: bool,
}

fn scanned_recording_integrity(
    is_raw: bool,
    recording_id: &str,
    metadata: Option<&serde_json::Value>,
) -> ScannedRecordingIntegrity {
    let raw_metadata_valid = !is_raw
        || metadata.is_some_and(|metadata| {
            metadata
                .get("schema_version")
                .and_then(serde_json::Value::as_u64)
                == Some(u64::from(raw_csi_recording::RAW_CSI_SCHEMA_VERSION))
                && metadata
                    .get("recording_id")
                    .and_then(serde_json::Value::as_str)
                    == Some(recording_id)
                && matches!(
                    metadata.get("status").and_then(serde_json::Value::as_str),
                    Some("recording" | "completed" | "incomplete")
                )
        });
    let stored_status = metadata
        .and_then(|metadata| metadata.get("status"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or(if is_raw { "incomplete" } else { "completed" });
    let dropped_frames = metadata
        .and_then(|metadata| metadata.get("dropped_frames"))
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let writer_error = metadata
        .and_then(|metadata| metadata.get("writer_error"))
        .is_some_and(|error| !error.is_null());
    let incomplete = !raw_metadata_valid
        || matches!(stored_status, "recording" | "incomplete")
        || metadata
            .and_then(|metadata| metadata.get("incomplete"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
        || dropped_frames > 0
        || writer_error;

    ScannedRecordingIntegrity {
        status: if incomplete {
            "incomplete".to_string()
        } else {
            stored_status.to_string()
        },
        incomplete,
        dropped_frames,
        metadata_valid: raw_metadata_valid,
    }
}

pub(crate) fn scan_recording_files() -> Vec<serde_json::Value> {
    let dir = PathBuf::from("data/recordings");
    let mut recordings = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            let (name, format, trainable, is_raw, metadata_path) = if let Some(name) =
                file_name.strip_suffix(raw_csi_recording::RAW_CSI_FILE_SUFFIX)
            {
                (
                    name.to_string(),
                    "raw-csi-v1-jsonl",
                    false,
                    true,
                    dir.join(format!("{name}.raw-csi.v1.meta.json")),
                )
            } else if let Some(name) = file_name.strip_suffix(".csi.jsonl") {
                (
                    name.to_string(),
                    "legacy-derived-jsonl",
                    true,
                    false,
                    dir.join(format!("{name}.csi.meta.json")),
                )
            } else if let Some(name) = file_name.strip_suffix(".jsonl") {
                (
                    name.to_string(),
                    "legacy-derived-jsonl",
                    true,
                    false,
                    dir.join(format!("{name}.meta.json")),
                )
            } else {
                continue;
            };
            {
                let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                let modified = entry
                    .metadata()
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                // Stream the count so a large raw capture is not read into RAM.
                let frame_count = std::fs::File::open(&path)
                    .map(|file| {
                        use std::io::BufRead;
                        std::io::BufReader::new(file).lines().count()
                    })
                    .unwrap_or(0);
                let metadata = std::fs::read(&metadata_path)
                    .ok()
                    .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok());
                // Only the live-state overlay in list_recordings can confirm
                // that a writer really is active. Raw data without a valid
                // matching sidecar must fail closed after a crash or write
                // error; it is never silently promoted to "completed".
                let integrity = scanned_recording_integrity(is_raw, &name, metadata.as_ref());
                let empty_metadata = serde_json::json!({});
                let metadata = metadata.as_ref().unwrap_or(&empty_metadata);
                recordings.push(serde_json::json!({
                    "id": name,
                    "name": name,
                    "path": path.display().to_string(),
                    "format": format,
                    "trainable": trainable,
                    "size_bytes": size,
                    "file_size_bytes": size,
                    "frames": frame_count,
                    "frame_count": frame_count,
                    "frames_written": metadata.get("frames_written"),
                    "modified_epoch": modified,
                    "status": integrity.status,
                    "label": metadata.get("label"),
                    "started_at": metadata
                        .get("started_at")
                        .or_else(|| metadata.get("started_at_unix_seconds")),
                    "ended_at": metadata
                        .get("ended_at")
                        .or_else(|| metadata.get("ended_at_unix_seconds")),
                    "dropped_frames": integrity.dropped_frames,
                    "incomplete": integrity.incomplete,
                    "integrity_error": if integrity.metadata_valid {
                        serde_json::Value::Null
                    } else {
                        serde_json::json!("missing_or_invalid_raw_metadata")
                    },
                    "capture_scope": metadata.get("capture_scope"),
                    "setup_id": metadata.get("setup_id"),
                    "setup_sha256": metadata.get("setup_sha256"),
                    "duration_secs": metadata.get("duration_secs"),
                    "rx_summaries": metadata.get("rx_summaries"),
                }));
            }
        }
    }
    recordings
}

#[cfg(test)]
mod raw_recording_lifecycle_tests {
    use super::*;
    use raw_csi_recording::{IqPair, RawCsiFrame, RAW_CSI_SCHEMA_VERSION};

    fn valid_metadata(recording_id: &str, status: &str) -> serde_json::Value {
        serde_json::json!({
            "schema_version": raw_csi_recording::RAW_CSI_SCHEMA_VERSION,
            "recording_id": recording_id,
            "status": status,
            "incomplete": false,
            "dropped_frames": 0,
            "writer_error": null,
        })
    }

    fn live_raw_frame(timestamp: u64, sequence: u32) -> RawCsiFrame {
        RawCsiFrame {
            schema_version: RAW_CSI_SCHEMA_VERSION,
            host_timestamp_unix_ns: timestamp,
            host_monotonic_ns: Some(timestamp),
            clock_epoch_id: Some("test-clock".to_string()),
            session_id: None,
            label: None,
            ground_truth: None,
            rx_id: 1,
            antenna_count: 1,
            subcarrier_count: 64,
            center_frequency_mhz: 2437,
            sequence,
            rssi_dbm: -50,
            noise_floor_dbm: -90,
            ppdu_type: 0,
            flags: 0,
            mesh_timestamp_us: None,
            source_binding: None,
            iq_pairs: vec![IqPair { i: 20, q: 0 }; 64],
        }
    }

    #[test]
    fn raw_recording_without_valid_matching_sidecar_fails_closed() {
        let missing = scanned_recording_integrity(true, "capture-a", None);
        assert_eq!(missing.status, "incomplete");
        assert!(missing.incomplete);
        assert!(!missing.metadata_valid);

        let mismatched = valid_metadata("capture-b", "completed");
        let mismatched = scanned_recording_integrity(true, "capture-a", Some(&mismatched));
        assert_eq!(mismatched.status, "incomplete");
        assert!(mismatched.incomplete);
        assert!(!mismatched.metadata_valid);

        let malformed = serde_json::json!({"recording_id": "capture-a"});
        let malformed = scanned_recording_integrity(true, "capture-a", Some(&malformed));
        assert_eq!(malformed.status, "incomplete");
        assert!(malformed.incomplete);
        assert!(!malformed.metadata_valid);
    }

    #[test]
    fn raw_recording_only_reports_completed_with_clean_terminal_metadata() {
        let completed = valid_metadata("capture-a", "completed");
        let completed = scanned_recording_integrity(true, "capture-a", Some(&completed));
        assert_eq!(completed.status, "completed");
        assert!(!completed.incomplete);
        assert!(completed.metadata_valid);

        let mut dropped = valid_metadata("capture-a", "completed");
        dropped["dropped_frames"] = serde_json::json!(3);
        let dropped = scanned_recording_integrity(true, "capture-a", Some(&dropped));
        assert_eq!(dropped.status, "incomplete");
        assert!(dropped.incomplete);

        let stale = valid_metadata("capture-a", "recording");
        let stale = scanned_recording_integrity(true, "capture-a", Some(&stale));
        assert_eq!(stale.status, "incomplete");
        assert!(stale.incomplete);
    }

    #[test]
    fn legacy_recording_without_sidecar_keeps_legacy_completed_default() {
        let legacy = scanned_recording_integrity(false, "legacy", None);
        assert_eq!(legacy.status, "completed");
        assert!(!legacy.incomplete);
        assert!(legacy.metadata_valid);
    }

    #[test]
    fn loaded_setup_is_auto_bound_or_must_be_repeated_exactly() {
        let expected_id = "setup-0123456789abcdef";
        let expected_sha256 = "a".repeat(64);
        let loaded = Some((expected_id, expected_sha256.as_str()));

        assert_eq!(
            resolve_recording_setup_identity(&serde_json::json!({}), loaded).unwrap(),
            (Some(expected_id.to_string()), Some(expected_sha256.clone()))
        );
        assert_eq!(
            resolve_recording_setup_identity(
                &serde_json::json!({
                    "setup_id": expected_id,
                    "setup_sha256": expected_sha256,
                }),
                loaded,
            )
            .unwrap(),
            (Some(expected_id.to_string()), Some("a".repeat(64)))
        );

        assert!(resolve_recording_setup_identity(
            &serde_json::json!({
                "setup_id": expected_id,
                "setup_sha256": "b".repeat(64),
            }),
            loaded,
        )
        .is_err());
        assert!(resolve_recording_setup_identity(
            &serde_json::json!({"setup_id": expected_id}),
            loaded,
        )
        .is_err());
    }

    #[test]
    fn recording_cannot_claim_a_setup_when_server_loaded_none() {
        assert_eq!(
            resolve_recording_setup_identity(&serde_json::json!({}), None).unwrap(),
            (None, None)
        );
        assert!(resolve_recording_setup_identity(
            &serde_json::json!({
                "setup_id": "setup-0123456789abcdef",
                "setup_sha256": "a".repeat(64),
            }),
            None,
        )
        .is_err());
        assert!(
            resolve_recording_setup_identity(&serde_json::json!({"setup_id": null}), None,)
                .is_err()
        );
    }

    #[test]
    fn rejected_setup_bound_ingress_marks_active_recording_incomplete() {
        let mut result = RecordingWriterResult::default();

        mark_recording_rejected(&mut result, Some(3), "sealed position setup rejected frame");

        assert!(result.incomplete());
        assert_eq!(result.frames_written, 0);
        assert_eq!(result.dropped_frames, 0);
        assert_eq!(
            result.error.as_deref(),
            Some(
                "RX3 sent raw CSI rejected before recording: sealed position setup rejected frame"
            )
        );
    }

    #[test]
    fn live_raw_input_is_independent_of_recorder_and_grid_failure_starts_a_new_epoch() {
        let mut tracker = position_live::LivePositionTracker::new(None);
        route_raw_frame_to_live_position(
            &mut tracker,
            None,
            true,
            live_raw_frame(10_000_000_000, 1),
        )
        .unwrap();
        assert_eq!(
            tracker.buffered_frame_count(),
            1,
            "live tracking must ingest raw CSI with no active recorder"
        );

        assert!(route_raw_frame_to_live_position(
            &mut tracker,
            None,
            false,
            live_raw_frame(10_100_000_000, 2),
        )
        .is_err());
        assert_eq!(
            tracker.buffered_frame_count(),
            0,
            "an invalid grid must not leave pre-transition frames reusable"
        );
        assert_eq!(
            tracker.current(),
            &position_live::LivePositionState::Uncalibrated
        );
    }

    #[test]
    fn recording_tracks_each_rx_and_rejects_a_mid_capture_grid_change() {
        let unique = raw_csi_recording::now_unix_ns().expect("test timestamp");
        let path = std::env::temp_dir().join(format!(
            "ruview-raw-rx-summary-test-{}-{unique}.jsonl",
            std::process::id()
        ));
        let file = create_private_recording_file(&path).expect("create test recording");
        let mut writer = std::io::BufWriter::new(file);
        let mut result = RecordingWriterResult::default();
        let session_id = Some("capture-a".to_string());

        let first_rx1 = live_raw_frame(10_000_000_000, 1);
        append_raw_recording_frame(
            &mut writer,
            first_rx1,
            None,
            &session_id,
            &None,
            &None,
            &mut result,
        )
        .expect("write first RX1 frame");

        let mut first_rx2 = live_raw_frame(10_100_000_000, 1);
        first_rx2.rx_id = 2;
        append_raw_recording_frame(
            &mut writer,
            first_rx2,
            None,
            &session_id,
            &None,
            &None,
            &mut result,
        )
        .expect("write first RX2 frame");

        let mut second_rx1 = live_raw_frame(10_200_000_000, 2);
        second_rx1.flags = raw_csi_recording::TRANSIENT_SYNC_FLAG;
        append_raw_recording_frame(
            &mut writer,
            second_rx1,
            None,
            &session_id,
            &None,
            &None,
            &mut result,
        )
        .expect("transient sync flag must not change the RX grid");

        assert_eq!(result.frames_written, 3);
        assert_eq!(result.rx_summaries.len(), 2);
        assert_eq!(result.rx_summaries[&1].frames_written, 2);
        assert_eq!(result.rx_summaries[&2].frames_written, 1);

        let mut changed_grid = live_raw_frame(10_300_000_000, 3);
        changed_grid.subcarrier_count = 63;
        changed_grid.iq_pairs.truncate(63);
        let error = append_raw_recording_frame(
            &mut writer,
            changed_grid,
            None,
            &session_id,
            &None,
            &None,
            &mut result,
        )
        .expect_err("a real RX grid change must fail closed");
        assert!(error.contains("changed CSI grid"));
        assert_eq!(result.frames_written, 3);
        assert_eq!(result.rx_summaries[&1].frames_written, 2);

        drop(writer);
        std::fs::remove_file(path).expect("remove test recording");
    }

    #[test]
    fn final_metadata_replace_is_private_and_complete() {
        use std::io::Write;

        let unique = raw_csi_recording::now_unix_ns().expect("test timestamp");
        let dir = std::env::temp_dir().join(format!(
            "ruview-raw-metadata-test-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir(&dir).expect("create test directory");
        let recording_id = "capture-a";
        let metadata_path = dir.join(format!("{recording_id}.raw-csi.v1.meta.json"));
        let mut metadata_file =
            create_private_recording_file(&metadata_path).expect("create initial metadata");
        let initial = valid_metadata(recording_id, "recording");
        metadata_file
            .write_all(&serde_json::to_vec_pretty(&initial).expect("serialize initial metadata"))
            .expect("write initial metadata");
        metadata_file.sync_all().expect("sync initial metadata");
        drop(metadata_file);

        let result = RecordingWriterResult {
            frames_written: 42,
            dropped_frames: 0,
            error: None,
            rx_summaries: BTreeMap::new(),
        };
        finalize_recording_metadata_in_dir(&dir, recording_id, 12, &result)
            .expect("atomically finalize metadata");

        let finalized: serde_json::Value = serde_json::from_slice(
            &std::fs::read(&metadata_path).expect("read finalized metadata"),
        )
        .expect("decode finalized metadata");
        assert_eq!(finalized["status"], "completed");
        assert_eq!(finalized["frames_written"], 42);
        assert_eq!(finalized["duration_secs"], 12);
        assert_eq!(finalized["incomplete"], false);
        assert!(finalized["ended_at_unix_ns"].as_u64().is_some());
        assert!(
            std::fs::read_dir(&dir)
                .expect("list test directory")
                .flatten()
                .all(|entry| !entry.file_name().to_string_lossy().ends_with(".tmp")),
            "atomic replace must not leave a temporary sidecar"
        );

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&metadata_path)
                .expect("metadata permissions")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600);
        }

        std::fs::remove_dir_all(&dir).expect("remove test directory");
    }

    #[test]
    fn zero_frame_recording_finalizes_as_incomplete() {
        use std::io::Write;

        let unique = raw_csi_recording::now_unix_ns().expect("test timestamp");
        let dir = std::env::temp_dir().join(format!(
            "ruview-zero-frame-metadata-test-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir(&dir).expect("create test directory");
        let recording_id = "zero-frame-capture";
        let metadata_path = dir.join(format!("{recording_id}.raw-csi.v1.meta.json"));
        let mut metadata_file =
            create_private_recording_file(&metadata_path).expect("create initial metadata");
        metadata_file
            .write_all(
                &serde_json::to_vec_pretty(&valid_metadata(recording_id, "recording"))
                    .expect("serialize initial metadata"),
            )
            .expect("write initial metadata");
        metadata_file.sync_all().expect("sync initial metadata");
        drop(metadata_file);

        let result = RecordingWriterResult::default();
        finalize_recording_metadata_in_dir(&dir, recording_id, 0, &result)
            .expect("finalize zero-frame metadata");

        let finalized: serde_json::Value = serde_json::from_slice(
            &std::fs::read(&metadata_path).expect("read finalized metadata"),
        )
        .expect("decode finalized metadata");
        assert_eq!(finalized["status"], "incomplete");
        assert_eq!(finalized["frames_written"], 0);
        assert_eq!(finalized["incomplete"], true);

        std::fs::remove_dir_all(&dir).expect("remove test directory");
    }
}

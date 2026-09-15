//! Background runtime tasks and ordered shutdown/finalization.

use super::*;

pub(crate) async fn windows_wifi_task(state: SharedState, tick_ms: u64) {
    let mut interval = tokio::time::interval(Duration::from_millis(tick_ms));
    let mut seq: u32 = 0;

    // ADR-022 Phase 3: Multi-BSSID pipeline state (kept across ticks)
    let mut registry = BssidRegistry::new(32, 30);
    let mut pipeline = WindowsWifiPipeline::new();

    info!(
        "Windows WiFi multi-BSSID pipeline active (tick={}ms, max_bssids=32)",
        tick_ms
    );

    loop {
        interval.tick().await;
        seq += 1;

        // ── Step 1: Run multi-BSSID scan via spawn_blocking ──────────
        // NetshBssidScanner is not Send, so we run `netsh` and parse
        // the output inside a blocking closure.
        let bssid_scan_result = tokio::task::spawn_blocking(|| {
            let output = std::process::Command::new("netsh")
                .args(["wlan", "show", "networks", "mode=bssid"])
                .output()
                .map_err(|e| format!("netsh bssid scan failed: {e}"))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(format!(
                    "netsh exited with {}: {}",
                    output.status,
                    stderr.trim()
                ));
            }

            let stdout = String::from_utf8_lossy(&output.stdout);
            parse_netsh_bssid_output(&stdout).map_err(|e| format!("parse error: {e}"))
        })
        .await;

        // Unwrap the JoinHandle result, then the inner Result.
        let observations = match bssid_scan_result {
            Ok(Ok(obs)) if !obs.is_empty() => obs,
            Ok(Ok(_empty)) => {
                debug!("Multi-BSSID scan returned 0 observations, falling back");
                windows_wifi_fallback_tick(&state, seq).await;
                continue;
            }
            Ok(Err(e)) => {
                warn!("Multi-BSSID scan error: {e}, falling back");
                windows_wifi_fallback_tick(&state, seq).await;
                continue;
            }
            Err(join_err) => {
                error!("spawn_blocking panicked: {join_err}");
                continue;
            }
        };

        let obs_count = observations.len();

        // Derive SSID from the first observation for the source label.
        let ssid = observations
            .first()
            .map(|o| o.ssid.clone())
            .unwrap_or_else(|| "Unknown".into());

        // ── Step 2: Feed observations into registry ──────────────────
        registry.update(&observations);
        let multi_ap_frame = registry.to_multi_ap_frame();

        // ── Step 3: Run enhanced pipeline ────────────────────────────
        let enhanced = pipeline.process(&multi_ap_frame);

        // ── Step 4: Build backward-compatible Esp32Frame ─────────────
        let first_rssi = observations.first().map(|o| o.rssi_dbm).unwrap_or(-80.0);
        let _first_signal_pct = observations.first().map(|o| o.signal_pct).unwrap_or(40.0);

        let frame = Esp32Frame {
            magic: 0xC511_0001,
            node_id: 0,
            n_antennas: 1,
            n_subcarriers: obs_count.min(u16::MAX as usize) as u16,
            freq_mhz: 2437,
            sequence: seq,
            rssi: first_rssi.clamp(-128.0, 127.0) as i8,
            noise_floor: -90,
            ppdu_type: wifi_densepose_hardware::PpduType::HtLegacy,
            amplitudes: multi_ap_frame.amplitudes.clone(),
            phases: multi_ap_frame.phases.clone(),
        };

        // ── Step 4b: Update frame history and extract features ───────
        let mut s_write_pre = state.write().await;
        s_write_pre
            .frame_history
            .push_back(frame.amplitudes.clone());
        if s_write_pre.frame_history.len() > FRAME_HISTORY_CAPACITY {
            s_write_pre.frame_history.pop_front();
        }
        let sample_rate_hz = 1000.0 / tick_ms as f64;
        let (features, mut classification, breathing_rate_hz, sub_variances, raw_motion) =
            extract_features_from_frame(&frame, &s_write_pre.frame_history, sample_rate_hz);
        smooth_and_classify(&mut s_write_pre, &mut classification, raw_motion);
        adaptive_override(&s_write_pre, &features, &mut classification);
        drop(s_write_pre);

        // ── Step 5: Build enhanced fields from pipeline result ───────
        let enhanced_motion = Some(serde_json::json!({
            "score": enhanced.motion.score,
            "level": format!("{:?}", enhanced.motion.level),
            "contributing_bssids": enhanced.motion.contributing_bssids,
        }));

        let enhanced_breathing = enhanced.breathing.as_ref().map(|b| {
            serde_json::json!({
                "rate_bpm": b.rate_bpm,
                "confidence": b.confidence,
                "bssid_count": b.bssid_count,
            })
        });

        let posture_str = enhanced.posture.map(|p| format!("{p:?}"));
        let sig_quality_score = Some(enhanced.signal_quality.score);
        let verdict_str = Some(format!("{:?}", enhanced.verdict));
        let bssid_n = Some(enhanced.bssid_count);

        // ── Step 6: Update shared state ──────────────────────────────
        let mut s = state.write().await;
        s.source = format!("wifi:{ssid}");
        s.rssi_history.push_back(first_rssi);
        if s.rssi_history.len() > 60 {
            s.rssi_history.pop_front();
        }

        s.tick += 1;
        let tick = s.tick;

        let motion_score = motion_score_for_level(&classification.motion_level);

        let raw_vitals = s
            .vital_detector
            .process_frame(&frame.amplitudes, &frame.phases);
        let vitals = smooth_vitals(&mut s, &raw_vitals);
        s.latest_vitals = vitals.clone();

        let feat_variance = features.variance;

        // ADR-044 §5.2: feed raw features into rolling-P95 estimators before scoring.
        s.p95_variance.push(features.variance);
        s.p95_motion_band_power.push(features.motion_band_power);
        s.p95_spectral_power.push(features.spectral_power);

        // Multi-person estimation with temporal smoothing (EMA α=0.10).
        let raw_score = compute_person_score(&s, &features);
        s.smoothed_person_score = s.smoothed_person_score * 0.90 + raw_score * 0.10;
        let est_persons = if classification.presence {
            let count = s.person_count();
            s.prev_person_count = count;
            count
        } else {
            s.prev_person_count = 0;
            0
        };

        let mut update = SensingUpdate {
            msg_type: "sensing_update".to_string(),
            timestamp: chrono::Utc::now().timestamp_millis() as f64 / 1000.0,
            source: format!("wifi:{ssid}"),
            tick,
            tx_position: s.tx_position,
            room_dimensions: s.room_dimensions,
            nodes: vec![NodeInfo {
                node_id: 0,
                rssi_dbm: first_rssi,
                position: [0.0, 0.0, 0.0],
                amplitude: multi_ap_frame.amplitudes,
                subcarrier_count: obs_count,
                sync: None, // multi-BSSID scan path — no mesh peer
            }],
            features,
            classification,
            signal_field: generate_signal_field(
                first_rssi,
                motion_score,
                breathing_rate_hz,
                feat_variance.min(1.0),
                &sub_variances,
            ),
            localization: None,
            position_estimate: None,
            vital_signs: Some(vitals),
            enhanced_motion,
            enhanced_breathing,
            posture: posture_str,
            signal_quality_score: sig_quality_score,
            quality_verdict: verdict_str,
            bssid_count: bssid_n,
            pose_keypoints: None,
            model_status: None,
            persons: None,
            estimated_persons: if est_persons > 0 {
                Some(est_persons)
            } else {
                None
            },
            node_features: None,
        };

        // Populate persons from the sensing update (Kalman-smoothed via tracker).
        let raw_persons = derive_pose_from_sensing(&update);
        let mut last_tracker_instant = s.last_tracker_instant.take();
        let tracked = tracker_bridge::tracker_update(
            &mut s.pose_tracker,
            &mut last_tracker_instant,
            raw_persons,
        );
        s.last_tracker_instant = last_tracker_instant;
        if !tracked.is_empty() {
            update.persons = Some(tracked);
        }
        // #1050: attach real signal_field-peak positions to each person.
        attach_field_positions(&mut update);

        if let Ok(json) = serde_json::to_string(&update) {
            let _ = s.tx.send(json);
        }
        s.latest_update = Some(update);

        debug!(
            "Multi-BSSID tick #{tick}: {obs_count} BSSIDs, quality={:.2}, verdict={:?}",
            enhanced.signal_quality.score, enhanced.verdict
        );
    }
}

/// Fallback: single-RSSI collection via `netsh wlan show interfaces`.
///
/// Used when the multi-BSSID scan fails or returns 0 observations.
async fn windows_wifi_fallback_tick(state: &SharedState, seq: u32) {
    let output = match tokio::process::Command::new("netsh")
        .args(["wlan", "show", "interfaces"])
        .output()
        .await
    {
        Ok(o) => String::from_utf8_lossy(&o.stdout).to_string(),
        Err(e) => {
            warn!("netsh interfaces fallback failed: {e}");
            return;
        }
    };

    let (rssi_dbm, signal_pct, ssid) = match parse_netsh_interfaces_output(&output) {
        Some(v) => v,
        None => {
            debug!("Fallback: no WiFi interface connected");
            return;
        }
    };

    let frame = Esp32Frame {
        magic: 0xC511_0001,
        node_id: 0,
        n_antennas: 1,
        n_subcarriers: 1,
        freq_mhz: 2437,
        sequence: seq,
        rssi: rssi_dbm as i8,
        noise_floor: -90,
        ppdu_type: wifi_densepose_hardware::PpduType::HtLegacy,
        amplitudes: vec![signal_pct],
        phases: vec![0.0],
    };

    let mut s = state.write().await;
    // Update frame history before extracting features.
    s.frame_history.push_back(frame.amplitudes.clone());
    if s.frame_history.len() > FRAME_HISTORY_CAPACITY {
        s.frame_history.pop_front();
    }
    let sample_rate_hz = 2.0_f64; // fallback tick ~ 500 ms => 2 Hz
    let (features, mut classification, breathing_rate_hz, sub_variances, raw_motion) =
        extract_features_from_frame(&frame, &s.frame_history, sample_rate_hz);
    smooth_and_classify(&mut s, &mut classification, raw_motion);
    adaptive_override(&s, &features, &mut classification);

    s.source = format!("wifi:{ssid}");
    s.rssi_history.push_back(rssi_dbm);
    if s.rssi_history.len() > 60 {
        s.rssi_history.pop_front();
    }

    s.tick += 1;
    let tick = s.tick;

    let motion_score = motion_score_for_level(&classification.motion_level);

    let raw_vitals = s
        .vital_detector
        .process_frame(&frame.amplitudes, &frame.phases);
    let vitals = smooth_vitals(&mut s, &raw_vitals);
    s.latest_vitals = vitals.clone();

    let feat_variance = features.variance;

    // ADR-044 §5.2: feed raw features into rolling-P95 estimators before scoring.
    s.p95_variance.push(features.variance);
    s.p95_motion_band_power.push(features.motion_band_power);
    s.p95_spectral_power.push(features.spectral_power);

    // Multi-person estimation with temporal smoothing (EMA α=0.10).
    let raw_score = compute_person_score(&s, &features);
    s.smoothed_person_score = s.smoothed_person_score * 0.90 + raw_score * 0.10;
    let est_persons = if classification.presence {
        let count = s.person_count();
        s.prev_person_count = count;
        count
    } else {
        s.prev_person_count = 0;
        0
    };

    let mut update = SensingUpdate {
        msg_type: "sensing_update".to_string(),
        timestamp: chrono::Utc::now().timestamp_millis() as f64 / 1000.0,
        source: format!("wifi:{ssid}"),
        tick,
        tx_position: s.tx_position,
        room_dimensions: s.room_dimensions,
        nodes: vec![NodeInfo {
            node_id: 0,
            rssi_dbm,
            position: [0.0, 0.0, 0.0],
            amplitude: vec![signal_pct],
            subcarrier_count: 1,
            sync: None, // synthetic-RSSI fallback path — no mesh peer
        }],
        features,
        classification,
        signal_field: generate_signal_field(
            rssi_dbm,
            motion_score,
            breathing_rate_hz,
            feat_variance.min(1.0),
            &sub_variances,
        ),
        localization: None,
        position_estimate: None,
        vital_signs: Some(vitals),
        enhanced_motion: None,
        enhanced_breathing: None,
        posture: None,
        signal_quality_score: None,
        quality_verdict: None,
        bssid_count: None,
        pose_keypoints: None,
        model_status: None,
        persons: None,
        estimated_persons: if est_persons > 0 {
            Some(est_persons)
        } else {
            None
        },
        node_features: None,
    };

    let raw_persons = derive_pose_from_sensing(&update);
    let mut last_tracker_instant = s.last_tracker_instant.take();
    let tracked =
        tracker_bridge::tracker_update(&mut s.pose_tracker, &mut last_tracker_instant, raw_persons);
    s.last_tracker_instant = last_tracker_instant;
    if !tracked.is_empty() {
        update.persons = Some(tracked);
    }
    // #1050: attach real signal_field-peak positions to each person.
    attach_field_positions(&mut update);

    if let Ok(json) = serde_json::to_string(&update) {
        let _ = s.tx.send(json);
    }
    s.latest_update = Some(update);
}

/// Probe if Windows WiFi is connected
pub(crate) async fn probe_windows_wifi() -> bool {
    match tokio::process::Command::new("netsh")
        .args(["wlan", "show", "interfaces"])
        .output()
        .await
    {
        Ok(o) => {
            let out = String::from_utf8_lossy(&o.stdout);
            parse_netsh_interfaces_output(&out).is_some()
        }
        Err(_) => false,
    }
}

/// Probe if ESP32 is streaming on UDP port
pub(crate) async fn probe_esp32(port: u16) -> bool {
    let addr = format!("0.0.0.0:{port}");
    match UdpSocket::bind(&addr).await {
        Ok(sock) => {
            // 4096 covers the largest ADR-018 frame plus the optional
            // 40-byte runtime TX-source-binding trailer. On Windows a too-small
            // recv buffer makes recv_from error on the oversized datagram,
            // which made this probe fail against HE-only streams.
            let mut buf = [0u8; 4096];
            match tokio::time::timeout(Duration::from_secs(2), sock.recv_from(&mut buf)).await {
                Ok(Ok((len, _))) => parse_esp32_frame(&buf[..len]).is_some(),
                _ => false,
            }
        }
        Err(_) => false,
    }
}

pub(crate) fn spawn_mmwave_node_diagnostics_poller(
    state: SharedState,
    configured_url: Option<String>,
    token: Option<String>,
) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(MMWAVE_NODE_DIAGNOSTICS_POLL_INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            let (preferred, expected, port) = {
                let state = state.read().await;
                let expected = state
                    .position_setup
                    .as_deref()
                    .and_then(|setup| setup.mmwave())
                    .map(|node| node.node_id().to_string())
                    .or_else(|| state.mmwave_connection.node_id.clone());
                (
                    configured_url
                        .clone()
                        .or_else(|| state.mmwave_connection.node_url.clone()),
                    expected,
                    state
                        .mmwave
                        .status(server_clock::now().host_monotonic_ns)
                        .udp_port,
                )
            };
            let token_for_probe = token.clone();
            let probe = tokio::task::spawn_blocking(move || {
                mmwave_connection::probe(
                    preferred.as_deref(),
                    expected.as_deref(),
                    port,
                    token_for_probe.as_deref(),
                    true,
                )
            })
            .await;
            if let Ok(mut probe) = probe {
                let mut state = state.write().await;
                state
                    .mmwave
                    .discovered_control(probe.status.node_url.clone(), token.clone());
                if !probe.status.reachable {
                    // Keep the established identity across outages so discovery
                    // cannot silently substitute a different physical radar.
                    probe.status.node_id = state.mmwave_connection.node_id.clone();
                    probe.status.node_url = state.mmwave_connection.node_url.clone();
                }
                state.mmwave_node_diagnostics.record(probe.diagnostics);
                state.mmwave_connection = probe.status;
            }
        }
    });
}

pub(crate) fn spawn_mmwave_session_ticker(state: SharedState) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(MMWAVE_SESSION_TICK_INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            let now = server_clock::now();
            let mut state = state.write().await;
            if let Err(error) = state.mmwave.tick(now) {
                debug!("mmWave session tick could not finalize the empty phase: {error}");
            }
        }
    });
}

const MMWAVE_SESSION_TICK_INTERVAL: Duration = Duration::from_millis(100);
pub(crate) const DEFAULT_MMWAVE_REORDER_HOLD_MS: u64 = 20;
pub(crate) const DEFAULT_MMWAVE_RECEIVE_BUFFER_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, Copy, Deserialize)]
struct MmwavePacketOrder {
    boot_id: u32,
    sequence: u32,
}

struct QueuedMmwaveDatagram {
    bytes: Vec<u8>,
    source: SocketAddr,
    host_time: server_clock::HostTimestamp,
    order: Option<MmwavePacketOrder>,
}

impl QueuedMmwaveDatagram {
    fn new(bytes: Vec<u8>, source: SocketAddr, host_time: server_clock::HostTimestamp) -> Self {
        let order = serde_json::from_slice(&bytes).ok();
        Self {
            bytes,
            source,
            host_time,
            order,
        }
    }
}

fn compare_mmwave_sequence(left: u32, right: u32) -> std::cmp::Ordering {
    if left == right {
        std::cmp::Ordering::Equal
    } else if right.wrapping_sub(left) < u32::MAX / 2 {
        std::cmp::Ordering::Less
    } else {
        std::cmp::Ordering::Greater
    }
}

fn mmwave_sequence_is_newer(candidate: u32, previous: u32) -> bool {
    candidate != previous && candidate.wrapping_sub(previous) < u32::MAX / 2
}

fn sort_mmwave_datagram_batch(batch: &mut [QueuedMmwaveDatagram]) {
    let Some(boot_id) = batch
        .first()
        .and_then(|packet| packet.order)
        .map(|order| order.boot_id)
    else {
        return;
    };
    if !batch
        .iter()
        .all(|packet| packet.order.is_some_and(|order| order.boot_id == boot_id))
    {
        // Preserve arrival order across malformed packets or a real node reboot
        // so the fail-closed validation path can report them accurately.
        return;
    }
    batch.sort_by(|left, right| {
        compare_mmwave_sequence(
            left.order.expect("single valid boot was checked").sequence,
            right.order.expect("single valid boot was checked").sequence,
        )
    });
}

async fn process_mmwave_batch(
    state: &SharedState,
    transport_metrics: &mmwave_calibration::MmwaveTransportMetrics,
    batch: &mut Vec<QueuedMmwaveDatagram>,
    last_forwarded: &mut Option<MmwavePacketOrder>,
) {
    sort_mmwave_datagram_batch(batch);
    for packet in batch.drain(..) {
        transport_metrics.note_dequeued();
        if let Some(order) = packet.order {
            if last_forwarded.is_some_and(|previous| {
                previous.boot_id == order.boot_id
                    && !mmwave_sequence_is_newer(order.sequence, previous.sequence)
            }) {
                if last_forwarded.is_some_and(|previous| previous.sequence == order.sequence) {
                    transport_metrics.note_duplicate();
                } else {
                    transport_metrics.note_sequence_discard();
                }
                continue;
            }
            *last_forwarded = Some(order);
        }
        process_mmwave_datagram(
            state,
            packet.bytes,
            packet.source,
            packet.host_time,
            transport_metrics,
        )
        .await;
    }
}

fn bind_mmwave_socket(address: &str, receive_buffer_bytes: usize) -> std::io::Result<UdpSocket> {
    let address: SocketAddr = address
        .parse()
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidInput, error))?;
    let socket = socket2::Socket::new(
        socket2::Domain::IPV4,
        socket2::Type::DGRAM,
        Some(socket2::Protocol::UDP),
    )?;
    socket.set_reuse_address(true)?;
    socket.set_recv_buffer_size(receive_buffer_bytes)?;
    socket.set_nonblocking(true)?;
    socket.bind(&address.into())?;
    UdpSocket::from_std(socket.into())
}

pub(crate) async fn mmwave_receiver_task(
    state: SharedState,
    port: u16,
    receive_buffer_bytes: usize,
    reorder_hold_ms: u64,
) {
    let address = format!("0.0.0.0:{port}");
    let socket = match bind_mmwave_socket(&address, receive_buffer_bytes) {
        Ok(socket) => socket,
        Err(error) => {
            error!("Could not bind mmWave UDP receiver to {address}: {error}");
            return;
        }
    };
    info!(
        "mmWave UDP receiver listening on {address} (SO_RCVBUF requested={} bytes, reorder_hold={} ms)",
        receive_buffer_bytes,
        reorder_hold_ms
    );
    // Keep socket reads independent from the CSI processing lock. The four
    // RX streams can hold `state` for a non-trivial amount of time; reading
    // mmWave datagrams only after that lock was released allowed the kernel
    // UDP queue to overflow and turned ordinary queue pressure into radar
    // sequence gaps. An unbounded in-process queue preserves every received
    // datagram and the order in which the socket delivered it.
    let (packet_tx, mut packet_rx) = mpsc::unbounded_channel::<QueuedMmwaveDatagram>();
    let transport_metrics = state.read().await.mmwave.transport_metrics();
    let processor_state = state.clone();
    let processor_metrics = transport_metrics.clone();
    tokio::spawn(async move {
        let mut batch = Vec::new();
        let mut last_forwarded = None;
        let mut flush_interval = tokio::time::interval(Duration::from_millis(
            reorder_hold_ms.max(1),
        ));
        flush_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        flush_interval.tick().await;
        loop {
            tokio::select! {
                packet = packet_rx.recv() => match packet {
                    Some(packet) => {
                        batch.push(packet);
                    }
                    None => {
                        process_mmwave_batch(
                            &processor_state,
                            &processor_metrics,
                            &mut batch,
                            &mut last_forwarded,
                        ).await;
                        return;
                    }
                },
                _ = flush_interval.tick() => {
                    process_mmwave_batch(
                        &processor_state,
                        &processor_metrics,
                        &mut batch,
                        &mut last_forwarded,
                    ).await;
                }
            }
        }
    });

    let mut buffer = [0_u8; 4096];
    loop {
        match socket.recv_from(&mut buffer).await {
            Ok((length, source)) => {
                let packet = QueuedMmwaveDatagram::new(
                    buffer[..length].to_vec(),
                    source,
                    server_clock::now(),
                );
                transport_metrics.note_received();
                if packet_tx.send(packet).is_err() {
                    transport_metrics.note_dequeued();
                    error!("mmWave processor stopped; UDP receiver is shutting down");
                    return;
                }
            }
            Err(error) => warn!("mmWave UDP receive failed: {error}"),
        }
    }
}

#[cfg(test)]
mod mmwave_udp_order_tests {
    use super::{compare_mmwave_sequence, mmwave_sequence_is_newer};

    #[test]
    fn sequence_order_repairs_short_udp_reordering() {
        let mut sequences = vec![12, 10, 11, 13];
        sequences.sort_by(|left, right| compare_mmwave_sequence(*left, *right));
        assert_eq!(sequences, vec![10, 11, 12, 13]);
    }

    #[test]
    fn sequence_order_handles_u32_wraparound() {
        let mut sequences = vec![0, u32::MAX - 1, u32::MAX, 1];
        sequences.sort_by(|left, right| compare_mmwave_sequence(*left, *right));
        assert_eq!(sequences, vec![u32::MAX - 1, u32::MAX, 0, 1]);
    }

    #[test]
    fn duplicate_and_late_sequences_are_not_newer() {
        assert!(!mmwave_sequence_is_newer(42, 42));
        assert!(!mmwave_sequence_is_newer(41, 42));
        assert!(mmwave_sequence_is_newer(43, 42));
        assert!(mmwave_sequence_is_newer(0, u32::MAX));
    }
}

async fn process_mmwave_datagram(
    state: &SharedState,
    bytes: Vec<u8>,
    source: SocketAddr,
    host_time: server_clock::HostTimestamp,
    transport_metrics: &mmwave_calibration::MmwaveTransportMetrics,
) {
    let started_at = std::time::Instant::now();
    let received_at_monotonic_ns = host_time.host_monotonic_ns;
    let mut state = state.write().await;
    let wifi_prediction = state.live_position_tracker.current().clone();
    state.mmwave.observe_wifi_prediction(&wifi_prediction);
    let accepted = match state.mmwave.ingest_json(&bytes, host_time) {
        Ok(()) => true,
        Err(reason) => {
            debug!("Rejected mmWave packet from {source}: {reason}");
            false
        }
    };
    if let Some((index_path, index_sha256)) = state.mmwave.take_pending_index() {
        let setup_identity = state.position_setup.as_ref().map(|setup| {
            (
                setup.setup_id().to_string(),
                setup.setup_sha256().to_string(),
            )
        });
        if let Some((setup_id, setup_sha256)) = setup_identity {
            match position_live::PositionIndexRuntime::load(
                &index_path,
                &setup_id,
                &setup_sha256,
                Some(&index_sha256),
            ) {
                Ok(runtime) => {
                    state.live_position_tracker.install_runtime(Some(runtime));
                    info!("Installed mmWave-gated position index {}", index_sha256);
                }
                Err(error) => {
                    error!("Generated mmWave position index failed runtime validation: {error}")
                }
            }
        }
    }
    transport_metrics.note_processed(
        received_at_monotonic_ns,
        server_clock::now().host_monotonic_ns,
        started_at.elapsed().as_millis() as u64,
        accepted,
    );
}

// ── Recording Endpoints ─────────────────────────────────────────────────────

pub(crate) async fn finalize_active_recording(
    state: &SharedState,
) -> Result<(String, u64, RecordingWriterResult), String> {
    let lifecycle = {
        let s = state.read().await;
        s.recording_lifecycle.clone()
    };
    let lifecycle_guard = lifecycle.lock_owned().await;

    let (recording_id, duration_secs, stop_tx, done_rx) = {
        let mut s = state.write().await;
        if s.recording_phase != RecordingLifecyclePhase::Recording || !s.recording_active {
            return Err("no recording in progress".to_string());
        }

        let recording_id = s
            .recording_current_id
            .clone()
            .ok_or_else(|| "active recording has no recording ID".to_string())?;
        let duration_secs = s
            .recording_start_time
            .map(|started| started.elapsed().as_secs())
            .unwrap_or(0);

        // This flag is also the producer gate. Flipping it while holding the
        // same state lock as the UDP loop creates a precise last-frame boundary:
        // no new raw frame can be sent after this point.
        s.recording_active = false;
        s.recording_phase = RecordingLifecyclePhase::Finalizing;
        let stop_tx = s.recording_stop_tx.take();
        let done_rx = s.recording_done_rx.take();
        for recording in s.recordings.iter_mut() {
            if recording.get("id").and_then(|value| value.as_str()) == Some(recording_id.as_str()) {
                recording["status"] = serde_json::json!("finalizing");
            }
        }
        (recording_id, duration_secs, stop_tx, done_rx)
    };

    // The owned task is deliberately detached from the HTTP request future.
    // Dropping a JoinHandle does not cancel its task, so a disconnected client
    // cannot strand the recorder in `Finalizing` after the producer gate closed.
    let finalizer_state = state.clone();
    let finalizer = tokio::spawn(async move {
        let _lifecycle_guard = lifecycle_guard;

        if let Some(stop_tx) = stop_tx {
            let _ = stop_tx.send(true);
        }

        let mut result = match done_rx {
            Some(done_rx) => done_rx.await.unwrap_or_else(|_| RecordingWriterResult {
                error: Some("recording writer ended without a completion result".to_string()),
                ..Default::default()
            }),
            None => RecordingWriterResult {
                error: Some("recording writer completion channel is missing".to_string()),
                ..Default::default()
            },
        };

        if let Err(error) = finalize_recording_metadata(&recording_id, duration_secs, &result) {
            append_recording_writer_error(&mut result, error);
        }

        let mut s = finalizer_state.write().await;
        s.recording_current_id = None;
        s.recording_start_time = None;
        s.recording_phase = RecordingLifecyclePhase::Idle;
        for recording in s.recordings.iter_mut() {
            if recording.get("id").and_then(|value| value.as_str()) == Some(recording_id.as_str()) {
                recording["status"] = serde_json::json!(if result.incomplete() {
                    "incomplete"
                } else {
                    "completed"
                });
                recording["duration_secs"] = serde_json::json!(duration_secs);
                recording["frames"] = serde_json::json!(result.frames_written);
                recording["frame_count"] = serde_json::json!(result.frames_written);
                recording["rx_summaries"] =
                    serde_json::json!(result.rx_summaries.values().collect::<Vec<_>>());
                recording["dropped_frames"] = serde_json::json!(result.dropped_frames);
            }
        }
        drop(s);

        (recording_id, duration_secs, result)
    });

    finalizer
        .await
        .map_err(|error| format!("recording finalizer task failed: {error}"))
}

pub(crate) async fn settle_recording_on_shutdown(
    state: &SharedState,
) -> Result<Option<(String, u64, RecordingWriterResult)>, String> {
    loop {
        let (phase, lifecycle) = {
            let s = state.read().await;
            (s.recording_phase, s.recording_lifecycle.clone())
        };
        match phase {
            RecordingLifecyclePhase::Idle => return Ok(None),
            RecordingLifecyclePhase::Recording => {
                return finalize_active_recording(state).await.map(Some);
            }
            RecordingLifecyclePhase::Finalizing => {
                // The owned finalizer holds this mutex through metadata commit
                // and state cleanup. Acquiring it therefore waits for a
                // cancellation-detached finalization already in progress.
                let guard = lifecycle.lock().await;
                drop(guard);
                if state.read().await.recording_phase == RecordingLifecyclePhase::Finalizing {
                    return Err(
                        "recording finalizer ended without clearing finalizing state".to_string(),
                    );
                }
            }
        }
    }
}

// ── UDP receiver task ────────────────────────────────────────────────────────

pub(crate) async fn udp_receiver_task(state: SharedState, udp_port: u16) {
    let addr = format!("0.0.0.0:{udp_port}");
    let socket = match UdpSocket::bind(&addr).await {
        Ok(s) => {
            info!("UDP listening on {addr} for ESP32 CSI frames");
            s
        }
        Err(e) => {
            error!("Failed to bind UDP {addr}: {e}");
            return;
        }
    };

    let mut buf = [0u8; 4096];
    loop {
        match socket.recv_from(&mut buf).await {
            Ok((len, src)) => {
                // ADR-039: Try edge vitals packet first (magic 0xC511_0002).
                if let Some(vitals) = parse_esp32_vitals(&buf[..len]) {
                    debug!(
                        "ESP32 vitals from {src}: node={} br={:.1} hr={:.1} pres={}",
                        vitals.node_id,
                        vitals.breathing_rate_bpm,
                        vitals.heartrate_bpm,
                        vitals.presence
                    );
                    let mut s = state.write().await;
                    if !edge_vitals_measurement_input_allowed(s.position_setup.is_some()) {
                        // A sealed experiment accepts only exact raw CSI whose
                        // TX-source trailer matches the setup. Do not touch
                        // global/per-RX liveness, RSSI, features, D4/D5/D6,
                        // position state, or the raw recorder for this packet.
                        debug!(
                            "ignoring edge-vitals packet from node {} while a sealed position setup is active",
                            vitals.node_id
                        );
                        continue;
                    }
                    // Issue #323: Also emit a sensing_update so the UI renders
                    // detections for ESP32 nodes running the edge DSP pipeline
                    // (Tier 2+).  Without this, vitals arrive but the UI shows
                    // "no detection" because it only renders sensing_update msgs.
                    s.source = "esp32".to_string();
                    s.last_esp32_frame = Some(std::time::Instant::now());

                    // ── Per-node state for edge vitals (issue #249) ──────
                    let node_id = vitals.node_id;
                    let calibration_bundle = s.active_calibration_bundle.clone();
                    let ns = s.node_states.entry(node_id).or_insert_with(|| {
                        NodeState::new_with_calibration(node_id, calibration_bundle.as_deref())
                    });
                    ns.last_frame_time = Some(std::time::Instant::now());
                    ns.edge_vitals = Some(vitals.clone());
                    ns.rssi_history.push_back(vitals.rssi as f64);
                    if ns.rssi_history.len() > 60 {
                        ns.rssi_history.pop_front();
                    }

                    // Store per-node person count from edge vitals.
                    let node_est = if vitals.presence {
                        (vitals.n_persons as usize).max(1)
                    } else {
                        0
                    };
                    ns.prev_person_count = node_est;

                    s.tick += 1;
                    let tick = s.tick;

                    let now = std::time::Instant::now();
                    let mut classification = edge_vitals_classification(&vitals);

                    // Edge-vitals is a useful fallback when a node does not
                    // provide raw CSI. Once D6 is calibrated, however, the
                    // static CSI fingerprints remain authoritative for room
                    // presence. Otherwise an interleaved vitals packet could
                    // overwrite a valid D6 position with the edge classifier.
                    if s.d5_presence.phase() == d5_presence::CalibrationPhase::Ready {
                        let sref: &mut AppStateInner = &mut s;
                        classification = aggregate_node_classification(
                            &sref.node_states,
                            now,
                            &mut sref.d5_presence,
                        );
                    } else {
                        let n_active = s
                            .node_states
                            .values()
                            .filter(|ns| {
                                ns.last_frame_time
                                    .is_some_and(|t| now.duration_since(t).as_secs() < 10)
                            })
                            .count();
                        if n_active > 1 {
                            classification.confidence = (classification.confidence
                                * (1.0 + 0.15 * (n_active as f64 - 1.0)))
                                .clamp(0.0, 1.0);
                        }
                    }
                    classification = apply_position_setup_classification_gate(
                        s.position_setup.is_some(),
                        s.d5_presence.phase(),
                        classification,
                    );
                    let public_vitals = public_edge_vitals_packet(&vitals, &classification);
                    if let Ok(json) = serde_json::to_string(&serde_json::json!({
                        "type": "edge_vitals",
                        "node_id": public_vitals.node_id,
                        "presence": public_vitals.presence,
                        "fall_detected": public_vitals.fall_detected,
                        "motion": public_vitals.motion,
                        "breathing_rate_bpm": public_vitals.breathing_rate_bpm,
                        "heartrate_bpm": public_vitals.heartrate_bpm,
                        "n_persons": public_vitals.n_persons,
                        "motion_energy": public_vitals.motion_energy,
                        "presence_score": public_vitals.presence_score,
                        "rssi": public_vitals.rssi,
                    })) {
                        let _ = s.tx.send(json);
                    }

                    // Aggregate person count only after the authoritative
                    // presence decision, matching the raw-CSI path.
                    let _total_persons = if classification.presence {
                        let dedup = s.dedup_factor;
                        let (fused, fallback_count) = multistatic_bridge::fuse_or_fallback(
                            &s.multistatic_fuser,
                            &s.node_states,
                            dedup,
                        );
                        match fused {
                            Some(ref f) => {
                                let score =
                                    multistatic_bridge::compute_person_score_from_amplitudes(
                                        &f.fused_amplitude,
                                    );
                                s.smoothed_person_score =
                                    s.smoothed_person_score * 0.90 + score * 0.10;
                                // #803: don't let the saturating activity score
                                // discard count-aware per-node estimates.
                                let count =
                                    aggregate_person_count(s.person_count(), &s.node_states);
                                s.prev_person_count = count;
                                count.max(1) // presence=true => at least 1
                            }
                            None => {
                                aggregate_person_count(fallback_count.unwrap_or(0), &s.node_states)
                                    .max(1)
                            }
                        }
                    } else {
                        s.prev_person_count = 0;
                        0
                    };

                    // Governed trust cycle (ADR-135..146): run the same live
                    // frames through the privacy/provenance/witness control
                    // plane. Trust state is recorded on the bridge (exposed on
                    // /api/v1/status); engine errors are counted + rate-limit
                    // logged instead of being swallowed (review finding 1).
                    // Split-borrow the two distinct fields off the guard.
                    {
                        let sref: &mut AppStateInner = &mut s;
                        let now_ms = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_millis() as i64)
                            .unwrap_or(0);
                        sref.engine_bridge.observe_cycle(&sref.node_states, now_ms);
                    }

                    // Feed field model calibration if active (use per-node history for ESP32).
                    if let Some(frame_history) = s
                        .node_states
                        .get(&node_id)
                        .map(|ns| ns.frame_history.clone())
                    {
                        if let Some(ref mut fm) = s.field_model {
                            field_bridge::maybe_feed_calibration(fm, &frame_history);
                        }
                    }

                    // Build nodes array with all active nodes.
                    let configured_positions = s.multistatic_fuser.node_positions().to_vec();
                    let mut active_nodes: Vec<NodeInfo> = s
                        .node_states
                        .iter()
                        .filter(|(_, n)| {
                            n.last_frame_time
                                .is_some_and(|t| now.duration_since(t).as_secs() < 10)
                        })
                        .map(|(&id, n)| NodeInfo {
                            node_id: id,
                            rssi_dbm: n.rssi_history.back().copied().unwrap_or(0.0),
                            position: configured_node_position(id, &configured_positions),
                            amplitude: vec![],
                            subcarrier_count: 0,
                            // Vitals-only path; still expose the sync snapshot
                            // if the node also speaks ESP-NOW.
                            sync: n.sync_snapshot(),
                        })
                        .collect();
                    active_nodes.sort_by_key(|node| node.node_id);

                    let features = FeatureInfo {
                        mean_rssi: vitals.rssi as f64,
                        variance: vitals.motion_energy as f64,
                        motion_band_power: vitals.motion_energy as f64,
                        breathing_band_power: if vitals.presence { 0.5 } else { 0.0 },
                        dominant_freq_hz: vitals.breathing_rate_bpm / 60.0,
                        change_points: 0,
                        spectral_power: vitals.motion_energy as f64,
                    };

                    // Store latest features on node for cross-node fusion.
                    if let Some(ns) = s.node_states.get_mut(&node_id) {
                        ns.latest_features = Some(features.clone());
                    }

                    // Cross-node fusion: combine features from all active nodes.
                    let fused_features = fuse_multi_node_features(&features, &s.node_states);

                    // Edge-vitals packets contain classifications and vital
                    // estimates but no per-subcarrier fingerprints of their
                    // own. Reuse only fresh D6 evidence already held by the
                    // raw-CSI path; the estimator fails closed otherwise.
                    let localization = estimate_live_localization(
                        &s.node_states,
                        now,
                        &classification,
                        s.tx_position,
                        s.room_dimensions,
                        &configured_positions,
                    );
                    let signal_field = signal_field_from_localization(&localization);
                    let candidate_position_estimate = match raw_csi_recording::now_unix_ns() {
                        Ok(now_unix_ns) => s.live_position_tracker.expire_if_raw_stale(now_unix_ns),
                        Err(error) => s.live_position_tracker.reject_input(format!(
                            "could not verify raw CSI freshness for edge vitals: {error}"
                        )),
                    };
                    let position_estimate = gate_mmwave_candidate_for_publication(
                        candidate_position_estimate,
                        s.mmwave.position_publication_allowed(),
                    );
                    let has_valid_position = classification.presence
                        && matches!(
                            &position_estimate,
                            position_live::LivePositionState::Position { .. }
                        );

                    let mut update = SensingUpdate {
                        msg_type: "sensing_update".to_string(),
                        timestamp: chrono::Utc::now().timestamp_millis() as f64 / 1000.0,
                        source: "esp32".to_string(),
                        tick,
                        tx_position: s.tx_position,
                        room_dimensions: s.room_dimensions,
                        nodes: active_nodes,
                        features: fused_features.clone(),
                        classification,
                        signal_field,
                        localization: Some(localization),
                        position_estimate: Some(position_estimate),
                        vital_signs: Some(VitalSigns {
                            breathing_rate_bpm: if vitals.breathing_rate_bpm > 0.0 {
                                Some(vitals.breathing_rate_bpm)
                            } else {
                                None
                            },
                            heart_rate_bpm: if vitals.heartrate_bpm > 0.0 {
                                Some(vitals.heartrate_bpm)
                            } else {
                                None
                            },
                            breathing_confidence: if vitals.presence { 0.7 } else { 0.0 },
                            heartbeat_confidence: if vitals.presence { 0.7 } else { 0.0 },
                            signal_quality: vitals.presence_score as f64,
                        }),
                        enhanced_motion: None,
                        enhanced_breathing: None,
                        posture: None,
                        signal_quality_score: None,
                        quality_verdict: None,
                        bssid_count: None,
                        pose_keypoints: None,
                        model_status: None,
                        persons: None,
                        estimated_persons: has_valid_position.then_some(1),
                        // ADR-084 Pass 3.6: surface per-node novelty_score
                        // (and the rest of the per-node feature snapshot)
                        // on the WebSocket envelope so cluster-Pi consumers
                        // can implement model-wake gating without round-
                        // tripping back to the server.
                        node_features: build_node_features(
                            &s.node_states,
                            now,
                            s.d5_presence.phase(),
                            s.position_setup.is_some(),
                        ),
                    };

                    let persons = derive_pose_from_sensing(&update);
                    s.pose_tracker = PoseTracker::new();
                    s.last_tracker_instant = None;
                    if !persons.is_empty() {
                        update.persons = Some(persons);
                    }
                    // ESP32 persons are exact discrete markers, never tracked
                    // synthetic skeletons or coarse signal-field peaks.
                    attach_field_positions(&mut update);

                    if let Ok(json) = serde_json::to_string(&update) {
                        let _ = s.tx.send(json);
                    }
                    s.latest_update = Some(update);
                    s.edge_vitals = Some(vitals);
                    continue;
                }

                // ADR-110 §A0.12: Try sync packet (magic 0xC511_A110).
                // A 32-byte UDP datagram carrying mesh-aligned epoch + sequence
                // high-water from the node's c6_sync_espnow EMA-smoothed offset.
                // Stored per-node so subsequent CSI frames with byte 19 bit 4
                // set can have an aligned timestamp recovered downstream.
                if len >= wifi_densepose_hardware::SYNC_PACKET_SIZE {
                    let magic = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
                    if magic == wifi_densepose_hardware::SYNC_PACKET_MAGIC {
                        match wifi_densepose_hardware::SyncPacket::from_bytes(&buf[..len]) {
                            Ok(sync) => {
                                debug!("ESP32 sync from {src}: node={} leader={} valid={} smoothed={} \
                                        seq={} offset_us={}",
                                       sync.node_id, sync.flags.is_leader, sync.flags.is_valid,
                                       sync.flags.smoothed_used, sync.sequence,
                                       sync.local_minus_epoch_us());
                                let mut s = state.write().await;
                                let node_id = sync.node_id;
                                let calibration_bundle = s.active_calibration_bundle.clone();
                                let ns = s.node_states.entry(node_id).or_insert_with(|| {
                                    NodeState::new_with_calibration(
                                        node_id,
                                        calibration_bundle.as_deref(),
                                    )
                                });
                                ns.apply_sync_packet(sync, std::time::Instant::now());
                                continue;
                            }
                            Err(e) => {
                                debug!("Sync packet decode error from {src}: {e}");
                                // Fall through — magic matched but decode failed; not a CSI frame.
                                continue;
                            }
                        }
                    }
                }

                // ADR-063: Try edge fused vitals packet (magic 0xC511_0004).
                // Must come BEFORE the WASM parser — issue #928: these two
                // packet types shared a magic and the WASM parser was eating
                // fused-vitals frames on the C6+mmWave config. The reassign of
                // WASM_OUTPUT_MAGIC → 0xC511_0007 (firmware side) plus this
                // dedicated parser resolve the collision.
                if let Some(fused) = parse_edge_fused_vitals(&buf[..len]) {
                    debug!(
                        "Edge fused vitals from {src}: node={} br={:.1} hr={:.1} \
                         mmwave_targets={} fusion_conf={}",
                        fused.node_id,
                        fused.breathing_rate_bpm,
                        fused.heartrate_bpm,
                        fused.mmwave_targets,
                        fused.fusion_confidence,
                    );
                    let s = state.write().await;
                    if let Ok(json) = serde_json::to_string(&serde_json::json!({
                        "type": "edge_fused_vitals",
                        "node_id": fused.node_id,
                        "breathing_rate_bpm": fused.breathing_rate_bpm,
                        "heartrate_bpm": fused.heartrate_bpm,
                        "n_persons": fused.n_persons,
                        "fusion_confidence": fused.fusion_confidence,
                        "mmwave": {
                            "hr_bpm": fused.mmwave_hr_bpm,
                            "br_bpm": fused.mmwave_br_bpm,
                            "distance_cm": fused.mmwave_distance_cm,
                            "targets": fused.mmwave_targets,
                            "confidence": fused.mmwave_confidence,
                            "type": fused.mmwave_type,
                        },
                        "motion_energy": fused.motion_energy,
                        "presence_score": fused.presence_score,
                        "timestamp_ms": fused.timestamp_ms,
                    })) {
                        let _ = s.tx.send(json);
                    }
                    continue;
                }

                // ADR-040: Try WASM output packet (magic 0xC511_0007 post-#928).
                if let Some(wasm_output) = parse_wasm_output(&buf[..len]) {
                    debug!(
                        "WASM output from {src}: node={} module={} events={}",
                        wasm_output.node_id,
                        wasm_output.module_id,
                        wasm_output.events.len()
                    );
                    let mut s = state.write().await;
                    // Broadcast WASM events via WebSocket.
                    if let Ok(json) = serde_json::to_string(&serde_json::json!({
                        "type": "wasm_event",
                        "node_id": wasm_output.node_id,
                        "module_id": wasm_output.module_id,
                        "events": wasm_output.events,
                    })) {
                        let _ = s.tx.send(json);
                    }
                    s.latest_wasm_events = Some(wasm_output);
                    continue;
                }

                if let Some(frame) = parse_esp32_frame(&buf[..len]) {
                    debug!(
                        "ESP32 frame from {src}: node={}, subs={}, seq={}",
                        frame.node_id, frame.n_subcarriers, frame.sequence
                    );

                    let mut s = state.write().await;
                    let frame_now = std::time::Instant::now();

                    // Decode the exact raw frame for live positioning on every
                    // CSI packet before it can affect source liveness,
                    // classification, D4/D5/D6, positioning, or recording.
                    let mesh_timestamp_us = s
                        .node_states
                        .get(&frame.node_id)
                        .and_then(|node| node.mesh_aligned_us_for_csi_frame(frame.sequence));
                    let host_time = server_clock::now();
                    let live_position_timestamp_ns = host_time.host_unix_ns;
                    let context = raw_csi_recording::RawCsiFrameContext {
                        host_timestamp_unix_ns: live_position_timestamp_ns,
                        host_monotonic_ns: Some(host_time.host_monotonic_ns),
                        clock_epoch_id: Some(host_time.clock_epoch_id),
                        mesh_timestamp_us,
                        ..Default::default()
                    };
                    let position_setup = s.position_setup.clone();
                    let raw_frame =
                        match raw_csi_recording::RawCsiFrame::from_packet(&buf[..len], context) {
                            Ok(raw_frame) => raw_frame,
                            Err(error) => {
                                reject_raw_csi_ingress(
                                    &mut s,
                                    Some(frame.node_id),
                                    format!("exact raw CSI validation failed: {error}"),
                                );
                                warn!(
                                "node {}: parsed CSI datagram rejected by exact raw validation: {}",
                                frame.node_id, error
                            );
                                continue;
                            }
                        };
                    let matches_sealed_grid = if let Some(setup) = position_setup.as_deref() {
                        if let Err(error) = setup.validate_raw_csi_source_identity(&raw_frame) {
                            reject_raw_csi_ingress(
                                &mut s,
                                Some(frame.node_id),
                                format!("sealed position setup rejected source identity: {error}"),
                            );
                            warn!(
                                "node {}: sealed position setup rejected raw CSI source identity: {}",
                                frame.node_id, error
                            );
                            continue;
                        }
                        match setup.raw_csi_frame_matches_expected_grid(&raw_frame) {
                            Ok(matches) => matches,
                            Err(error) => {
                                reject_raw_csi_ingress(
                                    &mut s,
                                    Some(frame.node_id),
                                    format!("sealed position setup rejected receiver: {error}"),
                                );
                                continue;
                            }
                        }
                    } else {
                        true
                    };
                    let matches_pinned_grid = s
                        .csi_grid_pin
                        .is_none_or(|grid_pin| grid_pin.matches(&raw_frame));
                    let matches_runtime_grid = matches_sealed_grid && matches_pinned_grid;
                    let source_binding_observation = match validated_complete_source_binding(
                        raw_frame.source_binding.as_ref(),
                        frame_now,
                        position_setup.is_some(),
                    ) {
                        Ok(observation) => observation,
                        Err(error) => {
                            reject_raw_csi_ingress(
                                &mut s,
                                Some(frame.node_id),
                                format!("TX-source binding validation failed: {error}"),
                            );
                            continue;
                        }
                    };

                    // A valid controlled transmitter can emit multiple CSI
                    // symbol grids. Keep its binding fresh, but never mix an
                    // off-grid frame into D5/D6, recording, or live position.
                    let calibration_bundle = s.active_calibration_bundle.clone();
                    let grid_accepted = s
                        .node_states
                        .entry(frame.node_id)
                        .or_insert_with(|| {
                            NodeState::new_with_calibration(
                                frame.node_id,
                                calibration_bundle.as_deref(),
                            )
                        })
                        .observe_validated_grid(
                            source_binding_observation,
                            frame.grid(),
                            matches_runtime_grid,
                        );

                    if !grid_accepted {
                        debug!(
                            "node {}: filtering {}-subcarrier {:?} frame (active grid {:?}, sealed grid match {}, pinned grid match {})",
                            frame.node_id,
                            frame.n_subcarriers,
                            frame.ppdu_type,
                            s.node_states
                                .get(&frame.node_id)
                                .and_then(|ns| ns.active_grid),
                            matches_sealed_grid,
                            matches_pinned_grid,
                        );
                        continue;
                    }

                    s.mmwave.observe_csi(&raw_frame);

                    s.source = "esp32".to_string();
                    s.last_esp32_frame = Some(frame_now);

                    if s.recording_active
                        && s.raw_csi_tx
                            .send(RawCsiIngress::Frame(raw_frame.clone()))
                            .is_err()
                    {
                        warn!(
                            "node {}: active raw recorder has no receiver",
                            frame.node_id
                        );
                    }
                    let live_position_input_accepted = match route_raw_frame_to_live_position(
                        &mut s.live_position_tracker,
                        position_setup.as_deref(),
                        true,
                        raw_frame,
                    ) {
                        Ok(()) => {
                            s.last_raw_csi_frame = Some(frame_now);
                            true
                        }
                        Err(error) => {
                            let estimate = s.live_position_tracker.current().clone();
                            replace_latest_esp32_position_estimate(&mut s, estimate);
                            debug!(
                                "node {}: live position rejected raw frame: {}",
                                frame.node_id, error
                            );
                            false
                        }
                    };

                    // Also maintain global frame_history for backward compat
                    // (simulation path, REST endpoints, etc.).
                    s.frame_history.push_back(frame.amplitudes.clone());
                    if s.frame_history.len() > FRAME_HISTORY_CAPACITY {
                        s.frame_history.pop_front();
                    }

                    // ── ADR-099: real-time introspection tap ────────────────
                    // Per-frame update of the attractor / DTW pipeline running
                    // parallel to the window-aggregated event path. Placed
                    // BEFORE the per-node `&mut` borrow of `s.node_states` so
                    // `s.intro` / `s.intro_tx` stay reachable. Never window-
                    // blocked; `/ws/introspection` sees a fresh snapshot on
                    // every accepted frame.
                    {
                        let intro_feature = if frame.amplitudes.is_empty() {
                            0.0
                        } else {
                            frame.amplitudes.iter().copied().sum::<f64>()
                                / frame.amplitudes.len() as f64
                        };
                        let intro_ts_ns = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_nanos() as u64)
                            .unwrap_or(0);
                        let _ = s.intro.update(intro_ts_ns, intro_feature);
                        if let Ok(intro_json) = serde_json::to_string(s.intro.snapshot()) {
                            let _ = s.intro_tx.send(intro_json);
                        }
                    }

                    // ── Per-node processing (issue #249) ──────────────────
                    // Process entirely within per-node state so different
                    // ESP32 nodes never mix their smoothing/vitals buffers.
                    // We scope the mutable borrow of node_states so we can
                    // access other AppStateInner fields afterward.
                    let node_id = frame.node_id;
                    // Clone adaptive model before mutable borrow of node_states
                    // to avoid unsafe raw pointer (review finding #2).
                    let adaptive_model_clone = s.adaptive_model.clone();
                    let d5_phase = s.d5_presence.phase();
                    let calibration_bundle = s.active_calibration_bundle.clone();

                    let ns = s.node_states.entry(node_id).or_insert_with(|| {
                        NodeState::new_with_calibration(node_id, calibration_bundle.as_deref())
                    });
                    let (features, mut classification) =
                        observe_frame_for_presence(ns, &frame, frame_now, d5_phase);
                    ns.observe_live_fusion_frame(&frame.amplitudes, host_time.host_monotonic_ns);

                    // Adaptive override using cloned model (safe, no raw pointers).
                    if let Some(ref model) = adaptive_model_clone {
                        let amps = ns.frame_history.back().map(|v| v.as_slice()).unwrap_or(&[]);
                        let feat_arr = adaptive_classifier::features_from_runtime(
                            &serde_json::json!({
                                "variance": features.variance,
                                "motion_band_power": features.motion_band_power,
                                "breathing_band_power": features.breathing_band_power,
                                "spectral_power": features.spectral_power,
                                "dominant_freq_hz": features.dominant_freq_hz,
                                "change_points": features.change_points,
                                "mean_rssi": features.mean_rssi,
                            }),
                            amps,
                        );
                        let (label, conf) = model.classify(&feat_arr);
                        classification.motion_level = label.to_string();
                        classification.presence = label != "absent";
                        classification.confidence =
                            (conf * 0.7 + classification.confidence * 0.3).clamp(0.0, 1.0);
                    }
                    ns.motion_confidence = classification.confidence;

                    ns.rssi_history.push_back(features.mean_rssi);
                    if ns.rssi_history.len() > 60 {
                        ns.rssi_history.pop_front();
                    }

                    let raw_vitals = ns
                        .vital_detector
                        .process_frame(&frame.amplitudes, &frame.phases);
                    let vitals = smooth_vitals_node(ns, &raw_vitals);
                    ns.latest_vitals = vitals.clone();

                    // DynamicMinCut person estimation from subcarrier correlation.
                    let corr_persons = estimate_persons_from_correlation(&ns.frame_history);
                    // #803: map the min-cut count onto a threshold-aligned score
                    // so it round-trips back to the same count. The old
                    // `corr_persons / 3.0` left 2 people at 0.667 — under the
                    // 0.70 up-threshold — so the count was pinned at 1.
                    let raw_score = corr_persons_to_score(corr_persons);
                    ns.smoothed_person_score = ns.smoothed_person_score * 0.92 + raw_score * 0.08;
                    if classification.presence {
                        let count =
                            score_to_person_count(ns.smoothed_person_score, ns.prev_person_count);
                        ns.prev_person_count = count;
                    } else {
                        ns.prev_person_count = 0;
                    }

                    // Store latest features on node for cross-node fusion.
                    ns.latest_features = Some(features.clone());

                    // Done with per-node mutable borrow; now read aggregated
                    // state from all nodes (the borrow of `ns` ends here).
                    // (We re-borrow node_states immutably via `s` below.)

                    s.rssi_history.push_back(features.mean_rssi);
                    if s.rssi_history.len() > 60 {
                        s.rssi_history.pop_front();
                    }
                    s.latest_vitals = vitals.clone();

                    // Cross-node fusion: combine features from all active nodes.
                    let fused_features = fuse_multi_node_features(&features, &s.node_states);
                    let now = frame_now;

                    // The room-level classification is a consensus across all
                    // live RX links, not the result of the last UDP packet.
                    let classification = {
                        let sref: &mut AppStateInner = &mut s;
                        aggregate_node_classification(&sref.node_states, now, &mut sref.d5_presence)
                    };
                    let classification = apply_position_setup_classification_gate(
                        s.position_setup.is_some(),
                        s.d5_presence.phase(),
                        classification,
                    );
                    let position_gate =
                        live_position_presence_gate(&s.node_states, now, &s.d5_presence);
                    let candidate_position_estimate = if live_position_input_accepted {
                        s.live_position_tracker
                            .tick(live_position_timestamp_ns, position_gate)
                    } else {
                        s.live_position_tracker.current().clone()
                    };
                    // A freshly generated mmWave-gated model remains usable
                    // inside the held-back evaluator but cannot become public
                    // before every blind-position gate passes.
                    let position_estimate = gate_mmwave_candidate_for_publication(
                        candidate_position_estimate,
                        s.mmwave.position_publication_allowed(),
                    );
                    let has_valid_position = classification.presence
                        && matches!(
                            &position_estimate,
                            position_live::LivePositionState::Position { .. }
                        );

                    s.tick += 1;
                    let tick = s.tick;

                    // Aggregate person count: gate on presence first (matching WiFi path).
                    let _total_persons = if classification.presence {
                        let dedup = s.dedup_factor;
                        let (fused, fallback_count) = multistatic_bridge::fuse_or_fallback(
                            &s.multistatic_fuser,
                            &s.node_states,
                            dedup,
                        );
                        match fused {
                            Some(ref f) => {
                                let score =
                                    multistatic_bridge::compute_person_score_from_amplitudes(
                                        &f.fused_amplitude,
                                    );
                                s.smoothed_person_score =
                                    s.smoothed_person_score * 0.90 + score * 0.10;
                                // #803: don't let the saturating activity score
                                // discard count-aware per-node estimates.
                                let count =
                                    aggregate_person_count(s.person_count(), &s.node_states);
                                s.prev_person_count = count;
                                count.max(1)
                            }
                            None => {
                                aggregate_person_count(fallback_count.unwrap_or(0), &s.node_states)
                                    .max(1)
                            }
                        }
                    } else {
                        s.prev_person_count = 0;
                        0
                    };

                    // Governed trust cycle (ADR-135..146): run the same live
                    // frames through the privacy/provenance/witness control
                    // plane. Trust state is recorded on the bridge (exposed on
                    // /api/v1/status); engine errors are counted + rate-limit
                    // logged instead of being swallowed (review finding 1).
                    // Split-borrow the two distinct fields off the guard.
                    {
                        let sref: &mut AppStateInner = &mut s;
                        let now_ms = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_millis() as i64)
                            .unwrap_or(0);
                        sref.engine_bridge.observe_cycle(&sref.node_states, now_ms);
                    }

                    // Feed field model calibration if active (use per-node history for ESP32).
                    if let Some(frame_history) = s
                        .node_states
                        .get(&node_id)
                        .map(|ns| ns.frame_history.clone())
                    {
                        if let Some(ref mut fm) = s.field_model {
                            field_bridge::maybe_feed_calibration(fm, &frame_history);
                        }
                    }

                    // Build nodes array with all active nodes. ADR-141 output
                    // gating (review finding 1c): when the governed engine
                    // emitted this cycle at class Restricted (base mode, or a
                    // contradiction/mesh-risk demotion below the configured
                    // class), the per-node raw amplitude vectors are suppressed
                    // from the live publish — the same field mapping bfld's
                    // privacy gate applies at Restricted (drop amplitude/phase
                    // proxies).
                    let suppress_raw = s.engine_bridge.suppress_raw_outputs();
                    let configured_positions = s.multistatic_fuser.node_positions().to_vec();
                    let mut active_nodes: Vec<NodeInfo> = s
                        .node_states
                        .iter()
                        .filter(|(_, n)| {
                            n.last_frame_time
                                .is_some_and(|t| now.duration_since(t).as_secs() < 10)
                        })
                        .map(|(&id, n)| NodeInfo {
                            node_id: id,
                            rssi_dbm: n.rssi_history.back().copied().unwrap_or(0.0),
                            position: configured_node_position(id, &configured_positions),
                            amplitude: if suppress_raw {
                                vec![]
                            } else {
                                n.frame_history
                                    .back()
                                    .map(|a| a.iter().take(56).cloned().collect())
                                    .unwrap_or_default()
                            },
                            subcarrier_count: if suppress_raw {
                                0
                            } else {
                                n.frame_history.back().map_or(0, |a| a.len())
                            },
                            // ADR-110 iter 23 / iter 30 — single source of truth.
                            sync: n.sync_snapshot(),
                        })
                        .collect();
                    active_nodes.sort_by_key(|node| node.node_id);
                    let localization = estimate_live_localization(
                        &s.node_states,
                        now,
                        &classification,
                        s.tx_position,
                        s.room_dimensions,
                        &configured_positions,
                    );
                    let signal_field = signal_field_from_localization(&localization);

                    let mut update = SensingUpdate {
                        msg_type: "sensing_update".to_string(),
                        timestamp: chrono::Utc::now().timestamp_millis() as f64 / 1000.0,
                        source: "esp32".to_string(),
                        tick,
                        tx_position: s.tx_position,
                        room_dimensions: s.room_dimensions,
                        nodes: active_nodes,
                        features: fused_features.clone(),
                        classification,
                        signal_field,
                        localization: Some(localization),
                        position_estimate: Some(position_estimate),
                        vital_signs: Some(vitals),
                        enhanced_motion: None,
                        enhanced_breathing: None,
                        posture: None,
                        signal_quality_score: None,
                        quality_verdict: None,
                        bssid_count: None,
                        pose_keypoints: None,
                        model_status: None,
                        persons: None,
                        estimated_persons: has_valid_position.then_some(1),
                        // ADR-084 Pass 3.6: surface per-node novelty_score
                        // (and the rest of the per-node feature snapshot)
                        // on the WebSocket envelope so cluster-Pi consumers
                        // can implement model-wake gating without round-
                        // tripping back to the server.
                        node_features: build_node_features(
                            &s.node_states,
                            now,
                            s.d5_presence.phase(),
                            s.position_setup.is_some(),
                        ),
                    };

                    let persons = derive_pose_from_sensing(&update);
                    s.pose_tracker = PoseTracker::new();
                    s.last_tracker_instant = None;
                    if !persons.is_empty() {
                        update.persons = Some(persons);
                    }
                    // ESP32 persons are exact discrete markers, never tracked
                    // synthetic skeletons or coarse signal-field peaks.
                    attach_field_positions(&mut update);

                    if let Ok(json) = serde_json::to_string(&update) {
                        let _ = s.tx.send(json);
                    }

                    // ── ADR-262 P3: emit a signed RuField FieldEvent ────────
                    // Join this cycle's SensingUpdate (features / classification
                    // / signal_field) with the governed engine's trust state
                    // (effective_class / demoted, recorded by `observe_cycle`
                    // above) into a `SensingSnapshot`, and surface it on
                    // `/api/field` + `/ws/field` via the P1 bridge. Only cycles
                    // whose mapped privacy class clears the §10 network egress
                    // gate are surfaced (P1/P2); a `Derived → P4/P5` cycle is
                    // held edge-local. `presence == false` ⇒ no phantom event.
                    emit_rufield_event(&s, &update, node_id);

                    s.latest_update = Some(update);

                    // Evict stale nodes every 100 ticks to prevent memory leak.
                    if tick % 100 == 0 {
                        let stale = Duration::from_secs(60);
                        let before = s.node_states.len();
                        s.node_states.retain(|_id, ns| {
                            ns.last_frame_time
                                .is_some_and(|t| now.duration_since(t) < stale)
                        });
                        let evicted = before - s.node_states.len();
                        if evicted > 0 {
                            info!(
                                "Evicted {} stale node(s), {} active",
                                evicted,
                                s.node_states.len()
                            );
                        }
                    }
                } else if has_esp32_csi_magic(&buf[..len]) {
                    // The CSI magic is authoritative even when the derived
                    // parser cannot safely construct a frame (for example a
                    // truncated I/Q payload). Treat it as rejected CSI rather
                    // than an unrelated UDP packet so an active capture cannot
                    // silently remain "complete". Only byte 4 is used as RX ID
                    // when it is actually present.
                    let rx_id = esp32_csi_header_rx_id(&buf[..len]);
                    let mut s = state.write().await;
                    reject_raw_csi_ingress(
                        &mut s,
                        rx_id,
                        "CSI datagram has valid magic but an invalid or truncated frame payload",
                    );
                    warn!("malformed ESP32 CSI datagram from {src} rejected before sensing");
                }
            }
            Err(e) => {
                warn!("UDP recv error: {e}");
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }
}

// ── Simulated data task ──────────────────────────────────────────────────────

pub(crate) async fn simulated_data_task(state: SharedState, tick_ms: u64) {
    let mut interval = tokio::time::interval(Duration::from_millis(tick_ms));
    info!("Simulated data source active (tick={}ms)", tick_ms);

    loop {
        interval.tick().await;

        let mut s = state.write().await;

        // Issue #1004: in `auto` mode this task runs alongside `udp_receiver_task`.
        // Once a real frame promotes `source` → "esp32", stop emitting synthetic
        // frames so we never clobber live CSI with simulated poses. (For an
        // explicit `--source simulated` demo, `source` stays "simulated" and the
        // simulator keeps running — that path never binds UDP, so it is never
        // promoted.) The task stays alive so it can resume serving if the real
        // source later ages out to "esp32:offline".
        if s.effective_source() == "esp32" {
            continue;
        }

        s.tick += 1;
        let tick = s.tick;

        let frame = generate_simulated_frame(tick);

        // Append current amplitudes to history before feature extraction.
        s.frame_history.push_back(frame.amplitudes.clone());
        if s.frame_history.len() > FRAME_HISTORY_CAPACITY {
            s.frame_history.pop_front();
        }

        let sample_rate_hz = 1000.0 / tick_ms as f64;
        let (features, mut classification, breathing_rate_hz, sub_variances, raw_motion) =
            extract_features_from_frame(&frame, &s.frame_history, sample_rate_hz);
        smooth_and_classify(&mut s, &mut classification, raw_motion);
        adaptive_override(&s, &features, &mut classification);

        s.rssi_history.push_back(features.mean_rssi);
        if s.rssi_history.len() > 60 {
            s.rssi_history.pop_front();
        }

        let motion_score = motion_score_for_level(&classification.motion_level);

        let raw_vitals = s
            .vital_detector
            .process_frame(&frame.amplitudes, &frame.phases);
        let vitals = smooth_vitals(&mut s, &raw_vitals);
        s.latest_vitals = vitals.clone();

        let frame_amplitudes = frame.amplitudes.clone();
        let frame_n_sub = frame.n_subcarriers;

        // ADR-044 §5.2: feed raw features into rolling-P95 estimators before scoring.
        s.p95_variance.push(features.variance);
        s.p95_motion_band_power.push(features.motion_band_power);
        s.p95_spectral_power.push(features.spectral_power);

        // Multi-person estimation with temporal smoothing (EMA α=0.10).
        let raw_score = compute_person_score(&s, &features);
        s.smoothed_person_score = s.smoothed_person_score * 0.90 + raw_score * 0.10;
        let est_persons = if classification.presence {
            let count = s.person_count();
            s.prev_person_count = count;
            count
        } else {
            s.prev_person_count = 0;
            0
        };

        let mut update = SensingUpdate {
            msg_type: "sensing_update".to_string(),
            timestamp: chrono::Utc::now().timestamp_millis() as f64 / 1000.0,
            source: "simulated".to_string(),
            tick,
            tx_position: s.tx_position,
            room_dimensions: s.room_dimensions,
            nodes: vec![NodeInfo {
                node_id: 1,
                rssi_dbm: features.mean_rssi,
                position: [2.0, 0.0, 1.5],
                amplitude: frame_amplitudes,
                subcarrier_count: frame_n_sub as usize,
                sync: None, // simulated frame path — no mesh peer
            }],
            features: features.clone(),
            classification,
            signal_field: generate_signal_field(
                features.mean_rssi,
                motion_score,
                breathing_rate_hz,
                features.variance.min(1.0),
                &sub_variances,
            ),
            localization: None,
            position_estimate: None,
            vital_signs: Some(vitals),
            enhanced_motion: None,
            enhanced_breathing: None,
            posture: None,
            signal_quality_score: None,
            quality_verdict: None,
            bssid_count: None,
            pose_keypoints: None,
            model_status: if s.model_loaded {
                Some(serde_json::json!({
                    "loaded": true,
                    "layers": s.progressive_loader.as_ref()
                        .map(|l| { let (a,b,c) = l.layer_status(); a as u8 + b as u8 + c as u8 })
                        .unwrap_or(0),
                    "sona_profile": s.active_sona_profile.as_deref().unwrap_or("default"),
                }))
            } else {
                None
            },
            persons: None,
            estimated_persons: if est_persons > 0 {
                Some(est_persons)
            } else {
                None
            },
            node_features: None,
        };

        // Populate persons from the sensing update (Kalman-smoothed via tracker).
        let raw_persons = derive_pose_from_sensing(&update);
        let mut last_tracker_instant = s.last_tracker_instant.take();
        let tracked = tracker_bridge::tracker_update(
            &mut s.pose_tracker,
            &mut last_tracker_instant,
            raw_persons,
        );
        s.last_tracker_instant = last_tracker_instant;
        if !tracked.is_empty() {
            update.persons = Some(tracked);
        }
        // #1050: attach real signal_field-peak positions to each person.
        attach_field_positions(&mut update);

        if update.classification.presence {
            s.total_detections += 1;
        }
        if let Ok(json) = serde_json::to_string(&update) {
            let _ = s.tx.send(json);
        }
        s.latest_update = Some(update);
    }
}

// ── Broadcast tick task (for ESP32 mode, sends buffered state) ───────────────

pub(crate) async fn broadcast_tick_task(state: SharedState, tick_ms: u64) {
    let mut interval = tokio::time::interval(Duration::from_millis(tick_ms));

    loop {
        interval.tick().await;
        let mut s = state.write().await;
        if s.latest_update
            .as_ref()
            .is_some_and(|update| update.source == "esp32")
            && position_raw_input_is_stale(s.last_raw_csi_frame, std::time::Instant::now())
        {
            let estimate = match raw_csi_recording::now_unix_ns() {
                Ok(now_unix_ns) => s.live_position_tracker.expire_if_raw_stale(now_unix_ns),
                Err(error) => s.live_position_tracker.reject_input(format!(
                    "could not verify raw CSI freshness before rebroadcast: {error}"
                )),
            };
            apply_latest_esp32_position_estimate(&mut s, estimate);
        }
        if let Some(ref update) = s.latest_update {
            if s.tx.receiver_count() > 0 {
                // Re-broadcast the latest sensing_update so pose WS clients
                // always get data even when ESP32 pauses between frames.
                //
                // Tag every rebroadcast with `effective_source()`. When an
                // ESP32 stream is offline, `public_sensing_update` also clears
                // stale detections, fields, and vital estimates so a frozen
                // position is never presented as current evidence.
                let effective_source = s.effective_source();
                let tagged = public_sensing_update(update, &effective_source);
                if let Ok(json) = serde_json::to_string(&tagged) {
                    let _ = s.tx.send(json);
                }
            }
        }
    }
}

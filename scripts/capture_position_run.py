#!/usr/bin/env python3
"""Run one setup-bound, lossless fixed-room CSI capture.

The request body deliberately contains only the recording ID. Point labels and
blind truth stay outside the raw capture so the offline protocol can verify
that prediction never received the answer.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
import time
import urllib.error
import urllib.request
from dataclasses import dataclass
from typing import Any


EXPECTED_RX_IDS = {1, 2, 3, 4}
MINIMUM_HZ_PER_RX = 5
MAXIMUM_BINDING_AGE_MS = 2_000
WATCHDOG_GRACE_SECONDS = 15
CALIBRATION_READY_TIMEOUT_SECONDS = 12
CALIBRATION_STATUS_POLL_SECONDS = 0.25
SHA256_PATTERN = re.compile(r"^[0-9a-f]{64}$")
SETUP_ID_PATTERN = re.compile(r"^setup-[0-9a-f]{16}$")

CALIBRATION_START_PATH = "/api/v1/classification/calibration/start"
CALIBRATION_STOP_PATH = "/api/v1/classification/calibration/stop"
CALIBRATION_STATUS_PATH = "/api/v1/classification/calibration/status"


@dataclass(frozen=True)
class CaptureProtocol:
    duration_seconds: int
    description: str
    setup_required: bool


PROTOCOLS = {
    "discovery": CaptureProtocol(
        duration_seconds=25,
        description="25-second unsealed RX/grid discovery",
        setup_required=False,
    ),
    "preflight": CaptureProtocol(
        duration_seconds=25,
        description="25-second sealed four-RX grid and throughput preflight",
        setup_required=True,
    ),
    "empty": CaptureProtocol(
        duration_seconds=65,
        description="65-second confirmed empty-room calibration capture",
        setup_required=True,
    ),
    "position": CaptureProtocol(
        duration_seconds=35,
        description="35-second unlabelled fixed-position capture",
        setup_required=True,
    ),
}


class CaptureError(RuntimeError):
    """A fail-closed protocol or server response error."""


def request_json(
    server: str,
    path: str,
    *,
    method: str = "GET",
    body: dict[str, Any] | None = None,
    timeout_seconds: float = 10.0,
) -> dict[str, Any]:
    url = f"{server.rstrip('/')}{path}"
    encoded = None if body is None else json.dumps(body).encode("utf-8")
    request = urllib.request.Request(
        url,
        data=encoded,
        method=method,
        headers={"Content-Type": "application/json"} if encoded is not None else {},
    )
    try:
        with urllib.request.urlopen(request, timeout=timeout_seconds) as response:
            payload = json.load(response)
    except (urllib.error.URLError, TimeoutError, json.JSONDecodeError) as error:
        raise CaptureError(f"{method} {path} failed: {error}") from error
    if not isinstance(payload, dict):
        raise CaptureError(f"{method} {path} returned a non-object JSON response")
    return payload


def validate_live_preflight(
    readiness: dict[str, Any],
    nodes_payload: dict[str, Any],
    *,
    setup_required: bool = True,
) -> tuple[str | None, str | None]:
    if readiness.get("status") != "ready":
        raise CaptureError("server /health/ready is not ready")
    if readiness.get("source") != "esp32":
        raise CaptureError(
            f"fresh ESP32 source required, got {readiness.get('source')!r}"
        )

    setup = readiness.get("position_setup")
    setup_active = isinstance(setup, dict) and setup.get("active") is True
    if setup_required and not setup_active:
        raise CaptureError("no sealed position setup is active")
    if not setup_required and setup_active:
        raise CaptureError(
            "discovery must run before setup sealing on a server without --position-setup"
        )
    if setup_active:
        setup_id = setup.get("setup_id")
        setup_sha256 = setup.get("setup_sha256")
        if not isinstance(setup_id, str) or not SETUP_ID_PATTERN.fullmatch(setup_id):
            raise CaptureError("active setup has an invalid setup_id")
        if (
            not isinstance(setup_sha256, str)
            or not SHA256_PATTERN.fullmatch(setup_sha256)
        ):
            raise CaptureError("active setup has an invalid setup_sha256")
    else:
        setup_id = None
        setup_sha256 = None

    nodes = nodes_payload.get("nodes")
    if not isinstance(nodes, list):
        raise CaptureError("/api/v1/nodes did not return a node list")
    active_ids = {
        node.get("node_id")
        for node in nodes
        if isinstance(node, dict)
        and node.get("status") == "active"
        and isinstance(node.get("node_id"), int)
    }
    if active_ids != EXPECTED_RX_IDS:
        raise CaptureError(
            f"fresh receivers must be exactly RX1-RX4, got {sorted(active_ids)}"
        )
    if (
        not setup_required
        and nodes_payload.get("source_binding_consistent_across_nodes") is not True
    ):
        raise CaptureError(
            "discovery requires fresh complete 0x07 TX bindings with one "
            "consistent private TX identity across RX1-RX4"
        )
    if setup_required:
        active_nodes = {
            node["node_id"]: node
            for node in nodes
            if isinstance(node, dict) and node.get("node_id") in EXPECTED_RX_IDS
        }
        required_attestations = (
            "source_binding_attested",
            "filter_enforced",
            "source_matched_filter",
            "identity_valid",
            "identity_matches_setup",
        )
        for rx_id in sorted(EXPECTED_RX_IDS):
            node = active_nodes[rx_id]
            missing = [
                field for field in required_attestations if node.get(field) is not True
            ]
            binding_age_ms = node.get("binding_last_seen_ms")
            if (
                missing
                or not isinstance(binding_age_ms, int)
                or isinstance(binding_age_ms, bool)
                or binding_age_ms < 0
                or binding_age_ms > MAXIMUM_BINDING_AGE_MS
            ):
                raise CaptureError(
                    f"RX{rx_id} has no fresh matching runtime TX binding"
                )
    return setup_id, setup_sha256


def calibration_nodes(
    payload: dict[str, Any], context: str
) -> list[dict[str, Any]]:
    nodes_payload = payload.get("nodes")
    if not isinstance(nodes_payload, list):
        raise CaptureError(f"{context} has no node list")
    by_id = {
        node.get("node_id"): node
        for node in nodes_payload
        if isinstance(node, dict)
        and isinstance(node.get("node_id"), int)
        and not isinstance(node.get("node_id"), bool)
    }
    if set(by_id) != EXPECTED_RX_IDS or len(nodes_payload) != len(EXPECTED_RX_IDS):
        raise CaptureError(
            f"{context} must report exactly RX1-RX4, got {sorted(by_id)}"
        )
    return [by_id[rx_id] for rx_id in sorted(EXPECTED_RX_IDS)]


def validate_calibration_collecting(payload: dict[str, Any]) -> None:
    if (
        payload.get("success") is not True
        or payload.get("phase") != "collecting"
        or payload.get("decision_status") != "calibrating"
        or payload.get("position_setup_active") is not True
    ):
        raise CaptureError("D5/D6 calibration did not enter the collecting phase")
    for node in calibration_nodes(payload, "collecting calibration status"):
        if node.get("fresh") is not True or not isinstance(node.get("d6"), dict):
            raise CaptureError(
                f"RX{node['node_id']} is not fresh for D5/D6 calibration"
            )


def validate_calibration_stop(payload: dict[str, Any]) -> None:
    if payload.get("success") is not True or payload.get("status") != "ready":
        raise CaptureError(
            f"D5/D6 calibration stop failed: {payload.get('error')!r}"
        )
    if payload.get("ready_nodes") != len(EXPECTED_RX_IDS):
        raise CaptureError(
            "D5/D6 calibration did not produce references for exactly RX1-RX4"
        )
    for node in calibration_nodes(payload, "calibration stop response"):
        if (
            node.get("ready") is not True
            or node.get("d5_ready") is not True
            or node.get("d6_ready") is not True
            or node.get("d5_error") is not None
            or node.get("d6_error") is not None
        ):
            raise CaptureError(
                f"RX{node['node_id']} has no complete D5/D6 calibration reference"
            )


def validate_calibration_ready(payload: dict[str, Any]) -> None:
    if (
        payload.get("success") is not True
        or payload.get("phase") != "ready"
        or payload.get("decision_status") != "operational"
        or payload.get("position_setup_active") is not True
        or payload.get("operational") is not True
        or payload.get("fresh_reference_nodes") != len(EXPECTED_RX_IDS)
        or payload.get("usable_live_nodes") != len(EXPECTED_RX_IDS)
    ):
        raise CaptureError("D5/D6 calibration is not operational for exactly RX1-RX4")
    for node in calibration_nodes(payload, "ready calibration status"):
        d5 = node.get("d5")
        d6 = node.get("d6")
        if (
            node.get("fresh") is not True
            or not isinstance(d5, dict)
            or d5.get("reference_ready") is not True
            or not isinstance(d6, dict)
            or d6.get("reference_ready") is not True
            or d6.get("observation_fresh") is not True
            or d6.get("evidence_ready") is not True
        ):
            raise CaptureError(
                f"RX{node['node_id']} has no fresh operational D5/D6 evidence"
            )


def start_empty_room_calibration(server: str) -> float:
    initial = request_json(server, CALIBRATION_STATUS_PATH)
    if initial.get("phase") == "collecting":
        raise CaptureError(
            "a D5/D6 calibration is already collecting; restart the server or finish it first"
        )

    started_at = time.monotonic()
    try:
        response = request_json(server, CALIBRATION_START_PATH, method="POST", body={})
    except CaptureError:
        # A lost HTTP response does not prove that the server rejected the
        # request. Treat an authoritative collecting status as a successful,
        # recoverable start so calibration is never abandoned in that phase.
        status = request_json(server, CALIBRATION_STATUS_PATH)
        if status.get("phase") != "collecting":
            raise
        validate_calibration_collecting(status)
        return started_at
    try:
        if response.get("success") is not True or response.get("status") != "collecting":
            raise CaptureError(
                f"D5/D6 calibration start failed: {response.get('error')!r}"
            )
        recommended_seconds = response.get("recommended_seconds")
        if (
            not isinstance(recommended_seconds, int)
            or isinstance(recommended_seconds, bool)
            or recommended_seconds <= 0
            or recommended_seconds > PROTOCOLS["empty"].duration_seconds
        ):
            raise CaptureError(
                "D5/D6 calibration returned an invalid recommended duration"
            )
        validate_calibration_collecting(
            request_json(server, CALIBRATION_STATUS_PATH)
        )
    except BaseException:
        recovery_status = request_json(server, CALIBRATION_STATUS_PATH)
        if recovery_status.get("phase") == "collecting":
            finish_empty_room_calibration(server, started_at)
        raise
    return started_at


def wait_until_calibration_ready(server: str) -> dict[str, Any]:
    deadline = time.monotonic() + CALIBRATION_READY_TIMEOUT_SECONDS
    last_status: dict[str, Any] | None = None
    while True:
        last_status = request_json(server, CALIBRATION_STATUS_PATH)
        if (
            last_status.get("phase") == "ready"
            and last_status.get("operational") is True
            and last_status.get("usable_live_nodes") == len(EXPECTED_RX_IDS)
        ):
            validate_calibration_ready(last_status)
            return last_status
        if time.monotonic() >= deadline:
            raise CaptureError(
                "D5/D6 calibration references were installed, but exactly RX1-RX4 "
                "did not become operational before the timeout"
            )
        time.sleep(CALIBRATION_STATUS_POLL_SECONDS)


def finish_empty_room_calibration(server: str, started_at: float) -> None:
    status = request_json(server, CALIBRATION_STATUS_PATH)
    if status.get("phase") == "ready":
        wait_until_calibration_ready(server)
        return
    validate_calibration_collecting(status)

    remaining = PROTOCOLS["empty"].duration_seconds - (
        time.monotonic() - started_at
    )
    if remaining > 0:
        print(
            "\nThe recording failed early. Keep the room empty while the active "
            "D5/D6 calibration is completed safely.",
            file=sys.stderr,
        )
        wait_for_capture(remaining)

    try:
        response = request_json(
            server,
            CALIBRATION_STOP_PATH,
            method="POST",
            body={},
            timeout_seconds=60.0,
        )
        validate_calibration_stop(response)
    except CaptureError as first_error:
        # A lost/malformed stop response is resolved from authoritative status.
        # If the server is still collecting, retry once; otherwise accept only a
        # fully validated ready state.
        recovery_status = request_json(server, CALIBRATION_STATUS_PATH)
        if recovery_status.get("phase") == "ready":
            wait_until_calibration_ready(server)
            return
        if recovery_status.get("phase") != "collecting":
            raise first_error
        validate_calibration_collecting(recovery_status)
        response = request_json(
            server,
            CALIBRATION_STOP_PATH,
            method="POST",
            body={},
            timeout_seconds=60.0,
        )
        validate_calibration_stop(response)
    wait_until_calibration_ready(server)


def validate_rx_summaries(
    summaries: Any, duration_seconds: int
) -> list[dict[str, Any]]:
    if not isinstance(summaries, list):
        raise CaptureError("recording has no per-RX summaries")
    by_id = {
        summary.get("rx_id"): summary
        for summary in summaries
        if isinstance(summary, dict) and isinstance(summary.get("rx_id"), int)
    }
    if set(by_id) != EXPECTED_RX_IDS or len(summaries) != len(EXPECTED_RX_IDS):
        raise CaptureError(
            f"recording summaries must be exactly RX1-RX4, got {sorted(by_id)}"
        )

    minimum_frames = duration_seconds * MINIMUM_HZ_PER_RX
    minimum_span_ns = max(0, duration_seconds - 1) * 1_000_000_000
    for rx_id in sorted(EXPECTED_RX_IDS):
        summary = by_id[rx_id]
        frames = summary.get("frames_written")
        first_ns = summary.get("first_host_timestamp_unix_ns")
        last_ns = summary.get("last_host_timestamp_unix_ns")
        grid = summary.get("grid")
        if not isinstance(frames, int) or frames < minimum_frames:
            raise CaptureError(
                f"RX{rx_id} contains {frames!r} frames; at least {minimum_frames} required"
            )
        if (
            not isinstance(first_ns, int)
            or not isinstance(last_ns, int)
            or last_ns < first_ns
            or last_ns - first_ns < minimum_span_ns
        ):
            raise CaptureError(
                f"RX{rx_id} does not cover the required capture duration"
            )
        span_seconds = (last_ns - first_ns) / 1_000_000_000
        rate_hz = (frames - 1) / span_seconds if span_seconds > 0 else 0.0
        if rate_hz < MINIMUM_HZ_PER_RX:
            raise CaptureError(
                f"RX{rx_id} rate is {rate_hz:.3f} Hz; "
                f"at least {MINIMUM_HZ_PER_RX} Hz required"
            )
        if (
            not isinstance(grid, dict)
            or not isinstance(grid.get("center_frequency_mhz"), int)
            or not isinstance(grid.get("antenna_count"), int)
            or not isinstance(grid.get("subcarrier_count"), int)
            or not isinstance(grid.get("ppdu_type"), int)
            or not isinstance(grid.get("layout_flags"), int)
        ):
            raise CaptureError(f"RX{rx_id} has no complete stable grid identity")
    return [by_id[rx_id] for rx_id in sorted(EXPECTED_RX_IDS)]


def validate_stop_response(
    payload: dict[str, Any], recording_id: str, duration_seconds: int
) -> tuple[int, list[dict[str, Any]]]:
    if payload.get("recording_id") != recording_id:
        raise CaptureError("stop response recording_id does not match the requested run")
    if payload.get("success") is not True or payload.get("incomplete") is not False:
        raise CaptureError(
            f"recording finished incomplete: {payload.get('writer_error')!r}"
        )
    if payload.get("dropped_frames") != 0:
        raise CaptureError(
            f"recording dropped {payload.get('dropped_frames')!r} raw frames"
        )
    frames = payload.get("frames_written")
    summaries = validate_rx_summaries(payload.get("rx_summaries"), duration_seconds)
    per_rx_total = sum(summary["frames_written"] for summary in summaries)
    if not isinstance(frames, int) or frames != per_rx_total:
        raise CaptureError(
            f"recording total {frames!r} does not match per-RX total {per_rx_total}"
        )
    return frames, summaries


def validate_recording_listing(
    payload: dict[str, Any],
    recording_id: str,
    setup_id: str | None,
    setup_sha256: str | None,
    frames_written: int,
    duration_seconds: int,
) -> None:
    recordings = payload.get("recordings")
    if not isinstance(recordings, list):
        raise CaptureError("recording list response has no recordings array")
    recording = next(
        (
            item
            for item in recordings
            if isinstance(item, dict) and item.get("id") == recording_id
        ),
        None,
    )
    if recording is None:
        raise CaptureError("completed recording is missing from the recording list")
    if (
        recording.get("status") != "completed"
        or recording.get("incomplete") is not False
        or recording.get("dropped_frames") != 0
        or recording.get("integrity_error") is not None
        or recording.get("format") != "raw-csi-v1-jsonl"
        or recording.get("capture_scope") != "validated_udp_csi_all_grids"
    ):
        raise CaptureError("recording sidecar does not report a clean completion")
    if (
        recording.get("setup_id") != setup_id
        or recording.get("setup_sha256") != setup_sha256
    ):
        raise CaptureError("recording sidecar is not bound to the active setup")
    if (
        recording.get("frames") != frames_written
        or recording.get("frame_count") != frames_written
        or recording.get("frames_written") != frames_written
    ):
        raise CaptureError("persisted raw/sidecar frame counts do not match stop result")
    if not isinstance(recording.get("duration_secs"), int) or recording[
        "duration_secs"
    ] < duration_seconds - 1:
        raise CaptureError("persisted sidecar duration is shorter than the protocol")
    validate_rx_summaries(recording.get("rx_summaries"), duration_seconds)


def find_recording(payload: dict[str, Any], recording_id: str) -> dict[str, Any] | None:
    recordings = payload.get("recordings")
    if not isinstance(recordings, list):
        return None
    return next(
        (
            recording
            for recording in recordings
            if isinstance(recording, dict) and recording.get("id") == recording_id
        ),
        None,
    )


def recover_uncertain_start(server: str, recording_id: str) -> None:
    """Stop this ID if the start may have succeeded but its response was lost."""
    try:
        listing = request_json(server, "/api/v1/recording/list")
        recording = find_recording(listing, recording_id)
        if recording is not None and recording.get("status") == "recording":
            request_json(
                server,
                "/api/v1/recording/stop",
                method="POST",
                body={},
                timeout_seconds=60.0,
            )
    except CaptureError:
        return


def wait_for_capture(duration_seconds: int) -> None:
    deadline = time.monotonic() + duration_seconds
    while True:
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            return
        print(f"\rRecording: {max(0, int(remaining + 0.999))} s remaining", end="", flush=True)
        time.sleep(min(1.0, remaining))


def completion_message(kind: str, recording_id: str, frames: int) -> str:
    if kind == "discovery":
        return (
            f"DISCOVERY COMPLETE (NOT MEASUREMENT-READY): {recording_id} "
            f"contains {frames} inventory frames with 0 drops. "
            "Seal the verified setup and pass the binding-aware preflight before any measurement."
        )
    return (
        f"PASS: {recording_id} completed with {frames} frames, "
        "0 drops, and matching setup identity."
    )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--server",
        default="http://localhost:8080",
        help="sensing-server base URL (default: http://localhost:8080)",
    )
    parser.add_argument("--kind", choices=sorted(PROTOCOLS), required=True)
    parser.add_argument("--recording-id", required=True)
    parser.add_argument(
        "--confirm-empty-room",
        action="store_true",
        help="required for --kind empty; confirms nobody will enter during capture",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    protocol = PROTOCOLS[args.kind]
    if args.kind == "empty" and not args.confirm_empty_room:
        raise CaptureError(
            "--kind empty requires --confirm-empty-room; any room entry invalidates the run"
        )

    print(f"Test: {protocol.description}")
    print(
        "Preflight: source, setup phase, fresh RX1-RX4, consistent TX binding, "
        "and lossless recorder"
    )
    readiness = request_json(args.server, "/health/ready")
    nodes = request_json(args.server, "/api/v1/nodes")
    setup_id, setup_sha256 = validate_live_preflight(
        readiness,
        nodes,
        setup_required=protocol.setup_required,
    )
    existing = find_recording(
        request_json(args.server, "/api/v1/recording/list"),
        args.recording_id,
    )
    if existing is not None:
        raise CaptureError("recording ID already exists; choose a new neutral ID")

    calibration_started_at = (
        start_empty_room_calibration(args.server) if args.kind == "empty" else None
    )
    started = False
    stop: dict[str, Any] | None = None
    primary_error: BaseException | None = None
    cleanup_errors: list[CaptureError] = []
    try:
        try:
            response = request_json(
                args.server,
                "/api/v1/recording/start",
                method="POST",
                body={
                    "id": args.recording_id,
                    "max_duration_seconds": (
                        protocol.duration_seconds + WATCHDOG_GRACE_SECONDS
                    ),
                },
            )
        except CaptureError:
            recover_uncertain_start(args.server, args.recording_id)
            raise
        if response.get("success") is not True:
            raise CaptureError(f"recording start rejected: {response.get('error')!r}")
        started = True
        if response.get("recording_id") != args.recording_id:
            raise CaptureError("recording start returned a different recording_id")
        if response.get("max_duration_seconds") != (
            protocol.duration_seconds + WATCHDOG_GRACE_SECONDS
        ):
            raise CaptureError("recording start did not arm the server-side watchdog")
        wait_for_capture(protocol.duration_seconds)
    except BaseException as error:
        primary_error = error
        if isinstance(error, KeyboardInterrupt):
            print("\nInterrupted; finalizing the partial recording.", file=sys.stderr)
    finally:
        if started:
            print("\nFinalizing lossless raw recording...")
            try:
                stop = request_json(
                    args.server,
                    "/api/v1/recording/stop",
                    method="POST",
                    body={},
                    timeout_seconds=60.0,
                )
            except CaptureError as error:
                cleanup_errors.append(error)
        if calibration_started_at is not None:
            try:
                finish_empty_room_calibration(args.server, calibration_started_at)
            except CaptureError as error:
                cleanup_errors.append(error)

    if primary_error is not None:
        if cleanup_errors:
            cleanup = "; ".join(str(error) for error in cleanup_errors)
            if isinstance(primary_error, KeyboardInterrupt):
                print(f"Cleanup also failed: {cleanup}", file=sys.stderr)
                raise primary_error
            raise CaptureError(
                f"capture failed: {primary_error}; cleanup also failed: {cleanup}"
            ) from primary_error
        raise primary_error
    if cleanup_errors:
        cleanup = "; ".join(str(error) for error in cleanup_errors)
        raise CaptureError(f"capture cleanup failed: {cleanup}")
    if stop is None:
        raise CaptureError("recording ended without a stop response")

    frames, summaries = validate_stop_response(
        stop, args.recording_id, protocol.duration_seconds
    )
    listing = request_json(args.server, "/api/v1/recording/list")
    validate_recording_listing(
        listing,
        args.recording_id,
        setup_id,
        setup_sha256,
        frames,
        protocol.duration_seconds,
    )
    print(completion_message(args.kind, args.recording_id, frames))
    if args.kind == "discovery":
        for summary in summaries:
            print(f"RX{summary['rx_id']} grid: {json.dumps(summary['grid'], sort_keys=True)}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except CaptureError as error:
        print(f"FAIL: {error}", file=sys.stderr)
        raise SystemExit(1)

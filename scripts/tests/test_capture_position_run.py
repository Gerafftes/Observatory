"""Contract tests for the fixed-room lossless capture runner."""

from __future__ import annotations

import os
import sys
import unittest
from argparse import Namespace
from unittest import mock


HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(os.path.dirname(HERE)))

from scripts import capture_position_run as capture  # noqa: E402


SETUP_ID = "setup-0123456789abcdef"
SETUP_SHA256 = "a" * 64


def readiness(source: str = "esp32", active: bool = True):
    return {
        "status": "ready",
        "source": source,
        "position_setup": {
            "active": active,
            "setup_id": SETUP_ID if active else None,
            "setup_sha256": SETUP_SHA256 if active else None,
        },
    }


def nodes(*active_ids: int, attested: bool = True):
    return {
        "source_binding_consistent_across_nodes": attested,
        "nodes": [
            {
                "node_id": node_id,
                "status": "active",
                "source_binding_attested": attested,
                "filter_enforced": attested,
                "source_matched_filter": attested,
                "identity_valid": attested,
                "identity_matches_setup": attested,
                "binding_last_seen_ms": 100,
            }
            for node_id in active_ids
        ]
    }


def rx_summaries(duration: int = 35, frames_per_rx: int | None = None):
    frames_per_rx = frames_per_rx or duration * 5
    return [
        {
            "rx_id": rx_id,
            "frames_written": frames_per_rx,
            "first_host_timestamp_unix_ns": 1_000_000_000,
            "last_host_timestamp_unix_ns": duration * 1_000_000_000,
            "grid": {
                "center_frequency_mhz": 2437,
                "antenna_count": 1,
                "subcarrier_count": 64,
                "ppdu_type": 0,
                "layout_flags": 0,
            },
        }
        for rx_id in range(1, 5)
    ]


def calibration_nodes(phase: str):
    ready = phase == "ready"
    return [
        {
            "node_id": rx_id,
            "fresh": True,
            "frame_rate_hz": 20.0,
            "d5": {
                "reference_ready": ready,
                "observation_fresh": True,
                "evidence_ready": ready,
            },
            "d6": {
                "reference_ready": ready,
                "observation_fresh": True,
                "evidence_ready": ready,
            },
        }
        for rx_id in range(1, 5)
    ]


def calibration_status(phase: str):
    ready = phase == "ready"
    return {
        "success": True,
        "phase": phase,
        "decision_status": "operational" if ready else "calibrating",
        "position_setup_active": True,
        "fresh_reference_nodes": 4 if ready else 0,
        "usable_live_nodes": 4 if ready else 0,
        "operational": ready,
        "nodes": calibration_nodes(phase),
    }


def calibration_stop_response():
    return {
        "success": True,
        "status": "ready",
        "ready_nodes": 4,
        "nodes": [
            {
                "node_id": rx_id,
                "ready": True,
                "d5_ready": True,
                "d5_error": None,
                "d6_ready": True,
                "d6_error": None,
            }
            for rx_id in range(1, 5)
        ],
    }


def recording_stop_response(recording_id: str, duration: int):
    summaries = rx_summaries(duration)
    return {
        "success": True,
        "recording_id": recording_id,
        "frames_written": sum(item["frames_written"] for item in summaries),
        "dropped_frames": 0,
        "incomplete": False,
        "writer_error": None,
        "rx_summaries": summaries,
    }


def recording_listing(recording_id: str, duration: int):
    stopped = recording_stop_response(recording_id, duration)
    frames = stopped["frames_written"]
    return {
        "recordings": [
            {
                "id": recording_id,
                "status": "completed",
                "incomplete": False,
                "dropped_frames": 0,
                "integrity_error": None,
                "format": "raw-csi-v1-jsonl",
                "capture_scope": "validated_udp_csi_all_grids",
                "setup_id": SETUP_ID,
                "setup_sha256": SETUP_SHA256,
                "frames": frames,
                "frame_count": frames,
                "frames_written": frames,
                "duration_secs": duration,
                "rx_summaries": stopped["rx_summaries"],
            }
        ]
    }


class TestPreflight(unittest.TestCase):
    def test_exact_fresh_four_rx_setup_passes(self):
        self.assertEqual(
            capture.validate_live_preflight(readiness(), nodes(1, 2, 3, 4)),
            (SETUP_ID, SETUP_SHA256),
        )

    def test_simulation_offline_or_missing_rx_fails_closed(self):
        cases = [
            (readiness("simulated"), nodes(1, 2, 3, 4)),
            (readiness("esp32:offline"), nodes(1, 2, 3, 4)),
            (readiness(active=False), nodes(1, 2, 3, 4)),
            (readiness(), nodes(1, 2, 3)),
            (readiness(), nodes(1, 2, 3, 4, 5)),
        ]
        for ready, node_payload in cases:
            with self.subTest(ready=ready, nodes=node_payload):
                with self.assertRaises(capture.CaptureError):
                    capture.validate_live_preflight(ready, node_payload)

    def test_discovery_requires_no_setup_but_still_requires_four_rx(self):
        no_setup = readiness(active=False)
        self.assertEqual(
            capture.validate_live_preflight(
                no_setup,
                nodes(1, 2, 3, 4),
                setup_required=False,
            ),
            (None, None),
        )
        with self.assertRaises(capture.CaptureError):
            capture.validate_live_preflight(
                readiness(),
                nodes(1, 2, 3, 4),
                setup_required=False,
            )

        inconsistent = nodes(1, 2, 3, 4)
        inconsistent["source_binding_consistent_across_nodes"] = False
        with self.assertRaises(capture.CaptureError):
            capture.validate_live_preflight(
                no_setup,
                inconsistent,
                setup_required=False,
            )

    def test_sealed_preflight_requires_fresh_runtime_binding_from_every_rx(self):
        with self.assertRaises(capture.CaptureError):
            capture.validate_live_preflight(
                readiness(),
                nodes(1, 2, 3, 4, attested=False),
            )

        stale = nodes(1, 2, 3, 4)
        stale["nodes"][2]["binding_last_seen_ms"] = 2_001
        with self.assertRaises(capture.CaptureError):
            capture.validate_live_preflight(readiness(), stale)


class TestEmptyRoomCalibration(unittest.TestCase):
    def test_collecting_stop_and_ready_require_exactly_rx1_through_rx4(self):
        capture.validate_calibration_collecting(calibration_status("collecting"))
        capture.validate_calibration_stop(calibration_stop_response())
        capture.validate_calibration_ready(calibration_status("ready"))

        invalid_payloads = []
        for payload in (
            calibration_status("collecting"),
            calibration_stop_response(),
            calibration_status("ready"),
        ):
            payload["nodes"] = payload["nodes"][:3]
            invalid_payloads.append(payload)
        validators = (
            capture.validate_calibration_collecting,
            capture.validate_calibration_stop,
            capture.validate_calibration_ready,
        )
        for validator, payload in zip(validators, invalid_payloads, strict=True):
            with self.subTest(validator=validator.__name__):
                with self.assertRaises(capture.CaptureError):
                    validator(payload)

    def test_ready_requires_fresh_d5_reference_and_d6_evidence_from_every_rx(self):
        cases = []
        missing_d5 = calibration_status("ready")
        missing_d5["nodes"][0]["d5"]["reference_ready"] = False
        cases.append(missing_d5)
        stale_d6 = calibration_status("ready")
        stale_d6["nodes"][1]["d6"]["observation_fresh"] = False
        cases.append(stale_d6)
        no_d6_evidence = calibration_status("ready")
        no_d6_evidence["nodes"][2]["d6"]["evidence_ready"] = False
        cases.append(no_d6_evidence)
        for payload in cases:
            with self.subTest(payload=payload):
                with self.assertRaises(capture.CaptureError):
                    capture.validate_calibration_ready(payload)

    def test_lost_start_response_recovers_from_authoritative_collecting_status(self):
        responses = [
            {"success": True, "phase": "uncalibrated"},
            capture.CaptureError("connection reset"),
            calibration_status("collecting"),
        ]

        def request(*_args, **_kwargs):
            response = responses.pop(0)
            if isinstance(response, BaseException):
                raise response
            return response

        with (
            mock.patch.object(capture, "request_json", side_effect=request),
            mock.patch.object(capture.time, "monotonic", return_value=123.0),
        ):
            self.assertEqual(capture.start_empty_room_calibration("http://server"), 123.0)
        self.assertEqual(responses, [])

    def test_lost_stop_response_uses_authoritative_ready_status(self):
        responses = [
            calibration_status("collecting"),
            capture.CaptureError("connection reset"),
            calibration_status("ready"),
            calibration_status("ready"),
        ]

        def request(*_args, **_kwargs):
            response = responses.pop(0)
            if isinstance(response, BaseException):
                raise response
            return response

        with (
            mock.patch.object(capture, "request_json", side_effect=request),
            mock.patch.object(capture.time, "monotonic", side_effect=[65.0, 65.0]),
        ):
            capture.finish_empty_room_calibration("http://server", 0.0)
        self.assertEqual(responses, [])


class TestCompletion(unittest.TestCase):
    def test_discovery_is_never_reported_as_measurement_ready(self):
        message = capture.completion_message("discovery", "inventory-01", 500)
        self.assertIn("NOT MEASUREMENT-READY", message)
        self.assertNotIn("PASS:", message)
        self.assertNotIn("matching setup identity", message)

        sealed = capture.completion_message("preflight", "preflight-01", 500)
        self.assertIn("PASS:", sealed)
        self.assertIn("matching setup identity", sealed)

    def test_clean_minimum_rate_completion_passes(self):
        duration = 35
        frames = duration * 5 * 4
        completed_frames, summaries = (
            capture.validate_stop_response(
                {
                    "success": True,
                    "recording_id": "run-01",
                    "frames_written": frames,
                    "dropped_frames": 0,
                    "incomplete": False,
                    "writer_error": None,
                    "rx_summaries": rx_summaries(duration),
                },
                "run-01",
                duration,
            )
        )
        self.assertEqual(completed_frames, frames)
        self.assertEqual(len(summaries), 4)

    def test_drops_incomplete_or_low_rate_fail_closed(self):
        base = {
            "success": True,
            "recording_id": "run-01",
            "frames_written": 700,
            "dropped_frames": 0,
            "incomplete": False,
            "writer_error": None,
            "rx_summaries": rx_summaries(),
        }
        cases = [
            {**base, "dropped_frames": 1},
            {**base, "success": False, "incomplete": True},
            {**base, "frames_written": 699},
            {**base, "recording_id": "other"},
            {**base, "rx_summaries": rx_summaries(frames_per_rx=174)},
            {**base, "rx_summaries": rx_summaries()[:3]},
        ]
        for payload in cases:
            with self.subTest(payload=payload):
                with self.assertRaises(capture.CaptureError):
                    capture.validate_stop_response(payload, "run-01", 35)

    def test_listing_must_match_clean_setup_bound_sidecar(self):
        clean = {
            "recordings": [
                {
                    "id": "run-01",
                    "status": "completed",
                    "incomplete": False,
                    "dropped_frames": 0,
                    "integrity_error": None,
                    "format": "raw-csi-v1-jsonl",
                    "capture_scope": "validated_udp_csi_all_grids",
                    "setup_id": SETUP_ID,
                    "setup_sha256": SETUP_SHA256,
                    "frames": 700,
                    "frame_count": 700,
                    "frames_written": 700,
                    "duration_secs": 35,
                    "rx_summaries": rx_summaries(),
                }
            ]
        }
        capture.validate_recording_listing(
            clean, "run-01", SETUP_ID, SETUP_SHA256, 700, 35
        )

        mismatched = {"recordings": [{**clean["recordings"][0], "setup_id": "other"}]}
        with self.assertRaises(capture.CaptureError):
            capture.validate_recording_listing(
                mismatched, "run-01", SETUP_ID, SETUP_SHA256, 700, 35
            )


class TestEmptyRoomMainFlow(unittest.TestCase):
    def run_main_with_responses(self, responses, calls=None):
        calls = [] if calls is None else calls

        def request(_server, path, **kwargs):
            expected_path, response = responses.pop(0)
            self.assertEqual(path, expected_path)
            calls.append((path, kwargs))
            if isinstance(response, BaseException):
                raise response
            return response

        args = Namespace(
            server="http://server",
            kind="empty",
            recording_id="empty-neutral-01",
            confirm_empty_room=True,
        )
        with (
            mock.patch.object(capture, "parse_args", return_value=args),
            mock.patch.object(capture, "request_json", side_effect=request),
            mock.patch.object(capture, "wait_for_capture") as wait,
            mock.patch.object(
                capture.time,
                "monotonic",
                side_effect=[0.0, 65.0, 65.0],
            ),
        ):
            result = capture.main()
        self.assertEqual(responses, [])
        return result, calls, wait

    def test_empty_run_calibrates_before_unlabelled_recording_and_validates_after(self):
        duration = capture.PROTOCOLS["empty"].duration_seconds
        responses = [
            ("/health/ready", readiness()),
            ("/api/v1/nodes", nodes(1, 2, 3, 4)),
            ("/api/v1/recording/list", {"recordings": []}),
            (capture.CALIBRATION_STATUS_PATH, {"success": True, "phase": "uncalibrated"}),
            (
                capture.CALIBRATION_START_PATH,
                {
                    "success": True,
                    "status": "collecting",
                    "recommended_seconds": 60,
                },
            ),
            (capture.CALIBRATION_STATUS_PATH, calibration_status("collecting")),
            (
                "/api/v1/recording/start",
                {
                    "success": True,
                    "recording_id": "empty-neutral-01",
                    "max_duration_seconds": duration + capture.WATCHDOG_GRACE_SECONDS,
                },
            ),
            (
                "/api/v1/recording/stop",
                recording_stop_response("empty-neutral-01", duration),
            ),
            (capture.CALIBRATION_STATUS_PATH, calibration_status("collecting")),
            (capture.CALIBRATION_STOP_PATH, calibration_stop_response()),
            (capture.CALIBRATION_STATUS_PATH, calibration_status("ready")),
            (
                "/api/v1/recording/list",
                recording_listing("empty-neutral-01", duration),
            ),
        ]
        result, calls, wait = self.run_main_with_responses(responses)
        self.assertEqual(result, 0)
        wait.assert_called_once_with(duration)

        paths = [path for path, _ in calls]
        self.assertLess(
            paths.index(capture.CALIBRATION_START_PATH),
            paths.index("/api/v1/recording/start"),
        )
        self.assertLess(
            paths.index("/api/v1/recording/stop"),
            paths.index(capture.CALIBRATION_STOP_PATH),
        )
        recording_start = next(
            kwargs for path, kwargs in calls if path == "/api/v1/recording/start"
        )
        self.assertEqual(
            recording_start["body"],
            {
                "id": "empty-neutral-01",
                "max_duration_seconds": duration + capture.WATCHDOG_GRACE_SECONDS,
            },
        )

    def test_recording_start_failure_still_finishes_active_calibration(self):
        duration = capture.PROTOCOLS["empty"].duration_seconds
        responses = [
            ("/health/ready", readiness()),
            ("/api/v1/nodes", nodes(1, 2, 3, 4)),
            ("/api/v1/recording/list", {"recordings": []}),
            (capture.CALIBRATION_STATUS_PATH, {"success": True, "phase": "uncalibrated"}),
            (
                capture.CALIBRATION_START_PATH,
                {
                    "success": True,
                    "status": "collecting",
                    "recommended_seconds": 60,
                },
            ),
            (capture.CALIBRATION_STATUS_PATH, calibration_status("collecting")),
            (
                "/api/v1/recording/start",
                {"success": False, "error": "recorder unavailable"},
            ),
            (capture.CALIBRATION_STATUS_PATH, calibration_status("collecting")),
            (capture.CALIBRATION_STOP_PATH, calibration_stop_response()),
            (capture.CALIBRATION_STATUS_PATH, calibration_status("ready")),
        ]

        with self.assertRaisesRegex(capture.CaptureError, "recorder unavailable"):
            self.run_main_with_responses(responses)
        self.assertEqual(responses, [])

    def test_recording_stop_failure_still_finishes_active_calibration(self):
        duration = capture.PROTOCOLS["empty"].duration_seconds
        responses = [
            ("/health/ready", readiness()),
            ("/api/v1/nodes", nodes(1, 2, 3, 4)),
            ("/api/v1/recording/list", {"recordings": []}),
            (capture.CALIBRATION_STATUS_PATH, {"success": True, "phase": "uncalibrated"}),
            (
                capture.CALIBRATION_START_PATH,
                {
                    "success": True,
                    "status": "collecting",
                    "recommended_seconds": 60,
                },
            ),
            (capture.CALIBRATION_STATUS_PATH, calibration_status("collecting")),
            (
                "/api/v1/recording/start",
                {
                    "success": True,
                    "recording_id": "empty-neutral-01",
                    "max_duration_seconds": duration + capture.WATCHDOG_GRACE_SECONDS,
                },
            ),
            ("/api/v1/recording/stop", capture.CaptureError("stop unavailable")),
            (capture.CALIBRATION_STATUS_PATH, calibration_status("collecting")),
            (capture.CALIBRATION_STOP_PATH, calibration_stop_response()),
            (capture.CALIBRATION_STATUS_PATH, calibration_status("ready")),
        ]
        calls = []
        with self.assertRaisesRegex(capture.CaptureError, "stop unavailable"):
            self.run_main_with_responses(responses, calls)
        self.assertEqual(responses, [])
        paths = [path for path, _ in calls]
        self.assertIn(capture.CALIBRATION_STOP_PATH, paths)


if __name__ == "__main__":
    unittest.main()

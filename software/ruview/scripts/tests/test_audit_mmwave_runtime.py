"""Tests for the fail-closed mmWave runtime artifact auditor."""

from __future__ import annotations

import json
import os
import sys
import tempfile
import unittest
from pathlib import Path


HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(os.path.dirname(HERE)))

from scripts import audit_mmwave_runtime as audit  # noqa: E402


TRANSFORM = {
    "local": "x_right_y_forward_mm",
    "room": "x_length_z_width_mm",
    "origin_x_mm": 2050,
    "origin_z_mm": 3300,
    "yaw_mdeg": -90000,
    "raw_x_inverted": False,
}


def setup_document():
    return {
        "schema_version": 2,
        "room_dimensions_mm": [4020, 2590, 3440],
        "mmwave": {
            "node_id": "MMWAVE1",
            "transform": {
                "origin_x_mm": 2050,
                "origin_z_mm": 3300,
                "yaw_mdeg": -90000,
                "raw_x_inverted": False,
            },
        },
    }


def record(sequence: int, monotonic_ns: int, *, boot_id: int = 7):
    room_x_mm, room_z_mm = audit.transform_target(TRANSFORM, -500, 1500)
    return {
        "schema_version": 2,
        "clock_epoch_id": "epoch-1",
        "host_monotonic_ns": monotonic_ns,
        "host_unix_ns": monotonic_ns + 1_000_000_000,
        "packet": {
            "schema": "ruview.mmwave.ld2450.v1",
            "node_id": "MMWAVE1",
            "boot_id": boot_id,
            "sequence": sequence,
            "coordinate_frame": dict(TRANSFORM),
            "targets": [
                {
                    "slot": 1,
                    "present": True,
                    "x_mm": -500,
                    "y_mm": 1500,
                    "room_x_mm": room_x_mm,
                    "room_z_mm": room_z_mm,
                }
            ],
        },
    }


def status(
    *,
    boot_id: int = 7,
    rejected: int = 0,
    bounds: int = 0,
    lost: int = 0,
    reboots: int = 0,
):
    return {
        "boot_id": boot_id,
        "packets_rejected": rejected,
        "packets_lost": lost,
        "reboot_count": reboots,
        "reject_reasons": {"room_bounds": bounds} if bounds else {},
        "transform": dict(TRANSFORM),
        "preflight": {
            "gates": [
                {
                    "id": "radar_sequence_loss_free",
                    "pass": lost == 0 and reboots == 0,
                }
            ]
        },
    }


class TestMmwaveRuntimeAudit(unittest.TestCase):
    def audit(self, records, before=None, after=None):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            setup_path = root / "setup.json"
            recording_path = root / "session.mmwave.jsonl"
            setup_path.write_text(json.dumps(setup_document()), encoding="utf-8")
            recording_path.write_text(
                "".join(json.dumps(item) + "\n" for item in records),
                encoding="utf-8",
            )
            before_path = None
            after_path = None
            if before is not None:
                before_path = root / "before.json"
                before_path.write_text(json.dumps(before), encoding="utf-8")
            if after is not None:
                after_path = root / "after.json"
                after_path.write_text(json.dumps(after), encoding="utf-8")
            return audit.audit_runtime(
                recording_path,
                setup_path,
                status_before_path=before_path,
                status_after_path=after_path,
            )

    def test_clean_session_and_status_window_pass(self):
        report = self.audit(
            [record(10, 1_000_000_000), record(11, 1_400_000_000)],
            status(),
            status(),
        )
        self.assertEqual(report["verdict"], "pass")
        self.assertTrue(all(item["pass"] for item in report["checks"]))

    def test_session_without_status_snapshots_fails_closed(self):
        report = self.audit([record(10, 1_000_000_000)])
        self.assertEqual(report["verdict"], "fail")
        failed = {item["id"] for item in report["checks"] if not item["pass"]}
        self.assertEqual(failed, {"server_status_snapshots"})

    def test_gap_reboot_and_server_bounds_reject_are_reported(self):
        report = self.audit(
            [
                record(10, 1_000_000_000),
                record(13, 2_000_000_000),
                record(1, 3_000_000_000, boot_id=8),
            ],
            status(),
            status(boot_id=8, rejected=1, bounds=1, lost=2, reboots=1),
        )
        self.assertEqual(report["verdict"], "fail")
        self.assertEqual(report["missing_packets"], 2)
        self.assertEqual(len(report["reboot_events"]), 1)
        self.assertEqual(report["status_evidence"]["room_bounds_delta"], 1)
        failed = {item["id"] for item in report["checks"] if not item["pass"]}
        self.assertIn("radar_sequence_continuity", failed)
        self.assertIn("radar_boot_stability", failed)
        self.assertIn("server_room_bounds_delta", failed)

    def test_recorded_coordinates_are_recomputed(self):
        invalid = record(10, 1_000_000_000)
        invalid["packet"]["targets"][0]["room_x_mm"] += 1
        report = self.audit([invalid], status(), status())
        self.assertEqual(report["verdict"], "fail")
        self.assertEqual(len(report["coordinate_mismatches"]), 1)

    def test_rear_wall_minus_90_transform_matches_room_references(self):
        references = {
            "sensor_front_1m": ((0, 1000), (2050, 2300)),
            "RX2": ((1970, 2330), (4020, 970)),
            "RX3": ((-2050, 1190), (0, 2110)),
            "RX4": ((1970, 840), (4020, 2460)),
            "TX": ((-540, 2910), (1510, 390)),
        }

        for name, (raw_position, expected_room_position) in references.items():
            with self.subTest(reference=name):
                self.assertEqual(
                    audit.transform_target(TRANSFORM, *raw_position),
                    expected_room_position,
                )


if __name__ == "__main__":
    unittest.main()

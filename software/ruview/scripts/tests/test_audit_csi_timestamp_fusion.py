"""Tests for the diagnostic Raw-CSI-v2 timestamp fusion auditor."""

from __future__ import annotations

import json
import os
import sys
import tempfile
import unittest
from pathlib import Path


HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(os.path.dirname(HERE)))

from scripts import audit_csi_timestamp_fusion as audit  # noqa: E402


def frame(rx_id: int, host_us: int, mesh_us: int | None, sequence: int) -> dict:
    return {
        "schema_version": 2,
        "rx_id": rx_id,
        "host_monotonic_ns": host_us * 1_000,
        "mesh_timestamp_us": mesh_us,
        "sequence": sequence,
    }


class TestCsiTimestampFusionAudit(unittest.TestCase):
    def audit(self, records: list[dict]) -> dict:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "capture.raw-csi.v2.jsonl"
            path.write_text(
                "".join(json.dumps(record) + "\n" for record in records),
                encoding="utf-8",
            )
            return audit.audit_capture(path)

    def test_mesh_divergence_uses_host_coherent_quartet(self):
        records = [
            frame(1, 1_000_000, 8_000_000, 10),
            frame(2, 1_005_000, 8_200_000, 20),
            frame(3, 1_011_000, 8_400_000, 30),
            frame(4, 1_018_000, 8_600_000, 40),
        ]

        report = self.audit(records)

        synchronizer = report["synchronizer"]
        self.assertEqual(synchronizer["quartet_count"], 1)
        self.assertEqual(synchronizer["basis_counts"]["host_monotonic"], 1)
        self.assertEqual(synchronizer["host_spread"]["max_us"], 18_000)
        self.assertEqual(synchronizer["mesh_spread"]["max_us"], 600_000)
        self.assertEqual(synchronizer["selected_guard_violations"], 0)
        self.assertFalse(report["sealed_live_acceptance"])
        self.assertFalse(report["calibration_eligible"])

    def test_unmatched_frame_is_discarded_before_disjoint_selection(self):
        records = [
            frame(1, 900_000, 7_900_000, 9),
            frame(1, 1_000_000, 8_000_000, 10),
            frame(2, 1_005_000, 8_004_000, 20),
            frame(3, 1_011_000, 8_009_000, 30),
            frame(4, 1_018_000, 8_012_000, 40),
        ]

        report = self.audit(records)

        synchronizer = report["synchronizer"]
        self.assertEqual(synchronizer["quartet_count"], 1)
        self.assertEqual(synchronizer["basis_counts"]["mesh"], 1)
        self.assertEqual(synchronizer["discarded_unmatched_frames_by_rx"]["1"], 1)

    def test_missing_receiver_fails_closed(self):
        records = [
            frame(1, 1_000_000, None, 10),
            frame(2, 1_005_000, None, 20),
            frame(3, 1_011_000, None, 30),
        ]

        with self.assertRaisesRegex(audit.AuditError, "RX4"):
            self.audit(records)

    def test_per_receiver_host_time_regression_fails_closed(self):
        records = [
            frame(1, 1_000_000, None, 10),
            frame(1, 999_999, None, 11),
            frame(2, 1_005_000, None, 20),
            frame(3, 1_011_000, None, 30),
            frame(4, 1_018_000, None, 40),
        ]

        with self.assertRaisesRegex(audit.AuditError, "host time regressed"):
            self.audit(records)


if __name__ == "__main__":
    unittest.main()

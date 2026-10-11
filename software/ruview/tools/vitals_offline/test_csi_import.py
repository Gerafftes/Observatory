"""Focused synthetic contract checks; no hardware or private captures needed."""

from __future__ import annotations

lazy from contextlib import redirect_stdout
lazy import copy
lazy import hashlib
lazy import io
lazy import json
lazy from pathlib import Path
lazy import tempfile
lazy import unittest

lazy from csi_import import (
    U32_MAX, RecordingError, analyze_capture, iter_tx1_frames,
    load_sealed_tx1_setup, main, validate_frame,
)


HERE = Path(__file__).resolve().parent
FIXTURE = HERE / "fixtures" / "synthetic_one_link.raw-csi.v1.jsonl"
TX1 = "a" * 64  # Synthetic expected filter identity, never a real device value.


def write_rows(directory: Path, rows: list[dict]) -> Path:
    path = directory / "synthetic-generated.raw-csi.v1.jsonl"
    with path.open("w", encoding="utf-8") as stream:
        for row in rows:
            stream.write(json.dumps(row) + "\n")
    return path


def base_v3(sequence: int, host_mono: int, driver: int) -> dict:
    return {
        "schema_version": 2,
        "host_timestamp_unix_ns": 1_800_000_000_000_000_000 + host_mono,
        "host_monotonic_ns": host_mono,
        "clock_epoch_id": "synthetic-host-epoch",
        "session_id": "synthetic-fixture",
        "label": "synthetic-no-physiology",
        "rx_id": 1,
        "antenna_count": 2,
        "subcarrier_count": 2,
        "center_frequency_mhz": 2437,
        "sequence": sequence,
        "rssi_dbm": -50,
        "noise_floor_dbm": -94,
        "ppdu_type": 0,
        "flags": 0x10,
        "mesh_timestamp_us": 1_000_000 + host_mono // 1000,
        "source_binding": {
            "trailer_version": 1,
            "flags": 7,
            "scheme": "sha256-ruview-tx-filter-mac-v1",
            "tx_filter_sha256": TX1,
        },
        "protocol_identity": {
            "protocol_version": 3,
            "role": 1,
            "boot_id": 123,
            "callback_started_us": 1_000_000 + host_mono // 1000,
            "driver_rx_timestamp_us": driver,
            "quality_flags": 0x12,
            "rx_state": 0,
            "antenna_index": 0,
            "gain_db": None,
            "csi_config_flags": 0x0B,
            "sync_age_ms": 20,
        },
        "iq_pairs": [
            {"i": -2, "q": 3}, {"i": 4, "q": -5},
            {"i": -6, "q": 7}, {"i": 8, "q": -9},
        ],
    }


class CsiImportTests(unittest.TestCase):
    def test_fixture_selects_one_rx_and_exact_bound_source(self) -> None:
        frames = list(iter_tx1_frames(FIXTURE, 1, TX1))
        self.assertEqual([frame.line_number for frame in frames], [1, 3, 4, 6])
        self.assertEqual([frame.segment_id for frame in frames], [1, 1, 1, 2])
        self.assertEqual(frames[0].complex_samples, (-3 + 4j, 5 - 6j))
        self.assertEqual(frames[0].samples_for_antenna(0), (-3 + 4j, 5 - 6j))
        self.assertEqual(frames[0].row["iq_pairs"][0], {"i": -3, "q": 4})
        report = analyze_capture(FIXTURE, 1, TX1)
        self.assertEqual(report["selected_frames"], 4)
        self.assertEqual(report["target_rx_rejections"], {"source_binding_mismatch": 1})
        self.assertEqual([part["frame_count"] for part in report["segments"]], [3, 1])
        self.assertEqual(report["segments"][0]["sequence_gap_events"], 1)
        self.assertEqual(report["segments"][0]["unobserved_serialized_sequence_ids"], 1)
        self.assertEqual(report["segments"][0]["time_basis"], "host_timestamp_unix_ns")
        self.assertEqual(report["selected_missing_metadata"]["quality_flags"], 4)
        self.assertIsNone(report["selected_quality_metrics"]["noise_floor_dbm_with_valid_flag"])
        self.assertIsNone(report["packet_loss"])

    def test_no_tx1_authority_does_not_select_any_source(self) -> None:
        report = analyze_capture(FIXTURE, 1)
        self.assertEqual(report["selected_frames"], 0)
        self.assertEqual(report["target_rx_rejections"], {"trusted_tx1_digest_missing": 5})
        self.assertFalse(report["source_authority"]["expected_tx1_filter_digest_supplied"])
        with self.assertRaises(ValueError):
            list(iter_tx1_frames(FIXTURE, 1, "0" * 64))

    def test_v3_preserves_antenna_order_clocks_wraps_and_splits(self) -> None:
        first = base_v3(U32_MAX - 1, 0, U32_MAX - 15)
        second = base_v3(U32_MAX, 100_000_000, U32_MAX - 5)
        third = base_v3(0, 200_000_000, 10)
        fourth = base_v3(1, 300_000_000, 110_000)
        fourth["host_timestamp_unix_ns"] += 2_000_000_000
        fifth = copy.deepcopy(fourth)
        fifth["sequence"] = 2
        fifth["host_monotonic_ns"] = 400_000_000
        fifth["host_timestamp_unix_ns"] += 100_000_000
        fifth["protocol_identity"]["boot_id"] = 124
        sixth = copy.deepcopy(fifth)
        sixth["sequence"] = 3
        sixth["host_monotonic_ns"] = 500_000_000
        sixth["host_timestamp_unix_ns"] += 100_000_000
        sixth["center_frequency_mhz"] = 2462
        with tempfile.TemporaryDirectory() as temporary:
            path = write_rows(Path(temporary), [first, second, third, fourth, fifth, sixth])
            frames = list(iter_tx1_frames(path, 1, TX1))
            self.assertEqual(frames[0].samples_for_antenna(1), (-6 + 7j, 8 - 9j))
            report = analyze_capture(path, 1, TX1)
        self.assertEqual([part["frame_count"] for part in report["segments"]], [3, 1, 1, 1])
        self.assertEqual(report["segments"][0]["time_basis"], "host_monotonic_ns")
        self.assertEqual(report["segments"][0]["observed_rate_hz"], 10)
        self.assertEqual(report["segments"][0]["sequence_wraps"], 1)
        self.assertEqual(report["segments"][0]["driver_timestamp_wraps"], 1)
        self.assertEqual(report["segment_breaks"], {
            "host_utc_mapping_jump": 1, "receiver_boot_change": 1,
            "grid_or_protocol_change": 1,
        })
        self.assertEqual(report["selected_quality_flags_set"], {
            "noise_floor_valid": 6, "raw_profile": 6
        })
        self.assertEqual(report["selected_quality_metrics"]["noise_floor_dbm_with_valid_flag"]["count"], 6)

    def test_reversals_duplicates_and_untrusted_interruptions_are_reported(self) -> None:
        first = base_v3(10, 0, 100)
        duplicate = copy.deepcopy(first)
        third = base_v3(11, 100_000_000, 200)
        third["mesh_timestamp_us"] = first["mesh_timestamp_us"] - 1
        reset = base_v3(2, 200_000_000, 300)
        reversed_host = base_v3(3, 150_000_000, 400)
        wrong_source = base_v3(4, 250_000_000, 500)
        wrong_source["source_binding"]["tx_filter_sha256"] = "b" * 64
        resumed = base_v3(5, 350_000_000, 600)
        with tempfile.TemporaryDirectory() as temporary:
            path = write_rows(Path(temporary), [
                first, duplicate, third, reset, reversed_host, wrong_source, resumed,
            ])
            report = analyze_capture(path, 1, TX1)
        self.assertEqual([part["frame_count"] for part in report["segments"]], [3, 1, 1, 1])
        self.assertEqual(report["segments"][0]["duplicate_host_timestamps"], 1)
        self.assertEqual(report["segments"][0]["duplicate_sequences_with_same_iq"], 1)
        self.assertEqual(report["segments"][0]["mesh_time_reversals"], 1)
        self.assertEqual(report["segment_breaks"], {
            "sequence_reset_or_reorder": 1,
            "host_time_reversal": 1,
            "unselected_target_rx_frame": 1,
        })
        self.assertEqual(report["target_rx_rejections"], {"source_binding_mismatch": 1})

    def test_invalid_line_and_conflicting_sequence_split_sections(self) -> None:
        first = base_v3(1, 0, 100)
        conflict = base_v3(1, 100_000_000, 200)
        conflict["iq_pairs"][0]["i"] = 17
        third = base_v3(2, 200_000_000, 300)
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "synthetic-generated.raw-csi.v1.jsonl"
            with path.open("w", encoding="utf-8") as output:
                output.write(json.dumps(first) + "\n")
                output.write(json.dumps(conflict) + "\n")
                output.write("{broken-json\n")
                output.write(json.dumps(third) + "\n")
            report = analyze_capture(path, 1, TX1)
            with self.assertRaisesRegex(RecordingError, "line 3"):
                list(iter_tx1_frames(path, 1, TX1))
        self.assertEqual(report["invalid_rows"], 1)
        self.assertEqual([part["frame_count"] for part in report["segments"]], [1, 1, 1])
        self.assertEqual(report["segment_breaks"], {
            "sequence_reused_with_different_iq": 1,
            "invalid_record_between_selected_frames": 1,
        })

    def test_invalid_or_unbound_data_never_becomes_tx1(self) -> None:
        row = base_v3(1, 100, 100)
        row["iq_pairs"].pop()
        with self.assertRaisesRegex(RecordingError, "iq_pairs length"):
            validate_frame(row, 1)
        row = base_v3(1, 100, 100)
        row["source_binding"]["flags"] = 0
        with self.assertRaisesRegex(RecordingError, "source binding"):
            validate_frame(row, 1)

    def test_sealed_setup_reference_must_match_recording_sidecar(self) -> None:
        definition = {
            "schema_version": 3,
            "transmitter": {"node_id": "TX1"},
            "radio": {"tx_filter_identity": {
                "scheme": "sha256-ruview-tx-filter-mac-v1", "sha256": TX1,
            }},
        }
        canonical = (json.dumps(definition, sort_keys=True, indent=2) + "\n").encode()
        digest = hashlib.sha256(b"ruview.position-setup.v3\0" + canonical).hexdigest()
        setup = {
            "kind": "ruview.position-setup", "schema_version": 3,
            "setup_id": f"setup-{digest[:16]}", "setup_sha256": digest,
            "definition": definition,
        }
        with tempfile.TemporaryDirectory() as temporary:
            folder = Path(temporary)
            setup_path = folder / "synthetic-setup.json"
            setup_path.write_text(json.dumps(setup), encoding="utf-8")
            recording = write_rows(folder, [base_v3(1, 100, 100)])
            sidecar = recording.with_name(recording.name.replace(".jsonl", ".meta.json"))
            metadata = {
                "status": "completed", "frames_written": 1,
                "rx_summaries": [{"rx_id": 1, "frames_written": 1}],
                "setup_id": setup["setup_id"], "setup_sha256": digest,
            }
            sidecar.write_text(json.dumps(metadata), encoding="utf-8")
            self.assertEqual(load_sealed_tx1_setup(setup_path).digest, TX1)
            report = analyze_capture(recording, 1, sealed_tx1_setup=setup_path)
            self.assertTrue(report["source_authority"]["sidecar_setup_identity_matched"])
            sidecar.unlink()
            with self.assertRaisesRegex(RecordingError, "no matching sealed setup identity"):
                analyze_capture(recording, 1, sealed_tx1_setup=setup_path)
            metadata["setup_id"] = "setup-unrelated"
            sidecar.write_text(json.dumps(metadata), encoding="utf-8")
            with self.assertRaisesRegex(RecordingError, "different sealed setup"):
                analyze_capture(recording, 1, sealed_tx1_setup=setup_path)

    def test_cli_writes_machine_readable_report_without_overwrite(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "quality.json"
            with redirect_stdout(io.StringIO()):
                self.assertEqual(main([
                    str(FIXTURE), "--rx", "1", "--trusted-tx1-filter-sha256", TX1,
                    "--report", str(output),
                ]), 0)
            report = json.loads(output.read_text(encoding="utf-8"))
            self.assertEqual(report["selected_frames"], 4)
            self.assertEqual(report["report_schema_version"], 1)
            self.assertNotIn(TX1, output.read_text(encoding="utf-8"))


if __name__ == "__main__":
    unittest.main()

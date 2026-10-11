"""Synthetic end-to-end checks through the original import, no hardware required."""

lazy import io
lazy import json
lazy import tempfile
lazy import unittest
lazy from contextlib import redirect_stderr, redirect_stdout
lazy from dataclasses import replace
lazy from pathlib import Path

lazy import numpy as np
lazy from csi_import import iter_tx1_frames
lazy from test_csi_import import FIXTURE, TX1, base_v3, write_rows
lazy from vitals import (
    ACCEPTED,
    CANDIDATE,
    UNAVAILABLE,
    Config,
    analyze_frames,
    analyze_recording,
    main,
)


def signal_rows(rate=17.51, duration=65, jitter=0, respiration=.27, pulse=1.23,
                respiration_amplitude=10, pulse_amplitude=9, noise=0.3, seed=42):
    rng = np.random.default_rng(seed)
    times = np.arange(0, duration + .5 / rate, 1 / rate)
    if jitter:
        times[1:-1] += rng.uniform(-jitter / rate, jitter / rate, len(times) - 2)
    rows = []
    for sequence, t in enumerate(times):
        mono = round(float(t) * 1e9)
        row = base_v3(sequence, mono, (1_000_000 + mono // 1000) & 0xFFFFFFFF)
        row["antenna_count"] = 1
        row["subcarrier_count"] = 8
        row["protocol_identity"]["quality_flags"] = 0x16
        value = (respiration_amplitude * np.sin(2 * np.pi * respiration * t)
                 + pulse_amplitude * np.sin(2 * np.pi * pulse * t))
        row["iq_pairs"] = [{"i": round(65 + value + rng.normal(0, noise)), "q": 0}
                           for _ in range(8)]
        rows.append(row)
    return rows


def analyze_rows(rows, config=None):
    with tempfile.TemporaryDirectory() as temporary:
        path = write_rows(Path(temporary), rows)
        return analyze_frames(list(iter_tx1_frames(path, 1, TX1)), config)


def branch_windows(report, branch):
    return [window for window in report["windows"] if window["branch"] == branch]


class VitalsTests(unittest.TestCase):
    def assert_no_frequency(self, window):
        self.assertEqual(window["status"], UNAVAILABLE)
        for key in ("candidate_hz", "candidate_bpm", "estimate_hz", "estimate_bpm"):
            self.assertIsNone(window[key], key)
        self.assertNotIn("peak_hz", window.get("spectral_quality", {}))

    def test_known_frequencies_at_different_real_rates(self):
        for rate in (12.5, 17.51, 24.43, 53.7):
            with self.subTest(rate=rate):
                report = analyze_rows(signal_rows(rate=rate))
                for branch, frequency in (("respiration", .27), ("pulse", 1.23)):
                    window = branch_windows(report, branch)[-1]
                    self.assertEqual(window["status"], ACCEPTED, window["reasons"])
                    self.assertAlmostEqual(window["estimate_hz"], frequency, delta=.009)
                    self.assertAlmostEqual(window["estimate_bpm"], 60 * frequency, delta=.54)
                    self.assertFalse(window["physiologically_validated"])
                    self.assertAlmostEqual(window["timing"]["observed_arrival_rate_hz"], rate, delta=.001)

    def test_irregular_timestamps_are_used_without_interpolation(self):
        report = analyze_rows(signal_rows(rate=17.51, jitter=.35))
        for branch, expected in (("respiration", .27), ("pulse", 1.23)):
            window = branch_windows(report, branch)[-1]
            self.assertEqual(window["status"], ACCEPTED, window["reasons"])
            self.assertAlmostEqual(window["estimate_hz"], expected, delta=.009)
            self.assertEqual(window["timing"]["interpolated_samples"], 0)

    def test_missing_packets_with_small_gaps_use_actual_elapsed_time(self):
        rows = [row for i, row in enumerate(signal_rows(rate=24.43)) if i % 19 != 8]
        report = analyze_rows(rows)
        self.assertEqual(branch_windows(report, "pulse")[-1]["status"], ACCEPTED)
        self.assertAlmostEqual(branch_windows(report, "pulse")[-1]["estimate_hz"], 1.23, delta=.009)

    def test_slow_sampling_can_support_breathing_but_not_pulse(self):
        report = analyze_rows(signal_rows(rate=3.7, pulse_amplitude=0))
        self.assertEqual(branch_windows(report, "respiration")[-1]["status"], ACCEPTED)
        self.assert_no_frequency(branch_windows(report, "pulse")[-1])
        self.assertIn("insufficient_observed_sampling_density", branch_windows(report, "pulse")[-1]["reasons"])

    def test_long_gap_blocks_intersecting_windows_and_later_recovers(self):
        rows = [row for row in signal_rows(duration=130)
                if not 61 < row["host_monotonic_ns"] / 1e9 < 64]
        report = analyze_rows(rows)
        for branch in ("respiration", "pulse"):
            windows = branch_windows(report, branch)
            gap_window = next(w for w in windows if w["end_s"] == 65)
            self.assert_no_frequency(gap_window)
            self.assertIn("data_gap_exceeds_limit", gap_window["reasons"])
            self.assertGreater(gap_window["timing"]["gaps"][0]["duration_s"], 2.9)
            self.assertEqual(windows[-1]["status"], ACCEPTED, windows[-1]["reasons"])

    def test_backward_clock_and_epoch_boot_grid_changes_never_join(self):
        for change in ("host", "callback", "epoch", "boot", "grid", "source"):
            with self.subTest(change=change):
                rows = signal_rows(duration=80)
                for row in rows[len(rows) // 2:]:
                    if change == "host":
                        row["host_monotonic_ns"] -= 30_000_000_000
                    elif change == "callback":
                        row["protocol_identity"]["callback_started_us"] -= 30_000_000
                    elif change == "epoch":
                        row["clock_epoch_id"] = "new-epoch"
                    elif change == "boot":
                        row["protocol_identity"]["boot_id"] += 1
                    elif change == "grid":
                        row["center_frequency_mhz"] += 5
                    elif change == "source":
                        row["source_binding"]["tx_filter_sha256"] = "b" * 64
                report = analyze_rows(rows)
                self.assertTrue(all(w["estimate_hz"] is None for w in report["windows"]))
                for window in branch_windows(report, "respiration"):
                    self.assert_no_frequency(window)
                self.assertGreaterEqual(len(report["segments"]), 1 if change == "source" else 2)

    def test_host_wall_clock_alone_has_no_frequency_even_for_perfect_tone(self):
        rows = signal_rows()
        for row in rows:
            row["schema_version"] = 1
            del row["host_monotonic_ns"], row["clock_epoch_id"], row["protocol_identity"]
        report = analyze_rows(rows)
        for window in report["windows"]:
            self.assert_no_frequency(window)
            self.assertIn("monotonic_host_time_missing", window["reasons"])

    def test_mesh_reversals_are_visible_and_never_used_as_time(self):
        rows = signal_rows()
        for row in rows[1::2]:
            row["mesh_timestamp_us"] = 0
        report = analyze_rows(rows)
        window = branch_windows(report, "respiration")[-1]
        self.assertEqual(window["status"], ACCEPTED)
        self.assertGreater(window["timing"]["mesh_reversals"], 0)
        self.assertFalse(window["timing"]["mesh_used_for_spectrum"])

    def test_duplicate_timestamps_or_sequences_have_no_frequency(self):
        for change in ("time", "sequence"):
            rows = signal_rows()
            index = len(rows) // 2
            if change == "time":
                rows[index]["host_monotonic_ns"] = rows[index - 1]["host_monotonic_ns"]
            else:
                rows[index]["sequence"] = rows[index - 1]["sequence"]
                rows[index]["iq_pairs"] = rows[index - 1]["iq_pairs"]
            window = branch_windows(analyze_rows(rows), "respiration")[-1]
            self.assert_no_frequency(window)

    def test_clock_drift_and_transport_bursts_are_rejected(self):
        for change in ("drift", "burst"):
            rows = signal_rows()
            for row in rows:
                callback = row["protocol_identity"]["callback_started_us"]
                if change == "drift":
                    callback = round(callback * 1.04)
                elif row["host_monotonic_ns"] > 30e9:
                    callback += 400_000
                row["protocol_identity"]["callback_started_us"] = callback
            window = branch_windows(analyze_rows(rows), "respiration")[-1]
            self.assert_no_frequency(window)
            self.assertIn("host_callback_timing_disagreement", window["reasons"])

    def test_quality_metadata_missing_downgrades_to_candidate(self):
        for field, reason in (("quality_flags", "quality_flags_missing"),
                              ("callback_started_us", "acquisition_clock_unverified")):
            rows = signal_rows()
            for row in rows:
                row["protocol_identity"][field] = None
            window = branch_windows(analyze_rows(rows), "respiration")[-1]
            self.assertEqual(window["status"], CANDIDATE)
            self.assertIsNotNone(window["candidate_hz"])
            self.assertIsNone(window["estimate_bpm"])
            self.assertIn(reason, window["reasons"])

    def test_noise_and_constant_signals_do_not_get_a_frequency(self):
        for seed in range(5):
            rows = signal_rows(respiration_amplitude=0, pulse_amplitude=0, noise=8, seed=seed)
            for window in analyze_rows(rows)["windows"]:
                self.assert_no_frequency(window)
        rows = signal_rows(respiration_amplitude=0, pulse_amplitude=0, noise=0)
        for window in analyze_rows(rows)["windows"]:
            self.assert_no_frequency(window)

    def test_frequency_drift_stays_diagnostic(self):
        rows = signal_rows(duration=60, pulse_amplitude=0)
        for row in rows:
            t = row["host_monotonic_ns"] / 1e9
            frequency = .18 if t < 30 else .36
            row["iq_pairs"] = [{"i": round(65 + 12 * np.sin(2 * np.pi * frequency * t)), "q": 0}] * 8
        window = branch_windows(analyze_rows(rows), "respiration")[-1]
        self.assertEqual(window["status"], CANDIDATE)
        self.assertIn("frequency_unstable_between_halves", window["reasons"])
        self.assertIsNone(window["estimate_bpm"])

    def test_respiration_harmonic_is_not_accepted_as_pulse(self):
        report = analyze_rows(signal_rows(respiration=.3, pulse=1.2))
        window = branch_windows(report, "pulse")[-1]
        self.assertEqual(window["status"], CANDIDATE)
        self.assertIn("possible_respiration_harmonic", window["reasons"])
        self.assertIn(4, window["respiration_context"]["possible_harmonic_orders"])
        self.assertIsNone(window["estimate_bpm"])

    def test_band_edge_and_two_competing_tones_are_not_accepted(self):
        window = branch_windows(analyze_rows(signal_rows(respiration=.1, pulse_amplitude=0)), "respiration")[-1]
        self.assertEqual(window["status"], CANDIDATE)
        self.assertIn("peak_at_investigation_boundary", window["reasons"])
        rows = signal_rows(respiration=.2, pulse_amplitude=0)
        for row in rows:
            t = row["host_monotonic_ns"] / 1e9
            for pair in row["iq_pairs"]:
                pair["i"] += round(10 * np.sin(2 * np.pi * .4 * t))
        window = branch_windows(analyze_rows(rows), "respiration")[-1]
        self.assertEqual(window["status"], CANDIDATE)
        self.assertIn("competing_spectral_peaks", window["reasons"])

    def test_pulse_quality_failure_does_not_block_respiration(self):
        rows = signal_rows(pulse_amplitude=0)
        report = analyze_rows(rows)
        self.assertEqual(branch_windows(report, "respiration")[-1]["status"], ACCEPTED)
        self.assert_no_frequency(branch_windows(report, "pulse")[-1])

    def test_time_scale_changes_frequency_instead_of_assuming_a_rate(self):
        rows = signal_rows(respiration=.27, pulse_amplitude=0)
        for row in rows:
            row["host_monotonic_ns"] *= 2
            row["host_timestamp_unix_ns"] = 1_800_000_000_000_000_000 + row["host_monotonic_ns"]
            row["protocol_identity"]["callback_started_us"] *= 2
            row["protocol_identity"]["driver_rx_timestamp_us"] *= 2
        window = branch_windows(analyze_rows(rows), "respiration")[-1]
        self.assertEqual(window["status"], ACCEPTED)
        self.assertAlmostEqual(window["estimate_hz"], .135, delta=.005)

    def test_pulse_requires_fresh_sufficient_respiration_context(self):
        report = analyze_rows(signal_rows(duration=40))
        window = branch_windows(report, "pulse")[-1]
        self.assertEqual(window["status"], CANDIDATE)
        self.assertIn("respiration_harmonics_unresolved", window["reasons"])

    def test_absent_breathing_does_not_prove_no_harmonic(self):
        window = branch_windows(analyze_rows(signal_rows(respiration_amplitude=0)), "pulse")[-1]
        self.assertEqual(window["status"], CANDIDATE)
        self.assertIsNone(window["estimate_bpm"])

    def test_old_bpm_never_survives_a_flat_window(self):
        rows = signal_rows(duration=125)
        for row in rows:
            if row["host_monotonic_ns"] > 61e9:
                row["iq_pairs"] = [{"i": 65, "q": 0}] * 8
        windows = branch_windows(analyze_rows(rows), "respiration")
        self.assertEqual(windows[0]["status"], ACCEPTED)
        self.assert_no_frequency(windows[-1])

    def test_labels_filenames_and_presence_confidence_have_no_effect(self):
        rows = signal_rows()
        original = analyze_rows(rows)
        for row in rows:
            row["label"] = "empty"
            row["session_id"] = "empty-pretend"
            row["presence_confidence"] = 0.999
            row["bpm"] = 999
        with tempfile.TemporaryDirectory() as temporary:
            path = write_rows(Path(temporary), rows)
            empty_path = path.with_name("empty.raw-csi.v1.jsonl")
            path.rename(empty_path)
            renamed = analyze_recording(empty_path, 1, TX1)
        self.assertEqual(original["windows"], renamed["windows"])

    def test_first_word_invalid_and_bad_subcarriers_are_excluded(self):
        rows = signal_rows()
        for row in rows:
            row["protocol_identity"]["quality_flags"] |= 1
            row["iq_pairs"][:3] = [{"i": 0, "q": 0}, {"i": 90, "q": 0}, {"i": 127, "q": 0}]
        window = branch_windows(analyze_rows(rows), "respiration")[-1]
        self.assertEqual(window["status"], ACCEPTED)
        self.assertEqual(window["amplitude"]["excluded_recorded_indices"], [0, 1, 2])

    def test_receiver_error_and_gain_steps_are_rejected(self):
        for change in ("state", "gain"):
            rows = signal_rows()
            for index, row in enumerate(rows):
                identity = row["protocol_identity"]
                if change == "state":
                    identity["rx_state"] = 1
                else:
                    identity["quality_flags"] |= 8
                    identity["gain_db"] = 10 if index < len(rows) // 2 else 20
            window = branch_windows(analyze_rows(rows), "respiration")[-1]
            self.assert_no_frequency(window)

    def test_unavailable_antenna_and_all_zero_subcarriers(self):
        rows = signal_rows()
        report = analyze_rows(rows, replace(Config(), antenna=3))
        self.assert_no_frequency(branch_windows(report, "respiration")[-1])
        for row in rows:
            row["iq_pairs"] = [{"i": 0, "q": 0}] * 8
        self.assert_no_frequency(branch_windows(analyze_rows(rows), "respiration")[-1])

    def test_only_requested_antenna_and_same_indices_over_time_are_used(self):
        rows = signal_rows(pulse_amplitude=0)
        for row in rows:
            row["antenna_count"] = 2
            row["iq_pairs"] = [{"i": 35 + i * 4, "q": 0} for i in range(8)] + row["iq_pairs"]
        flat = branch_windows(analyze_rows(rows), "respiration")[-1]
        self.assert_no_frequency(flat)
        tone = branch_windows(analyze_rows(rows, replace(Config(), antenna=1)), "respiration")[-1]
        self.assertEqual(tone["status"], ACCEPTED)
        self.assertAlmostEqual(tone["estimate_hz"], .27, delta=.009)

    def test_v2_identity_cannot_claim_v3_quality_evidence(self):
        rows = signal_rows()
        for row in rows:
            row["protocol_identity"]["protocol_version"] = 2
        window = branch_windows(analyze_rows(rows), "respiration")[-1]
        self.assertEqual(window["status"], CANDIDATE)
        self.assertIn("quality_flags_missing", window["reasons"])

    def test_overly_narrow_band_cannot_invent_background_evidence(self):
        config = replace(Config(), respiration_band=(.26, .28))
        window = branch_windows(analyze_rows(signal_rows(), config), "respiration")[-1]
        self.assert_no_frequency(window)
        self.assertIn("insufficient_spectral_background", window["reasons"])

    def test_configuration_and_window_cadence(self):
        config = Config(respiration_band=(.15, .5), pulse_band=(1, 1.5),
                        respiration_window_s=40, pulse_window_s=20, step_s=7)
        report = analyze_rows(signal_rows(duration=61), config)
        self.assertEqual([w["end_s"] for w in branch_windows(report, "respiration")][0:3], [40, 47, 54])
        self.assertEqual(branch_windows(report, "respiration")[-1]["status"], ACCEPTED)
        for changes in ({"step_s": 0}, {"pulse_band": (2, 1)}, {"respiration_window_s": 1},
                        {"max_gap_s": float("nan")}, {"min_samples_per_cycle": 1},
                        {"min_candidate_fraction": 0}, {"antenna": -1}):
            with self.assertRaises(ValueError):
                replace(Config(), **changes)

    def test_recording_cli_is_private_finite_and_refuses_overwrite(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "analysis.json"
            arguments = [str(FIXTURE), "--rx", "1", "--trusted-tx1-filter-sha256", TX1,
                         "--report", str(output)]
            with redirect_stdout(io.StringIO()):
                self.assertEqual(main(arguments), 0)
            text = output.read_text()
            self.assertNotIn(TX1, text)
            self.assertNotIn("NaN", text)
            report = json.loads(text)
            self.assertEqual(report["import_quality"]["selected_frames"], 4)
            with redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
                main(arguments)
            self.assertEqual(text, output.read_text())

    def test_no_authority_has_no_values_and_invalid_rows_fail_closed(self):
        report = analyze_recording(FIXTURE, 1)
        self.assertEqual(report["windows"], [])
        self.assertEqual(report["reasons"], ["no_selected_frames"])
        with tempfile.TemporaryDirectory() as temporary:
            path = write_rows(Path(temporary), [base_v3(1, 0, 0)])
            with path.open("a") as stream:
                stream.write("broken\n")
            with self.assertRaisesRegex(ValueError, "invalid recording rows"):
                analyze_recording(path, 1, TX1)


if __name__ == "__main__":
    unittest.main()

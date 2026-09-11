from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path

from scripts.evaluate_d5_replay import (
    LabeledRun,
    ReplayConfig,
    RunSample,
    WindowSample,
    calibration_block_mean_samples,
    evaluate_d5,
    fit_empty_only_model,
    load_run,
)


def synthetic_run(
    label: str,
    node_values: dict[int, list[float]],
    interval_s: float = 1.0,
) -> LabeledRun:
    sample_count = len(next(iter(node_values.values())))
    if any(len(values) != sample_count for values in node_values.values()):
        raise ValueError("All synthetic nodes need the same number of samples")

    samples = tuple(
        RunSample(
            elapsed_s=index * interval_s,
            scores={
                node_id: values[index]
                for node_id, values in node_values.items()
            },
        )
        for index in range(sample_count)
    )
    return LabeledRun(label=label, path=Path(f"/{label}.jsonl"), samples=samples)


class D5ReplayTests(unittest.TestCase):
    def setUp(self) -> None:
        self.config = ReplayConfig(
            window_seconds=2.0,
            min_samples_per_window=2,
            min_calibration_blocks=6,
            mad_multiplier=2.0,
            min_robust_scale=0.001,
            quorum=2,
            min_effect=0.01,
            min_reliability=0.30,
            min_vote_fraction=0.35,
            present_persistence_seconds=1.0,
            absent_persistence_seconds=0.0,
            min_validation_recall=0.80,
            max_validation_false_positive_rate=0.10,
        )

    def test_default_deployment_rule_matches_audited_parameters(self) -> None:
        config = ReplayConfig()

        self.assertEqual(config.window_seconds, 10.0)
        self.assertEqual(config.min_calibration_blocks, 6)
        self.assertEqual(config.mad_multiplier, 1.0)
        self.assertEqual(config.min_robust_scale, 0.005)
        self.assertEqual(config.quorum, 2)
        self.assertEqual(config.present_persistence_seconds, 2.0)
        self.assertEqual(config.absent_persistence_seconds, 2.0)

    def test_calibration_uses_complete_non_overlapping_block_means(self) -> None:
        run = LabeledRun(
            label="blocks",
            path=Path("/blocks.jsonl"),
            recording_duration_s=25.0,
            samples=(
                RunSample(0.0, {1: 1.0, 2: 10.0}),
                RunSample(5.0, {1: 3.0, 2: 14.0}),
                RunSample(10.0, {1: 5.0, 2: 20.0}),
                RunSample(15.0, {1: 7.0, 2: 24.0}),
                RunSample(20.0, {1: 100.0, 2: 100.0}),
                RunSample(24.0, {1: 100.0, 2: 100.0}),
            ),
        )

        blocks = calibration_block_mean_samples(run, block_seconds=10.0)

        self.assertEqual(len(blocks), 2)
        self.assertEqual(blocks[0].scores, {1: 2.0, 2: 12.0})
        self.assertEqual(blocks[1].scores, {1: 6.0, 2: 22.0})

    def test_empty_reference_uses_median_and_mad_of_block_means(self) -> None:
        blocks = tuple(
            WindowSample(
                elapsed_s=float(index * 10),
                scores={1: value},
            )
            for index, value in enumerate(
                [0.01, 0.02, 0.03, 0.04, 0.05, 0.06],
                start=1,
            )
        )
        config = ReplayConfig(
            min_calibration_blocks=6,
            mad_multiplier=1.0,
            min_robust_scale=0.005,
        )

        model = fit_empty_only_model(blocks, config)
        reference = model.nodes[0]

        self.assertAlmostEqual(reference.median, 0.035)
        self.assertAlmostEqual(reference.mad, 0.015)
        self.assertAlmostEqual(reference.robust_scale, 0.022239)
        self.assertAlmostEqual(reference.threshold, 0.057239)
        self.assertEqual(reference.block_count, 6)

    def test_stable_positive_link_passes_both_swapped_folds(self) -> None:
        quiet = [0.01] * 20
        noisy = [0.02, 0.08] * 10
        present = [0.08] * 20
        pair1_empty = synthetic_run(
            "pair1_empty",
            {1: quiet, 2: quiet, 3: noisy, 4: noisy},
        )
        pair1_present = synthetic_run(
            "pair1_present",
            {1: present, 2: present, 3: noisy, 4: noisy},
        )
        pair2_empty = synthetic_run(
            "pair2_empty",
            {1: quiet, 2: quiet, 3: noisy, 4: noisy},
        )
        pair2_present = synthetic_run(
            "pair2_present",
            {1: present, 2: present, 3: noisy, 4: noisy},
        )

        result = evaluate_d5(
            pair1_empty,
            pair1_present,
            pair2_empty,
            pair2_present,
            self.config,
        )

        self.assertTrue(result["passed"])
        for fold in result["deployment_candidate"]["folds"]:
            self.assertEqual(fold["validation"]["false_positive_rate"], 0.0)
            self.assertGreaterEqual(fold["validation"]["recall"], 0.80)
        for fold in result["supervised_negative_control"]["folds"]:
            self.assertEqual(fold["model"]["selected_node_ids"], [1, 2])

    def test_swapped_folds_do_not_leak_validation_link_selection(self) -> None:
        quiet = [0.01] * 20
        present = [0.08] * 20
        pair1_empty = synthetic_run("pair1_empty", {1: quiet, 2: quiet})
        pair1_present = synthetic_run("pair1_present", {1: present, 2: quiet})
        pair2_empty = synthetic_run("pair2_empty", {1: quiet, 2: quiet})
        pair2_present = synthetic_run("pair2_present", {1: quiet, 2: present})

        result = evaluate_d5(
            pair1_empty,
            pair1_present,
            pair2_empty,
            pair2_present,
            self.config,
        )

        folds = result["supervised_negative_control"]["folds"]
        self.assertEqual(folds[0]["model"]["selected_node_ids"], [1])
        self.assertEqual(folds[1]["model"]["selected_node_ids"], [2])
        self.assertEqual(folds[0]["validation"]["recall"], 0.0)
        self.assertEqual(folds[1]["validation"]["recall"], 0.0)
        self.assertFalse(result["passed"])

    def test_empty_only_thresholds_do_not_use_present_runs(self) -> None:
        quiet = [0.01] * 20
        low_present = [0.02] * 20
        high_present = [0.20] * 20
        pair1_empty = synthetic_run(
            "pair1_empty",
            {1: quiet, 2: quiet, 3: quiet, 4: quiet},
        )
        pair2_empty = synthetic_run(
            "pair2_empty",
            {1: quiet, 2: quiet, 3: quiet, 4: quiet},
        )

        low_result = evaluate_d5(
            pair1_empty,
            synthetic_run(
                "pair1_low_present",
                {1: low_present, 2: low_present, 3: quiet, 4: quiet},
            ),
            pair2_empty,
            synthetic_run(
                "pair2_low_present",
                {1: low_present, 2: low_present, 3: quiet, 4: quiet},
            ),
            self.config,
        )
        high_result = evaluate_d5(
            pair1_empty,
            synthetic_run(
                "pair1_high_present",
                {1: high_present, 2: high_present, 3: quiet, 4: quiet},
            ),
            pair2_empty,
            synthetic_run(
                "pair2_high_present",
                {1: high_present, 2: high_present, 3: quiet, 4: quiet},
            ),
            self.config,
        )

        low_models = [
            fold["model"] for fold in low_result["deployment_candidate"]["folds"]
        ]
        high_models = [
            fold["model"] for fold in high_result["deployment_candidate"]["folds"]
        ]
        self.assertEqual(low_models, high_models)

    def test_load_run_reads_recording_envelope_and_skips_stale_nodes(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "raw_sensing.jsonl"
            rows = [
                {
                    "elapsed_s": 0.0,
                    "data": {
                        "node_features": [
                            {
                                "node_id": 1,
                                "smoothed_motion_score": 0.1,
                                "stale": False,
                            },
                            {
                                "node_id": 2,
                                "smoothed_motion_score": 0.2,
                                "stale": True,
                            },
                        ]
                    },
                },
                {
                    "elapsed_s": 1.0,
                    "data": {
                        "node_features": [
                            {
                                "node_id": 1,
                                "smoothed_motion_score": 0.3,
                                "stale": False,
                            }
                        ]
                    },
                },
            ]
            path.write_text(
                "".join(json.dumps(row) + "\n" for row in rows),
                encoding="utf-8",
            )

            run = load_run(path, "test", "smoothed_motion_score")

        self.assertEqual(len(run.samples), 2)
        self.assertEqual(run.samples[0].scores, {1: 0.1})
        self.assertEqual(run.samples[1].scores, {1: 0.3})


if __name__ == "__main__":
    unittest.main()

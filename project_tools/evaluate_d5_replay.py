#!/usr/bin/env python3
"""
Evaluate candidate D5 presence detectors against the four labeled D4 runs.

The deployment candidate learns only a robust per-RX empty-room reference:

1. calibrate median/MAD on E0c only, then replay E0d/E1b unchanged;
2. calibrate median/MAD on E0d only, then replay E0c/E1 unchanged.

A supervised train/validate swap is retained as an explicitly labeled negative
control. Only the Python standard library is required. Machine-readable JSON is
written to stdout; a short human-readable summary is written to stderr.
"""

from __future__ import annotations

import argparse
import json
import math
import statistics
import sys
from collections import deque
from dataclasses import asdict, dataclass, replace
from pathlib import Path
from typing import Any, Iterable, TextIO


SCRIPT_DIR = Path(__file__).resolve().parent
PROJECT_DIR = SCRIPT_DIR.parent
RAW_DATA_DIR = PROJECT_DIR / "data" / "raw"

DEFAULT_PAIR1_EMPTY = (
    RAW_DATA_DIR
    / "2026-07-26_21-49-13_E0c_leerraum_Mac_mittig_D4_TX_MAC_Filter"
    / "raw_sensing.jsonl"
)
DEFAULT_PAIR1_PRESENT = (
    RAW_DATA_DIR
    / "2026-07-26_21-53-24_E1_person_sitzt_still_mittig_Mac_mittig_D4_TX_MAC_Filter"
    / "raw_sensing.jsonl"
)
DEFAULT_PAIR2_EMPTY = (
    RAW_DATA_DIR
    / "2026-07-26_22-00-55_E0d_bestaetigung_leerraum_Mac_mittig_D4_TX_MAC_Filter"
    / "raw_sensing.jsonl"
)
DEFAULT_PAIR2_PRESENT = (
    RAW_DATA_DIR
    / "2026-07-26_22-04-22_E1b_bestaetigung_person_sitzt_still_Mac_mittig_D4_TX_MAC_Filter"
    / "raw_sensing.jsonl"
)


@dataclass(frozen=True)
class ReplayConfig:
    window_seconds: float = 10.0
    min_samples_per_window: int = 5
    min_calibration_blocks: int = 6
    mad_multiplier: float = 1.0
    min_robust_scale: float = 0.005
    quorum: int = 2
    min_effect: float = 0.005
    min_reliability: float = 0.30
    min_vote_fraction: float = 0.35
    present_persistence_seconds: float = 2.0
    absent_persistence_seconds: float = 2.0
    score_field: str = "smoothed_motion_score"
    min_validation_recall: float = 0.80
    max_validation_false_positive_rate: float = 0.10


@dataclass(frozen=True)
class RunSample:
    elapsed_s: float
    scores: dict[int, float]


@dataclass(frozen=True)
class LabeledRun:
    label: str
    path: Path
    samples: tuple[RunSample, ...]
    recording_duration_s: float | None = None


@dataclass(frozen=True)
class WindowSample:
    elapsed_s: float
    scores: dict[int, float]


@dataclass(frozen=True)
class NodeModel:
    node_id: int
    threshold: float
    empty_median: float
    present_median: float
    effect: float
    empty_iqr: float
    training_true_positive_rate: float
    training_false_positive_rate: float
    training_balanced_accuracy: float
    reliability: float
    selected: bool
    rejection_reason: str | None


@dataclass(frozen=True)
class PresenceModel:
    nodes: tuple[NodeModel, ...]

    @property
    def selected_nodes(self) -> tuple[NodeModel, ...]:
        return tuple(node for node in self.nodes if node.selected)


@dataclass(frozen=True)
class EmptyBaselineNode:
    node_id: int
    median: float
    mad: float
    robust_scale: float
    threshold: float
    calibration_exceedance_rate: float
    block_count: int


@dataclass(frozen=True)
class EmptyOnlyModel:
    nodes: tuple[EmptyBaselineNode, ...]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Replay D5 on two independent empty/still pairs. JSON is printed "
            "to stdout and a concise summary to stderr."
        )
    )
    parser.add_argument("--pair1-empty", type=Path, default=DEFAULT_PAIR1_EMPTY)
    parser.add_argument("--pair1-present", type=Path, default=DEFAULT_PAIR1_PRESENT)
    parser.add_argument("--pair2-empty", type=Path, default=DEFAULT_PAIR2_EMPTY)
    parser.add_argument("--pair2-present", type=Path, default=DEFAULT_PAIR2_PRESENT)
    parser.add_argument(
        "--window-seconds",
        type=float,
        default=ReplayConfig.window_seconds,
        help="Causal rolling-mean window. Default: 10",
    )
    parser.add_argument(
        "--min-samples-per-window",
        type=int,
        default=ReplayConfig.min_samples_per_window,
        help="Minimum samples required for an RX rolling mean. Default: 5",
    )
    parser.add_argument(
        "--min-calibration-blocks",
        type=int,
        default=ReplayConfig.min_calibration_blocks,
        help="Minimum complete non-overlapping calibration blocks per RX. Default: 6",
    )
    parser.add_argument(
        "--mad-multiplier",
        type=float,
        default=ReplayConfig.mad_multiplier,
        help="Fixed median + k*MAD-scale multiplier. Default: 1",
    )
    parser.add_argument(
        "--min-robust-scale",
        type=float,
        default=ReplayConfig.min_robust_scale,
        help="Floor for 1.4826*MAD to avoid near-zero thresholds. Default: 0.005",
    )
    parser.add_argument(
        "--quorum",
        type=int,
        default=ReplayConfig.quorum,
        help="Number of RX votes required for presence. Default: 2",
    )
    parser.add_argument(
        "--min-effect",
        type=float,
        default=ReplayConfig.min_effect,
        help="Minimum positive train-pair median shift for a reliable RX. Default: 0.005",
    )
    parser.add_argument(
        "--min-reliability",
        type=float,
        default=ReplayConfig.min_reliability,
        help="Minimum train-only Youden reliability (TPR minus FPR). Default: 0.30",
    )
    parser.add_argument(
        "--min-vote-fraction",
        type=float,
        default=ReplayConfig.min_vote_fraction,
        help="Reliability-weighted RX vote fraction required for presence. Default: 0.35",
    )
    parser.add_argument(
        "--present-persistence-seconds",
        type=float,
        default=ReplayConfig.present_persistence_seconds,
        help="Continuous positive time required before PRESENT_STILL. Default: 2",
    )
    parser.add_argument(
        "--absent-persistence-seconds",
        type=float,
        default=ReplayConfig.absent_persistence_seconds,
        help="Continuous negative time required before returning to ABSENT. Default: 2",
    )
    parser.add_argument(
        "--score-field",
        default=ReplayConfig.score_field,
        help="Numeric node_features field to replay. Default: smoothed_motion_score",
    )
    parser.add_argument(
        "--min-validation-recall",
        type=float,
        default=ReplayConfig.min_validation_recall,
        help="Per-fold still-person recall target. Default: 0.80",
    )
    parser.add_argument(
        "--max-validation-false-positive-rate",
        type=float,
        default=ReplayConfig.max_validation_false_positive_rate,
        help="Per-fold empty-room false-positive limit. Default: 0.10",
    )
    parser.add_argument(
        "--json-indent",
        type=int,
        default=2,
        help="JSON indentation. Use 0 for compact output. Default: 2",
    )
    return parser.parse_args()


def validate_config(config: ReplayConfig) -> None:
    if config.window_seconds <= 0:
        raise ValueError("--window-seconds must be larger than 0")
    if config.min_samples_per_window <= 0:
        raise ValueError("--min-samples-per-window must be larger than 0")
    if config.min_calibration_blocks <= 0:
        raise ValueError("--min-calibration-blocks must be larger than 0")
    if config.mad_multiplier < 0:
        raise ValueError("--mad-multiplier must not be negative")
    if config.min_robust_scale <= 0:
        raise ValueError("--min-robust-scale must be larger than 0")
    if config.quorum <= 0:
        raise ValueError("--quorum must be larger than 0")
    if config.min_effect < 0:
        raise ValueError("--min-effect must not be negative")
    for name, value in (
        ("--min-reliability", config.min_reliability),
        ("--min-vote-fraction", config.min_vote_fraction),
        ("--min-validation-recall", config.min_validation_recall),
        (
            "--max-validation-false-positive-rate",
            config.max_validation_false_positive_rate,
        ),
    ):
        if not 0 <= value <= 1:
            raise ValueError(f"{name} must be between 0 and 1")
    if config.present_persistence_seconds < 0:
        raise ValueError("--present-persistence-seconds must not be negative")
    if config.absent_persistence_seconds < 0:
        raise ValueError("--absent-persistence-seconds must not be negative")
    if not config.score_field:
        raise ValueError("--score-field must not be empty")


def resolve_raw_path(path: Path) -> Path:
    return path / "raw_sensing.jsonl" if path.is_dir() else path


def load_run(path: Path, label: str, score_field: str) -> LabeledRun:
    raw_path = resolve_raw_path(path).resolve()
    if not raw_path.is_file():
        raise FileNotFoundError(f"Run file not found: {raw_path}")

    samples: list[RunSample] = []
    previous_elapsed = -math.inf
    with raw_path.open(encoding="utf-8") as handle:
        for line_number, line in enumerate(handle, start=1):
            if not line.strip():
                continue
            try:
                envelope = json.loads(line)
            except json.JSONDecodeError as error:
                raise ValueError(
                    f"{raw_path}:{line_number}: invalid JSON: {error.msg}"
                ) from error

            if not isinstance(envelope, dict):
                raise ValueError(f"{raw_path}:{line_number}: expected a JSON object")

            data = envelope.get("data", envelope)
            node_features = data.get("node_features") if isinstance(data, dict) else None
            if not isinstance(node_features, list):
                raise ValueError(
                    f"{raw_path}:{line_number}: missing data.node_features list"
                )

            elapsed_value = envelope.get("elapsed_s")
            if elapsed_value is None and isinstance(data, dict):
                elapsed_value = data.get("timestamp")
            elapsed_s = finite_float(
                elapsed_value,
                f"{raw_path}:{line_number}: missing or invalid elapsed time",
            )
            if elapsed_s < previous_elapsed:
                raise ValueError(
                    f"{raw_path}:{line_number}: elapsed time moved backwards"
                )
            previous_elapsed = elapsed_s

            scores: dict[int, float] = {}
            for node in node_features:
                if not isinstance(node, dict) or node.get("stale") is True:
                    continue
                node_id_value = node.get("node_id")
                score_value = node.get(score_field)
                if node_id_value is None or score_value is None:
                    continue
                try:
                    node_id = int(node_id_value)
                    score = float(score_value)
                except (TypeError, ValueError):
                    continue
                if math.isfinite(score):
                    scores[node_id] = score

            if scores:
                samples.append(RunSample(elapsed_s=elapsed_s, scores=scores))

    if not samples:
        raise ValueError(f"{raw_path}: no usable {score_field!r} samples")
    return LabeledRun(
        label=label,
        path=raw_path,
        samples=tuple(samples),
        recording_duration_s=load_recording_duration(raw_path),
    )


def load_recording_duration(raw_path: Path) -> float | None:
    metadata_path = raw_path.parent / "metadata.json"
    if not metadata_path.is_file():
        return None
    try:
        metadata = json.loads(metadata_path.read_text(encoding="utf-8"))
        duration_s = float(metadata["duration_s"])
    except (KeyError, TypeError, ValueError, json.JSONDecodeError):
        return None
    return duration_s if math.isfinite(duration_s) and duration_s > 0 else None


def finite_float(value: Any, error_message: str) -> float:
    try:
        number = float(value)
    except (TypeError, ValueError) as error:
        raise ValueError(error_message) from error
    if not math.isfinite(number):
        raise ValueError(error_message)
    return number


def rolling_mean_samples(
    run: LabeledRun,
    window_seconds: float,
    min_samples_per_window: int,
) -> tuple[WindowSample, ...]:
    histories: dict[int, deque[tuple[float, float]]] = {}
    sums: dict[int, float] = {}
    first_seen: dict[int, float] = {}
    output: list[WindowSample] = []

    for sample in run.samples:
        for node_id, score in sample.scores.items():
            history = histories.setdefault(node_id, deque())
            history.append((sample.elapsed_s, score))
            sums[node_id] = sums.get(node_id, 0.0) + score
            first_seen.setdefault(node_id, sample.elapsed_s)

            cutoff = sample.elapsed_s - window_seconds
            while history and history[0][0] < cutoff:
                _, removed_score = history.popleft()
                sums[node_id] -= removed_score

        rolling_scores: dict[int, float] = {}
        for node_id in sample.scores:
            history = histories[node_id]
            has_full_window = (
                sample.elapsed_s - first_seen[node_id] >= window_seconds
            )
            if has_full_window and len(history) >= min_samples_per_window:
                rolling_scores[node_id] = sums[node_id] / len(history)

        if rolling_scores:
            output.append(
                WindowSample(
                    elapsed_s=sample.elapsed_s,
                    scores=rolling_scores,
                )
            )

    if not output:
        raise ValueError(
            f"{run.path}: no complete {window_seconds:g}-second rolling windows"
        )
    return tuple(output)


def calibration_block_mean_samples(
    run: LabeledRun,
    block_seconds: float,
) -> tuple[WindowSample, ...]:
    """
    Build the same non-overlapping complete-block means used by the Rust D5 fit.

    A recording's requested duration is used when metadata.json is available,
    because it represents the calibration start/stop interval. Without metadata,
    one median sample interval is added to the observed span.
    """
    if block_seconds <= 0:
        raise ValueError("calibration block duration must be larger than 0")

    started_at = run.samples[0].elapsed_s
    observed_duration = 0.0
    if len(run.samples) >= 2:
        intervals = [
            current.elapsed_s - previous.elapsed_s
            for previous, current in zip(run.samples, run.samples[1:])
            if current.elapsed_s > previous.elapsed_s
        ]
        inferred_interval = statistics.median(intervals) if intervals else 0.0
        observed_duration = (
            run.samples[-1].elapsed_s - started_at + inferred_interval
        )
    if run.recording_duration_s is not None and observed_duration > 0:
        calibration_duration = min(
            run.recording_duration_s,
            observed_duration,
        )
    elif run.recording_duration_s is not None:
        calibration_duration = run.recording_duration_s
    else:
        calibration_duration = observed_duration

    complete_block_count = math.floor(calibration_duration / block_seconds)
    if complete_block_count <= 0:
        raise ValueError(
            f"{run.path}: no complete {block_seconds:g}-second calibration block"
        )

    sums: list[dict[int, float]] = [
        {} for _ in range(complete_block_count)
    ]
    counts: list[dict[int, int]] = [
        {} for _ in range(complete_block_count)
    ]
    calibration_end = started_at + calibration_duration
    for sample in run.samples:
        if sample.elapsed_s < started_at or sample.elapsed_s >= calibration_end:
            continue
        block_index = math.floor(
            (sample.elapsed_s - started_at) / block_seconds
        )
        if block_index >= complete_block_count:
            continue
        for node_id, score in sample.scores.items():
            sums[block_index][node_id] = (
                sums[block_index].get(node_id, 0.0) + score
            )
            counts[block_index][node_id] = (
                counts[block_index].get(node_id, 0) + 1
            )

    output: list[WindowSample] = []
    for block_index in range(complete_block_count):
        scores = {
            node_id: total / counts[block_index][node_id]
            for node_id, total in sums[block_index].items()
            if counts[block_index][node_id] > 0
        }
        if scores:
            output.append(
                WindowSample(
                    elapsed_s=started_at + (block_index + 1) * block_seconds,
                    scores=scores,
                )
            )
    if not output:
        raise ValueError(f"{run.path}: complete calibration blocks had no samples")
    return tuple(output)


def values_by_node(samples: Iterable[WindowSample]) -> dict[int, list[float]]:
    values: dict[int, list[float]] = {}
    for sample in samples:
        for node_id, score in sample.scores.items():
            values.setdefault(node_id, []).append(score)
    return values


def percentile(values: list[float], fraction: float) -> float:
    if not values:
        raise ValueError("percentile requires at least one value")
    ordered = sorted(values)
    position = (len(ordered) - 1) * fraction
    lower_index = math.floor(position)
    upper_index = math.ceil(position)
    if lower_index == upper_index:
        return ordered[lower_index]
    weight = position - lower_index
    return ordered[lower_index] * (1 - weight) + ordered[upper_index] * weight


def rates_at_threshold(
    empty_values: list[float],
    present_values: list[float],
    threshold: float,
) -> tuple[float, float, float]:
    false_positive_rate = sum(value > threshold for value in empty_values) / len(
        empty_values
    )
    true_positive_rate = sum(value > threshold for value in present_values) / len(
        present_values
    )
    balanced_accuracy = (
        true_positive_rate + (1.0 - false_positive_rate)
    ) / 2.0
    return true_positive_rate, false_positive_rate, balanced_accuracy


def threshold_candidates(values: Iterable[float]) -> list[float]:
    ordered = sorted(set(values))
    if not ordered:
        return []
    candidates = [math.nextafter(ordered[0], -math.inf)]
    candidates.extend(
        (lower + upper) / 2.0
        for lower, upper in zip(ordered, ordered[1:])
    )
    candidates.append(ordered[-1])
    return candidates


def best_node_model(
    node_id: int,
    empty_values: list[float],
    present_values: list[float],
    config: ReplayConfig,
) -> NodeModel:
    empty_median = statistics.median(empty_values)
    present_median = statistics.median(present_values)
    effect = present_median - empty_median
    empty_iqr = percentile(empty_values, 0.75) - percentile(empty_values, 0.25)

    best: tuple[tuple[float, float, float, float], float, float, float, float] | None
    best = None
    for threshold in threshold_candidates([*empty_values, *present_values]):
        true_positive_rate, false_positive_rate, balanced_accuracy = (
            rates_at_threshold(empty_values, present_values, threshold)
        )
        rank = (
            balanced_accuracy,
            -false_positive_rate,
            true_positive_rate,
            threshold,
        )
        if best is None or rank > best[0]:
            best = (
                rank,
                threshold,
                true_positive_rate,
                false_positive_rate,
                balanced_accuracy,
            )

    if best is None:
        raise ValueError(f"RX{node_id}: no threshold candidates")
    _, threshold, true_positive_rate, false_positive_rate, balanced_accuracy = best
    reliability = max(0.0, true_positive_rate - false_positive_rate)

    rejection_reason: str | None = None
    if effect < config.min_effect:
        rejection_reason = "effect_below_minimum"
    elif reliability < config.min_reliability:
        rejection_reason = "reliability_below_minimum"

    return NodeModel(
        node_id=node_id,
        threshold=threshold,
        empty_median=empty_median,
        present_median=present_median,
        effect=effect,
        empty_iqr=empty_iqr,
        training_true_positive_rate=true_positive_rate,
        training_false_positive_rate=false_positive_rate,
        training_balanced_accuracy=balanced_accuracy,
        reliability=reliability,
        selected=rejection_reason is None,
        rejection_reason=rejection_reason,
    )


def fit_model(
    empty_samples: tuple[WindowSample, ...],
    present_samples: tuple[WindowSample, ...],
    config: ReplayConfig,
) -> PresenceModel:
    empty_by_node = values_by_node(empty_samples)
    present_by_node = values_by_node(present_samples)
    common_nodes = sorted(empty_by_node.keys() & present_by_node.keys())
    if not common_nodes:
        raise ValueError("Training pair contains no common RX nodes")

    nodes = tuple(
        best_node_model(
            node_id,
            empty_by_node[node_id],
            present_by_node[node_id],
            config,
        )
        for node_id in common_nodes
    )
    return PresenceModel(nodes=nodes)


def persistent_states(
    samples: tuple[WindowSample, ...],
    raw_states: list[bool],
    present_seconds: float,
    absent_seconds: float,
) -> list[bool]:
    state = False
    positive_since: float | None = None
    negative_since: float | None = None
    output: list[bool] = []

    for sample, raw_state in zip(samples, raw_states):
        if raw_state:
            negative_since = None
            if state:
                positive_since = None
            else:
                if positive_since is None:
                    positive_since = sample.elapsed_s
                if sample.elapsed_s - positive_since >= present_seconds:
                    state = True
                    positive_since = None
        else:
            positive_since = None
            if not state:
                negative_since = None
            else:
                if negative_since is None:
                    negative_since = sample.elapsed_s
                if sample.elapsed_s - negative_since >= absent_seconds:
                    state = False
                    negative_since = None
        output.append(state)

    return output


def evaluate_run(
    model: PresenceModel,
    samples: tuple[WindowSample, ...],
    expected_present: bool,
    config: ReplayConfig,
) -> dict[str, Any]:
    selected_nodes = model.selected_nodes
    total_weight = sum(node.reliability for node in selected_nodes)
    raw_states: list[bool] = []
    vote_fractions: list[float] = []

    for sample in samples:
        positive_weight = sum(
            node.reliability
            for node in selected_nodes
            if sample.scores.get(node.node_id, -math.inf) > node.threshold
        )
        vote_fraction = positive_weight / total_weight if total_weight > 0 else 0.0
        vote_fractions.append(vote_fraction)
        raw_states.append(
            total_weight > 0 and vote_fraction >= config.min_vote_fraction
        )

    states = persistent_states(
        samples,
        raw_states,
        config.present_persistence_seconds,
        config.absent_persistence_seconds,
    )
    positive_count = sum(states)
    positive_rate = positive_count / len(states)
    first_positive_elapsed_s = next(
        (
            sample.elapsed_s - samples[0].elapsed_s
            for sample, state in zip(samples, states)
            if state
        ),
        None,
    )

    result: dict[str, Any] = {
        "expected": "present" if expected_present else "empty",
        "evaluated_samples": len(samples),
        "raw_positive_samples": sum(raw_states),
        "raw_positive_rate": sum(raw_states) / len(raw_states),
        "persistent_positive_samples": positive_count,
        "persistent_positive_rate": positive_rate,
        "first_positive_after_window_s": first_positive_elapsed_s,
        "mean_vote_fraction": statistics.fmean(vote_fractions),
        "model_available": bool(selected_nodes),
    }
    if expected_present:
        result["recall"] = positive_rate
    else:
        result["false_positive_rate"] = positive_rate
        result["specificity"] = 1.0 - positive_rate
    return result


def fit_empty_only_model(
    calibration_blocks: tuple[WindowSample, ...],
    config: ReplayConfig,
) -> EmptyOnlyModel:
    calibration_by_node = values_by_node(calibration_blocks)
    if not calibration_by_node:
        raise ValueError("Empty-room calibration contains no RX nodes")

    nodes: list[EmptyBaselineNode] = []
    for node_id in sorted(calibration_by_node):
        values = calibration_by_node[node_id]
        if len(values) < config.min_calibration_blocks:
            continue
        median = statistics.median(values)
        mad = statistics.median(abs(value - median) for value in values)
        robust_scale = max(1.4826 * mad, config.min_robust_scale)
        threshold = median + config.mad_multiplier * robust_scale
        calibration_exceedance_rate = sum(
            value > threshold for value in values
        ) / len(values)
        nodes.append(
            EmptyBaselineNode(
                node_id=node_id,
                median=median,
                mad=mad,
                robust_scale=robust_scale,
                threshold=threshold,
                calibration_exceedance_rate=calibration_exceedance_rate,
                block_count=len(values),
            )
        )
    if not nodes:
        raise ValueError(
            "No RX has enough complete empty-room calibration blocks"
        )
    return EmptyOnlyModel(nodes=tuple(nodes))


def evaluate_empty_only_run(
    model: EmptyOnlyModel,
    samples: tuple[WindowSample, ...],
    expected_present: bool,
    config: ReplayConfig,
) -> dict[str, Any]:
    raw_states: list[bool] = []
    vote_counts: list[int] = []
    model_available = len(model.nodes) >= config.quorum

    for sample in samples:
        vote_count = sum(
            sample.scores.get(node.node_id, -math.inf) > node.threshold
            for node in model.nodes
        )
        vote_counts.append(vote_count)
        raw_states.append(model_available and vote_count >= config.quorum)

    states = persistent_states(
        samples,
        raw_states,
        config.present_persistence_seconds,
        config.absent_persistence_seconds,
    )
    positive_count = sum(states)
    positive_rate = positive_count / len(states)
    first_positive_elapsed_s = next(
        (
            sample.elapsed_s - samples[0].elapsed_s
            for sample, state in zip(samples, states)
            if state
        ),
        None,
    )

    result: dict[str, Any] = {
        "expected": "present" if expected_present else "empty",
        "evaluated_samples": len(samples),
        "raw_positive_samples": sum(raw_states),
        "raw_positive_rate": sum(raw_states) / len(raw_states),
        "persistent_positive_samples": positive_count,
        "persistent_positive_rate": positive_rate,
        "first_positive_after_window_s": first_positive_elapsed_s,
        "mean_rx_votes": statistics.fmean(vote_counts),
        "model_available": model_available,
    }
    if expected_present:
        result["recall"] = positive_rate
    else:
        result["false_positive_rate"] = positive_rate
        result["specificity"] = 1.0 - positive_rate
    return result


def evaluate_empty_only_pair(
    model: EmptyOnlyModel,
    empty_samples: tuple[WindowSample, ...],
    present_samples: tuple[WindowSample, ...],
    config: ReplayConfig,
) -> dict[str, Any]:
    empty_result = evaluate_empty_only_run(model, empty_samples, False, config)
    present_result = evaluate_empty_only_run(model, present_samples, True, config)
    false_positive_rate = float(empty_result["false_positive_rate"])
    recall = float(present_result["recall"])
    return {
        "empty": empty_result,
        "present": present_result,
        "false_positive_rate": false_positive_rate,
        "recall": recall,
        "balanced_accuracy": ((1.0 - false_positive_rate) + recall) / 2.0,
    }


def evaluate_pair(
    model: PresenceModel,
    empty_samples: tuple[WindowSample, ...],
    present_samples: tuple[WindowSample, ...],
    config: ReplayConfig,
) -> dict[str, Any]:
    empty_result = evaluate_run(model, empty_samples, False, config)
    present_result = evaluate_run(model, present_samples, True, config)
    false_positive_rate = float(empty_result["false_positive_rate"])
    recall = float(present_result["recall"])
    return {
        "empty": empty_result,
        "present": present_result,
        "false_positive_rate": false_positive_rate,
        "recall": recall,
        "balanced_accuracy": ((1.0 - false_positive_rate) + recall) / 2.0,
    }


def model_to_dict(model: PresenceModel) -> dict[str, Any]:
    return {
        "selected_node_ids": [node.node_id for node in model.selected_nodes],
        "nodes": [asdict(node) for node in model.nodes],
    }


def empty_only_model_to_dict(model: EmptyOnlyModel) -> dict[str, Any]:
    return {
        "node_ids": [node.node_id for node in model.nodes],
        "nodes": [asdict(node) for node in model.nodes],
    }


def run_metadata(run: LabeledRun, windows: tuple[WindowSample, ...]) -> dict[str, Any]:
    return {
        "label": run.label,
        "path": str(run.path),
        "raw_samples": len(run.samples),
        "rolling_samples": len(windows),
        "duration_s": run.samples[-1].elapsed_s - run.samples[0].elapsed_s,
        "node_ids": sorted(
            {
                node_id
                for sample in run.samples
                for node_id in sample.scores
            }
        ),
    }


def evaluate_fold(
    name: str,
    train_pair_name: str,
    validation_pair_name: str,
    train_empty: tuple[WindowSample, ...],
    train_present: tuple[WindowSample, ...],
    validation_empty: tuple[WindowSample, ...],
    validation_present: tuple[WindowSample, ...],
    config: ReplayConfig,
) -> dict[str, Any]:
    model = fit_model(train_empty, train_present, config)
    training_metrics = evaluate_pair(model, train_empty, train_present, config)
    validation_metrics = evaluate_pair(
        model,
        validation_empty,
        validation_present,
        config,
    )
    passed = bool(model.selected_nodes) and (
        validation_metrics["false_positive_rate"]
        <= config.max_validation_false_positive_rate
        and validation_metrics["recall"] >= config.min_validation_recall
    )
    return {
        "name": name,
        "train_pair": train_pair_name,
        "validation_pair": validation_pair_name,
        "model": model_to_dict(model),
        "training": training_metrics,
        "validation": validation_metrics,
        "passed": passed,
    }


def evaluate_empty_only_fold(
    name: str,
    calibration_name: str,
    validation_pair_name: str,
    calibration_blocks: tuple[WindowSample, ...],
    calibration_live_windows: tuple[WindowSample, ...],
    validation_empty: tuple[WindowSample, ...],
    validation_present: tuple[WindowSample, ...],
    config: ReplayConfig,
) -> dict[str, Any]:
    model = fit_empty_only_model(calibration_blocks, config)
    calibration_metrics = evaluate_empty_only_run(
        model,
        calibration_live_windows,
        False,
        config,
    )
    validation_metrics = evaluate_empty_only_pair(
        model,
        validation_empty,
        validation_present,
        config,
    )
    passed = bool(validation_metrics["empty"]["model_available"]) and (
        validation_metrics["false_positive_rate"]
        <= config.max_validation_false_positive_rate
        and validation_metrics["recall"] >= config.min_validation_recall
    )
    return {
        "name": name,
        "calibration_run": calibration_name,
        "validation_pair": validation_pair_name,
        "model": empty_only_model_to_dict(model),
        "calibration_empty": calibration_metrics,
        "validation": validation_metrics,
        "passed": passed,
    }


def macro_validation(folds: list[dict[str, Any]]) -> dict[str, float]:
    return {
        "false_positive_rate": statistics.fmean(
            float(fold["validation"]["false_positive_rate"]) for fold in folds
        ),
        "recall": statistics.fmean(
            float(fold["validation"]["recall"]) for fold in folds
        ),
        "balanced_accuracy": statistics.fmean(
            float(fold["validation"]["balanced_accuracy"]) for fold in folds
        ),
    }


def evaluate_d5(
    pair1_empty: LabeledRun,
    pair1_present: LabeledRun,
    pair2_empty: LabeledRun,
    pair2_present: LabeledRun,
    config: ReplayConfig,
) -> dict[str, Any]:
    validate_config(config)
    runs = (pair1_empty, pair1_present, pair2_empty, pair2_present)
    pair1_empty_windows = rolling_mean_samples(
        pair1_empty,
        config.window_seconds,
        config.min_samples_per_window,
    )
    pair1_present_windows = rolling_mean_samples(
        pair1_present,
        config.window_seconds,
        config.min_samples_per_window,
    )
    pair2_empty_windows = rolling_mean_samples(
        pair2_empty,
        config.window_seconds,
        config.min_samples_per_window,
    )
    pair2_present_windows = rolling_mean_samples(
        pair2_present,
        config.window_seconds,
        config.min_samples_per_window,
    )
    pair1_empty_calibration_blocks = calibration_block_mean_samples(
        pair1_empty,
        config.window_seconds,
    )
    pair2_empty_calibration_blocks = calibration_block_mean_samples(
        pair2_empty,
        config.window_seconds,
    )
    run_windows = (
        (pair1_empty, pair1_empty_windows),
        (pair1_present, pair1_present_windows),
        (pair2_empty, pair2_empty_windows),
        (pair2_present, pair2_present_windows),
    )

    empty_only_folds = [
        evaluate_empty_only_fold(
            name="calibrate_E0c_only_validate_E0d_E1b",
            calibration_name=pair1_empty.label,
            validation_pair_name="E0d_empty_and_E1b_present",
            calibration_blocks=pair1_empty_calibration_blocks,
            calibration_live_windows=pair1_empty_windows,
            validation_empty=pair2_empty_windows,
            validation_present=pair2_present_windows,
            config=config,
        ),
        evaluate_empty_only_fold(
            name="calibrate_E0d_only_validate_E0c_E1",
            calibration_name=pair2_empty.label,
            validation_pair_name="E0c_empty_and_E1_present",
            calibration_blocks=pair2_empty_calibration_blocks,
            calibration_live_windows=pair2_empty_windows,
            validation_empty=pair1_empty_windows,
            validation_present=pair1_present_windows,
            config=config,
        ),
    ]
    empty_only_passed = all(bool(fold["passed"]) for fold in empty_only_folds)

    rejected_config = replace(
        config,
        mad_multiplier=3.0,
        min_robust_scale=0.002,
    )
    rejected_empty_only_folds = [
        evaluate_empty_only_fold(
            name="calibrate_E0c_only_validate_E0d_E1b",
            calibration_name=pair1_empty.label,
            validation_pair_name="E0d_empty_and_E1b_present",
            calibration_blocks=pair1_empty_calibration_blocks,
            calibration_live_windows=pair1_empty_windows,
            validation_empty=pair2_empty_windows,
            validation_present=pair2_present_windows,
            config=rejected_config,
        ),
        evaluate_empty_only_fold(
            name="calibrate_E0d_only_validate_E0c_E1",
            calibration_name=pair2_empty.label,
            validation_pair_name="E0c_empty_and_E1_present",
            calibration_blocks=pair2_empty_calibration_blocks,
            calibration_live_windows=pair2_empty_windows,
            validation_empty=pair1_empty_windows,
            validation_present=pair1_present_windows,
            config=rejected_config,
        ),
    ]

    supervised_folds = [
        evaluate_fold(
            name="train_pair1_validate_pair2",
            train_pair_name="pair1",
            validation_pair_name="pair2",
            train_empty=pair1_empty_windows,
            train_present=pair1_present_windows,
            validation_empty=pair2_empty_windows,
            validation_present=pair2_present_windows,
            config=config,
        ),
        evaluate_fold(
            name="train_pair2_validate_pair1",
            train_pair_name="pair2",
            validation_pair_name="pair1",
            train_empty=pair2_empty_windows,
            train_present=pair2_present_windows,
            validation_empty=pair1_empty_windows,
            validation_present=pair1_present_windows,
            config=config,
        ),
    ]
    supervised_passed = all(bool(fold["passed"]) for fold in supervised_folds)

    return {
        "schema_version": 2,
        "method": "D5_empty_only_robust_quorum_with_negative_control",
        "passed": empty_only_passed,
        "config": asdict(config),
        "runs": {
            run.label: run_metadata(run, windows)
            for run, windows in run_windows
        },
        "deployment_candidate": {
            "method": (
                "empty_only_per_rx_median_mad_fixed_threshold_and_quorum"
            ),
            "rule_status": "independently_audited_and_pre_specified",
            "uses_present_data_for_fit": False,
            "primary_fold": empty_only_folds[0]["name"],
            "primary_passed": empty_only_folds[0]["passed"],
            "passed": empty_only_passed,
            "calibration_block_counts": {
                pair1_empty.label: len(pair1_empty_calibration_blocks),
                pair2_empty.label: len(pair2_empty_calibration_blocks),
            },
            "folds": empty_only_folds,
            "validation_macro": macro_validation(empty_only_folds),
        },
        "rejected_sensitivity_variant": {
            "status": "rejected",
            "reason": (
                "Retained for comparison only; it is not the independently "
                "audited pre-specified deployment rule."
            ),
            "config_overrides": {
                "mad_multiplier": rejected_config.mad_multiplier,
                "min_robust_scale": rejected_config.min_robust_scale,
            },
            "common_config": {
                "window_seconds": rejected_config.window_seconds,
                "quorum": rejected_config.quorum,
                "present_persistence_seconds": (
                    rejected_config.present_persistence_seconds
                ),
                "absent_persistence_seconds": (
                    rejected_config.absent_persistence_seconds
                ),
            },
            "folds": rejected_empty_only_folds,
            "validation_macro": macro_validation(rejected_empty_only_folds),
        },
        "supervised_negative_control": {
            "method": "train_on_empty_and_present_validate_swapped",
            "uses_present_data_for_fit": True,
            "passed": supervised_passed,
            "folds": supervised_folds,
            "validation_macro": macro_validation(supervised_folds),
        },
        "decision": {
            "passed": empty_only_passed,
            "rule": (
                "Both empty-only calibration folds need an available quorum, "
                f"false_positive_rate <= "
                f"{config.max_validation_false_positive_rate:.3f}, and "
                f"recall >= {config.min_validation_recall:.3f}."
            ),
        },
    }


def format_percent(value: float) -> str:
    return f"{100.0 * value:.1f}%"


def print_text_summary(result: dict[str, Any], output: TextIO = sys.stderr) -> None:
    status = "BESTANDEN" if result["passed"] else "NICHT BESTANDEN"
    deployment = result["deployment_candidate"]
    print(f"D5 Leerraum-only-Kandidat: {status}", file=output)
    for fold in deployment["folds"]:
        validation = fold["validation"]
        print(
            f"- Kalibrierung {fold['calibration_run']} -> "
            f"{fold['validation_pair']}: "
            f"Leerraum-FPR {format_percent(validation['false_positive_rate'])}, "
            f"Still-Recall {format_percent(validation['recall'])}, "
            f"BA {format_percent(validation['balanced_accuracy'])}",
            file=output,
        )
    macro = deployment["validation_macro"]
    print(
        "- Validierungsmittel: "
        f"FPR {format_percent(macro['false_positive_rate'])}, "
        f"Recall {format_percent(macro['recall'])}, "
        f"BA {format_percent(macro['balanced_accuracy'])}",
        file=output,
    )
    negative_control = result["supervised_negative_control"]
    rejected = result["rejected_sensitivity_variant"]
    rejected_primary = rejected["folds"][0]["validation"]
    rejected_reverse = rejected["folds"][1]["validation"]
    print(
        "- Verworfene z=3/floor=0,002-Variante: "
        f"Primary FPR {format_percent(rejected_primary['false_positive_rate'])}, "
        f"Recall {format_percent(rejected_primary['recall'])}; "
        f"Reverse FPR {format_percent(rejected_reverse['false_positive_rate'])}, "
        f"Recall {format_percent(rejected_reverse['recall'])}",
        file=output,
    )
    print(
        "- Supervised Negativkontrolle: "
        f"{'BESTANDEN' if negative_control['passed'] else 'NICHT BESTANDEN'}, "
        f"FPR "
        f"{format_percent(negative_control['validation_macro']['false_positive_rate'])}, "
        f"Recall "
        f"{format_percent(negative_control['validation_macro']['recall'])}",
        file=output,
    )


def main() -> int:
    args = parse_args()
    config = ReplayConfig(
        window_seconds=args.window_seconds,
        min_samples_per_window=args.min_samples_per_window,
        min_calibration_blocks=args.min_calibration_blocks,
        mad_multiplier=args.mad_multiplier,
        min_robust_scale=args.min_robust_scale,
        quorum=args.quorum,
        min_effect=args.min_effect,
        min_reliability=args.min_reliability,
        min_vote_fraction=args.min_vote_fraction,
        present_persistence_seconds=args.present_persistence_seconds,
        absent_persistence_seconds=args.absent_persistence_seconds,
        score_field=args.score_field,
        min_validation_recall=args.min_validation_recall,
        max_validation_false_positive_rate=args.max_validation_false_positive_rate,
    )

    try:
        validate_config(config)
        pair1_empty = load_run(args.pair1_empty, "E0c_empty", config.score_field)
        pair1_present = load_run(
            args.pair1_present,
            "E1_present",
            config.score_field,
        )
        pair2_empty = load_run(args.pair2_empty, "E0d_empty", config.score_field)
        pair2_present = load_run(
            args.pair2_present,
            "E1b_present",
            config.score_field,
        )
        result = evaluate_d5(
            pair1_empty,
            pair1_present,
            pair2_empty,
            pair2_present,
            config,
        )
    except (FileNotFoundError, ValueError) as error:
        print(f"ERROR: {error}", file=sys.stderr)
        return 2

    indent = None if args.json_indent == 0 else args.json_indent
    print(json.dumps(result, ensure_ascii=False, indent=indent, sort_keys=True))
    print_text_summary(result)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

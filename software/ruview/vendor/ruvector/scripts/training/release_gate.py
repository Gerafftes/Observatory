#!/usr/bin/env python3
"""
Release gate automation for RuvLTRA model training.

Implements the 7 ship/no-ship criteria from ADR-129 (Section 3.2).
A model version is approved for publishing only if ALL gates pass.

Usage:
    python release_gate.py --model-path /path/to/model --results-dir /path/to/results
    python release_gate.py --results-dir ./results  # model-path is optional

Exit codes:
    0 - All gates PASS (ship)
    1 - One or more gates FAIL (no-ship)
"""

import argparse
import json
import sys
from pathlib import Path


# ---------------------------------------------------------------------------
# Threshold configuration per model size
# ---------------------------------------------------------------------------

THRESHOLDS = {
    "0.5B": {
        "humaneval_pass1_absolute": 0.45,   # G1: >=45% absolute
        "humaneval_pass1_delta": 0.05,       # G1: >=5pp improvement
        "routing_accuracy_min": 0.80,        # G2: >=80%
        "wikitext2_ppl_increase_max": 0.05,  # G3: <5% increase
        "tq_compression_min": 8.0,           # G4: >=8x
        "tq_ppl_delta_max": 0.01,            # G4: <1%
        "long_context_ppl_max": 20.0,        # G5: <20 PPL at 16K
        "contamination_max": 0,              # G6: zero contamination
        "tok_per_sec_min": 80,               # G7: >=80 tok/s
    },
    "3B": {
        "humaneval_pass1_absolute": 0.55,    # G1: >=55% absolute
        "humaneval_pass1_delta": 0.05,       # G1: >=5pp improvement
        "routing_accuracy_min": 0.80,        # G2: >=80%
        "wikitext2_ppl_increase_max": 0.05,  # G3: <5% increase
        "tq_compression_min": 8.0,           # G4: >=8x
        "tq_ppl_delta_max": 0.01,            # G4: <1%
        "long_context_ppl_max": 20.0,        # G5: <20 PPL at 16K
        "contamination_max": 0,              # G6: zero contamination
        "tok_per_sec_min": 40,               # G7: >=40 tok/s
    },
}


# ---------------------------------------------------------------------------
# Gate check functions
# ---------------------------------------------------------------------------

def check_g1(baseline, candidate, thresholds):
    """G1: Code quality - HumanEval pass@1."""
    base_score = baseline["humaneval_pass1"]
    cand_score = candidate["humaneval_pass1"]
    delta = cand_score - base_score
    abs_threshold = thresholds["humaneval_pass1_absolute"]
    delta_threshold = thresholds["humaneval_pass1_delta"]

    meets_absolute = cand_score >= abs_threshold
    meets_delta = delta >= delta_threshold

    passed = meets_absolute or meets_delta
    detail = (
        f"pass@1={cand_score:.1%} (baseline={base_score:.1%}, "
        f"delta={delta:+.1%}); "
        f"need >={abs_threshold:.0%} absolute OR >={delta_threshold:.0%} improvement"
    )
    return passed, detail


def check_g2(baseline, candidate, thresholds):
    """G2: Routing no-regression - accuracy >= 80%."""
    accuracy = candidate["routing_accuracy"]
    minimum = thresholds["routing_accuracy_min"]

    passed = accuracy >= minimum
    detail = (
        f"routing_accuracy={accuracy:.1%}; "
        f"need >={minimum:.0%}"
    )
    return passed, detail


def check_g3(baseline, candidate, thresholds):
    """G3: General no-regression - wikitext-2 perplexity increase < 5%."""
    base_ppl = baseline["wikitext2_ppl"]
    cand_ppl = candidate["wikitext2_ppl"]
    max_increase = thresholds["wikitext2_ppl_increase_max"]

    if base_ppl > 0:
        pct_increase = (cand_ppl - base_ppl) / base_ppl
    else:
        pct_increase = 0.0

    passed = pct_increase < max_increase
    detail = (
        f"wikitext2_ppl={cand_ppl:.2f} (baseline={base_ppl:.2f}, "
        f"increase={pct_increase:+.2%}); "
        f"need <{max_increase:.0%} increase"
    )
    return passed, detail


def check_g4(baseline, candidate, thresholds):
    """G4: TurboQuant memory - compression >= 8x, perplexity delta < 1%."""
    compression = candidate["tq_compression"]
    ppl_delta = candidate["tq_ppl_delta"]
    min_compression = thresholds["tq_compression_min"]
    max_ppl_delta = thresholds["tq_ppl_delta_max"]

    passed = compression >= min_compression and ppl_delta < max_ppl_delta
    detail = (
        f"compression={compression:.1f}x (need >={min_compression:.0f}x), "
        f"ppl_delta={ppl_delta:.3%} (need <{max_ppl_delta:.0%})"
    )
    return passed, detail


def check_g5(baseline, candidate, thresholds):
    """G5: Long context - perplexity at 16K < 20 PPL."""
    ppl = candidate["long_context_ppl"]
    maximum = thresholds["long_context_ppl_max"]

    passed = ppl < maximum
    detail = f"long_context_ppl={ppl:.1f} PPL; need <{maximum:.0f} PPL"
    return passed, detail


def check_g6(baseline, candidate, thresholds):
    """G6: Contamination - zero eval contamination."""
    count = candidate["contamination_count"]
    maximum = thresholds["contamination_max"]

    passed = count <= maximum
    detail = f"contamination_count={count}; need <={maximum}"
    return passed, detail


def check_g7(baseline, candidate, thresholds):
    """G7: Inference speed - tok/s above minimum."""
    speed = candidate["tok_per_sec"]
    minimum = thresholds["tok_per_sec_min"]

    passed = speed >= minimum
    detail = f"tok/s={speed:.0f}; need >={minimum}"
    return passed, detail


# ---------------------------------------------------------------------------
# Gate runner
# ---------------------------------------------------------------------------

GATES = [
    ("G1", "Code quality (HumanEval pass@1)", check_g1),
    ("G2", "Routing no-regression", check_g2),
    ("G3", "General no-regression (wikitext-2 PPL)", check_g3),
    ("G4", "TurboQuant memory", check_g4),
    ("G5", "Long context", check_g5),
    ("G6", "Contamination", check_g6),
    ("G7", "Inference speed", check_g7),
]


def run_gates(data):
    """Run all 7 release gates and return results."""
    model_size = data["model_size"]
    if model_size not in THRESHOLDS:
        supported = ", ".join(sorted(THRESHOLDS.keys()))
        print(
            f"ERROR: Unknown model_size '{model_size}'. "
            f"Supported: {supported}",
            file=sys.stderr,
        )
        sys.exit(1)

    thresholds = THRESHOLDS[model_size]
    baseline = data["baseline"]
    candidate = data["candidate"]

    results = []
    for gate_id, gate_name, check_fn in GATES:
        passed, detail = check_fn(baseline, candidate, thresholds)
        results.append({
            "gate": gate_id,
            "name": gate_name,
            "passed": passed,
            "detail": detail,
        })

    return results


def print_results(results, model_size):
    """Print formatted gate results and overall verdict."""
    print("=" * 72)
    print(f"  RuvLTRA Release Gate Report  |  Model size: {model_size}")
    print("=" * 72)

    all_passed = True
    for r in results:
        status = "PASS" if r["passed"] else "FAIL"
        marker = " " if r["passed"] else ">"
        if not r["passed"]:
            all_passed = False
        print(f"  {marker} [{status}] {r['gate']}: {r['name']}")
        print(f"          {r['detail']}")

    print("-" * 72)
    verdict = "PASS -- ship approved" if all_passed else "FAIL -- do not ship"
    print(f"  Verdict: {verdict}")
    print("=" * 72)

    return all_passed


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main():
    parser = argparse.ArgumentParser(
        description="RuvLTRA release gate checker (ADR-129 Section 3.2)",
    )
    parser.add_argument(
        "--model-path",
        type=str,
        default=None,
        help="Path to the model directory (informational, logged in output)",
    )
    parser.add_argument(
        "--results-dir",
        type=str,
        required=True,
        help="Directory containing gate_results.json",
    )
    parser.add_argument(
        "--output-json",
        type=str,
        default=None,
        help="Optional path to write JSON report",
    )
    args = parser.parse_args()

    results_file = Path(args.results_dir) / "gate_results.json"
    if not results_file.exists():
        print(
            f"ERROR: {results_file} not found. "
            f"Run evaluation scripts first to generate gate results.",
            file=sys.stderr,
        )
        sys.exit(1)

    with open(results_file, "r") as f:
        data = json.load(f)

    if args.model_path:
        print(f"Model: {args.model_path}")

    results = run_gates(data)
    all_passed = print_results(results, data["model_size"])

    if args.output_json:
        report = {
            "model_size": data["model_size"],
            "model_path": args.model_path,
            "verdict": "PASS" if all_passed else "FAIL",
            "gates": results,
        }
        output_path = Path(args.output_json)
        output_path.parent.mkdir(parents=True, exist_ok=True)
        with open(output_path, "w") as f:
            json.dump(report, f, indent=2)
        print(f"\nJSON report written to: {output_path}")

    sys.exit(0 if all_passed else 1)


if __name__ == "__main__":
    main()

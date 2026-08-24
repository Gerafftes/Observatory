#!/usr/bin/env python3
"""
Eval contamination check for RuvLTRA training corpus.

Implements the 13-gram overlap check from ADR-129 Section 2.2:
  - Takes a training corpus (JSONL) and an eval set (JSONL or plain text)
  - Computes 13-gram overlap between each training record and eval instances
  - Reports any contaminated records (>50% 13-gram overlap with any eval instance)
  - Contaminated records should be removed from training

Usage:
    python contamination_check.py \\
        --corpus data/training/corpus.jsonl \\
        --eval data/eval/humaneval.jsonl \\
        [--ngram-size 13] \\
        [--threshold 0.5] \\
        [--output data/training/contamination_report.json]

The eval file can be:
  - JSONL with a "text" or "prompt" or "content" field per line
  - Plain text with one eval instance per line
"""

import argparse
import hashlib
import json
import sys
from collections import defaultdict
from datetime import datetime, timezone
from pathlib import Path


def extract_ngrams(text: str, n: int) -> set[tuple[str, ...]]:
    """Extract character-level n-grams from whitespace-normalized text."""
    # Normalize: lowercase, collapse whitespace
    tokens = text.lower().split()
    if len(tokens) < n:
        return set()
    return {tuple(tokens[i : i + n]) for i in range(len(tokens) - n + 1)}


def ngram_overlap_ratio(
    train_ngrams: set[tuple[str, ...]],
    eval_ngrams: set[tuple[str, ...]],
) -> float:
    """Fraction of train record's n-grams that appear in the eval instance."""
    if not train_ngrams:
        return 0.0
    intersection = train_ngrams & eval_ngrams
    return len(intersection) / len(train_ngrams)


def load_eval_set(eval_path: Path) -> list[dict]:
    """Load eval instances from JSONL or plain text."""
    instances = []
    text_content = eval_path.read_text(encoding="utf-8")

    for line_no, line in enumerate(text_content.splitlines(), 1):
        line = line.strip()
        if not line:
            continue

        # Try JSONL first
        try:
            obj = json.loads(line)
            text = (
                obj.get("text")
                or obj.get("prompt")
                or obj.get("content")
                or obj.get("input")
                or ""
            )
            if text:
                instances.append({
                    "eval_id": obj.get("id", obj.get("task_id", f"eval-{line_no}")),
                    "text": text,
                })
                continue
        except json.JSONDecodeError:
            pass

        # Fall back to plain text
        instances.append({
            "eval_id": f"eval-{line_no}",
            "text": line,
        })

    return instances


def load_corpus(corpus_path: Path) -> list[dict]:
    """Load training corpus from JSONL."""
    records = []
    for line in corpus_path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            records.append(json.loads(line))
        except json.JSONDecodeError:
            continue
    return records


def run_contamination_check(
    corpus: list[dict],
    eval_set: list[dict],
    ngram_size: int = 13,
    threshold: float = 0.5,
) -> dict:
    """
    Check each training record for n-gram overlap with eval instances.

    Returns a report dict with contaminated records and summary stats.
    """
    # Pre-compute eval n-grams
    print(f"[contamination] Building {ngram_size}-gram index for {len(eval_set)} eval instances...")
    eval_ngrams_list = []
    for inst in eval_set:
        ngrams = extract_ngrams(inst["text"], ngram_size)
        eval_ngrams_list.append((inst["eval_id"], ngrams))

    # Build a combined eval n-gram set for fast initial screening
    all_eval_ngrams: set[tuple[str, ...]] = set()
    for _, ngrams in eval_ngrams_list:
        all_eval_ngrams.update(ngrams)

    print(f"[contamination] Eval index: {len(all_eval_ngrams):,} unique {ngram_size}-grams")
    print(f"[contamination] Checking {len(corpus)} training records...")

    contaminated = []
    checked = 0

    for rec in corpus:
        text = rec.get("text", "")
        train_ngrams = extract_ngrams(text, ngram_size)

        if not train_ngrams:
            continue

        # Fast screen: check overlap with combined eval set first
        combined_ratio = ngram_overlap_ratio(train_ngrams, all_eval_ngrams)
        if combined_ratio < threshold * 0.5:
            # Very unlikely to be contaminated with any single eval instance
            checked += 1
            continue

        # Detailed check: find the specific eval instance(s) with high overlap
        max_overlap = 0.0
        max_eval_id = ""
        matching_evals = []

        for eval_id, eval_ngrams in eval_ngrams_list:
            ratio = ngram_overlap_ratio(train_ngrams, eval_ngrams)
            if ratio > max_overlap:
                max_overlap = ratio
                max_eval_id = eval_id
            if ratio >= threshold:
                matching_evals.append({
                    "eval_id": eval_id,
                    "overlap_ratio": round(ratio, 4),
                })

        if max_overlap >= threshold:
            contaminated.append({
                "record_id": rec.get("id", "unknown"),
                "source": rec.get("source", "unknown"),
                "content_hash": rec.get("content_hash", ""),
                "max_overlap": round(max_overlap, 4),
                "max_overlap_eval_id": max_eval_id,
                "matching_evals": matching_evals,
                "text_preview": text[:200],
            })

        checked += 1
        if checked % 500 == 0:
            print(f"  ... checked {checked}/{len(corpus)} records")

    report = {
        "check_date": datetime.now(timezone.utc).isoformat(),
        "ngram_size": ngram_size,
        "overlap_threshold": threshold,
        "corpus_records": len(corpus),
        "eval_instances": len(eval_set),
        "records_checked": checked,
        "contaminated_count": len(contaminated),
        "contamination_rate": round(len(contaminated) / max(len(corpus), 1), 4),
        "verdict": "FAIL" if contaminated else "PASS",
        "contaminated_records": contaminated,
    }

    return report


def print_report(report: dict) -> None:
    """Pretty-print the contamination report."""
    print("\n" + "=" * 60)
    print("CONTAMINATION CHECK REPORT")
    print("=" * 60)
    print(f"Date:              {report['check_date']}")
    print(f"N-gram size:       {report['ngram_size']}")
    print(f"Overlap threshold: {report['overlap_threshold']}")
    print(f"Corpus records:    {report['corpus_records']}")
    print(f"Eval instances:    {report['eval_instances']}")
    print(f"Records checked:   {report['records_checked']}")
    print(f"Contaminated:      {report['contaminated_count']}")
    print(f"Contamination rate:{report['contamination_rate']:.2%}")
    print(f"Verdict:           {report['verdict']}")

    if report["contaminated_records"]:
        print("\nContaminated records:")
        for i, rec in enumerate(report["contaminated_records"], 1):
            print(f"\n  [{i}] Record {rec['record_id']} (source: {rec['source']})")
            print(f"      Max overlap: {rec['max_overlap']:.2%} with {rec['max_overlap_eval_id']}")
            print(f"      Matching eval instances: {len(rec['matching_evals'])}")
            print(f"      Preview: {rec['text_preview'][:100]}...")
    else:
        print("\nNo contamination detected. Training corpus is clean.")

    print("=" * 60)


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Check training corpus for eval set contamination (ADR-129 Section 2.2)"
    )
    parser.add_argument(
        "--corpus",
        required=True,
        type=Path,
        help="Path to training corpus JSONL file",
    )
    parser.add_argument(
        "--eval",
        required=True,
        type=Path,
        help="Path to eval set (JSONL with text/prompt/content field, or plain text)",
    )
    parser.add_argument(
        "--ngram-size",
        type=int,
        default=13,
        help="N-gram size for overlap check (default: 13)",
    )
    parser.add_argument(
        "--threshold",
        type=float,
        default=0.5,
        help="Overlap ratio threshold to flag contamination (default: 0.5)",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=None,
        help="Path to write JSON report (default: data/training/contamination_report.json)",
    )

    args = parser.parse_args()

    if not args.corpus.exists():
        print(f"Error: corpus file not found: {args.corpus}", file=sys.stderr)
        sys.exit(1)
    if not args.eval.exists():
        print(f"Error: eval file not found: {args.eval}", file=sys.stderr)
        sys.exit(1)

    # Load data
    corpus = load_corpus(args.corpus)
    eval_set = load_eval_set(args.eval)

    if not corpus:
        print("Error: corpus is empty.", file=sys.stderr)
        sys.exit(1)
    if not eval_set:
        print("Error: eval set is empty.", file=sys.stderr)
        sys.exit(1)

    # Run check
    report = run_contamination_check(
        corpus=corpus,
        eval_set=eval_set,
        ngram_size=args.ngram_size,
        threshold=args.threshold,
    )

    # Output
    print_report(report)

    output_path = args.output or Path("data/training/contamination_report.json")
    output_path.parent.mkdir(parents=True, exist_ok=True)
    with open(output_path, "w", encoding="utf-8") as fh:
        json.dump(report, fh, indent=2, ensure_ascii=False)
    print(f"\nReport written to: {output_path}")

    # Exit code: non-zero if contamination found (for CI gating)
    if report["verdict"] == "FAIL":
        print(f"\nWARNING: {report['contaminated_count']} contaminated records found. "
              "Remove them before training (ADR-129 G6).")
        sys.exit(1)


if __name__ == "__main__":
    main()

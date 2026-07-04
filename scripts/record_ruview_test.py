#!/usr/bin/env python3
"""
Record RuView CSI test runs to files.

Outputs per run:
  - raw_sensing.jsonl: complete /api/v1/sensing/latest responses
  - summary.csv: compact values for quick plotting/spreadsheet work
  - metadata.json: run setup and timing
  - errors.log: request/parse errors during the run
"""

from __future__ import annotations

import argparse
import csv
import json
import re
import sys
import time
import urllib.error
import urllib.request
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


SCRIPT_DIR = Path(__file__).resolve().parent
PROJECT_DIR = SCRIPT_DIR.parent
DEFAULT_OUT_ROOT = PROJECT_DIR / "data" / "raw"
DEFAULT_URL = "http://localhost:8080/api/v1/sensing/latest"


CSV_FIELDS = [
    "sample_index",
    "timestamp_local",
    "timestamp_utc",
    "elapsed_s",
    "tick",
    "tick_changed",
    "node_count",
    "node_ids",
    "missing_nodes",
    "estimated_persons",
    "presence",
    "motion_level",
    "classification_confidence",
    "mean_rssi",
    "variance",
    "motion_band_power",
    "breathing_band_power",
    "dominant_freq_hz",
    "spectral_power",
    "change_points",
    "breathing_rate_bpm",
    "breathing_confidence",
    "heart_rate_bpm",
    "heartbeat_confidence",
    "signal_quality",
]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Record RuView /api/v1/sensing/latest into JSONL and CSV files."
    )
    parser.add_argument(
        "--label",
        required=True,
        help='Short test label, e.g. "A0_leerer_raum" or "A3_atmung_sitzend".',
    )
    parser.add_argument(
        "--duration",
        type=float,
        required=True,
        help="Recording duration in seconds.",
    )
    parser.add_argument(
        "--interval",
        type=float,
        default=1.0,
        help="Polling interval in seconds. Default: 1.0",
    )
    parser.add_argument(
        "--url",
        default=DEFAULT_URL,
        help=f"RuView API URL. Default: {DEFAULT_URL}",
    )
    parser.add_argument(
        "--out-root",
        type=Path,
        default=DEFAULT_OUT_ROOT,
        help=f"Output root. Default: {DEFAULT_OUT_ROOT}",
    )
    parser.add_argument(
        "--expected-nodes",
        default="1,2,3,4",
        help='Comma-separated expected node IDs. Default: "1,2,3,4".',
    )
    parser.add_argument(
        "--notes",
        default="",
        help="Optional short note stored in metadata.json.",
    )
    return parser.parse_args()


def sanitize_label(label: str) -> str:
    cleaned = re.sub(r"[^A-Za-z0-9._-]+", "_", label.strip())
    cleaned = cleaned.strip("._-")
    return cleaned or "test"


def parse_expected_nodes(value: str) -> list[int]:
    if not value.strip():
        return []

    nodes: list[int] = []
    for part in value.split(","):
        part = part.strip()
        if not part:
            continue
        nodes.append(int(part))
    return sorted(set(nodes))


def now_local_iso() -> str:
    return datetime.now().astimezone().isoformat(timespec="seconds")


def now_utc_iso() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="seconds")


def fetch_json(url: str) -> dict[str, Any]:
    request = urllib.request.Request(url, headers={"Accept": "application/json"})
    with urllib.request.urlopen(request, timeout=3) as response:
        body = response.read()
    data = json.loads(body.decode("utf-8"))
    if not isinstance(data, dict):
        raise ValueError("RuView response was not a JSON object")
    return data


def number_or_empty(value: Any) -> Any:
    return "" if value is None else value


def collect_node_ids(data: dict[str, Any]) -> list[int]:
    node_ids: set[int] = set()

    for node in data.get("nodes", []) or []:
        if isinstance(node, dict) and node.get("node_id") is not None:
            node_ids.add(int(node["node_id"]))

    for node in data.get("node_features", []) or []:
        if isinstance(node, dict) and node.get("node_id") is not None:
            node_ids.add(int(node["node_id"]))

    return sorted(node_ids)


def build_summary_row(
    data: dict[str, Any],
    sample_index: int,
    elapsed_s: float,
    previous_tick: Any,
    expected_nodes: list[int],
) -> dict[str, Any]:
    features = data.get("features", {}) or {}
    classification = data.get("classification", {}) or {}
    vital_signs = data.get("vital_signs", {}) or {}
    node_ids = collect_node_ids(data)
    missing_nodes = [node for node in expected_nodes if node not in node_ids]
    tick = data.get("tick")

    return {
        "sample_index": sample_index,
        "timestamp_local": now_local_iso(),
        "timestamp_utc": now_utc_iso(),
        "elapsed_s": round(elapsed_s, 3),
        "tick": number_or_empty(tick),
        "tick_changed": "" if previous_tick is None else tick != previous_tick,
        "node_count": len(node_ids),
        "node_ids": " ".join(str(node_id) for node_id in node_ids),
        "missing_nodes": " ".join(str(node_id) for node_id in missing_nodes),
        "estimated_persons": number_or_empty(data.get("estimated_persons")),
        "presence": number_or_empty(classification.get("presence")),
        "motion_level": number_or_empty(classification.get("motion_level")),
        "classification_confidence": number_or_empty(classification.get("confidence")),
        "mean_rssi": number_or_empty(features.get("mean_rssi")),
        "variance": number_or_empty(features.get("variance")),
        "motion_band_power": number_or_empty(features.get("motion_band_power")),
        "breathing_band_power": number_or_empty(features.get("breathing_band_power")),
        "dominant_freq_hz": number_or_empty(features.get("dominant_freq_hz")),
        "spectral_power": number_or_empty(features.get("spectral_power")),
        "change_points": number_or_empty(features.get("change_points")),
        "breathing_rate_bpm": number_or_empty(vital_signs.get("breathing_rate_bpm")),
        "breathing_confidence": number_or_empty(vital_signs.get("breathing_confidence")),
        "heart_rate_bpm": number_or_empty(vital_signs.get("heart_rate_bpm")),
        "heartbeat_confidence": number_or_empty(vital_signs.get("heartbeat_confidence")),
        "signal_quality": number_or_empty(vital_signs.get("signal_quality")),
    }


def write_metadata(
    run_dir: Path,
    args: argparse.Namespace,
    expected_nodes: list[int],
    started_local: str,
    started_utc: str,
) -> None:
    metadata = {
        "label": args.label,
        "duration_s": args.duration,
        "interval_s": args.interval,
        "url": args.url,
        "expected_nodes": expected_nodes,
        "notes": args.notes,
        "started_local": started_local,
        "started_utc": started_utc,
        "outputs": {
            "raw_jsonl": "raw_sensing.jsonl",
            "summary_csv": "summary.csv",
            "errors_log": "errors.log",
        },
    }
    (run_dir / "metadata.json").write_text(
        json.dumps(metadata, indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )


def print_live(row: dict[str, Any]) -> None:
    print(
        "sample",
        row["sample_index"],
        "tick",
        row["tick"],
        "nodes",
        f"[{row['node_ids']}]",
        "missing",
        f"[{row['missing_nodes']}]",
        "presence",
        row["presence"],
        "motion",
        row["motion_level"],
        "rssi",
        row["mean_rssi"],
        flush=True,
    )


def main() -> int:
    args = parse_args()
    if args.duration <= 0:
        print("ERROR: --duration must be larger than 0", file=sys.stderr)
        return 2
    if args.interval <= 0:
        print("ERROR: --interval must be larger than 0", file=sys.stderr)
        return 2

    expected_nodes = parse_expected_nodes(args.expected_nodes)
    label = sanitize_label(args.label)
    run_stamp = datetime.now().astimezone().strftime("%Y-%m-%d_%H-%M-%S")
    run_dir = args.out_root / f"{run_stamp}_{label}"
    run_dir.mkdir(parents=True, exist_ok=False)

    started_local = now_local_iso()
    started_utc = now_utc_iso()
    write_metadata(run_dir, args, expected_nodes, started_local, started_utc)

    raw_path = run_dir / "raw_sensing.jsonl"
    csv_path = run_dir / "summary.csv"
    error_path = run_dir / "errors.log"

    print(f"Recording: {args.label}")
    print(f"Output: {run_dir}")
    print("Stop early with Ctrl+C if needed.")

    start = time.monotonic()
    deadline = start + args.duration
    sample_index = 0
    previous_tick: Any = None

    with raw_path.open("w", encoding="utf-8") as raw_file, csv_path.open(
        "w", encoding="utf-8", newline=""
    ) as csv_file, error_path.open("w", encoding="utf-8") as error_file:
        writer = csv.DictWriter(csv_file, fieldnames=CSV_FIELDS)
        writer.writeheader()

        try:
            while time.monotonic() <= deadline:
                sample_started = time.monotonic()
                elapsed_s = sample_started - start

                try:
                    data = fetch_json(args.url)
                    envelope = {
                        "sample_index": sample_index,
                        "recorded_local": now_local_iso(),
                        "recorded_utc": now_utc_iso(),
                        "elapsed_s": round(elapsed_s, 3),
                        "data": data,
                    }
                    raw_file.write(json.dumps(envelope, ensure_ascii=False) + "\n")
                    raw_file.flush()

                    row = build_summary_row(
                        data=data,
                        sample_index=sample_index,
                        elapsed_s=elapsed_s,
                        previous_tick=previous_tick,
                        expected_nodes=expected_nodes,
                    )
                    writer.writerow(row)
                    csv_file.flush()
                    print_live(row)
                    previous_tick = data.get("tick")
                    sample_index += 1
                except (urllib.error.URLError, TimeoutError, json.JSONDecodeError, ValueError) as exc:
                    error_file.write(f"{now_local_iso()} sample={sample_index} error={exc}\n")
                    error_file.flush()
                    print(f"ERROR sample {sample_index}: {exc}", file=sys.stderr, flush=True)
                    sample_index += 1

                sleep_s = args.interval - (time.monotonic() - sample_started)
                if sleep_s > 0:
                    time.sleep(sleep_s)
        except KeyboardInterrupt:
            print("\nStopped by user.")

    print(f"Done. Files written to: {run_dir}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

#!/usr/bin/env python3
"""Replay Raw-CSI-v2 timing evidence through the 60 ms queue matcher.

This tool is deliberately diagnostic-only. Historical captures can exercise
the synchronizer and distinguish host-coherent from mesh-coherent quartets,
but they cannot validate a sealed live setup or produce calibration evidence.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Iterable


DEFAULT_GUARD_US = 60_000
REQUIRED_RX_IDS = (1, 2, 3, 4)


class AuditError(RuntimeError):
    """The capture cannot support a fail-closed timing audit."""


@dataclass(frozen=True)
class TimingFrame:
    rx_id: int
    sequence: int
    host_monotonic_us: int
    mesh_timestamp_us: int | None


@dataclass(frozen=True)
class Quartet:
    frames: dict[int, TimingFrame]
    basis: str
    host_spread_us: int
    mesh_spread_us: int | None

    @property
    def selected_spread_us(self) -> int:
        if self.basis == "mesh":
            if self.mesh_spread_us is None:
                raise AssertionError("mesh basis requires a complete mesh spread")
            return self.mesh_spread_us
        return self.host_spread_us


def require_int(record: dict[str, Any], field: str, line_number: int) -> int:
    value = record.get(field)
    if not isinstance(value, int) or isinstance(value, bool) or value < 0:
        raise AuditError(f"line {line_number}: {field} must be a non-negative integer")
    return value


def load_capture(path: Path) -> dict[int, list[TimingFrame]]:
    frames_by_rx = {rx_id: [] for rx_id in REQUIRED_RX_IDS}
    last_host_us: dict[int, int] = {}
    try:
        with path.open(encoding="utf-8") as handle:
            for line_number, line in enumerate(handle, start=1):
                if not line.strip():
                    continue
                try:
                    record = json.loads(line)
                except json.JSONDecodeError as error:
                    raise AuditError(f"line {line_number}: invalid JSON: {error}") from error
                if not isinstance(record, dict):
                    raise AuditError(f"line {line_number}: record must be a JSON object")
                if record.get("schema_version") != 2:
                    raise AuditError(f"line {line_number}: expected Raw-CSI schema_version 2")

                rx_id = require_int(record, "rx_id", line_number)
                if rx_id not in frames_by_rx:
                    raise AuditError(f"line {line_number}: unexpected rx_id {rx_id}")
                host_ns = require_int(record, "host_monotonic_ns", line_number)
                sequence = require_int(record, "sequence", line_number)
                mesh_value = record.get("mesh_timestamp_us")
                if mesh_value is not None and (
                    not isinstance(mesh_value, int)
                    or isinstance(mesh_value, bool)
                    or mesh_value < 0
                ):
                    raise AuditError(
                        f"line {line_number}: mesh_timestamp_us must be null or non-negative"
                    )

                host_us = host_ns // 1_000
                previous_host_us = last_host_us.get(rx_id)
                if previous_host_us is not None and host_us < previous_host_us:
                    raise AuditError(
                        f"line {line_number}: RX{rx_id} host time regressed "
                        f"from {previous_host_us} to {host_us} us"
                    )
                last_host_us[rx_id] = host_us
                frames_by_rx[rx_id].append(
                    TimingFrame(
                        rx_id=rx_id,
                        sequence=sequence,
                        host_monotonic_us=host_us,
                        mesh_timestamp_us=mesh_value,
                    )
                )
    except OSError as error:
        raise AuditError(f"cannot read {path}: {error}") from error

    missing = [rx_id for rx_id, frames in frames_by_rx.items() if not frames]
    if missing:
        labels = ", ".join(f"RX{rx_id}" for rx_id in missing)
        raise AuditError(f"capture has no frames for {labels}")
    return frames_by_rx


def spread(values: Iterable[int]) -> int:
    collected = tuple(values)
    return max(collected) - min(collected)


def select_disjoint_quartets(
    frames_by_rx: dict[int, list[TimingFrame]], guard_us: int
) -> tuple[list[Quartet], dict[int, int]]:
    if guard_us <= 0:
        raise AuditError("guard_us must be positive")

    indices = {rx_id: 0 for rx_id in REQUIRED_RX_IDS}
    discarded = {rx_id: 0 for rx_id in REQUIRED_RX_IDS}
    quartets: list[Quartet] = []

    while all(indices[rx_id] < len(frames_by_rx[rx_id]) for rx_id in REQUIRED_RX_IDS):
        candidates = {
            rx_id: frames_by_rx[rx_id][indices[rx_id]] for rx_id in REQUIRED_RX_IDS
        }
        host_spread_us = spread(
            frame.host_monotonic_us for frame in candidates.values()
        )
        if host_spread_us > guard_us:
            earliest_rx = min(
                REQUIRED_RX_IDS,
                key=lambda rx_id: (candidates[rx_id].host_monotonic_us, rx_id),
            )
            indices[earliest_rx] += 1
            discarded[earliest_rx] += 1
            continue

        mesh_values = [frame.mesh_timestamp_us for frame in candidates.values()]
        mesh_spread_us = (
            spread(value for value in mesh_values if value is not None)
            if all(value is not None for value in mesh_values)
            else None
        )
        basis = (
            "mesh"
            if mesh_spread_us is not None and mesh_spread_us <= guard_us
            else "host_monotonic"
        )
        quartets.append(
            Quartet(
                frames=candidates,
                basis=basis,
                host_spread_us=host_spread_us,
                mesh_spread_us=mesh_spread_us,
            )
        )
        for rx_id in REQUIRED_RX_IDS:
            indices[rx_id] += 1

    return quartets, discarded


def percentile(values: list[int], fraction: float) -> float | None:
    if not values:
        return None
    ordered = sorted(values)
    position = (len(ordered) - 1) * fraction
    lower = math.floor(position)
    upper = math.ceil(position)
    if lower == upper:
        return float(ordered[lower])
    weight = position - lower
    return ordered[lower] * (1.0 - weight) + ordered[upper] * weight


def spread_summary(values: list[int]) -> dict[str, float | int | None]:
    return {
        "count": len(values),
        "median_us": percentile(values, 0.5),
        "p95_us": percentile(values, 0.95),
        "max_us": max(values) if values else None,
    }


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    try:
        with path.open("rb") as handle:
            for chunk in iter(lambda: handle.read(1024 * 1024), b""):
                digest.update(chunk)
    except OSError as error:
        raise AuditError(f"cannot hash {path}: {error}") from error
    return digest.hexdigest()


def audit_capture(path: Path, guard_us: int = DEFAULT_GUARD_US) -> dict[str, Any]:
    frames_by_rx = load_capture(path)
    quartets, discarded = select_disjoint_quartets(frames_by_rx, guard_us)
    if not quartets:
        raise AuditError("capture contains no host-coherent four-RX quartet")

    host_spreads = [quartet.host_spread_us for quartet in quartets]
    mesh_spreads = [
        quartet.mesh_spread_us
        for quartet in quartets
        if quartet.mesh_spread_us is not None
    ]
    selected_spreads = [quartet.selected_spread_us for quartet in quartets]
    host_fallback_count = sum(
        quartet.basis == "host_monotonic" for quartet in quartets
    )
    first_host_us = min(frames[0].host_monotonic_us for frames in frames_by_rx.values())
    last_host_us = max(frames[-1].host_monotonic_us for frames in frames_by_rx.values())
    duration_seconds = (last_host_us - first_host_us) / 1_000_000

    return {
        "schema_version": 1,
        "evidence_class": "historical_diagnostic_only",
        "sealed_live_acceptance": False,
        "calibration_eligible": False,
        "input": {
            "path": str(path),
            "sha256": sha256_file(path),
            "duration_seconds": duration_seconds,
            "rx_frame_counts": {
                str(rx_id): len(frames_by_rx[rx_id]) for rx_id in REQUIRED_RX_IDS
            },
        },
        "synchronizer": {
            "guard_us": guard_us,
            "quartet_count": len(quartets),
            "selection_rate_hz": len(quartets) / duration_seconds
            if duration_seconds > 0
            else None,
            "basis_counts": {
                "mesh": len(quartets) - host_fallback_count,
                "host_monotonic": host_fallback_count,
            },
            "discarded_unmatched_frames_by_rx": {
                str(rx_id): discarded[rx_id] for rx_id in REQUIRED_RX_IDS
            },
            "host_spread": spread_summary(host_spreads),
            "mesh_spread": spread_summary(mesh_spreads),
            "selected_spread": spread_summary(selected_spreads),
            "selected_guard_violations": sum(
                selected_spread > guard_us for selected_spread in selected_spreads
            ),
        },
        "interpretation": (
            "This replay tests queue selection against historical timing only. "
            "It does not reproduce live task scheduling or bounded-queue eviction, "
            "and cannot confirm current firmware, network, sync, sealed runtime, "
            "engine-error behavior, collector, calibration, or blind-validation state."
        ),
    }


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("capture", type=Path, help="Raw-CSI-v2 JSONL capture")
    parser.add_argument("--guard-us", type=int, default=DEFAULT_GUARD_US)
    parser.add_argument("--output", type=Path, help="Optional JSON report path")
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(sys.argv[1:] if argv is None else argv)
    try:
        report = audit_capture(args.capture, args.guard_us)
        encoded = json.dumps(report, indent=2, sort_keys=True) + "\n"
        if args.output is None:
            sys.stdout.write(encoded)
        else:
            args.output.write_text(encoded, encoding="utf-8")
    except (AuditError, OSError) as error:
        print(f"audit failed: {error}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

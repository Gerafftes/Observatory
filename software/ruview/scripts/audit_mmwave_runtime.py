#!/usr/bin/env python3
"""Audit one mmWave session together with before/after runtime snapshots.

The session JSONL contains accepted packets only. Server-side status snapshots
are therefore required for a fail-closed verdict: content rejects such as
``room_bounds`` never reach the session writer.
"""

from __future__ import annotations

import argparse
import json
import math
import sys
from collections import deque
from dataclasses import dataclass
from pathlib import Path
from typing import Any


UINT32_MASK = (1 << 32) - 1


class AuditError(RuntimeError):
    """An input artifact is malformed or cannot support the requested audit."""


@dataclass(frozen=True)
class SetupGeometry:
    room_x_mm: int
    room_z_mm: int
    node_id: str
    transform: dict[str, Any]


def load_json(path: Path) -> dict[str, Any]:
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise AuditError(f"cannot read JSON {path}: {error}") from error
    if not isinstance(payload, dict):
        raise AuditError(f"{path} must contain a JSON object")
    return payload


def load_setup(path: Path) -> SetupGeometry:
    payload = load_json(path)
    definition = payload.get("definition", payload)
    if not isinstance(definition, dict):
        raise AuditError("setup definition is missing")
    dimensions = definition.get("room_dimensions_mm")
    mmwave = definition.get("mmwave")
    if (
        not isinstance(dimensions, list)
        or len(dimensions) != 3
        or not all(isinstance(value, int) and value > 0 for value in dimensions)
        or not isinstance(mmwave, dict)
    ):
        raise AuditError("setup has no valid room_dimensions_mm/mmwave definition")
    transform = mmwave.get("transform")
    node_id = mmwave.get("node_id")
    if not isinstance(transform, dict) or not isinstance(node_id, str) or not node_id:
        raise AuditError("setup has no valid mmWave node/transform")
    expected_transform = {
        "local": "x_right_y_forward_mm",
        "room": "x_length_z_width_mm",
        "origin_x_mm": transform.get("origin_x_mm"),
        "origin_z_mm": transform.get("origin_z_mm"),
        "yaw_mdeg": transform.get("yaw_mdeg"),
        "raw_x_inverted": transform.get("raw_x_inverted"),
    }
    if (
        not all(
            isinstance(expected_transform[field], int)
            and not isinstance(expected_transform[field], bool)
            for field in ("origin_x_mm", "origin_z_mm", "yaw_mdeg")
        )
        or not isinstance(expected_transform["raw_x_inverted"], bool)
    ):
        raise AuditError("setup has an invalid mmWave transform")
    return SetupGeometry(
        room_x_mm=dimensions[0],
        room_z_mm=dimensions[2],
        node_id=node_id,
        transform=expected_transform,
    )


def load_records(path: Path) -> list[dict[str, Any]]:
    records: list[dict[str, Any]] = []
    try:
        with path.open(encoding="utf-8") as handle:
            for line_number, line in enumerate(handle, start=1):
                if not line.strip():
                    continue
                try:
                    record = json.loads(line)
                except json.JSONDecodeError as error:
                    raise AuditError(
                        f"{path}:{line_number} contains invalid JSON: {error}"
                    ) from error
                if not isinstance(record, dict) or not isinstance(
                    record.get("packet"), dict
                ):
                    raise AuditError(f"{path}:{line_number} has no packet object")
                records.append(record)
    except OSError as error:
        raise AuditError(f"cannot read JSONL {path}: {error}") from error
    if not records:
        raise AuditError(f"{path} contains no mmWave records")
    return records


def c_lround(value: float) -> int:
    """Match C lround: halfway cases round away from zero."""
    return math.floor(value + 0.5) if value >= 0 else math.ceil(value - 0.5)


def transform_target(
    transform: dict[str, Any], raw_x_mm: int, forward_y_mm: int
) -> tuple[int, int]:
    yaw = transform["yaw_mdeg"] * math.pi / 180_000.0
    local_right = -raw_x_mm if transform["raw_x_inverted"] else raw_x_mm
    room_x_mm = transform["origin_x_mm"] + c_lround(
        forward_y_mm * math.cos(yaw) - local_right * math.sin(yaw)
    )
    room_z_mm = transform["origin_z_mm"] + c_lround(
        forward_y_mm * math.sin(yaw) + local_right * math.cos(yaw)
    )
    return room_x_mm, room_z_mm


def check(check_id: str, passed: bool, detail: str) -> dict[str, Any]:
    return {"id": check_id, "pass": passed, "detail": detail}


def status_payload(payload: dict[str, Any]) -> dict[str, Any]:
    for key in ("mmwave", "status", "data"):
        nested = payload.get(key)
        if isinstance(nested, dict) and (
            "packets_lost" in nested or "reject_reasons" in nested
        ):
            return nested
    return payload


def counter(payload: dict[str, Any], field: str) -> int | None:
    value = payload.get(field)
    return value if isinstance(value, int) and not isinstance(value, bool) else None


def reason_counter(payload: dict[str, Any], reason: str) -> int | None:
    reasons = payload.get("reject_reasons")
    if not isinstance(reasons, dict):
        return None
    value = reasons.get(reason, 0)
    return value if isinstance(value, int) and not isinstance(value, bool) else None


def counter_delta(
    before: dict[str, Any], after: dict[str, Any], field: str
) -> tuple[int | None, str]:
    initial = counter(before, field)
    final = counter(after, field)
    if initial is None or final is None or final < initial:
        return None, f"{field} counters are missing or decreased ({initial!r} -> {final!r})"
    return final - initial, f"{field} {initial} -> {final}"


def reason_delta(
    before: dict[str, Any], after: dict[str, Any], reason: str
) -> tuple[int | None, str]:
    initial = reason_counter(before, reason)
    final = reason_counter(after, reason)
    if initial is None or final is None or final < initial:
        return None, f"{reason} counters are missing or decreased ({initial!r} -> {final!r})"
    return final - initial, f"{reason} {initial} -> {final}"


def radar_gate_passed(status: dict[str, Any]) -> bool:
    preflight = status.get("preflight")
    gates = preflight.get("gates") if isinstance(preflight, dict) else None
    if not isinstance(gates, list):
        return False
    return any(
        isinstance(gate, dict)
        and gate.get("id") == "radar_sequence_loss_free"
        and gate.get("pass") is True
        for gate in gates
    )


def audit_status_snapshots(
    before_payload: dict[str, Any] | None,
    after_payload: dict[str, Any] | None,
    geometry: SetupGeometry,
) -> tuple[list[dict[str, Any]], dict[str, Any]]:
    if before_payload is None or after_payload is None:
        return (
            [
                check(
                    "server_status_snapshots",
                    False,
                    "before/after snapshots are required because rejected packets "
                    "are absent from session JSONL",
                )
            ],
            {"provided": False},
        )

    before = status_payload(before_payload)
    after = status_payload(after_payload)
    rejected_delta, rejected_detail = counter_delta(
        before, after, "packets_rejected"
    )
    lost_delta, lost_detail = counter_delta(before, after, "packets_lost")
    reboot_delta, reboot_detail = counter_delta(before, after, "reboot_count")
    bounds_delta, bounds_detail = reason_delta(before, after, "room_bounds")
    before_boot = counter(before, "boot_id")
    after_boot = counter(after, "boot_id")
    checks = [
        check(
            "server_rejections_delta",
            rejected_delta == 0,
            rejected_detail,
        ),
        check("server_room_bounds_delta", bounds_delta == 0, bounds_detail),
        check("server_packet_loss_delta", lost_delta == 0, lost_detail),
        check("server_reboot_delta", reboot_delta == 0, reboot_detail),
        check(
            "server_boot_identity",
            before_boot is not None and before_boot == after_boot,
            f"boot_id {before_boot!r} -> {after_boot!r}",
        ),
        check(
            "server_transform",
            after.get("transform") == geometry.transform,
            "after snapshot transform matches sealed setup"
            if after.get("transform") == geometry.transform
            else "after snapshot transform does not match sealed setup",
        ),
        check(
            "server_radar_sequence_gate",
            radar_gate_passed(after),
            "radar_sequence_loss_free preflight gate passes"
            if radar_gate_passed(after)
            else "radar_sequence_loss_free preflight gate is absent or failing",
        ),
    ]
    return checks, {
        "provided": True,
        "before_boot_id": before_boot,
        "after_boot_id": after_boot,
        "packets_rejected_delta": rejected_delta,
        "room_bounds_delta": bounds_delta,
        "packets_lost_delta": lost_delta,
        "reboot_count_delta": reboot_delta,
    }


def audit_runtime(
    recording_path: Path,
    setup_path: Path,
    *,
    status_before_path: Path | None = None,
    status_after_path: Path | None = None,
) -> dict[str, Any]:
    geometry = load_setup(setup_path)
    records = load_records(recording_path)
    boot_ids: set[int] = set()
    clock_epochs: set[str] = set()
    transforms: set[str] = set()
    sequence_gaps: list[dict[str, Any]] = []
    reboot_events: list[dict[str, Any]] = []
    out_of_order_events: list[dict[str, Any]] = []
    coordinate_mismatches: list[dict[str, Any]] = []
    bounds_violations: list[dict[str, Any]] = []
    node_mismatches = 0
    transform_mismatches = 0
    timestamp_faults = 0
    duplicate_packets = 0
    previous_boot: int | None = None
    previous_sequence: int | None = None
    previous_monotonic_ns: int | None = None
    recent_sequences: deque[int] = deque(maxlen=64)

    for record_index, record in enumerate(records, start=1):
        packet = record["packet"]
        boot_id = packet.get("boot_id")
        sequence = packet.get("sequence")
        host_monotonic_ns = record.get("host_monotonic_ns")
        transform = packet.get("coordinate_frame")
        targets = packet.get("targets")
        if (
            not isinstance(boot_id, int)
            or isinstance(boot_id, bool)
            or not isinstance(sequence, int)
            or isinstance(sequence, bool)
            or not isinstance(host_monotonic_ns, int)
            or isinstance(host_monotonic_ns, bool)
            or not isinstance(transform, dict)
            or not isinstance(targets, list)
        ):
            raise AuditError(f"record {record_index} has malformed runtime fields")
        boot_ids.add(boot_id)
        epoch = record.get("clock_epoch_id")
        if isinstance(epoch, str):
            clock_epochs.add(epoch)
        transforms.add(json.dumps(transform, sort_keys=True, separators=(",", ":")))
        if packet.get("node_id") != geometry.node_id:
            node_mismatches += 1
        if transform != geometry.transform:
            transform_mismatches += 1
        if previous_monotonic_ns is not None and host_monotonic_ns <= previous_monotonic_ns:
            timestamp_faults += 1

        if previous_boot is not None and boot_id != previous_boot:
            reboot_events.append(
                {
                    "record": record_index,
                    "previous_boot_id": previous_boot,
                    "received_boot_id": boot_id,
                }
            )
            recent_sequences.clear()
        elif previous_sequence is not None:
            if sequence in recent_sequences:
                duplicate_packets += 1
                continue
            expected = (previous_sequence + 1) & UINT32_MASK
            if sequence != expected:
                delta = (sequence - expected) & UINT32_MASK
                event = {
                    "record": record_index,
                    "expected_sequence": expected,
                    "received_sequence": sequence,
                    "host_gap_seconds": (
                        (host_monotonic_ns - previous_monotonic_ns) / 1_000_000_000
                        if previous_monotonic_ns is not None
                        else None
                    ),
                }
                if delta <= UINT32_MASK // 2:
                    event["missing_packets"] = delta
                    sequence_gaps.append(event)
                else:
                    out_of_order_events.append(event)

        for target in targets:
            if not isinstance(target, dict) or target.get("present") is not True:
                continue
            raw_x_mm = target.get("x_mm")
            forward_y_mm = target.get("y_mm")
            room_x_mm = target.get("room_x_mm")
            room_z_mm = target.get("room_z_mm")
            if not all(
                isinstance(value, int) and not isinstance(value, bool)
                for value in (raw_x_mm, forward_y_mm, room_x_mm, room_z_mm)
            ):
                raise AuditError(f"record {record_index} has a malformed target")
            recomputed = transform_target(transform, raw_x_mm, forward_y_mm)
            if recomputed != (room_x_mm, room_z_mm):
                coordinate_mismatches.append(
                    {
                        "record": record_index,
                        "slot": target.get("slot"),
                        "recorded": [room_x_mm, room_z_mm],
                        "recomputed": list(recomputed),
                    }
                )
            if not (
                0 <= room_x_mm <= geometry.room_x_mm
                and 0 <= room_z_mm <= geometry.room_z_mm
            ):
                bounds_violations.append(
                    {
                        "record": record_index,
                        "slot": target.get("slot"),
                        "position_mm": [room_x_mm, room_z_mm],
                    }
                )

        recent_sequences.append(sequence)
        previous_boot = boot_id
        previous_sequence = sequence
        previous_monotonic_ns = host_monotonic_ns

    missing_packets = sum(event["missing_packets"] for event in sequence_gaps)
    checks = [
        check(
            "sealed_node_identity",
            node_mismatches == 0,
            f"node mismatches={node_mismatches}",
        ),
        check(
            "sealed_transform",
            transform_mismatches == 0 and len(transforms) == 1,
            f"transform mismatches={transform_mismatches}, distinct transforms={len(transforms)}",
        ),
        check(
            "coordinate_recomputation",
            not coordinate_mismatches,
            f"coordinate mismatches={len(coordinate_mismatches)}",
        ),
        check(
            "accepted_target_bounds",
            not bounds_violations,
            f"accepted out-of-room targets={len(bounds_violations)}",
        ),
        check(
            "radar_boot_stability",
            not reboot_events and len(boot_ids) == 1,
            f"boot IDs={sorted(boot_ids)}, reboot events={len(reboot_events)}",
        ),
        check(
            "radar_sequence_continuity",
            not sequence_gaps and not out_of_order_events,
            f"gaps={len(sequence_gaps)}, missing={missing_packets}, "
            f"out_of_order={len(out_of_order_events)}",
        ),
        check(
            "host_clock_continuity",
            timestamp_faults == 0 and len(clock_epochs) == 1,
            f"timestamp faults={timestamp_faults}, clock epochs={len(clock_epochs)}",
        ),
    ]
    before = load_json(status_before_path) if status_before_path else None
    after = load_json(status_after_path) if status_after_path else None
    status_checks, status_evidence = audit_status_snapshots(before, after, geometry)
    checks.extend(status_checks)
    passed = all(item["pass"] for item in checks)
    return {
        "schema_version": 1,
        "verdict": "pass" if passed else "fail",
        "recording_path": str(recording_path),
        "setup_path": str(setup_path),
        "records": len(records),
        "boot_ids": sorted(boot_ids),
        "clock_epoch_ids": sorted(clock_epochs),
        "distinct_transforms": [json.loads(item) for item in sorted(transforms)],
        "duplicate_packets": duplicate_packets,
        "missing_packets": missing_packets,
        "sequence_gaps": sequence_gaps,
        "reboot_events": reboot_events,
        "out_of_order_events": out_of_order_events,
        "coordinate_mismatches": coordinate_mismatches,
        "accepted_bounds_violations": bounds_violations,
        "status_evidence": status_evidence,
        "checks": checks,
        "artifact_limitations": [
            "session JSONL contains accepted packets only",
            "server-rejected room_bounds packets require before/after status snapshots",
            "in-room coordinates do not prove the physical yaw without a known target position",
        ],
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--recording", type=Path, required=True)
    parser.add_argument("--setup", type=Path, required=True)
    parser.add_argument("--status-before", type=Path)
    parser.add_argument("--status-after", type=Path)
    parser.add_argument("--output", type=Path)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    report = audit_runtime(
        args.recording,
        args.setup,
        status_before_path=args.status_before,
        status_after_path=args.status_after,
    )
    rendered = json.dumps(report, indent=2, sort_keys=True) + "\n"
    if args.output:
        args.output.write_text(rendered, encoding="utf-8")
    print(rendered, end="")
    return 0 if report["verdict"] == "pass" else 1


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except AuditError as error:
        print(f"FAIL: {error}", file=sys.stderr)
        raise SystemExit(2)

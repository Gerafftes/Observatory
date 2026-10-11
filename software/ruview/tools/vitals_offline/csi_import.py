"""Read Observatory raw CSI recordings for a single, explicitly bound TX1/RX link.

This module does not estimate vital signs. It keeps recorded I/Q values and
packet metadata unchanged so later experiments can choose their own methods.
"""

from __future__ import annotations

lazy import argparse
lazy from collections import Counter
lazy from dataclasses import dataclass, replace
lazy import hashlib
lazy import json
lazy from pathlib import Path
lazy import statistics
lazy import sys
lazy from typing import Iterator


SOURCE_SCHEME = "sha256-ruview-tx-filter-mac-v1"
SOURCE_FLAGS_COMPLETE = 0x07
TRANSIENT_SYNC_FLAG = 0x10
U32_MAX = 0xFFFFFFFF


class RecordingError(ValueError):
    """A row does not satisfy the supported raw recording contract."""


@dataclass(frozen=True)
class Tx1Authority:
    digest: str
    source: str
    setup_id: str | None = None
    setup_sha256: str | None = None


def load_sealed_tx1_setup(path: Path) -> Tx1Authority:
    """Verify a historical setup seal solely to obtain its TX1 filter identity.

    Room geometry, mmWave fields, and setup readiness are not used by the
    vital-sign experiment. This check is independent of the current server
    executable, but requires a self-consistent historical setup artifact.
    """
    setup = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(setup, dict) or setup.get("kind") != "ruview.position-setup":
        raise RecordingError("expected a sealed ruview.position-setup artifact")
    definition = setup.get("definition")
    if not isinstance(definition, dict):
        raise RecordingError("sealed setup has no definition")
    version = definition.get("schema_version")
    if type(version) is not int or version not in (1, 2, 3) or setup.get("schema_version") != version:
        raise RecordingError("unsupported or inconsistent sealed setup schema")
    canonical = (json.dumps(definition, sort_keys=True, indent=2, ensure_ascii=False) + "\n").encode()
    domain = f"ruview.position-setup.v{version}\0".encode()
    expected_seal = hashlib.sha256(domain + canonical).hexdigest()
    if setup.get("setup_sha256") != expected_seal or setup.get("setup_id") != f"setup-{expected_seal[:16]}":
        raise RecordingError("sealed setup hash or ID is invalid")
    transmitter = definition.get("transmitter")
    radio = definition.get("radio")
    if not isinstance(transmitter, dict) or transmitter.get("node_id", "TX1") != "TX1":
        raise RecordingError("sealed setup transmitter is not TX1")
    if not isinstance(radio, dict) or not isinstance(radio.get("tx_filter_identity"), dict):
        raise RecordingError("sealed setup lacks TX filter identity")
    identity = radio["tx_filter_identity"]
    if identity.get("scheme") != SOURCE_SCHEME:
        raise RecordingError("sealed setup uses another TX filter scheme")
    digest = _digest(identity.get("sha256"))
    if digest == "0" * 64:
        raise RecordingError("sealed setup contains an unbound TX filter identity")
    return Tx1Authority(digest, "verified_historical_sealed_setup", setup["setup_id"], expected_seal)


def _integer(value: object, name: str, maximum: int) -> int:
    if type(value) is not int or not 0 <= value <= maximum:
        raise RecordingError(f"{name} must be an integer in 0..{maximum}")
    return value


def _optional_integer(row: dict, name: str, maximum: int) -> int | None:
    value = row.get(name)
    return None if value is None else _integer(value, name, maximum)


def _digest(value: object) -> str:
    if not isinstance(value, str) or len(value) != 64 or any(c not in "0123456789abcdef" for c in value):
        raise RecordingError("TX filter digest must be 64 lowercase hexadecimal characters")
    return value


@dataclass(frozen=True)
class ImportedFrame:
    """One validated frame, with antenna-major signed I/Q in recorded order."""

    row: dict
    line_number: int
    segment_id: int | None = None
    segment_start_reason: str | None = None

    @property
    def complex_samples(self) -> tuple[complex, ...]:
        return tuple(complex(pair["i"], pair["q"]) for pair in self.row["iq_pairs"])

    def samples_for_antenna(self, antenna_index: int) -> tuple[complex, ...]:
        count = self.row["antenna_count"]
        if not 0 <= antenna_index < count:
            raise IndexError(f"antenna index must be in 0..{count - 1}")
        width = self.row["subcarrier_count"]
        start = antenna_index * width
        return self.complex_samples[start : start + width]


def validate_frame(row: object, line_number: int) -> ImportedFrame:
    if not isinstance(row, dict):
        raise RecordingError("frame must be a JSON object")
    schema = _integer(row.get("schema_version"), "schema_version", 65535)
    if schema not in (1, 2):
        raise RecordingError(f"unsupported schema_version {schema}")
    _integer(row.get("host_timestamp_unix_ns"), "host_timestamp_unix_ns", 2**64 - 1)
    mono = _optional_integer(row, "host_monotonic_ns", 2**64 - 1)
    epoch = row.get("clock_epoch_id")
    if epoch is not None and (not isinstance(epoch, str) or not epoch):
        raise RecordingError("clock_epoch_id must be a nonempty string")
    if schema == 2 and (mono is None or epoch is None):
        raise RecordingError("schema v2 requires host_monotonic_ns and clock_epoch_id")
    for name, limit in (
        ("rx_id", 255), ("antenna_count", 255), ("subcarrier_count", 65535),
        ("center_frequency_mhz", U32_MAX), ("sequence", U32_MAX),
        ("ppdu_type", 255), ("flags", 255),
    ):
        _integer(row.get(name), name, limit)
    for name in ("rssi_dbm", "noise_floor_dbm"):
        value = row.get(name)
        if type(value) is not int or not -128 <= value <= 127:
            raise RecordingError(f"{name} must be a signed 8-bit integer")
    if row["antenna_count"] == 0 or row["subcarrier_count"] == 0:
        raise RecordingError("antenna_count and subcarrier_count must be positive")
    _optional_integer(row, "mesh_timestamp_us", 2**64 - 1)
    pairs = row.get("iq_pairs")
    if not isinstance(pairs, list) or len(pairs) != row["antenna_count"] * row["subcarrier_count"]:
        raise RecordingError("iq_pairs length does not match antenna_count * subcarrier_count")
    for pair in pairs:
        if not isinstance(pair, dict) or set(pair) != {"i", "q"}:
            raise RecordingError("each I/Q pair must contain exactly i and q")
        for part in ("i", "q"):
            if type(pair[part]) is not int or not -128 <= pair[part] <= 127:
                raise RecordingError("I/Q values must be signed 8-bit integers")
    binding = row.get("source_binding")
    if binding is not None:
        if not isinstance(binding, dict) or set(binding) != {
            "trailer_version", "flags", "scheme", "tx_filter_sha256"
        }:
            raise RecordingError("source_binding has an unsupported shape")
        if (type(binding["trailer_version"]) is not int
                or binding["trailer_version"] != 1
                or binding["scheme"] != SOURCE_SCHEME):
            raise RecordingError("unsupported source binding version or scheme")
        flags = _integer(binding["flags"], "source_binding.flags", 255)
        digest = _digest(binding["tx_filter_sha256"])
        if not ((flags == 0 and digest == "0" * 64) or
                (flags == SOURCE_FLAGS_COMPLETE and digest != "0" * 64)):
            raise RecordingError("incomplete or inconsistent source binding")
    identity = row.get("protocol_identity")
    if identity is not None:
        if binding is None or not isinstance(identity, dict):
            raise RecordingError("protocol_identity requires a source binding")
        if (type(identity.get("protocol_version")) is not int
                or identity["protocol_version"] not in (2, 3)
                or type(identity.get("role")) is not int or identity["role"] != 1):
            raise RecordingError("unsupported protocol identity version or role")
        if _integer(identity.get("boot_id"), "boot_id", 2**64 - 1) == 0:
            raise RecordingError("boot_id must be nonzero")
        if identity["protocol_version"] == 3:
            for name, limit in (
                ("callback_started_us", 2**64 - 1), ("driver_rx_timestamp_us", U32_MAX),
                ("quality_flags", 255), ("rx_state", 255), ("antenna_index", 255),
                ("csi_config_flags", U32_MAX), ("sync_age_ms", U32_MAX),
            ):
                _optional_integer(identity, name, limit)
            quality = identity.get("quality_flags")
            if quality is not None and quality & ~0x3F:
                raise RecordingError("unknown CSI quality flag")
            gain = identity.get("gain_db")
            if gain is not None and (type(gain) is not int or not -128 <= gain <= 127):
                raise RecordingError("gain_db must be a signed 8-bit integer")
            if quality is not None and bool(quality & 0x08) != (gain is not None):
                raise RecordingError("gain_db availability disagrees with quality flags")
    return ImportedFrame(row, line_number)


def _selected(frame: ImportedFrame, rx_id: int, tx1_digest: str) -> bool:
    binding = frame.row.get("source_binding")
    return (frame.row["rx_id"] == rx_id and binding is not None
            and binding["flags"] == SOURCE_FLAGS_COMPLETE
            and binding["tx_filter_sha256"] == tx1_digest)


def iter_tx1_frames(path: Path, rx_id: int, trusted_tx1_digest: str) -> Iterator[ImportedFrame]:
    """Yield only frames matching an independently verified TX1 filter digest.

    The caller must establish that the expected digest belongs to TX1; this
    cannot be inferred from a recording's RX ID, UDP source, or filename.
    """
    if not 1 <= rx_id <= 4:
        raise ValueError("rx_id must be RX1..RX4")
    trusted_tx1_digest = _digest(trusted_tx1_digest)
    if trusted_tx1_digest == "0" * 64:
        raise ValueError("an unbound all-zero digest cannot identify TX1")
    segment: _Segment | None = None
    segment_id = 0
    pending_break: str | None = None
    with path.open(encoding="utf-8") as stream:
        for line_number, line in enumerate(stream, 1):
            try:
                frame = validate_frame(json.loads(line), line_number)
            except (json.JSONDecodeError, RecordingError) as error:
                raise RecordingError(f"line {line_number}: {error}") from error
            if frame.row["rx_id"] != rx_id:
                continue
            if not _selected(frame, rx_id, trusted_tx1_digest):
                pending_break = "unselected_target_rx_frame"
                continue
            reason = pending_break or (segment.incompatibility(frame) if segment else "first_selected_frame")
            if reason:
                segment_id += 1
                segment = _Segment(frame, reason)
            else:
                segment.add(frame)
            pending_break = None
            yield replace(frame, segment_id=segment_id, segment_start_reason=reason)


def _quantile(values: list[int], fraction: float) -> float | None:
    if not values:
        return None
    ordered = sorted(values)
    position = (len(ordered) - 1) * fraction
    lower = int(position)
    upper = min(lower + 1, len(ordered) - 1)
    return ordered[lower] + (ordered[upper] - ordered[lower]) * (position - lower)


def _numeric_summary(values: list[int]) -> dict | None:
    if not values:
        return None
    return {"count": len(values), "min": min(values), "median": statistics.median(values), "max": max(values)}


def _grid(row: dict) -> dict:
    identity = row.get("protocol_identity") or {}
    return {
        "center_frequency_mhz": row["center_frequency_mhz"],
        "antenna_count": row["antenna_count"],
        "subcarrier_count": row["subcarrier_count"],
        "ppdu_type": row["ppdu_type"],
        "layout_flags": row["flags"] & ~TRANSIENT_SYNC_FLAG,
        "protocol_version": identity.get("protocol_version"),
        "csi_config_flags": identity.get("csi_config_flags"),
        "antenna_index": identity.get("antenna_index"),
    }


def _clock(row: dict) -> tuple[str, int, str | None]:
    if row.get("host_monotonic_ns") is not None and row.get("clock_epoch_id"):
        return "host_monotonic_ns", row["host_monotonic_ns"], row["clock_epoch_id"]
    return "host_timestamp_unix_ns", row["host_timestamp_unix_ns"], None


class _Segment:
    def __init__(self, frame: ImportedFrame, reason: str):
        row = frame.row
        identity = row.get("protocol_identity") or {}
        self.first = frame
        self.last = frame
        self.frame_count = 1
        self.reason = reason
        self.grid = _grid(row)
        self.schema_version = row["schema_version"]
        self.boot_id = identity.get("boot_id")
        self.time_basis, self.first_time, self.clock_epoch_id = _clock(row)
        self.last_time = self.first_time
        self.intervals: list[int] = []
        self.callback_intervals_us: list[int] = []
        self.driver_intervals_us: list[int] = []
        self.mesh_intervals_us: list[int] = []
        self.duplicate_host_timestamps = 0
        self.duplicate_sequences = 0
        self.sequence_gaps = 0
        self.unobserved_sequence_ids = 0
        self.sequence_wraps = 0
        self.driver_timestamp_wraps = 0
        self.mesh_time_reversals = 0

    def incompatibility(self, frame: ImportedFrame) -> str | None:
        row = frame.row
        previous = self.last.row
        identity = row.get("protocol_identity") or {}
        prior_identity = previous.get("protocol_identity") or {}
        time_basis, current_time, epoch = _clock(row)
        for changed, name in (
            (row["schema_version"] != self.schema_version, "schema_change"),
            (_grid(row) != self.grid, "grid_or_protocol_change"),
            (identity.get("boot_id") != self.boot_id, "receiver_boot_change"),
            (time_basis != self.time_basis or epoch != self.clock_epoch_id, "host_clock_epoch_change"),
            (current_time < self.last_time, "host_time_reversal"),
        ):
            if changed:
                return name
        if time_basis == "host_monotonic_ns":
            wall_delta = row["host_timestamp_unix_ns"] - previous["host_timestamp_unix_ns"]
            if abs(wall_delta - (current_time - self.last_time)) > 500_000_000:
                return "host_utc_mapping_jump"
        sequence = row["sequence"]
        last_sequence = previous["sequence"]
        if sequence == last_sequence and row["iq_pairs"] != previous["iq_pairs"]:
            return "sequence_reused_with_different_iq"
        if sequence < last_sequence and not _u32_wrap(last_sequence, sequence):
            return "sequence_reset_or_reorder"
        callback = identity.get("callback_started_us")
        prior_callback = prior_identity.get("callback_started_us")
        if callback is not None and prior_callback is not None and callback < prior_callback:
            return "receiver_callback_time_reversal"
        driver = identity.get("driver_rx_timestamp_us")
        prior_driver = prior_identity.get("driver_rx_timestamp_us")
        if driver is not None and prior_driver is not None and driver < prior_driver:
            if not _u32_wrap(prior_driver, driver):
                return "driver_timestamp_reset_or_reorder"
        return None

    def add(self, frame: ImportedFrame) -> None:
        row = frame.row
        previous = self.last.row
        _, current_time, _ = _clock(row)
        interval = current_time - self.last_time
        if interval > 0:
            self.intervals.append(interval)
        else:
            self.duplicate_host_timestamps += 1
        sequence = row["sequence"]
        prior_sequence = previous["sequence"]
        delta = (sequence - prior_sequence) & U32_MAX
        if sequence < prior_sequence:
            self.sequence_wraps += 1
        if delta == 0:
            self.duplicate_sequences += 1
        elif delta > 1:
            self.sequence_gaps += 1
            self.unobserved_sequence_ids += delta - 1
        identity = row.get("protocol_identity") or {}
        prior_identity = previous.get("protocol_identity") or {}
        callback = identity.get("callback_started_us")
        prior_callback = prior_identity.get("callback_started_us")
        if callback is not None and prior_callback is not None and callback > prior_callback:
            self.callback_intervals_us.append(callback - prior_callback)
        driver = identity.get("driver_rx_timestamp_us")
        prior_driver = prior_identity.get("driver_rx_timestamp_us")
        if driver is not None and prior_driver is not None:
            if driver < prior_driver:
                self.driver_timestamp_wraps += 1
            driver_delta = (driver - prior_driver) & U32_MAX
            if driver_delta > 0:
                self.driver_intervals_us.append(driver_delta)
        mesh = row.get("mesh_timestamp_us")
        prior_mesh = previous.get("mesh_timestamp_us")
        if mesh is not None and prior_mesh is not None:
            if mesh < prior_mesh:
                self.mesh_time_reversals += 1
            elif mesh > prior_mesh:
                self.mesh_intervals_us.append(mesh - prior_mesh)
        self.last = frame
        self.last_time = current_time
        self.frame_count += 1

    def report(self) -> dict:
        median = statistics.median(self.intervals) if self.intervals else None
        elapsed = self.last_time - self.first_time
        return {
            "first_line": self.first.line_number,
            "last_line": self.last.line_number,
            "frame_count": self.frame_count,
            "start_reason": self.reason,
            "schema_version": self.schema_version,
            "grid": self.grid,
            "receiver_boot_id_present": self.boot_id is not None,
            "time_basis": self.time_basis,
            "host_clock_epoch_id": self.clock_epoch_id,
            "first_host_utc_unix_ns": self.first.row["host_timestamp_unix_ns"],
            "last_host_utc_unix_ns": self.last.row["host_timestamp_unix_ns"],
            "duration_seconds": elapsed / 1e9 if elapsed > 0 else 0,
            "observed_rate_hz": (self.frame_count - 1) * 1e9 / elapsed if elapsed > 0 else None,
            "median_interval_ms": median / 1e6 if median is not None else None,
            "min_positive_interval_ms": min(self.intervals) / 1e6 if self.intervals else None,
            "p05_interval_ms": _quantile(self.intervals, 0.05) / 1e6 if self.intervals else None,
            "p95_interval_ms": _quantile(self.intervals, 0.95) / 1e6 if self.intervals else None,
            "max_interval_ms": max(self.intervals) / 1e6 if self.intervals else None,
            "intervals_over_3x_median": sum(x > 3 * median for x in self.intervals) if median else 0,
            "median_rx_callback_interval_us": (
                statistics.median(self.callback_intervals_us) if self.callback_intervals_us else None
            ),
            "median_driver_rx_interval_us": (
                statistics.median(self.driver_intervals_us) if self.driver_intervals_us else None
            ),
            "median_mesh_interval_us": (
                statistics.median(self.mesh_intervals_us) if self.mesh_intervals_us else None
            ),
            "duplicate_host_timestamps": self.duplicate_host_timestamps,
            "duplicate_sequences_with_same_iq": self.duplicate_sequences,
            "sequence_gap_events": self.sequence_gaps,
            "unobserved_serialized_sequence_ids": self.unobserved_sequence_ids,
            "sequence_wraps": self.sequence_wraps,
            "driver_timestamp_wraps": self.driver_timestamp_wraps,
            "mesh_time_reversals": self.mesh_time_reversals,
        }


def _u32_wrap(before: int, after: int) -> bool:
    return before >= 0xF0000000 and after <= 0x0FFFFFFF


def _sidecar_report(path: Path, total_rows: int, rx_counts: Counter) -> dict:
    sidecar = path.with_name(path.name.replace(".jsonl", ".meta.json"))
    if not sidecar.is_file():
        return {"present": False, "checks": ["metadata_sidecar_missing"]}
    try:
        metadata = json.loads(sidecar.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return {"present": True, "checks": ["metadata_sidecar_unreadable"]}
    checks = []
    if metadata.get("frames_written") != total_rows:
        checks.append("frames_written_mismatch")
    summaries = metadata.get("rx_summaries")
    if isinstance(summaries, list):
        expected = {
            item.get("rx_id"): item.get("frames_written")
            for item in summaries if isinstance(item, dict)
        }
        if expected != dict(rx_counts):
            checks.append("rx_summary_count_mismatch")
    else:
        checks.append("rx_summaries_missing")
    if metadata.get("status") != "completed" or metadata.get("incomplete") is True:
        checks.append("recording_not_completed")
    if bool(metadata.get("setup_id")) != bool(metadata.get("setup_sha256")):
        checks.append("partial_setup_identity")
    return {
        "present": True,
        "status": metadata.get("status"),
        "setup_identity_present": bool(metadata.get("setup_id") and metadata.get("setup_sha256")),
        "recording_writer_dropped_frames": metadata.get("dropped_frames"),
        "checks": checks,
    }


def analyze_capture(
    path: Path,
    rx_id: int,
    trusted_tx1_digest: str | None = None,
    sealed_tx1_setup: Path | None = None,
) -> dict:
    """Build a quality report. Without TX1 authority, no stream is selected."""
    if not 1 <= rx_id <= 4:
        raise ValueError("rx_id must be RX1..RX4")
    if trusted_tx1_digest is not None and sealed_tx1_setup is not None:
        raise ValueError("choose either a sealed TX1 setup or an independent TX1 digest")
    authority = load_sealed_tx1_setup(sealed_tx1_setup) if sealed_tx1_setup else None
    if trusted_tx1_digest is not None:
        trusted_tx1_digest = _digest(trusted_tx1_digest)
        if trusted_tx1_digest == "0" * 64:
            raise ValueError("an unbound all-zero digest cannot identify TX1")
        authority = Tx1Authority(trusted_tx1_digest, "operator_supplied_independent_digest")
    trusted_tx1_digest = authority.digest if authority else None
    rows = 0
    invalid_rows = 0
    rx_counts: Counter = Counter()
    rejection_counts: Counter = Counter()
    missing: Counter = Counter()
    quality: Counter = Counter()
    rssi_values: list[int] = []
    valid_noise_floor_values: list[int] = []
    valid_rx_states: Counter = Counter()
    available_gains: list[int] = []
    valid_sync_ages: list[int] = []
    protocol_versions: Counter = Counter()
    breaks: Counter = Counter()
    segments: list[_Segment] = []
    segment: _Segment | None = None
    pending_break: str | None = None
    with path.open(encoding="utf-8") as stream:
        for line_number, line in enumerate(stream, 1):
            try:
                frame = validate_frame(json.loads(line), line_number)
            except (json.JSONDecodeError, RecordingError):
                invalid_rows += 1
                pending_break = "invalid_record_between_selected_frames"
                continue
            row = frame.row
            rows += 1
            rx_counts[row["rx_id"]] += 1
            if row["rx_id"] != rx_id:
                continue
            binding = row.get("source_binding")
            if trusted_tx1_digest is None:
                rejection_counts["trusted_tx1_digest_missing"] += 1
                pending_break = "unselected_target_rx_frame"
                continue
            if binding is None:
                rejection_counts["source_binding_missing"] += 1
                pending_break = "unselected_target_rx_frame"
                continue
            if binding["flags"] != SOURCE_FLAGS_COMPLETE:
                rejection_counts["source_binding_incomplete"] += 1
                pending_break = "unselected_target_rx_frame"
                continue
            if binding["tx_filter_sha256"] != trusted_tx1_digest:
                rejection_counts["source_binding_mismatch"] += 1
                pending_break = "unselected_target_rx_frame"
                continue
            if segment is None:
                segment = _Segment(frame, pending_break or "first_selected_frame")
                segments.append(segment)
            else:
                reason = pending_break or segment.incompatibility(frame)
                if reason:
                    breaks[reason] += 1
                    segment = _Segment(frame, reason)
                    segments.append(segment)
                else:
                    segment.add(frame)
            pending_break = None
            rssi_values.append(row["rssi_dbm"])
            identity = row.get("protocol_identity")
            if identity is None:
                missing["receiver_protocol_identity"] += 1
                missing["receiver_boot_id"] += 1
                missing["receiver_callback_time"] += 1
                missing["driver_rx_time"] += 1
                missing["quality_flags"] += 1
                missing["gain_db"] += 1
            else:
                protocol_versions[str(identity["protocol_version"])] += 1
                if identity.get("callback_started_us") is None:
                    missing["receiver_callback_time"] += 1
                if identity.get("driver_rx_timestamp_us") is None:
                    missing["driver_rx_time"] += 1
                flags = identity.get("quality_flags")
                if flags is None:
                    missing["quality_flags"] += 1
                else:
                    for bit, label in ((1, "first_word_invalid"), (2, "noise_floor_valid"),
                                       (4, "rx_state_valid"), (8, "gain_available"),
                                       (16, "raw_profile"), (32, "sync_valid")):
                        if flags & bit:
                            quality[label] += 1
                    if flags & 0x02:
                        valid_noise_floor_values.append(row["noise_floor_dbm"])
                    if flags & 0x04 and identity.get("rx_state") is not None:
                        valid_rx_states[str(identity["rx_state"])] += 1
                    if flags & 0x20 and identity.get("sync_age_ms") is not None:
                        valid_sync_ages.append(identity["sync_age_ms"])
                if identity.get("gain_db") is not None:
                    available_gains.append(identity["gain_db"])
                if identity.get("gain_db") is None:
                    missing["gain_db"] += 1
            if row.get("host_monotonic_ns") is None or not row.get("clock_epoch_id"):
                missing["host_monotonic_or_epoch"] += 1
            if row.get("mesh_timestamp_us") is None:
                missing["mesh_aligned_device_time"] += 1
    selected = sum(item.frame_count for item in segments)
    sidecar = _sidecar_report(path, rows, rx_counts)
    if authority and authority.setup_id:
        if not sidecar.get("setup_identity_present"):
            raise RecordingError("recording has no matching sealed setup identity in its sidecar")
        metadata_path = path.with_name(path.name.replace(".jsonl", ".meta.json"))
        metadata = json.loads(metadata_path.read_text(encoding="utf-8"))
        if (metadata.get("setup_id"), metadata.get("setup_sha256")) != (
            authority.setup_id, authority.setup_sha256
        ):
            raise RecordingError("recording sidecar refers to a different sealed setup")
    with path.open("rb") as raw_input:
        input_sha256 = hashlib.file_digest(raw_input, "sha256").hexdigest()
    report = {
        "report_schema_version": 1,
        "input": str(path),
        "input_sha256": input_sha256,
        "selected_rx_id": rx_id,
        "source_authority": {
            "expected_tx1_filter_digest_supplied": trusted_tx1_digest is not None,
            "authority_type": authority.source if authority else None,
            "criterion": "source_binding version 1, flags 0x07, exact TX1 reference digest match",
            "historical_setup_seal_verified": bool(authority and authority.setup_id),
            "sidecar_setup_identity_matched": (
                bool(authority and authority.setup_id and sidecar.get("setup_identity_present"))
            ),
            "rx_transport_ip_recorded": False,
            "rx_id_is_receiver_reported": True,
            "wireless_sender_cryptographically_authenticated": False,
        },
        "all_valid_rows": rows,
        "invalid_rows": invalid_rows,
        "frames_by_rx": {str(key): value for key, value in sorted(rx_counts.items())},
        "selected_frames": selected,
        "target_rx_rejections": dict(rejection_counts),
        "sidecar": sidecar,
        "segments": [item.report() for item in segments],
        "segment_breaks": dict(breaks),
        "selected_missing_metadata": dict(missing),
        "selected_protocol_versions": dict(protocol_versions),
        "selected_quality_flags_set": dict(quality),
        "selected_quality_metrics": {
            "rssi_dbm": _numeric_summary(rssi_values),
            "noise_floor_dbm_with_valid_flag": _numeric_summary(valid_noise_floor_values),
            "rx_state_with_valid_flag": dict(valid_rx_states),
            "gain_db_when_available": _numeric_summary(available_gains),
            "sync_age_ms_with_valid_flag": _numeric_summary(valid_sync_ages),
        },
        "subcarrier_mapping": (
            "antenna-major recorded index only; physical subcarrier and LTF boundaries not encoded"
        ),
        "not_recorded_fields": [
            "rx_transport_source_ip", "physical_subcarrier_indices", "ltf_boundaries",
            "tx_transmit_timestamp", "tx_sent_packet_count",
        ],
        "complex_value_contract": (
            "complex(i,q) from signed recorded I/Q; no scaling, normalization or reordering"
        ),
        "time_contract": {
            "host_timestamp_unix_ns": "UTC wall-clock receive time; can jump",
            "host_monotonic_ns": "process-local receive time within clock_epoch_id, when present",
            "callback_started_us": "RX callback timer, not TX transmit time, when present",
            "driver_rx_timestamp_us": "RX driver u32 microsecond timer, wrap possible, when present",
            "mesh_timestamp_us": "optional aligned device timestamp, not assumed UTC",
        },
        "packet_loss": None,
        "packet_loss_reason": (
            "RX serialization sequence gaps do not establish TX sent packets or a complete denominator"
        ),
    }
    return report


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Read-only one-RX CSI import quality report")
    parser.add_argument("input", type=Path, help="existing *.raw-csi.v1.jsonl recording")
    parser.add_argument("--rx", type=int, required=True, help="selected RX number, 1..4")
    source = parser.add_mutually_exclusive_group()
    source.add_argument(
        "--trusted-tx1-filter-sha256", help="TX1 filter digest verified independently of this recording"
    )
    source.add_argument(
        "--sealed-tx1-setup", type=Path,
        help="historical sealed setup used only as a TX1 source identity reference",
    )
    parser.add_argument(
        "--report", type=Path, required=True, help="output JSON quality report; choose a private location"
    )
    args = parser.parse_args(argv)
    try:
        if not args.input.is_file():
            raise ValueError("input recording does not exist")
        if args.report.exists():
            raise ValueError("report already exists; existing files are not overwritten")
        report = analyze_capture(args.input, args.rx, args.trusted_tx1_filter_sha256, args.sealed_tx1_setup)
        args.report.parent.mkdir(parents=True, exist_ok=True)
        with args.report.open("x", encoding="utf-8") as output:
            json.dump(report, output, indent=2, sort_keys=True)
            output.write("\n")
    except (OSError, ValueError) as error:
        parser.error(str(error))
    print(
        f"read {report['all_valid_rows']} rows; selected {report['selected_frames']} RX{args.rx} TX1-bound rows; "
        f"{len(report['segments'])} segment(s); {report['invalid_rows']} invalid row(s)"
    )
    print(f"quality report: {args.report}")
    return 0


if __name__ == "__main__":
    sys.exit(main())

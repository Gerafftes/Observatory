"""Offline amplitude experiment on the VITAL-01 import contract, never physiology.

No resampling, imputed packets, label-dependent logic, or retained estimates.
NumPy is the only dependency beyond the existing standard-library importer.
"""

from __future__ import annotations

lazy import argparse
lazy import hashlib
lazy import json
lazy import math
lazy import sys
lazy from collections import Counter
lazy from dataclasses import asdict, dataclass
lazy from itertools import groupby
lazy from pathlib import Path

lazy import numpy as np
lazy from csi_import import (
    ImportedFrame,
    RecordingError,
    _clock,
    analyze_capture,
    iter_tx1_frames,
    load_sealed_tx1_setup,
)

UNAVAILABLE = "not_evaluable"
CANDIDATE = "diagnostic_candidate"
ACCEPTED = "experimental_estimate"


def _vital_identity(frame: ImportedFrame) -> dict:
    identity = frame.row.get("protocol_identity") or {}
    # The import contract validates quality/acquisition metadata only in protocol v3.
    return identity if identity.get("protocol_version") == 3 else {"boot_id": identity.get("boot_id")}


@dataclass(frozen=True)
class Config:
    respiration_band: tuple[float, float] = (0.1, 0.5)
    pulse_band: tuple[float, float] = (0.8, 2.17)
    respiration_window_s: float = 60.0
    pulse_window_s: float = 30.0
    step_s: float = 5.0
    antenna: int = 0
    max_gap_s: float = 0.5
    max_gap_medians: float = 3.0
    min_samples_per_cycle: float = 4.0
    min_candidate_fraction: float = 0.08
    min_estimate_fraction: float = 0.20
    min_peak_to_background: float = 6.0
    min_peak_to_competitor: float = 2.0
    max_clock_rate_error: float = 0.005

    def __post_init__(self) -> None:
        for band in (self.respiration_band, self.pulse_band):
            if len(band) != 2 or not all(math.isfinite(x) for x in band) or not 0 < band[0] < band[1]:
                raise ValueError("frequency bands must be finite, positive, ordered pairs")
        if self.respiration_band[1] >= self.pulse_band[0]:
            raise ValueError("respiration and pulse bands must not overlap")
        for name in (
            "respiration_window_s", "pulse_window_s", "step_s", "max_gap_s",
            "max_gap_medians", "min_samples_per_cycle", "min_peak_to_background",
            "min_peak_to_competitor", "max_clock_rate_error",
        ):
            if not math.isfinite(getattr(self, name)) or getattr(self, name) <= 0:
                raise ValueError(f"{name} must be finite and positive")
        if type(self.antenna) is not int or self.antenna < 0:
            raise ValueError("antenna must be a nonnegative integer")
        if not 0 < self.min_candidate_fraction <= self.min_estimate_fraction <= 1:
            raise ValueError("spectral fractions must satisfy 0 < candidate <= estimate <= 1")
        if self.min_samples_per_cycle < 4 or self.max_gap_medians < 1:
            raise ValueError("require at least four samples/cycle and gap multiplier >= 1")
        for window, band in ((self.respiration_window_s, self.respiration_band),
                             (self.pulse_window_s, self.pulse_band)):
            if window * band[0] < 4:
                raise ValueError("each window must cover at least four cycles at its lower band edge")


def _time_diagnostics(frames: list[ImportedFrame], times: np.ndarray,
                      band: tuple[float, float], config: Config) -> dict:
    intervals = np.diff(times)
    positive = intervals[intervals > 0]
    median = float(np.median(positive)) if len(positive) else None
    # Conservative sampling coverage policy, not a Nyquist theorem for irregular data.
    gap_limit = min(config.max_gap_s, 1 / (config.min_samples_per_cycle * band[1]),
                    config.max_gap_medians * median) if median else config.max_gap_s
    gaps = []
    for i in np.flatnonzero(intervals > gap_limit):
        gaps.append({"before_line": frames[i].line_number,
                     "after_line": frames[i + 1].line_number,
                     "start_s": float(times[i]), "end_s": float(times[i + 1]),
                     "duration_s": float(intervals[i])})
    cuts = [0, *(int(i) + 1 for i in np.flatnonzero((intervals > gap_limit) | (intervals <= 0))), len(frames)]
    spans = [{"first_line": frames[start].line_number,
              "last_line": frames[stop - 1].line_number,
              "start_s": float(times[start]), "end_s": float(times[stop - 1]),
              "duration_s": float(times[stop - 1] - times[start]), "frame_count": stop - start}
             for start, stop in zip(cuts, cuts[1:])]
    return {
        "time_basis": _clock(frames[0].row)[0],
        "frame_count": len(frames),
        "duration_s": float(times[-1] - times[0]),
        "observed_arrival_rate_hz": ((len(times) - 1) / float(times[-1] - times[0])
                                     if times[-1] > times[0] else None),
        "median_interval_s": median,
        "p95_interval_s": float(np.quantile(positive, .95)) if len(positive) else None,
        "max_interval_s": float(max(positive)) if len(positive) else None,
        "nonpositive_intervals": int(np.sum(intervals <= 0)),
        "gap_limit_s": gap_limit,
        "gaps": gaps,
        "gap_free_spans": spans,
        "longest_gap_free_span_s": max(span["duration_s"] for span in spans),
        "mesh_reversals": sum(
            a.row.get("mesh_timestamp_us") is not None
            and b.row.get("mesh_timestamp_us") is not None
            and b.row["mesh_timestamp_us"] < a.row["mesh_timestamp_us"]
            for a, b in zip(frames, frames[1:])
        ),
        "mesh_used_for_spectrum": False,
        "interpolated_samples": 0,
    }


def _amplitude(frames: list[ImportedFrame], antenna: int) -> tuple[np.ndarray | None, dict]:
    if any(antenna >= f.row["antenna_count"] for f in frames):
        return None, {"reason": "antenna_unavailable", "used_recorded_indices": []}
    samples = np.asarray([f.samples_for_antenna(antenna) for f in frames])
    amplitude = np.abs(samples)
    valid = np.mean(amplitude == 0, axis=0) <= .05
    clipped = (samples.real == -128) | (samples.real == 127) | (samples.imag == -128) | (samples.imag == 127)
    valid &= np.mean(clipped, axis=0) <= .01
    # first_word_invalid refers to the first four raw bytes, i.e. two I/Q pairs.
    if antenna == 0 and any((_vital_identity(f).get("quality_flags") or 0) & 1
                            for f in frames):
        valid[:2] = False
    means = amplitude.mean(axis=0)
    valid &= means > 0
    indices = np.flatnonzero(valid).tolist()
    metrics = {"method": "mean_relative_amplitude", "antenna": antenna,
               "used_recorded_indices": indices,
               "excluded_recorded_indices": np.flatnonzero(~valid).tolist(),
               "physical_subcarrier_mapping_known": False}
    if not indices:
        return None, dict(metrics, reason="no_usable_subcarriers")
    # Each column is the SAME recorded subcarrier over time, never an FFT across I/Q bins.
    signal = (amplitude[:, valid] / means[valid] - 1).mean(axis=1)
    metrics["relative_amplitude_rms"] = float(np.std(signal))
    return signal, metrics


def _spectrum(times: np.ndarray, signal: np.ndarray, band: tuple[float, float]) -> dict:
    """Least-squares sinusoid + offset + trend at actual (possibly irregular) times.

    Improvement over an offset/trend-only fit is a unitless explained-variance
    fraction. It is neither a probability nor a physiological confidence.
    """
    elapsed = times - times[0]
    duration = float(elapsed[-1])
    trend = np.column_stack((np.ones(len(times)), elapsed / duration))
    residual = signal - trend @ np.linalg.lstsq(trend, signal, rcond=None)[0]
    variance = float(residual @ residual)
    if variance / len(times) < 1e-8:
        return {"reason": "flat_signal"}
    # Fourfold frequency-grid oversampling is NOT extra physical resolution.
    frequencies = np.linspace(band[0], band[1], math.ceil((band[1] - band[0]) * duration * 4) + 1)
    powers = []
    for frequency in frequencies:
        angle = 2 * np.pi * frequency * elapsed
        design = np.column_stack((trend, np.sin(angle), np.cos(angle)))
        error = signal - design @ np.linalg.lstsq(design, signal, rcond=None)[0]
        powers.append(max(0.0, 1 - float(error @ error) / variance))
    power = np.asarray(powers)
    peak_index = int(np.argmax(power))
    peak = float(power[peak_index])
    frequency = float(frequencies[peak_index])
    away = np.abs(frequencies - frequency) > 2 / duration
    if np.count_nonzero(away) < 3:
        return {"reason": "insufficient_spectral_background", "resolution_hz": 1 / duration}
    background = float(np.median(power[away]))
    competitor = float(max(power[away]))
    return {"peak_hz": frequency, "explained_fraction": peak,
            "peak_to_background": peak / max(background, 1e-12),
            "peak_to_competitor": peak / max(competitor, 1e-12),
            "at_band_edge": peak_index in (0, len(frequencies) - 1),
            "resolution_hz": 1 / duration,
            "frequency_grid_step_hz": float(frequencies[1] - frequencies[0])}


def _candidate_supported(spectrum: dict, config: Config) -> bool:
    return ("peak_hz" in spectrum
            and spectrum["explained_fraction"] >= config.min_candidate_fraction
            and spectrum["peak_to_background"] >= config.min_peak_to_background)


def _evaluate_window(frames: list[ImportedFrame], times: np.ndarray, end: float,
                     branch: str, config: Config) -> dict:
    band = config.respiration_band if branch == "respiration" else config.pulse_band
    window = config.respiration_window_s if branch == "respiration" else config.pulse_window_s
    start = max(0.0, end - window)
    indices = np.flatnonzero((times >= start - 1e-9) & (times <= end + 1e-9))
    selected = [frames[i] for i in indices]
    selected_times = times[indices]
    result = {"branch": branch, "start_s": start, "end_s": end,
              "requested_window_s": window, "status": UNAVAILABLE,
              "candidate_hz": None, "candidate_bpm": None,
              "estimate_hz": None, "estimate_bpm": None,
              "physiologically_validated": False, "reasons": [],
              "first_line": selected[0].line_number if selected else None,
              "last_line": selected[-1].line_number if selected else None}
    reasons = result["reasons"]
    if not selected:
        reasons.append("no_samples_in_window")
        return result
    timing = _time_diagnostics(selected, selected_times, band, config)
    result["timing"] = timing
    identities = [_vital_identity(f) for f in selected]
    missing_monotonic = any(_clock(f.row)[0] != "host_monotonic_ns" for f in selected)
    if missing_monotonic:
        reasons.append("monotonic_host_time_missing")
    if any(not identity.get("boot_id") for identity in identities):
        reasons.append("receiver_boot_identity_missing")
    if timing["nonpositive_intervals"]:
        reasons.append("nonpositive_time_intervals")
    if timing["gaps"]:
        reasons.append("data_gap_exceeds_limit")
    if len(selected) < 16:
        reasons.append("too_few_samples")
    # At most one allowed packet interval may be absent at each window edge.
    if (end < window - 1e-9 or selected_times[0] - start > timing["gap_limit_s"]
            or end - selected_times[-1] > timing["gap_limit_s"]
            or timing["duration_s"] < window - 2 * timing["gap_limit_s"]):
        reasons.append("insufficient_window_coverage")
    rate = timing["observed_arrival_rate_hz"]
    if rate is None or rate < config.min_samples_per_cycle * band[1]:
        reasons.append("insufficient_observed_sampling_density")
    sequences = [f.row["sequence"] for f in selected]
    if len(set(sequences)) != len(sequences):
        reasons.append("duplicate_serialized_packets")
    if any((identity.get("quality_flags") or 0) & 4 and identity.get("rx_state") not in (None, 0)
           for identity in identities):
        reasons.append("receiver_error_state")
    gains = {identity.get("gain_db") for identity in identities}
    if len(gains - {None}) > 1:
        reasons.append("gain_changed_within_window")
    signal, amplitude = _amplitude(selected, config.antenna)
    result["amplitude"] = amplitude
    if signal is None:
        reasons.append(amplitude["reason"])
    # Arrival time needs a corroborating acquisition clock for technical acceptance.
    acceptance_reasons = []
    callbacks = [identity.get("callback_started_us") for identity in identities]
    if all(value is not None for value in callbacks) and len(callbacks) > 1:
        device_time = np.asarray([value - callbacks[0] for value in callbacks], dtype=float) / 1e6
        host_elapsed = selected_times - selected_times[0]
        if np.any(np.diff(device_time) <= 0):
            reasons.append("nonpositive_callback_intervals")
        elif host_elapsed[-1] > 0:
            slope = float(np.polyfit(device_time, host_elapsed, 1)[0])
            residual = host_elapsed - slope * device_time
            spread = float(np.ptp(residual))
            timing["host_callback_rate_ratio"] = slope
            timing["host_callback_residual_range_s"] = spread
            if abs(slope - 1) > config.max_clock_rate_error or spread > timing["gap_limit_s"]:
                reasons.append("host_callback_timing_disagreement")
    else:
        acceptance_reasons.append("acquisition_clock_unverified")
    if any(identity.get("quality_flags") is None for identity in identities):
        acceptance_reasons.append("quality_flags_missing")
    elif any(not identity["quality_flags"] & 16 for identity in identities):
        acceptance_reasons.append("raw_profile_unverified")
    if any(not ((identity.get("quality_flags") or 0) & 4) or identity.get("rx_state") is None
           for identity in identities):
        acceptance_reasons.append("receiver_state_unverified")
    if reasons:
        return result
    spectrum = _spectrum(selected_times, signal, band)
    if not _candidate_supported(spectrum, config):
        # Do not serialize a random maximum as a frequency candidate.
        result["spectral_quality"] = {k: v for k, v in spectrum.items() if k != "peak_hz"}
        reasons.append(spectrum.get("reason", "no_supported_periodicity"))
        return result
    result["spectral_quality"] = spectrum
    result["status"] = CANDIDATE
    result["candidate_hz"] = spectrum["peak_hz"]
    result["candidate_bpm"] = 60 * spectrum["peak_hz"]
    reasons.extend(acceptance_reasons)
    if spectrum["explained_fraction"] < config.min_estimate_fraction:
        reasons.append("weak_explained_variance")
    if spectrum["peak_to_competitor"] < config.min_peak_to_competitor:
        reasons.append("competing_spectral_peaks")
    if spectrum["at_band_edge"]:
        reasons.append("peak_at_investigation_boundary")
    halves = []
    midpoint = (selected_times[0] + selected_times[-1]) / 2
    for mask in (selected_times <= midpoint, selected_times > midpoint):
        half = _spectrum(selected_times[mask], signal[mask], band)
        halves.append(half)
    stable = all(_candidate_supported(half, config)
                 and abs(half["peak_hz"] - spectrum["peak_hz"]) <= 2 / window
                 for half in halves)
    # Unsupported half-window maxima also remain unreported.
    result["stability"] = {"passed": stable, "tolerance_hz": 2 / window,
                           "halves": [half if _candidate_supported(half, config)
                                      else {k: v for k, v in half.items() if k != "peak_hz"}
                                      for half in halves]}
    if not stable:
        reasons.append("frequency_unstable_between_halves")
    if not reasons:
        result["status"] = ACCEPTED
        result["estimate_hz"] = result["candidate_hz"]
        result["estimate_bpm"] = result["candidate_bpm"]
    return result


def _check_pulse_harmonics(pulse: dict, respiration: dict, config: Config) -> None:
    if pulse["candidate_hz"] is None:
        return
    pulse["respiration_context"] = {"start_s": respiration["start_s"],
                                    "end_s": respiration["end_s"],
                                    "status": respiration["status"],
                                    "candidate_hz": respiration["candidate_hz"]}
    if respiration["status"] != ACCEPTED:
        pulse["reasons"].append("respiration_harmonics_unresolved")
    else:
        # Conservative ambiguity veto, including higher-order harmonics. Never subtract
        # a presumed harmonic and declare the residual to be a heartbeat.
        frequency = respiration["candidate_hz"]
        tolerance = 1 / config.pulse_window_s
        matches = [order for order in range(2, math.ceil(config.pulse_band[1] / frequency) + 1)
                   if abs(pulse["candidate_hz"] - order * frequency)
                   <= tolerance + order / (4 * config.respiration_window_s)]
        pulse["respiration_context"]["possible_harmonic_orders"] = matches
        if matches:
            pulse["reasons"].append("possible_respiration_harmonic")
    if pulse["reasons"]:
        pulse["status"] = CANDIDATE
        pulse["estimate_hz"] = pulse["estimate_bpm"] = None


def analyze_frames(frames: list[ImportedFrame], config: Config | None = None) -> dict:
    """Consume validated, selected frames from iter_tx1_frames in original order."""
    config = config or Config()
    segments = []
    windows = []
    for segment_id, group in groupby(frames, key=lambda frame: frame.segment_id):
        if segment_id is None:
            raise ValueError("use iter_tx1_frames so import segment boundaries are preserved")
        part = list(group)
        origin = _clock(part[0].row)[1]
        # Subtract integer nanoseconds BEFORE converting to float, preserving precision.
        times = np.asarray([(_clock(f.row)[1] - origin) / 1e9 for f in part])
        duration = float(times[-1])
        segment_report = {"segment_id": segment_id, "start_reason": part[0].segment_start_reason,
                          "first_line": part[0].line_number, "last_line": part[-1].line_number,
                          "frame_count": len(part), "duration_s": duration, "branches": {}}
        respiration_cache = {}
        for branch, band, window in (
            ("respiration", config.respiration_band, config.respiration_window_s),
            ("pulse", config.pulse_band, config.pulse_window_s),
        ):
            segment_report["branches"][branch] = _time_diagnostics(part, times, band, config)
            ends = list(np.arange(window, duration + 1e-9, config.step_s))
            # A final fresh partial/off-cadence window documents short recordings and
            # a recording that ends in a gap; an old accepted value is never carried.
            if not ends or duration - ends[-1] > 1e-6:
                ends.append(duration)
            for end in ends:
                end = float(end)
                result = _evaluate_window(part, times, end, branch, config)
                if branch == "respiration":
                    respiration_cache[end] = result
                else:
                    if end not in respiration_cache:
                        respiration_cache[end] = _evaluate_window(part, times, end, "respiration", config)
                    _check_pulse_harmonics(result, respiration_cache[end], config)
                result["segment_id"] = segment_id
                result["scheduled_update"] = end >= window and abs((end - window) % config.step_s) < 1e-6
                windows.append(result)
        segments.append(segment_report)
    counts = {branch: dict(Counter(w["status"] for w in windows if w["branch"] == branch))
              for branch in ("respiration", "pulse")}
    return {"analysis_version": "VITAL-02", "config": asdict(config),
            "physiologically_validated": False,
            "frequency_unit": "Hz", "rate_unit": "cycles_per_minute",
            "segments": segments, "windows": windows, "status_counts": counts,
            "reasons": [] if frames else ["no_selected_frames"],
            "interpretation": "Technical signal diagnostics only; no physiological validation."}


def analyze_recording(path: Path, rx_id: int, trusted_tx1_digest: str | None = None,
                      sealed_tx1_setup: Path | None = None, config: Config | None = None) -> dict:
    # VITAL-01 owns validation, source authority (including sidecar matching), and selection.
    quality = analyze_capture(path, rx_id, trusted_tx1_digest, sealed_tx1_setup)
    if quality["invalid_rows"]:
        raise RecordingError("invalid recording rows; repair the source before frequency analysis")
    digest = load_sealed_tx1_setup(sealed_tx1_setup).digest if sealed_tx1_setup else trusted_tx1_digest
    frames = list(iter_tx1_frames(path, rx_id, digest)) if digest else []
    with path.open("rb") as stream:
        if hashlib.file_digest(stream, "sha256").hexdigest() != quality["input_sha256"]:
            raise RecordingError("recording changed during analysis")
    result = analyze_frames(frames, config)
    result["report_schema_version"] = 1
    result["import_quality"] = quality
    result["implementation_sha256"] = {
        name: hashlib.sha256(Path(__file__).with_name(name).read_bytes()).hexdigest()
        for name in ("vitals.py", "csi_import.py")
    }
    return result


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="VITAL-02 offline CSI amplitude experiment")
    parser.add_argument("input", type=Path)
    parser.add_argument("--rx", type=int, required=True)
    source = parser.add_mutually_exclusive_group()
    source.add_argument("--trusted-tx1-filter-sha256")
    source.add_argument("--sealed-tx1-setup", type=Path)
    parser.add_argument("--report", type=Path, required=True, help="new private JSON report")
    parser.add_argument("--respiration-band", nargs=2, type=float, default=(.1, .5), metavar=("LOW", "HIGH"))
    parser.add_argument("--pulse-band", nargs=2, type=float, default=(.8, 2.17), metavar=("LOW", "HIGH"))
    parser.add_argument("--respiration-window", type=float, default=60)
    parser.add_argument("--pulse-window", type=float, default=30)
    parser.add_argument("--step", type=float, default=5)
    parser.add_argument("--antenna", type=int, default=0)
    args = parser.parse_args(argv)
    try:
        config = Config(tuple(args.respiration_band), tuple(args.pulse_band),
                        args.respiration_window, args.pulse_window, args.step, args.antenna)
        if args.report.exists():
            raise ValueError("report already exists; existing files are not overwritten")
        report = analyze_recording(args.input, args.rx, args.trusted_tx1_filter_sha256,
                                   args.sealed_tx1_setup, config)
        args.report.parent.mkdir(parents=True, exist_ok=True)
        with args.report.open("x", encoding="utf-8") as output:
            json.dump(report, output, indent=2, sort_keys=True, allow_nan=False)
            output.write("\n")
    except (OSError, ValueError) as error:
        parser.error(str(error))
    print(f"VITAL-02: {report['status_counts']}; physiologically validated: false")
    print(f"private report: {args.report}")
    return 0


if __name__ == "__main__":
    sys.exit(main())

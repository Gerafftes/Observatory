#!/usr/bin/env python3
"""Build the D4/D5/D6 run inventory and the report figures.

The script is deliberately read-only with respect to recordings. It derives the
inventory from the archived JSONL/metadata files and writes only dated result
artifacts below ``RuView/results``.
"""

from __future__ import annotations

import csv
import json
import math
import statistics
from collections import Counter, defaultdict
from pathlib import Path
from typing import Any, Iterable

from PIL import Image, ImageDraw, ImageFont


PROJECT_DIR = Path(__file__).resolve().parent.parent
BLL_DIR = PROJECT_DIR.parent
API_DATA_DIR = BLL_DIR / "wifi-csi-dokumentation" / "data" / "raw"
D6_DATA_DIR = BLL_DIR / "data" / "recordings"
RESULTS_DIR = PROJECT_DIR / "results"
FIGURES_DIR = RESULTS_DIR / "2026-08-23_D4-D5-D6_figures"
INVENTORY_PATH = RESULTS_DIR / "2026-08-23_D4-D5-D6_laufuebersicht.csv"
D4_RX_PATH = RESULTS_DIR / "2026-08-23_D4_RX_diagnostik.csv"

WIDTH = 1600
HEIGHT = 900
BACKGROUND = "#f6f4ef"
INK = "#20242b"
MUTED = "#667085"
GRID = "#d7d2c8"
NAVY = "#214761"
TEAL = "#2a7f79"
RED = "#b34b4b"
AMBER = "#c4872b"


def font(size: int, bold: bool = False) -> ImageFont.FreeTypeFont:
    name = "Arial Bold.ttf" if bold else "Arial.ttf"
    return ImageFont.truetype(Path("/System/Library/Fonts/Supplemental") / name, size)


def load_jsonl(path: Path) -> list[dict[str, Any]]:
    with path.open(encoding="utf-8") as handle:
        return [json.loads(line) for line in handle if line.strip()]


def percent(count: int | None, denominator: int | None) -> float | None:
    if count is None or not denominator:
        return None
    return 100.0 * count / denominator


def safe_round(value: float | None, digits: int = 4) -> float | str:
    return "" if value is None else round(value, digits)


def percentile(values: Iterable[float], percentile_value: float) -> float | None:
    ordered = sorted(values)
    if not ordered:
        return None
    rank = (len(ordered) - 1) * percentile_value
    lower = math.floor(rank)
    upper = math.ceil(rank)
    if lower == upper:
        return ordered[lower]
    fraction = rank - lower
    return ordered[lower] * (1 - fraction) + ordered[upper] * fraction


def classify_api_run(name: str) -> tuple[str, str, str, str]:
    if name.startswith("2026-06-28"):
        condition = "empty" if "leerer_raum" in name or "_empty_" in name else "person"
        return "A/G historical", condition, "historical technical only", "historical_only"
    if "19-31-37_A0" in name:
        return "pre-D4", "empty", "historical technical only", "historical_only"
    if "19-35-45_A1" in name:
        return "pre-D4", "person", "historical technical only", "historical_only"
    if "21-05-50_E0_" in name:
        return "D4", "mixed", "contaminated empty run", "excluded_contaminated"
    if "E0b_" in name or "E0c_" in name or "E0d_" in name:
        return "D4", "empty", "direct D4 comparison", "classification_evidence"
    if "E1_person" in name or "E1b_" in name:
        return "D4", "person", "direct D4 comparison", "classification_evidence"
    if "D5_E1_still_persistenz" in name:
        return "D5 live", "person", "live persistence", "classification_evidence"
    if "D5_E1_still_sitzend" in name:
        return "D5 live", "person", "live still-person", "classification_evidence"
    if "D5_abs_E0_calibration" in name:
        return "D5-abs", "empty", "calibration only", "calibration_only"
    if "D5_abs_E0_validation" in name:
        return "D5-abs", "empty", "blind empty validation", "classification_evidence"
    if "D5_abs_E1_" in name:
        return "D5-abs", "person", "still-person validation", "classification_evidence_limited"
    return "unassigned", "unknown", "inventory only", "inventory_only"


def analyze_api_run(path: Path) -> tuple[dict[str, Any], list[dict[str, Any]]]:
    metadata_path = path / "metadata.json"
    raw_path = path / "raw_sensing.jsonl"
    summary_path = path / "summary.csv"
    errors_path = path / "errors.log"
    metadata = json.loads(metadata_path.read_text(encoding="utf-8"))
    samples = load_jsonl(raw_path)
    group, condition, role, evidence_class = classify_api_run(path.name)
    elapsed = [float(sample["elapsed_s"]) for sample in samples]
    duration = max(elapsed) - min(elapsed) if elapsed else None
    sample_count = len(samples)
    global_positive = sum(bool(sample["data"]["classification"]["presence"]) for sample in samples)
    node_ids_by_sample: list[set[int]] = []
    stale_count = 0
    invalid_sync_count = 0
    raster_profiles: Counter[tuple[int, int]] = Counter()
    rx_positive: Counter[int] = Counter()
    rx_diagnostics: dict[int, dict[str, list[float] | int]] = {
        rx: {
            "raw": [],
            "smoothed": [],
            "baseline": [],
            "positive": 0,
        }
        for rx in range(1, 5)
    }

    for sample in samples:
        data = sample["data"]
        features = data.get("node_features", [])
        node_ids = {int(item["node_id"]) for item in features}
        node_ids_by_sample.append(node_ids)
        for item in features:
            rx = int(item["node_id"])
            stale_count += int(bool(item.get("stale")))
            vote = item.get("d5_presence", {}).get("vote")
            local_positive = bool(vote) if vote is not None else bool(item["classification"]["presence"])
            rx_positive[rx] += int(local_positive)
            if rx in rx_diagnostics:
                rx_diagnostics[rx]["positive"] = int(rx_diagnostics[rx]["positive"]) + int(local_positive)
                for source, target in (
                    ("raw_motion_score", "raw"),
                    ("smoothed_motion_score", "smoothed"),
                    ("quiet_motion_baseline", "baseline"),
                ):
                    value = item.get(source)
                    if isinstance(value, (int, float)):
                        values = rx_diagnostics[rx][target]
                        assert isinstance(values, list)
                        values.append(float(value))
        for node in data.get("nodes", []):
            raster_profiles[(int(node.get("subcarrier_count", -1)), len(node.get("amplitude", [])))] += 1
            sync = node.get("sync", {})
            invalid_sync_count += int(sync.get("is_valid") is False)

    errors = errors_path.read_text(encoding="utf-8").strip() if errors_path.exists() else None
    required_present = all(item.exists() for item in (raw_path, metadata_path, summary_path, errors_path))
    all_rx = bool(samples) and all(node_ids == {1, 2, 3, 4} for node_ids in node_ids_by_sample)
    raster_text = ";".join(f"{subcarriers}/{amplitudes}:{count}" for (subcarriers, amplitudes), count in sorted(raster_profiles.items()))
    row: dict[str, Any] = {
        "phase": group,
        "recording_id": metadata.get("label", path.name),
        "condition": condition,
        "role": role,
        "evidence_class": evidence_class,
        "status": "",
        "incomplete": "",
        "server_version": "",
        "planned_duration_s": metadata.get("duration_s", ""),
        "actual_duration_s": safe_round(duration, 6),
        "valid_samples_or_frames": sample_count,
        "stored_rate_hz": safe_round(sample_count / duration if duration else None, 4),
        "required_files_present": required_present,
        "errors_or_writer_error": errors or "",
        "dropped_frames": "",
        "all_rx_present": all_rx,
        "stale_observations": stale_count,
        "invalid_sync_observations": invalid_sync_count,
        "global_positive_count": global_positive,
        "global_positive_pct": safe_round(percent(global_positive, sample_count), 3),
        "rx1_positive_count": rx_positive[1],
        "rx1_positive_pct": safe_round(percent(rx_positive[1], sample_count), 3),
        "rx2_positive_count": rx_positive[2],
        "rx2_positive_pct": safe_round(percent(rx_positive[2], sample_count), 3),
        "rx3_positive_count": rx_positive[3],
        "rx3_positive_pct": safe_round(percent(rx_positive[3], sample_count), 3),
        "rx4_positive_count": rx_positive[4],
        "rx4_positive_pct": safe_round(percent(rx_positive[4], sample_count), 3),
        "csi_grid_or_api_raster": raster_text,
        "setup_id": "",
        "setup_sha256": "",
        "tx_filter_sha256": "",
        "raw_matches_metadata": "",
        "timestamp_duplicates": "",
        "timestamp_regressions": "",
        "max_interframe_gap_ms": "",
        "gaps_over_500ms": "",
        "notes": metadata.get("notes", ""),
        "source_path": str(path.relative_to(BLL_DIR)),
    }
    diagnostic_rows: list[dict[str, Any]] = []
    if group == "D4" and condition in {"empty", "person"}:
        short_name = next((token for token in ("E0b", "E0c", "E0d", "E1b", "E1") if token in path.name), path.name)
        for rx, values in rx_diagnostics.items():
            raw_values = values["raw"]
            smoothed_values = values["smoothed"]
            baseline_values = values["baseline"]
            assert isinstance(raw_values, list)
            assert isinstance(smoothed_values, list)
            assert isinstance(baseline_values, list)
            diagnostic_rows.append(
                {
                    "run": short_name,
                    "condition": condition,
                    "rx": f"RX{rx}",
                    "samples": sample_count,
                    "positive_count": values["positive"],
                    "positive_pct": safe_round(percent(int(values["positive"]), sample_count), 3),
                    "raw_mean": safe_round(statistics.fmean(raw_values) if raw_values else None, 6),
                    "raw_p95": safe_round(percentile(raw_values, 0.95), 6),
                    "smoothed_mean": safe_round(statistics.fmean(smoothed_values) if smoothed_values else None, 6),
                    "smoothed_p95": safe_round(percentile(smoothed_values, 0.95), 6),
                    "baseline_mean": safe_round(statistics.fmean(baseline_values) if baseline_values else None, 6),
                    "source_path": str(path.relative_to(BLL_DIR)),
                }
            )
    return row, diagnostic_rows


def classify_d6(recording_id: str, setup_id: str | None) -> tuple[str, str, str]:
    if recording_id.startswith("discovery"):
        return "discovery", "device/data inventory", "discovery_only"
    if recording_id.endswith("-01"):
        return "empty" if recording_id.startswith("empty") else "preflight", "old setup series", "historical_technical"
    if recording_id.startswith("empty"):
        return "empty", "current setup baseline", "baseline_candidate"
    if recording_id.startswith("preflight"):
        return "preflight", "current setup transport test", "technical_evidence"
    return "unknown", "inventory only", "inventory_only"


def analyze_d6(meta_path: Path) -> dict[str, Any]:
    metadata = json.loads(meta_path.read_text(encoding="utf-8"))
    recording_id = metadata["recording_id"]
    raw_path = meta_path.with_name(f"{recording_id}.raw-csi.v1.jsonl")
    frames = load_jsonl(raw_path)
    condition, role, evidence_class = classify_d6(recording_id, metadata.get("setup_id"))
    by_rx: dict[int, list[dict[str, Any]]] = defaultdict(list)
    grids: Counter[tuple[int, int, int, int, int]] = Counter()
    sessions: set[str] = set()
    bindings: set[str] = set()
    for frame in frames:
        rx = int(frame["rx_id"])
        by_rx[rx].append(frame)
        grids[(
            int(frame["center_frequency_mhz"]),
            int(frame["antenna_count"]),
            int(frame["subcarrier_count"]),
            len(frame["iq_pairs"]),
            int(frame["ppdu_type"]),
        )] += 1
        sessions.add(str(frame["session_id"]))
        bindings.add(str(frame.get("source_binding", {}).get("tx_filter_sha256", "")))

    duplicate_count = 0
    regression_count = 0
    sequence_regressions = 0
    max_gap_ms = 0.0
    gaps_over_500ms = 0
    for rx_frames in by_rx.values():
        timestamps = [int(frame["host_timestamp_unix_ns"]) for frame in rx_frames]
        sequences = [int(frame["sequence"]) for frame in rx_frames]
        duplicate_count += len(timestamps) - len(set(timestamps))
        deltas = [current - previous for previous, current in zip(timestamps, timestamps[1:])]
        regression_count += sum(delta < 0 for delta in deltas)
        if deltas:
            max_gap_ms = max(max_gap_ms, max(deltas) / 1_000_000)
            gaps_over_500ms += sum(delta > 500_000_000 for delta in deltas)
        sequence_regressions += sum(current < previous for previous, current in zip(sequences, sequences[1:]))

    timestamps = [int(frame["host_timestamp_unix_ns"]) for frame in frames]
    duration = (max(timestamps) - min(timestamps)) / 1_000_000_000 if timestamps else None
    summary_counts = {int(item["rx_id"]): int(item["frames_written"]) for item in metadata["rx_summaries"]}
    raw_counts = {rx: len(items) for rx, items in by_rx.items()}
    raw_matches = len(frames) == int(metadata["frames_written"]) and raw_counts == summary_counts
    grid_text = ";".join(
        f"{frequency}MHz/{antennas}ant/{subcarriers}sc/{iq_count}iq/ppdu{ppdu}:{count}"
        for (frequency, antennas, subcarriers, iq_count, ppdu), count in sorted(grids.items())
    )
    notes = [f"sequence_regressions={sequence_regressions}", f"session_ids={len(sessions)}", f"bindings={len(bindings)}"]
    return {
        "phase": "D6",
        "recording_id": recording_id,
        "condition": condition,
        "role": role,
        "evidence_class": evidence_class,
        "status": metadata.get("status", ""),
        "incomplete": metadata.get("incomplete", ""),
        "server_version": metadata.get("server_version", ""),
        "planned_duration_s": metadata.get("duration_secs", ""),
        "actual_duration_s": safe_round(duration, 6),
        "valid_samples_or_frames": len(frames),
        "stored_rate_hz": safe_round(len(frames) / duration if duration else None, 4),
        "required_files_present": meta_path.exists() and raw_path.exists(),
        "errors_or_writer_error": metadata.get("writer_error") or "",
        "dropped_frames": metadata.get("dropped_frames", ""),
        "all_rx_present": set(by_rx) == {1, 2, 3, 4},
        "stale_observations": "",
        "invalid_sync_observations": "",
        "global_positive_count": "",
        "global_positive_pct": "",
        "rx1_positive_count": raw_counts.get(1, 0),
        "rx1_positive_pct": "",
        "rx2_positive_count": raw_counts.get(2, 0),
        "rx2_positive_pct": "",
        "rx3_positive_count": raw_counts.get(3, 0),
        "rx3_positive_pct": "",
        "rx4_positive_count": raw_counts.get(4, 0),
        "rx4_positive_pct": "",
        "csi_grid_or_api_raster": grid_text,
        "setup_id": metadata.get("setup_id") or "",
        "setup_sha256": metadata.get("setup_sha256") or "",
        "tx_filter_sha256": next(iter(bindings)) if len(bindings) == 1 else ";".join(sorted(bindings)),
        "raw_matches_metadata": raw_matches,
        "timestamp_duplicates": duplicate_count,
        "timestamp_regressions": regression_count,
        "max_interframe_gap_ms": safe_round(max_gap_ms, 3),
        "gaps_over_500ms": gaps_over_500ms,
        "notes": "; ".join(notes),
        "source_path": str(meta_path.relative_to(BLL_DIR)),
    }


def write_csv(path: Path, rows: list[dict[str, Any]]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=list(rows[0]))
        writer.writeheader()
        writer.writerows(rows)


def draw_header(draw: ImageDraw.ImageDraw, title: str, subtitle: str) -> None:
    title_size = 46
    while title_size > 30 and draw.textbbox((0, 0), title, font=font(title_size, True))[2] > WIDTH - 180:
        title_size -= 2
    draw.text((90, 55), title, fill=INK, font=font(title_size, True))
    draw.text((90, 118), subtitle, fill=MUTED, font=font(24))


def save_global_comparison() -> None:
    image = Image.new("RGB", (WIDTH, HEIGHT), BACKGROUND)
    draw = ImageDraw.Draw(image)
    draw_header(
        draw,
        "Global classification: replay success did not transfer live",
        "Percent of eligible samples; false presence lower is better, still recall higher is better",
    )
    methods = ["D4 pooled", "D5 replay", "D5 live", "D5-abs"]
    fpr = [75.246, 0.0, None, 0.0]
    recall = [88.397, 89.340, 0.0, 0.0]
    panels = [(90, 205, 735, 790, "Empty-room false presence", fpr, 10.0, RED), (865, 205, 1510, 790, "Still-person recall", recall, 80.0, TEAL)]
    for left, top, right, bottom, panel_title, values, target, color in panels:
        draw.rounded_rectangle((left, top, right, bottom), 22, fill="#ffffff", outline=GRID, width=2)
        draw.text((left + 35, top + 28), panel_title, fill=INK, font=font(30, True))
        draw.text((right - 175, top + 33), f"gate {target:.0f}%", fill=MUTED, font=font(20))
        chart_left = left + 170
        chart_right = right - 45
        chart_top = top + 105
        chart_bottom = bottom - 45
        target_x = chart_left + (chart_right - chart_left) * target / 100
        draw.line((target_x, chart_top - 10, target_x, chart_bottom), fill=AMBER, width=4)
        for tick in range(0, 101, 20):
            x = chart_left + (chart_right - chart_left) * tick / 100
            draw.line((x, chart_top, x, chart_bottom), fill=GRID, width=1)
            draw.text((x - 14, chart_bottom + 8), str(tick), fill=MUTED, font=font(18))
        row_height = 95
        for index, (method, value) in enumerate(zip(methods, values)):
            y = chart_top + index * row_height + 20
            draw.text((left + 30, y + 8), method, fill=INK, font=font(21))
            if value is None:
                draw.rounded_rectangle((chart_left, y, chart_right, y + 45), 8, fill="#ece9e2")
                draw.text((chart_left + 18, y + 8), "N/A — no paired empty run", fill=MUTED, font=font(18))
            else:
                x_end = chart_left + (chart_right - chart_left) * value / 100
                if value == 0:
                    draw.ellipse((chart_left - 5, y + 17, chart_left + 5, y + 27), fill=color)
                else:
                    draw.rounded_rectangle((chart_left, y, x_end, y + 45), 8, fill=color)
                label_x = max(chart_left + 12, min(x_end + 10, chart_right - 70))
                draw.text((label_x, y + 8), f"{value:.1f}%", fill=INK, font=font(20, True))
    image.save(FIGURES_DIR / "01_globaler_vergleich.png")


def heat_color(value: float) -> tuple[int, int, int]:
    start = (245, 244, 240)
    end = (179, 75, 75)
    ratio = max(0.0, min(value / 100.0, 1.0))
    return tuple(round(a + (b - a) * ratio) for a, b in zip(start, end))


def save_d4_heatmap() -> None:
    image = Image.new("RGB", (WIDTH, HEIGHT), BACKGROUND)
    draw = ImageDraw.Draw(image)
    draw_header(
        draw,
        "D4 empty-room false presence moved between radio paths",
        "Local presence votes per RX; each cell uses 237 valid samples",
    )
    values = {
        "E0b": [0.0, 12.236, 39.662, 84.810],
        "E0c": [0.0, 13.080, 40.506, 0.0],
        "E0d": [0.0, 83.544, 22.363, 0.0],
    }
    left, top = 260, 230
    cell_w, cell_h = 270, 150
    for col, rx in enumerate(("RX1", "RX2", "RX3", "RX4")):
        draw.text((left + col * cell_w + 95, top - 55), rx, fill=INK, font=font(29, True))
    for row, (run, row_values) in enumerate(values.items()):
        y = top + row * cell_h
        draw.text((105, y + 52), run, fill=INK, font=font(31, True))
        for col, value in enumerate(row_values):
            x = left + col * cell_w
            draw.rounded_rectangle((x, y, x + cell_w - 20, y + cell_h - 20), 18, fill=heat_color(value), outline="#ffffff", width=3)
            text_color = "#ffffff" if value > 55 else INK
            draw.text((x + 65, y + 42), f"{value:.1f}%", fill=text_color, font=font(34, True))
    draw.text((260, 735), "A single local PRESENT_STILL vote was sufficient for global D4 presence.", fill=MUTED, font=font(25))
    image.save(FIGURES_DIR / "02_D4_RX_leerraum_heatmap.png")


def save_d5_link_switching() -> None:
    image = Image.new("RGB", (WIDTH, HEIGHT), BACKGROUND)
    draw = ImageDraw.Draw(image)
    draw_header(
        draw,
        "D5 live: the informative link switched, but quorum was never reached",
        "Per-RX vote rate in two consecutive still-person recordings; denominators 236 and 114 samples",
    )
    left, right, top, bottom = 300, 1300, 235, 700
    first = [0.0, 0.0, 0.0, 100 * 87 / 236]
    second = [0.0, 100 * 1 / 114, 100.0, 0.0]
    colors = ["#6d7f8b", "#a47738", "#2a7f79", "#7f5a83"]
    for tick in range(0, 101, 20):
        y = bottom - (bottom - top) * tick / 100
        draw.line((left, y, right, y), fill=GRID, width=2)
        draw.text((left - 65, y - 12), f"{tick}%", fill=MUTED, font=font(20))
    x1, x2 = 535, 1065
    draw.text((x1 - 55, bottom + 35), "D5 E1", fill=INK, font=font(27, True))
    draw.text((x2 - 95, bottom + 35), "E1 persistence", fill=INK, font=font(27, True))
    for index, rx in enumerate(("RX1", "RX2", "RX3", "RX4")):
        y1 = bottom - (bottom - top) * first[index] / 100
        y2 = bottom - (bottom - top) * second[index] / 100
        draw.line((x1, y1, x2, y2), fill=colors[index], width=5)
        draw.ellipse((x1 - 10, y1 - 10, x1 + 10, y1 + 10), fill=colors[index])
        draw.ellipse((x2 - 10, y2 - 10, x2 + 10, y2 + 10), fill=colors[index])
        label_y = 205 + index * 42
        draw.rectangle((1330, label_y, 1355, label_y + 25), fill=colors[index])
        draw.text((1370, label_y - 3), rx, fill=INK, font=font(21))
    draw.text((x1 + 20, bottom - (bottom - top) * first[3] / 100 - 18), "RX4 36.9%", fill=colors[3], font=font(22, True))
    draw.text((x2 + 20, top - 12), "RX3 100%", fill=colors[2], font=font(22, True))
    draw.text((300, 825), "Global Still-Recall: 0/350 samples. D5-abs E1 also produced 0/276 votes on every RX.", fill=RED, font=font(24, True))
    image.save(FIGURES_DIR / "03_D5_live_RX_linkwechsel.png")


def save_d6_frame_rates(d6_rows: list[dict[str, Any]]) -> None:
    image = Image.new("RGB", (WIDTH, HEIGHT), BACKGROUND)
    draw = ImageDraw.Draw(image)
    draw_header(
        draw,
        "D6 captures are complete and setup-bound, with uneven RX throughput",
        "Raw frames per second from first-to-last host timestamp; dots show RX1–RX4",
    )
    left, right, top, bottom = 150, 1480, 235, 730
    colors = ["#6d7f8b", "#a47738", "#2a7f79", "#7f5a83"]
    for tick in range(0, 31, 5):
        y = bottom - (bottom - top) * tick / 30
        draw.line((left, y, right, y), fill=GRID, width=2)
        draw.text((80, y - 12), f"{tick}", fill=MUTED, font=font(20))
    spacing = (right - left) / len(d6_rows)
    for index, row in enumerate(d6_rows):
        x = left + spacing * (index + 0.5)
        duration = float(row["actual_duration_s"])
        rates = [float(row[f"rx{rx}_positive_count"]) / duration for rx in range(1, 5)]
        for rx_index, value in enumerate(rates):
            y = bottom - (bottom - top) * value / 30
            draw.ellipse((x - 9, y - 9, x + 9, y + 9), fill=colors[rx_index])
        short = str(row["recording_id"]).replace("-neutral-20260809-", "\n")
        draw.multiline_text((x, bottom + 30), short, fill=INK, font=font(19), anchor="ma", align="center", spacing=4)
    for index, rx in enumerate(("RX1", "RX2", "RX3", "RX4")):
        x = 500 + index * 170
        draw.ellipse((x, 815, x + 20, 835), fill=colors[index])
        draw.text((x + 30, 809), rx, fill=INK, font=font(21))
    image.save(FIGURES_DIR / "04_D6_RX_frameraten.png")


def main() -> None:
    api_rows: list[dict[str, Any]] = []
    diagnostic_rows: list[dict[str, Any]] = []
    for path in sorted(item for item in API_DATA_DIR.iterdir() if item.is_dir()):
        row, diagnostics = analyze_api_run(path)
        api_rows.append(row)
        diagnostic_rows.extend(diagnostics)
    d6_rows = [analyze_d6(path) for path in sorted(D6_DATA_DIR.glob("*.raw-csi.v1.meta.json"))]
    rows = api_rows + d6_rows
    write_csv(INVENTORY_PATH, rows)
    write_csv(D4_RX_PATH, diagnostic_rows)
    FIGURES_DIR.mkdir(parents=True, exist_ok=True)
    save_global_comparison()
    save_d4_heatmap()
    save_d5_link_switching()
    save_d6_frame_rates(d6_rows)
    print(f"Wrote {len(rows)} inventory rows to {INVENTORY_PATH}")
    print(f"Wrote {len(diagnostic_rows)} D4/RX rows to {D4_RX_PATH}")
    print(f"Wrote 4 figures to {FIGURES_DIR}")


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Render the mmWave transport comparison in the repository's Bklit style.

The values are copied from the checked before/after and redundancy tables in
``results/2026-09-13_mmwave-transport-ota-vergleich.md``.  This renderer is a
deterministic static export: it keeps the Bklit chart vocabulary (neutral
palette, rounded columns, zero-based grids, direct labels) while preserving
the different units in separate panels instead of inventing a combined score.
"""

from __future__ import annotations

from pathlib import Path

from PIL import Image, ImageDraw, ImageFont


PROJECT_DIR = Path(__file__).resolve().parent.parent
OUTPUT_DIR = PROJECT_DIR / "results" / "mmwave-transport_bklit"

WIDTH, HEIGHT = 1876, 1294
BACKGROUND = "#ffffff"
INK = "#202020"
MUTED = "#747474"
GRID = "#e4e4e0"
BORDER = "#d8d8d2"
PRIMARY = "#4f4f4d"
SECONDARY = "#8c8c88"
ACCENT = "#b34b4b"
SOFT = "#f3f3f0"


BEFORE = "Vorher · 0.1.0"
AFTER = "Nachher · 0.1.2"


def font(size: int, bold: bool = False) -> ImageFont.FreeTypeFont:
    name = "Arial Bold.ttf" if bold else "Arial.ttf"
    return ImageFont.truetype(Path("/System/Library/Fonts/Supplemental") / name, size)


def canvas(kicker: str, title: str, subtitle: str) -> tuple[Image.Image, ImageDraw.ImageDraw]:
    image = Image.new("RGB", (WIDTH, HEIGHT), BACKGROUND)
    draw = ImageDraw.Draw(image)
    draw.rectangle((0, 0, WIDTH - 1, HEIGHT - 1), outline=BORDER, width=2)
    draw.text((54, 51), kicker, fill=MUTED, font=font(24))
    draw.text((54, 108), title, fill=INK, font=font(47, True))
    draw.text((54, 182), subtitle, fill=MUTED, font=font(25))
    draw.rectangle((1662, 51, 1822, 101), outline=BORDER, width=2)
    draw.text((1680, 63), "BKLIT / DATA", fill=INK, font=font(20))
    return image, draw


def footer(draw: ImageDraw.ImageDraw, note: str) -> None:
    draw.line((0, 1159, WIDTH, 1159), fill=BORDER, width=2)
    draw.text((54, 1206), note, fill=MUTED, font=font(23))


def panel(draw: ImageDraw.ImageDraw, bounds: tuple[int, int, int, int], title: str, note: str = "") -> None:
    left, top, right, bottom = bounds
    draw.rounded_rectangle(bounds, 18, fill=BACKGROUND, outline=BORDER, width=2)
    draw.text((left + 30, top + 26), title, fill=INK, font=font(25, True))
    if note:
        draw.text((left + 30, top + 67), note, fill=MUTED, font=font(18))


def grouped_bars(
    draw: ImageDraw.ImageDraw,
    bounds: tuple[int, int, int, int],
    labels: list[str],
    series: list[tuple[str, list[float], str]],
    max_value: float,
    tick_step: float,
    formatter=lambda value: f"{value:g}",
) -> None:
    left, top, right, bottom = bounds
    chart_left, chart_right = left + 95, right - 35
    chart_top, chart_bottom = top + 112, bottom - 88
    ticks = []
    value = 0.0
    while value <= max_value + tick_step / 2:
        ticks.append(value)
        value += tick_step
    for tick in ticks:
        y = chart_bottom - (chart_bottom - chart_top) * tick / max_value
        draw.line((chart_left, y, chart_right, y), fill=GRID, width=2)
        draw.text((left + 25, y - 13), formatter(tick), fill=MUTED, font=font(18))

    group_width = (chart_right - chart_left) / max(len(labels), 1)
    bar_width = min(76, group_width / (len(series) + 1.8))
    for index, label in enumerate(labels):
        center = chart_left + group_width * (index + 0.5)
        start = center - (len(series) * bar_width + (len(series) - 1) * 14) / 2
        for series_index, (_, values, color) in enumerate(series):
            value = values[index]
            x0 = start + series_index * (bar_width + 14)
            y0 = chart_bottom - (chart_bottom - chart_top) * value / max_value
            if value == 0:
                draw.ellipse((x0 + bar_width / 2 - 5, chart_bottom - 5, x0 + bar_width / 2 + 5, chart_bottom + 5), fill=color)
            else:
                draw.rounded_rectangle((x0, y0, x0 + bar_width, chart_bottom), 10, fill=color)
            label_y = max(y0 - 31, chart_top + 3)
            draw.text((x0 + bar_width / 2, label_y), formatter(value), fill=INK, font=font(18, True), anchor="ma")
        # Keep the category label and the legend on separate baselines.  This
        # matters in narrow panels where a one-item legend starts near the
        # first category label.
        draw.text((center, chart_bottom + 17), label, fill=MUTED, font=font(20), anchor="ma")

    legend_y = bottom - 27
    legend_x = chart_left
    for name, _, color in series:
        draw.ellipse((legend_x, legend_y, legend_x + 18, legend_y + 18), fill=color)
        draw.text((legend_x + 28, legend_y - 3), name, fill=MUTED, font=font(18))
        legend_x += 180 + len(name) * 3


def save_arrival_and_loss() -> None:
    image, draw = canvas(
        "01 · ANKUNFT UND VERLUST",
        "Weniger Last, weniger Duplikate, weniger Sequenzverlust",
        "180-Sekunden-Aggregat; Vorher = drei Kopien mit 5 ms Abstand, Nachher = eine Kopie ohne Zusatzabstand",
    )
    panel(draw, (54, 286, 916, 1110), "Ankunft", "Rohdatagramme und eindeutige Ankünfte")
    grouped_bars(
        draw,
        (54, 286, 916, 1110),
        [BEFORE, AFTER],
        [("Roh UDP", [972, 417], PRIMARY), ("Eindeutig", [375, 417], SECONDARY)],
        1100,
        200,
    )
    panel(draw, (960, 286, 1822, 700), "Sequenzverlust", "Niedriger ist besser · getrennte Prozent-Skala")
    grouped_bars(
        draw,
        (960, 286, 1822, 700),
        [BEFORE, AFTER],
        [("Verlustquote", [20.6, 13.1], ACCENT)],
        25,
        5,
        lambda value: f"{value:.0f}%",
    )
    panel(draw, (960, 730, 1822, 1110), "Duplikate", "Niedriger ist besser · eigene absolute Skala")
    grouped_bars(
        draw,
        (960, 730, 1822, 1110),
        [BEFORE, AFTER],
        [("Duplikate", [596, 0], ACCENT)],
        650,
        100,
    )
    footer(draw, "BKLIT-Export · Werte direkt aus der Vorher-/Nachher-Tabelle · Verlust- und Duplikat-Skalen nicht zusammenlegen")
    image.save(OUTPUT_DIR / "01_ankunft_und_verlust.png")


def save_arrival_latency() -> None:
    image, draw = canvas(
        "02 · ANKUNFTSLATENZ",
        "Die lange Latenzspitze wurde kürzer",
        "Server-Ankunftsabstand über die drei 60-Sekunden-Läufe; P95 ist der robustere Tail-Indikator",
    )
    panel(draw, (54, 286, 1822, 1110), "Median und P95", "Millisekunden · niedriger ist besser")
    grouped_bars(
        draw,
        (54, 286, 1822, 1110),
        [BEFORE, AFTER],
        [("Median", [328.7, 389.7], PRIMARY), ("P95", [1363.3, 984.0], SECONDARY)],
        1500,
        300,
        lambda value: f"{value:.0f}",
    )
    footer(draw, "Median: +18,6% (schlechter) · P95: −27,8% (besser) · der End-to-End-Abstand bleibt durch die Sensor-Quellrate begrenzt")
    image.save(OUTPUT_DIR / "02_ankunftslatenz.png")


def save_server_processing() -> None:
    image, draw = canvas(
        "03 · SERVERVERARBEITUNG",
        "Verarbeitung bleibt schnell und die Queue wächst nicht weiter",
        "Receive-to-process-Latenz plus separate Queue-Evidenz; unterschiedliche Queue-Messarten sind ausdrücklich markiert",
    )
    panel(draw, (54, 286, 1060, 1110), "Receive → Process", "Millisekunden · niedriger ist besser")
    grouped_bars(
        draw,
        (54, 286, 1060, 1110),
        [BEFORE, AFTER],
        [("Median", [11.3, 10.3], PRIMARY), ("P95", [20.7, 20.0], SECONDARY)],
        24,
        4,
        lambda value: f"{value:.0f}",
    )
    panel(draw, (1105, 286, 1822, 1110), "Queue-Evidenz", "Nicht identische Messart · kein falscher gemeinsamer Index")
    draw.text((1140, 445), "Vorher", fill=MUTED, font=font(22))
    draw.text((1140, 490), "kumulativer Peak", fill=INK, font=font(25, True))
    draw.text((1140, 535), "8", fill=PRIMARY, font=font(65, True))
    draw.line((1140, 645, 1780, 645), fill=BORDER, width=2)
    draw.text((1140, 695), "Nachher", fill=MUTED, font=font(22))
    draw.text((1140, 740), "neuer Peak / Stichprobe", fill=INK, font=font(25, True))
    draw.text((1140, 785), "0 / 1", fill=SECONDARY, font=font(65, True))
    draw.rounded_rectangle((1140, 920, 1775, 1000), 12, fill=SOFT)
    draw.text((1168, 943), "Queue nicht monoton gewachsen", fill=INK, font=font(22, True))
    footer(draw, "Process-Median: −8,8% · Process-P95: −3,2% · Queue: keine neue Spitze in den Nachher-Läufen")
    image.save(OUTPUT_DIR / "03_serververarbeitung.png")


def save_redundancy() -> None:
    image, draw = canvas(
        "04 · REDUNDANZVERGLEICH",
        "Die zweite UDP-Kopie verbessert die Verluste nicht",
        "Separater 60-Sekunden-Test; eine Kopie bleibt der niedrigste-Latenz-Standard",
    )
    panel(draw, (54, 286, 652, 1110), "Sequenzverlust", "Prozent · niedriger ist besser")
    grouped_bars(
        draw,
        (54, 286, 652, 1110),
        ["1 Kopie", "2 Kopien"],
        [("Verlustquote", [13.1, 13.7], ACCENT)],
        20,
        5,
        lambda value: f"{value:.1f}%",
    )
    panel(draw, (680, 286, 1248, 1110), "Duplikate", "Absolute Datagramme · niedriger ist besser")
    grouped_bars(
        draw,
        (680, 286, 1248, 1110),
        ["1 Kopie", "2 Kopien"],
        [("Duplikate", [0, 105], PRIMARY)],
        120,
        20,
    )
    panel(draw, (1276, 286, 1822, 1110), "Ankunft P95", "Millisekunden · niedriger ist besser")
    grouped_bars(
        draw,
        (1276, 286, 1822, 1110),
        ["1 Kopie", "2 Kopien"],
        [("P95", [984, 1076], SECONDARY)],
        1200,
        200,
    )
    footer(draw, "Ergebnis: 2 Kopien = −0,6 pp Verlust, aber +105 Duplikate und +92 ms P95 · Standard bleibt 1 Kopie")
    image.save(OUTPUT_DIR / "04_redundanzvergleich.png")


def main() -> None:
    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    save_arrival_and_loss()
    save_arrival_latency()
    save_server_processing()
    save_redundancy()
    print(f"Wrote 4 Bklit-style figures to {OUTPUT_DIR}")


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Render the firmware 0.1.5/0.1.6 sequence-loss comparison.

The source values are preserved in
``experiment-reports/2026-09-20_mmwave-sequenzverlustvergleich/data/vergleich.csv``.
The static renderer keeps counts and percentages on separate zero-based scales.
"""

from __future__ import annotations

from pathlib import Path

from PIL import Image, ImageDraw, ImageFont


PROJECT_DIR = Path(__file__).resolve().parent.parent
OUTPUT_DIR = (
    PROJECT_DIR
    / "experiment-reports"
    / "2026-09-20_mmwave-sequenzverlustvergleich"
    / "figures"
)

WIDTH, HEIGHT = 1876, 1294
BACKGROUND = "#ffffff"
INK = "#202020"
MUTED = "#747474"
GRID = "#e4e4e0"
BORDER = "#d8d8d2"
PRIMARY = "#4f4f4d"
SECONDARY = "#9a9a96"
ACCENT = "#b34b4b"
SOFT = "#f3f3f0"

FW_015 = "Firmware 0.1.5"
FW_016 = "Firmware 0.1.6"


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
    draw.text((54, 1206), note, fill=MUTED, font=font(22))


def panel(
    draw: ImageDraw.ImageDraw,
    bounds: tuple[int, int, int, int],
    title: str,
    note: str = "",
) -> None:
    left, top, _, _ = bounds
    draw.rounded_rectangle(bounds, 18, fill=BACKGROUND, outline=BORDER, width=2)
    draw.text((left + 30, top + 26), title, fill=INK, font=font(25, True))
    if note:
        draw.text((left + 30, top + 67), note, fill=MUTED, font=font(18))


def comparison_bars(
    draw: ImageDraw.ImageDraw,
    bounds: tuple[int, int, int, int],
    values: tuple[float, float],
    max_value: float,
    tick_step: float,
    formatter,
    labels: tuple[str, str] = (FW_015, FW_016),
) -> None:
    left, top, right, bottom = bounds
    chart_left, chart_right = left + 100, right - 40
    chart_top, chart_bottom = top + 125, bottom - 92

    tick = 0.0
    while tick <= max_value + tick_step / 2:
        y = chart_bottom - (chart_bottom - chart_top) * tick / max_value
        draw.line((chart_left, y, chart_right, y), fill=GRID, width=2)
        draw.text((left + 25, y - 13), formatter(tick), fill=MUTED, font=font(18))
        tick += tick_step

    colors = (SECONDARY, PRIMARY)
    group_width = (chart_right - chart_left) / 2
    bar_width = min(150, group_width * 0.42)
    for index, (label, value, color) in enumerate(zip(labels, values, colors)):
        center = chart_left + group_width * (index + 0.5)
        x0 = center - bar_width / 2
        y0 = chart_bottom - (chart_bottom - chart_top) * value / max_value
        if value == 0:
            draw.ellipse(
                (center - 7, chart_bottom - 7, center + 7, chart_bottom + 7),
                fill=color,
            )
        else:
            draw.rounded_rectangle((x0, y0, x0 + bar_width, chart_bottom), 12, fill=color)
        draw.text(
            (center, max(y0 - 37, chart_top + 3)),
            formatter(value),
            fill=INK,
            font=font(22, True),
            anchor="ma",
        )
        draw.text((center, chart_bottom + 20), label, fill=MUTED, font=font(20), anchor="ma")


def status_card(
    draw: ImageDraw.ImageDraw,
    bounds: tuple[int, int, int, int],
    version: str,
    verdict: str,
    detail: str,
    filled: bool,
) -> None:
    left, top, right, bottom = bounds
    draw.rounded_rectangle(
        bounds,
        16,
        fill=INK if filled else BACKGROUND,
        outline=INK if filled else BORDER,
        width=2,
    )
    text_color = BACKGROUND if filled else INK
    muted_color = "#d7d7d2" if filled else MUTED
    draw.text((left + 28, top + 25), version, fill=muted_color, font=font(20))
    draw.text((left + 28, top + 72), verdict, fill=text_color, font=font(29, True))
    draw.text((left + 28, bottom - 48), detail, fill=muted_color, font=font(18))


def save_sequence_loss() -> None:
    image, draw = canvas(
        "01 · SEQUENZVERLUST",
        "Sequenzverlust im 60-Sekunden-Vergleich",
        "Firmware 0.1.5 und 0.1.6 · Verlustquote und absolute Lücken bleiben getrennt skaliert",
    )
    panel(draw, (54, 286, 1160, 1110), "Sequenzverlust", "Prozent · niedriger ist besser")
    comparison_bars(
        draw,
        (54, 286, 1160, 1110),
        (0.70, 0.00),
        0.8,
        0.2,
        lambda value: f"{value:.2f}%".replace(".", ","),
    )
    panel(draw, (1205, 286, 1822, 700), "Verlorene Sequenzen", "Absolute Anzahl")
    comparison_bars(
        draw,
        (1205, 286, 1822, 700),
        (2, 0),
        2.5,
        0.5,
        lambda value: f"{value:g}".replace(".", ","),
        ("0.1.5", "0.1.6"),
    )
    panel(draw, (1205, 730, 1822, 1110), "Messumfang", "Neue Sequenzen im selben Zeitfenster")
    draw.text((1240, 845), FW_015, fill=MUTED, font=font(20))
    draw.text((1778, 842), "286", fill=SECONDARY, font=font(42, True), anchor="ra")
    draw.line((1240, 925, 1780, 925), fill=BORDER, width=2)
    draw.text((1240, 972), FW_016, fill=MUTED, font=font(20))
    draw.text((1778, 969), "263", fill=PRIMARY, font=font(42, True), anchor="ra")
    footer(draw, "0.1.5: 2 Lücken bei 286 neuen Sequenzen · 0.1.6: 0 Lücken bei 263 neuen Sequenzen")
    image.save(OUTPUT_DIR / "01-sequenzverlust.png")


def save_ack_delivery() -> None:
    image, draw = canvas(
        "02 · ACK-ZUSTELLUNG",
        "Bestätigte Pakete und vollständige ACK-Timeouts",
        "Zählerdeltas aus zwei gleich langen 60-Sekunden-Fenstern · getrennte absolute Skalen",
    )
    panel(draw, (54, 286, 916, 1110), "Bestätigte Pakete", "Absolute Zählerdifferenz · höher ist besser")
    comparison_bars(
        draw,
        (54, 286, 916, 1110),
        (205, 263),
        300,
        50,
        lambda value: f"{value:.0f}",
        ("0.1.5", "0.1.6"),
    )
    panel(draw, (960, 286, 1822, 1110), "Vollständige ACK-Timeouts", "Absolute Zählerdifferenz · niedriger ist besser")
    comparison_bars(
        draw,
        (960, 286, 1822, 1110),
        (84, 0),
        100,
        20,
        lambda value: f"{value:.0f}",
        ("0.1.5", "0.1.6"),
    )
    footer(draw, "0.1.6 gegenüber 0.1.5: +58 ACKs (+28,3%) · vollständige ACK-Timeouts 84 → 0")
    image.save(OUTPUT_DIR / "02-ack-zustellung.png")


def save_duplicates_and_gate() -> None:
    image, draw = canvas(
        "03 · DUPLIKATE UND GATE",
        "Weniger Duplikate, Sequenzverlust-Gate bestanden",
        "Duplikate bleiben als Retransmissionskosten sichtbar; das Gate bewertet die beobachteten Sequenzlücken",
    )
    panel(draw, (54, 286, 1080, 1110), "Duplikate", "Absolute Anzahl · niedriger ist besser")
    comparison_bars(
        draw,
        (54, 286, 1080, 1110),
        (524, 173),
        600,
        100,
        lambda value: f"{value:.0f}",
    )
    panel(draw, (1125, 286, 1822, 1110), "radar_sequence_loss_free", "Server-Gate im jeweiligen Messfenster")
    status_card(
        draw,
        (1160, 430, 1787, 640),
        FW_015,
        "NICHT BESTANDEN",
        "2 verlorene Sequenzen",
        False,
    )
    status_card(
        draw,
        (1160, 710, 1787, 920),
        FW_016,
        "BESTANDEN",
        "0 verlorene Sequenzen",
        True,
    )
    draw.rounded_rectangle((1160, 965, 1787, 1040), 12, fill=SOFT)
    draw.text((1188, 988), "Duplikate: 524 → 173  (−67,0%)", fill=INK, font=font(21, True))
    footer(draw, "Bestanden belegt dieses 60-s-Fenster; es ist keine allgemeine Garantie für alle späteren Läufe")
    image.save(OUTPUT_DIR / "03-duplikate-und-gate.png")


def main() -> None:
    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    save_sequence_loss()
    save_ack_delivery()
    save_duplicates_and_gate()
    print(f"Wrote 3 Bklit-style figures to {OUTPUT_DIR}")


if __name__ == "__main__":
    main()

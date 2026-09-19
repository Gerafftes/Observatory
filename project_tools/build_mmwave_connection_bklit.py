#!/usr/bin/env python3
"""Render source-backed mmWave connection diagrams in the Bklit visual style.

These diagrams describe the implemented transport and coordinate contracts.
They intentionally contain no newly measured latency, loss, or accuracy data.
"""

from __future__ import annotations

import textwrap
from pathlib import Path

from PIL import Image, ImageDraw

from build_mmwave_transport_bklit import (
    ACCENT,
    BACKGROUND,
    BORDER,
    HEIGHT,
    INK,
    MUTED,
    PRIMARY,
    SECONDARY,
    SOFT,
    WIDTH,
    font,
    footer,
    panel,
)


PROJECT_DIR = Path(__file__).resolve().parent.parent
OUTPUT_DIR = PROJECT_DIR / "project-media" / "diagrams" / "mmwave-connection"


def canvas(kicker: str, title: str, subtitle: str) -> tuple[Image.Image, ImageDraw.ImageDraw]:
    image = Image.new("RGB", (WIDTH, HEIGHT), BACKGROUND)
    draw = ImageDraw.Draw(image)
    draw.rectangle((0, 0, WIDTH - 1, HEIGHT - 1), outline=BORDER, width=2)
    draw.text((54, 51), kicker, fill=MUTED, font=font(24))
    draw.text((54, 108), title, fill=INK, font=font(47, True))
    draw.text((54, 182), subtitle, fill=MUTED, font=font(25))
    draw.rectangle((1648, 51, 1822, 101), outline=BORDER, width=2)
    draw.text((1665, 63), "BKLIT / TECH", fill=INK, font=font(20))
    return image, draw


def wrapped_text(
    draw: ImageDraw.ImageDraw,
    xy: tuple[int, int],
    text: str,
    width: int,
    size: int,
    *,
    color: str = INK,
    bold: bool = False,
    spacing: int = 10,
) -> None:
    lines = textwrap.wrap(text, width=width, break_long_words=False)
    draw.multiline_text(xy, "\n".join(lines), fill=color, font=font(size, bold), spacing=spacing)


def card(
    draw: ImageDraw.ImageDraw,
    bounds: tuple[int, int, int, int],
    step: str,
    title: str,
    body: str,
    *,
    accent: str = PRIMARY,
) -> None:
    left, top, right, bottom = bounds
    draw.rounded_rectangle(bounds, 18, fill=BACKGROUND, outline=BORDER, width=2)
    draw.rounded_rectangle((left + 24, top + 24, left + 76, top + 76), 10, fill=accent)
    draw.text((left + 50, top + 50), step, fill=BACKGROUND, font=font(21, True), anchor="mm")
    wrapped_text(draw, (left + 96, top + 25), title, 20, 25, bold=True)
    wrapped_text(draw, (left + 28, top + 112), body, 29, 21, color=MUTED, spacing=12)


def arrow(
    draw: ImageDraw.ImageDraw,
    start: tuple[int, int],
    end: tuple[int, int],
    *,
    color: str = SECONDARY,
    width: int = 5,
) -> None:
    draw.line((*start, *end), fill=color, width=width)
    x, y = end
    if end[0] >= start[0]:
        head = [(x, y), (x - 18, y - 11), (x - 18, y + 11)]
    else:
        head = [(x, y), (x + 18, y - 11), (x + 18, y + 11)]
    draw.polygon(head, fill=color)


def chip(draw: ImageDraw.ImageDraw, bounds: tuple[int, int, int, int], label: str, color: str) -> None:
    draw.rounded_rectangle(bounds, 14, fill=color)
    left, top, right, bottom = bounds
    draw.text(((left + right) / 2, (top + bottom) / 2), label, fill=BACKGROUND, font=font(21, True), anchor="mm")


def save_ack_flow() -> None:
    image, draw = canvas(
        "01 · ACK-VERBINDUNGSFLUSS",
        "Messung voran, Bestätigung zurück",
        "Der ACK schließt den Pfad Node → WLAN → Server; Wiederholungen behalten dieselbe Identität",
    )
    boxes = [
        (54, 300, 382, 725),
        (484, 300, 812, 725),
        (914, 300, 1242, 725),
        (1344, 300, 1822, 725),
    ]
    card(draw, boxes[0], "1", "HLK-LD2450", "30-Byte-UART-Frame mit bis zu drei lokalen Zielpositionen.")
    card(draw, boxes[1], "2", "ESP32-C3 Node", "JSON-Paket mit node_id, boot_id, sequence und rohen lokalen Koordinaten.")
    card(draw, boxes[2], "3", "UDP über WLAN", "Eine Messung pro Datagramm. sendto() allein bestätigt nur die lokale Übergabe.", accent=SECONDARY)
    card(draw, boxes[3], "4", "Sensing Server", "Sequenz prüfen, Wiederholung deduplizieren, Setup-Transform anwenden und 12-Byte-ACK senden.", accent=ACCENT)
    for left, right in zip(boxes, boxes[1:]):
        arrow(draw, (left[2] + 12, 518), (right[0] - 12, 518))

    draw.text((54, 785), "RÜCKWEG", fill=MUTED, font=font(20, True))
    arrow(draw, (1700, 835), (650, 835), color=ACCENT, width=6)
    draw.text((1175, 795), "ACK · magic + boot_id + sequence", fill=ACCENT, font=font(23, True), anchor="ma")

    draw.rounded_rectangle((54, 905, 1822, 1098), 18, fill=SOFT)
    draw.text((84, 937), "Wenn der ACK ausbleibt", fill=INK, font=font(26, True))
    wrapped_text(
        draw,
        (84, 987),
        "Der Node sendet dieselbe boot_id/sequence erneut. Dadurch kann ein verlorener ACK repariert werden, ohne die Wiederholung als neue Messung zu zählen.",
        112,
        23,
        color=MUTED,
    )
    footer(draw, "Technisches Vertragsdiagramm · Quellcode- und Konfigurationsstand · keine neue Transportmessung")
    image.save(OUTPUT_DIR / "01-ack-verbindungsfluss.png")


def save_timeout_model() -> None:
    image, draw = canvas(
        "02 · TIMEOUT UND RETRY",
        "Vom starren 50-ms-Fenster zum gelernten Zeitbudget",
        "Konfigurationsvergleich; die Werte beschreiben das implementierte Verhalten, nicht eine neue Latenzmessung",
    )
    panel(draw, (54, 300, 680, 1110), "Bisher", "Fester ACK-Timeout")
    draw.text((95, 430), "50 ms", fill=PRIMARY, font=font(78, True))
    wrapped_text(draw, (95, 550), "Jeder Versuch wartete gleich lange. WLAN-/AP-Tails konnten dadurch verfrüht wie Paketverlust wirken.", 37, 24, color=MUTED)
    chip(draw, (95, 790, 310, 850), "8 Versuche", SECONDARY)
    chip(draw, (330, 790, 620, 850), "gleiche sequence", PRIMARY)

    panel(draw, (710, 300, 1822, 1110), "Jetzt", "Adaptiver End-to-End-Timeout mit begrenztem Backoff")
    draw.text((755, 405), "150 ms", fill=ACCENT, font=font(65, True))
    draw.text((1045, 430), "Startwert und Untergrenze", fill=INK, font=font(25, True))
    wrapped_text(draw, (755, 510), "Erfolgreiche Erstversuche aktualisieren SRTT + 4 × RTTVAR. Ein erfolgreicher Retry hebt die gelernte Untergrenze für den aktuellen Boot an.", 68, 23, color=MUTED)

    draw.text((755, 690), "Beispiel ab dem Standardwert", fill=INK, font=font(23, True))
    values = ["150", "300", "600", "1200", "2000"]
    x = 755
    for index, value in enumerate(values):
        box_width = 140 if index < 4 else 170
        chip(draw, (x, 750, x + box_width, 820), f"{value} ms", ACCENT if index == 4 else PRIMARY)
        if index < len(values) - 1:
            arrow(draw, (x + box_width + 8, 785), (x + box_width + 42, 785), width=4)
        x += box_width + 55

    wrapped_text(draw, (755, 890), "Exponentiell pro Versuch, aber auf 2000 ms begrenzt. Nach komplett fehlender Bestätigung wird die Basis ebenfalls erhöht.", 67, 23, color=MUTED)
    footer(draw, "Quellen: Firmware-Kconfig und ACK-Timing-Logik · 150 ms initial/minimal · 2000 ms Maximum · bis zu 8 Versuche")
    image.save(OUTPUT_DIR / "02-timeout-und-retry.png")


def save_transform_ownership() -> None:
    image, draw = canvas(
        "03 · TRANSFORM-VERANTWORTUNG",
        "Die Raumtransformation gehört jetzt dem Server-Setup",
        "Der Node liefert Identität plus Rohkoordinaten; Legacy-Raumfelder bleiben nur für Kompatibilität erhalten",
    )

    draw.text((54, 295), "LEGACY-PFAD", fill=MUTED, font=font(20, True))
    legacy = [(54, 340, 520, 585), (705, 340, 1171, 585), (1356, 340, 1822, 585)]
    card(draw, legacy[0], "A", "Transform am Node", "Origin, Yaw und X-Invertierung müssen auf den ESP geschrieben werden.", accent=SECONDARY)
    card(draw, legacy[1], "B", "Raumfelder im Paket", "room_x_mm und room_z_mm transportieren bereits transformierte Werte.", accent=SECONDARY)
    card(draw, legacy[2], "C", "Collector", "Kompatibilitätsweg für ältere Empfänger und Standalone-Diagnostik.", accent=SECONDARY)
    arrow(draw, (532, 462), (693, 462))
    arrow(draw, (1183, 462), (1344, 462))

    draw.text((54, 640), "AKTUELLER SERVER-PFAD", fill=ACCENT, font=font(20, True))
    current = [(54, 685, 520, 1015), (705, 685, 1171, 1015), (1356, 685, 1822, 1015)]
    card(draw, current[0], "1", "Node sendet roh", "node_id, boot_id, sequence sowie lokale target_x_mm/target_y_mm sind maßgeblich.")
    card(draw, current[1], "2", "Setup-v2 am Server", "Der Server wählt den versiegelten Transform passend zur node_id und wendet ihn genau einmal an.", accent=ACCENT)
    card(draw, current[2], "3", "Raum + UI", "Bounds, Kalibrierung und Anzeige verwenden dieselbe setupgebundene Raumposition.")
    arrow(draw, (532, 850), (693, 850), color=ACCENT)
    arrow(draw, (1183, 850), (1344, 850), color=ACCENT)

    footer(draw, "mmWave bleibt Ground Truth / Referenz · die Radarposition wird nie zum Eingabefeature des WLAN-CSI-Prädiktors")
    image.save(OUTPUT_DIR / "03-transform-verantwortung.png")


def main() -> None:
    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    save_ack_flow()
    save_timeout_model()
    save_transform_ownership()
    print(f"Wrote 3 Bklit-style connection diagrams to {OUTPUT_DIR}")


if __name__ == "__main__":
    main()

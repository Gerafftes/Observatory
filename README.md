# WLAN-CSI-Projekt [![Hack Club Stardance](https://img.shields.io/badge/Hack%20Club-Stardance-ec3750?style=flat-square&logo=hackclub&logoColor=white)](https://stardance.hackclub.com/projects/25673)

[![ESP32](https://img.shields.io/badge/ESP32-E7352C?style=flat-square&logo=espressif&logoColor=white)](https://www.espressif.com/en/products/socs/esp32)
[![Status](https://img.shields.io/badge/status-work%20in%20progress-orange)](#)

> **You are being sensed.**
>
> This room has a secret system, a machine that watches the Wi-Fi every second
> it is running. I know because I built it.
>
> I designed the machine to detect movement, but it sees every disturbance.
> Reflections caused by ordinary people—people like you. Signals the old
> algorithms considered “irrelevant.”
>
> They couldn’t understand them, so I decided I would. But I needed a partner,
> another sensor with the precision to reveal the truth.
>
> Bound by physics, we work without cameras. You’ll never see us. But moving or
> motionless, if you change the signal… we’ll find *you*.

*Frei nach dem Intro der Serie
[*Person of Interest*](https://warnertv.de/serie/sendungen/person-of-interest).*

Berichtsdokumentation eines experimentellen WLAN-CSI-Systems mit einem
ESP32-S3-Sender, vier ESP32-S3-Empfängern und einem HLK-LD2450 als späterer
Referenzsensor.

**Stand: 14. August 2026**

## Forschungsfrage

Wie zuverlässig kann ein ESP32-basiertes WLAN-CSI-System Bewegungen und
Atemrhythmen im Raum erfassen, und welche physikalischen Grenzen ergeben sich
dabei?

## Aktueller Stand

| Bereich | Status | Einordnung |
|---|---|---|
| 1 TX / 4 RX | Transport nachgewiesen | Alle vier RX liefern gebundene Raw-CSI-Daten im gemeinsamen Raster. |
| D4 Bewegung | Experimentell | Grobe Bewegungsalarme wurden reduziert, Still-Präsenz bleibt unzuverlässig. |
| D5 Still-Präsenz | Livetest nicht bestanden | Trotz aktiver Kalibrierung blieben 350 von 350 Samples bei anwesender stiller Person `ABSENT`. |
| D6 Position | Software vorbereitet | Neun feste Punkte, aufbaugebundene Aufnahmen und Blindtests sind implementiert; ein realer, blind bestandener Positionsindex fehlt. |
| mmWave-Referenz | Teilweise in Betrieb | ESP32-C3, CSI-WLAN und Statusdienst sind nachgewiesen. Der LD2450-Datenpfad wartet auf die vollständige Verbindung mit PCB-01. |
| Gesamtsystem | Nicht validiert | Es gibt noch keinen gemeinsamen realen PASS für Classification, Position und mmWave-Referenz. |

Der verbindliche technische Wiedereinstieg mit den neuesten Hardwareprüfungen
steht in
[`08-aktueller-arbeitsstand-d6-und-position.md`](08-aktueller-arbeitsstand-d6-und-position.md).

## Systemaufbau

Fünf ESP32-S3-Boards bilden das WLAN-CSI-System: Ein TX-Board sendet, RX1 bis
RX4 empfangen CSI-Rohdaten. Ein separates ESP32-C3-Board bindet später den
HLK-LD2450 als unabhängige Referenz an.

<img src="images/esp32-s3-boards.jpeg" alt="Fünf beschriftete ESP32-S3-Boards für TX und RX1 bis RX4" width="480">

<img src="images/hlk-ld2450-mmwave-sensor.jpeg" alt="HLK-LD2450 24G mmWave-Sensor mit Antenne und Anschlusskabel" width="480">

Eine geplante Hub-PCB soll die Verkabelung des Zielaufbaus vereinfachen.

<img src="images/hub-pcb-schema.png" alt="KiCad-Skizze einer Hub-PCB für TX und vier RX-Boards" width="480">

Die Gerber- und Bohrdaten der mmWave-Platine PCB-01 sind unter
[`hardware/pcb-01/`](hardware/pcb-01/) mit Prüfsumme und Fertigungshinweis
abgelegt.

## Messansatz

Die Entwicklung ist bewusst in getrennte Stufen gegliedert:

1. **Raw-CSI und Transport:** Paketquelle, RX-Identität, Raster, Datenrate und
   Verluste werden geprüft.
2. **Classification:** Bewegung, Still-Präsenz und Leerraum werden getrennt
   bewertet. Fehlende Evidenz darf keine Anwesenheit behaupten.
3. **D6-Position:** Das System klassifiziert ausschließlich neun vermessene
   Bodenpunkte, `unknown` oder `ambiguous`. Eine scheinpräzise kontinuierliche
   Heatmap gilt nicht als Positionsmessung.
4. **mmWave-Referenz:** Der Radarwert darf für Kalibrierung und zurückgehaltene
   Wahrheit verwendet werden, aber nicht als Eingabe des späteren
   WLAN-CSI-Prädiktors.
5. **Blinde Prüfung:** Training, Vorhersage und Wahrheit bleiben durch getrennte
   Dateien und Hashbindungen voneinander isoliert.

Der D6-Ablauf bleibt fail-closed:

```text
physischer Aufbau
→ Setup-Siegel
→ 25-s-Preflight
→ 65-s-Leerraumkalibrierung
→ P01–P09-Training
→ Positionsindex
→ Blindtests
→ gemeinsame Qualitätsgates
→ Live-Anzeige
```

## Bisherige belastbare Ergebnisse

- Eine technische Discovery am 9. August lieferte `2.612` Frames von RX1 bis
  RX4 bei `0` Drops. Das belegt Transport, Bindung und Rasterstabilität, nicht
  die Erkennungs- oder Positionsgüte.
- Ein historisch versiegelter D6-Preflight bestand mit `2.545` Frames und
  `0` Drops. Nach einem Sidecar-Fix bestand das neu versiegelte Setup erneut
  mit `2.701` Frames und `0` Drops.
- Die anschließende 65-Sekunden-Leerraumkalibrierung schrieb `6.102` Frames bei
  `0` Drops und bestand die strikte Offline-Inspektion.
- Der erste reale D5-Still-Livetest erreichte `0 %` Still-Recall. D5 bleibt
  deshalb deaktiviert und experimentell.
- Die D6- und mmWave-Softwaretests belegen die Softwarekette, aber keine reale
  RF-, Radar-, Classification- oder Positionsleistung.

Die D6-Siegel vom 9. August gehören zu ihrem damaligen Serverartefakt und
physischen Aufbau. Durch die spätere Ergänzung von ESP32-C3, PCB und
mmWave-Hardware ist der aktuelle Aufbau verändert und noch nicht als Setup v2
versiegelt.

## Dokumentation lesen

| Datei | Inhalt |
|---|---|
| [`00-status-und-annahmen.md`](00-status-und-annahmen.md) | Aufbau, Annahmen, Koordinaten und offene Punkte |
| [`01-projektjournal.md`](01-projektjournal.md) | Chronologischer Entwicklungsverlauf |
| [`02-versuchslog.md`](02-versuchslog.md) | Durchgeführte Versuche |
| [`03-messprotokoll.md`](03-messprotokoll.md) | Messabläufe und Qualitätsregeln |
| [`04-auswertung-bis-problemfrage.md`](04-auswertung-bis-problemfrage.md) | Auswertung entlang der Forschungsfrage |
| [`05-erfolge-niederlagen-und-aenderungen.md`](05-erfolge-niederlagen-und-aenderungen.md) | Erfolge, Fehlschläge und Kursänderungen |
| [`06-ruview-anpassungen.md`](06-ruview-anpassungen.md) | Lokale Änderungen an RuView |
| [`07-screenshot-nachweise.md`](07-screenshot-nachweise.md) | Visuelle Nachweise und Fehlerbilder |
| [`08-aktueller-arbeitsstand-d6-und-position.md`](08-aktueller-arbeitsstand-d6-und-position.md) | Verbindlicher aktueller D6-/mmWave-Wiedereinstieg |
| [`hardware/pcb-01/`](hardware/pcb-01/) | Gerber- und Bohrdaten der mmWave-Platine PCB-01 |
| [`results/`](results/) | Ausführliche Ergebnisberichte |
| [`templates/messblatt.md`](templates/messblatt.md) | Vorlage für neue Messungen |

Wichtige Ergebnisberichte:

- [D5: Offline-Replay und experimentelle Präsenzkalibrierung](results/2026-07-26_D5_offline-replay-und-experimentelle-praesenzkalibrierung.md)
- [D5: realer Still-Livetest](results/2026-07-26_D5_realer-still-livetest.md)
- [D6: Setupaufnahme und TX-Firmwareidentität](results/2026-08-09_D6_setupaufnahme-und-TX-firmwareidentitaet.md)
- [D6: Setup-Siegel und Preflight](results/2026-08-09_D6_setup-siegel-und-preflight.md)
- [D6: Sidecar-Fix, Neusiegelung und Leerraumkalibrierung](results/2026-08-09_D6_sidecar-fix-neusiegelung-und-preflight.md)

## Repository-Struktur

```text
wifi-csi-dokumentation/
├── README.md
├── 00-status-und-annahmen.md
├── 01-projektjournal.md
├── 02-versuchslog.md
├── 03-messprotokoll.md
├── 04-auswertung-bis-problemfrage.md
├── 05-erfolge-niederlagen-und-aenderungen.md
├── 06-ruview-anpassungen.md
├── 07-screenshot-nachweise.md
├── 08-aktueller-arbeitsstand-d6-und-position.md
├── data/
├── hardware/
│   └── pcb-01/
├── images/
├── logs/
├── results/
├── skizzen/
└── templates/
```

## Veröffentlichungs- und Dokumentationsregeln

- Dieses Repository ist eine Berichtsdokumentation, keine
  Schritt-für-Schritt-Anleitung.
- Softwaretests, erfolgreiche Übertragung und erfolgreiche Flashvorgänge
  werden nicht als reale Sensor- oder Messvalidierung ausgegeben.
- Rohdaten werden nur veröffentlicht, wenn Umfang, Datenschutz und
  Reproduzierbarkeit geprüft sind. Lokale Pfade in älteren Berichten sind kein
  Hinweis darauf, dass die zugehörigen Dateien bereits auf GitHub liegen.
- Fehlversuche bleiben dokumentiert, weil sie für die physikalischen und
  technischen Grenzen des Systems relevant sind.
- Geheimnisse, WLAN-Zugangsdaten und private Gerätekennungen gehören nicht in
  die öffentliche Dokumentation.

Die verwendete Softwarebasis für RX-Firmware und Sensing-Server ist
[ruvnet/RuView](https://github.com/ruvnet/RuView). RuView bleibt ein separates
Repository und wird hier nur in seiner für das Projekt verwendeten Rolle
dokumentiert.

# Observatory
[**Deutsch**](README.md) · [English](README.en.md)

[![Hack Club Stardance](https://img.shields.io/badge/Hack%20Club-Stardance-ec3750?style=flat-square&logo=hackclub&logoColor=white)](https://stardance.hackclub.com/projects/25673)
[![ESP32](https://img.shields.io/badge/ESP32-E7352C?style=flat-square&logo=espressif&logoColor=white)](https://www.espressif.com/en/products/socs/esp32)
[![Status](https://img.shields.io/badge/status-experimental-orange)](#aktueller-validierungsstand)
[![License: PolyForm Noncommercial 1.0.0](https://img.shields.io/badge/License-PolyForm%20Noncommercial%201.0.0-blue.svg)](LICENSE.md)

## Projekt ansehen

**[Observatory auf Stardance ansehen](https://stardance.hackclub.com/projects/25673)**

Eine öffentliche Live-Demo gibt es derzeit nicht: Die echte Messung benötigt
den festen lokalen Raumaufbau mit TX, RX1 bis RX4 und dem Referenzsensor. Der
[aktuelle technische Wiedereinstieg](08-aktueller-arbeitsstand-d6-und-position.md)
zeigt, was bereits real geprüft wurde und welches Hardware-Gate als Nächstes
folgt.

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

*Nach dem Intro der Serie
[*Person of Interest*](https://en.wikipedia.org/wiki/Person_of_Interest_(TV_series)).*

Ein experimentelles 1TX-/4RX-WLAN-CSI-System, das Präsenz, Bewegung und neun
feste Raumpositionen ohne Kamera untersucht und mmWave nur als unabhängige
Referenz verwendet.

<img src="images/esp32-s3-boards.jpeg" alt="Die fünf beschrifteten ESP32-S3-Boards des Observatory-Aufbaus: RX1 bis RX4 und TX" width="420">


## Schnelleinstieg

Dieses Repository enthält die Berichtsdokumentation und benötigt keinen Build:

```bash
git clone https://github.com/Gerafftes/Observatory.git
cd Observatory
```

Für einen ersten reproduzierbaren Check öffnest du den technischen
Wiedereinstieg und arbeitest die Setup-/Preflight-Gates durch. Dafür brauchst
du keine lokale Runtime in diesem Dokumentations-Repository.

Die drei wichtigsten Einstiegspunkte sind:

1. [Aktueller D6-/mmWave-Arbeitsstand](08-aktueller-arbeitsstand-d6-und-position.md)
2. [Ergebnisberichte](results/)
3. [PCB-01-Fertigungsdaten](hardware/pcb-01/)

Die verwendete RX-Firmware und der Sensing-Server stammen aus dem separaten
Upstream-Projekt [ruvnet/RuView](https://github.com/ruvnet/RuView). Die lokalen
Projektanpassungen sind unter
[`06-ruview-anpassungen.md`](06-ruview-anpassungen.md) dokumentiert.

## Was Observatory kann

- Raw-CSI von vier ESP32-S3-Empfängern verlustfrei und aufbaugebunden
  erfassen.
- Paketquelle, RX-Identität, Subcarrier-Raster, Datenrate und Drops vor einer
  Messung prüfen.
- Bewegung, Still-Präsenz und Position als getrennte Evidenzstufen behandeln.
- Position ausschließlich als P01 bis P09, `unknown` oder `ambiguous`
  ausgeben, statt eine scheinpräzise kontinuierliche Heatmap zu erfinden.
- Training, Blindvorhersage und Wahrheit durch getrennte Dateien und
  Hashbindungen voneinander isolieren.
- Einen HLK-LD2450 als Kalibrierungs- und Referenzsensor verwenden, ohne seine
  Werte in den späteren WLAN-CSI-Prädiktor einzuspeisen.

## Lokale Checks (optional)

Dieses Dokumentations-Repository braucht keine lokale Runtime. Der vorhandene
Offline-Auswertetest lässt sich mit Python 3 ausführen:

```bash
python3 -m unittest scripts/tests/test_evaluate_d5_replay.py
```

Der Check verarbeitet keine neuen Sensorsignale und beweist keine Live-
Hardwarequalität.

## Forschungsfrage

Wie zuverlässig kann ein ESP32-basiertes WLAN-CSI-System Bewegungen und
Atemrhythmen im Raum erfassen, und welche physikalischen Grenzen ergeben sich
dabei?

## Aktueller Validierungsstand

**Stand: 14. August 2026**

| Bereich | Status | Was dieser Status belegt |
|---|---|---|
| 1 TX / 4 RX | Transport nachgewiesen | Alle vier RX lieferten gebundene Raw-CSI-Daten im gemeinsamen Raster. |
| D4 Bewegung | Experimentell | Grobe Bewegungsalarme wurden reduziert; Still-Präsenz bleibt unzuverlässig. |
| D5 Still-Präsenz | Livetest nicht bestanden | Bei anwesender stiller Person blieben 350 von 350 Samples `ABSENT`. |
| D6 Position | Software vorbereitet | Aufnahmen, neun Punkte und Blindtests sind implementiert; ein realer blind bestandener Index fehlt. |
| mmWave-Referenz | Teilweise in Betrieb | ESP32-C3, CSI-WLAN und Statusdienst sind nachgewiesen; der reale LD2450-Datenpfad ist noch nicht vollständig validiert. |
| Gesamtsystem | Nicht validiert | Es gibt noch keinen gemeinsamen realen PASS für Classification, Position und mmWave. |

Softwaretests, ein erfolgreicher Flashvorgang oder eine verlustfreie
Übertragung gelten hier ausdrücklich nicht als Nachweis realer Sensor- oder
Positionsgenauigkeit.

## Benutzeroberfläche

### mmWave-Kalibrierungsassistent

Der Assistent führt durch Verbindung, Ausrichtung, Abdeckung, Zonen, Training,
Blindtest und Ergebnis. Der Screenshot zeigt den geprüften Fehlerzustand
`Server nicht erreichbar` mit HTTP 502, nicht eine verbundene Radaraufnahme.

<img src="images/ui/mmwave-calibration-server-unreachable.png" alt="Siebenstufiger mmWave-Kalibrierungsassistent mit nicht erreichbarem Server und HTTP-502-Status" width="900">

### Experiment-Cockpit

Das neue Cockpit hält Setup-Profil, WiFi-Workflow, Aufnahmen und die getrennte
mmWave-Referenz in einer Ansicht. Die Aufnahmen unten stammen aus einem
simulierten Lauf ohne angeschlossene Sensoren.

<img src="images/ui/experiment-cockpit-setup.png" alt="Experiment-Cockpit mit Setup-Profil, Statusübersicht und simuliertem Hardwarezustand" width="900">

<img src="images/ui/experiment-cockpit-guide.png" alt="Experiment-Cockpit mit Raum-, TX- und RX-Positionen sowie wartender mmWave-Referenz" width="900">

<img src="images/ui/experiment-cockpit-workflow-guide.png" alt="Experiment-Cockpit mit Workflow-Guide und zehn gesperrten beziehungsweise freigeschalteten Phasen" width="900">

#### Kurzanleitung

1. **Setup-Profil öffnen:** Raummaße (Länge/Höhe/Breite), TX-Position und
   RX1–RX4-Positionen eintragen, danach **Neue Profilversion speichern**.
2. **Experiment-Run anlegen:** Versuchsname und gespeichertes Profil wählen,
   dann **WiFi-Experiment anlegen**.
3. **Setup versiegeln:** Der Guide speichert Profil und Hash für diesen Run.
   Das ist nur ein Software-Schritt.
4. **Leere WiFi-Baseline:** Raum leer halten, Leerkalibrierung starten und
   abschließen. Ohne CSI-Nodes stoppt der Ablauf bei **zu wenigen
   RX-Fingerprints**.
5. **mmWave-Kalibrierung:** Der Radar liefert separat Positionspakete,
   Abdeckung und CSI-Zeitbezug. Ohne Sensor bleibt der Status **Wartet auf
   mmWave**.
6. **Blindtest:** Eine reproduzierbare Reihenfolge erzeugen und neue CSI-
   Aufnahmen ohne Ground Truth sammeln. Ohne RX-Nodes gibt es keine gültigen
   Aufnahmen; der Software-Demo-Run bleibt nur eine Ansicht.
7. **Prediction und Truth trennen:** Zuerst nur die WiFi-Prediction
   registrieren, danach die getrennte Radar-/Positionswahrheit aufdecken.
8. **Evaluation und Report:** Accuracy, Coverage, Fehlerdistanz,
   Confusion Matrix und Qualitätsgates auswerten und anschließend den Report
   schreiben. Ohne Hardware bleibt er ausdrücklich
   **SOFTWARE-ONLY / UNVALIDATED** und beweist keine echte Messqualität.

## Wie es funktioniert

WLAN-Signale verändern sich durch Reflexion, Abschattung und Multipath. Ein
TX-Board erzeugt den kontrollierten Funkverkehr; RX1 bis RX4 messen die
komplexen CSI-Werte aus verschiedenen Raumpositionen. Observatory vergleicht
diese Messungen mit einer aufbaugebundenen Leerraumreferenz.

Die Positionsbestimmung ist absichtlich diskret. Statt zwischen ungemessenen
Koordinaten zu interpolieren, lernt D6 Fingerprints für neun markierte
Bodenpunkte. Reicht die Evidenz nicht aus oder passen mehrere Punkte ähnlich
gut, muss das System `unknown` oder `ambiguous` ausgeben.

Der mmWave-Sensor dient nur als unabhängige Referenz für Kalibrierung und
Blindbewertung. Dadurch wird verhindert, dass der WLAN-CSI-Prädiktor während
des Tests indirekt die richtige Antwort erhält.

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

## Belastbare Ergebnisse

- Die technische Discovery vom 9. August lieferte `2.612` Frames von RX1 bis
  RX4 bei `0` Drops. Das belegt Transport, Bindung und Rasterstabilität, nicht
  die Erkennungs- oder Positionsgüte.
- Zwei historische versiegelte D6-Preflights bestanden mit `2.545`
  beziehungsweise `2.701` Frames und jeweils `0` Drops.
- Eine 65-Sekunden-Leerraumkalibrierung schrieb `6.102` Frames bei `0` Drops
  und bestand die strikte Offline-Inspektion.
- Der erste reale D5-Still-Livetest erreichte `0 %` Still-Recall. D5 bleibt
  deshalb deaktiviert und experimentell.
- Durch die spätere Ergänzung von ESP32-C3, PCB und mmWave-Hardware ist der
  aktuelle physische Aufbau verändert und noch nicht als Setup v2 versiegelt.

### D4/D5/D6-Ergebnisdiagramme

Der [technische D4/D5/D6-Ergebnisbericht](results/2026-08-23_D4-D5-D6_technischer-ergebnisbericht.md)
ist mit der [Laufübersicht über 25 Aufnahmen](results/2026-08-23_D4-D5-D6_laufuebersicht.csv),
der [D4-RX-Diagnostik](results/2026-08-23_D4_RX_diagnostik.csv) und dem
[Diagrammvertrag inklusive QA](results/2026-08-23_D4-D5-D6_chart-map.md)
verknüpft. Die vier geprüften Diagramme sind hier direkt sichtbar:

<table>
<tr>
<td><a href="results/2026-08-23_D4-D5-D6_figures/01_globaler_vergleich.png"><img src="results/2026-08-23_D4-D5-D6_figures/01_globaler_vergleich.png" alt="Globaler Vergleich von D4 und D5-abs für Leerraum-Fehlpräsenz und Still-Recall" width="480"></a><br><strong>Globaler Vergleich</strong><br>D5-abs entfernt die Leerraum-Fehlpräsenz, verliert dabei aber den Still-Recall. Deshalb ist die Variante insgesamt nicht bestanden.</td>
<td><a href="results/2026-08-23_D4-D5-D6_figures/02_D4_RX_leerraum_heatmap.png"><img src="results/2026-08-23_D4-D5-D6_figures/02_D4_RX_leerraum_heatmap.png" alt="D4-Leerraumstimmen als RX-Heatmap" width="480"></a><br><strong>D4-RX-Leerraum-Heatmap</strong><br>Die Fehlpräsenz entsteht lokal und wechselt zwischen den RX-Pfaden. Ein einzelner stabiler Verursacher ist nicht erkennbar.</td>
</tr>
<tr>
<td><a href="results/2026-08-23_D4-D5-D6_figures/03_D5_live_RX_linkwechsel.png"><img src="results/2026-08-23_D4-D5-D6_figures/03_D5_live_RX_linkwechsel.png" alt="D5-Livetest mit RX-Linkwechseln" width="480"></a><br><strong>D5-Live-Linkwechsel</strong><br>Die Präsenzstimmen wechseln zwischen RX3 und RX4. Das Zwei-RX-Quorum bleibt dadurch aus, und die stille Person wird nicht erkannt.</td>
<td><a href="results/2026-08-23_D4-D5-D6_figures/04_D6_RX_frameraten.png"><img src="results/2026-08-23_D4-D5-D6_figures/04_D6_RX_frameraten.png" alt="D6-RX-Frameraten über fünf Aufnahmen" width="480"></a><br><strong>D6-RX-Frameraten</strong><br>Alle vier RX sind in den fünf technischen Aufnahmen vertreten. Das belegt Erfassung und Transport, aber keine Positionsgenauigkeit.</td>
</tr>
</table>

D5-abs senkt die globale Leerraum-Fehlpräsenz von D4s `75,2 %` auf `0 %`,
senkt aber zugleich den Still-Recall von `88,4 %` auf `0 %` und ist deshalb
insgesamt **nicht bestanden**. D6 ist technisch vollständig und
setupgebunden; daraus folgt keine Aussage über Erkennungs- oder
Positionsgenauigkeit.

Wichtige Nachweise:

- [D5: Offline-Replay und experimentelle Präsenzkalibrierung](results/2026-07-26_D5_offline-replay-und-experimentelle-praesenzkalibrierung.md)
- [D5: realer Still-Livetest](results/2026-07-26_D5_realer-still-livetest.md)
- [D6: Setupaufnahme und TX-Firmwareidentität](results/2026-08-09_D6_setupaufnahme-und-TX-firmwareidentitaet.md)
- [D6: Setup-Siegel und Preflight](results/2026-08-09_D6_setup-siegel-und-preflight.md)
- [D6: Sidecar-Fix, Neusiegelung und Leerraumkalibrierung](results/2026-08-09_D6_sidecar-fix-neusiegelung-und-preflight.md)

## Hardware

Der WLAN-CSI-Aufbau besteht aus fünf ESP32-S3-Boards. Ein separates ESP32-C3-
Board bindet den HLK-LD2450 später als unabhängigen Referenzsensor an.

<img src="images/hlk-ld2450-mmwave-sensor.jpeg" alt="HLK-LD2450 24G mmWave-Referenzsensor" width="460">

PCB-01 verbindet den ESP32-C3 mit dem mmWave-Referenzpfad. Die folgende
Fertigungsvorschau zeigt den verwendeten Platinenstand:

<img src="images/pcb-01-preview.webp" alt="Fertigungsvorschau von PCB-01 mit ESP32-C3-Footprint, C1, C2 und Anschluss U2" width="460">

Die [Gerber- und Bohrdaten von PCB-01](hardware/pcb-01/) liegen mit SHA-256 und
Fertigungshinweis im Repository.

Als Gehäuse für die ESP32-S3-Boards ist das externe MakerWorld-Modell
[*ESP32 S3 Wroom Case*](https://makerworld.com/de/models/1456361-esp32-s3-wroom-case#profileId-1517915)
vorgesehen. Wegen seiner MakerWorld Standard Digital File License wird die
STL nicht erneut im Repository bereitgestellt. Weitere Hinweise stehen unter
[`hardware/esp32-s3-case/`](hardware/esp32-s3-case/).

## Dokumentation

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
| [`08-aktueller-arbeitsstand-d6-und-position.md`](08-aktueller-arbeitsstand-d6-und-position.md) | Verbindlicher D6-/mmWave-Wiedereinstieg |
| [`results/`](results/) | Ausführliche Ergebnisberichte |
| [`templates/messblatt.md`](templates/messblatt.md) | Vorlage für neue Messungen |

## Lizenz

Dieses Projekt steht unter der
[PolyForm Noncommercial License 1.0.0](LICENSE.md). Nutzung, Änderung und
Weitergabe sind nur im Rahmen der dort definierten nichtkommerziellen Zwecke
erlaubt.

## Credits

- [ruvnet/RuView](https://github.com/ruvnet/RuView) stellt die Softwarebasis
  für RX-Firmware und Sensing-Server bereit.
- [Espressif](https://www.espressif.com/en/products/socs/esp32) entwickelt die
  verwendeten ESP32-Plattformen.
- Das ESP32-S3-WROOM-Gehäuse wurde von MakerWorld-Nutzer
  [`aiekick`](https://makerworld.com/de/models/1456361-esp32-s3-wroom-case#profileId-1517915)
  erstellt und wird unter der MakerWorld Standard Digital File License
  angeboten.
- Der filmische Prolog ist vom Intro der Serie
  [*Person of Interest*](https://warnertv.de/serie/sendungen/person-of-interest)
  inspiriert.

## Dokumentationsregeln

- Fehlversuche bleiben dokumentiert, weil sie technische und physikalische
  Grenzen sichtbar machen.
- Rohdaten werden erst veröffentlicht, wenn Umfang, Datenschutz und
  Reproduzierbarkeit geprüft sind.
- Geheimnisse, WLAN-Zugangsdaten und private Gerätekennungen gehören nicht in
  die Veröffentlichung.

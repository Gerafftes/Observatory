<h1 align="center">Observatory</h1>

<p align="center">
  Erforsche, was WLAN über Bewegung im Raum verrät – ohne Kamera.
  Observatory hilft dir, Funksignale mit ESP32-Boards aufzuzeichnen und
  auszuwerten. Ziel ist die fortlaufende Ortung einer Person.
</p>

<p align="center">
  <a href="https://stardance.hackclub.com/projects/25673">Stardance</a> ·
  <a href="#schnelleinstieg">Schnellstart</a> ·
  <a href="#was-observatory-kann">Möglichkeiten</a> ·
  <a href="experiment-reports/README.md">Ergebnisse</a> ·
  <a href="hardware/README.md">Hardware</a> ·
  <a href="docs/research/rf-sensing-sources.md">Quellen</a> ·
  <a href="README.en.md">English</a>
</p>

<p align="center">
  <a href="https://stardance.hackclub.com/projects/25673"><img src="https://img.shields.io/badge/Hack%20Club-Stardance-ec3750?style=flat-square&amp;logo=hackclub&amp;logoColor=white" alt="Hack Club Stardance"></a>
  <a href="#aktueller-validierungsstand"><img src="https://img.shields.io/badge/status-experimental-orange" alt="Experimental status"></a>
  <a href="LICENSE.md"><img src="https://img.shields.io/badge/License-PolyForm%20Noncommercial%201.0.0-blue.svg" alt="PolyForm Noncommercial 1.0.0 license"></a>
</p>

<table>
  <tr>
    <td align="center" width="50%">
      <a href="project-media/esp32-s3-boards.jpeg"><img src="project-media/esp32-s3-boards.jpeg" alt="Die fünf beschrifteten ESP32-S3-Boards des Observatory-Aufbaus: RX1 bis RX4 und TX" width="100%"></a><br>
      <sub>ESP32-S3-Boards: RX1 bis RX4 und TX</sub>
    </td>
    <td align="center" width="50%">
      <a href="project-media/mmwave-breadboard-setup.jpeg"><img src="project-media/mmwave-breadboard-setup-hero.jpeg" alt="Vorläufiger Breadboard-Aufbau mit HLK-LD2450 und ESP32-C3" width="48%"></a>
      <a href="hardware/pcb-03/"><img src="hardware/pcb-03/preview/PCB-03-top.png" alt="PCB-03-Platinenvorschau für den ESP32-C3-/HLK-LD2450-Referenzknoten" width="48%"></a><br>
      <sub>mmWave: Breadboard-Aufbau und PCB-03-Platinenvorschau</sub>
    </td>
  </tr>
</table>

## Projekt ansehen

**[Projekt auf Stardance ansehen](https://stardance.hackclub.com/projects/25673)** · **[Oberfläche ohne Sensoren ausprobieren](#schnelleinstieg)**

Du möchtest verstehen, wie sich Menschen über WLAN-Signale erkennen lassen?
Observatory verbindet Rohdatenerfassung, eine Experiment-Oberfläche und
dokumentierte Versuche. So kannst du nachvollziehen, was die Daten tatsächlich
zeigen und wo die Methode an Grenzen stößt.

**Bereits belegt:** Erfassung mit vier Empfängern, ein Funkvergleich zwischen
RX1 und RX4 sowie 60 Minuten stabiler TX-only-Erfassung. Zuverlässige
fortlaufende WiFi-Ortung ist noch in Entwicklung. Eine öffentliche Live-Demo
gibt es derzeit nicht.

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

## Inhaltsverzeichnis

- [Schnelleinstieg](#schnelleinstieg)
- [Was Observatory kann](#was-observatory-kann)
- [Aktueller Validierungsstand](#aktueller-validierungsstand)
- [Benutzeroberfläche](#benutzeroberfläche)
- [Wie es funktioniert](#wie-es-funktioniert)
- [Hardware](#hardware)
- [Dokumentation](#dokumentation)
- [Lizenz und Credits](#lizenz-und-credits)

## Schnelleinstieg

Starte mit der Simulation, um die Oberfläche ohne Sensoren kennenzulernen.
Dafür brauchst du Git und eine Rust-/Cargo-Umgebung; Details zum Quellstand
stehen im [Software-Einstieg](software/README.md).

```bash
git clone https://github.com/Gerafftes/Observatory.git
cd Observatory/software/ruview/v2
cargo run -p wifi-densepose-sensing-server --no-default-features -- \
  --source simulate --http-port 3002 --ws-port 3001
```

Öffne anschließend `http://127.0.0.1:3002/ui/index.html#sensing`.

1. **Erkunden:** Setup und Messablauf in der [Cockpit-Anleitung](software/experiment-cockpit.md) kennenlernen.
2. **Einordnen:** [Versuchsergebnisse und Grenzen](experiment-reports/README.md) lesen.
3. **Vorbereiten:** Für eigene Messungen [Hardware und Revisionen](hardware/README.md) prüfen.

> [!NOTE]
> Die Simulation zeigt UI und Workflow mit künstlichen Daten (`SOFTWARE-ONLY / UNVALIDATED`). Reale Messungen benötigen den lokalen Hardwareaufbau und eine eigene Kalibrierung.

## Was Observatory kann

- **Signale untersuchen:** Raw-CSI von vier ESP32-S3-Empfängern erfassen;
  Quelle, Datenrate, Zeitfenster und Lücken prüfen.
- **Versuche nachvollziehen:** Aufbau und Aufnahmen an Versionen binden;
  Training, Blindvorhersage und Vergleichswahrheit getrennt halten.
- **Funkstrecken vergleichen:** RX1↔RX4 zusätzlich zu den vier TX→RX-Strecken
  erfassen. Der technische Paartest ist bestanden; sein Positionsnutzen ist offen.

**Das nächste Ziel:** Eine Person soll sich frei durch den kalibrierten Raum
bewegen können, während das System ihre Position allein aus WiFi-Daten schätzt.
Die Kalibrierung soll durch langsames Ablaufen erfolgen; zwei mmWave-Sensoren
sollen Positionslabels und eine unabhängige Vergleichsreferenz liefern.

Der bisherige D6-Ansatz liefert diskrete Punkte oder Zonen mit `unknown` oder
`ambiguous` bei unsicherer Evidenz. Freie Ortung, Mehrpersonenbetrieb und die
Übertragung auf einen zweiten Aufbau bleiben zu prüfen.

## Aktueller Validierungsstand

**Stand: 29. Oktober 2026.** Die Angaben fassen gespeicherte Befunde zusammen.

| Bereich | Ergebnis | Grenze |
|---|---|---|
| Vier-RX-Erfassung | 180-s-Basis bestanden: 173/173 gemeinsame gültige Fenster | Kein Nachweis der Positionsgenauigkeit |
| RX1↔RX4-Paartest | 300 s bestanden: 2.581/2.578 qualifizierte Peerframes, beide Richtungen 293/293 gültige Fenster | Positionsnutzen und Vier-RX-Mesh offen |
| 60 Minuten TX-only | Nach Offline-Neubewertung bestanden: 533.429 Frames, 3.589/3.593 gültige gemeinsame Fenster (99,89 %; mindestens 90 % gefordert) | Ursprünglicher automatischer Abschluss fehlgeschlagen; kein globaler Firmware-Freeze daraus |
| Zwei mmWave-Referenzen | Gemeinsame Abnahme offen | Raum-/Zeitzuordnung und Referenzgenauigkeit noch zu prüfen |
| Kontinuierliche WiFi-Ortung | In Entwicklung | Laufkalibrierung und unabhängige Blindprüfung noch nicht als Gesamtfunktion nachgewiesen |

Der 60-Minuten-Lauf wurde zweimal identisch neu ausgewertet; sein Zeitnachweis
wurde aus Originalbelegen rekonstruiert. Der Fehler des damaligen Online-Abschlusses
bleibt dokumentiert. Ausführliche Dauerlaufbelege liegen privat außerhalb des
Repositorys; neuere Entwicklungsberichte sind noch nicht veröffentlicht.

Frühere D5-abs-Versuche beseitigten Leerraum-Fehlpräsenz, verloren aber den
Still-Recall vollständig und scheiterten insgesamt. Atem-/Herzfrequenzindikatoren
bleiben experimentell und sind nicht physiologisch validiert.

> [!IMPORTANT]
> Bestandene Erfassungs- und Softwareprüfungen belegen weder verlustfreie Funkübertragung noch zuverlässige Ortung. Sequenzlücken bleiben dokumentiert; Radarpositionen und simulierte Anzeigen sind keine WiFi-Ortungsnachweise.

## Benutzeroberfläche

Im Experiment-Cockpit kannst du Setup-Profile, Kalibrierung, Blindaufnahmen und
Auswertung verwalten. Die [Cockpit-Anleitung](software/experiment-cockpit.md)
führt durch den Ablauf und den mmWave-Kalibrierungsassistenten.

<a href="software/experiment-cockpit.md"><img src="project-media/ui/experiment-cockpit-setup.png" alt="Experiment-Cockpit mit Setup-Profil, Statusübersicht und simuliertem Hardwarezustand" width="760"></a>

*Die Abbildung zeigt einen simulierten Softwarezustand ohne angeschlossene Sensoren.*

## Wie es funktioniert

Ein ESP32-S3 sendet WLAN-Pakete; vier Empfänger messen Channel State Information
(CSI), also den Einfluss des Raums auf den Funkkanal. Bewegung, Abschattung und
Reflexionen verändern diese Signale. Der Host sammelt und prüft die Rohdaten.

Für die geplante Laufkalibrierung folgen auf eine Leerraumaufnahme langsame Wege
durch den Raum mit mmWave als Referenz. Getrennte Trainings- und Prüfsitzungen
sollen zeigen, ob das WiFi-Modell neue Bewegungen ohne Radar als Eingang orten kann.
Die [Architekturseite](experiment-reports/architecture/architecture.md) erklärt
die vorhandenen Datenpfade und den bisherigen D6-Ansatz.

<p align="center">
  <a href="experiment-reports/architecture/architecture.md"><img src="experiment-reports/architecture/figures/mmwave-connection/01-ack-verbindungsfluss.png" alt="BKLIT-Diagramm des mmWave-ACK-Verbindungsflusses vom HLK-LD2450 über den ESP32-C3 und WLAN bis zum Sensing Server" width="760"></a>
</p>
<p align="center"><sub>Das Diagramm zeigt einen mmWave-Referenzpfad. WiFi-CSI und die gemeinsame Zwei-Radar-Kalibrierung sind hier nicht dargestellt.</sub></p>

## Hardware

Für den WiFi-Aufbau brauchst du fünf ESP32-S3-Boards: TX1 und RX1 bis RX4.
Zwei HLK-LD2450 mit ESP32-C3 sind als Kalibrierungs- und Referenzknoten vorgesehen.
Oben siehst du den früheren Breadboard-Aufbau und die PCB-03-Vorschau.

- [PCB-03: aktuelle Revision, KiCad-Quellen und Fertigungsdaten](hardware/pcb-03/)
- [Breadboard: CAD, Befestigungsteile und Bauteile](hardware/breadboard/README.md)
- [MMWAVE1/2: Firmware und Anbindung](software/ruview/firmware/esp32-mmwave-node/README.md)

PCB-01 und PCB-02 bleiben in der [Hardwareübersicht](hardware/README.md) als
frühere Revisionen dokumentiert; für den aktuellen Aufbau ist **PCB-03** zu verwenden.

### RX-Status-LED

Im Normalbetrieb: RX1 blau, RX2 grün, RX3 rot, RX4 violett.
Weitere Zustände erkennst du an Farbe und Blinkfolge:

<a href="devlogs/RX_Farbmuster.png"><img src="devlogs/RX_Farbmuster.png" alt="RX-LED-Farben und Blinkmuster für Normalbetrieb, Start, WLAN/IP, fehlende TX1-CSI, Initialisierungsfehler, Transportfehler, OTA und interne Fehler" width="560"></a>

## Dokumentation

| Dein nächster Schritt | Einstieg |
|---|---|
| Versuche und Ergebnisse prüfen | [Historische Ergebnisberichte](experiment-reports/README.md) |
| Software bauen und Herkunft prüfen | [Software-Einstieg](software/README.md) · [RuView-Anpassungen](docs/project/06-ruview-anpassungen.md) |
| Eigene Messungen vorbereiten | [Messprotokoll](docs/project/03-messprotokoll.md) · [Messblatt](measurement-templates/messblatt.md) |
| Entwicklung und Quellen nachvollziehen | [Projektjournal](docs/project/01-projektjournal.md) · [Forschungsquellen](docs/research/rf-sensing-sources.md) |

## Lizenz und Credits

Observatory-eigene Inhalte stehen unter [PolyForm Noncommercial 1.0.0](LICENSE.md).
Die Software basiert auf [ruvnet/RuView](https://github.com/ruvnet/RuView);
Drittkomponenten behalten ihre jeweiligen Lizenz- und Notice-Dateien.

- [Espressif](https://www.espressif.com/en/products/socs/esp32): ESP32-Plattformen.
- [aiekick auf MakerWorld](https://makerworld.com/de/models/1456361-esp32-s3-wroom-case#profileId-1517915): ESP32-S3-WROOM-Gehäuse unter MakerWorld Standard Digital File License.
- [*Person of Interest*](https://warnertv.de/serie/sendungen/person-of-interest): Inspiration für den Prolog.

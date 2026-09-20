# Ergebnisübersicht

[English](README.en.md)

Dieses Verzeichnis ist die kanonische, nach Datum sortierte Übersicht der
gemessenen BLL-Ergebnisse. Jeder Eintrag ist ein eigenständiges Paket: Der
Bericht liegt als `README.md` direkt neben seinen Diagrammen, Tabellen und
Renderhinweisen.

## Quelle der Wahrheit

`experiment-reports/` ist die einzige aktuelle Quelle für Ergebnisnachweise.
`software/ruview/results/` bleibt ein historischer Software-Snapshot und wird
weder automatisch synchronisiert noch als neuere Messung interpretiert.

Die Pakete heißen `YYYY-MM-DD_<Serie>_<Ergebnistyp>`. Versuchskennungen wie
`D4`, `E0` oder `RX` bleiben großgeschrieben; beschreibende Bestandteile sind
kleingeschriebene ASCII-Slugs. Innerhalb eines Pakets gilt:

- `README.md`: Ergebnis, Grenzen und nächste Entscheidung
- `figures/`: direkt eingebettete Diagramme oder Screenshots
- `data/`: tabellarische Ableitungen, nicht die Rohaufnahmen
- `chart-map.md` und `rendering.md`: Zuordnung, Herkunft und Render-QA

Die Tabelle ist bewusst **neueste zuerst** sortiert. Ordnernamen allein sind
auf GitHub und im Dateisystem meist aufsteigend sortiert und ersetzen diesen
Index daher nicht.

## Ergebniskatalog

| Datum | Serie | Ergebnistyp | Evidenzklasse | Bewertung | Bericht | Primärdiagramm |
|---|---|---|---|---|---|---|
| 2026-09-20 | mmWave | Sequenzverlustvergleich | zwei bereitgestellte 60-Sekunden-Fenster | `0.1.6` **bestanden**; 0 verlorene Sequenzen | [Bericht](2026-09-20_mmwave-sequenzverlustvergleich/README.md) | [Sequenzverlust](2026-09-20_mmwave-sequenzverlustvergleich/figures/01-sequenzverlust.png) |
| 2026-09-13 | mmWave | Transportvergleich | drei kontrollierte Vorher-/Nachher-Läufe plus Redundanztest | **besser, aber nicht verlustfrei** | [Bericht](2026-09-13_mmwave_transport-vergleich/README.md) | [Ankunft und Verlust](2026-09-13_mmwave_transport-vergleich/figures/01-ankunft-und-verlust.png) |
| 2026-08-30 | mmWave | Runtime-Audit | aufgezeichnete akzeptierte Pakete | **nicht bestanden**; Sequenzlücke | [Bericht](2026-08-30_mmwave_runtime-audit/README.md) | — |
| 2026-08-23 | D4/D5/D6 | technischer Gesamtbericht | 25 Aufnahmen, Replay und setupgebundene Techniknachweise | D5-abs **nicht bestanden**; D6 nur technisch | [Bericht](2026-08-23_D4-D5-D6_technischer-bericht/README.md) | [Globaler Vergleich](2026-08-23_D4-D5-D6_technischer-bericht/figures/01-globaler-vergleich.png) |
| 2026-08-09 | D6 | Sidecar-Fix, Neusiegelung und Preflight | Live-Runner plus strikte Offline-Inspektion | Recorder-Preflight und Leerraumkalibrierung **bestanden** | [Bericht](2026-08-09_D6_sidecar-fix-neusiegelung-und-preflight/README.md) | — |
| 2026-08-09 | D6 | versiegelter Preflight | setupgebundener 25-Sekunden-Preflight | Recorder-Preflight **bestanden**; Live-Visualisierung nicht belegt | [Bericht](2026-08-09_D6_setup-siegel-und-preflight/README.md) | — |
| 2026-08-09 | D6 | Setupaufnahme und TX-Identität | Read-only-Hardware- und Konfigurationsnachweis | vorbereitend; damaliger Aufbau noch nicht versiegelt | [Bericht](2026-08-09_D6_setupaufnahme-und-tx-firmwareidentitaet/README.md) | — |
| 2026-07-26 | E0d/E1b | unabhängige Bestätigung | zweites Leerraum-/Still-Paar | RX4-Hypothese verworfen; RX3 nur vorläufig | [Bericht](2026-07-26_E0d-E1b_unabhaengige-bestaetigung/README.md) | — |
| 2026-07-26 | E0c/E1 | Still-Person-Trennung | ein Leerraum-/Still-Paar | messbar, aber nicht unabhängig bestätigt | [Bericht](2026-07-26_E0c-E1_still-person-separation/README.md) | — |
| 2026-07-26 | E0b/E0c | Mac-Positions-A/B-Test | ein räumlicher A/B-Wechsel | RX4-Effekt gestützt; keine allgemeine Distanzfunktion | [Bericht](2026-07-26_E0b-E0c_mac-position-ab-test/README.md) | — |
| 2026-07-26 | D5 | realer Still-Livetest | physischer Livetest | **nicht bestanden** | [Bericht](2026-07-26_D5_still-livetest/README.md) | — |
| 2026-07-26 | D5 | Offline-Replay und Präsenzkalibrierung | Softwaretests und Replay | Softwarepfad bestanden; reale Kalibrierung damals offen | [Bericht](2026-07-26_D5_offline-replay-und-praesenzkalibrierung/README.md) | — |
| 2026-07-26 | D4/E0b | sauberer Leerraumtest | kontrollierter Leerraumlauf | **nicht bestanden** | [Bericht](2026-07-26_D4-E0b_sauberer-leerraum/README.md) | — |
| 2026-07-26 | D4/E0 | Leerraum-Mischlauf | Lauf mit zwei unmarkierten Raumzutritten | **unklar**; keine gültige FPR | [Bericht](2026-07-26_D4-E0_leerraum/README.md) | — |
| 2026-07-18 | fester Raum | Livevisualisierungs-Diagnose | Browser- und Laufbeobachtung | Datenpfad sichtbar; kein Positionsnachweis | [Bericht](2026-07-18_fester-raum_live-visualisierung-diagnose/README.md) | [Livevisualisierung](2026-07-18_fester-raum_live-visualisierung-diagnose/figures/01-live-visualisierung.png) |
| 2026-06-28 | G2 | RX-Verteilungs-Qualitätscheck | Logger, Serverlog und Screenshot | Erfassung gut; Visualisierung unzuverlässig | [Bericht](2026-06-28_G2_rx-verteilung-qualitaetscheck/README.md) | [Livevisualisierung](2026-06-28_G2_rx-verteilung-qualitaetscheck/figures/01-live-visualisierung.png) |
| 2026-06-28 | G1 | Guard-500-ms-Qualitätscheck | API-Samples und kurzer Serverlog | Workaround gestützt; physische Synchronität nicht belegt | [Bericht](2026-06-28_G1_guard500ms-qualitaetscheck/README.md) | — |
| 2026-06-28 | A0–A3 | Qualitätscheck | vier frühe Messreihen | technisch brauchbar; Zuverlässigkeit noch offen | [Bericht](2026-06-28_A0-A3_qualitaetscheck/README.md) | — |

## Aktuelle Primärdiagramme

### mmWave-Transport

[![Sequenzverlust mit Firmware 0.1.5 und 0.1.6](2026-09-20_mmwave-sequenzverlustvergleich/figures/01-sequenzverlust.png)](2026-09-20_mmwave-sequenzverlustvergleich/README.md)

Firmware `0.1.6` erreichte im bereitgestellten 60-Sekunden-Fenster `0,00 %`
Sequenzverlust; das Gate `radar_sequence_loss_free` bestand. Zwei weitere
Diagramme, die exakten Tabellenwerte und die Evidenzgrenze stehen direkt im
[Ergebnispaket](2026-09-20_mmwave-sequenzverlustvergleich/README.md).

### D4/D5/D6

[![Globaler Vergleich von D4, D5 Replay und D5-abs](2026-08-23_D4-D5-D6_technischer-bericht/figures/01-globaler-vergleich.png)](2026-08-23_D4-D5-D6_technischer-bericht/README.md)

D5-abs entfernt in den ausgewerteten Läufen die Leerraum-Fehlpräsenz, verliert
aber zugleich den Still-Recall und ist insgesamt nicht bestanden. Alle vier
Diagramme, CSV-Ableitungen und Herkunftshinweise stehen direkt im
[Ergebnispaket](2026-08-23_D4-D5-D6_technischer-bericht/README.md).

Alle Aussagen bleiben an ihre jeweilige Setup-Serie gebunden. Ein bestandener
Transport- oder Recorder-Test ist kein Beleg für Erkennungs- oder
Positionsgenauigkeit.

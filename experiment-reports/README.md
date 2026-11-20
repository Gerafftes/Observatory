# Ergebnisübersicht

[English](README.en.md)

Öffentlicher historischer Katalog, neueste Einträge zuerst. Alle ursprünglichen Katalogzeilen bleiben erhalten. Ein Bericht ist nur verlinkt, wenn er in diesem öffentlichen Ausschnitt enthalten ist. Lokale Quelltabellen, Screenshots, Rohdaten, Generatoren und Renderdokumentation bleiben bei den Originalen. Diese Dokumentationsableitung wiederholt keine Messungen oder Tests.

| Datum | Serie | Ergebnistyp | Evidenzklasse | Bewertung | Bericht |
|---|---|---|---|---|---|
| 2026-09-20 | mmWave | Sequenzverlustvergleich | zwei bereitgestellte 60-Sekunden-Fenster | `0.1.6` **bestanden**; 0 verlorene Sequenzen | [Bericht](2026-09-20_mmwave-sequenzverlustvergleich/README.md) |
| 2026-09-13 | mmWave | Transportvergleich | drei kontrollierte Vorher-/Nachher-Läufe plus Redundanztest | **besser, aber nicht verlustfrei** | historischer Bericht; in diesem Ausschnitt nicht enthalten |
| 2026-08-30 | mmWave | Runtime-Audit | aufgezeichnete akzeptierte Pakete | **nicht bestanden**; Sequenzlücke | historischer Bericht; in diesem Ausschnitt nicht enthalten |
| 2026-08-23 | D4/D5/D6 | technischer Gesamtbericht | 25 Aufnahmen, Replay und setupgebundene Techniknachweise | D5-abs **nicht bestanden**; D6 nur technisch | [Bericht](2026-08-23_D4-D5-D6_technischer-bericht/README.md) |
| 2026-08-09 | D6 | Sidecar-Fix, Neusiegelung und Preflight | Live-Runner plus strikte Offline-Inspektion | Recorder-Preflight und Leerraumkalibrierung **bestanden** | historischer Bericht; in diesem Ausschnitt nicht enthalten |
| 2026-08-09 | D6 | versiegelter Preflight | setupgebundener 25-Sekunden-Preflight | Recorder-Preflight **bestanden**; Live-Visualisierung nicht belegt | historischer Bericht; in diesem Ausschnitt nicht enthalten |
| 2026-08-09 | D6 | Setupaufnahme und TX-Identität | Read-only-Hardware- und Konfigurationsnachweis | vorbereitend; damaliger Aufbau noch nicht versiegelt | [Bericht](2026-08-09_D6_setupaufnahme-und-tx-firmwareidentitaet/README.md) |
| 2026-07-26 | E0d/E1b | unabhängige Bestätigung | zweites Leerraum-/Still-Paar | RX4-Hypothese verworfen; RX3 nur vorläufig | historischer Bericht; in diesem Ausschnitt nicht enthalten |
| 2026-07-26 | E0c/E1 | Still-Person-Trennung | ein Leerraum-/Still-Paar | messbar, aber nicht unabhängig bestätigt | historischer Bericht; in diesem Ausschnitt nicht enthalten |
| 2026-07-26 | E0b/E0c | Mac-Positions-A/B-Test | ein räumlicher A/B-Wechsel | RX4-Effekt gestützt; keine allgemeine Distanzfunktion | historischer Bericht; in diesem Ausschnitt nicht enthalten |
| 2026-07-26 | D5 | realer Still-Livetest | physischer Livetest | **nicht bestanden** | historischer Bericht; in diesem Ausschnitt nicht enthalten |
| 2026-07-26 | D5 | Offline-Replay und Präsenzkalibrierung | Softwaretests und Replay | Softwarepfad bestanden; reale Kalibrierung damals offen | historischer Bericht; in diesem Ausschnitt nicht enthalten |
| 2026-07-26 | D4/E0b | sauberer Leerraumtest | kontrollierter Leerraumlauf | **nicht bestanden** | [Bericht](2026-07-26_D4-E0b_sauberer-leerraum/README.md) |
| 2026-07-26 | D4/E0 | Leerraum-Mischlauf | Lauf mit zwei unmarkierten Raumzutritten | **unklar**; keine gültige FPR | [Bericht](2026-07-26_D4-E0_leerraum/README.md) |
| 2026-07-18 | fester Raum | Livevisualisierungs-Diagnose | Browser- und Laufbeobachtung | Datenpfad sichtbar; kein Positionsnachweis | historischer Bericht; in diesem Ausschnitt nicht enthalten |
| 2026-06-28 | G2 | RX-Verteilungs-Qualitätscheck | Logger, Serverlog und Screenshot | Erfassung gut; Visualisierung unzuverlässig | historischer Bericht; in diesem Ausschnitt nicht enthalten |
| 2026-06-28 | G1 | Guard-500-ms-Qualitätscheck | API-Samples und kurzer Serverlog | Workaround gestützt; physische Synchronität nicht belegt | [Bericht](2026-06-28_G1_guard500ms-qualitaetscheck/README.md) |
| 2026-06-28 | A0–A3 | Qualitätscheck | vier frühe Messreihen | technisch brauchbar; Zuverlässigkeit noch offen | historischer Bericht; in diesem Ausschnitt nicht enthalten |

Alle Aussagen bleiben an ihren damaligen Aufbau und ihr Zeitfenster gebunden. Bestandene Transport- oder Recorderprüfungen belegen weder Personenerkennung, physiologische Gültigkeit, Positionsgenauigkeit noch qualifizierte Ground Truth. Der Vergleich vom 20. September beruht auf einer bereitgestellten Tabelle zweier Fenster ohne Roh-Snapshots, genaue Startzeitpunkte oder Wiederholungen. D4/D5/D6 enthält keine unabhängig validierte mmWave-Ground-Truth. Historische Diagramme sind bewusst ausgelassen; die enthaltenen Berichte bewahren Zahlen und Grenzen.

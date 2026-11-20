# Ergebnisse und Grenzen

Diese Übersicht trennt beobachtete technische Ergebnisse von offenen Leistungsnachweisen. Eine ausführliche Übersicht der datierten Versuche und Diagramme steht im [Ergebniskatalog](../../experiment-reports/README.md).

| Thema | Ergebnis | Grenze |
| --- | --- | --- |
| D4/D5-Anwesenheit | D4 zeigte in drei Leerraumläufen 535/711 positive Samples. D5-abs erreichte 0/273 Fehlpräsenzen, aber auch 0/276 Still-Recall. | Die untersuchten Verfahren bestanden die Anwesenheitsaufgabe nicht insgesamt. [Bericht](../../experiment-reports/2026-08-23_D4-D5-D6_technischer-bericht/README.md) |
| D6-Aufnahme | Fünf technisch abgeschlossene Roh-CSI-Aufnahmen mit allen vier RX; vier mit Setupbindung, Discovery nur Inventur. | Die zwei Setupserien nicht gemeinsam kalibrieren. Keine Ground Truth und kein Nachweis zuverlässiger Live-Positionen. [Bericht](../../experiment-reports/2026-08-23_D4-D5-D6_technischer-bericht/README.md) |
| mmWave-Transport | Im dokumentierten 60-s-Fenster mit Firmware 0.1.6 wurde keine verlorene Sequenz ausgewiesen. | Zwei bereitgestellte Vergleichsfenster; keine allgemeine Verlustfreiheit oder Genauigkeitsabnahme. [Bericht](../../experiment-reports/2026-09-20_mmwave-sequenzverlustvergleich/README.md) |
| Vier RX und RX1↔RX4 | Am 29. Oktober: Basis 173/173 gemeinsame Fenster; Paarlauf in beiden Richtungen 293/293 Fenster. | Datenqualität und technischer Paartest; Positionsnutzen und Vier-RX-Mesh offen. [Abschluss vom 29. Oktober](../project/rx-unified-tag14-result-20261029.md) |
| 60 Minuten TX-only | Nach Offline-Neubewertung 3.589/3.593 gültige gemeinsame Fenster; die 90-%-Grenze wurde erreicht. | Der ursprüngliche Online-Abschluss schlug fehl; daraus folgt kein globaler Firmware-Freeze. Die detaillierten Laufbelege sind nicht Teil des öffentlichen Repositorys. [Projekt-README](../../README.md#aktueller-validierungsstand) |
| Kontinuierliche WiFi-Ortung | Entwicklungsziel. | Laufkalibrierung, zwei geprüfte Radare und unabhängige Blindabnahme sind als Gesamtfunktion nicht belegt. |

Historische Fehlversuche und widersprüchlich wirkende Zwischenstände bleiben mit Datum und jeweiligen Bedingungen erhalten. Ein späterer erfolgreicher Lauf ändert nicht rückwirkend das Ergebnis eines früheren Laufs. Messungen, Offlineauswertungen, Simulation und Softwaretests sind unterschiedliche Evidenzklassen.

Weiter: [Positionierung](../software/positioning.md) · [Forschungsquellen](sources.md) · [Projektstatus](../project/status.md).

## Forschungsfrage und Auswertungsplan

Der erhaltene [historische Auswertungsplan](../project/04-auswertung-bis-problemfrage.md) trennt Bewegung/Anwesenheit, ruhige Atmung, Zonen und den Vergleich eines kontrollierten TX mit Routerverkehr. Seine Juni-Pilotwerte sind wegen unzureichender Labelung keine belastbaren Trefferquoten; positives Offline-Replay und gescheiterter Livetransfer bleiben getrennt.

Für spätere Bewertungen nennt er False Positives/Negatives pro RX, Referenzfehler und Confidence der Atemfrequenz sowie Zonenverwechslungen und den Einfluss von Funkpfaden, Abstand, Bewegung, Mehrpersonenüberlagerung, Phase und Raumwechsel. Diese Größen sind ein Prüfplan, keine abgenommenen Fähigkeiten. Die damalige Drei-RX-Mindestidee ersetzt nicht heutige Vier-RX-Kriterien.

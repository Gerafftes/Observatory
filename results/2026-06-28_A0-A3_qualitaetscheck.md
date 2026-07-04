# Qualitätscheck Messreihen A0 bis A3 — 2026-06-28

## Datensatz und Grain

Quelle: RuView API `GET /api/v1/sensing/latest`

Einheit der Auswertung: ein API-Snapshot pro Sekunde. Pro Snapshot wurden die kompakte CSV-Zeile und die vollständige JSON-Antwort gespeichert.

## Geprüfte Messordner

| Messung | Ordner | Soll-Dauer | Samples | Logger-Fehler |
|---|---|---:|---:|---:|
| A0 leerer Raum | `2026-06-28_00-57-47_A0_leerer_raum` | 60 s | 60 | 0 |
| A1 Person steht Mitte | `2026-06-28_01-05-06_A1_person_steht_mitte` | 60 s | 60 | 0 |
| A2 Person läuft | `2026-06-28_01-07-19_A2_person_laeuft` | 60 s | 60 | 0 |
| A3 Atmung sitzend | `2026-06-28_01-08-32_A3_atmung_sitzend` | 180 s | 180 | 0 |

## Verbindungs- und Node-Qualität

| Messung | Node-IDs in allen Samples | Fehlende Nodes | Samples mit 4 Nodes und 64 Subcarriern |
|---|---:|---:|---:|
| A0 | 1, 2, 3, 4 | 0/60 | 45/60 = 75.0 % |
| A1 | 1, 2, 3, 4 | 0/60 | 55/60 = 91.7 % |
| A2 | 1, 2, 3, 4 | 0/60 | 58/60 = 96.7 % |
| A3 | 1, 2, 3, 4 | 0/180 | 169/180 = 93.9 % |

Bewertung: Die Mehrknoten-Erfassung ist gelungen. Alle vier Nodes waren in jeder Messung sichtbar. Einzelne Snapshots enthalten jedoch `subcarrier_count=0`; diese sollten für spätere Detailauswertungen markiert oder gefiltert werden.

## Kompakte Signalübersicht

Mittelwerte über die CSV-Summary:

| Messung | Presence-True | häufigste Motion-Klasse | mean RSSI | Varianz | Motion-Power | Breathing-Power | Signalqualität |
|---|---:|---|---:|---:|---:|---:|---:|
| A0 leerer Raum | 58/60 | `present_moving` | -57.6 dBm | 35.5 | 57.0 | 52.7 | 0.44 |
| A1 steht | 55/60 | `present_still` | -58.4 dBm | 37.3 | 58.3 | 52.1 | 0.39 |
| A2 läuft | 60/60 | `present_still` / `present_moving` gemischt | -57.5 dBm | 39.4 | 59.8 | 62.2 | 0.40 |
| A3 Atmung | 173/180 | `present_still` | -58.4 dBm | 37.6 | 57.0 | 59.1 | 0.40 |

## Erste Interpretation

- Die Datenerfassung selbst funktioniert: vier RX-Nodes, keine Loggerfehler, fortlaufende Ticks.
- Bewegung/Person verändert die aggregierten Features leicht. A2 zeigt im Vergleich zu A0/A1 höhere Varianz, Motion-Power und besonders höhere Breathing-Power.
- A3 ist als ruhige Messung plausibel, weil `present_still` dominiert.
- A0 ist nicht als echte Negativklasse sauber: Der leere Raum wurde überwiegend als `presence=True` und oft als `present_moving` klassifiziert. Das ist ein wichtiger False-Positive-Befund.
- Die Vitalwerte sind noch nicht belastbar. Dass A0 trotz leerem Raum Atem- und Herzfrequenzwerte enthält, zeigt, dass diese Werte aktuell nicht ohne Referenzmessung als echte Vitalzeichen interpretiert werden dürfen.

## Qualitätsrisiken für die spätere Auswertung

| Risiko | Schwere | Begründung | Umgang |
|---|---|---|---|
| False Positives im leeren Raum | hoch | A0 meldet fast durchgehend `presence=True` | Klassifikation nicht ungeprüft übernehmen; eigene Schwellwerte/Features auswerten |
| Einzelne Snapshots ohne Subcarrier | mittel | A0 hat nur 75 % vollständig gültige 4x64-Snapshots | Für Analyse Qualitätsfilter `subcarrier_count == 64` je Node verwenden |
| Vitalzeichen ohne Referenz | hoch | A0 enthält BPM-Werte, obwohl keine Person erwartet wurde | Atem/Herz nur mit mmWave/Fitnessuhr oder manuellem Atemzählen bewerten |
| Timing/Fusion | mittel | RuView meldete zuvor `Multistatic fusion failed` | Für erste Tests per-node/Fallback akzeptieren; echte Positionsfusion später separat verbessern |

## Timestamp-/Guard-Intervall-Befund

RuView meldete im 4RX-Betrieb wiederholt:

```text
Multistatic fusion failed
Timestamp spread ... us exceeds guard interval 60000 us
using per-node sum/dedup fallback
```

Das Standard-Guard-Intervall des Servers liegt bei 60 ms hard und 20 ms soft. Die beobachtete ESP32-/WLAN-Zeitspreizung liegt teilweise deutlich darüber. Dadurch werden Frames verschiedener Nodes nicht immer als ein gemeinsamer synchroner Messzeitpunkt fusioniert.

Für die nächste Visualisierungsrunde wird ein pragmatischer Server-Workaround getestet:

```text
WDP_GUARD_INTERVAL_US=500000
WDP_SOFT_GUARD_US=200000
```

Bewertung: Dieser Workaround ist sinnvoll, um die RuView-Visualisierung stabiler zu bekommen und weniger Fallback-Meldungen zu sehen. Für methodisch saubere Positions- oder Atemauswertung ist er nur begrenzt geeignet, weil stärker zeitversetzte Frames gemeinsam betrachtet werden. Langfristig bleibt bessere Synchronisation/TDM die sauberere Lösung.

## Fazit für den Bericht

Die Messreihe ist für die Frage nach Bewegungserfassung und Systemgrenzen brauchbar. Sie belegt technisch den stabilen 4RX-Betrieb und zeigt erste Feature-Unterschiede zwischen leerem Raum, stehender Person, Bewegung und ruhiger Atmung. Für quantitative Aussagen zur Zuverlässigkeit müssen weitere Wiederholungen mit sauberer Negativklasse und Referenzwerten folgen.

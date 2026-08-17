# Qualitätscheck G1 Guard-Intervall-Test — 2026-06-28

## Datensatz

Messordner: `data/raw/2026-06-28_01-24-12_G1_guard500ms_person_laeuft`

Setup laut Metadata:

- Label: `G1_guard500ms_person_laeuft`
- Dauer: 60 s
- Intervall: 1 s
- Erwartete Nodes: `1, 2, 3, 4`
- Notiz: Guard 500 ms hard / 200 ms soft, Person läuft langsam

## Vollständigkeit

| Prüfung | Ergebnis |
|---|---:|
| CSV-Samples | 60 |
| JSONL-Samples | 60 |
| Logger-Fehler | 0 |
| eindeutige Ticks | 60 |
| Tick-Fortschritt | 59/59 Übergänge |
| Samples mit Nodes 1,2,3,4 | 60/60 |
| Samples mit vollständigen 4x64 Subcarriern | 56/60 = 93,3 % |

Bewertung: Die Messung ist vollständig und für eine erste Auswertung brauchbar.

## Node-Qualität

| Node | Samples | Samples mit `subcarrier_count=0` | mittlerer RSSI |
|---:|---:|---:|---:|
| 1 | 60 | 4 | -64,42 dBm |
| 2 | 60 | 4 | -65,98 dBm |
| 3 | 60 | 4 | -59,45 dBm |
| 4 | 60 | 4 | -66,63 dBm |

Alle Nodes waren sichtbar. Einzelne Snapshots enthalten wieder leere Subcarrier-Daten; diese sollten bei späterer Detailanalyse gefiltert werden.

## Signalübersicht

| Kennwert | Mittelwert | Median | Minimum | Maximum |
|---|---:|---:|---:|---:|
| mean RSSI | -59,03 dBm | -59,00 dBm | -64,00 dBm | -58,00 dBm |
| Varianz | 40,79 | 40,74 | 34,63 | 47,33 |
| Motion-Power | 62,52 | 62,37 | 54,16 | 72,17 |
| Breathing-Power | 61,55 | 61,13 | 53,70 | 70,92 |
| Signalqualität | 0,37 | 0,34 | 0,21 | 0,50 |

Klassifikation:

- `presence=True`: 59/60
- `presence=False`: 1/60
- `present_still`: 41/60
- `present_moving`: 18/60
- `absent`: 1/60

## Vergleich zu A2

G1 ist inhaltlich am ehesten mit A2 vergleichbar, weil beide „Person läuft“ messen.

| Kennwert | A2 | G1 |
|---|---:|---:|
| Samples mit 4x64 Subcarriern | 58/60 = 96,7 % | 56/60 = 93,3 % |
| Varianz Mittelwert | 39,37 | 40,79 |
| Motion-Power Mittelwert | 59,82 | 62,52 |
| Breathing-Power Mittelwert | 62,23 | 61,55 |
| Signalqualität Mittelwert | 0,40 | 0,37 |

Die API-Daten selbst zeigen eine stabile 4RX-Erfassung. Ob das größere Guard-Intervall die multistatische Fusion tatsächlich verbessert hat, muss zusätzlich am Serverlog geprüft werden, weil der Logger nur die RuView-API-Snapshots speichert und keine `Multistatic fusion failed`-Zählung aus dem Serverlog enthält.

## Serverlog-Auswertung

Nachträglich wurde ein Serverlog-Ausschnitt zum G1-Test geprüft.

| Prüfung | Ergebnis |
|---|---:|
| Logzeilen | 50 |
| ESP32-Frame-Zeilen | 47 |
| `Multistatic fusion failed` | 0 |
| Nodes im Log | 1, 2, 3, 4 |
| Subcarrier in Frame-Zeilen | 64 |

Node-Verteilung im Ausschnitt:

| Node | IP | Frames |
|---:|---|---:|
| 1 | `RX1_IP` | 12 |
| 2 | `RX3_IP` | 12 |
| 3 | `RX_OTHER_IP` | 13 |
| 4 | `RX2_IP` | 10 |

Bewertung: Im gelieferten Logausschnitt traten keine Fusion-Fallback-Meldungen auf. Das spricht dafür, dass das größere Guard-Intervall von 500 ms den Visualisierungs-/Fusion-Workaround verbessert hat. Einschränkung: Der geprüfte Ausschnitt umfasst nur einen kurzen Zeitraum; für eine belastbare Rate müsste der vollständige Serverlog über die gesamte G1-Messdauer gespeichert werden.

## Methodische Einordnung

Der Guard-Intervall-Test ist ein Visualisierungs-Workaround. Wenn weniger Fallback-Meldungen auftreten, ist das hilfreich für die Darstellung. Es bedeutet aber nicht automatisch, dass die Messung physikalisch synchroner wurde; der Server akzeptiert lediglich größere Zeitabstände zwischen Node-Frames.

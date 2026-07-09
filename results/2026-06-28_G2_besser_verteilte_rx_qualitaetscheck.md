# Qualitätscheck G2 — RX besser im Raum verteilt

## Datensätze

| Messung | Ordner | Dauer | Beschreibung |
|---|---|---:|---|
| G2 Bewegung | `2026-06-28_01-32-32_G2_guard500ms_rx_besser_verteilt_person_laeuft` | 60 s | RX-Module besser verteilt, Person läuft langsam |
| G2 leerer Raum | `2026-06-28_01-34-01_G2_empty_rx_besser_verteilt` | 60 s | RX-Module besser verteilt, leerer Raum |

Server: Guard-Intervall 500 ms hard / 200 ms soft.

## Vollständigkeit und Node-Qualität

| Messung | Samples | Logger-Fehler | Node-IDs vollständig | vollständige 4x64-Subcarrier-Samples |
|---|---:|---:|---:|---:|
| G2 Bewegung | 60 | 0 | 60/60 | 58/60 = 96,7 % |
| G2 leerer Raum | 60 | 0 | 60/60 | 59/60 = 98,3 % |

Bewertung: Die technische Erfassung ist sehr gut. Die bessere Verteilung hat die Datenvollständigkeit nicht verschlechtert; im leeren Raum war sie sogar sehr stabil.

## Signal- und Klassifikationsübersicht

| Messung | Presence-True | Motion-Klassen | Estimated Persons | Varianz | Motion-Power | Breathing-Power | Signalqualität |
|---|---:|---|---|---:|---:|---:|---:|
| G2 Bewegung | 60/60 | 31 `present_still`, 26 `present_moving`, 3 `active` | 60x `1` | 39,88 | 60,37 | 67,23 | 0,40 |
| G2 leerer Raum | 60/60 | 39 `present_still`, 21 `present_moving` | 60x `1` | 44,83 | 66,38 | 73,93 | 0,43 |

## Serverlog

Datei: `logs/G2_besser_verteilte_rx_server.log`

| Prüfung | Ergebnis |
|---|---:|
| Logzeilen | 17.032 |
| ESP32-Frame-Zeilen | 16.097 |
| `Multistatic fusion failed` | 31 |
| Guard-Intervall in Fehlermeldungen | 500.000 µs |
| Timestamp-Spread bei Fallbacks | 500.783–815.455 µs |

Frame-Verteilung:

| Node | Frames |
|---:|---:|
| 1 | 3.950 |
| 2 | 4.095 |
| 3 | 4.015 |
| 4 | 4.037 |

Bewertung: Der 500-ms-Guard reduziert die Fallback-Problematik deutlich, beseitigt sie aber nicht vollständig. Einige Frame-Gruppen liegen weiterhin über 500 ms auseinander.

## Webansicht / Visualisierung

Zugehöriger Screenshot: [`skizzen/screenshots/2026-06-28_01-39-16_g2-jumping-heatmap-multiple-poses.png`](../skizzen/screenshots/2026-06-28_01-39-16_g2-jumping-heatmap-multiple-poses.png)

Beobachtung aus Screenshot und Live-Ansicht: Die Heatmap und Pose-/Personhypothesen springen stark hin und her. Es erscheinen mehrere Pose-ähnliche Markierungen und helle Feldmaxima, obwohl die echte Personensituation einfacher ist.

Einordnung:

- Die Webansicht ist aktuell keine zuverlässige Positionsmessung.
- Der leere Raum G2 wird trotzdem in 60/60 Samples als `presence=True` und `estimated_persons=1` klassifiziert. Das erklärt, warum die Visualisierung auch ohne belastbare Personenerkennung etwas anzeigt.
- Die Node-Positionen wurden bisher nicht passend zur realen RX-Verteilung an den Server übergeben. RuView unterstützt `--node-positions` im Format `x,y,z;x,y,z;...`; ohne diese Geometrie arbeitet die Fusion/Visualisierung mit unpassenden bzw. Standard-Positionen.
- Das größere Guard-Intervall hilft bei der Fusion, macht die Frames aber nicht physikalisch synchroner.

## Fazit

Die bessere RX-Verteilung verbessert die reine Datenerfassung und liefert sehr vollständige 4RX-Daten. Sie löst aber nicht automatisch die falsche/unstabile visuelle Positionsanzeige. Für eine brauchbarere Visualisierung sind als nächste Schritte nötig:

1. reale RX-Positionen messen und mit `--node-positions` an RuView übergeben,
2. leeren Raum als Baseline/Kalibrierung nutzen,
3. Web-Pose/Personenzahl vorerst nicht als Messwert verwenden,
4. für den Bericht primär CSV-/Feature-Auswertung und nicht die springende Pose-Ansicht verwenden.

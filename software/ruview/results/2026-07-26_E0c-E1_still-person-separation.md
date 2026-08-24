# E0c gegen E1: stille Person bei mittigem Mac — 2026-07-26

## Fragestellung

Nach dem Mac-Positions-A/B-Test sollte geklärt werden, ob sich eine still sitzende Person bei unverändertem mittigem Mac-Standort überhaupt vom leeren Raum unterscheidet. Dieser Vergleich ist die Entscheidungsgrundlage für eine spätere Änderung von Schwellen und RX-Fusion.

## Versuchsdesign

| Lauf | Zustand | Dauer | Samples |
|---|---|---:|---:|
| E0c | leerer Raum | 60 s | 237 |
| E1 | eine Person sitzt möglichst still mittig im Raum | 60 s | 237 |

Unverändert:

- Mac relativ mittig
- fester 1TX-/4RX-Aufbau
- TX-MAC-Filter auf RX1 bis RX4
- D4-Bewegungsmetrik
- 250 ms Abfrageintervall
- keine Bewertung von Heatmap oder Positionsanzeige

Rohdaten E1:

`data/raw/2026-07-26_21-53-24_E1_person_sitzt_still_mittig_Mac_mittig_D4_TX_MAC_Filter/`

Beide Läufe enthalten alle vier RX in jedem Sample und keine Logger-Fehler.

## Aktuelle Klassifikation

### Global

| Lauf | `ABSENT` | `PRESENT_STILL` | Präsenzanteil |
|---|---:|---:|---:|
| E0c leer | 126 | 111 | 46,8 % |
| E1 still | 48 | 189 | 79,7 % |

Die aktuelle globale Ausgabe reagiert auf die Person, ist wegen der hohen Leerraum-Fehlpräsenz aber nicht zuverlässig genug.

### Pro RX

| RX | Fehlpräsenz E0c | Präsenz E1 | Änderung |
|---|---:|---:|---:|
| RX1 | 0,0 % | 0,0 % | 0,0 Prozentpunkte |
| RX2 | 13,1 % | 8,0 % | −5,1 Prozentpunkte |
| RX3 | 40,5 % | 72,2 % | +31,7 Prozentpunkte |
| RX4 | 0,0 % | 38,8 % | +38,8 Prozentpunkte |

RX1 erkennt diese sitzende Position nicht. RX2 liefert keine nutzbare positive Trennung. RX3 reagiert, besitzt aber eine hohe Leerraum-Basis. RX4 zeigt die sauberste Trennung.

## Score-Vergleich

| RX | Raw Mean leer | Raw Mean still | Smoothed Mean leer | Smoothed Mean still | deskriptive AUC Smoothed |
|---|---:|---:|---:|---:|---:|
| RX1 | 0,027 | 0,023 | 0,002 | 0,000 | 0,095 |
| RX2 | 0,058 | 0,061 | 0,018 | 0,017 | 0,517 |
| RX3 | 0,092 | 0,111 | 0,037 | 0,051 | 0,679 |
| RX4 | 0,027 | 0,077 | 0,001 | 0,036 | 0,982 |

Die AUC ist hier rein deskriptiv als Wahrscheinlichkeit zu lesen, dass ein zufälliger Still-Sample einen höheren Score als ein zufälliger Leerraum-Sample besitzt. Wegen der zeitlichen Abhängigkeit der Samples und nur eines Laufpaars ist sie kein unabhängiger statistischer Nachweis.

## Zeitliche Stabilität von RX4

Der RX4-Effekt bleibt nach der anfänglichen Sitzbewegung bestehen:

| Abschnitt | Smoothed Mean E0c leer | Smoothed Mean E1 still |
|---|---:|---:|
| 0–10 s | 0,002 | 0,025 |
| 10–20 s | 0,000 | 0,039 |
| 20–40 s | 0,002 | 0,039 |
| 40–60 s | 0,000 | 0,039 |

Damit erfasst RX4 nicht nur das Hinsetzen, sondern auch eine anhaltende Veränderung des CSI-Links bei still sitzender Person.

## Vorläufige RX4-Schwellenanalyse

| Smoothed-Schwelle | E0c leer darüber | E1 still darüber |
|---:|---:|---:|
| 0,003 | 4,6 % | 94,5 % |
| 0,005 | 4,2 % | 91,1 % |
| 0,010 | 3,0 % | 83,5 % |
| 0,015 | 2,5 % | 77,2 % |
| 0,020 | 1,3 % | 69,2 % |
| 0,030 | 1,3 % | 57,8 % |
| 0,040 | 0,8 % | 39,7 % |

Die derzeitige Klassenlogik verwendet für den Übergang zu `PRESENT_STILL` effektiv die 0,04-Klasse mit Debounce. Sie verschenkt bei RX4 einen großen Teil der vorhandenen Trennung. Eine Schwelle um 0,01 wäre für dieses Laufpaar deutlich empfindlicher.

## Interpretation

Eine still sitzende Person ist mit dem aktuellen Aufbau grundsätzlich messbar, aber nicht auf allen Links gleich:

- RX4 ist für die getestete Sitzposition hoch informativ.
- RX3 liefert ein schwächeres zusätzliches Signal.
- RX1 und RX2 helfen für diese Position nicht.
- Die globale ODER-Verknüpfung gleich behandelter RX ist ungeeignet, weil sie die hohe Leerraum-Basis von RX3 direkt übernimmt.

Die geeignete Richtung ist deshalb keine einzige globale Schwelle für alle RX. Benötigt werden per-RX-Leerraumreferenzen und eine Fusion der Abweichung von dieser individuellen Referenz. Eine Mindestdauer kann kurze Ausreißer zusätzlich unterdrücken.

## Nächster Entscheidungspunkt

Die Schwelle 0,01 ist ein Kandidat, keine endgültig validierte Einstellung. Sie wurde aus genau diesem Laufpaar abgeleitet. Vor einer belastbaren Übernahme muss sie mit mindestens einem neuen Leerraum-/Still-Paar am unveränderten Aufbau bestätigt werden.

## Nachträgliche unabhängige Prüfung

E0d/E1b bestätigte die RX4-Reaktion nicht: RX4 blieb im zweiten Still-Lauf vollständig unter `0,01`. Der feste RX4-Schwellenkandidat wird daher verworfen. Reproduzierbar war stattdessen der Anstieg des RX3-Minutenmittels gegenüber dem jeweils direkt vorherigen Leerraum.

Auswertung: [2026-07-26_E0d-E1b_unabhaengige-bestaetigung.md](2026-07-26_E0d-E1b_unabhaengige-bestaetigung.md)

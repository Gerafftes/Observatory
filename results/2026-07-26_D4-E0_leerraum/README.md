# D4/E0-Mischlauf — 2026-07-26

> **Nachträglicher Gültigkeitshinweis:** Der Raum wurde während der Aufnahme zweimal kurz betreten. Der Lauf ist deshalb keine reine Leerraum-Baseline und darf nicht zur Festlegung von Schwellen oder zur Berechnung einer Leerraum-Fehlerrate verwendet werden. Die Messung bleibt als dokumentierter Mischlauf erhalten.

## Ziel

Geplant war ein Leerraumtest nach der D4-Änderung und dem TX-MAC-Filter auf allen vier RX. Da der Raum während der Aufnahme zweimal kurz betreten wurde, kann der Lauf nachträglich nur als Mischlauf aus Leerraum und kurzzeitiger Anwesenheit ausgewertet werden. Die Heatmap und eine räumliche Positionsschätzung waren nicht Gegenstand dieses Tests.

## Aufbau

- 1 kontrollierter TX, Absender-MAC `AE:27:6E:A8:D2:64`
- 4 RX mit aktivem Filter auf diese TX-MAC
- fester Raumaufbau mit korrigierter TX-/RX-Geometrie
- RuView-Sensing-Server mit D4-Bewegungsmetrik
- geplanter Leerraumlauf; während der 60 Sekunden zweimal kurz betreten
- Abfrageintervall 250 ms

Rohdaten:

`data/raw/2026-07-26_21-05-50_E0_leerer_raum_D4_alle_RX_TX_MAC_Filter/`

## Datenqualität

- 237 gespeicherte Samples in 59,883 Sekunden
- 237 unterschiedliche Server-Ticks
- RX1, RX2, RX3 und RX4 in jedem Sample vorhanden
- keine Logger-Fehler
- kein RX wurde als `stale` markiert

## Ergebnis

### Globale Klassifikation

| Klasse | Samples | Anteil |
|---|---:|---:|
| `ABSENT` | 129 | 54,4 % |
| `PRESENT_STILL` | 108 | 45,6 % |
| `PRESENT_MOVING` | 0 | 0,0 % |
| `ACTIVE` | 0 | 0,0 % |

### Klassifikation pro RX

| RX | `ABSENT` | `PRESENT_STILL` | `PRESENT_MOVING` | `ACTIVE` |
|---|---:|---:|---:|---:|
| RX1 | 237 (100,0 %) | 0 | 0 | 0 |
| RX2 | 188 (79,3 %) | 49 (20,7 %) | 0 | 0 |
| RX3 | 175 (73,8 %) | 55 (23,2 %) | 7 (3,0 %) | 0 |
| RX4 | 194 (81,9 %) | 43 (18,1 %) | 0 | 0 |

### Bewegungswerte pro RX

| RX | Raw Mean | Raw P95 | Smoothed Mean | Smoothed P95 | Baseline Mean |
|---|---:|---:|---:|---:|---:|
| RX1 | 0,027 | 0,038 | 0,001 | 0,003 | 0,019 |
| RX2 | 0,066 | 0,192 | 0,022 | 0,071 | 0,039 |
| RX3 | 0,088 | 0,228 | 0,033 | 0,148 | 0,049 |
| RX4 | 0,071 | 0,206 | 0,025 | 0,065 | 0,041 |

## Vergleich mit dem vorherigen Leerraumlauf

Für den Vergleich wurden aus `2026-07-26_19-31-37_A0_leerer_raum_fortlaufend` ebenfalls die ersten 60 Sekunden mit 237 Samples verwendet.

| Globale Klasse | vor D4 | D4-Mischlauf |
|---|---:|---:|
| `ACTIVE` | 124 | 0 |
| `PRESENT_MOVING` | 111 | 0 |
| `PRESENT_STILL` | 2 | 108 |
| `ABSENT` | 0 | 129 |

D4 reduziert die groben Bewegungs-Fehlalarme deutlich. Vor D4 wurde der leere Raum in 235 von 237 Samples als bewegt oder aktiv ausgegeben; nach D4 trat global weder `PRESENT_MOVING` noch `ACTIVE` auf.

## Vorläufige Beobachtung

Im gesamten Mischlauf wird in 45,6 % der Samples `PRESENT_STILL` ausgegeben. Wegen der zwei kurzen Raumzutritte ist dieser Anteil keine gültige Leerraum-Fehlerrate. RX1 bleibt vollständig ruhig, während RX2, RX3 und RX4 die Still-Schwelle zeitweise überschreiten. RX3 erzeugt zusätzlich sieben lokale `PRESENT_MOVING`-Samples, erreicht damit aber nicht das globale Bewegungsquorum.

Die globale Aggregation verlangt für Bewegung ein Quorum von mindestens zwei der vier RX. Für `PRESENT_STILL` genügt dagegen bereits ein einziger RX. Dadurch wird jede einzelne Still-Fehlklassifikation von RX2, RX3 oder RX4 zur globalen Anwesenheitsmeldung.

## Einordnung

Der Lauf deutet darauf hin, dass D4 die zuvor dominierende grobe Bewegungsaktivität reduziert. Wegen der zwei nicht zeitlich markierten Raumzutritte ist aber weder ein Bestehen noch ein Scheitern von E0 ableitbar. Insbesondere dürfen die 45,6 % `PRESENT_STILL` nicht als Leerraum-Fehlerrate bezeichnet werden.

Zuerst muss E0 als vollständig ununterbrochener 60-Sekunden-Leerraumlauf wiederholt werden. Danach folgt ein kontrollierter Lauf mit still sitzender Person. Erst der Vergleich dieser beiden gültigen Läufe zeigt, ob eine Quorum-Regel, eine höhere Präsenzschwelle oder eine andere zeitliche Logik Still-Anwesenheit trennt, ohne echte stille Personen zu verlieren.

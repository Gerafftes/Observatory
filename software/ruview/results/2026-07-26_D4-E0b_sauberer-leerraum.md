# D4/E0b: sauberer Leerraumtest — 2026-07-26

## Gültigkeit

Dieser Lauf ist die gültige Wiederholung des zuvor kontaminierten E0-Versuchs. Der Raum blieb während der vollständigen Aufnahme leer. Das Messende wurde außerhalb des Raums durch zweimaliges Blinken der Home-Assistant-Leuchte `CAB SM Küche Fackel` signalisiert.

## Ziel

Geprüft werden ausschließlich die per-RX- und globale Klassifikation mit D4 und aktivem TX-MAC-Filter im leeren Raum. Heatmap und Positionsanzeige werden nicht bewertet.

## Aufbau und Daten

- 1 kontrollierter TX, Absender-MAC `TX_MAC_REDACTED`
- RX1 bis RX4 mit aktivem TX-MAC-Filter
- fester Raumaufbau und korrigierte Gerätegeometrie
- 10 Sekunden Vorlauf, danach 60 Sekunden leerer Raum
- 250 ms Abfrageintervall
- 237 gespeicherte Samples in 59,850 Sekunden
- 237 unterschiedliche Server-Ticks
- alle vier RX in jedem Sample vorhanden
- keine Logger-Fehler und keine als `stale` markierten RX

Rohdaten:

`data/raw/2026-07-26_21-24-17_E0b_sauberer_leerraum_D4_alle_RX_TX_MAC_Filter/`

## Ergebnis

### Globale Klassifikation

| Klasse | Samples | Anteil |
|---|---:|---:|
| `PRESENT_STILL` | 218 | 92,0 % |
| `ABSENT` | 19 | 8,0 % |
| `PRESENT_MOVING` | 0 | 0,0 % |
| `ACTIVE` | 0 | 0,0 % |

### Klassifikation pro RX

| RX | `ABSENT` | `PRESENT_STILL` | `PRESENT_MOVING` | `ACTIVE` |
|---|---:|---:|---:|---:|
| RX1 | 237 (100,0 %) | 0 | 0 | 0 |
| RX2 | 208 (87,8 %) | 29 (12,2 %) | 0 | 0 |
| RX3 | 143 (60,3 %) | 94 (39,7 %) | 0 | 0 |
| RX4 | 36 (15,2 %) | 200 (84,4 %) | 1 (0,4 %) | 0 |

### Bewegungswerte pro RX

| RX | Raw Mean | Raw P95 | Smoothed Mean | Smoothed P95 | Baseline Mean |
|---|---:|---:|---:|---:|---:|
| RX1 | 0,022 | 0,029 | 0,000 | 0,000 | 0,018 |
| RX2 | 0,060 | 0,190 | 0,020 | 0,052 | 0,038 |
| RX3 | 0,088 | 0,212 | 0,035 | 0,077 | 0,048 |
| RX4 | 0,121 | 0,218 | 0,062 | 0,094 | 0,054 |

## Technische Ursache der globalen Fehlklassifikation

RX1 bleibt vollständig ruhig. RX2 überschreitet die Still-Schwelle nur zeitweise, RX3 häufiger und RX4 fast durchgehend. Die globale Aggregation gibt bereits dann `PRESENT_STILL` aus, wenn mindestens ein einziger aktiver RX diese Klasse meldet.

Die Verteilung der gleichzeitig präsenzmeldenden RX bestätigt den Effekt:

| Präsenzmeldende RX | Samples | Anteil |
|---|---:|---:|
| nur RX4 | 108 | 45,6 % |
| RX3 und RX4 | 67 | 28,3 % |
| kein RX | 19 | 8,0 % |
| übrige Kombinationen | 43 | 18,1 % |

RX4 allein verursacht damit bereits fast die Hälfte aller globalen Samples. Die Kombination aus niedriger lokaler Still-Schwelle und „mindestens ein RX genügt“ macht die globale Leerraumerkennung unbrauchbar.

## Bewertung

E0b ist nicht bestanden. D4 verhindert zwar weiterhin globale Klassen `PRESENT_MOVING` und `ACTIVE`, löst die Anwesenheitserkennung aber nicht. Eine reine Kalibrierungswiederholung würde die strukturelle Aggregationsursache nicht beheben.

Vor einer Regeländerung ist ein gleich langer Positivlauf mit still sitzender Person erforderlich. Nur der Vergleich der Verteilungen zeigt, ob:

1. eine höhere lokale Still-Schwelle,
2. ein Quorum aus mehreren RX,
3. eine Mindestdauer oder
4. eine Kombination daraus

den Leerraum sicher verwirft, ohne eine echte still sitzende Person ebenfalls als `ABSENT` zu verlieren.

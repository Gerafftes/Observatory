# A/B-Test Mac-Position: E0b gegen E0c — 2026-07-26

## Fragestellung

Im gültigen Leerraumlauf E0b stieg die lokale Fehlpräsenz von RX1 bis RX4 stark an. Da der Mac beim vorherigen Aufbau näher an RX4 stand, wurde geprüft, ob seine Position die CSI-Ruhe einzelner Empfänger beeinflusst.

## Versuchsdesign

Zwischen den beiden Läufen wurde nur der Standort des Macs verändert:

- E0b: vorheriger Mac-Standort
- E0c: Mac relativ mittig im Raum

Unverändert blieben:

- leerer Raum während der vollständigen 60-Sekunden-Aufnahme
- 1 kontrollierter TX
- RX1 bis RX4 mit TX-MAC-Filter
- D4-Bewegungsmetrik
- RX-, TX- und Raumgeometrie
- 250 ms Abfrageintervall
- keine Bewertung von Heatmap oder Positionsanzeige

Rohdaten E0c:

`data/raw/2026-07-26_21-49-13_E0c_leerraum_Mac_mittig_D4_TX_MAC_Filter/`

## Datenqualität

Beide Läufe enthalten jeweils 237 Samples und 237 unterschiedliche Server-Ticks. Alle vier RX waren in jedem Sample vorhanden. Es gab keine Logger-Fehler.

## Ergebnis

### Globale Leerraumklassifikation

| Lauf | `ABSENT` | `PRESENT_STILL` | globale Fehlpräsenz |
|---|---:|---:|---:|
| E0b, Mac vorher | 19 | 218 | 92,0 % |
| E0c, Mac mittig | 126 | 111 | 46,8 % |

### Fehlpräsenz pro RX

| RX | E0b | E0c | Änderung |
|---|---:|---:|---:|
| RX1 | 0,0 % | 0,0 % | 0,0 Prozentpunkte |
| RX2 | 12,2 % | 13,1 % | +0,9 Prozentpunkte |
| RX3 | 39,7 % | 40,5 % | +0,8 Prozentpunkte |
| RX4 | 84,8 % | 0,0 % | −84,8 Prozentpunkte |

Bei RX4 umfasst E0b 200-mal `PRESENT_STILL` und einmal `PRESENT_MOVING`; E0c enthält für RX4 ausschließlich `ABSENT`.

### Bewegungswerte

| RX | Raw Mean E0b | Raw Mean E0c | Smoothed Mean E0b | Smoothed Mean E0c | RSSI E0b | RSSI E0c |
|---|---:|---:|---:|---:|---:|---:|
| RX1 | 0,022 | 0,027 | 0,000 | 0,002 | −45,0 dBm | −45,9 dBm |
| RX2 | 0,060 | 0,058 | 0,020 | 0,018 | −58,0 dBm | −56,5 dBm |
| RX3 | 0,088 | 0,092 | 0,035 | 0,037 | −64,0 dBm | −64,2 dBm |
| RX4 | 0,121 | 0,027 | 0,062 | 0,001 | −57,0 dBm | −52,0 dBm |

RX4 zeigt nach dem Umstellen gleichzeitig einen deutlich niedrigeren Bewegungswert und einen um etwa 5 dB stärkeren TX-Empfang.

## Interpretation

Die Hypothese ist für RX4 stark gestützt: Die vorherige Nähe beziehungsweise der vorherige räumliche Bezug des Macs zu RX4 war ein wesentlicher Störfaktor. Das Umstellen beseitigte dessen lokale Fehlpräsenz vollständig und halbierte dadurch ungefähr die globale Fehlpräsenz.

Der Test unterscheidet noch nicht zwischen zwei physikalischen Mechanismen:

1. Funkaktivität oder Nahfeldeinfluss des Macs auf RX4,
2. Änderung des Multipfadfeldes durch Standort, Metallgehäuse und Kabel des Macs.

Da nur ein räumlicher A/B-Wechsel gemessen wurde, ist noch keine allgemeine Distanzfunktion bewiesen. Eine Wiederholung beider Mac-Positionen könnte den kausalen Zusammenhang weiter absichern.

RX2 und RX3 änderten sich dagegen praktisch nicht. Ihre Fehlpräsenz wird daher nicht durch den früheren Mac-RX4-Abstand erklärt und muss separat untersucht werden.

## Konsequenz

- Der Mac bleibt für die folgenden Tests am mittigen Standort.
- E0c ersetzt E0b als aktuelle Leerraum-Referenz für diesen Standort.
- Vor Änderungen an Schwellen oder Quorum folgt ein Positivlauf mit still sitzender Person.
- RX2 und besonders RX3 bleiben als unabhängige Störquellen in der Auswertung sichtbar.

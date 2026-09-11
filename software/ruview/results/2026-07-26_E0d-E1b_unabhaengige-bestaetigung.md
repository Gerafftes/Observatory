# E0d/E1b: unabhängige Bestätigung — 2026-07-26

## Ziel

Der aus E0c/E1 abgeleitete RX4-Schwellenkandidat `smoothed_motion_score > 0,01` sollte an einem neuen Leerraum-/Still-Paar unter unverändertem Aufbau geprüft werden.

## Versuchsdesign

| Lauf | Zustand | Dauer | Samples |
|---|---|---:|---:|
| E0d | leerer Raum | 60 s | 237 |
| E1b | dieselbe Person sitzt möglichst still an derselben mittigen Position | 60 s | 237 |

Mac, TX, RX, Firmware, Server und D4-Logik blieben unverändert. Beide Läufe enthalten alle vier RX in jedem Sample und keine Logger-Fehler.

Rohdaten:

- `data/raw/2026-07-26_22-00-55_E0d_bestaetigung_leerraum_Mac_mittig_D4_TX_MAC_Filter/`
- `data/raw/2026-07-26_22-04-22_E1b_bestaetigung_person_sitzt_still_Mac_mittig_D4_TX_MAC_Filter/`

## Ergebnis der aktuellen Klassifikation

### Global

| Lauf | `ABSENT` | `PRESENT_STILL` | Präsenzanteil |
|---|---:|---:|---:|
| E0d leer | 31 | 206 | 86,9 % |
| E1b still | 7 | 230 | 97,0 % |

Die aktuelle globale Klassifikation trennt die Zustände nicht brauchbar.

### Pro RX

| RX | Präsenz E0d leer | Präsenz E1b still | Änderung |
|---|---:|---:|---:|
| RX1 | 0,0 % | 0,0 % | 0,0 Prozentpunkte |
| RX2 | 83,5 % | 72,6 % | −10,9 Prozentpunkte |
| RX3 | 22,4 % | 78,1 % | +55,7 Prozentpunkte |
| RX4 | 0,0 % | 0,0 % | 0,0 Prozentpunkte |

RX2 erzeugte im Leerraum starke Fehlpräsenz. RX3 reagierte deutlich auf die Person. RX1 und RX4 blieben vollständig `ABSENT`.

## Score-Vergleich

| RX | Smoothed Mean E0d leer | Smoothed Mean E1b still | Änderung | deskriptive AUC |
|---|---:|---:|---:|---:|
| RX1 | 0,000 | 0,000 | ungefähr 0,000 | 0,505 |
| RX2 | 0,059 | 0,052 | −0,007 | 0,398 |
| RX3 | 0,027 | 0,055 | +0,028 | 0,815 |
| RX4 | 0,000 | 0,000 | ungefähr 0,000 | 0,726 |

Die RX4-AUC entsteht nur aus sehr kleinen Werten nahe null und ist praktisch nicht nutzbar: Kein einziger RX4-Sample überschritt `0,003`, `0,005` oder `0,01`.

## Entscheidung zum RX4-Schwellenkandidaten

Der Kandidat `RX4 > 0,01` ist nicht bestätigt:

| Lauf | Anteil RX4 über 0,01 |
|---|---:|
| E0c leer | 3,0 % |
| E1 still | 83,5 % |
| E0d leer | 0,0 % |
| E1b still | 0,0 % |

RX4 blieb zwar in beiden Leerraumläufen nach dem Umstellen des Macs ruhig. Die Reaktion auf die still sitzende Person war jedoch nicht reproduzierbar. Wahrscheinlich veränderten bereits kleine Unterschiede in Sitzposition, Körperhaltung oder Orientierung den empfindlichen TX–Person–RX4-Pfad.

## Reproduzierbarer Befund von RX3

RX3 stieg in beiden Paaren vom jeweils unmittelbar vorherigen Leerraum zur stillen Person:

| Paar | Leerraum Smoothed Mean | Still Smoothed Mean | Änderung |
|---|---:|---:|---:|
| E0c → E1 | 0,037 | 0,051 | +0,014 |
| E0d → E1b | 0,027 | 0,055 | +0,028 |

Eine feste RX3-Sample-Schwelle bleibt dennoch unzureichend, weil die Leerraumverteilungen stark überlappen. Auf Minutenebene lagen beide Leerraum-Mittel unter `0,045` und beide Still-Mittel darüber. Dieser Befund spricht für eine längere zeitliche Auswertung, ist mit nur zwei Paaren aber noch kein validierter Produktionsschwellwert.

## Schlussfolgerung

Eine fest auf RX4 zugeschnittene Lösung wird verworfen. Die Daten sprechen stattdessen für:

1. per-RX-Leerraumreferenzen,
2. Bewertung einer anhaltenden Abweichung über mehrere Sekunden statt einzelner Samples,
3. Zuverlässigkeitsgewichtung oder Ausschluss aktuell instabiler Links wie RX2,
4. Fusion der informativen Links, ohne dass ein einzelner beliebig verrauschter RX globale Präsenz erzwingt.

Noch wurden keine Klassifikationsschwellen im Server verändert.

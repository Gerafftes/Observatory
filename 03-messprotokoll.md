# 03 Messprotokoll

## Zweck

Das Messprotokoll stellt sicher, dass die Daten später wirklich zur Problemfrage passen:

> Wie zuverlässig kann ein WLAN-CSI-System Bewegungen und Atemrhythmen im Raum erfassen?

## Vor jeder Messung dokumentieren

- Datum und Uhrzeit
- Raum und grobe Skizze
- Position von TX, RX1, RX2, RX3 und mmWave
- Abstand TX zu RXs
- WLAN-Kanal
- verwendete Node-IDs
- ob `--filter-mac` aktiv ist
- ob mmWave parallel läuft

## Messreihen erster Test

| ID | Dauer | Situation | Referenz |
|---|---:|---|---|
| A0 | 60 s | leerer Raum | keine Person |
| A1 | 60 s | Person steht ruhig in Mitte | manuelles Label |
| A2 | 60 s | Person läuft langsam durch Raum | manuelles Label + mmWave |
| A3 | 120 s | Person sitzt ruhig und atmet normal | mmWave/Fitnessuhr/manuelles Zählen |
| A4-L | 60 s | Person steht links | manuelles Label |
| A4-M | 60 s | Person steht Mitte | manuelles Label |
| A4-R | 60 s | Person steht rechts | manuelles Label |
| G1 | 60 s | Guard-Intervall-Test mit 4RX, Person bewegt sich langsam | Serverlog: weniger/keine `Multistatic fusion failed` |
| D5 E1 | 59,7 s | Person sitzt nach realer D5-Leerraumkalibrierung still | manuelles Label; 4RX vollständig |
| D5 E1 Persistenz | 29,9 s | dieselbe Person sitzt weiter still | manuelles Label; 4RX vollständig |

## Dateinamen

Empfohlen:

```text
data/raw/2026-06-26_A0_empty_room/
data/raw/2026-06-26_A1_person_middle/
data/raw/2026-06-26_A2_motion_walk/
data/raw/2026-06-26_A3_breathing_sitting/
data/raw/2026-06-26_A4_zone_left/
```

## Während der Messung notieren

- Startzeit
- Endzeit
- besondere Ereignisse
- Bewegung im Raum
- Störungen durch andere Personen/Geräte
- sichtbare Ausfälle im Server
- ob alle Node-IDs aktiv waren

## Erste Qualitätsprüfung

Direkt nach jeder Messung prüfen:

- Kommen Frames von RX1?
- Kommen Frames von RX2?
- Kommen Frames von RX3?
- Haben die Frames plausible RSSI-Werte?
- Ist Bewegung im Signal sichtbar?
- Hat mmWave plausible Werte geliefert?

## Guard-Intervall-Test G1

Ziel: Prüfen, ob ein größeres RuView-Guard-Intervall die Mehrknoten-Visualisierung stabilisiert.

Server-Start für G1:

```zsh
mkdir -p /Users/Johann/Development/BLL/wifi-csi-dokumentation/logs

WDP_GUARD_INTERVAL_US=500000 \
WDP_SOFT_GUARD_US=200000 \
RUST_LOG=debug ./target/release/sensing-server \
  --source esp32 \
  --udp-port 5005 \
  --http-port 8080 \
  --ws-port 8765 \
  --bind-addr 0.0.0.0 \
  2>&1 | tee /Users/Johann/Development/BLL/wifi-csi-dokumentation/logs/G1_guard500ms_server.log
```

Erfolgskriterium:

- `nodes [1, 2, 3, 4]` bleibt stabil
- Serverlog zeigt deutlich weniger `Multistatic fusion failed`
- RuView-Visualisierung bleibt sichtbar stabiler

Methodische Einschränkung:

Ein größeres Guard-Intervall ist ein Visualisierungs-Workaround. Es ersetzt keine echte Synchronisation der ESP32-Knoten.

## Abbruchkriterien

Messung wiederholen, wenn:

- ein RX komplett fehlt
- TX/RX neu gestartet hat
- Laptop/Server UDP-Pakete nicht empfängt
- Person während Atemmessung stark gesprochen/gelaufen ist
- mmWave-Referenz ausgefallen ist

## Minimaler Tagesabschluss

Am Ende des ersten Testtags sollten mindestens diese Daten existieren:

- leerer Raum
- Person steht
- Person bewegt sich
- ruhige Atmung
- eine kurze Notiz, welche RX-Nodes funktioniert haben

## D5-Livetest vom 2026-07-26

| Lauf | Rohdaten | Ergebnis |
|---|---|---|
| D5 E1 | `data/raw/2026-07-26_23-28-03_D5_E1_still_sitzend/` | 236 von 236 Samples global `ABSENT` |
| D5 E1 Persistenz | `data/raw/2026-07-26_23-30-21_D5_E1_still_persistenz/` | 114 von 114 Samples global `ABSENT` |

Qualitätsprüfung:

- RX1 bis RX4 in jedem Sample vorhanden
- keine Einträge in den beiden `errors.log`-Dateien
- D5-Referenz und aktuelle Evidenz für alle vier RX vorhanden
- reale Person manuell als still sitzend gelabelt

Bewertung:

Der Positivtest ist nicht bestanden. Für eine vollständige Aussage zur realen D5-Leistung fehlt unter derselben Kalibrierung noch der separat aufgezeichnete blinde Leerraumlauf.

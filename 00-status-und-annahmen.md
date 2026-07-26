# 00 Status und Annahmen

## Ziel der aktuellen Phase

Die aktuelle Phase soll aus dem stabilen 1TX-/4RX-Datenempfang eine belastbare Live-Auswertung entwickeln. Ziel ist:

- CSI-Daten von mehreren ESP32-RX-Nodes empfangen
- leeren Raum, still sitzende Person und Bewegung reproduzierbar unterscheiden
- eine ruhende Person stabil und eine Positionsänderung nachvollziehbar darstellen
- erst nach sauberem CSI-Datenstrom ruhige Atmung experimentell testen
- mmWave erst in einer späteren Phase als Referenz ergänzen

## Hardware aktuell

| Gerät | Rolle im ersten Test | Status |
|---|---|---|
| ESP32 #1 | kontrollierter WLAN-Sender / TX | vorhanden |
| ESP32 #2 | RuView CSI-Empfänger RX1 | online, liefert CSI |
| ESP32 #3 | RuView CSI-Empfänger RX2 | online, liefert CSI |
| ESP32 #4 | RuView CSI-Empfänger RX3 | online, liefert CSI |
| ESP32 #5 | RuView CSI-Empfänger RX4 | online, liefert CSI |
| mmWave-Modul | spätere Referenz für Presence/Atmung/Distanz | vorhanden, aktuell bewusst nicht verwendet |
| Laptop/PC | Server, Logging, Dashboard | gleichzeitig per Kabel/Hotspot und mit dem CSI-Netz verbunden; 4RX-Live-Empfang nachgewiesen |

## Annahmen

- Die ESP32-Boards sind ESP32-S3-Boards, idealerweise mit ausreichend Flash/PSRAM.
- Die ersten Tests laufen auf 2,4 GHz.
- Der Laptop/PC befindet sich im selben Netzwerk wie die ESP32-RX-Nodes.
- Für den TX wird eine kleine separate SoftAP-/Sender-Firmware verwendet.
- Für RX1-RX3 wird RuView `firmware/esp32-csi-node` verwendet.
- Das mmWave-Modul wird zuerst separat als Referenz betrachtet, nicht als Teil des WLAN-CSI-Systems.

## Fester Raumaufbau vom 2026-07-18

Raummaße:

- Länge: `4,02 m`
- Breite: `3,44 m`
- Höhe: `2,59 m`

Gemessene Positionen in der ursprünglichen Notation `(Breite, Länge, Höhe)`:

| Gerät | Breite | Länge | Höhe |
|---|---:|---:|---:|
| TX | 3,05 m | 2,51 m | 1,19 m |
| RX1 | 3,16 m | 4,02 m | 0,50 m |
| RX2 | 2,47 m | 0,00 m | 0,87 m |
| RX3 | 1,33 m | 4,02 m | 0,74 m |
| RX4 | 0,98 m | 0,00 m | 0,87 m |

Für RuView wurden die Koordinaten in `(x=Länge, y=Höhe, z=Breite)` überführt und entsprechend der gewählten Ansicht gespiegelt:

| Gerät | RuView-Koordinate `[x, y, z]` |
|---|---|
| RX1 | `[0.00, 0.50, 0.28]` |
| RX2 | `[4.02, 0.87, 0.97]` |
| RX3 | `[0.00, 0.74, 2.11]` |
| RX4 | `[4.02, 0.87, 2.46]` |
| TX | `[1.51, 1.19, 0.39]` |

Diese Werte sind untereinander konsistent: `x = 4,02 m - Länge` und `z = 3,44 m - Breite`.

## RuView-Rolle

RuView wird für diese Teile verwendet:

- ESP32-RX-Firmware zur CSI-Erfassung
- UDP-Streaming der CSI-Daten
- Sensing-Server auf Laptop/PC
- einfache Live-Visualisierung
- erste Presence-/Vitals-Heuristiken

RuView wird im ersten Test nicht als fertige wissenschaftliche Auswertung übernommen. Die Rohdaten und Referenzwerte müssen zusätzlich dokumentiert und später selbst ausgewertet werden.

## Noch offen

- Prüfen, ob die CSI-Callback-Pipeline jedes RX ausschließlich auf den vorgesehenen TX und einen einheitlichen Paket-/CSI-Typ begrenzt
- Wechselnde Subcarrier-Raster und Paketquellen vor der Merkmalsextraktion ausschließen
- Danach neue gelabelte Referenzmessungen für leeren Raum, stilles Sitzen und Bewegung aufnehmen
- Erst anschließend leere-Raum-Kalibrierung und Visualisierung erneut prüfen
- Genaues mmWave-Modul und dessen Schnittstelle: USB oder UART

## Aktueller Stand nach dem D5-Livetest vom 2026-07-26

- Der TX-MAC-Filter ist inzwischen auf RX1 bis RX4 aktiv.
- D4 reduziert die zuvor dominierenden groben Bewegungs-Fehlalarme, löst aber die Still-Präsenz-Klassifikation nicht.
- D5 ergänzt per-RX-Leerraumreferenzen, 10-Sekunden-Fenster und ein absolutes Zwei-RX-Quorum.
- Das D5-Offline-Replay war auf zwei historischen Laufpaaren positiv.
- Eine neue reale D5-Leerraumkalibrierung wurde erfolgreich aktiv; alle vier RX hatten anschließend Referenz und aktuelle Evidenz.
- Der anschließende reale Still-Livetest wurde nicht bestanden: 350 von 350 Samples blieben global `ABSENT`.
- Im ersten Abschnitt reagierte zeitweise nur RX4, im zweiten durchgehend nur RX3. Zwei gleichzeitig ausreichend lange zustimmende RX traten nicht auf.
- D5 ist deshalb weiterhin experimentell und nicht als Standard aktiviert.

Der nächste belastbare Entscheidungsschritt ist eine zusammengehörige blinde Testserie unter derselben Kalibrierung:

1. leerer Raum,
2. still sitzende Person an der bisherigen Position,
3. mindestens eine weitere Still-Position,
4. erst danach eine Änderung der Fusion oder Schwellen.

Ausführliche Auswertung: [results/2026-07-26_D5_realer-still-livetest.md](results/2026-07-26_D5_realer-still-livetest.md)

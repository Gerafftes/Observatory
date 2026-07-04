# 00 Status und Annahmen

## Ziel der aktuellen Phase

Der erste Versuch soll nicht direkt eine fertige Ortung liefern. Ziel ist:

- CSI-Daten von mehreren ESP32-RX-Nodes empfangen
- Bewegung im CSI-Signal sichtbar machen
- ruhige Atmung experimentell testen
- mmWave als Referenz parallel mitlaufen lassen

## Hardware aktuell

| Gerät | Rolle im ersten Test | Status |
|---|---|---|
| ESP32 #1 | kontrollierter WLAN-Sender / TX | vorhanden |
| ESP32 #2 | RuView CSI-Empfänger RX1 | vorhanden |
| ESP32 #3 | RuView CSI-Empfänger RX2 | vorhanden |
| ESP32 #4 | RuView CSI-Empfänger RX3 | provisioniert als `node_id=3`; seriell stabil bis `CSI streaming active`, Live-Empfang im Server noch offen |
| ESP32 #5 | RuView CSI-Empfänger RX4 | kommt später |
| mmWave-Modul | Referenz für Presence/Atmung/Distanz | vorhanden |
| Laptop/PC | Server, Logging, Dashboard | vorhanden; fuer 3RX-Test braucht der Mac eine stabile IP im `csi-test`-Netz |

## Annahmen

- Die ESP32-Boards sind ESP32-S3-Boards, idealerweise mit ausreichend Flash/PSRAM.
- Die ersten Tests laufen auf 2,4 GHz.
- Der Laptop/PC befindet sich im selben Netzwerk wie die ESP32-RX-Nodes.
- Für den TX wird eine kleine separate SoftAP-/Sender-Firmware verwendet.
- Für RX1-RX3 wird RuView `firmware/esp32-csi-node` verwendet.
- Das mmWave-Modul wird zuerst separat als Referenz betrachtet, nicht als Teil des WLAN-CSI-Systems.

## RuView-Rolle

RuView wird für diese Teile verwendet:

- ESP32-RX-Firmware zur CSI-Erfassung
- UDP-Streaming der CSI-Daten
- Sensing-Server auf Laptop/PC
- einfache Live-Visualisierung
- erste Presence-/Vitals-Heuristiken

RuView wird im ersten Test nicht als fertige wissenschaftliche Auswertung übernommen. Die Rohdaten und Referenzwerte müssen zusätzlich dokumentiert und später selbst ausgewertet werden.

## Noch offen

- Stabile Ziel-IP des Mac im `csi-test`-Netz festlegen, vorgeschlagen `192.168.4.50`
- RX1-RX3 danach einheitlich auf die feste Mac-IP als `target_ip` provisionieren
- Exakte Board-Ports am Laptop/PC dauerhaft notieren, da macOS Ports nach Reconnect neu enumerieren koennen
- MAC-Adresse des TX-ESP32
- Genaues mmWave-Modul und dessen Schnittstelle: USB oder UART
- Ob RuView direkt aus Release-Binaries geflasht wird oder lokal mit ESP-IDF gebaut wird

# Hardwareübersicht

Observatory untersucht einen Raum mit einem WLAN-Sender (TX1), vier WLAN-Empfängern (RX1–RX4) und getrennten mmWave-Referenzsensoren. Der Aufbau dient der Forschung; eine zuverlässige kontinuierliche Ortung ist noch nicht abgenommen.

| Bestandteil | Aufgabe | Grundlage |
| --- | --- | --- |
| 1 × ESP32-S3 als TX | Erzeugt kontrollierten WLAN-Verkehr für die CSI-Erfassung. | [TX-Firmware](../../software/ruview/firmware/esp32-csi-tx/README.md) |
| 4 × ESP32-S3 als RX | Erfassen CSI und übertragen Daten zum Host. RX1 und RX4 wurden zusätzlich als Paar untersucht. | [RX-Firmware](../../software/ruview/firmware/esp32-csi-node/README.md) |
| 2 × HLK-LD2450 mit ESP32-C3 | Vorgesehene getrennte Radar-Kalibrierungs- und Vergleichsreferenzen. | [mmWave-Firmware](../../software/ruview/firmware/esp32-mmwave-node/README.md) |
| Host-Rechner | Nimmt Daten entgegen und stellt Server und Oberfläche bereit. | [Softwarearchitektur](../software/architecture.md) |

## Aufbau

Für den mmWave-Träger ist PCB-03 die aktuelle vorgesehene Revision. PCB-01 und PCB-02 dokumentieren frühere Stände. Der Breadboard-Aufbau ist eine weitere Aufbauvariante. Fertigungsdaten, Bauteile und Montagehinweise stehen jeweils bei der Hardware; ein vorhandener Entwurf ist noch kein Nachweis einer erfolgreichen Fertigung oder Passprobe.

- [Hardwareübersicht mit Revisionen](../../hardware/README.md)
- [PCB-03: Quellen, Hinweise und Prüfberichte](../../hardware/pcb-03/README.md)
- [Breadboard: Aufbau und Bauteile](../../hardware/breadboard/README.md)

Raummaße, Montageorte und Ausrichtung sind Teil des jeweiligen Aufbaus. Historische Koordinaten aus einzelnen Versuchen nicht als allgemeine Vorgabe übernehmen. Bei den Radaren müssen insbesondere Zuordnung, Blickrichtung und Links-/Rechts-Spiegelung am realen Raum geprüft werden.

Weiter: [Installation](../setup/installation.md) · [Firmware und Flashen](../setup/firmware.md) · [mmWave-Referenz](../software/mmwave.md).

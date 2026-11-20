# Firmware vorbereiten und flashen

TX, RX und mmWave verwenden unterschiedliche Firmware und Geräteprofile. Diese Seite ordnet die vorhandenen Wege ein; sie ist noch keine vollständig validierte Erstinstallationsanleitung für einen neuen Gesamtaufbau.

## Vor einem Geräteeingriff

1. Gerät, Board, Firmwarequelle, tatsächliches Flashlayout und Zielgerät eindeutig zuordnen. Ein serielles Öffnen kann bereits einen Reset auslösen.
2. Erforderliche Betriebsunterbrechung und Schreibzugriff getrennt für den konkreten Eingriff freigeben lassen. Ein Inventur- oder Leseauftrag ist keine Flashfreigabe.
3. Bei bestehenden Geräten die benötigten privaten Konfigurationen und Sicherungen erhalten und den gerätegebundenen Rückweg vorab prüfen. Keine Zugangsdaten oder Speicherabbilder in das Repository übernehmen.
4. Nur das zum Gerät und Vorgang passende Image und die dafür geprüfte Schreibmethode verwenden; Schutz- und Authentisierungseinstellungen nicht lockern.
5. Nach einem zulässigen Eingriff Identität, Start, Konfiguration, Datenfluss und Rückkehr gemäß dem jeweiligen Prüfauftrag kontrollieren.

## TX1 – Sender

Die gepflegte ESP-IDF-Implementierung und der reproduzierbare Offline-Build stehen in der [TX-Firmwareanleitung](../../software/ruview/firmware/esp32-csi-tx/README.md#reproducible-rebuild-from-a-clean-checkout). Der Build öffnet und beschreibt kein Gerät.

**Bestehender TX1:** Die Migration hat einen gerätegebundenen Plan, ein vorhandenes Layout und einen gesonderten Rückweg. Die TX-Anleitung warnt ausdrücklich vor generischem `idf.py flash`, einem zusammengeführten Vollimage und `erase-flash` für diese Migration. Der interne Plan bleibt die lokale Arbeitsgrundlage.

**Neues TX-Board:** Ein neuer Satz Identitäten und Netzwerkdaten ist erforderlich. Die TX-Anleitung bezeichnet die initiale NVS-Erstellung und die Hardwareabnahme eines neuen Boards ausdrücklich als offen. Deshalb lässt sich aus dem vorhandenen Migrationswerkzeug noch kein vollständiger Erstinstallationsbefehl ableiten.

## RX1–RX4 – CSI-Empfänger

Die [RX-Firmwareanleitung](../../software/ruview/firmware/esp32-csi-node/README.md) enthält ältere allgemeine Build- und Flashwege. Dort genannte Legacy-Binärdateien sind für kontrollierte Lokalisierungs- oder D5/D6-Messungen ausdrücklich ungeeignet. Ihre Flashgrößen, Partitionen und Offsets dürfen nicht ungeprüft auf ein anderes Image oder einen bestehenden RX übertragen werden.

Im lokalen Arbeitsstand existiert ein separater Builder `build_rx_release.py` für die gemeinsame RX-App. Er bindet Quellen und ESP-IDF-Version, baut zweimal und beschreibt kein Gerät. Dieser Builder ist gegenwärtig unversioniert; sein Vorhandensein in einem frischen Checkout ist deshalb nicht zugesichert. Ein auf den tatsächlichen Gerätestand abgestimmter öffentlicher Installations- und Rückweg für diese Variante muss noch ausgearbeitet und geprüft werden.

## MMWAVE1 und MMWAVE2

Die [mmWave-Firmwareanleitung](../../software/ruview/firmware/esp32-mmwave-node/README.md#mmwave1-and-mmwave2-upload) beschreibt ESP-IDF 5.4, getrennte Konfiguration und Builds für beide stabilen Node-IDs sowie die erste USB-Installation. WLAN-Zugang, Serverziel und OTA-Token sind gerätespezifisch und privat zu behandeln.

Der dort beschriebene vollständige USB-Flash schreibt mehr als nur die App. Er ist für eine ausdrücklich beauftragte Erstinstallation beziehungsweise einen bewusst geplanten vollständigen Reflash gedacht. Bei einem bestehenden Gerät zuerst Layout, Daten und Rückweg prüfen. Node-ID und Funktion nach dem Start überprüfen; eine erfolgreiche Installation ersetzt keine Radar-Kalibrierung.

Quellen: jeweilige Firmwareanleitungen und [Hardwareübersicht](../hardware/overview.md). In dieser Dokumentationsarbeit wurden keine Geräte geöffnet, geflasht oder provisioniert.

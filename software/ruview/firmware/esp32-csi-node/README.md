# ESP32 CSI Node Firmware – Observatory-Einordnung

Öffentliche Auswahl aus der umfangreicheren Komponentenbeschreibung. Der Observatory-Aufbau verwendet RX1–RX4 als CSI-Empfänger; Datenqualität und Quellenbindung sind versuchsbezogen nachzuweisen. Die ursprüngliche Upstream-Funktionsliste ist keine Abnahme für diesen Aufbau.

## Daten und Grenzen

Die Firmware kann CSI erfassen und per UDP zum Host übertragen. On-device-Signalverarbeitung, heuristische Anwesenheits-/Vitalindikatoren und optionale WASM-Pfade sind getrennte Softwaremöglichkeiten. Die sogenannte Personenzahl in der beschriebenen Tier-2-Variante ist eine Subcarrier-/Slot-Heuristik, kein trainierter Beleg tatsächlicher Belegung. Diese Firmware führt keine Pose-Schätzung aus; angezeigte Skeletons oder Vitalwerte sind kein Observatory-Positions- oder physiologischer Nachweis.

## Legacy binaries and installation

Die archivierten v0.6.7-Binärdateien entstanden vor dem TX-Source-Binding-Trailer von v0.7.0. Sie dürfen nicht für kontrollierte Lokalisierung oder D5/D6-Messungen verwendet werden. Frühere 4-/8-MB-Layouts, Partitionen und Offsets sind keine allgemeine Schreibanweisung für bestehende RX.

Die Originalanleitung enthält ESP-IDF-5.4-Build-/Flashbeispiele für vollständige Quellbäume. Dieser Dokumentationssatz enthält weder Buildumgebung noch Images oder Provisionierungsdaten. Ein gerätegebundener Installations-/Rückweg ist separat zu prüfen; serielles Öffnen kann bereits einen Reset auslösen. Betriebsunterbrechung und Schreibzugriff brauchen jeweils die passende Freigabe.

Ein lokaler separater `build_rx_release.py`-Stand ist von den Legacy-Beispielen zu unterscheiden. Seine bloße lokale Existenz oder ein bestandener Softwaretest belegen keinen allgemeinen Erstinstallationsweg.

## Quellen und nächste Einordnung

Der [gebundene öffentliche CSI-Kollektor](https://github.com/Gerafftes/Observatory/blob/028f9d3f2703e1f3610e856f48fa0a709647818b/software/ruview/firmware/esp32-csi-node/main/csi_collector.c) ist eine Quellreferenz, kein Nachweis des laufenden Gerätestands. [Firmware vorbereiten](../../../../docs/setup/firmware.md), [CSI-Qualität](../../../../docs/software/csi.md) und der [Tag14-Abschluss](../../../../docs/project/rx-unified-tag14-result-20261029.md) unterscheiden Software, technischen Transport und offenen Positionsnutzen.

# Koordinatorreview: RX3-C1-Pilot vom 14. Oktober 2026

**RX3-Pilot bestanden. Rückweg belegt, C1 installiert und am Laufende auf ota_0 / VALID bestätigt; acht exportierte Configwerte erhalten. RX2 blieb auf C. G1 bleibt offen.** Nächster Schritt ist ein gebündelter Folgeauftrag für RX1 und RX4: eigene Rückwegprüfung, Installation nacheinander und abschließend ein vollständiger 30-Minuten-Vier-RX-Lauf.

Grundlage ist der [Abschlussbericht](../../private/rx3-rueckweg-c1-pilot-20261014T163119Z/REPORT.private.md) mit Originalantworten, Receipts, ROM-Leseproben und PCAP. Dieser Review erfolgte ausschließlich lokal; kein neuer Gerätezugriff und kein Agent gestartet. Letzter Live-Endbeleg: **14. Oktober 2026, 18:46:18 Europe/Berlin**. Er bestätigt den Zustand am Laufende, keine spätere Erreichbarkeit.

## Installation, Rückweg und Erhaltung

Der einzige OTA-POST auf RX3 endete mit HTTP 200, Prozess-Exit 0 und vollständigen 1.238.784 Uploadbytes. Frische Statusantworten nach OTA und vor/nach Capture bestätigen den gebundenen C1-Runtime-Digest, ota_0 / VALID und denselben neuen Boot während der Messung. Config- und Diagnoseantworten bestätigen acht typgenau unveränderte Werte sowie die erwartete TX-Quelle und Diagnose-Quellidentität. RX2 behielt C, Boot und Config.

Je zwei Bootloader- und Partitionstabellenlesungen sind bytegleich. Das aktuelle 18.960-Byte-Bootloaderimage passt exakt zum gebundenen alten Build; Imageprüfung, Tabellen-MD5, Grenzen und beide 2-MiB-Appslots stimmen. Die passende lokale Bootloaderkonfiguration belegt aktiviertes App-Rollback. Der Rückweg gilt für einen Startfehler **vor erfolgreicher Appvalidierung**. Es wurde kein fehlgeschlagener Boot provoziert und kein tatsächliches Rollback getestet; ein späterer Qualitätsfehler nach VALID garantiert keine automatische Rückkehr.

Die historischen Diagnoseworte `RX2-v2` auf RX3 wurden nicht als Geräteidentität verwendet. Die Zuordnung beruht auf Node, MAC, Boot und Runtime-/Quellbindung. Es gibt keinen neuen Firmwareänderungsbedarf aus diesem Rollenstring.

## CSI-Ergebnis

| Metrik im 120-Sekunden-Messfenster | RX3 / C1 | RX2 / C |
|---|---:|---:|
| Gültige eigene, zum 128er-Raster passende CSI | 2.020 | 1.869 |
| Passende CSI pro Sekunde | **16,833** | **15,575** |
| Gültige eigene Auswahlfenster | **113/113** | **113/113** |
| Eigene Fenster nach der alten strengen Regel | 113/113 | 113/113 |
| Größte innere passende Datenlücke | 459,564 ms | 427,213 ms |
| Gültige Formatausschlüsse | 0 | 0 |
| Ungeklärte Sequenzpositionen / Übergangsnenner | 8/2.027 | 9/1.877 |

Auch die gemeinsamen **Zwei-RX-Pilotfenster** bestehen mit 113/113. Die vorab festgelegte Schwelle betrug mindestens fünf passende CSI/s und 102/113 Fenster je RX. Sie wurde ohne Regeländerung erfüllt. Dies rechtfertigt den nächsten Rolloutschritt für RX1/RX4, aber kein Vier-RX-G1-PASS.

Die Aufnahme ist vollständig: 120,000979 s monotone Messdauer, rund zwei Sekunden Vor- und Nachlauf, 4.463 Records, geschlossener Captureprozess, Exit 0 und null Kernel-Drops. Interface-Drops sind unbekannt. 51 Records nach der Endmarke belegen den Nachlauf. Keine Zeitstempelregression; Clock-Ankerspanne 1,804083 ms. Kein HTTP-Zugriff während des gesamten Capturelebenszyklus.

Der frühere RX3-Lauf mit alter App erreichte 3,903 passende CSI/s; der neue Pilot erreicht 16,833/s. Die neue Format-/Betriebsqualität ist positiv belegt. Unterschiedliche Messdauer und Aufbau-/Funkbedingungen erlauben keine isolierte HT20-Kausalitätsbehauptung. Ungeklärte Sequenzpositionen bleiben ungeklärt und sind keine bewiesene Funkverlustquote.

## Unabhängig geprüfte Belege

- Alle **501 Manifestdateien** des Piloten und **138 versiegelten Eingangsdateien** hinsichtlich Größe beziehungsweise SHA-256 geprüft; außerdem alle **4.297 C1-Manifestdateien** erneut verifiziert.
- Alle **23 HTTP-Receipts** gegen ihre Originalbodies und die gespeicherte Historie geprüft: 22 GETs, genau ein vollständiger RX3-OTA-POST. Config-, Runtime-, Boot- und Diagnosebindungen stimmen.
- Original-PCAP erneut mit der ausgewählten und der alten strengen Analyse dekodiert. Beide Ergebnisse stimmen jeweils exakt mit den gespeicherten Zusammenfassungen überein. Sequenzlücken und Nenner zusätzlich durch unabhängige Mengenrechnung bestätigt.
- Alle **95 Fake-/Offline-Tests** erneut bestanden. Der Testtreiber sperrt echte Sockets, Prozesse und Device-/Tokenzugriffe. Die Belegprüfung blockiert neue Netzwerk-/Devicezugriffe und Artefaktschreibzugriffe; Originalartefakte wurden nicht verändert.
- Vor den Dokumentationsänderungen dieses Reviews waren Git-Arbeitsstatus und Hashes der ursprünglichen geänderten/unversionierten Dateien gleich dem Laufbeginn. Keine Produktdatei verändert. Nach der Planaktualisierung bestehen alle **15 Trackerprüfungen** und die Whitespaceprüfung.

## Verbrauch und nächster Auftrag

Die aktiven Abschnitte dauerten zusammen **226,325 s**, etwa 3 min 46 s, unter allen festgelegten Zeitgrenzen. Belegt sind ein ROM-Eintritt, zwei initiale Syncversuche, vier begrenzte ROM-Lesungen mit zusammen 72 KiB, ein normaler Wartungsrückkehrstart, ein App-OTA-POST und ein Capture. Zusätzlich erfolgte der firmwareseitige OTA-Neustart. Kein manueller Flash-/Restorelauf, Stub, Baudwechsel oder Konfigurationsumbau. Die einmaligen RX3-Freigaben A/B sind verbraucht.

**Nächster konkreter Schritt:** RX1 und RX4 im selben Folgeauftrag nacheinander qualifizieren und auf das bereits geprüfte C1-Image bringen. Vor jedem Upload eigene aktuelle Identitäts-/Config-/Slot- und Rückwegbindung; bei einem unklaren Ergebnis keine Fortsetzung auf dem anderen RX. RX3 bleibt C1, RX2 bleibt C, TX1 und Peerprofil bleiben unverändert. Nach erfolgreichen Installations- und Endzustandsgates genau einen vollständigen 30-Minuten-Vier-RX-Lauf mit Vor-/Nachlauf und Endprüfung durchführen. Die bestehende G1-Schwelle bleibt mindestens 90 % gültige gemeinsame Fenster; bei 1.793 Fenstern mindestens 1.614.

Hierfür werden **RX1 und RX4 nacheinander am COM-/UART-Datenanschluss des Mac** benötigt. TX1, RX2 und RX3 bleiben für Funkvergleich und gemeinsame Abnahme eingeschaltet; danach müssen alle vier RX gleichzeitig im Funkaufbau laufen. Keine zusätzlichen Radare für diesen Format-/Transportlauf erforderlich. Geräteunterbrechung und C1-Schreibzugriff für RX1/RX4 brauchen eigene passende Freigaben; der RX3-Pilot erteilt sie nicht.

Kein weiterer allgemeiner Offline-Firmwareauftrag ist aus diesem Ergebnis begründet. G1, physische LTF-/Maskenbindung, Serverannahme, Ressourcen, RX↔RX-Funk-/Positionsnutzen und Tracking bleiben eigenständige offene Nachweise. Tag 2 bleibt 7/7; Tag 3 ist noch nicht abgeschlossen. **Schwierigkeit des nächsten gebündelten Schritts: 4/5**, einschließlich der bisherigen Versuche und der zwei noch gerätespezifisch zu bindenden Installationen.

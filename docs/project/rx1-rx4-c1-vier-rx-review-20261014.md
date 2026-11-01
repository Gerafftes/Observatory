# Koordinatorreview: RX1/RX4-C1 und Vier-RX-Lauf vom 14. Oktober

**RX1 und RX4 sind am bestätigten Laufende auf C1 / ota_0 / VALID. RX2 bleibt C, RX3 C1. Der vollständige 30-Minuten-Vier-RX-CSI-Vertrag besteht mit 1.786/1.793 gemeinsamen Fenstern, 99,61 %. Gesamt-G1 und Tracking bleiben offen.**

Grundlage: [Abschlussbericht des neu freigegebenen Laufs](../../private/rx1-rx4-c1-vier-rx-restart-20261014T175514Z/REPORT.private.md), gespeicherte Originalantworten, ROMproben, Ausführungssiegel und PCAP. Letzter Live-Endbeleg **14. Oktober 2026, 20:34:10 Europe/Berlin**. Dieser Koordinatorreview erfolgte ausschließlich lokal: keine Geräteabfrage, kein Capture/Reset/Upload und kein Agent gestartet.

## Geräte und Installationsabschluss

RX1 und RX4 wurden nacheinander bearbeitet. Je Gerät eigene S3-/MAC-/Portbindung, je zwei bytegleiche Bootloader- und Tabellenlesungen, verifiziertes Image und Partitionstabellen-MD5 sowie passend gebundene lokale Buildkonfiguration. Beide Rückwege sind für einen Startfehler vor erfolgreicher Appvalidierung belegt; kein absichtlicher Fehlerboot oder tatsächlicher Rollback getestet. Schlechte Qualität nach VALID ist kein garantierter Rückkehrauslöser.

Je RX genau ein C1-App-OTA-POST, curl Exit 0 / HTTP 200 und 1.238.784 Uploadbytes. Frische Status-, Config- und Diagnoseantworten bestätigen das gebundene C1, neuen stabilen Boot, ota_0 / VALID und acht typgenau unveränderte Configfelder. RX2/RX3 behielten ihre am Anfang frisch gebundenen Boots und Configs über den gesamten Lauf. Alle vier Endgates bestehen. Geerbter Diagnose-Rollenstring `RX2-v2` war kein physischer Gerätebeweis.

## Gemeinsame CSI-Qualität

| Empfänger | Image / Endzustand | Passende 128er-CSI/s | Eigene gültige Auswahlfenster | Eigene strenge Fenster |
|---|---|---:|---:|---:|
| RX1 | C1 / ota_0 / VALID | **17,072** | **1.790/1.793** | 1.787/1.793 |
| RX2 | C / ota_0 / VALID | **19,660** | **1.792/1.793** | 1.789/1.793 |
| RX3 | C1 / ota_0 / VALID | **18,296** | **1.792/1.793** | 1.789/1.793 |
| RX4 | C1 / ota_0 / VALID | **19,142** | **1.792/1.793** | 1.786/1.793 |

Ausgewählte gemeinsame Fenster **1.786/1.793 = 99,61 %**, alte strenge Regel **1.780/1.793 = 99,27 %**. Beide bestehen die unveränderte Mindestzahl 1.614. Alle RX bestehen zusätzlich mindestens fünf passende CSI/s und die eigene Mindestfensterzahl. Sieben gemeinsame Auswahlfenster scheitern an weniger als 2,5 s gemeinsamer Abdeckung; sie bleiben im Nenner und mit ihren Gründen erhalten. Keine Abnahmeschwelle geändert.

Gültige CSI und Rasteranteil: RX1 30.730/30.729, RX2 35.389/35.388, RX3 32.933/32.932, RX4 34.459/34.456. Nur sechs gültige 64er-Formatausschlüsse insgesamt, keine 192er. Ungeklärte Sequenzpositionen separat: RX1 124/30.853, RX2 201/35.589, RX3 123/33.055, RX4 149/34.607. Je eine gebundene Boot-/Quellenepoche, keine Duplikate oder verspäteten eindeutigen Pakete. Keine bewiesene WLANverlustursache daraus ableiten.

Messfenster 20:04:05–20:34:05, monotone Dauer 1.800,005098 s. Vollständiger PCAP mit 147.976 Records und 54.733.320 Byte; etwa zwei Sekunden Vor-/Nachlauf, 139 echte Records nach der Endmarke. Exit 0, eigener Captureprozess geschlossen, null Kernel-Drops, Interface-Drops unbekannt. Keine Zeitstempelregression, Clock-Ankerspanne 22,696334 ms unter 50 ms. Keine HTTP-/UART-/ROM-/Resetaktion im gesamten Capturelebenszyklus bis Cleanupende. Alle zwölf End-GETs erfolgten danach.

## Unabhängige Prüfung und begrenzte Fortsetzung

- Alle **2.519 Manifestdateien** des neuen Laufs geprüft; erstes Ausführungssiegel **169** Eingänge und separates Fortsetzungssiegel **sieben** Eingänge unverändert. Zusätzlich gesamte C1-Lieferung **4.297**, RX3-Pilot **501** und erster abgebrochener RX1/RX4-Lauf **150** Manifestdateien erneut verifiziert.
- Alle **61 HTTP-Receipts**, einschließlich genau zwei vollständiger OTA-POSTs und **zwölf Diagnoseantworten**, gegen Originalbodies, History, erwartete Runtime-/Source-/Bootbindung geprüft. Alle acht Configfelder je RX sowie beide Kontrollboots stimmen. Eigene ROMidentitäten, alle acht Lesehashes/Wiederholungen, Bootloader-Build und Tabellen-MD5 passen.
- Original-PCAP erneut mit **beiden** Analysen dekodiert: Zusammenfassungen jeweils exakt reproduziert. Sequenzmengen und Nenner zusätzlich unabhängig bestätigt. Capturevollständigkeit, Clock, Endzustände und gemeinsame Mindestschwellen geprüft. Der lokale Prüflauf blockierte neue Netzwerk-/Geräte- und Artefaktschreibzugriffe; Originale blieben erhalten.
- **97 ursprüngliche plus vier Fortsetzungstests = 101 Offline-/Fake-I/O-Tests** erneut bestanden, einschließlich echte I/O sperrendem Testtreiber. Budgetstarts unverändert, alle Zeit-/Aktionsgrenzen eingehalten und erforderliche 45-Minuten-Globalreserve vor C vorhanden.
- Vor den Koordinator-Dokumentationsänderungen dieses Reviews waren ursprünglicher Gitstatus und Hashes der anfänglich geänderten/unversionierten Dateien gleich dem Agentenlaufbeginn. Keine Produktdatei durch diesen Review geändert. Nach der Plan-/Trackeraktualisierung bestehen alle 15 bestehenden Trackerprüfungen sowie die Whitespaceprüfung.

Der erste Auftrag stoppte an fehlender RX4-Cachebindung nach sechs GETs, ohne seriellen Zugriff, Reset, Upload oder Capture. Dieser historische Nichtstart bleibt erhalten. Johann autorisierte anschließend direkt einen **neuen Einmallauf mit neuen Budgets** und eine einmalige geplante Offlineauswertung.

Der neue Einstieg stoppte zunächst ausschließlich nach passiver USBinventur: Zwei COM-Kabel widersprachen seiner zu engen Ein-Port-Regel, null Geräteaktionen. Johann bestätigte ausdrücklich das unveränderte RX1-Kabel und RX4 als zweites Gerät sowie die Fortsetzung innerhalb derselben Budgets. Die minimale Auswahlkorrektur erhielt vier eigene Tests und ein separates Siegel. Die ursprüngliche monotone Startzeit, Zähler und Verbrauchsmarker wurden nachweislich nicht zurückgesetzt. Maßgeblich sind `continuation-live-result.private.json` und `continuation-http-history.private.json`; die alten Nullaktions-Enddateien bleiben bestehen. Die direkten Autorisierungen wurden im Agentenchat „14“ gelesen und mit den Artefakten abgeglichen.

Tatsächlicher neuer Gesamtverbrauch: **37 min 50,086 s**, 59/64 GETs, zwei von zwei erlaubten POSTs, ein Capture, zwei ROM-Eintritte, vier initiale Syncversuche, acht Lesungen mit zusammen 144 KiB und zwei normale A-Rückkehrstarts. Kein zusätzlicher B-Reset; dazu je ein OTA-Neustart. Kein manueller Restore, Stub, Baudwechsel, Config-/Security-/Netz-/Dienstumbau oder Firmware-Neubau. Die einmalige Auswertungsautomation ist laut gebundenem Abschluss pausiert. Alle Freigaben dieses abgeschlossenen Laufs sind verbraucht.

TX1 stand laut Johann nun am normalen Standort. Diese neue Funkaufstellung unterscheidet sich vom früheren Wartungsaufbau. Der positive neue Befund beweist daher keine isolierte Firmware-/HT20-Kausalität. Der Standort ist eine Nutzerangabe, keine vermessene Geometrie oder Kalibrierung.

## Heutiger Fortschritt und nächster Schritt

**Tag 3, 14. Oktober: ein von fünf geplanten Punkten abgeschlossen.** `g1` bezeichnet im Tracker nun ausdrücklich den bestätigten **CSI-Teil** des 30-Minuten-Vertrags. Gesamt-G1 bleibt offen. Tag 1 bleibt 5/5, Tag 2 7/7.

Die vier offenen heutigen Punkte sind: physische Layout-/Gültigkeitsmaske und Zeitvertrag (`data-rules`), vollständige Fehler-/Ressourcen-/Datenlückenbilanz (`data-accounting`), gerätegebundene Vorbereitung des Paarversuchs (`peer-trial`) und tatsächlicher RX1↔RX4-Funktest (`peer-pair-test`). Die gemessene Format-/Sequenzbilanz ist abgeschlossen; sie ersetzt nicht die noch fehlende Gesamtbilanz. Der offline implementierte Peerpfad allein schließt keinen dieser Live-Nachweise.

**Nächster konkreter Schritt: einen gebündelten Offline-Auftrag zur Qualifizierung der bestehenden Datenbasis vorbereiten.** Zuerst R1/R4: tatsächliches verwendetes 128er-Layout, Verarbeitungskonfiguration und Gültigkeitsmaske verbindlich aus den gebundenen Quellen/SDK-/Paketbelegen herleiten oder fehlende Belege präzise benennen. Im selben Offlineblock den vorhandenen Zeit-/Bilanzvertrag und die Kompatibilität der Paarversuchskandidaten mit dem jetzt bestätigten C1-Stand prüfen, damit der folgende Hardwaretermin gezielt vorbereitet ist. Keine neue Installation oder Hardwaremessung aus diesem Review.

Bestätigten Firmwarestand beibehalten: RX1/RX3/RX4 C1, RX2 C. Kein weiterer allgemeiner Format-/Flash-/Captureversuch begründet. Kalibrierung, physische Maske, vollständige Ressourcen-/Fehlerbilanz, echte Serverannahme, Radarreferenz, Positionsleistung und RX↔RX-Nutzen bleiben eigene Nachweise. **Schwierigkeit des nächsten Offlineblocks: 4/5**; bisherige Versuche berücksichtigt, die konkrete Firmware-/Transporthürde ist jetzt geschlossen.

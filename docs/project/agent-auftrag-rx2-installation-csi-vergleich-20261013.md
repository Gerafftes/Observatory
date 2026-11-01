<!-- Publication copy: local paths and hardware identifiers were pseudonymized; private original retained. -->
# Agent Auftrag RX2 Kandidat C installieren und CSI vergleichen

**Tag 2, 13. Oktober 2026. Schwierigkeit: 4/5**, einschließlich des früher abgebrochenen OTA-Versuchs und der noch offenen HT20-Funkwirkung. Johann hat TX1 per USB und RX2 per COM/UART mit dem Mac verbunden. Dieser Auftrag ist vorbereitet; kein Agent wurde gestartet und kein Gerätezugriff ausgeführt.

**Vor Ausführung sind die Freigaben A, B und C direkt von Johann zu bestätigen.** Die unten beschriebenen Grenzen bilden den konkreten vorgeschlagenen Umfang. Eine Bestätigung im menschlichen Startprompt genügt; danach nicht erneut dieselben Freigaben anfordern. Frühere Wartungs-/OTA-/Capturefreigaben werden nicht übernommen.

```text
AUFTRAG: Einmalige RX2-App-OTA mit Kandidat C und 120-s-CSI-Vergleich

Projekt: .
Plan: Tag 2 / 13. Oktober 2026.

Führe ausschließlich diesen begrenzten Auftrag aus, sobald Johann die
Freigaben A/B/C ausdrücklich bestätigt hat. Keine weiteren Agenten starten,
keine anderen Chats anschreiben. Nach Lieferung stoppen.

ZIEL UND FEHLER

Das ist der Fehler:
Der alte 120,001-s-Mitschnitt enthält 2.198 gültige RX2-CSI-Frames, davon
nur 231 im vorhandenen 128er-Raster: 1,925/s. 1.967 vollständige 192er-Frames
sind Formatausschlüsse. 0/113 diagnostische Teilfenster nach Einschwingen
bestanden. Neun Sequenzpositionen bei 2.206 Übergängen bleiben ungeklärt;
ihre Ursache ist nicht als WLAN-/TX-Verlust nachgewiesen.

Das erwarte ich:
Exakt Kandidat C über den bestehenden authentisierten App-OTA-Weg installieren,
neuen Boot/Image/Source/OTA-Zustand und Konfiguration bestätigen und genau
ein 120-s-Fenster passiv aufnehmen. Absolute passende 128er-Rate und gültige
diagnostische Fenster mit dem gespeicherten Ausgangsbefund vergleichen.
Eine geringere 192er-Quote allein ist kein Erfolg.

Kandidat C ist offline geprüft: RX2-HT20-20261013-v1 plus NULL-Guard.
Der separate Host-Sync-Zählerfix ist ebenfalls offline geprüft, aber nicht
aktiviert. Diesen Hostkandidaten hier nicht starten oder installieren.
Die unabhängige PCAP-Bilanz benötigt keine Hostdienständerung.

FREIGABEN UND FESTE GRENZEN

A – RX2-Betriebsunterbrechung:
 Höchstens 20 Minuten ab dem ersten aktiven RX2-Request bis zum Ende der
 letzten Hardware-/Captureaktion, monoton gemessen. Umfasst den normalen
 OTA-Neustart und unveränderten Firmware-Selbsttest/automatischen Rollbackpfad.
 Keine seriellen Portöffnungen, DTR/RTS/BREAK, manuellen Resets, ROM/Stub,
 Strom-/Kabelwechsel oder provozierten Rollbacks. COM bleibt geschlossen.

B – RX2-Geräteschreibzugriff:
 Höchstens EIN authentisierter POST /ota mit exakt der unten gebundenen
 Kandidat-C-Appdatei in den frisch bestätigten inaktiven OTA-Slot, inklusive
 normaler OTA-Bootauswahl-/Bestätigungsmetadaten durch die Firmware.
 Keine gezielten Bootloader-, Partitions-, NVS-, PHY-, SPIFFS- oder
 Sicherheitsänderungen; kein Restore, zweites Image oder serieller Ersatzweg.
 Ein gestarteter POST-Prozess verbraucht den einen Versuch auch bei Timeout.

C – Begrenzte Verifikation und Aufnahme:
 Höchstens 20 RX2-GETs insgesamt, ausschließlich /ota/status, /config und
 /config?diag=1 am eindeutig gebundenen RX2-Endpunkt. Genau ein passiver
 120-s-Hostmitschnitt nach bestätigtem Kandidatenboot. Keine TX1-/anderen
 RX-/Radarabfragen, keine Scans/Pings/ARP-Probes, kein Hostnetz-/AP-Umbau,
 keine Anfrage an die aktive Sensing-API und keine Dienstaktivierung.

TX1, andere RX, Radare, produktive Hostruntime und Sicherheitswerte erhalten.
Keine Installation/Downloads von Werkzeugen oder neue Firmwareentwicklung.
Arbeitsbaum, Backups und Originalbelege erhalten; keine Git-Eingriffe.
ProjectAtlas nicht verwenden. Alle drei Freigaben gelten nur für diesen Lauf.
Bei fehlender Freigabe ausschließlich lokale Vorbereitung abschließen.

1. LOKALE VORBEREITUNG UND EINGANGSBINDUNG

Arbeitsbaum und wirksame AGENTS.md prüfen. Neue private Ablage verwenden,
z.B. private/rx2-install-csi-20261013-<UTC>/; Ignore UND Nichtversionierung
prüfen, Ordner 0700/Dateien 0600. Keine vorhandenen Ablagen überschreiben.
Tokens, NVS-/Gerätewerte, Rohantworten und Payloads nicht in öffentliche
Berichte, Argumentlisten, Shellhistorie oder Konsolenausgaben kopieren.

Maßgebliche Eingänge, relativ zum Projekt:
 private/tag2-nullschutz-sync-offline-20261013-v1/
   REPORT.private.md, DELIVERY.private.json, BUILD-BINDUNG.private.md,
   HARDWAREVERGLEICH-NICHT-AUSFUEHREN.private.md, relevante Manifesteinträge;
   build-C/rx/esp32-csi-node.bin und gebundene C/rx-OTA-/Diagnosequellen.
 private/rx2-maintenance-run-20261012-20261012T180912Z/
   REPORT.private.md, final-verification.private.json, get-receipts.private.json;
   nötige gebundene Originalstatus-/Config-/Netzbelege aus observations/.
 private/rx2-backup-next-plan-B-sync5-v1/
   INSTALLATION-UND-RUECKWEG.private.md, partition-analysis.private.json,
   app-analysis.private.json, otadata-analysis.private.json,
   hardware-security.private.json und Manifestbindung der Vollsicherungen.
 private/rx2-diagnostic-install-20261011-v2/
   PROCEDURE.private.md, install_once.py: nur als Methoden-/Offline-Testvorlage.
 private/rx2-csi-120s-20261012-20261012T193815Z/
   REPORT.private.md, capture_once.py, pcap_offline.py, analyse_saved.py,
   capture-config.private.json und gebundene Setup-/TX-Quellenreferenzen.

Alte Helfer nicht unverändert ausführen: Kandidat, Länge, Hashes, Pfade,
Zeit-/Requestbudgets und Einmalmarker müssen zum neuen Auftrag passen.
Nur benötigte reine Funktionen wiederverwenden; neue Kopien lokal prüfen.
Insbesondere enthält der alte OTA-Helfer alte feste Kandidatenwerte.

EXAKTES LIEFERIMAGE:
 private/tag2-nullschutz-sync-offline-20261013-v1/build-C/rx/esp32-csi-node.bin
 Länge: 1.239.232 Byte
 BIN-Datei-SHA256:
 b896d45ad992623f6d56cadc9927824a7869a989d05232f28ad1c5f5ae39ce9e
 Erwarteter Runtime-Image-Digest:
 e674e9fa055c05037c95a9df7c99dd2097b2c3544016ea99794888721a6a191a
 Erwartete Diagnose-Source-SHA:
 5dd96c6299d99cfa86ad3dac04c5d08d9747657a6e55134d0ebae4f7b5ccd4bc
 ELF-SHA256:
 a72b6832de16abab60dd8274705741d0175a4796ae33e78deba02cfbb950334c

Appimage/ELF/Manifest gezielt prüfen, nicht sämtliche Build-/Historienbäume.
Beide vorhandenen lokalen Vollsicherungen auf je 16.777.216 Byte und gemeinsamen
SHA256 fcb4fa023a160016018880d496af2dc7bdf03dd5ff2abc8c11867ac9c414d2c3
prüfen; Bytegleichheit bestätigen. Keine neue Gerätesicherung, Kopie oder
Extraktion erforderlich. Vorhandenen alten App-Rückweg und seine Grenzen
lesen. Ein vorhandener Rückweg ist keine Restorefreigabe oder Restoregarantie.

Der frühere Upload endete mit curl 56/HTTP 000; seine Ursache wurde nicht
behoben oder geklärt. C ist ein neuer Formatkandidat, kein bewiesener OTA-Fix.
Genau ein neu freigegebener Installationsversuch, keine Recovery-/Retryschleife.

Vor Livebeginn Helfer/Decoder/Filter/Referenzen/Toolversionen versiegeln und
gezielte Doubles-/Fixturechecks abschließen: falsches Gerät/Slot/Config,
fehlende Freigabe, ein verbrauchter POST, Timeout/HTTP000 mit ausschließlich
lesender Nachprüfung, falscher Image-/Source-/Bootwert, fehlendes VALID,
Request-/Zeitlimit, leeres Capture, beschädigter Tail, Clocksprung und Cleanup.
Captureberechtigung vor dem Installationsversuch klären; bei fehlender
Capturefähigkeit keine Installation nur für einen anschließend unmöglichen Test.
Keine echten Captureprozesse zur Probe und keine permanente BPF-Rechteänderung.

2. FRISCHE RX2-ZUORDNUNG UND GENAU EINE APP-OTA

Vor jeder aktiven Anfrage erwartete physische RX2-/MAC-/Interfacebindung
passiv aus vorhandenen Belegen und aktuellem Netzkontext bestätigen.
IP/Port/Interface nicht hart aus alten Beispielen übernehmen. Kein Scan.
Fehlender Cache allein beweist keinen Ausfall; unbeteiligte alte IP-Aliase
nicht gegen einen separat korrekt gebundenen Endpunkt ausspielen.
Bestätigte falsche/mehrdeutige Zuordnung stoppt vor dem ersten Request.

Start des 20-Minuten-Budgets unmittelbar vor dem ersten aktiven Request.
Vorher genau /ota/status, authentisiertes /config, /ota/status lesen.
Node RX2/2, gültige nichtnull Boot-ID, stabiler Vorherboot, bekannter voller
Runtime-Digest, laufender VALID-Slot und zulässiger anderer nächster Slot
müssen passen. Letzte alte Runtime:
 e09d3a227dd7419b32009361b4d02ca49fa536f2a8d28816525854493a90b354
Letzter belegter Zustand war ota_1/VALID, nächster ota_0; aktuellen Zustand
prüfen, nicht blind die historische Zieladresse schreiben. Slotkapazität
2 MiB, Kandidatreserve 857.920 Byte. Schutz-/Chip-/Partitionskompatibilität
an die gebundenen unveränderten Gerätebelege knüpfen; bei konkreter Abweichung
stoppen, keine Schutzwerte passend ändern oder in ROM ausweichen.

Acht Exportfelder nach Typ UND Wert mit der letzten gebundenen Referenz
vergleichen: target_ip, target_port, node_id, csi_channel, edge_tier,
tdm_slot, tdm_total, filter_mac_configured. Keine Configschreiboperation.
Falls C bereits exakt und VALID läuft, keinen unnötigen POST durchführen;
Source-/Config-/Bootprüfung abschließen und als ALREADY_INSTALLED ausweisen.
Ein fremdes/unbekanntes Image berechtigt nicht zum Überschreiben.

Unmittelbar vor POST Bindung und Kandidatenhash erneut prüfen. Exklusiver
Lauf-/POST-Verbrauchsmarker verhindert Doppelstart; keine Marker löschen.
Genau ein Request: POST /ota, HTTP/1.1, application/octet-stream, feste
Content-Length 1239232, Originaldatei als unveränderte Binärdaten, kein
Multipart/Chunking/Expect-Handshake, keine Proxy-/Redirect-/Retryverwendung.
Vorhandenes Authverfahren unverändert; Token nur über geschützte stdin- oder
Configübergabe. Connect höchstens 3 s, POST höchstens 120 s innerhalb Restbudget.
Uploadbeginn/-ende, Exitcode, HTTP-Code, size_upload, Antwortgröße/-hash und
monotone Zeiten privat speichern. Host-Uploadbytes sind keine Flashbestätigung.

3. BOOT, KONFIGURATION UND DIAGNOSE BESTÄTIGEN

Nach POST unabhängig von dessen HTTP-Ausgang begrenzt ausschließlich lesend
beobachten: bis zu zwölf /ota/status-Versuche innerhalb 180 s, höchstens
3 s Connect/7 s pro GET, bis zu 10 s Abstand, stets im Gesamt-/20-GET-Budget.
Bei fehlender aktueller passiver Bindung keine blinden Anfragen senden.
Keine langen blockierenden Sleeps; Restzeit vor jeder Aktion prüfen.

Installiert gilt C erst bei vollständigem Runtime-Digest, neuer Boot-ID
gegenüber Vorher (außer ALREADY_INSTALLED), passendem neuen laufenden Slot,
VALID und drei konsistenten Statusbelegen mit jeweils mindestens 3 s Abstand.
Diese Statusbelege zählen zu den zwölf erlaubten Nachbootversuchen.
HTTP200 allein reicht nicht. Auch bei Upload-Timeout kann ausschließlich
der echte Endzustand C bestätigen; keine automatische zweite Installation.

Nach bestätigtem C einmal /config gegen unmittelbaren Vorherexport und
historische Referenz vergleichen. Danach /config?diag=1 einmal zum Binden/
Aufwärmen des ersten Imagehash-Exports, einmal direkt vor und einmal direkt
nach Capture. Zusätzlich genau ein abschließender /ota/status.
Maximalbudget: 3 Vorher-GETs + 12 Nachbootstatus + 1 Config + 3 Diagnose
+ 1 Endstatus = 20 GETs. Vor Capture bei fehlendem Restbudget stoppen.
Jede Antwort technisch begrenzen, höchstens 16 KiB; Bodies/Receipts privat.

Diagnosevertrag: schema tx1.offline-diagnostic.v1, role RX2-v2, complete=true,
exakter Image-/Source-Digest, dieselbe Boot-ID wie Status. Statusboot ist
Hexstring, Diagnoseboot uint64; numerisch korrekt vergleichen. Fünf Klassen,
14 stages, acht features, 14 grid_stages, Typen/Zeitfolge/Teilmengen prüfen.
Ersten Hashaufwand getrennt dokumentieren. Keine hohe CSI-Rate voraussetzen,
um den Diagnoseexport gültig zu nennen; kein atomarer Mehrstufen-Snapshot.

Bei fehlendem C/VALID oder geänderter Config/Quelle kein Kandidatenvergleich.
Restliche erlaubte lesende Zustandsprüfung darf bisherige App/Config bestätigen;
andernfalls Endzustand ausdrücklich unbekannt. Kein Reset/Restore/Retry.

4. GENAU EIN 120-SEKUNDEN-CSI-FENSTER UND VERGLEICH

Erst nach C-/Config-/Diagnosebestätigung passiv am bisherigen Host aufnehmen.
Restbudget für volle 120 s plus Capturecleanup (höchstens 10 s) und die
abschließenden erlaubten GETs reservieren. Genau ein Captureprozess, normaler
BPF-Tap, -p, volle Snaplen, numerisch, enger RX2-UDP-Filter zum bestehenden
Hostziel. Kein zweiter UDP-Socket, SO_REUSEPORT, Relay, Recorder oder Server.
COM bleibt geschlossen; keine Geräteabfragen während des Messfensters.

Readiness = Listening/geöffneter Capturehandle plus gültiger PCAP-Header,
nicht erstes Paket. Danach monoton volle 120 s, auch bei null CSI oder
Formatabweichungen. Uhranker erfassen; Unix-/Monotonzeit getrennt erhalten.
PCAP höchstens 64 MiB; technischer Fehler/Größenende/physischer Quellenkonflikt
ergibt INCOMPLETE. Keine zweite Aufnahme, Rotation oder stille Tailkorrektur.
Nur eigenen Captureprozess beenden, stderr/Exitcode/Kerneldrops sichern.

Neue Referenz an Kandidatenboot binden, keinen alten Bootfilter unverändert
übernehmen. TX-Filterhash aus unabhängiger Projekt-/Vorherreferenz verwenden,
nicht aus erstem Paket erfinden. Keine exakte TX-App-/Boot-/Slotzuordnung
behaupten. CSI, Sync, Feature-State, Mesh-Health/Anomalien getrennt klassifizieren.
Vollständige Längen, Version, Node 2, Boot, Quellenbindung und Metadaten prüfen.

Alle gültigen Formate gemeinsam vor Rastergate bilanzieren: einzigartige
CSI, Rasterkandidaten, Formatausschlüsse/Nenner, Duplikate/Umordnung, Wraps,
Bootwechsel und ungeklärte innere Sequenzpositionen/Übergänge. Spät gefüllte
Positionen berücksichtigen; unbekannte Randverluste nicht erfinden. Kein
Sync-bedingter Zählerreset oder globale Legacy-Subtraktion in eigener Bilanz.

Gesamt-/Sekundenraten, 64/128/192-Verteilung samt Flags, innere/Randlücken
und 3-s-Teilfenster im 1-s-Schritt getrennt berichten. Geprüften Decoder
und dessen konservative Fensterregel unverändert verwenden; nach 5 s
Einschwingen 113 Teilfenster. Annahmekriterium für diesen vorbereiteten
Formatvergleich: mindestens 5 passende 128er/s und 113/113 diagnostische
Fenster bestanden, je mindestens 15 Frames, 2,5 s Abdeckung, 5 Hz,
innere Lücke <1 s, einheitliches passendes Layout. Keine Lockerung nach Sichtung.
Anzahl bestandener Fenster auch bei verfehltem Kriterium als Ergebnis berichten.
Diese RX2-Diagnose ist keine gemeinsame Vier-RX-/Positionsabnahme.

Mit dem historischen Mitschnitt vergleichen: 18,316 gültige/s, 1,925 passende/s,
89,49 % Formatausschlüsse, 9/2206 offene Positionen, 0/113 Fenster.
Keine neue Basismessung oder Rückinstallation. Vergleich alte e09d-App gegen C
ist nicht ausschließlich HT20-A/B; geerbte Diagnostik/Basisunterschiede nennen.

Diagnose- und Transportzählerdeltas getrennt berichten; Vorher/Nachher-GETs
klammern das Capture ein, bilden nicht exakt dessen Fenster ab. Saturation,
Reset, Epochengrenzen und offene Queues berücksichtigen. Queue-/Sendefehler
nicht verschweigen und Livebilanz nicht auf atomare Gleichheit zwingen.
Keine neue Heap-/Stacktelemetrie erfinden: fehlende Laufzeitmargen bleiben
RESOURCE_RUNTIME_MARGIN_OPEN. Physischer Bin-/Masken-/Kalibrierungsbezug und
Socket-/Serverannahme bleiben getrennte offene Gates. Hostfix nicht aktivieren.

5. LIEFERUNG UND ENDE

Privat liefern: Freigaben/Marker, Helfer-/Tool-/Referenzhashes, Vorher-/Nachher-
Originalantworten und Receipts, genau-ein-POST-Beleg oder begründetes SKIP,
Kandidaten-/Boot-/Slot-/Config-/Diagbindung, PCAP/Dropstatistik/Zeitanker,
Paket-/Sequenz-/Fensterbilanz, Vergleich, Originalerhaltung und Abschlussmanifest.
Keine Produktcode-, Konfigurations-, Netz-, Dienst- oder Backupänderung.

Getrennte Urteile:
 Installation: NOT_STARTED / C_CONFIRMED_VALID / INCOMPLETE_OR_FAILED.
 Endlaufzustand: C_CONFIRMED / OLD_FIRMWARE_CONFIRMED / UNKNOWN.
 Capture: NOT_RUN / COMPLETE / INCOMPLETE.
 Formatvergleich: PASS / FAIL / NOT_ASSESSABLE, mit Zahlen und Beweisgrenzen.
 Bei C_CONFIRMED nach unklarem POST dessen Mehrdeutigkeit trotzdem erhalten.

Auch bei Format-FAIL bestätigten Boot/Config-Endzustand berichten. Kein
Nachjustieren, zweiter Upload, weiterer Capture, Restore oder automatischer
Folgeauftrag. Bericht kurz: Ist exakt C VALID? Konfiguration gleich? Aufnahme
vollständig? Absolute Rasterrate/Fenster und getrennte Fehlerbilanz? Läuft RX2
am Ende bestätigt? Genau ein nächster konkreter Schritt. Danach stoppen.
```

<!-- Publication copy: local paths and hardware identifiers were pseudonymized; private original retained. -->
# Agent Auftrag für 120 Sekunden RX2 CSI Transport am Host

Stand: 12. Oktober 2026. Johann beauftragt einen ausschließlich lesenden 120-Sekunden-Mitschnitt am bestehenden Host, ohne Reset oder Flashen. Der Auftrag enthält keine Betriebsunterbrechungs-, Geräte- oder Hostdienst-Schreibfreigabe. Die vorherige RX2-Wartung ist abgeschlossen; ihre Freigaben werden nicht wiederverwendet. Der Koordinator hat keinen Agenten gestartet und keine Hardware angesprochen.

```text
AUFTRAG: RX2-CSI-Transport einmalig 120 Sekunden am bestehenden Host
passiv aufnehmen und getrennt auswerten

Projekt: .

Führe ausschließlich diesen begrenzten Auftrag aus.
Starte keine weiteren Agenten und schreibe keine anderen Chats an.
Ziel: Welche RX2-CSI-Daten erreichen den Host, wie kontinuierlich sind sie,
welche passen zum bestehenden Raster und welche Sequenzlücken bleiben offen?
Keine Reparatur, Installation oder Erweiterung auf RX↔RX durchführen.

1. GRENZEN UND MINIMALER EINSTIEG

Freigegeben: ein passiver Host-Mitschnitt mit 120 Sekunden Messfenster,
lokale passive Metadatenprüfung und anschließende Offlineauswertung.
Keine serielle Portöffnung/USB-Probe, DTR/RTS/BREAK, Reset, Strom-/Kabelwechsel,
ROM/Stub, Flash/OTA/Restore/Provisionierung oder Geräte-Konfigurationsänderung.
TX1, andere RX, Radare und Sicherheitswerte unverändert lassen.
Keine Geräte-HTTP-Abfragen, Pings, Scans, ARP-Probes oder Änderungen an
Interface, Route, DHCP, Firewall, Hostdiensten oder bestehenden Empfängern.

Arbeitsbaum und wirksame AGENTS.md prüfen; vorhandene Änderungen erhalten.
Gezielt lesen, Pfade relativ zum Projekt:
 - private/rx2-maintenance-run-20261012-20261012T180912Z/REPORT.private.md,
   final-verification.private.json, get-receipts.private.json und die benötigten
   gebundenen Originalstatus-/Config-/Netzbelege aus observations/;
 - software/ruview/v2/crates/wifi-densepose-sensing-server/src/
   raw_csi_recording.rs, csi_identity.rs, relevante Ingressstellen in
   runtime_tasks.rs, Rasterregeln in state.rs und position_capture.rs;
 - project_tools/mmwave_freshness_packets.py, tx1_transport_comparison.py,
   tx7b_coverage.py sowie capture_command/Readiness/Cleanup in
   tx1_comparison_capture.py oder tx7b_capture.py; nur geeignete reine Funktionen;
 - bei Bedarf für bestehende Offline-Gapbilanz:
   private/tx1-coordination-20261010/agent-c/run-20261010T162612Z-23bdcf89/
   analyze.py, gap_details.py und test_accounting.py;
 - private/tagespaket-20261012-20261012T161450Z/B-patchreview.private.md
   zum noch offenen Sync-Zählerfehler.

Keine alten Geräte-/Gesamtcollector ausführen und keine großen Belegbäume
erneut hashen. Historische private Ausgangsdaten nicht in Konsole kopieren.

RX2 ist im letzten Endvergleich mit bisheriger App, ota_1 / VALID,
stabilem Boot und acht unveränderten Configfeldern bestätigt. Das ersetzt
keinen aktuellen CSI-Nachweis. Benötigte RX2-IP/Interface, Ziel-IP/UDP-Port,
Bootreferenz und TX-Filter-/Quellenreferenz aus gebundenen Belegen ableiten,
nicht pauschal alte Adressen oder UDP-Port 5005 voraussetzen.
Fehlt eine unabhängige TX-Quellenreferenz, nicht aus dem ersten Paket erfinden;
Transport trotzdem beobachten und die Quellenprüfung als offen ausweisen.

2. PASSIVE AUFNAHME VORBEREITEN

Bestehendes Projektinterface, lokale Zieladresse, Route und aktuelle passive
Nachbarzuordnung prüfen. Keine unbeteiligten alten IP-Aliase als Widerspruch
gegen einen separat korrekt gebundenen RX2-Endpunkt behandeln.
Fehlende Cacheeinträge allein beweisen keinen Geräteausfall; zusätzliche
Bindung aus tatsächlich eintreffenden Paketen soweit möglich nachweisen.
Bei bestätigtem fremdem Gerät/Interface nicht dort weiter aufnehmen.
Zeigt die Referenz auf eine andere als die vorhandene Host-Zieladresse,
den Beobachtungspunkt als unpassend melden, keine Netzanpassung vornehmen.

Capture bevorzugt mit vorhandener passiver PCAP-/tcpdump-Funktion durchführen:
nur bestehendes Interface, enger RX2-UDP-Filter zum konfigurierten Hostziel,
volle Paketlänge, numerische Darstellung, kein Promiscuous-/Monitor-Modus.
Ein BPF-Tap kopiert; keinen zweiten UDP-Socket an den Empfangsport binden,
kein SO_REUSEPORT/Relay und keinen neuen Sensing-Server starten.
record-csi-udp.py ist hierfür ungeeignet: es bindet einen UDP-Port und verwendet
einen vereinfachten Parser. Diesen Recorder nicht als Ersatz starten.

Captureberechtigung vor dem Messfenster im verfügbaren Toolrahmen prüfen.
Falls nötig nur die ausdrücklich beauftragte passive Captureausführung zur
Toolfreigabe vorlegen. Keine BPF-Rechte/Sicherheitsregeln dauerhaft ändern,
keine Pakete installieren, keine Passwörter anfordern oder Schutz umgehen.
Bei fehlender Berechtigung konkret als NOT_RUN berichten, keinen Verkehr erfinden.

Neue private Ablage vor Nutzung auf Ignore UND Nichtversionierung prüfen;
Verzeichnisse 0700, Dateien 0600, keine alten Dateien überschreiben.
Helfer, Filter, Decoder, Referenzen und relevante Toolversionen vor Start binden.
Nur benötigte reine Funktionen für vorhandene Runtime anpassen; nichts installieren.

Gezielte Offlinechecks vor Capture:
 - gültige 128-/64-/192-IQ-Paar-Frames, falsche Länge/Version/Quelle,
   unbekannte Metadaten und separate Sync-/Anomalie-Nachrichten;
 - 100–104 vollständig, 101–103 off-grid: drei Formatausschlüsse, keine
   unbeobachtete Sequenzposition; bei fehlender 102 genau eine offene Position;
 - Duplikat, verspätete Nachlieferung, u32-Wrap und Bootwechsel ohne Mischbilanz;
 - PCAP mit gültigem Header und null Paketen bleibt volle 120 Sekunden aktiv;
   kein Readiness-Timeout allein wegen ausbleibender Daten;
 - abgebrochener PCAP-Tail, Kernel-/Capturedrops, Uhrsprung, Größen-/Zeitende
   und Cleanup; eine unvollständige Aufnahme wird nicht zu einem PASS.

3. GENAU EIN 120-SEKUNDEN-MESSFENSTER

Verbrauchsmarker anlegen; genau einen Captureprozess starten.
Readiness anhand geöffnetem Capturehandle/Listening und gültigem PCAP-Header
belegen, nicht anhand des ersten CSI-Frames. Messdauer monoton steuern.
Ab Readiness 120 Sekunden aufnehmen, danach sauber beenden/flushen;
Prozesscleanup höchstens zehn weitere Sekunden. Tatsächliche Zeiten und
Randabweichungen dokumentieren, nicht auf 120 Sekunden schönrechnen.

Kein Frühabbruch wegen leerem Eingang, Formatabweichungen, Sync, Bootwechsel
oder Sequenzlücken. Beobachten und später auswerten, nicht Geräte reparieren.
Technischer Capture-/Schreibfehler, bestätigter Quellenkonflikt oder Größenlimit
beendet diesen einen Lauf mit INCOMPLETE. Keine zweite Aufnahme als Retry.

PCAP maximal 64 MiB; bei Erreichen stoppen und Unvollständigkeit ausweisen.
Keine Ringrotation, Überschreibung oder stilles Weglassen beschädigter Tails.
Capture-stderr/Exitcode, Paket-/Kernel-Dropstatistik, Start/Ende und rohe
Dateien privat sichern. Nur den eigenen Captureprozess beenden.

PCAP-Unixzeit und monotone Prozesszeit getrennt erhalten; Clock-Anker während
der Aufnahme protokollieren. Keine Offlineverarbeitungszeit als Empfangszeit
oder erfundene monotone Zeit in RawCsiFrame.v2 eintragen. Bei Uhrsprüngen
zeitliche Lücken/Qualitätsfenster als unsicher markieren statt weiterzurechnen.

Falls der bestehende Hostserver bereits läuft, dürfen höchstens zwei rein
lokale lesende Nodes-Statusabfragen vor/nach dem Fenster ergänzen, sofern der
Handler nachweislich keine Geräteabfragen auslöst. Keine Remoteabfragen,
keine Dienststarts/-neustarts und kein Aktivieren einer Serveraufnahme.
PCAP-Ergebnis bleibt unabhängig von fehlendem API-Zusatz vollständig auswertbar.

4. TRANSPORT, QUELLE, RASTER UND LÜCKEN GETRENNT BILANZIEREN

Alle gefilterten UDP-Nachrichten klassifizieren; gültige Sync-/Anomalieframes
nicht als beschädigte CSI zählen. Aktuellen Decodervertrag verwenden:
vollständige Längen, Protokollversion, Node 2, RX-Boot, Quellenbindung und Layout.
RX2 als UDP-Sender nicht mit dem physischen Sender der CSI-Messung verwechseln.
TX-Boot/Anwendungssequenz/Slot nur bei tatsächlich verifizierter Zuordnung.

Pro Boot-/Quellen-/Layoutepoche ausweisen:
 - UDP-Beobachtungen, ungültige/andere Nachrichten, strukturell gültige CSI;
 - eindeutige RX2-/Boot-/TX-Quellenbindung oder konkrete fehlende Belege;
 - gültige einzigartige CSI-Transportbeobachtungen, Duplikate/Umordnung,
   Layout-/Längenverteilung und Frames passend/nicht passend zum bestehenden Raster;
 - Formatausschlüsse mit Zahl und gültigem gemeinsamen Nenner;
 - unbeobachtete Sequenzpositionen im eindeutig beobachteten inneren Intervall,
   verspätet doch beobachtete Positionen, Wraps und deren gültige Nenner.

Alle gültigen Formate vor dem Rastergate berücksichtigen. Off-grid-Frames
dürfen keine vermeintlichen Empfangslücken erzeugen. Keine globale Subtraktion
skipped_grid_frames minus inferred_lost_frames. Randverluste vor erster/nach
letzter Sequenz sind ohne Senderbeleg unbekannt. Große/mehrdeutige Sprünge und
Bootwechsel nicht überbrücken. Verspätete Nachlieferungen ausdrücklich ausweisen;
retrospektiv beobachtete Positionen nicht als unempfangen bezeichnen.
Abweichung zur In-order-Zulassung des Servers gegebenenfalls gesondert erklären.

Zähler/Nenner/Scope nennen; bei fehlender Basis Prozent unbekannt, nicht 0 %.
Ungeklärte Sequenzlücken sind kein Beweis von WLAN-/TX-Verlusten und können
Capture-/Hostblindstellen enthalten. Der bekannte verspätete Sync darf die
eigene bootgebundene Auswertung nicht zurücksetzen. Den Produktpatch nicht fixen
oder deployen; dessen API-Zähler sind kein alleiniger Messnachweis.

Gesamtrate und je Sekunde getrennt für gültige CSI und Rasterkandidaten berichten;
größte innere und Randlücken sowie Datenabdeckung ausweisen. RX2-Teilfenster
diagnostisch an den vorhandenen 3-s-/1-s-Schritt-/Mindestdatenregeln prüfen,
soweit Zeit-/Layout-/Quellenlage dies erlaubt. Keine neue Kalibrierung und keine
Lockerung des 128er- oder Vier-RX-Vertrags. Kein gemeinsamer Vier-RX-PASS.

PCAP bedeutet Beobachtung am Host-Capturepunkt, nicht nachgewiesene Zustellung
an den UDP-Socket, Serverannahme oder Verarbeitung. Eine aus Quellcode abgeleitete
Rasterkompatibilität entsprechend benennen. Aktuellen lokalen Serverstatus nur
bei gebundener Runtime/gleichem Zeitintervall vergleichen; alte kumulative Werte
nicht als Zähler dieses Messfensters ausgeben. Quellen- und Zeitunsicherheit erhalten.

5. LIEFERUNG UND ENDE

Privat liefern: Capture-/Interface-/Quellen-/Bootbindung, Referenzen, Roh-PCAP,
Capturelog/Dropstatistik, Zeiten, Decoder-/Helfer-/Testhashes, Bilanz je Epoche,
Sekundenübersicht, zulässige RX2-Teilfenster und Abschlussmanifest.
Keine Tokens, privaten Rohwerte oder vollständigen UDP-Payloads im Bericht.

Urteil:
 RX2_CSI_BOUND_TRANSPORT_OBSERVED
 RX2_CSI_OBSERVED_BINDING_INCOMPLETE
 RX2_CSI_NOT_OBSERVED bei vollständiger Aufnahme ohne validierbare CSI
 RX2_CSI_CAPTURE_INCOMPLETE
 RX2_CSI_CAPTURE_NOT_RUN

Separat: Rasterkandidaten kontinuierlich / zeitweise / keine / unbestimmt;
tatsächliche Serverannahme belegt oder offen. Weder HTTP-Rückkehr noch eine
einzelne passende Framegröße beweist CSI-Qualität oder Positionsleistung.

Abschluss kurz: Aufnahme vollständig? Aktuelle RX2-/Boot-/Quellenbindung?
Gültige CSI versus Rasterkandidaten? Formatausschlüsse versus ungeklärte Lücken?
Artefaktpfad, Beweisgrenzen und genau ein nächster konkreter Schritt.

Keine Produktcode-/Firmware-/Gitänderungen, Installation oder Downloads.
Nach diesem einen Mitschnitt und Bericht stoppen. Keine automatische Fortsetzung.
```

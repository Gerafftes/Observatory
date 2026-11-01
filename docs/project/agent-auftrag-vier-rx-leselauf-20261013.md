<!-- Publication copy: local paths and hardware identifiers were pseudonymized; private original retained. -->
# Agent Auftrag für 30 Minuten TX1 zu vier RX

**Tag 2 / Vorbereitung von G1 am 14. Oktober. Schwierigkeit: 3/5.** RX2 läuft im zuletzt geprüften Abschluss mit Kandidat C auf ota_0 / VALID. RX1, RX3 und RX4 sind laut Johann ebenfalls mit Strom versorgt. Der Auftrag bündelt Offline-Absicherung des Capture-Abschlusses und einen gemeinsamen Leselauf. Kein Agent wurde gestartet.

Geplante Dauer: 30 Minuten Messfenster, etwa 35–45 Minuten für den aktiven Lese-/Captureabschnitt einschließlich Vorher-/Nachherprüfung. Lokale Vorbereitung und Offlineauswertung kommen hinzu. Johann braucht keine Messwege abzulaufen; Gerätepositionen und Aufbau bleiben bestehen.

Den folgenden Block vollständig an den ausführenden Agenten übergeben:

```text
AUFTRAG: Capture-Ende offline absichern und TX1→RX1–RX4 30 Minuten lesen

Projekt: .

Führe diesen begrenzten Auftrag vollständig aus: lokale Helferkorrektur,
gezielte Offlinechecks, genau ein gemeinsamer passiver 30-Minuten-Lauf und
Auswertung. Keine weiteren Agenten starten oder andere Chats anschreiben.
Nach dem Bericht stoppen. Keine Geräte-/Produktreparatur aus Befunden ableiten.

ZIEL UND BELEGTER AUSGANG

Das ist der Fehler:
Der letzte RX2-C-Mitschnitt liefert ausreichend passende CSI, scheitert aber
formal am letzten Fenster: 112/113 statt 113/113, nur 2,100084 s Abdeckung.
Letzte CSI-Ankunft 878,896 ms vor Ende; SIGINT unmittelbar nach der Endmarke.
Capturepufferung ist eine plausible, unbewiesene Erklärung. Die tatsächliche
gemeinsame Datenbasis aller vier RX ist noch nicht aktuell qualifiziert.

Das erwarte ich:
Messfenster und Capture-Vor-/Nachlauf sauber trennen und den Endabschluss
mit Fixtures absichern. Danach alle vier TX1→RX-Strecken gleichzeitig
beobachten: Identität, Firmware-/Boot-/Quellenbindung, Formate, absolute
128er-Rate, Lücken/Fehler und gemeinsame gültige 3-s-Fenster über 30 Minuten.
Jeden RX getrennt ausweisen; keine Drei-RX-Ersatzabnahme.

Belegter RX2-Stand:
 C installiert/ota_0/VALID, stabiler neuer Boot bis Endstatus, acht gleiche
 Configfelder. 3.825 passende CSI in 120,010 s = 31,872/s, 0 Formatausschlüsse,
 13 ungeklärte Positionen/3.837 Übergänge. Kein bewiesener Funkverlust.
 Runtime-Image-Digest C:
 e674e9fa055c05037c95a9df7c99dd2097b2c3544016ea99794888721a6a191a
 Diagnose-Source-SHA C:
 5dd96c6299d99cfa86ad3dac04c5d08d9747657a6e55134d0ebae4f7b5ccd4bc
 C bleibt unverändert auf RX2. Nicht voraussetzen, dass andere RX C verwenden.
 Der Host-Sync-Fix ist nur offline gebaut; nicht aktivieren.
 Dieser Auftrag prüft TX1→RX. RX↔RX und Radar bleiben eigene Arbeitspakete.

1. UMFANG, GRENZEN UND GEZIELTE EINGÄNGE

Erlaubt im menschlich beauftragten Leselauf:
- Lokale neue private Helfer/Tests/Analyse; vorhandene Werkzeuge verwenden.
- Höchstens 24 RX-Geräte-GETs insgesamt an unabhängig zugeordneten RX1–RX4:
  ausschließlich /ota/status, authentisiertes /config und bei RX2
  /config?diag=1. GET /ota/status dient nur der Inventur; keine OTA-Schreibaktion.
- Optional höchstens zwei lokale GET /api/v1/nodes vor/nach dem Messfenster,
  nur bei bereits laufender, gebundener Runtime und nachweislich ohne Fan-out.
- Genau ein passiver Captureprozess: 2 s Vorlauf, 1.800 s Messfenster,
  2 s Nachlauf, anschließend höchstens 10 s Cleanup.

Aktives Beobachtungsbudget maximal 45 Minuten ab erstem Geräte-GET bis
letzter Geräte-/Captureaktion. Offlinevorbereitung vorher und Auswertung
danach zählen nicht dazu. Das ist KEINE Freigabe einer Betriebsunterbrechung.

Verboten: Flash/OTA-Upload/Restore, POST/PUT/PATCH/DELETE, Reset, ROM/Stub,
serielle Portöffnung, DTR/RTS/BREAK, Strom-/Kabel-/Positionswechsel,
Provisionieren, NVS-/Security-/Channel-/Rate-/Filter-/Gain-/Self-Ping-Änderungen.
Keine TX1-/Radarabfragen, Pings/Scans/ARP-Probes, Hostnetz-/Alias-/Routeränderung,
Dienststart/-neustart, Hostpatchaktivierung, Kalibrierung oder Modelltraining.
Kein zweiter UDP-Empfänger, Relay, SO_REUSEPORT oder neue Serveraufnahme.
Keine Downloads/Installation, Git-Eingriffe oder ProjectAtlas-Verwendung.

Arbeitsbaum und wirksame AGENTS.md zuerst prüfen. Vorhandene Änderungen,
Altberichte, Builds, Backups und Messdaten erhalten. Neue private Ablage,
z.B. private/tx1-vier-rx-30min-20261013-<UTC>/; Ignore UND Nichtversionierung
prüfen. Ordner 0700, private Dateien 0600. Keine Tokens, Geräte-/NVS-Werte,
Rohantworten oder Payloads öffentlich oder in Tool-Ausgaben kopieren.
PCAP maximal 512 MiB; mindestens 2 GiB freien privaten Ablageplatz prüfen.
Keine Rotation/Überschreibung. Bei Größen-/Schreibfehler INCOMPLETE, kein Retry.

Minimal lesen, Pfade relativ zum Projekt:
 docs/project/rx2-c-installationsreview-20261013.md;
 private/rx2-install-csi-new-20261013-20261013T143100Z/
   REPORT.private.md, capture_once.py, pcap_offline.py, capture-proof.private.json,
   capture-config.private.json, reference.private.json und benötigte Receipts.
   setup-/TX-Quellenreferenzen aus capture-config gezielt nachverfolgen.
 Für historische RX1/3/4-Zuordnungen gezielt vorhandene Inventur-/Gemeinschafts-
 belege nutzen, Einstieg private/tx1-transport-comparison-20261004/ und
 private/tx1-coordination-20261010/agent-c/run-20261010T162612Z-23bdcf89/.
 Keine gesamte private Historie oder komplette Buildbäume neu prüfen.
 project_tools/tx7b_coverage.py für reine Zeit-/PCAP-Abdeckungsfunktionen;
 project_tools/mmwave_freshness_packets.py für geeignete reine Paketfunktionen.
 software/ruview/v2/crates/wifi-densepose-sensing-server/src/
   raw_csi_recording.rs, position_capture.rs, csi_identity.rs und nötige
   Raster-/Ingressstellen; system_routes.rs nur bei optionalem Nodesvergleich.

Keine alten Gesamtcollector starten: sie fragen TX/Radare/weitere APIs ab.
Manche project_tools nutzen lazy-import-Syntax; nur benötigte Funktionen in
vorhandener Pythonruntime verwenden/anpassen, nichts nachinstallieren.

2. CAPTURE-ENDE UND MULTI-RX-AUSWERTUNG OFFLINE ABSICHERN

Nur neue Helferkopie ändern, keine Originale oder Produktdateien.
Ein 120-s-/Node-2-Helfer ist nicht unverändert für 30 Minuten/vier RX geeignet.
Parser/Wireprüfung wiederverwenden; Dauer, Quellenmenge, Ledger und Fenster
gezielt parametrieren. Kein neues Messframework oder unnötige Abhängigkeit.

Capture-Lebenszyklus:
 Handle/Listening plus gültiger PCAP-Header beweisen Readiness, kein Paket nötig.
 Danach 2 s Vorlauf, Startanker setzen, volle 1.800 s messen, Endanker setzen.
 Capture danach mindestens weitere 2 s laufen lassen, um gepufferte letzte
 Pakete abzuholen; erst dann sauber beenden. Cleanup höchstens weitere 10 s.
 Actual-/Sollzeiten und Capture-Lebenszyklus getrennt dokumentieren.
 -U betrifft Dateiausgabe, kein Beweis geleerter Eingangspuffer. Zusätzliche
 Immediate-/Timeoutoptionen nur verwenden, wenn lokal unterstützt und geprüft;
 kein BPF-Handle oder Probe-Capture für Offlinechecks öffnen.

Messgutschrift ausschließlich anhand ursprünglicher PCAP-Ankunftszeit innerhalb
[Startanker, Endanker). Vor-/Nachlaufpakete erhalten keine Gutschrift. Während
Nachlauf ausgelieferte Pakete mit Zeitstempel innerhalb des Messfensters zählen.
Keine erfundenen Empfangszeiten, nachgefüllten Frames oder veränderten Regeln.
PCAP-/Unixzeit und monotone Prozessanker getrennt erhalten; Uhrankerspanne/-
schritte prüfen, bisherige 50-ms-Zeitqualitätsschwelle beibehalten.
Nachlauf separat ausweisen: Gibt es echte Records mit ursprünglichem
Zeitstempel nach der Endmarke? Das belegt Fortschritt des Erfassungspfads,
garantiert aber keine verlustfreie Zustellung. Fehlen sie, keine vollständige
Pufferleerung behaupten; END_BOUNDARY_OBSERVABILITY_OPEN berichten.

Mit lokalem Bytewriter/Fakeprozess testen, keine echte Aufnahme:
- Verzögerte letzte Batch: innerhalb des Fensters gestempelt, erst nach
  Endanker geschrieben; neuer Helfer erhält sie, sofortiger alter Stop verliert sie.
- Pakete nach Endanker zählen nicht; PCAP mit Header und null Paketen bleibt
  volle 1.800 s im Fakeclocklauf gültig. Kein datenabhängiger Frühabbruch.
- Vier RX mit gleichen Sequenznummern bleiben getrennt; fehlender RX,
  64/128/192-Mix, falsche Quelle/Node/Länge/Version, Duplikate, späte
  Nachlieferung, u32-Wrap und Bootwechsel korrekt klassifizieren.
- Teilfenster an Anfang/Ende, verspätete Dateischreibung, Dateilimit, kaputter
  Tail, Uhrsprung und Prozess-/Cleanupfehler. Kein Entfernen des letzten Fensters.
- Initiale Statuszuordnung ohne Cache: kein Token vor passender MAC/Antwort;
  falsche MAC/fremde Antwort/Timeout stoppt weitere Zugriffe auf diesen Endpunkt.

Synthetische und alte reale Daten getrennt halten. Alten RX2-Test nicht
nachträglich zu PASS erklären. Testbefehle/Ergebnisse und Helferhashes versiegeln.
BPF-Filter ggf. ausschließlich mit synthetischer Datei/offline prüfen.
Für reale 30-Minuten-Daten lineare/gleitende Auswertung und begrenzten Speicher
verwenden; nicht für jedes Fenster erneut die vollständige PCAP scannen.

3. GERÄTE UND HOST FRISCH BINDEN, EINMAL AUFNEHMEN

Vor Livebeginn vorhandene Host-Zieladressen, aktive Projektverbindung und
direkte RX-Routen lokal lesen. Bestehende mehrere IPv4-Adressen/Aliase auf
einem Interface sind kein Fehler, sofern konkretes Ziel und Route passen.
Nie Aliase entfernen, Interface/DHCP ändern oder einen neuen Port binden.
Pro RX eigenständig Node-ID, erwartete physische MAC/Endpunkt, Boot, Image,
Config, TX-Filterreferenz und erwartetes Raster aus unabhängigen Belegen ableiten.
Alte Adressen sind Kandidaten, keine aktuellen Zustandsbeweise. Fehlende MAC-
Referenz nicht aus erstem neuen Paket als unabhängig bestätigt erfinden.

Geplanter Requestverbrauch: je RX Vorherstatus, Config, zweiter Vorherstatus,
Nachherstatus und Config = 20 GETs; RX2-Diagnose vor/nach = 2 weitere.
Höchstens zwei Reserve-Status-GETs an bereits zugeordnete Endpunkte; Gesamt 24.
Connect maximal 3 s, Request maximal 7 s, Antwort maximal 16 KiB.
Keine unbegrenzten Retries, wechselnden IP-Versuche oder Endpunktsuche.

Bei fehlendem Cache ist der erste Vorherstatus genau ein unauthentisierter
GET /ota/status am einzigen historisch bekannten RX-Endpunkt. Normale
Adressauflösung dieser TCP-Verbindung zulässig; kein Token/Cookie/Proxy/
Redirect/Retry. Danach erwartete MAC/Interface und Antwortnode bestätigen.
Bei vorher schon widersprüchlicher MAC dort keine Anfrage; bei fehlerhafter
Antwort oder fehlender Zuordnung keine authentisierte Folgeanfrage.

Danach /config nur authentisiert lesen und Vorherboot mit zweitem Status
prüfen. Pro RX Werte nur vergleichen, nicht angleichen. RX2 muss vollen C-
Digest/VALID und im Diagnoseexport C-Source bestätigen. Andere RX dürfen
ihre bisherigen Firmware-/Layoutstände behalten; Abweichungen sind Befunde.
Statusboot Hexstring vs Diagnoseboot uint64 korrekt numerisch vergleichen.
Auth ausschließlich über private stdin-/Configübergabe, nicht argv/Logs.

Unklare Identität oder ungeeignete Daten eines einzelnen RX nicht durch
Umprogrammieren reparieren und nicht als erfolgreiche Vier-RX-Basis ausgeben.
Die sinnvoll abgrenzbaren autorisierten RX dürfen dennoch vollständig passiv
beobachtet werden; unbestätigte/fehlende RX bleiben sichtbar und verhindern
das Vier-RX-PASS. Kein fremdes Gerät ansprechen oder als RX-Ersatz verwenden.
Fehlender Hostpfad, physischer Widerspruch im gewählten Captureziel oder
fehlende Berechtigung verhindern den Start dieses Capturepfads.

Optional bestehende lokale Runtime vor/nach über Prozess/Executablehash/
Argumente/Startzeit binden. GET /api/v1/nodes höchstens zweimal nur lokal,
wenn Handler tatsächlich gespeicherten Zustand ohne Geräte-Fan-out liest.
Fehlender Dienst/alte API ist kein Grund für Start, Update oder Neustart.
Bekannten Sync-Zählerfehler einer alten geladenen Runtime berücksichtigen.

Genau einen Captureprozess mit neuem Einmalmarker starten: normales Interface,
-p, volle Paketlänge, numerisch, enger Filter aus den erlaubten RX-Quellen und
ihren tatsächlich konfigurierten lokalen Ziel-IP/UDP-Port-Tupeln. Nicht allein
Node 2 oder dessen Zielport voraussetzen. Unbeteiligten Radar-/Netzverkehr
nicht in den Filter aufnehmen. Filter privat speichern und binden.

30 Minuten Messfenster ohne Geräte-/API-Abfragen; lokale Zeit-/Dateigrößen-
und passive Bindungsprüfungen erlaubt. Ein fehlender Datenstrom, Formatwechsel
oder RX-Bootwechsel ist ein Messbefund, kein Reparatur- oder Retryauftrag.
Bei technischer/physischer Schutzverletzung nur eigenen Prozess beenden,
INCOMPLETE berichten. Keine zweite Aufnahme. Vor-/Nachlauf wie oben.
Danach begrenzte Nachherstatus-/Config-/RX2-Diagnoseabfragen im Gesamtbudget.

4. GETRENNTE RX-BILANZEN UND GEMEINSAME FENSTER

Alle UDP-Nachrichten klassifizieren: CSI, Sync, Feature-State, Mesh/Anomalien
und unbekannte/ungültige Formen. Netzwerkquelle, Node, Boot, Versions-/Längen-
und TX-Quellenbindung prüfen; RX als UDP-Sender nicht mit CSI-Funksender
verwechseln. Keine exakte TX-Appsequenz/Boot/Slotzuordnung ohne Payloadbeweis.

Pro RX und zusammenhängender Boot-/Quellenepoche: alle gültigen einzigartigen
CSI vor Rastergate bilanzieren, passende 128er und Formatausschlüsse mit
Nennern, Layout-/Flagverteilung, Duplikate/Umordnung/Wrapping und ungeklärte
innere Positionen ausweisen. Später beobachtete Positionen schließen Lücken;
Layoutwechsel allein startet kein neues Sequenzledger. Boot-/Quellwechsel
und mehrdeutige Sprünge nicht überbrücken. Randverluste bleiben unbekannt.
Abweichung zur In-order-Zulassung des Servers gesondert erklären.

Erwartetes Raster aus versiegeltem Setup je RX verwenden; RX2s bestätigte
C-Profilbindung gesondert ergänzen, nicht das alte Setup überschreiben.
Kein dominantes Format automatisch als neues Raster wählen, keine Kürzung
192→128, keine Masken-/Kalibrierungsmischung trotz gleicher Paarzahl.
Unbekannte Bindung/Metadaten als unbekannt erhalten. Keine globale Subtraktion
von Legacy-Lücken und Formatausschlüssen oder behauptete WLANverlustursache.

Je Sekunde/Minute gültige Rate, Rasterrate, Datenabdeckung und innere/Randlücken
ausweisen. Nur dauerhaft erreichbare oder drei frische RX kaschieren keinen
fehlenden vierten RX. Diagnose-/API-Deltas klammern die Messung ein, sind
nicht exakt derselbe Zeitraum; Bootwechsel/Sättigung/Rückgänge markieren.
Kein SEND_OK/PCAP-Quotient als pauschale Funkverlustquote, keine atomare
Bilanzgleichheit offener Livepipelines. Keine Heap-/Stacktelemetrie erfinden.

Vorab festgelegter Fenstervertrag:
 Für Fenstergutschrift nur einzigartige, gebundene In-order-Frames wie im
 bisherigen Decoder verwenden. Späte Nachlieferungen dürfen die retrospektive
 Lückenbilanz schließen, werden dadurch nicht zu Serverannahmen umetikettiert.
 5 s Einschwingen, 3-s-Fenster im 1-s-Schritt; je RX mindestens 15 passende
 Frames, mindestens 2,5 s Abdeckung, mindestens 5 Hz, innere Lücke <1 s,
 einheitliches passendes Layout und gültige Boot-/Quellen-/Zeitbindung.
 Alle vier RX müssen gleichzeitig erfüllen; tatsächliche gemeinsame
 Überdeckung mindestens 2,5 s. Bestehende Zeitregeln nicht lockern.

Bei gültiger Zeitbasis entstehen 1.793 gemeinsame Fenster nach Einschwingen
(Startsekunden 5..1797). Ziel aus dem Gesamtplan: mindestens 90 %, also
mindestens 1.614/1.793 qualifizierte Fenster. Fehlende RX, Neustarts und
Lücken nicht nachträglich aus dem Nenner entfernen. Prozent bei ungesicherter
Zeit-/Identitätsbasis nicht als gemessenen Qualitätswert ausgeben.

RX2 zusätzlich im ERSTEN und LETZTEN 120-s-Teil des selben Messfensters
mit dem alten strengen 113/113-Vertrag auswerten. Kein zweiter Capture und
keine künstliche Verlängerung dieser Teilintervalle. Das bestätigt oder
widerlegt die Endkantenverbesserung, ohne den alten FAIL umzudeklarieren.

Transport-/Fensterleistung ist noch keine vollständige G1-/Trackingabnahme:
physischen LTF-/Bin-/Maskenbezug, Serverannahme/Verarbeitung, Ressourcen und
noch fehlende Pflichtnachweise separat offen führen. Ein optionaler gebundener
Nodesvergleich kann konkrete Serverannahme belegen; keine Zeit-/Paketgleichheit
oder jede einzelne Zustellung daraus erfinden. RX↔RX wurde hier nicht getestet.
TX1-Fehlerhistorie wird in diesem Lauf nicht aktiv gelesen; daraus keinen
neuen vollständigen TX-/System-Fehlerfreiheitsnachweis ableiten.

5. LIEFERUNG UND ENDE

Privat liefern: Helferkorrektur/Tests, neue Referenzen, Geräte-/Runtimebindung,
Vorher-/Nachheroriginale mit Receipts, genau-ein-Capture-Proof, Zeitanker,
Original-PCAP inkl. Vor-/Nachlauf, saubere Auswerteintervallgrenzen, getrennte
RX-/Epochen-/Sekunden-/Minuten-/Fensterbilanzen und Abschlussmanifest.
Nur Header-/Identitätsmetadaten zusätzlich exportieren, keine unnötigen
Payloadduplikate. Originale/dirty Produktdateien unverändert nachweisen.

Getrennte Urteile:
 Capture: NOT_RUN / COMPLETE / INCOMPLETE;
 je RX: BOUND_RASTER_USABLE / RASTER_INSUFFICIENT / BINDING_INCOMPLETE /
        NO_VALID_CSI_OBSERVED;
 gemeinsame Fensterbasis: PASS / FAIL / NOT_ASSESSABLE;
 gesamte G1-Abnahme: erfüllt oder konkrete noch offene Pflichtnachweise.
 Keine Server-/Ressourcen-/Positionsabnahme aus PCAP allein ableiten.

Abschluss knapp: Läuft RX2 weiter mit C/VALID? Welche Firmware/Boots/Layouts
haben RX1/3/4? Absolute Rasterraten und gemeinsame Fensterquote? Endkante
beobachtbar oder weiter unklar? Engster belegter Engpass und genau ein
nächster konkreter Schritt. Kein Flash-/Reset-/Configfix, weitere Aufnahme,
Hintergrundmonitoring oder automatische Folgearbeit. Danach stoppen.
```

<!-- Publication copy: local paths and hardware identifiers were pseudonymized; private original retained. -->
# Offlineauftrag: zwei RX-Auswertefehler und Erfassungsexporte für RX/Radar

Projekt: `.`

Führe diesen begrenzten Auftrag vollständig offline aus: korrigierte Auswertung, implementierte minimale Erfassungsexporte, durchgehende Dateiintegration, notwendige Regressionen und lokale Builds liefern. Starte keine weiteren Agenten, schreibe keine anderen Chats an und lege keine Automation an. Nach Bericht und Übergabe zum unabhängigen Koordinatorreview stoppen. Kein automatischer Hardwarebeginn.

## Ziel und Ausgangslage

**Das ist der Fehler:**

Paket 1 wurde unabhängig geprüft. In der neuen RX-Dateiauswertung sind zwei konkrete Fehler bestätigt:

1. Nach einer zulässigen 128→64→128-Rückkehr werden wieder passende Frames dauerhaft ausgeschlossen. Gegenbeleg: 2.274 physisch gültige passende Rückkehrframes ohne Gutschrift, nur 93/473 Basisfenster. Die bisherige Regression erwartet fälschlich FAIL für diesen Fall.
2. TX1- und Peerframes werden gegen dieselbe Empfängerreferenz für `grid` und `physical_key` geprüft. Ein 128er-TX1-Link und ein gültiger 64er-LLTF-Peerlink am selben RX sind nicht getrennt deklarierbar. Gegenbeleg: je Richtung 1.920 gültige Peertransportframes vollständig ausgeschlossen.

Die Radarberechnung und historische Vier-RX-Regression bestehen. Ihre Erfassungsübergabe nennt aber fehlende dauerhaft gespeicherte Messgrenzen/Clockbindungen, Recorderoriginale und vollständige Paket-/Hostzeitreihen. Die existierenden Firmware-/Recorderkorrekturen bleiben geschlossen; Originalkandidaten und Dirty-Arbeitsbaum bleiben erhalten.

**Das erwarte ich:**

Beide Gegenfälle sind erhalten und korrigiert regressiert. Die benötigten RX-/Radar-Dateibelege entstehen aus tatsächlich verdrahteten Produzenten in neuen privaten Adapter-/Hostkopien. Ein vollständiger Offlineprobeablauf führt synthetische Pakete durch diese Produzenten, schließt deren Dateien und wertet genau diese Outputs ohne manuelle Erfolgsfelder aus. Die Erfassungsübergabe unterscheidet umgesetzte Belege, erst im Hardwareauftrag frisch zu bindende Werte und weiterhin technisch unbekannte Firmware-/Ressourcenbeobachtungen. Kein nur erneut beschriebenes Exportkonzept als abgeschlossene Umsetzung ausgeben.

## Relevante Eingänge

Zuerst Arbeitsbaum und wirksame `AGENTS.md` prüfen. Danach diese Pfade gezielt lesen, keine allgemeine Repo-/Chat-/Backup-/Vendorinventur:

- Review `./docs/project/paket1-auswertung-review-20261015.md`.
- Neue Auswertebasis `./private/abendlaeufe-auswertung-offline-20261015T120658Z/`: Bericht, Erfassungsübergabe, Eingangsvertrag, Manifeste; danach `scripts/wifi.py`, `radar.py`, `common.py`, `evaluate.py`, Guards, relevante Tests und Fixtures. `host/.../offline_export.rs` ist der vorhandene reine Rust-Dateieinstieg. Historische Helfer erhalten.
- Erhaltene Gegenbelege `./private/paket1-auswertung-koordinatorreview-20261015/`: `REVIEW.private.json`, `counterexamples.private.json`, beide Ergebnisordner und `peer-lltf-valid/input.private.json` mit 23 gebundenen Dateien. Der genaue unveränderte `layout-return`-Input steht im Reviewreceipt; keine eigene Suche nach dem neuesten Fixture nötig.
- Adapterbasis `./private/recordervertrag-korrektur-offline-20261015T083911Z/`: `scripts/real_adapter.py`, Sessionkern, zugehörige Kern-/Adapter-/Recorderprüfungen, Bindungen und gesperrter Hardwareplan. `frames_written`-Korrektur erhalten.
- Produktionsfähige private Hostbasis `./private/offline-tagespaket-20261015T061442Z/host/crates/wifi-densepose-sensing-server/src/`: `runtime_tasks.rs`, `main.rs`, `state.rs`, `server_clock.rs`, `recording_routes.rs`, `raw_csi_recording.rs`, `peer_csi.rs`, `mmwave_calibration.rs`, `mmwave_routes.rs` und direkt nötige Typen/Tests. Der Auswerteexporter und der spätere normale Host sind verschiedene Einstiege; beide ausdrücklich binden, keine Verwechslung der Binarys.
- Gesperrte Paarplanung in der Adapterbasis und Radarplanung `./private/offline-tagespaket-20261015T061442Z/RADAR-G2-HARDWAREPLAN.NICHT-AUSFUEHREN.private.md`. Bereits eingefrorene Dauer, Punkte, Nenner und Schwellen erhalten.

## Arbeitsweise und harte Grenzen

Neue private Ablage, z. B. `./private/rx-radar-export-korrektur-offline-20261015-<UTC>/`. Vor Neuanlage wirksamen Ignore und Nichtversionierung prüfen, Verzeichnisse 0700/Dateien 0600; nur notwendige eigene ausführbare Kopien 0700. Vorherquellen, Gegenbelege und Ursprungslieferungen unverändert erhalten. Referenzierte unveränderte Kandidaten nach Hash binden; keine unnötigen Cachekopien.

**Hardware-/Liveaktionen = 0.** Kein serieller/USB-/ROM-/Stub-/Reset-/Flash-/OTA-/Restore-/Provisionierungszugriff. Kein echtes HTTP/curl/tcpdump, keine Projekt-API, ARP-/WLAN-/Routen-/Netz- oder produktive Prozess-/Socketinventur. Keine bestehenden Dienste starten/stoppen/ersetzen, keine Credentials lesen. Keine Geräte-/NVS-/Security-/CAD-/Transform-/Kalibrierungs- oder Hostnetzänderung. Keine Firmwareänderung, SDKbuilds oder Geräteinstallation in diesem Auftrag.

Erlaubt sind neue private Auswerte-/Adapter-/Hostquellen, notwendige lokale Dateiexporte und Offlineprüfprogramme mit synthetischen Eingängen. Keine produktiven Arbeitsbaumdateien überschreiben. Kein Git-Reset/Clean/Staging/Commit/Push, keine Plan-/Tracker-/Tageshäkchenänderung. Keine Downloads, Installationen oder neuen Abhängigkeiten. Cargo `--locked --offline`, lokale Builds und Tests unter Prozess-Netzverbot. Reale Netz-/Geräte-/Credential-/Produktivprozesswege vor Testdispatch sperren. Eigene gebundene Datei-Testprogramme und Compiler sind zulässige lokale Unterprozesse; kein Serverstart als Fixture.

Live-CLI und native Geräte-/Hostadapter bleiben gesperrt. Alte Freigaben gelten nicht für diesen Auftrag. Auch nach PASS keine Livefortsetzung.

## A – beide RX-Auswertefehler korrigieren

### Zulässige Layout-Rückkehr

Die globale Pensionierung eines verlassenen Layouts in `wifi.py` entfernen beziehungsweise durch die bereits geprüfte Trennung von Identität und transienter Form ersetzen. Boot-/Quellen-/Verarbeitungskonfiguration, tatsächliche Physicalmaske und bestehende Frischeprüfungen weiter beachten. Unterschiedliche Formen nicht in einem Featurefenster vermischen; lokale Übergangsfenster bleiben begrenzt abgewiesen. Danach muss ein wieder zum eigenen vorab deklarierten Vertrag passendes Layout erneut geeignet sein können. Fremde alte Boots/Quellen dürfen dadurch nicht wieder zugelassen werden.

Den unveränderten Vorhergegenbeleg erneut reproduzieren. Auf der neuen Basis nicht bloß „kein Fehler“ testen: Zahl wieder geeigneter Frames sowie Erholung mehrerer vollständig nach der Rückkehr liegender Fenster explizit prüfen. Den bisherigen Test mit erwarteter dauerhafter Ablehnung fachlich korrigieren; seine negative Identitäts-/Grenzabdeckung erhalten.

### Verträge pro gerichteter Strecke

Vier TX1-Basislinks und RX1→RX4/RX4→RX1 erhalten explizite getrennte Format-/Physicalreferenzen. Für einen RX dürfen z. B. TX1-HT128 und Peer-LLTF64 gleichzeitig korrekt deklariert sein. Das ist keine pauschale Zulassung beliebiger Peerformen: Node-/MAC-/Quellen-/PHY-/LTF-/Processingbindung und gültige Maske je Strecke vor Datenansicht festlegen. Fehlende, unbekannte, widersprüchliche oder vertauschte Referenzen klar abweisen.

Eingangsvertrag, Vorlagen, Normalisierung und Berichte konsistent aktualisieren. Referenzen aus überprüften Konfigurationen/Parserdefinitionen ableiten, nicht nachträglich aus den jeweils günstigsten Messpaketen wählen. Gegenbeleg mit gemischten 128er-TX1-/64er-Peerdaten vollständig durch den aktuellen Rustparser und die reale Datei-CLI führen. Beide Peerlinks behalten jeweils ≥1.920 Pakete, ≥2 Hz und ≥858/953 geeignete Fenster; TX1-Vertrag unverändert.

## B – RX-Adapter: tatsächliche Dateien und Clockbelege exportieren

In einer neuen Kopie des geprüften Adapters kleine zusätzliche Exporte implementieren, ohne neue Requests, Polls, Captures, Retries oder Hostwechsel:

- Tatsächlich verwendete lokale Messgrenzen separat speichern: Messstart, Messende, Vor-/Nachlauf sowie tatsächliche Erfassungsgrenzen. Ein gewünschtes Ende allein ist kein beobachteter Endbeleg. Keine erste/letzte sichtbare Paketzeit als Ersatz einsetzen.
- Vollständige ursprüngliche Recorder-Start-/Stopantworten privat sichern; Startsidecar vor Überschreiben, fertiges Endsidecar und TX1-Roh-JSONL nach belegtem Ende exportieren. Hash, Größe, Zeilenzahl und Aufnahme-ID binden. Kanonisches `frames_written` unverändert typgenau prüfen; gleiche Daten nicht zweimal zählen.
- Peer-JSONL und vorhandenen Writer-/Endbeleg mit Lauf-/Phasenzuordnung und Zeilenzahl ergänzen. Tatsächlichen Flush/Close beziehungsweise Fehlerstatus belegen, nicht nur Dateivorhandensein.
- Bereits erzeugte Geräte-Originalantworten eindeutig nach Aktion, RX und Vor-/Nachklammer indizieren. BIN-, Runtime-, Diagnose-Source- und TX-Filterhash getrennt erhalten; acht Configwerte typgenau unverändert vergleichen. Keine zusätzlichen Liveabfragen.
- Adapterzeit, PCAP-Wandzeit und prozesslokale Servermonotonie ausdrücklich unterscheiden. Vorhandene Server-Zeitstempel und eng umschließende lokale Uhrenproben dauerhaft speichern; Abbildung mit messbarer Unsicherheit und Epochenbezug implementieren und testen. Keine Gleichsetzung der monotonen Ursprünge, keine erfundene Unsicherheit null. Mehrdeutige Fensterränder nicht zugunsten eines PASS verschieben; vorhandene Clockgrenzen erhalten.

Einzelne Fehler dürfen keinen zweiten Recorderstopp, Ersatzcapture, Upload oder ungeplanten Hostwechsel auslösen. Bisherige Budgets bleiben: 480/960 s, zwei Captures, zwei Geräte-OTA-POSTs, 42 normal geplante GETs, maximal 120 mit zwölf fürs Ende, drei Hostwechsel, 6.000 s einschließlich Endreserve und vier separat gezählte lokale Recordersteuerungen.

## C – Host: begrenzten RX-/Radar-Belegexport tatsächlich anbinden

Einen kleinen gemeinsamen Dateiexport an den vorhandenen Aufnahme-/Empfangspfaden implementieren. Keine neue Netzwerk-API oder zusätzliche Empfangssockets. Standardmäßig deaktiviert, Aktivierung später nur über ausdrücklich gebundene lokale Aufnahmeoption und neue private Zielablage. Keine ungefragte Dauertelemetrie.

Für RX am tatsächlichen `udp_receiver_task`-/Ingresspfad originale Empfangszeit, Payloaddigest, Quelle/Node/Boot/Sequenz soweit verifiziert, tatsächlichen Zeitpunkt der Verarbeitung und deren konkrete Annahme-/Ablehnungsstufe erfassen. Parserannahme, Featurezulassung, Serverannahme und Dateischreibung sind verschiedene Ereignisse. Dateischreibung am tatsächlichen Writer beziehungsweise durch dessen gebundenes Ergebnis belegen. Nicht an jeder frühen Rückkehr denselben angenommenen Erfolgswert schreiben. Peer- und TX1-Belege bleiben getrennt korrelierbar.

Für Radar den vorhandenen Empfang und `process_mmwave_datagram`/`MmwaveFleet::ingest_from_json_at` verwenden: Originalpaket beziehungsweise verlustfreier Rohpayload, Quelle, Node/Boot/Sequenz, Empfangs-/Verarbeitungszeit mit Hostepoche und tatsächlicher Annahmegrund dauerhaft exportieren. Keine Punktmediane, ACKs oder Statuszusammenfassungen als Rohzeitreihe ausgeben. Die aktuelle serverseitige Transformations-/Setupbindung aus dem unveränderten Vertrag referenzieren; nicht neu kalibrieren oder doppelt transformieren.

Begrenzte Queue, Datensatz-/Dateigröße und Abschlusszeit konkret festlegen und begründen. Dateischreiben darf den Empfang nicht durch unbeschränkte synchrone Arbeit blockieren. Überlauf, Writerfehler, Speichergrenze, Abbruch oder fehlender Flush/Close müssen einen sichtbaren unvollständigen Export erzeugen. Drop-/Fehlerzähler und endgültigen Hash/Zeilenzahl/Start-/End-/Epochen-/Phasenbeleg sichern. Keine stille Stichprobe oder erfundene Vollständigkeit.

Lauf-/Phasenmanifest, Setup-/Kandidaten-/Quellenbindung und Eingangsnormalisierung so verdrahten, dass die neue Auswertung unmittelbar genau diese Produzentendateien lesen kann. Für Radar P1–P9, tatsächliche Phasengrenzen, Zugänglichkeit und physische Unsicherheit als separat auszufüllendes Messprotokoll vorbereiten. Heute keine realen Werte behaupten. Fehlende manuelle Pflichtangaben bleiben erkennbar offen.

## D – Aussageumfang und Firmwarelücken ausdrücklich begrenzen

Die vorhandene Firmware liefert keine vollständigen korrelierten Ein-/Ausgangszeitstempel für den 100-ms-Nachweis je Paket. Diese Lücke **nicht** durch erfundene Zeitwerte, Hostzeiten, konstante `server_admitted=true`-Felder oder ein neues Firmwarepaket schließen.

Transport-/Rasterbeobachtung, physische Featureeignung, gemessene Hostfrische, Firmwarefrische, tatsächliche Aufnahmeannahme und Ressourcen jeweils als eigenen Unterbefund führen. Fehlende Belege sperren die Aussagen, die sie benötigen; sie dürfen weder korrekte getrennte Teilstatistiken verschwinden lassen noch durch einen allgemeinen PASS verdeckt werden. Ein Gesamturteil mit ausdrücklich geforderter, aber fehlender Firmwarefrische bleibt NICHT_BEURTEILBAR. Bestehende Schutz-/Kalibrierungsgates und Abnahmekriterien nicht abschwächen oder umbenennen, um Erfolg zu erzeugen.

Vor Abschluss eine Bereitschaftsmatrix liefern: welche konkreten RX-/Radarurteile der implementierte Export nun ermöglicht, welche nur frische Hardwarebindungen/physische Messangaben brauchen und welche weiterhin nicht erzeugbare Gerätebelege erfordern. Aus vorhandenen C/C1-Legacyframes keine physische Attestation erraten; der lokal gebaute RX2/RX3-Metadatenkandidat bleibt uninstalliert. Radar-G2 und RX-Lauf getrennt beurteilen. Keine neue Hardwarepflicht oder ein größerer Firmwareforschungsauftrag aus dieser Lücke.

## E – durchgehende Regressionen und notwendige Builds

Vorherquellen und beide Gegenbelege erhalten. Tests müssen über tatsächliche Produzenten → geschlossene Dateien → Normalisierung → Parser → Auswerte-CLI laufen. Keine handgefertigten Ersatzendbelege zwischen den Stufen einsetzen, um den Gutpfad passend zu machen.

Mindestens folgende Fälle:

- Layout-Rückkehr mit später wieder gültigen Fenstern; falscher Boot/Quelle und wirklicher Konfigurationswechsel bleiben gesperrt. Gemischter TX1-128-/Peer-64-Gesamtlauf mit eigenem Vertrag je Richtung, vertauschten/fehlenden Referenzen und unbekannter Form.
- Vorhandene RX-/Radar-Gut-, Grenz-, Fehler- und Determinismusfälle erhalten. 480 s: 473/min 426; 960 s: 953/min 858; Peer ≥1.920/≥2 Hz; Radar je Stufe 180/min 162 und eingefrorene Fehler-/Frische-/Überlappungsgrenzen. Historisch unverändert 1.786/1.793 ausgewählt und 1.780/1.793 streng.
- Echte lokale Exportproduzenten mit eingespeisten synthetischen Paketen/Uhren betreiben, anschließend vollständige RX- und Radarinputs daraus auswerten. Erwartete Unterurteile vorher festlegen; Firmwarefrische ohne reale Gerätebelege bleibt unbekannt. Gute Radar-Einzel-/Gemeinschaftsdaten und schlechte/fehlende/stale/mehrdeutige zweite Quelle am tatsächlichen Rustpfad prüfen.
- Writer-/Queueüberlauf, abgeschnittene Zeile, falscher Hash/ID/Phasenbezug, fehlender Endbeleg, fehlende Originalantwort, Wanduhrsprung, nicht deckungsgleiche monotone Ursprünge und Epochwechsel dürfen keinen falschen Erfolgsbefund erzeugen. Eine Uhrabbildung mit verschiedenen Ursprüngen und begrenzter Unsicherheit muss korrekt funktionieren; unzureichende Überlappung bleibt offen.
- Default ohne Exportoption erzeugt keine Tracedatei und verändert keine fachliche Verarbeitung. Aufnahmeabbruch und Writerfehler halten die vorhandenen Aktions-/Cleanupbudgets ein. Keine unsichtbaren zusätzlichen HTTP-/Hostaktionen.

Wegen Adapteränderungen alle **34 Kernprüfungen, 26 bisherigen Adapterszenarien und zehn Recorderregressionstests** erhalten und wiederholen; die 292 Szenariodurchläufe nicht als 292 eigenständige Tests ausgeben. Neue Export-/Erfassungsfälle ergänzen. Endgültige Kandidaten-/Host-/Profilbytes nochmals durch denselben Fake-I/O-Sessioneinstieg prüfen.

Geänderte Hostpfade in beiden Modi Default und `experimental-peer` prüfen und beide tatsächlichen Hostkandidaten lokal aus dem endgültigen Graphen bauen. Bestehende Behandlung ausdrücklich isolierter Loopbackfixtures beibehalten, niemals den Projektserver kontaktieren; netzfreie Tests und gegebenenfalls reine Testfixtures klar trennen. Den reinen Auswerteexporter nur neu bauen, falls seine tatsächlich kompilierten Quellen geändert wurden; richtige Produktions-/Exporterbindings nachweisen. Keine SDK-/Firmwarebuilds. Reine Quell-/Hash-/Buildprüfung ist kein Hardware- oder Laufzeitlastnachweis.

## Abschluss und Lieferung

Neue private Lieferung: minimaler Patch und Quellen, unveränderte Vorherbelege, Tests/Receipts, neue lokale Hostbinarys samt Source-/Binary-/Profilbindungen, Normalisierer/Dateiverträge und Templates, vollständige synthetische Produzentenoutputs mit zugehörigen Auswerteberichten, historische Regression, aktualisierte `ERFASSUNGSUEBERGABE.private.md`, Abschlussmanifest und Erhaltungsnachweis. Nicht benötigte weitere Projektbestandteile unangetastet lassen.

Die Übergabe muss für jedes erforderliche Feld den jetzt implementierten Produzenten und dessen beweisenden Integrationstest nennen. Offen verbleibende Felder mit Grund und betroffener Aussage auflisten. Gesperrte spätere Aufnahmebefehle und Hardwareplanbindungen vorbereiten; keine Freigaben erteilen und keine Messung ausführen. Tatsächliche neue Host-/Adapterkandidaten brauchen erst den unabhängigen Koordinatorreview, bevor sie in einen Hardwareauftrag übernommen werden.

Urteile getrennt:

```text
RX_LAYOUT_RUECKKEHR: KORRIGIERT_UND_REGRESSIERT / KONKRETE_LUECKE
RX_STRECKENVERTRAEGE: KORRIGIERT_UND_REGRESSIERT / KONKRETE_LUECKE
RX_DATEIEXPORTE_UND_CLOCKBINDUNG: IMPLEMENTIERT_UND_INTEGRIERT / KONKRETE_LUECKE
RADAR_PAKET_HOSTZEIT_EXPORT: IMPLEMENTIERT_UND_INTEGRIERT / KONKRETE_LUECKE
PRODUZENT_BIS_AUSWERTUNG: BESTANDEN / NICHT_BESTANDEN
HOST_DEFAULT_PEER_BUILDS: BESTANDEN / KONKRETE_LUECKE
FIRMWAREFRISCHE: BELEG_FEHLT, kein erfundener oder neuer Gerätebeleg
ERFASSUNGSUEBERGABE: BENANNTE_TEILURTEILE_ABGEDECKT / KONKRETE_LUECKEN
RADAR_HARDWAREVORBEREITUNG: ZUR_KOORDINATORPRUEFUNG / NICHT_READY
RX_HARDWAREVORBEREITUNG: ZUR_KOORDINATORPRUEFUNG_MIT_BENANNTEN_GRENZEN / NICHT_READY
HARDWAREAKTIONEN: 0; INSTALLATION: KEINE; HOSTAKTIVIERUNG: KEINE
```

Nach der finalen Integration Quellen/Inputs/Binarys/Manifeste erneut unverändert binden, ursprüngliche Dirty-Dateien und versiegelte Lieferungen nach Hash erhalten. Genau ein nächster Schritt: unabhängiger Koordinatorreview dieser gebündelten Korrektur-/Exportlieferung. Danach stoppen. Kein Hardwarelauf, kein Folgeagent, keine automatische Fortsetzung.

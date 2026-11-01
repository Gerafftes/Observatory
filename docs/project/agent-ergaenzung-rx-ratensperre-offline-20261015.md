<!-- Publication copy: local paths and hardware identifiers were pseudonymized; private original retained. -->
# Offline-Ergänzung: RX-Ratensperre, Kandidatenneubau und Vergleichsvorbereitung

Projekt: `.`

Diese Ergänzung gehört zum laufenden Auftrag `./docs/project/agent-auftrag-rx-dateibudget-vorklammern-abschluss-offline-20261015.md`. Führe dessen Dateibudget-, Diagnoseklammer- und Abschlussplanarbeiten weiter und integriere die folgenden Arbeiten vor dem gemeinsamen Abschluss. Keine weiteren Agenten starten, keine anderen Chats anschreiben, keine Automationen. Nach vollständig geprüfter Offline-Lieferung für den Koordinatorreview stoppen.

## Ziel, Befund und genaue Umfangserweiterung

**Das ist der Fehler:**

Im gebundenen C1-Collector setzt das globale 20-ms-Ratengate seinen Zeitanker vor der TX-Quellenprüfung. Ein später verworfenes fremdes Paket kann deshalb einen TX1-Slot verdrängen. Zusätzlich können strikt am letzten angenommenen Paket ausgerichtete 20-ms-Gates bei kleinen Ankunftsschwankungen nutzbare Frames unnötig verwerfen. Der Paarpfad hat mit `csi_peer_admit()` bereits eine eigene Quellenzulassung sowie Gesamt-/Linkquoten und kann anschließend zusätzlich durch die Collector-Gates laufen. Seine tatsächliche Reihenfolge muss berücksichtigt werden; er ist nicht mit dem Defaultpfad identisch.

Der neuere öffentliche Arbeitsstand enthält bereits einen Ansatz mit Quellenprüfung vor der Zulassung und phasenbezogener Deadline mit 500 µs Frühtoleranz. Dieser Stand hat auch andere Diagnose-/Transportänderungen und ist **keine komplett zu übernehmende Firmwarebasis**. Die im laufenden Auftrag gebundenen Paar- und Metadatenimages enthalten die Korrektur noch nicht. Ein Quellfund beweist weder die Ursache des historischen Ratenrückgangs noch einen realen Leistungsgewinn.

**Das erwarte ich:**

Eine minimale, tatsächlich integrierte Ratensperrenkorrektur auf der geprüften privaten C1-/Peerbasis, erhaltene Vorhergegenbeispiele, bestandene Regressionen an den echten Callback-/Peerpfaden, neu gebaute und gebundene betroffene Firmwarevarianten und eine ausführbare Offline-Vorbereitung des späteren Hardwarevergleichs. Dateibudget, Diagnoseklammern, Adapter und Auswertung müssen mit den endgültigen Kandidaten konsistent sein.

**Diese Ergänzung ersetzt ausschließlich für die Ratensperrenkorrektur und ihre nötigen Regressionen/Builds die bisherigen Verbote „Firmware unverändert“ und „kein SDK-/Firmwarebuild“.** Erlaubt sind lokale Änderungen in neuen privaten Quellkopien und lokale No-Fetch-Neubauten. Hardwarezugriff, Installation, Liveaktivierung und zusätzliche Geräteeingriffe bleiben vollständig ausgeschlossen. Keine allgemeine Collector-/Transportmodernisierung.

## Eingänge und relevante Dateien

Zuerst den eigenen aktuellen Zwischenstand des Hauptauftrags sichern; keine zweite konkurrierende Arbeitsbasis beginnen. Bereits bestandene Arbeiten und Zwischenbelege erhalten. Bei zwischenzeitlich erstelltem Abschluss eine neue Folgelieferung mit Bezug darauf anlegen, keinen versiegelten Bestand erweitern oder umschreiben.

Gezielt lesen und binden:

- Die eigene neueste private Arbeitskopie des Dateibudget-/Vorklammerauftrags und dessen aktuellen Sessionvertrag. Der inzwischen gemeldete Zwischenplan mit 82 GETs und 4.900 s ist noch kein Koordinator-PASS; die tatsächlichen finalen Dateien sind maßgeblich. Nicht auf den historischen 42-GET-Vertrag zurückfallen.
- `./private/rx-radar-export-korrektur-offline-20261015T152415Z/CANDIDATES.private.json`: Herkunft der bisherigen Images, Host- und Metadatenbindung. Neuere eigene Adapter-/Hostarbeiten darauf erhalten.
- Geprüfte Firmwarebasis `./private/peer-c1-review-korrektur-offline-20261015T050734Z/rx/`: insbesondere `main/csi_collector.c`, `csi_peer.c`, `csi_peer.h`, `csi_transport.c`, `rx_diag.c`, `rx_diag.h`, `Kconfig.projbuild` und direkt betroffene Test-/Buildbindungen. Die Quellen der vier vorgesehenen Varianten zunächst gegeneinander prüfen.
- Metadatenbasis `./private/offline-tagespaket-20261015T061442Z/rx/` und `profiles/tx-only-metadata.sdkconfig`. Das gemeinsame RX2/RX3-Image ist noch nicht installiert.
- Nur als gezielte Implementierungs-/Testreferenz: `./software/ruview/firmware/esp32-csi-node/main/csi_collector.c` und `test/test_rx_diagnostics.c`. Vorhandene Fremdframe-, Jitter-, Überlast- und Pausentests nutzen, aber an die tatsächlichen privaten Produktionspfade und Diagnoseverträge anpassen. Keine komplette Übernahme des öffentlichen Collectors, Workers oder Diagnose-API.
- Vorhandene Build-/Testvorlagen unter `./private/peer-c1-blocker-korrektur-offline-20261014T205020Z/scripts/`, insbesondere `native_peer.py`, `native_corrected.py`, `native_regressions.py`, `build_firmware.py`, `bind_candidates.py`. Erst Sideeffects/Pfade lesen; nur angepasste eigene Kopien ausführen. Historische Gesamtskripte können Originale überschreiben.

Keine breite Repo-, Chat-, Vendor- oder Backupinventur. ProjectAtlas nicht verwenden. SDK-/APIfragen anhand des bereits gebundenen lokalen SDKstands klären; keine Downloads oder neuen Abhängigkeiten.

## E – gezielte Ratensperrenkorrektur

1. Den tatsächlichen Weg für Default, TX1 im Paarprofil und Peerpartner aufschreiben: Eingangsvalidierung, Quellenprüfung, Gesamt-/Linkzulassung, Collector-Gates, Serialisierung/Sequenz, Queue und Diagnose. Ein einziger expliziter Zeitbezug pro Zulassungsentscheidung; keine verdeckte zweite Sperre, die einen zugelassenen Slot durch denselben Jitter nochmals verliert.
2. Ungültige Eingaben und fremde Quellen dürfen keinen TX1-/Peerzeitanker oder Quotenplatz verbrauchen. NULL, fehlender/zu großer/ungültiger IQ-Puffer und RX-Fehlerzustand sicher behandeln. Physisch diagnostisch lesbare Formen nicht bloß wegen fehlender Kalibrierzulassung verwerfen. Kein gespeicherter Zeiger auf temporäre Callback-Puffer.
3. Für passende gültige Frames das vorhandene Deadlineprinzip mit explizit begrenzter Frühtoleranz von 500 µs gezielt übernehmen, soweit es mit den bestehenden Verträgen vereinbar ist: angenommene frühe Frames verschieben die Sollphase nicht dauerhaft; verpasste Slots erzeugen keine Nachholgutschrift oder Sendebursts. Zeitnull, Wiederinitialisierung und ungültige/rückläufige Uhrwerte deterministisch behandeln. Kein Ratensweep, keine Ratenerhöhung.
4. **Paargrenzen erhalten:** nominell insgesamt 50 Hz je RX, bestehende Aufteilung maximal 40 TX1- und zehn Peerzulassungen je Quotenfenster, acht Poolplätze mit vorhandener Linkreservierung, begrenzte Firmwarelaufzeit 3.000 s, Hostprofil 1.200 s, Partner-/AP-/Selbstablehnung. Prüfen, ob `csi_peer_admit()` selbst die strikte 20-ms-Jitterfalle enthält; nur den betroffenen Zeitentscheid korrigieren. Bestehende Linkquoten und Überlastschutz nicht zugunsten höherer Balken lockern. Bei dauerhafter Überlast bleiben tatsächliche Drops sichtbar.
5. Die minimale Lösung kann einen kleinen gemeinsam benutzten Zeitentscheid benötigen; keine generische Schedulerarchitektur. Nicht den gesamten neueren Transport übernehmen. HT20, PHYrate 6 Mbit/s im Paarprofil, LTF/Skalierung, Selbst-Ping-Einstellung, Wire-/Layout-/Maskenvertrag, NVS, OTA, Bootloader, Partitionierung und Sicherheit unverändert lassen.
6. Diagnose an die geänderte Auswahlreihenfolge korrekt anbinden. Tatsächlicher Eingang, ungültige/fremde Quelle, Rate-/Linkquotenverwerfung, Serialisierung, Queue-/Sendefehler und Hostbeobachtung bleiben unterscheidbar. Bestehende Diagnosefelder möglichst erhalten; geänderte Semantik explizit kennzeichnen und im Auswerter versions-/quellgebunden behandeln. Kein Paket doppelt gut-/abschreiben, keine Sequenz für einen nie serialisierten Frame erfinden. Physischer Empfang, Softwareverwerfung und ungeklärte Sequenzlücke bleiben getrennte Populationen.

## F – Vorhergegenbeispiele und Regressionen

Mit kontrollierter synthetischer Zeit die **echten** Produktionsfunktionen aus alten und neuen privaten Quellen kompilieren/aufrufen. SDK-, Uhr- und Sendeschnittstellen dürfen Doubles sein; nicht den Algorithmus im Test nachbauen. Gegenbeispiele zunächst unverändert gegen die alte Basis ausführen und als erwarteten Fehlerbeleg erhalten, danach dieselben Eingänge gegen die Korrektur prüfen. Ein nicht reproduzierter Verdacht bleibt als solcher bezeichnet.

Pflichtfälle:

- Fremdes Paket unmittelbar vor passendem TX1-Paket; Variante mit gleicher Zeit und mit 400 µs Abstand. Der Abstand von 400 µs liegt ausdrücklich innerhalb der vorgesehenen 500-µs-Toleranz. Wiederholter Fremdverkehr und ungültige Eingaben verbrauchen keine Slots/Quoten.
- Passende TX1-Folge mit abwechselnd 19,8/20,2 ms und längere 20-ms-Sollfolge mit deterministischem Jitter innerhalb ±450 µs. Erwartete Zulassungen vorab angeben; Frühgrenzen 499/500/501 µs prüfen. Default und tatsächlicher Paar-TX1-Pfad gesondert testen, unter dessen unveränderten Linkquoten.
- Gleiche-Zeit-Bursts, anhaltender 1-kHz-Eingang, lange Pause und anschließender Burst, verspätetes Paket, Zeitnull, Wiederinitialisierung und ungültige/rückläufige Zeit. Endliche Frequenz-/Burstschranke einschließlich zulässiger Frühtoleranz herleiten; eine endpunktinklusive Fensterzählung nicht fälschlich als Ratenerhöhung bewerten.
- Interleavte TX1-/Peerframes auf RX1/RX4, Quotenfenstergrenzen, beide Richtungen, fremde/Selbst-/AP-Quellen, abgelaufener Peerzeitraum und Default ohne Peerbetrieb. Peer darf keine TX1-Quote unbemerkt verbrauchen; Gesamtlimit, getrennte Quoten und acht Poolplätze müssen weiterhin halten. Keine Erwartung „jeder TX1-Frame passiert“ bei absichtlicher Überlast oder legitimer Peerbelegung.
- NULL, ungültige Puffer/Längen/RX-Zustände, Filter an/aus und Ratengate offen/geschlossen. Kein undefiniertes Verhalten, keine ungültige Speicherreferenz, kein Slot-/Quotenverbrauch und eindeutige Diagnose.
- Queuevoll, Sendefehler, Ablauf nach Backoff, 100-ms-Frischegrenzen, paralleler Statussnapshot und begrenztes Ende. Ratensperrenkorrektur darf keinen früher geschlossenen Frische-/Ownership-/Abbruchfehler wieder öffnen.

Bestehende relevante native Regressionen erneut ausführen, inklusive ASan/UBSan, C→Rust-Wire-/Physicalinterop und gemeinsamer Maske. Erwartete NVS-Nichterasure, HT20-/PHYstartreihenfolge, Layout-Rückkehr, Defaultisolation, Partnerbindung und zeitliches Ende erhalten. Die laufenden 34 Kern-, 26 Adapter- und zehn Recorderprüfungen mit 292 Szenariodurchläufen sowie neue Klammer-/Lastprüfungen auf den endgültigen Stand beziehen. Neue Sollzahlen separat ausweisen; historische fehlgeschlagene Versuche nicht als erfolgreiche Tests zählen.

## G – Neubau, Bindung und gemeinsame Lieferung

Vier bestehende Firmwarevarianten aus den finalen minimal korrigierten Quellen lokal neu bauen: TX-only-Defaultkontrolle, RX1-Paar, RX4-Paar und das gemeinsame TX-only-Metadatenimage für RX2/RX3. Keine zusätzlichen Varianten oder Installation. Unterschiede in Peer-/Attestationsprofilen erhalten. Bei belegter Nichtbetroffenheit einer Variante diese vor dem Build anhand tatsächlicher Callpaths begründen; die gemeinsamen Collector-/Peeränderungen nicht durch alte Images verdecken.

Den bereits gebundenen lokalen ESP-IDF-/S3-Stand und Cache verwenden. **Vor dem ersten SDK-Aufruf wirksames Prozess-Netzverbot nachweisen**, zusätzlich Component Manager/Fetch deaktivieren; diese Optionen allein verhindern nicht alle CMake-Downloads. Keine Toolchain-/SDKmigration oder Installation. Fehlende lokale Voraussetzung präzise als Buildblocker berichten, alle übrigen Offlinearbeiten abschließen; keine Ersatzdownloads.

Neue BIN-/ELF-/Runtime-/Source-/Config-/Testhashes, ausgeführte Buildargumente, Exitcodes und Appgrößen gegen die gespeicherte 2-MiB-Referenz binden. Bootloader-/Partition-/NVS-/Securityverträge mit den alten Referenzen vergleichen; keine neuen Geräteslotbelege daraus ableiten. Originalimages und alte Manifeste erhalten.

Endgültige neue Kandidaten in eigener versionierter Kandidaten-/Adapterbindung verankern. Hardwareplan, Erwartungsprüfung vor erster I/O, Sessionvertrag, Source-/Schemaerkennung, Fake-Originalantworten, Normalisierung und Report gemeinsam aktualisieren. Ein alter Runtimehash darf das neue Image nicht versehentlich zulassen. Alte weiterhin installierte RX2-/RX3-C/C1-Erwartungen von neuen uninstallierten RX2-/RX3-Kandidaten ausdrücklich trennen.

Nur tatsächlich betroffene Host-/Dateibinarys neu bauen; keine pauschale Wiederholung unveränderter Hostbuilds. Den tatsächlichen Weg Adapter → Sessionkern → Fake-I/O und synthetischer Produzent → geschlossene Dateien → Normalisierung → Parser → Bericht mit finalen Bindungen wiederholen. Testoutputs ausschließlich in eigenen neuen Ablagen, auch bei eincompiliertem `CARGO_MANIFEST_DIR`.

Dateibudget/Lastmodell des Hauptauftrags an den finalen Zulassungsschranken prüfen: nicht mit den historischen 17–32 Frames/s dimensionieren. Vier RX, alle TX1-/Peer-/Sync-/Diagnoseklassen und Quoten-/Jitter-/Burstgrenzen berücksichtigen; keine doppelte Anrechnung der Peerquote auf ein bereits gemeinsames 50-Hz-Limit. Der 960-s-Lastnachweis bleibt mindestens 80 TX1- plus vier Peerframes/s; höhere belegte Obergrenzen samt Byte-/Disk-/Flushreserve zusätzlich absichern. Die zuletzt geplanten Dateietats müssen diese Rechnung bestehen oder vorab nachvollziehbar angepasst werden.

## H – Hardwarevergleich ausschließlich vorbereiten

Den gesperrten Hardwarevorschlag des Hauptauftrags um eine konkrete Vergleichstabelle und deren tatsächliche Auswertung ergänzen. **Keine Messung durchführen.**

Innerhalb des bisherigen Umfangs bleiben genau zwei Captures (480 s Basis, 960 s Paar), höchstens zwei OTA-POSTs ausschließlich RX1/RX4, höchstens 120 GETs mit zwölf End-GETs Reserve, vier Recordersteuerungen, drei Hostwechsel und maximal 6.000 s einschließlich 600 s Endreserve. Den neuen normalen GET-Verbrauch und Worst-case-Pfad exakt aus dem finalen Vertrag rechnen. Keine zusätzlichen Captures, Geräte-GETs während der Aufnahme, Wiederholungen oder versteckten Uploads vorsehen. Frische Diagnoseklammern bleiben Pflicht.

Je RX und Phase vorbereiten: Firmware/Boot/Quellen-/Configbindung, Messdauer, passende TX1-Frames/s, gültige gemeinsame Fenster, Datenlücken und Datenalter soweit belegt, Eingangs-/Fremd-/Rate-/Linkquoten-/Queue-/Sendezähler mit Nenner und Klammerintervall, Host-/Writerbelege und ihre Vollständigkeit. Peerlinks getrennt ausweisen. Nicht exportierbare oder nicht vergleichbare Zähler bleiben unbekannt; keinen Erfolg allein aus höherer Framerate ableiten. Konkrete JSON-/Tabellenfelder und ein synthetisches vollständiges Positiv- sowie Fehlbelegbeispiel liefern.

**Wirkungsgrenze verbindlich im späteren Bericht:** Der Basis→Paarvergleich ändert auf RX1/RX4 zugleich Firmware und Peerbetrieb. Er misst die kombinierte Wirkung und kann eine isolierte Verbesserung durch die Ratensperre nicht beweisen. RX2/RX3 bleiben unveränderte Kontrolllinks; gleichbleibende Geometrie, TXrate, Selbst-Ping und Hostbedingungen sind zu dokumentieren. Die historischen 31,9 Frames/s sind keine aktuelle Sollschwelle. Keine Zusage einer bestimmten Framerate oder Positionsgenauigkeit.

Falls ein isolierter Hardware-A/B-Nachweis erforderlich bleibt, dessen kleinsten gesonderten Plan konkret beschreiben: identische Aufstellung, gleiche Parameter und Peerzustände, nur Ratensperrenstand unterschiedlich. Erforderliche zusätzliche Images/Uploads/Aufnahmen/Freigaben ausdrücklich nennen, aber **nicht** in die vorstehenden Budgets einschmuggeln oder durchführen. Keine neue Steuer-API allein zur Ermöglichung dieses Zusatzversuchs entwickeln.

Ein Vier-RX-Rollout der neuen Ratensperre ist ein eigener zukünftiger Umfang. RX2/RX3 heute nur offline qualifizieren; weder deren Installation noch weitere Unterbrechungen sind freigegeben. Die drei Restpunkte vom 14. Oktober bleiben eigene Abnahmezeilen; keine Tageshäkchen ändern.

## Grenzen, Ablage und Abschlusskriterien

Geräte-/Liveaktionen **null**: kein serieller/USB-/ROMzugriff, Reset, Flash, OTA, Restore, Provisionieren, Geräte-HTTP/Ping/Scan, Capture, Projekt-API, ARP-/WLAN-/Routen-/produktive Prozess-/Socketinventur oder Host-/Dienstaktivierung. TX1, installierte RX, Radare und Hostnetz unverändert. Keine echten Credentials lesen, keine eFuses-/Sicherheitsänderungen. Keine neuen Agenten, Chatnachrichten oder Automationen.

Neue private Ablage unter `./private/` beziehungsweise neue eigene Fortsetzung verwenden; vor Anlage wirksamen Git-Ignore und Nichtversionierung prüfen. Verzeichnisse 0700, Dateien 0600, nötige eigene Ausführbare 0700. Originale, Rohdaten, Backups, Setups und Dirty Worktree erhalten; kein Reset/Clean/Staging/Commit/Push. Reale Identitäten/Configs bleiben aus öffentlichen Berichten und Toolausgaben. Koordinatorplan/Tracker nicht bearbeiten.

Zusätzlich zur Hauptlieferung liefern: `RATENSPERRE.private.md` mit Vorher-/Nachherpfad und Gegenbelegen, Patch, tatsächliche Test-/Buildreceipts, endgültige vier Variantenbindungen, aktualisierte Adapter-/Session-/Lastbindung und `HARDWAREVERGLEICH-RATENSPERRE.private.md`. Ein finaler Gegencheck prüft die **ausgelieferten** Quellen/Images/Erwartungen; Tests gegen einen Zwischenstand genügen nicht.

Getrennte Abschlussurteile:

```text
RATENSPERRE_DEFAULT: KORRIGIERT_UND_GEPRUEFT / KONKRETE_LUECKE
RATENSPERRE_PAAR_TX1_UND_PEER: KORRIGIERT_UNTER_ERHALTENEN_QUOTEN / KONKRETE_LUECKE
VORHERGEGENBEISPIELE_UND_REGRESSIONEN: BESTANDEN / KONKRETE_LUECKE
FIRMWARE_NEUBAUTEN_UND_BINDUNGEN: VARIANTEN_EINZELN_AUSWEISEN
DATEIBUDGET_VORKLAMMERN_UND_PRODUZENT_BIS_BERICHT: BESTANDEN / KONKRETE_LUECKE
HARDWAREVERGLEICH: VORBEREITET_MIT_WIRKUNGSGRENZE / KONKRETE_LUECKE
ISOLIERTER_REALER_RATENGEWINN: NICHT_GEMESSEN
RX2_RX3_INSTALLATION: NICHT_FREIGEGEBEN_UND_NICHT_DURCHGEFUEHRT
HARDWAREAKTIONEN: 0; INSTALLATION: KEINE; HOSTAKTIVIERUNG: KEINE
```

Einziger nächster Schritt nach Lieferung: unabhängiger Koordinatorreview des gesamten Offlineabschlusses einschließlich dieser Ergänzung. Danach stoppen; kein automatischer Hardwarelauf bei angeschlossenen Geräten oder aufgrund früherer Freigaben.

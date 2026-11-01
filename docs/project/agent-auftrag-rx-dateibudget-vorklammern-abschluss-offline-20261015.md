<!-- Publication copy: local paths and hardware identifiers were pseudonymized; private original retained. -->
# Offlineauftrag: RX-Dateibudget, Diagnose-Vorklammern und Abschlussplan für den 14. Oktober

Projekt: `.`

Johann bereitet parallel TX1 und vier RX physisch vor. Das ist **keine Erlaubnis für Geräteabfragen oder Liveaktionen dieses Agenten**. Führe ausschließlich die nachstehenden Offlinearbeiten vollständig aus. Keine weiteren Agenten, Chatnachrichten oder Automationen. Nach Lieferung für den unabhängigen Koordinatorreview stoppen; danach kein automatischer Messbeginn.

## Ziel und bestätigte Ausgangslage

**Das ist der Fehler beziehungsweise die offene Aufgabe:**

Beide RX-Auswertefehler und die RX-/Radar-Erfassungsexporte der Lieferung `./private/rx-radar-export-korrektur-offline-20261015T152415Z/` sind unabhängig geprüft. Nicht neu entwickeln: Layout-Rückkehr, getrennte TX1-/Peerraster, kanonischer Recorder-Endvertrag und tatsächliche Produzent→Datei→Bericht-Integration bleiben erhalten.

Vor dem geplanten 480-/960-s-RX-Lauf sind drei Dinge zu erledigen:

1. Die feste Tracegrenze von 256 MiB ist nicht ausreichend abgesichert. Der bisherige synthetische Paarlauf mit 24 TX1- plus vier Peerframes/s erzeugt etwa 96 MB. Die unabhängige Projektion bei 74,1692 TX1- plus vier Peerframes/s ergibt 270.309.757 Byte, oberhalb von 268.435.456 Byte; bei 80 plus vier etwa 290.568.725 Byte. Ein realer Überlauf wurde nicht beobachtet.
2. Der bisherige 42-GET-Plan liefert keine vollständige frische Diagnose-/Source-Vorklammer aller RX vor der Basis und keine frische RX2-/RX3-Diagnose vor dem Paar. Nachherantworten dürfen diese Lücken nicht rückwirkend füllen.
3. Vom 14. Oktober sind drei Punkte offen: reale Vier-RX-Layout-/Masken-/Zeitbindung, vollständige Fehler-/Ressourcenbilanz und tatsächlicher RX1↔RX4-Funkvergleich. Ein erfolgreicher Paarlauf allein belegt nicht alle drei. RX2/RX3-Legacyattestation, Firmwarezeit und manche Ressourcenwerte fehlen weiterhin.

**Das erwarte ich:**

Eine neue private, getestete und versiegelte Lieferung mit tragfähigem endlichem Dateibudget, einem ausführbar modellierten und getesteten Diagnoseklammerplan sowie einer konkreten Abnahmematrix für alle drei Restpunkte. Der folgende Hardwareauftrag muss daraus ohne neue allgemeine Codeanalyse vorbereitet werden können: genaue Reihenfolge, Dateien, erwartete Teilurteile, nötige Freigaben und bekannte Grenzen stehen fest. Keine Zusage „5/5 nach einem Lauf“, wenn Pflichtbelege technisch nicht entstehen können.

## Relevante Eingänge – gezielt lesen

Zuerst Arbeitsbaum und wirksame `AGENTS.md` prüfen. Danach:

- `./docs/project/tracking-zeitplan-20261025.md`: aktuelle Priorität RX vor Radar und die drei offenen Punkte. `./docs/project/rx-radar-export-review-20261015.md`: aktueller Review einschließlich Dateibudgetvorbehalt; seine frühere Radarpriorität ist durch Johanns jüngere Entscheidung überholt.
- `./private/rx-radar-export-koordinatorreview-20261015/rx-volume-projection.private.json` und zugehöriger Reviewreceipt. Beobachtete Vergangenheit, synthetische Bytegrößen und Hochrechnung getrennt halten.
- Aktuelle Lieferbasis `./private/rx-radar-export-korrektur-offline-20261015T152415Z/`: Bericht, Bereitschaftsmatrix, Erfassungsübergabe, Validierungsindex und Manifeste; danach gezielt `adapter/session_core.py`, `real_adapter.py`, `capture_export.py`, `SESSION-CONTRACT.private.json` und betroffene Tests/Fake-I/O-Eingänge.
- Im privaten Host derselben Lieferung: `host/crates/wifi-densepose-sensing-server/src/evidence_export.rs`, zugehörige Empfangs-/Raw-/Peerwriterpfade, `scripts/normalize.py`, Dateiverträge, tatsächliche Produzentenfixtures und Tests. Gespeicherte ursprüngliche Gegenfälle für Layout-Rückkehr und TX1-128/Peer-64 erhalten.
- Unveränderte Firmware-/Metadatenkandidaten ausschließlich über `CANDIDATES.private.json` binden. Für die Abnahmematrix bei Bedarf die tatsächlichen Diagnoseschemata/Statusproduzenten nur lesend prüfen; öffentliche Ansatzpunkte `./software/ruview/firmware/esp32-csi-node/main/ota_update.c`, `csi_transport.c`, `Kconfig.projbuild` und TX-seitig `tx_ota.c` im entsprechenden `esp32-csi-tx/main/`-Verzeichnis. Ein aktueller Quellstand allein beweist kein installiertes Geräteverhalten.

Keine breite Repo-/Chat-/Backup-/Vendorinventur, keine neuen externen Quellen nötig. Originale niemals als schreibende Gesamteinstiege ausführen.

## A – Dateibudget umsetzen und unter passender Last prüfen

Neue private Kopie/Ablage verwenden, z. B. `./private/rx-budget-vorklammern-offline-20261015-<UTC>/`. Vor Anlage Ignore und Nichtversionierung prüfen; Verzeichnisse 0700/Dateien 0600, notwendige eigene ausführbare Dateien 0700. Keine Originallieferung umschreiben.

Das Bytebudget aus tatsächlich serialisierten Datensatzgrößen und einer belegten Rate-/Burstobergrenze herleiten. Je Phase erfassen: alle vier Empfänger, beide Peerstrecken ohne Doppelzählung, Empfangs-/Schreibeereignisse, Raw-/Peer-/PCAPdateien, Vor-/Nachlauf, maximale Feldlängen und endliche Reserve. Nicht nur Mittelwerte des alten Sechs-Hz-Fixtures verwenden; Source-/Configraten an die tatsächlich gebundenen Kandidaten koppeln. Unbekannte Hintergrund-/Fehlerlast ausdrücklich begrenzen beziehungsweise als Überlaufgrund ausweisen.

Die kleinste korrekte Lösung wählen und implementieren: nachvollziehbar größerer endlicher Etat, verlustfreie Kompaktierung redundanter Ereignisdaten oder eine andere kleine belegbare Variante. Kein unbeschränkter Writer, keine stille Stichprobe, kein Weglassen von Pflichtbelegen, keine kürzere Messung oder gelockerte Qualitätsgrenze. Originalpayload mindestens am vorgesehenen tatsächlichen Empfangsbeleg verlustfrei erhalten; Writerbelege eindeutig dazu korrelieren. Bei Formatänderung Writer, Normalisierer, Schema, Verträge, Guards und Auswerter gemeinsam anpassen.

Queue-/Datensatz-/Dateigrenzen, Ressourcenbedarf der späteren Dateiauswertung und begrenzten Flush/Close erhalten beziehungsweise begründet neu binden. Ein größerer Dateietat ist kein vergrößerter unbegrenzter RAMpuffer. Neue Dateigrenze vor Datenansicht festlegen, nicht bei Überlauf nachträglich erhöhen. Lokalen Platzbedarf vor dem späteren Lauf prüfbar machen; heute keine Produktivprozess-/Geräteinventur.

Nachweis: eine echte vollständige **960-s-Synthese mit schneller synthetischer Zeit**, mindestens 80 TX1-Frames/s insgesamt plus vier Peerframes/s, durch den tatsächlichen Exportwriter bis Normalisierung/Auswertung führen. Source-gestützte höhere vorgesehene Obergrenzen zusätzlich rechnerisch und passend getestet absichern; Rate-/Burstannahmen und Reserve ausdrücklich nennen. Kein 16-minütiges Echtzeitwarten. Endliche Outputs, korrekte Zähler/Hashes, kein unbemerkter Drop und erfolgreicher begrenzter Abschluss. Zusätzlich knapp unter/auf/über der neuen Grenze, Writerfehler und Queueüberlauf prüfen: außerhalb des zulässigen Budgets sichtbar unvollständig, niemals falscher PASS.

## B – Diagnose-Vorklammern in den echten Ablauf integrieren

Für beide Messphasen vor dem Capture eine konsistente frische Bindung aller vier RX vorsehen: Status/Boot/Runtimeimage, acht Configfelder und Diagnose-/Sourceidentität. Bereits korrekt geplante Belege dürfen begründet verwendet werden; jede Verwendung braucht beobachteten Zeitpunkt, Node-/Boot-/Bildbezug, explizite Aktualitätsgrenze und genügend Budget. Keine alte gespeicherte Antwort als frische Vorklammer und keine späteren End-GETs rückdatieren.

Neue notwendige GETs mit festem Ziel, Pfad, Zeitpunkt, Timeout, Phase und Abbruchregel im Ablaufkern und tatsächlichen Adapter implementieren. Konkrete neue Normalzahl selbst festlegen und unabhängig nachrechnen. Die frühere Zahl 42 ist ein historischer Planwert, keine Aufforderung, notwendige Abfragen zu verstecken. Neue Step-/Kosten-/GETzahlen in einer **neuen Version** von Sessionvertrag, Tests, Kandidaten-/Adapterbindungen und gesperrtem Hardwareauftrag konsistent führen. Alte Version erhalten.

**Unveränderte Obergrenzen:** höchstens 120 Geräte-GETs, davon zwölf ausschließlich fürs Ende reserviert; höchstens zwei OTA-POSTs für RX1/RX4 und keine weiteren Geräte; zwei Captures 480/960 s; vier lokale Recordersteuerungen; drei Hostwechsel; Gesamt höchstens 6.000 s mit 600 s Endreserve. Keine Geräteabfrage während der Aufnahme, kein Retry oder Ersatzlauf.

Zusätzliche Klammern dürfen die begrenzte Peerzeit nicht sprengen: Hostprofil höchstens 1.200 s seit Öffnung, Firmware jeweils 3.000 s seit Boot, VALID 0/60/120 s mit bisherigen einzelnen Timeouts und Konsistenzprüfungen. Insbesondere längere RX2/RX3-Vorklammern sinnvoll **vor** Öffnung des begrenzten Peerhostprofils einplanen, wenn sie sonst die Restmessung verdrängen würden. Aktualität und Zustand bis Capturestart trotzdem prüfen. Vor jedem Dispatch und nach Wartezeiten Gesamtrestpfad samt Messung, Klammern, Hostrückkehr und Endreserve neu berechnen.

Normalisierung muss genau die neuen echten Originalantworten erkennen und der richtigen Vor-/Nachklammer zuordnen. Ein vollständiger synthetischer Produzentenlauf darf anschließend keine vermeidbare `source_before=null`- oder Boot-/Configvorklammerlücke mehr aus dem alten Ablauf haben. Fehlende, falsche, veraltete oder wechselnde Bindung führt kontrolliert zum Abbruch beziehungsweise offenem Teilurteil, ohne nächsten Upload bei RX1-Fehler.

## C – verbindlicher Abschlussplan für die drei offenen Punkte

Eine Datei `ABSCHLUSSPLAN-14.private.md` mit genau diesen drei getrennten Zeilen liefern. Pro Zeile: aktuelles Kriterium, tatsächlicher Produzent, konkrete Messung/Datei/Felder, Auswertung, PASS/FAIL/NICHT_BEURTEILBAR, fehlende Voraussetzung und kleinster nötiger Folgeauftrag. Keine Checkliste, die Belege nur behauptet.

1. **Layout, Gültigkeitsmaske und Zeitvertrag über vier RX:** aktuelle Form-/Processing-/Boot-/Quellenbindung, passende Masken und monotone Epochen belegen. Uninstallierter RX2/RX3-TX-only-Metadatenkandidat ist keine gemessene Attestation. Genau ausweisen, was der geplante Paarlauf mit installiertem C/C1 bereits prüfen kann und wofür gegebenenfalls ein **eigener später geprüfter und freigegebener** RX2/RX3-Metadatenrollout erforderlich bleibt. Heute weder Firmware bearbeiten/bauen noch Installation ausführen oder still in die Zwei-POST-Paarplanung aufnehmen.
2. **Vollständige Fehler-/Ressourcenbilanz:** tatsächlich exportierbare Transport-/Formatausschluss-/Sequenz-/Socket-/Server-/Queue-/Writerbelege mit ihren Nennern führen; reale TX-/RX-/Hostressourcen und Firmwarealter nur soweit wirklich beobachtbar. Die vorhandenen Antwortschemas auf nutzbare Felder prüfen und als Messquelle binden. Fehlende CPU-/Heap-/Stack-/Firmwarezeitwerte nicht aus Softwaregrenzen, Quelltests oder einem ACK ableiten. Wenn das vollständige Kriterium sie verlangt und sie fehlen, bleibt dieser Punkt offen. Ressourcenlast des eigenen Offline-Testprozesses ist kein Gerätewert.
3. **RX1↔RX4-Funkvergleich:** zwei gerichtete Links, gültige physische Formen, feste 480-/960-s-Fenster, vorhandene Qualitätsgrenzen, vier TX1-Kontrolllinks und definierter Endzustand. Paarfunk-/Transportteilurteil, physische Kalibrierung, Firmwarefrische und Positionsnutzen getrennt ausgeben. Ein reiner Funkvergleich benötigt keine Radar-G2-Referenz; Positionsnutzen/gelabelte Wege später schon.

Vor dem späteren Hardwareauftrag eindeutig beantworten: **Welche dieser drei Zeilen kann genau dieser freizugebende Lauf schließen, welche nicht und warum?** Verbleibende Lücken dürfen keinen sinnvollen getrennten Funk-/Transportnachweis verschwinden lassen; sie dürfen aber keinen pauschalen „14. fertig / 5 von 5“-Befund erzeugen. Keine Abnahmekriterien still abschwächen. Keine neue allgemeine Firmwareforschung beginnen.

## D – Regressionen, Builds und Messübergabe

Betroffene Tests vollständig ausführen. Die bisherigen 34 Kernprüfungen, 26 Adapterszenarien und zehn Recorderregressionen mit 292 Szenariodurchläufen in ihrer fachlichen Abdeckung erhalten. Bewusst versionsabhängige GET-/Stepzahlen dürfen an den ausdrücklich neuen Vertrag angepasst werden; keine Sicherheits-/Abbruch-/Fristtests entfernen, um die neue Planung passend zu machen. Alter Vertrag und dessen Gegenbelege bleiben unverändert.

Zusätzlich testen:

- Tragfähiger Dateietat bei voller vorgesehener Dauer und Rate, exakte Grenzfälle, deterministischer Report, keine unbemerkte Datenauslassung.
- Vollständige frische Vorklammern aller vier RX vor beiden Aufnahmen; fehlende/typfalsche Diagnose, falsche Quelle/Boot/Runtime, Configwechsel und veraltete Antwort sperren den jeweiligen nächsten Schritt. Keine Wiederverwendung einer Nachklammer als Vorbeleg.
- Neue genaue Normalzahl und maximaler GET-Verbrauch einschließlich zwölf End-GETs; Worst-case-Timeouts, Peerhost-/Bootfristen, Abbruch nach Wartezeit, kein Dispatch nach Gesamtfrist, ausreichende Endreserve und kein weiterer Hostwechsel/Upload/Capture.
- Tatsächlicher neuer Adapter → Sessionkern → Fake-I/O mit endgültigen Kandidaten-/Profilbytes; anschließend tatsächlicher lokaler Produzent → geschlossene Dateien → neue Normalisierung → Parser → Bericht. Neue Vorklammern müssen in diesem Bericht tatsächlich ankommen.
- Geschlossene Layout-Rückkehr mit 2.274 wieder gutgeschriebenen Frames/470 von 473 Fenstern, TX1-128/LLTF64-Peerverträge mit je 1.920 Paketen/953 Fenstern, `frames_written`, Radar-Export und historische 1.786/1.793 bzw. 1.780/1.793 erhalten. Firmwarefrische ohne Gerätezeitbelege bleibt ausdrücklich unbekannt.

Nur betroffene Kandidaten neu bauen: Hoständerung erfordert neue gebundene Default-/Peerhostbinarys und betroffene Regressionen beider Varianten. Kompilierte Quellenänderung des reinen Exporters/Dateiproduzenten erfordert dessen passenden Neubau und neue separate Bindung. Kein SDK-/Firmwarebuild. Normale Hostbinarys und Liveadapter weiterhin gesperrt; ausschließlich gesonderte Datei-Testprogramme unter Prozess-Netzverbot verwenden.

**Wichtig bei Wiederholungen:** Rusttests können einen eincompilierten `CARGO_MANIFEST_DIR`-Ausgabepfad benutzen. Binarykopie plus TMPDIR genügt nicht immer. Testoutputs ausschließlich in neuer eigener Prüfablage erzeugen; keinen versiegelten Ursprungsordner durch neue Testdateien erweitern. Nötige kopierte Testquellen/Outputs richtig binden; alte Gesamteinstiege können Originalreceipts überschreiben und sind entsprechend anzupassen.

Lieferung in neuer privater Ablage: Bericht, Änderungen/Patches, Quell-/Input-/Kandidaten-/Test-/Abschlussmanifeste, Byte-/Last-/Reserveberechnung, neuer Sessionvertrag samt Dispatcher-/Abbruchtests, korrigierte Normalisierung, Produzentenoutputs/Berichte, Erhaltungsnachweis und `ABSCHLUSSPLAN-14.private.md`.

Zusätzlich einen **kopierfertigen, weiterhin gesperrten Vorschlag für den anschließenden RX-Hardwareauftrag** liefern: genaue Geräte/Reihenfolge, neue geplante GETs, unveränderte Höchstbudgets, erforderliche frische Bindungen, explizit getrennte Unterbrechungs-/Schreib-/Hostfreigaben, Datum/Geometrie-/Kandidatenbindung, Abbruch/Ende und erreichbare Abschlusszeilen. Kein eigener RX2/RX3-Rollout in diesem Vorschlag ohne klar getrennten neuen Umfang. Der Koordinator muss nach Review nur noch konkrete Freigaben und frische Liveparameter festlegen müssen, keine Grundsatzanalyse wiederholen.

Hardwarevorbereitung für Johann als kurze Anschlusscheckliste beifügen: TX1 und alle vier RX normal an ihren festen Positionen versorgen, Mac im bisherigen Projekt-WLAN; im vorgesehenen WLAN-/OTAablauf keine COM-/Mac-USB-Datenverbindung nötig. Physisches Bereitstellen ist kein laufender Agententest und keine Livefreigabe.

## Grenzen und Abschlussurteile

**Geräte-/Livezugriffe null**, auch bei schon eingeschalteter Hardware: keine seriellen/USB-/ROM-/Reset-/Flash-/OTA-/Restore-/Provisionierungsaktionen, kein echtes HTTP/curl/tcpdump, keine Projekt-API, keine ARP-/WLAN-/Routen-/produktive Prozess-/Socketinventur. Keine bestehenden Dienste starten/stoppen, keine echten Credentials lesen. Keine NVS-/Security-/Netz-/Radar-/Transformationsänderung. Keine externen Downloads, Installation oder neuen Abhängigkeiten. Cargo `--locked --offline`, Prozess-Netzverbot und vor Dispatch gesperrte echte I/O-Einstiege.

Produktarbeitsbaum, Originalquellen/-tests/-manifeste, Backups, Rohdaten, Setups und Kandidaten erhalten; kein Git-Reset/Clean/Staging/Commit/Push. Reale Identitäten/Configs/Rohdaten bleiben privat und aus Toolausgaben. Koordinatorplan, Tracker und Tageshäkchen nicht verändern. Keine Freigaben erteilen oder Livekopie vorweg entsperren.

Abgeschlossen erst bei tatsächlicher Umsetzung und bestandenem Gegencheck der finalen Lieferung. Getrennt berichten:

```text
RX_DATEIBUDGET: IMPLEMENTIERT_UND_UNTER_LAST_GEPRUEFT / KONKRETE_LUECKE
DIAGNOSE_VORKLAMMERN: IM_ABLAUF_IMPLEMENTIERT_UND_INTEGRIERT / KONKRETE_LUECKE
NEUER_ABLAUFVERTRAG: GET_NORMALZAHL=<N>; GET_MAX=120; END_GETS=12; WORST_CASE_SECONDS=<N>
RESTPFAD_UND_ENDBUDGET: BESTANDEN / NICHT_BESTANDEN
PRODUZENT_BIS_BERICHT: BESTANDEN / NICHT_BESTANDEN
ABSCHLUSSPLAN_14: DREI_PUNKTE_MIT_ERREICHBAREN_UND_OFFENEN_NACHWEISEN_VOLLSTAENDIG
RX_HARDWAREAUFTRAG: ZUR_KOORDINATORPRUEFUNG / NICHT_READY
FIRMWAREFRISCHE_UND_WEITERE_GERAETELUECKEN: EXPLIZIT_OFFEN
HARDWAREAKTIONEN: 0; INSTALLATION: KEINE; HOSTAKTIVIERUNG: KEINE
```

Genau ein nächster Schritt: unabhängiger Koordinatorreview dieser Offline-Lieferung und des konkreten RX-Hardwarevorschlags. Danach stoppen; weder bei angeschlossener Hardware noch aufgrund früherer Freigaben selbstständig messen.

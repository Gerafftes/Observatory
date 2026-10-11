# Lokale TX1-Bedienhilfen für spätere Hardwarefenster

Diese Hilfen bilden den öffentlichen, geheimnisfreien Werkzeugteil des
internen TX1-Plans. Der interne Plan bindet die privaten Live-Artefakte und ist
absichtlich nicht Teil des öffentlichen Source-Exports.
Import, Hilfetext und Trockenlauf öffnen keine Geräte. Jetzt wurden ausschließlich
Fixtures, SDK-Parser und lokale Builds benutzt. Eine angeschlossene Platine
und ein erfolgreiches `--help` sind keine Gerätefreigabe.

Alle späteren Eingaben/Ergebnisse liegen in `private/` (Verzeichnis 0700,
Dateien 0600, Git-Ausnahme prüfen; bereits versionierte Dateien separat prüfen).
Die Tools überschreiben keine Ergebnisdateien. Keine Passwörter oder Tokens
in CLI-Argumenten verwenden. Vor dem Offline-Fenster SDK, Python, esptool 4.12.0,
NVS-Generator 0.1.9, alle Artefakte und diese Anleitung lokal bereitlegen.
Keines der Mess-/USB-Tools lädt etwas herunter oder verwendet ein Modell.

## 1. Private Provisionierung, ausschließlich an einer Kopie

`provision.py` erwartet eine JSON-Datei mit genau diesen Feldern:

| Felder | Typ / Vertrag |
|---|---|
| `schema_v`, `node_id` | Ganzzahl 1, Text `TX1` |
| `ssid`, `password` | 1–32 / 8–63 UTF-8-Bytes; Migration `csi-test` und bestätigtes privates Passwort |
| `base_mac`, `ap_mac` | Sechs Hex-Oktette mit Doppelpunkten; separate nichtleere Unicast-Adressen; produktive Werte nur aus privatem Gerätebefund |
| `channel`, `rate_hz` | Ganzzahlen, Migration 6 / 50 |
| `bw_mhz`, `protocol`, `secondary` | 20 oder 40 MHz; IDF-b/g/n-Bitmaske 1–7; 0=kein, 1=oberer, 2=unterer Nebenkanal; konsistent bestätigen |
| `beacon_tu`, `country`, `country_nch` | 100–60000 TU in 100er-Schritten; zweistelliges Länderkürzel, Kanalbereich 1 bis N (höchstens 13) |
| `led_gpio`, `led_active_low` | -1 oder 0–48, Ganzzahl 0/1; vorhandene LED-Konfiguration erhalten |
| `ota_psk` | 64 kleine Hex-Zeichen, separates Geheimnis in `security` |

Es gibt keine produktiven MAC-/Passwort-/PHY-Defaults. Die künstlichen Werte in
Tests sind keine Provisionierungsvorlage. Alte PHY-/Leistungsbefunde sind noch
zu klären. Allgemeine Nachbauer verwenden eigene Identitäten und eigene Netze.

Der Import nimmt einen **0x5000-Byte-Auszug** der alten NVS als Eingabe. Er fügt
`tx_cfg`/`security` auf einer vollständig obsoleten Seite hinzu (alternativ bei
zwei freien Seiten auf einer davon), erhält alte lebende Einträge und schreibt
den Schema-Marker zuletzt. Er verschiebt keine lebenden Datensätze und führt
keine allgemeine Kompaktierung aus. Auch die entstehende Kopie ist privat.

```sh
python software/ruview/firmware/esp32-csi-tx/provision.py \
  --profile private/TX-RUN/profile.json \
  --existing-nvs private/TX-RUN/nvs-original.bin \
  --output private/TX-RUN/nvs-merged.bin --idf-path "$IDF_PATH"
```

Dieser Befehl schreibt nur eine neue lokale Datei. Fehlercode 2 verweigert den
Import. SDK-Lesbarkeit und alter App-Betrieb mit erweitertem NVS bleiben ein
separater Kompatibilitätsnachweis; CRC-/Eintragserhalt allein beweist sie nicht.

## 2. 7A/7B: Verbindung und RX4-Transportvergleich

Für die TX1-Abnahme bleibt das Mac-WLAN mit `csi-test` verbunden und der
Internet-Uplink läuft parallel über LAN. Die Tools wählen kein WLAN und setzen
keine DHCP-/Alias-/Routingwerte. Eine separat gestartete lesende Prüfung
kontrolliert CSI-Route, DHCP-Lease, Gateway-MAC, den vorhandenen einzelnen
`.50/32`-Alias, bekannte fremde `.50`-Claims und ob die Default-Route über ein
anderes Interface als CSI-WLAN läuft (`internet_route_is_separate`). Die
Messkonfiguration enthält weiterhin die beim eingefrorenen 7A erfassten
USB-Hotspot-Felder; `usb_hotspot_route` ist nur deren historische
Routendiagnose und kein aktueller Internetnachweis. Die Verbindungskontrolle
sendet selbst keinen Internet-Probeverkehr; Internet-Erreichbarkeit über das
LAN muss beim aktuellen Vorabcheck separat mit einem begrenzten HTTPS-Aufruf
belegt werden. Meldet macOS eine andere SSID, bricht die Prüfung ab. Ist die
SSID nicht offengelegt, darf sie nur dann als nicht beobachtbar notiert werden,
wenn DHCP-Netz, Gateway-Adresse und exakte erwartete Gateway-MAC, WLAN-Interface
und `.50/32`-Alias zusammenpassen. Die Messung bleibt bei fehlendem Internet
offline möglich.

Private Messkonfiguration als JSON mit genau diesen Feldern:

- `ap_mac`, `rx4_ip`, `rx4_image_sha256`: bestätigte Quelladresse, direkte RX4-IP,
  Digest aus RX4-Diagnose; kein DNS-Ziel.
- `server_port`, `server_binary_sha256`: vorhandener lokaler Server und sein
  BIN-Hash. Es muss genau ein passender Listener existieren.
- `internet_interface`, `internet_hardware_port`: im 7A-Vertrag gebundenes
  LAN-Interface und Hardware-Port. Die aktuelle Default-Route muss genau über
  diese Schnittstelle laufen und darf nicht auf das CSI-WLAN zeigen.
- `channel=6`, `rate_hz=50`, `old_manifest_sha256`: Netzvertrag und Hash des
  privaten Original-Sicherungsmanifests.
- `candidate_image_sha256`, `candidate_source_sha256`: `runtime_image_sha256`
  und `source_sha256` aus dem endgültigen Build-Receipt; schon vor 7A festlegen.
- `conditions_sha256`: Hash eines vorher geschriebenen privaten Zustandsprotokolls
  mit TX/RX4-Position, Stromversorgung, Clientbestand, Serverstartargumenten,
  Filter/Raster/Setup-Bezug und geklärten PHY-/Leistungsbefunden. Das Protokoll
  gehört zur Freigabeevidenz. Zwischen 7A/7B gelten dieselben Bedingungen.
- `criteria_sha256`, `provision_sha256`, `start_helper_sha256`: die im 7A-Vertrag
  gebundenen Hashes der festgelegten Kriterien, privaten Profil-Provenienz und
  Starthelfer-Provenienz. Sie werden nicht aus einer früheren Messung erraten.

Die Konfiguration wird gehasht und in Ergebnissen/Grenzen gebunden. Die
Live-Messung löst RX4 vor dem Warm-up über den frischen Node-4-Transport auf
und prüft am selben direkten Ziel die erwartete Firmware, Boot-ID und Bindung.
Eine neue DHCP-Adresse ersetzt nur die Laufzeitadresse für Statusabfragen und
Paketauswertung; die eingefrorene Konfiguration und ihre Vergleichsgrenzen
bleiben unverändert. Die Auflösung steht im privaten Ergebnis. Innerhalb des
180-Sekunden-Vorlaufs müssen 20 Sekunden mit stabiler Boot-ID, frischen Daten
und fortlaufend akzeptierten 128er-Frames erreicht werden. Nach dem Vorlauf
führen Adress- oder Identitätswechsel weiterhin zum Abbruch.

Die reale USB-Planvorbereitung akzeptiert keine Fixture-Baseline. Zur Verbindungskontrolle
(später, nach bewusstem WLAN-Wechsel und eigener Startentscheidung):

```sh
python software/ruview/firmware/esp32-csi-tx/tools/tx1_measure.py \
  --config private/TX-RUN/measurement.json --phase connection \
  --output private/TX-CONNECTION-UNIQUE --start-observation
```

Für 7A dieselben Optionen mit `--phase before`, für 7B `--phase after` und
zusätzlich `--limits private/TX-7A/limits.json --limits-sha256 FREIGEGEBENER_HASH`.
Hat die eingefrorene 7A-Konfiguration den vollständigen Datei-SHA des Kandidaten
gebunden, zusätzlich `--plan private/TX-RELEASE/plan.json` angeben. Das Werkzeug
validiert den vollständigen USB-Plan samt Kandidat, Sicherungen und der
abgeleiteten Runtime-Bindung; Konfiguration und Grenzen müssen bytegenau den
Planartefakten entsprechen. Nur für die Firmwarestatus-Prüfung verwendet es
den daraus verifizierten Runtime-Image-Digest. Ergebnis und Grenzvergleich
behalten die ursprüngliche 7A-Konfigurationsidentität; der Planhash und der
Runtime-Digest werden zusätzlich protokolliert. Die historischen Dateien und
numerischen Grenzen werden nicht umgeschrieben.
Ohne `--start-observation` verweigert das Tool reale Beobachtung. `--fixture`
verwendet ausschließlich eine lokale JSON-Fixture und wartet keine echten
Messzeiten ab. Fixture-Ergebnisse sind ausdrücklich als solche markiert.

**Vor jedem ersten Flash muss 7A vollständig erfolgreich sein.** Kein neuer
Frame bedeutet keine Baseline. Der Ablauf hat maximal 180 s Bereitschaft,
60 s Aufwärmen und drei unmittelbar aufeinanderfolgende 120-s-Fenster mit
geteilten Grenzsnapshots. Abschlussbudget 30 s; absolute Begrenzung 630 s
(einschließlich reservierter Zeit für das Beenden des eigenen Mitschnitts).
Serverdiagnose jede Sekunde, RX-/TX-Diagnose alle fünf Sekunden, HTTP je
1 s Connect-/2 s Gesamtlimit, keine Retryketten. 30 s ohne ersten Fortschritt,
Frische ab 2 s, Identitäts-/Rasterwechsel, Counter-Reset und zwei aufeinanderfolgende
fehlende HTTP-Samples brechen ab. Ein fehlender Grenzwert, ein fehlendes Sample,
Mitschnittverlust oder unaufgeklärter Wrap ergibt **unklar**. Uhrsprünge >50 ms
zwischen monotoner Fensterdauer und PCAP-Wanduhr machen die Zuordnung unklar;
120-s-Dauer darf durch die Abfragegrenzen höchstens 1 s abweichen. Dies sind
Erfassungsprüfungen, keine erfundenen Funktoleranzen.

Der Vergleich bildet den eingefrorenen Mixed-V2-Vertrag ab: gültige RX4-CSI-
Pakete mit 64 und 128 Subcarriern werden getrennt gezählt und vollständig gegen
Raster, Header, Länge, Quellbindung und Boot-ID geprüft. `skipped_grid_frames`
darf nicht pauschal null sein; sein Zuwachs muss exakt innerhalb der per
HTTP-Anfragezeit eingeklammerten Zahl gültiger 64er-Frames liegen. Der
angenommene 128er-Serverpfad muss auch im Warm-up und zwischen Samples
mindestens alle zwei Sekunden Fortschritt zeigen. `rx4_max_gap` bleibt die
eigenständige PCAP-Lücke des 128er-Pfads; Intervalle aller gültigen CSI-Pakete
werden zusätzlich diagnostisch erfasst. Die globalen `/health/metrics`
Eingangszähler werden als monotone Fensterdifferenzen protokolliert, ändern aber
keine eingefrorene 7A-Vergleichsgrenze.

Der einzige gestartete Prozess ist ein begrenzter eigener `tcpdump`, ohne
Monitor Mode, mit Filter für UDP 4210/5005. Vorhandene BPF-Leseberechtigung ist
Voraussetzung; das Tool eskaliert keine Rechte. Der vorhandene Server bleibt
unangetastet. Fehlt er, stoppt das Tool. Dann ist **vor** einem neuen Messlauf
ein gesondert genehmigter lokaler Serverstart auf freiem Port mit dokumentierten
Argumenten erforderlich; bestehende Listener dürfen weder beendet noch ersetzt
werden. Es wird kein temporärer zweiter UDP-Empfänger gebunden.

Private Ausgaben: Readiness-/Warm-up-Samples, fortlaufendes `samples.jsonl`,
`capture.pcap`, `capture.log`, Fenster-Snapshots und Paket-Audit, partielle
Fehlersnapshots, `observations.json`, `result.json` und bei 7A `limits.json`.
Erfolg benötigt drei
vollständige Fenster und null vom Kernel gemeldete Mitschnittverluste.
Sounding-Format, Quell-MAC, v2-Boot-ID, RX-Bindung, RX-Boot und Image werden geprüft.
Sequenz-/Zeit-/RSSI-/Queue-Snapshots bleiben nachvollziehbar. Fehlende alte
TX-Diagnose bleibt nicht verfügbar. Sendererfolg ist keine Zustellbestätigung.

Die Grenzen werden exakt aus Auftrag 4 erzeugt: Minimum der drei alten
Sounding-/RX4-Raten; Maximum für p95, größte Lücken, Sequenzlückenquote und die
jeweiligen RX4-Fehlerzähler. Quote ausschließlich
`Δinferred_lost_frames / (Δsequence_observations + Δinferred_lost_frames)`, mit
positivem Nenner und ausgewiesenen Differenzen. Es wird keine zusätzliche
TX-Funkverlustquote erfunden. Vor 7B müssen Hash und konkrete Grenztabelle
feststehen. Neue TX-only-Zähler werden separat dokumentiert; ihnen fehlt eine
alte Vergleichsgröße. Fachlich gewünschte zusätzliche Toleranzen fehlen derzeit
und müssen vor der Hardwarephase separat begründet werden, falls benötigt.

Exitcodes: 0 = vollständige Beobachtung/Vergleich bestanden; bei `connection`
zusätzlich CSI-Identität und getrennte, an die Messkonfiguration gebundene
LAN-Internetroute bestätigt. 3 = benötigter Messpfad nicht verfügbar (für
`connection`: CSI-/LAN-Routen nicht wie gebunden), 4 = verweigert oder unklar,
5 = Vergleich nicht bestanden. Exitcode 2 ist reserviert. RX1–RX3 bleiben offen.
Diese Teilabnahme ersetzt weder 1TX/4RX-Abnahme noch wissenschaftliche
Messvalidierung.

## 3. Manifestgebundene USB-Migration und Rückweg

`tx1_usb.py capture` liest nach explizitem `--start` und passendem
`--approve-reference-sha256` zwei vollständige 16-MiB-Abbilder des eindeutig
identifizierten TX1. Beide Abbilder und ihre private Manifestdatei werden vor
dem Vergleich unter `private/` gespeichert. Die Prüfung vergleicht jedes
4-KiB-Segment mit der historischen Baseline. Bootloader, Partitionstabelle und
Original-app0 müssen bytegleich bleiben. Abweichungen in NVS, OTA-Daten, app1,
SPIFFS oder Coredump werden nach Partition und Sektor erfasst; unbekannte
Bereiche bleiben bis zur Klassifikation gesperrt. Nicht übereinstimmende
Vollabbilder oder veränderte unveränderliche Bereiche erzeugen keinen Plan.

Standardmäßig startet nach dem Capture wieder die alte Anwendung. Soll im
ausdrücklich freigegebenen Migrationsfenster unmittelbar danach ein Plan
vorbereitet und geschrieben werden, verhindert `--hold-in-bootloader` diesen
Zwischenstart und damit dessen mögliche NVS-Änderungen. Auch ein fehlgeschlagener
Capture startet mit dieser Option die Anwendung nicht automatisch. Der
Betrieb bleibt unterbrochen, bis ein gesondert freigegebener Start erfolgt.
Das Manifest protokolliert die Option und ob ein Boot angefordert wurde;
beide Vollreads und der spätere strenge Pre-Write-Vergleich bleiben Pflicht.
Capture-Fortschritt erscheint blockweise als sofort geflushte
`usb_progress`-Zeilen und gehört in ein privates Log.

Ein abgebrochener Capture behält ein privates `failure.json` mit begrenztem
Phasenbefund ohne Gerätekennung. Während der zwei Reads enthält es die
Readback-Nummer, den nächsten 256-KiB-Block und die Zahl abgeschlossener
Blöcke; dadurch lässt sich ein USB-Abbruch dem Stub-/Baudwechsel oder einem
konkreten Flash-Leseabschnitt zuordnen. Der Bericht ist Diagnose, keine
Freigabe zum automatischen Wiederholen.

`tx1_usb.py prepare` bleibt offline. Es nimmt `--backup-manifest` für die
frische doppelte Pre-Migration-Sicherung und `--historical-backup-manifest` für
die unveränderte Referenz der bestandenen 7A-Baseline entgegen. Dazu kommen
`--build-receipt`, `--profile`, `--limits`, `--baseline`, `--idf-path` und
`--output`. Wenn das unveränderte 7A-Ergebnis die Kandidatenidentität als
vollständigen Datei-SHA statt als angehängten Runtime-Image-Digest speichert,
kommt `--baseline-binding private/PFAD/baseline-runtime-binding.json` hinzu.
Diese streng gebundene Ableitung prüft Originalergebnis, Grenzen, Messkonfiguration,
Build-Receipt und beide Image-Digests; sie ändert weder Messwerte noch Grenzen
und erzeugt keine neue Baseline. Der Plan bindet die Ableitung und Konfiguration
mit eigenen Artefakthashes. Das additive NVS-Merge startet aus dem aktuellen
Readback; die 7A-Grenzen und deren Bindung an die historische Sicherung bleiben
erhalten. Das Werkzeug prüft Tabellen-MD5, beide Sicherungsabbilder,
S3-Image/Checksumme/Digest/Secure-Version, die Sektorvergleichsdatei und alle
Artefakt-Hashes. Es erzeugt ein privates `plan.json`, ein erhaltend ergänztes
NVS-Abbild und die konkrete OTA-Auswahl. Offizielle SDK-Parser werden
wiederverwendet.

Das geplante Schreibset ist app1 → NVS → OTA-Daten. app0, Bootloader,
Partitionstabelle, SPIFFS, coredump und die übrigen Flashbereiche bleiben erhalten.
OTA-Kopie 0 (Sequenz 1) bleibt; Kopie 1 erhält Sequenz 2, Zustand UNDEFINED und
CRC. Ob der alte Bootloader diesen Vertrag trägt, ist ein externes Pflichtgate.
Es gibt keinen behaupteten automatischen TX-Rollback.

Der Standardstart von `run` ist immer offline, auch mit `--operation restore`:

```sh
python software/ruview/firmware/esp32-csi-tx/tools/tx1_usb.py run \
  --plan private/TX-PLAN/plan.json --operation migrate \
  --output private/TX-DRY-UNIQUE
```

Ein Gerätestart benötigt `--start`, `--approve-plan-sha256 HASH` und das an
diesem Plan gebundene `--clearance private/FREIGABE.json`. Das sind technische
Bindungs- und Ausführungsgates; sie ersetzen keine Auftragsgrenze. Das
Clearance-JSON enthält `plan_sha256`, `device_secure_version: 0` und unter
`evidence` genau diese fünf Artefaktdatensätze (je `path`, `size`, `sha256`):

1. `old_bootloader_image_compatibility`: alter Bootloader und beide App-Images,
   Header/Segmente/Revision/Sicherheitszustand geklärt.
2. `shared_data_backward_compatibility`: altes IDF-5.5.4-App-/NVS-/PHY-Verhalten
   nach dem Rückwechsel; read-only-Neustart des Kandidaten geklärt.
3. `old_runtime_identity_phy_power`: echte alte Netzbaseline und private
   Bedingungen/PHY-Werte/11-dBm-Referenz geklärt.
4. `ota_selection_undefined_supported`: genau die geplante Auswahl mit erhaltenem
   Bootloader belegt, kein Rückschluss aus RX4-VALID.
5. `manual_recovery_ready`: lokaler USB-Rückweg, Original, Versorgung, Bedienung
   und konkretes Ende bei Fehler vorab beschlossen.

Artefakthashes belegen Bindung und Unverändertheit; sie ersetzen nicht die
inhaltliche Prüfung dieser Nachweise durch den Freigebenden. Kein automatischer
Generator kann diese offenen Gerätebefunde zu einem grünen Gate erklären.

Nach explizitem Start: genau einen der beiden belegten TX1-Zugriffswege zuordnen:
historische USB-Brücke mit ihrer gespeicherten Kennung oder nativen Espressif-Port
`303a:1001` mit TX1-Factory-MAC als USB-Serienkennung. Unbekannte oder mehrdeutige
Pfade und belegte Ports verweigern; anschließend ROM-Chip/Factory-MAC/JEDEC prüfen,
gespeicherten Sicherheitsstatus feldgenau vergleichen. Nur bei `key_purposes`
gelten JSON-Liste und ROM-Tupel mit denselben sieben Ganzzahlen als gleich;
alle Werte und Flags bleiben vollständig geprüft. Fehlende oder unbekannte
Felder, ungültige Typen und abweichende Werte verweigern den Zugriff mit
konkretem Feldbefund. Anschließend nur nicht geheime BLOCK0-Sicherheitsfelder einschließlich
SECURE_VERSION lesen. Nur bei unverändertem, erlaubtem Zustand RAM-Stub starten.
Es gibt keine eFuse-, Verschlüsselungs- oder Sicherheits-Schreibbefehle.

Unmittelbar vor der Schreibschleife wird der aktuelle Vollreadback erneut in
der privaten Laufakte gespeichert. Er muss exakt dem frischen Restorepunkt
entsprechen. Bei Abweichung bleibt der vollständige Readback mit Feldbefund
erhalten und es erfolgt kein Schreiben. Jeder geschriebene Bereich wird
zurückgelesen; danach wird der gesamte Flash unabhängig verglichen und der
Post-Write-Readback gespeichert.

Rückweg: `--operation restore` mit dem selben Plan stellt den vollständigen
frischen 16-MiB-Restorepunkt wieder her und speichert den unabhängigen
Readback. Nur `--boot-after-verify` löst danach bewusst einen Start aus. Ein
automatischer, **einmaliger** Original-Restore nach einem Schreibfehler ist
ausschließlich mit zusätzlichem `--approve-restore-sha256 ORIGINAL_HASH` im
gebundenen Auftrag erlaubt. Nach Zeitbudgetüberschreitung erfolgt kein weiterer
Schreibversuch. Auch esptools impliziter Reconnect-/Reflash-Versuch ist
deaktiviert. Unbekannte Lage: stoppen, private Evidenz erhalten, nicht
improvisieren. Hardware-Transportabnahme erfolgt separat; USB-Readback ist noch
kein erfolgreiches WLAN oder alter Datenpfad nach Restore.

Maximal 3600 s insgesamt, je Vollreadback 1200 s, feste 256-KiB-Leseblöcke;
serielle Streaming-Leseoperationen haben zusätzlich ein 3-Sekunden-Zeitlimit.
`run` schreibt sofort geflushte `usb_progress`-Zeilen in das private USB-Log:
Identitätsprüfung, Lesestufe, nächste Blockadresse, abgeschlossene Blöcke,
Schreibbeginn und vollständige Verifikation. Ein stilles oder früher
gepuffertes Log allein belegt keinen Stillstand; vor einem Abbruch müssen
Prozesszustand und tatsächliche Lesestufe geprüft werden.
absolute Uhr begrenzt auch blockierende SDK-Aufrufe. Diese Grenzen sind aus dem
früheren langsamen USB-Pfad großzügig gewählt, keine Geschwindigkeitsgarantie.
Bei Ablauf bleibt das Gerät gegebenenfalls im Recoveryzustand. Exitcodes:
0 = offline geprüft bzw. ausdrücklich ausgeführter Schreib-/Leseschritt bestätigt,
2 = vor Geräteöffnung verweigert, 5 = gestoppt/fehlgeschlagen, 6 = einmaliger
vorab genehmigter Original-Restore verifiziert. Resultate und SDK-Logs bleiben
privat. 0/6 behaupten keinen Boot-, LED-, WLAN-, RX- oder OTA-Hardwareerfolg.

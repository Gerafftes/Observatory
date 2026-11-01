<!-- Publication copy: local paths and hardware identifiers were pseudonymized; private original retained. -->
# Gemeinsame RX-Firmware – Umsetzungsnachweis

**Abschließender Livestand:** Alle vier RX sind installiert; Tag14 ist am
Peerblocker nicht bestanden. Die TX-only-/Default-Rückkehr ist abgeschlossen.
Siehe [Ergebnisbericht](rx-unified-tag14-result-20261027.md).

**Historischer Nachtrag aus dem Hardwarepilot:** Der erste Kandidat wurde einmal korrekt auf
RX2 installiert, bestand aber die Datenvorprüfung nicht. MGMT+DATA-Erfassung
und der bisherige 20-ms-Verkehrsgeber waren beim Zusammenführen ausgelassen.
Ein korrigierter Kandidat ist offline geprüft und zweimal reproduziert. Nach
zusätzlicher Kandidatenfreigabe bestand er RX2s echten Q900-Pilot; die bedingte
Fortsetzung läuft. Maßgeblich ist der
[Pilotbefund mit Korrektur und verbrauchtem Schreibanspruch](rx-unified-pilot-20261027.md).
Die folgende ursprüngliche Softwarelieferung bleibt ein historischer Stand;
sie ist keine bestandene Pilot-/Tag14-Abnahme.

Maßgeblich ist der [übernommene vollständige Plan](rx-unified-plan-20261027.md).
Die Fortsetzung von C (`01a178ce-7042-7e72-b7e2-f3bf15a1da21`) wurde lokal
umgesetzt. Ponytail Ultra; keine weiteren Agenten oder Chats. Die
[konkrete Tag14-Anweisung](tag14-mit-rx-unified-20261027.md) beschreibt die
spätere Installation, die erste mögliche Messung und die verbleibende Abnahme.

## Ergebnis und Grenze

Die beauftragte lokale Softwareumsetzung ist abgeschlossen. Ein gemeinsamer
reproduzierbarer S3-Appkandidat, passende native Host-/Controlprogramme,
Provisionierung und Mess-/Dateiauswertungshelfer sind vorbereitet und geprüft.
**Geräteinstallation, reale Tag14-Abnahme und Firmware-Freeze sind offen.**

Es gab keinen Gerätezugriff, seriellen Zugriff, Reset, OTA, Flash, produktiven
Hostwechsel, Netzwechsel oder Messstart. TX1, Radar, frühere Run10-/R-/C-Belege,
Claims, Backups und Stops bleiben erhalten. G/Gesamt bleibt pausiert. Die
Selbstprüfung ist keine unabhängige Fremdprüfung oder Hardwarequalifikation.

## Abgleich mit dem beauftragten Plan

| Paket | Umgesetzt und geprüft | Verbleibender realer Nachweis |
|---|---|---|
| P0.1 gemeinsamer Build | `rx_release`, gemeinsame `rx_sources.cmake`, eigenes S3-Profil, `led_strip` 3.0.3, gebundenes sauberes IDF 5.4; zwei bytegleiche Builds | Installation und Boot auf vier tatsächlichen Geräten |
| P0.2 Gerätekonfiguration | ID 1–4, Boardpin, TX-MAC, WLAN/Ziel, bis zu drei Peers und Profil; komplette Anfrage vor Write; physische Geräte-ID und erwartete Generation | Aktuelle Geräteinventur und passende aktive Konfiguration |
| P0.3 NVS-Erhalt | Ein versionierter eigener Blob, CRC, Generation; lesende Legacy-Migration; keine automatische NVS-Löschung, keine Änderung fremder Schlüssel/Geheimnisse | Gerätespezifische Sicherung, Readback und Bootbelege; keine pauschale Stromausfallgarantie aus Mocks |
| P0.4 Reconnect/Ressourcen | Ein Zustandsautomat; GOT_IP-Vorrang, Timer-/Startfehler, keine überlappenden Versuche; Socket-/ENOMEM-Pfad, Teilinitialisierungs-Cleanup, begrenzter RAM-Ereignisring | AP-/Hostausfall und Ressourcenreserven am Gerät |
| P0.5 CSI-Pfad | Ein achtteiliger Pool; begrenzte Prüfung/Kopie im Callback, Quoten nach Herkunftsfilter, reservierte Peerplätze, Stopbarriere auch für COPYING, atomare Terminalbilanz | Tatsächliche Quellen, Transport-/Host-/PCAP-Bilanz und Zeitklammer |
| P1 Peerfähigkeit | TX-only, Paar-/Mesh-Versuch und getrennte Dauerprofile; 900-s-Versuch ab Boot; bei 50 Hz 40 TX / zehn insgesamt Peer, faire 4/3/3-Restverteilung | Paarlauf; alle zwölf gerichteten Peerlinks benötigen zusätzlich eine eigene Meshabnahme |
| P1 Messvertrag | Wire v4 mit Boot-/Geräte-/Callback-/Queue-/Sendezeit, Generation/Profil/Quell-/Empfängeridentität; expliziter A063-LTF-Trailer und Maske; IQ unverändert | Tatsächlicher Nutzen/Qualität der Daten; Gain und Clipping bleiben unbekannt |
| P1 Host/Recorder/Replay | Setup v6 bindet vier Profile und Geräte; Rohdaten aller erlaubten Links/Grids, getrennte primäre Modellprojektion; Epochengrenzen, Sendersequenzen, verlustfreies Replay; keine Peer-Radar-/Positionsgutschrift | Reale Aufnahme, Qualifikation, Zeitbindung und geeignete neue Kalibrierung |
| P1 Diagnose/Bedienung | Einheitliche Ressourcen-/Reconnect-/Linkdiagnose, Identitätsfarbe und Gesundheitszustand; aktive/gespeicherte Konfiguration; authentisierte Abfragen; gerätegebundene RAM-Messlease mit Besitzer | LED/Boardpin, Laufzeitreserven und geordneter Betrieb vor Ort |
| P1 Wartung | Raw-Bearer-OTA `/ota`, Größe/Slot/VALID/Messsperre, begrenztes Streaming, kein POST-Retry, quieszenter Transport, Boot-Selbsttest; Control prüft tatsächlichen neuen Boot/Image-Digest | Installation/Readback/Boot auf genau gebundenem Gerät; keine positive Verifikation aus Dateihash |
| P2 Provisionierung | Eine Python-3.14-kompatible Implementierung, kompatibler alter Einstieg; vorhandene Geräte per authentisierter ID-/Generationsbindung, ohne Portcache/Voll-NVS-Update | Aktueller gültiger Alt-NVS-Zustand; gesperrte Altgeräte brauchen einen eigenen Korrekturauftrag |
| P2 Installer/Altmodule | `make rx-release RX_OUTPUT=...`, build-only `install.sh --profile observatory-rx --rx-output ...`; Legacy-Aufrufer bleiben erhalten; WASM/Swarm/Edge-DSP/Radar/C6 außerhalb des gemeinsamen App-Pfads | Keine Bereinigung historischer Quellen/Backups abgeleitet |
| Einführung/Tag14 | Begrenzter Messhelfer, Q900-/Paarbereitschaft, feste Frist, eigene Recorder-/Lease-/PCAP-Schließung; nativer deterministischer Fensterauditor und PCAP-Abgleich; konkrete Anleitung | Neue getrennte Betriebs-/Schreibrechte, Pilot, vier Geräte, Short/Basis/Paar, Default, Wiederanlauf, mindestens 60 Minuten Dauerlauf |

Die neue LTF-Beschreibung folgt dem vorhandenen eingefrorenen A063-Vertrag;
kein reserviertes Altbyte wird umgedeutet. Für HT20-TX mit 128 IQ-Paaren sind
108 beziehungsweise bei ungültigem Präfix 107 Bins geeignet; für Non-HT-Peer
mit 64 Paaren 52 beziehungsweise 51. DC/Guard/Präfix bleiben im Roh-IQ enthalten
und werden nur durch die separate Maske ausgeschlossen. Unbekannte physische
Kombinationen bleiben diagnostisch erkennbar und erhalten keine Messgutschrift.
Standard v2/v3 bleibt lesbar. Private frühere Zusatzformate behalten ihre
unveränderten eingefrorenen Leser; daraus folgt keine generelle Kompatibilität
jedes alten Experimentalformats mit dem neuen Standardhost.

## Ausgeführte Selbstprüfung

Alle neuen Ergebnisse, Compiler-/Testcaches und temporären Daten liegen extern:
`PRIVATE_EVIDENCE/rx-unified-offline-20261027-run01/` und `run02/`.
Maßgeblich für den Schlussstand ist **run02**.

| Prüfung | Tatsächliches Ergebnis / Beleg unter run02 |
|---|---|
| Gesamter nativer Host ohne Standardfeatures | `host-control-final.log`: 548 bestanden, null fehlgeschlagen, sechs explizit ausgelassen |
| Controlprogramm einschließlich Datei-/OTA-Verifikation und echtem lokalem HTTP-Test | `control-delivery-final.log`: 22 bestanden, null fehlgeschlagen; ausschließlich lokaler Testserver |
| Python-Provisionierung, Builder, Messhelfer und PCAP-Parser | `python-delivery-final.log`: 59 bestanden; fehlerhafte Zustandsdateien und Mini-Images sind künstliche Negativ-/Mockfixtures |
| Gesamte C-Hostsuite | `c-tests-physical-final.log`: bestanden; bestehende ADR-/Vital-/mmWave-/Quellen-/Identitätstests und neue Produktionsmodule |
| Produktions-C mit ASan/UBSan | Profile/NVS/HTTP, echte Callback-/Pool-/Wire-/Senderpfade, Thread-Stoprennen, Reconnect-/Timer-/GOT_IP-Rennen, OTA-Fehlerpfade, Teilstart-Cleanup bestanden |
| C→Rust-Golden | `csi-v4-physical.golden`, 460 B, vom tatsächlichen C-Pfad erzeugt; Rust prüft unverändertes IQ, Identität, Timing, LTF-Trailer, CRC, Präfix und Roundtrip |
| Native Regeln und Gegenfälle | Basis/Paar durch tatsächlichen Decoder; fehlende Peerrichtung, verspätetes Ende, falscher Hash, Boot-/Profil-/Quellenwechsel und falsche Grids werden nicht freigegeben |
| Schlussprüfung des korrigierten synthetischen Lastgenerators | `native-audit-final.log`: zwei Audit-Tests bestanden, ein Exporttest absichtlich ausgelassen; Export separat bestanden |
| Optimierte Hostprogramme | `release-final-source.log`: erfolgreicher Release-Build von `sensing-server` und `ruview-control`; tatsächliche Binärhashes im Liefermanifest |
| S3-Build und Reproduktion | `release-physical/`: zwei unabhängige SDK-Builds, App bytegleich, angehängter SDK-Image-Digest geprüft |
| Installer/Beispieldatei | `bash -n software/ruview/install.sh`; JSON-Vorlage syntaktisch geprüft; Platzhalter absichtlich nicht ausführbar |
| Lieferabschluss | `delivery-final/DELIVERY.json` und `delivery-final/VERIFIED.json`: Quellarchiv, Binär-/Firmware-/Testbindungen und erneuter Bytevergleich; frühere Lieferansätze bleiben separat erhalten |

Fünf ausgelassene Hosttests brauchen ausdrücklich private historische CAD-/Seal-
Fixtures. Der sechste ist der absichtlich nicht automatisch schreibende Export
synthetischer Kapazitätsdateien. Dieser wurde separat mit explizitem externem
Ziel ausgeführt (`export-synthetic-capacity-complete.log`: ein Test bestanden). Es wird
kein aktueller PASS für die fünf privaten Tests behauptet.

Die ursprünglichen 3-s-Fensterregeln sind erhalten: Basis 156/173, Paar 264/293,
TX mindestens 5 Hz, beide Peerrichtungen mindestens 600 Frames / 2 Hz. Die
native Kapazitätsprobe verwendete **synthetische** Dateien bei voller konfigurierter
Rate: Basis 36000 Frames / 125152536 B, Paar 60000 Frames / 201600534 B.
Im Paar bleiben RX2/RX3 mit 50 Hz TX-only; RX1/RX4 haben je 40 Hz TX und
zehn Hz Peer. Der erste synthetische Lastgenerator setzte alle TX-Links im Paar
auf 40 Hz; das wurde nur im Testdatengenerator korrigiert und gezielt erneut
geprüft. Der tatsächliche Release-Binärhash blieb dabei unverändert.
Je zwei Auswertungen beider Dateien waren bytegleich und zusammen in
**7,91 s** innerhalb des 24-s-Budgets (`synthetic-capacity-complete/BENCHMARK.json`).
Der vorherige 20,99-s-Lauf mit 54000 Paarframes bleibt als Zwischenbeleg erhalten.
Das ist ein einzelner lokaler Softwareleistungsnachweis, keine Garantie für
beliebige reale Aufnahmen und kein Funk-/Tag14-PASS.

Jev wurde einmal für drei konkrete Lebenszyklusaussagen eingesetzt: Boot-only-
Konfiguration 0,85; Stop wartet auf COPYING 0,18; Start sperrt alten Worker während
Stop 0,23. Die beiden unklaren Lebenszyklusfragen wurden selbst am Quellpfad und
mit dem echten Threadtest geklärt und korrigiert. Jev hat weder Testresultate
noch Gerätefreigaben zertifiziert. Keine unabhängigen Agenten wurden gestartet.

Ein alter inkrementeller Rustcache erlitt zuvor einen Compilerfehler und später
einen Dateizugriffsfehler. Er blieb unverändert erhalten; die erfolgreichen
Schlussläufe verwendeten den neuen externen run02-Cache ohne Inkrementalmodus.
Daraus wird keine Ursache der historischen Quellleseauffälligkeiten abgeleitet.

Beim ersten abschließenden Vergleich der neuen Einzeldatei-Quellkopien wurde
eine Abweichung in `test_vitals_count_presence.c` gemeldet. Dieser erste
abweichende Lesepuffer wurde nicht gespeichert. Ein danach einmal begrenzter
Vergleich aller 1118 Einträge sicherte tatsächliche abweichende Kopiepuffer für
`adaptive_controller_decide.c`, `cog-person-count/src/inference.rs` und
`csi_layout.rs`; die gleichzeitig gelesenen Arbeitsquellen entsprachen jeweils
dem Manifest. Das erklärt den ersten Befund nicht. Sämtliche Einzelkopien und
Negativbelege bleiben unter `delivery/` und `delivery-source-integrity-01/`.

Als getrennte Lieferform wurde einmal ein neues komprimiertes Quellarchiv aus
den gebundenen Arbeitsquellen erstellt und anschließend jeder seiner 1118
Einträge gegen das Manifest verglichen (`delivery-archive/SOURCE-VERIFIED.json`).
Dieser Vergleich bestand. **Die Ursache der Einzelkopieabweichungen ist offen**;
der Archivvergleich ist kein Reparatur- oder Ursachenbeleg. Die fehlgeschlagenen
Einzelkopien gehören nicht zur freigegebenen Softwarelieferung. Vor einer
späteren Installation bleibt ein frischer gebundener Artefaktvergleich Pflicht.
Nach der beschriebenen Korrektur des Testdatengenerators wurde der endgültige
Quellstand erneut als neue Lieferung unter `delivery-final/` gebunden und
geprüft; die vorherigen Lieferformen wurden nicht überschrieben.

## Endgültiger Softwarekandidat

Pfad: `run02/release-physical/rx-unified.bin`, **903696 B** im **2097152-B-Slot**.

- App-Datei-SHA256: `e339fb0ad8c1c6c05affdf57acae263476580b1efd056f22fa8850f41778430f`
- Laufender Image-Digest: `4e3bc5c3f6dff62b492abf0fe8b8e513bc6f412a192cf58312e3d6df81d2f60c`
- Firmwarequellen-SHA256: `55ba3f7b8bc49e66a1c5bdd635c19f304a024bd605624edc882858148bc830ab`
- SDK-Commit: `67c1de1eebe095d554d281952fde63c16ee2dca0`
- Partitionsdatei-SHA256: `67222c257c0477501fd4002275638dc4262b34eb68235b8289fb1337054d322b`

`RELEASE.json`, `SOURCE-MANIFEST.json`, Konfiguration, Compilerhash, Lizenz,
Ressourcenbericht und beide Builds liegen daneben. Heap-Poisoning, Stack-Canary,
starker Compiler-Stackschutz und reproduzierbarer Build sind aktiv; HE-Tagging
ist ausdrücklich gesetzt. DIRAM: 141011 B belegt / 200749 B im Linkerbericht frei.
Der separate IRAM-Bereich ist nahezu vollständig belegt; dieser statische
Bericht wird nicht als gemessener Heap-/Stackreservebeleg ausgegeben. Das Image
initialisiert keine ausgeschlossenen DSP-/WASM-/Radar-/C6-Module.

Ein gleicher Apphash gilt für alle vier gültigen Gerätekonfigurationen; es gibt
keine vier getrennten Firmwarebuilds. Die Hardware-ID, das tatsächliche Layout,
physische Flashgröße und der aktive/inaktive Slot bleiben vor jedem späteren
Write individuell zu prüfen. Sicherheitszustand und Geräteschlüssel bleiben
unverändert. `installation_authorized=false`, `hardware_validated=false`.

Der erste Entwicklungskandidat unter run01 bleibt historische Zwischenlieferung:
901568 B, Apphash `d62f6239956032df0606e0dc3cca4bf395c3c848f37c6a3d36f13ad7585a941e`,
Quellhash `70675ce703d1feaf4eb9855600b6be13a77c98d418e89cdabcd18540e26768d8`.
Er ist überholt. Auch weitere Zwischen-/Fehlerausgaben wurden nicht überschrieben.

## Wiederholen und später fortsetzen

```sh
export IDF_TOOLS_PATH="$PWD/.toolchains/espressif-v5.4"
source .toolchains/esp-idf-v5.4/export.sh
python software/ruview/firmware/esp32-csi-node/build_rx_release.py \
  --output PRIVATE_EVIDENCE/NEUER_ORDNER
```

Der Builder baut ausschließlich offline zweimal, vergleicht Quellen/Snapshot und
Appbytes und stoppt bei Abweichung ohne automatische Wiederholung. Er öffnet kein
Gerät. `make -C software/ruview rx-release RX_OUTPUT=...` nutzt denselben Pfad.

Für den nächsten Auftrag die [Tag14-Anweisung](tag14-mit-rx-unified-20261027.md)
verwenden: neue getrennte Freigaben, Pilot, übrige RX einzeln, neue Vier-RX-Q900,
Short 30 s, Basis 180 s, Paar 300 s mit neuem Setup, zweifache Dateiauswertung und
geordnete Rückkehr zum neuen v4-Default. Erste Messung erst nach gültiger
Vier-RX-Qualifikation; kein bestimmter Messbeginn aus alten Belegen abgeleitet.
Die reale Abnahme bleibt ausdrücklich offen.
## Ergebnis der anschließenden Livekampagne

Die gemeinsame App ist auf allen vier RX installiert. Vier-RX-Q900, Short und
die ausdrücklich zusätzliche Basis bestanden; der Paarlauf scheiterte an
beiden fehlenden Peerlinks. Die Rückkehr zum neuen TX-only-Default ist mit
vier aktiven RX über mehr als 20 s belegt. Vollständiges Tag14 und Firmware-
Freeze sind nicht erreicht. Der [Ergebnisbericht](rx-unified-tag14-result-20261027.md)
trennt Livebefunde, lokale Korrekturen und offene Ursache. Die vorstehenden
Software-/Pilotstände bleiben historische Nachweise.


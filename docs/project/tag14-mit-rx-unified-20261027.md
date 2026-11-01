<!-- Publication copy: local paths and hardware identifiers were pseudonymized; private original retained. -->
# Tag 14 mit der gemeinsamen RX-Firmware

**Ausgeführtes Ergebnis vom 27.10.2026:** Installation, Vier-RX-Q900, Short,
zusätzliche Basis und TX-only-/Default-Rückkehr sind belegt. Der Paarlauf ist
wegen beider fehlender Peerlinks nicht bestanden; Tag14 bleibt offen.
Siehe [abschließenden Ergebnisbericht](rx-unified-tag14-result-20261027.md).
Diese Anleitung bleibt der Sollablauf und erteilt keine weiteren Rechte.

**Aktueller Stand nach dem korrigierten Pilot:** Die erste RX2-Datenvorprüfung
war fehlgeschlagen. Nach ausdrücklicher B-Ergänzung wurde der korrigierte Kandidat
installiert und bestand RX2s Q900 samt physischer grüner LED-Bestätigung.
Die bedingte Fortsetzung mit den anderen RX ist damit freigegeben. Der gemeinsame Kandidat mit
App-SHA256 `18c54935a5b99a147755e59cce1aeb471287f9687c9fda37eb0891ae948f1fe8`
ist an diese zusätzliche Freigabe gebunden. Siehe
[Pilotbefund und nächster Schreibumfang](rx-unified-pilot-20261027.md).
Das bereits festgehaltene Gesamtende bleibt 27.10.2026, 23:10 Uhr Europe/Berlin.
Die folgenden Abschnitte beschreiben den vorbereiteten Sollablauf, keine
erneute Freigabe oder einen bestandenen Pilot.

Dieser Ablauf gehört zum [RX-Plan](rx-unified-plan-20261027.md) und zum
[Umsetzungsnachweis](rx-unified-implementation-20261027.md). Er ist eine
vorbereitete Arbeitsanweisung, keine Geräte-, Schreib- oder Messfreigabe.
Tag 14 und der Firmware-Freeze sind erst nach den unten genannten realen
Nachweisen bestanden. G/Gesamt bleibt bis zu einem ausdrücklichen Auftrag pausiert.

## 1. Was jetzt bereitliegt

- Ein gemeinsames ESP32-S3-App-Image für RX1–RX4; Identität, LED-Pin,
  Zieladresse und Peerprofil werden konfiguriert.
- Ein dazu passender nativer Host mit Wire-v4-Decoder, physischer LTF-Maske,
  Quellen-/Profilbindung und vollständiger Rohaufzeichnung aller erlaubten Links.
- Authentisierte Konfiguration, Diagnose und OTA sowie ein begrenzter
  Messhelfer mit gerätegebundenen RAM-Messsperren und eigener PCAP-Schließung.
- Ein nativer Dateiauditor für die ursprünglichen Tag14-Fensterregeln.

Der korrigierte, durch die ausdrückliche B-Ergänzung freigegebene Kandidat liegt unter
`PRIVATE_EVIDENCE/rx-unified-live-20261027-run01/producer-fix/release-candidate-v2/`.
`RELEASE.json` bindet App-Dateihash, laufenden ESP-Image-Digest, Quellmanifest,
SDK, Compiler, Konfiguration und Partitionsdatei. Diese beiden App-Hashes sind
unterschiedliche Größen: Für `/ota/status` gilt `image_sha256`, für die
Dateilieferung `app_sha256`. Vor einer Installation den abschließenden
Liefernachweis aus dem Umsetzungsbericht prüfen.

Die ursprüngliche Softwarelieferung liegt unter
`rx-unified-offline-20261027-run02/delivery-final/`; ihr erster Firmwarekandidat
ist durch den fehlgeschlagenen Pilot überholt. Der ursprüngliche Host bleibt
eine Wire-v4-Referenz. Für den tatsächlichen RX-only-Betrieb wurde ein neuer
Host mit deaktivierten mmWave-Netzdiensten und -Steuerrouten gebaut und lokal
geprüft; seine konkrete Bindung steht im Pilotbefund. Die neue Firmwarebindung
steht ebenfalls dort.
Frühere neue
Einzeldatei-Quellkopien unter `run02/delivery/` hatten Integritätsabweichungen
und dürfen nicht als geprüfte Lieferung verwendet werden. Das neue Quellarchiv
wurde separat gegen alle 1118 Manifesteinträge geprüft; die Ursache der älteren
Kopieabweichungen bleibt offen. Vor jeder späteren Installation die tatsächlich
zu verwendenden Appbytes frisch gegen den gebundenen Hash prüfen und bei jeder
Abweichung stoppen, ohne sie durch wiederholtes Lesen zu übergehen.

Standard-Wire v2/v3 bleibt lesbar; v4 ergänzt Identität und die explizite
32-Byte-LTF-Beschreibung. Historische private Timing-/Peer-Experimente behalten
ihre eingefrorenen Leser und Belege. Das ist keine Zusage, beliebige frühere
private Zusatztrailer mit dem neuen Standardhost auszuwerten. Die bisherigen
Run10-/NoFlash-Runner erwarten andere Geräteverträge und werden für diesen
Kandidaten nicht unverändert eingesetzt.

## 2. Der nächste Auftrag: Installation und Qualifikation

Es braucht einen neuen, zeitlich begrenzten Auftrag mit getrennten Rechten:

1. **Betriebsrecht:** benannte RX auslesen und nötigenfalls unterbrechen/resetten,
   den produktiven Default-Host kontrolliert durch den gebundenen v4-Host
   ersetzen, anschließend die benannten Testprofile und Messphasen ausführen.
2. **Schreibrecht:** pro konkret geprüftem RX genau einen an App-Hash, Gerät und
   Appslot gebundenen Installationswrite; zusätzlich einzeln benannte
   Konfigurationswrites für Basis, Paarprofil und Rückkehr zur Basis. Keine
   automatische Wiederholung, kein Restore, kein Voll-NVS-Write und keine
   Sicherheitsänderung. Unklarer Writeausgang bleibt ungeklärt, bis gelesen geprüft.

Beide Rechte müssen zum neuen Kandidaten passen. Alte C-/R-/Run10-Rechte,
verstrichene Fristen und RX4s alter verbrauchter Writeanspruch gelten nicht.
Ein gemeinsames absolutes Ende von höchstens sechs Stunden wird vor Beginn
festgehalten; Teilprogramme starten keine neue Sechs-Stunden-Frist.

**Vor Ort:** TX1 und die vier RX bleiben am bekannten Messplatz und im bestehenden
Netz. TX1 und Radar werden nicht umgebaut oder geflasht. Für den Pilot wird ein
zuverlässiges USB-Datenkabel zum Mac benötigt; die übrigen RX bleiben versorgt.
RX2 ist ein möglicher Pilot, sofern die aktuelle Inventur ihn dafür bestätigt.
Danach RX1, RX3 und RX4 einzeln bearbeiten; Kabel/Port und physische Gerätekennung
jedes Mal neu zuordnen. Nicht mehrere Geräte gleichzeitig beschreiben.

Für jedes Gerät vor dem Write tatsächlich prüfen und privat extern sichern:

- physische Identität, Chip, aktuelle App/Bootidentität, Sicherheitszustand;
- echte JEDEC-/Flashkapazität, aktuelle Partitionstabelle, aktiver/inaktiver
  Slot, Slotgröße und gebundener Zielbereich;
- die verlangten frischen Sicherungen und deren tatsächlichen Bytevergleich,
  einen konkreten Rückkehrweg und den späteren Readbackvergleich;
- bestehende NVS-Schlüssel und Geheimnisse, gültige Geräte-ID 1–4, TX-MAC,
  WLAN und unicast Hostziel, Kanal und tatsächlichen LED-Pin.

8-MiB-Imageheader, physische Flashkapazität und Partitionierung getrennt prüfen.
Insbesondere bei RX4 muss eine tatsächliche 16-MiB-Kompatibilität für dieses
Gerät und diesen Slot belegt werden; eine allgemeine „größer passt“-Regel genügt
nicht. Der App-Build ist kein Auftrag, Bootloader oder Partitionstabelle zu schreiben.
Die bisherigen Quelllese-/Archivauffälligkeiten bleiben als Negativbefunde erhalten.

Der neue Startpfad migriert alte Schlüssel nur lesend. Ein ungültiger vorhandener
RX-Blob, eine fehlende ID oder unbrauchbare Netzkonfiguration sperrt den Start.
Er löscht NVS nicht zur Reparatur. Der Legacy-LED-Standard ist GPIO48; ein
GPIO38-Board muss ausdrücklich passend konfiguriert werden. Bei gesperrtem
Altzustand zuerst einen gerätegebundenen Korrekturauftrag vorbereiten, statt
`--initial-provision` auf ein bestehendes Gerät anzuwenden.

Nach dem gebundenen Appwrite: tatsächlicher Readback, ein ausdrücklich erlaubter
Boot, laufender Image-Digest/Quellhash, aktiver gespeicherter Stand, Boot-ID,
Diagnose, LED und Ressourcen. Beim Fehler stoppen; keinen zweiten Appwrite aus
dieser Anleitung ableiten. Der Pilot muss seine eigene Qualifikation bestehen,
bevor das neue Image auf die übrigen Geräte kommt.

## 3. Konfiguration und Host binden

Basis auf allen vier RX: `mode=tx-only`, `rate_hz=50`, `peers=[]`; die eigene
Geräte-ID, TX-MAC, Ziel-IP/-Port und der Boardpin müssen stimmen. Jede Änderung
erzeugt eine neue Generation. Gespeichert und aktiv müssen nach dem ausdrücklich
erlaubten Neustart identisch sein. HTTP-Änderungen verwenden die vorhandene
Authentisierung und die physische `device_id`; keine Geheimnisse ins Repository.

Beispiel für einen **später freigegebenen** Basiswechsel, mit einem privat
vorbereiteten Patch und der tatsächlich gelesenen Generation:

```json
{"expected_generation":17,"mode":"tx-only","rate_hz":50,"peers":[]}
```

```sh
python software/ruview/firmware/esp32-csi-node/provision.py \
  --device-url "$RX_URL" --device-id "$RX_DEVICE_ID" \
  --ota-token-env BLL_RX_OTA_PSK --config-patch "$PRIVATE_PATCH"
```

Dieser Aufruf speichert; er aktiviert nicht selbst durch einen Reset. Der
Neustart ist ein eigener freigegebener Schritt. Nach unklarem POST wird kein
POST wiederholt; gespeicherten und aktiven Stand erst lesend feststellen.

Mess- **und** Default-Host müssen v4 lesen. Ein alter v3-Default ist nach dem
Firmwarewechsel kein gültiger Endzustand. Den produktiven Prozess nur innerhalb
des Betriebsrechts wechseln; keine Schnittstellen-/Adress-/Routingänderung
als Nebenwirkung. PID, tatsächliche ausführbare Datei und SHA256 binden.

Aus aktueller realer Geometrie und den authentisiert gelesenen Geräteangaben
eine Setup-Spezifikation v6 erstellen. Jeder Receiver erhält `rx_profile` mit
`generation`, numerischem `mode`, `rate_hz`, `config_sha256`, physischer
`device_id` und sortierten `peer_ids`. Alle vier Firmwareidentitäten binden
dasselbe Image und Wire v4. Das erwartete TX-Grid ist das qualifizierte HT20-
128er-Grid; das per Frame maskierte ungültige Präfix gehört nicht in dessen
stabile Layoutflags. Eine neue Profilgeneration braucht ein neues versiegeltes
Setup. Alte Kalibrierungen sind dadurch nicht automatisch weiter gültig.

```sh
"$SENSING_SERVER" --position-create-setup "$PRIVATE_SPEC" \
  --position-output "$NEW_SEALED_SETUP"
```

Die Startparameter anhand von `--help` und dem tatsächlich gewählten externen
Datenordner verwenden; kein altes Setup
durch Austausch seines Hashfeldes umetikettieren. Vor Messbeginn liefern
`/api/v1/system/build` und `/api/v1/system/clock` die tatsächliche Prozess- und
Uhrbindung. Der Messhelfer startet oder beendet keinen Serverprozess.

## 4. Wann die erste Messung beginnen kann

**Erst nach Installation und gültiger neuer Vier-RX-Qualifikation.** Im neuen
Basisboot müssen je RX zwei echte `VALID`-Belege im selben Boot und aktiven
Profil vorliegen. Zwischen Ende der ersten Antwort und Beginn der zweiten
Abfrage liegen mindestens 60,2 Sekunden. Erster Diagnosesnapshot bei Bootalter
800–850 s, letzter bei 900–920 s; Zielwerte des Helfers sind 810 und 915 s.
Bei verpasstem Fenster stoppt er, ohne selbst einen Reset auszulösen.

Zusätzlich: vollständige Zählerbilanz, wachsende qualifizierte CSI-Zähler,
interner Heap aktuell/minimal mindestens 32768 B, größter Block mindestens
16384 B, Stackreserven mindestens 1024 B, keine Allokationsfehler/Sättigung,
kein Konfigurationspending und exakt gebundenes Hostziel. Diese Gerätewerte
ersetzen noch keinen Quellen-/Layout-/Fensternachweis im Host.

Der Pilot benötigt dafür gut 15 Minuten ab gültigem Boot. Nach der einzelnen
Installation der übrigen RX ist eine gemeinsame frische Vier-RX-Basisqualifikation
am einfachsten; die vier Wartefenster laufen dann parallel und benötigen erneut
gut 15 Minuten. USB-Inventur, Sicherungen, Installation, erlaubte Neustarts und
Hostwechsel kommen dazu. Dafür gibt es ohne aktuelle Gerätebeobachtung keine
seriöse feste Uhrzeit.

## 5. Messfolge für Tag 14

| Schritt | Aufbau und feste Dauer | Erfolgskriterium / nächster Schritt |
|---|---|---|
| Short | Vier RX in Basis, 30 s Rohdaten; eigene PCAP mit je 10 s Vor-/Nachlauf | Quellen-, Layout-, Uhr-, Ressourcen- und Schließbelege vollständig; sonst stoppen |
| Basis | Dieselben Boots/Profile, 180 s | Vier TX-Links; je mindestens 156/173 gültige 3-s-Fenster und gemeinsame Abdeckung |
| Paar vorbereiten | Nur RX1/RX4: `pair-trial`, wechselseitige echte MAC, 50 Hz, neue Generation und ausdrücklich erlaubter Boot | Neues Setup; zwei gespreizte `VALID`-Belege über mindestens 60,2 s; RX2/RX3 behalten ihre Basisqualifikation |
| Paar | 300 s; RX1↔RX4 zusätzlich zu allen vier TX-Links | Je mindestens 264/293 Fenster; beide Peerrichtungen je mindestens 600 geeignete Frames und 2 Hz |
| Dateien auswerten | Basis und Paar zweimal mit demselben nativen Auditor | Gleiche deterministische Ergebnisse, gebundene Raw-/Metadatenhashes; gemeinsames natives Dateibudget 24 s nach ursprünglichem Vertrag |
| Rückkehr | RX1/RX4 wieder `tx-only`, neue Generation und erlaubter Boot; neues Default-Setup | Neuer v4-Default, mindestens 20 s beobachtet; alle eigenen Recorder/PCAP/Logger/Kinder beendet, Hashes und Endzustand belegt |

TX-Fenster verlangen mindestens 5 Hz, mindestens 15 geeignete Frames in 3 s,
mindestens 2,5 s Abdeckung und keine Lücke von mindestens 1 s. Peerfenster
entsprechend mindestens sechs Frames und 2 Hz. Die 3-s-Fenster beginnen ab
Sekunde 5; die Nenner sind 173 und 293. HT-TX-128 und Non-HT-Peer-64 haben
unterschiedliche LTF-Masken. Roh-IQ bleibt unverändert, DC/Guard/ungültiger
Präfix werden über die tatsächliche Maske ausgeschlossen.

Im Paarprofil gilt das ursprüngliche 900-s-Fenster ab Boot. Der Helfer verlangt
zusätzliche Reserve und ersetzt einen abgelaufenen Versuch nicht durch den
Dauerbetrieb. 40 Hz TX und insgesamt höchstens 10 Hz Peer sind Aufnahmebudgets;
sie beweisen keine RF-Verlustrate. Ein fehlender Link wird nicht durch Mehrdaten
eines anderen Links ausgeglichen. Die echte Host-Endklammer und die Unsicherheit
der Zuordnung zwischen Controller- und Hostuhr müssen die vorgegebenen 50 ms
einhalten. Die vollständigen Geräte-HTTP-Abfragen sind breitere Beobachtungen
und werden mit ihrer tatsächlichen Dauer separat ausgewiesen. Der ursprüngliche
Tag14-Uhrvertrag setzt deren gemeinsame Übertragungsdauer nicht auf 50 ms;
ein Fehlschlag der tatsächlichen Zeitgates bleibt ein Fehlschlag.

## 6. Vorbereitete Werkzeuge bedienen

Ein privater Messplan gemäß `observatory.rx-measurement-plan.v1` enthält Phase,
vier geordnete Geräte, deren tatsächliche URLs/IDs/Generation/Konfigurationshashes,
Modus/Peer-IDs, Token-Umgebungsnamen, Image-/Quellhash, tatsächliche Host-PID und
Binärhash, versiegeltes Setup im laufenden Host, externen Recorderpfad, eindeutige
Recording-ID, bestehende CSI-Zieladresse/-Port und ausdrücklich gewählte
PCAP-Schnittstelle. Vorlage: `rx-unified-measurement-plan.example.json`.
Die Platzhalter dürfen erst mit tatsächlich gelesenen Werten ersetzt werden.

```sh
# Offline; keine Geräte- oder Netzabfrage.
python software/ruview/scripts/rx_measurement.py plan --plan "$PRIVATE_PLAN"

# Erst im neuen freigegebenen Auftrag; dasselbe absolute Ende für alle Phasen.
python software/ruview/scripts/rx_measurement.py qualify \
  --plan "$PRIVATE_PLAN" --output "$NEW_Q_DIR" \
  --execute-authorized --deadline-unix "$AUTHORIZED_END_UNIX"
python software/ruview/scripts/rx_measurement.py collect \
  --plan "$PRIVATE_PLAN" --qualification "$Q_RECEIPT" --output "$NEW_PHASE_DIR" \
  --execute-authorized --deadline-unix "$AUTHORIZED_END_UNIX"
```

Basis und Short können dieselbe gültige Q900-Quittung verwenden. Für die
Paarqualifikation wird die Basisquittung zusätzlich mit `--qualification`
übergeben; daraus werden ausschließlich die unveränderten RX2/RX3 übernommen.
Jede Ausgabe bekommt einen neuen privaten externen Ordner. Keine Tokenwerte in
Shellhistorie oder Berichte schreiben. PCAP braucht bereits passende lokale
Capture-Rechte; der Helfer setzt keine Rechte und ruft kein `sudo` auf.

```sh
# Rein lesende Dateiauswertung; Ausgabe jeweils exklusiv neu anlegen.
set -C
"$SENSING_SERVER" --raw-csi-audit "$RAW_JSONL" --raw-csi-audit-seconds 180 \
  > "$NEW_NATIVE_REPORT"
python software/ruview/scripts/rx_pcap.py \
  --pcap "$PCAP" --close "$PCAP_CLOSE" --native "$NEW_NATIVE_REPORT" \
  --phase-receipt "$PHASE_RECEIPT" --output "$NEW_PCAP_REPORT"
```

Für das Paar `300` verwenden; native Auswertung für Basis/Paar wiederholen und
Ergebnisse samt tatsächlicher Gesamtzeit vergleichen. Die Reports lassen
`full_tag14_acceptance=false`, weil Dateien allein weder reale Ressourcen,
vollständige Zeit-/Firmware-/Hostbilanz noch Default-/Prozessabschluss beweisen.
Diese Belege müssen im Abschluss nebeneinander geprüft werden. PCAP-Lücken,
Host-Sequenzlücken und RF-Verlust sind unterschiedliche Aussagen.

## 7. Was nach Tag 14 für den Firmware-Freeze bleibt

Der begrenzte Messkern hat weiterhin 2100 s plus 150 s geordneten Abschluss als
Arbeitsbudget. Die neue Installation/Qualifikation und der zusätzliche
mindestens 60-minütige Stabilitätslauf sind darin nicht verschwunden. Zeitlich
zuerst Installation und Qualifikation einplanen, dann Short/Basis/Paar und
Rückkehr, danach den eigenen Dauerlauf. Unbekannte USB-/Fehlerzeiten erlauben
keine belastbare Gesamtzusage „in einer Stunde fertig“.

Den 60-minütigen Lauf ausdrücklich mit TX-only oder den getrennten
`pair-continuous`/`mesh-continuous`-Profilen planen; dafür die richtige neue
Setup-/Qualifikationsbindung verwenden. Vier-RX-Peerbetrieb benötigt zusätzlich
alle zwölf gerichteten Peerlinks, die reale faire Gesamtquote und eigene
Ressourcen-/Wiederanlaufnachweise. Ein Paarlauf nimmt diese Topologie nicht ab.

Erst wenn neue Vier-RX-Basis, Paarvergleich, Wiederanlauf und Dauerlauf real
bestanden sind, ist der Firmware-Freeze erreicht. Danach folgen Radar,
Datensammlung/Modelle, Blindvergleich und Raumwechsel gemäß Zeitplan. Ein
Positionsnutzen oder Zwei-Personen-Erfolg wird aus der Firmwarelieferung nicht
abgeleitet.

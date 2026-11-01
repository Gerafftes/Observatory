<!-- Publication copy: local paths and hardware identifiers were pseudonymized; private original retained. -->
# Agent-Auftrag: RX3-Rückweg klären und C1 als Einzelpilot prüfen

**Schwierigkeit: 4/5.** Der geprüfte Firmwarekandidat liegt vor; Gerätebindung, Rückweg und tatsächliche Formatwirkung müssen noch belegt werden. Kein Agent wurde gestartet.

**Freigabestatus: A und B sind für diesen konkreten einmaligen Auftrag erteilt.** Johann hat am 14. Oktober 2026 im Koordinatorchat ausdrücklich bestätigt: „Beide Freigaben A und B sind erteilt.“ Die Freigaben gelten ausschließlich im unten beschriebenen Umfang. Keine erneute Grundsatzbestätigung nötig; die technischen Gates bleiben zwingend. Noch keine Hardwareaktion ausgeführt und kein Agent gestartet.

Freigegebener Liveumfang: A höchstens 15 Minuten Unterbrechung, B höchstens 30 Minuten aktiver Pilotabschnitt; zusammen höchstens 45 Minuten. Insgesamt maximal 24 RX-GETs, ein App-OTA-POST und ein passiver 120-Sekunden-Capture. Keine manuelle Flash-/Restorefreigabe.

Den folgenden Block vollständig an den ausführenden Agenten übergeben:

```text
AUFTRAG: RX3-Rückwegprüfung und anschließend bedingter C1-Einzelpilot

Projekt: .

Bearbeite diesen begrenzten Auftrag vollständig:
0. Vorhandene Rückwegbelege und lokale Werkzeuge gezielt binden/testen.
A. Falls nötig im erteilten Umfang A: RX3-Bootloader/Partitionstabelle
   einmalig lesend prüfen und RX3 zum bisherigen Betrieb zurückführen.
B. Nur bei belegtem Rückweg im erteilten Umfang B: genau einen C1-App-OTA-
   Upload auf RX3 und einen 120-s-Vergleich mit unverändertem RX2 durchführen.

Keine weiteren Agenten starten, andere Chats anschreiben oder Automation anlegen.
TX1 und RX2 bleiben unverändert. RX1/RX4 nicht anfassen, Peerprofil nicht aktivieren.
Nach Abschlussbericht stoppen. Kein weiterer Empfänger und keine automatische Fortsetzung.

ZIEL

Das ist der Fehler:
G1 scheitert an der 128er-Unterdeckung von RX1/RX3/RX4. Der vollständige
Vier-RX-Lauf hatte 97/1.793 gemeinsame Fenster = 5,41 %. RX3 hatte 3,903
passende CSI/s, RX2 auf C 28,965/s und alle eigenen Auswahlfenster gültig.
Ein gemeinsames C1-Image ist inzwischen offline gebaut und geprüft.
Sein NVS-Fehlerpfad stoppt ohne Löschen vor WLAN/HTTP. Daher genügt ein
gedachter HTTP-Rückupload bei unerreichbarer App nicht als Rückweg.
Der tatsächlich installierte RX3-Bootloader-Rückweg ist noch nicht gebunden.

Das erwarte ich:
 - Einen RX3-spezifischen belegten Rückweg, keine Ableitung allein aus VALID,
   next_partition, Kandidatenflags oder einem historischen Sicherungsnamen.
 - Einen begrenzten Einzelpilot erst nach erfüllter Voraussetzung und Freigaben.
 - Exakte C1-/Boot-/Config-/Quellenbindung und echte passende CSI-Fenster.
 - Klare getrennte Urteile für Rückweg, Installation, Betrieb und Pilotqualität.
   Kein G1-PASS aus zwei RX und kein erfundener Funk-/Positionsnachweis.

ERTEILTE FREIGABEN FÜR DIESEN AUFTRAG

A ist ausdrücklich ERTEILT:
 Genau RX3, höchstens 15 Minuten Betriebsunterbrechung. Falls vorhandene
 Belege nicht genügen: ein geplanter ROM-Eintritt und ein geplanter normaler
 Rückkehrstart; ROM-Lesen ohne Stub bei 115200, maximal fünf initiale Sync-
 Versuche innerhalb dieses einen Eintritts, keine Baudwechsel/Resetretry.
 Nur Chip-/MAC-/zulässige Sicherheitsstatusbindung, Bootloaderimage und
 Partitionstabelle lesen, jeweils höchstens zwei übereinstimmende Lesungen.
 Kein persistentes Schreiben, Flashen, Restore, NVS-/eFuse-Dump oder Provisionieren.

B ist ausdrücklich ERTEILT:
 Genau RX3, höchstens 30 Minuten aktiver Pilotabschnitt einschließlich
 OTA-bedingtem Neustart. Ein authentisierter POST /ota ausschließlich mit
 dem unten gebundenen C1-Image in den frisch belegten inaktiven Appbereich.
 Vor-/Nachher-GETs und genau ein passiver 120-s-Capture RX3+Kontrolle RX2.
 Falls eine nachgewiesene automatische Rückkehr einen normalen Neustart
 benötigt: höchstens ein zusätzlicher kontrollierter RX3-Neustart dafür,
 nur innerhalb dieses Budgets und ohne weitere Geräte-/Flashaktionen.

Freigabequelle ist Johanns direkte Nachricht im Koordinatorchat am 14. Oktober
2026: „Beide Freigaben A und B sind erteilt. Gib mir bitte den aktualisierten
kopierfertigen Agent-Auftrag. Starte keinen Agenten. Welche hardware wird benötigt?“
Diese Freigabe betrifft genau den zuvor vollständig beschriebenen A-/B-Umfang.
Alte RX2-/Capture-/Offlinefreigaben sind keine zusätzliche Erlaubnis.
Nach erfüllten Gates selbständig weiterarbeiten, keine erneute Zustimmung
für bereits freigegebene Schritte verlangen. Freigabequelle und Umfang privat
dokumentieren. Fehlender Rückweg oder technische Bindung ist keine Freigabelücke:
in diesem Fall vor B stoppen, nicht die Schranke umgehen. Keine Dauerfreigabe.

0. OFFLINEVORBEREITUNG UND GEZIELTE EINGÄNGE

Arbeitsbaum und wirksame AGENTS.md zuerst prüfen. Vorhandene Dirty-Dateien,
Quellen, Builds, Backups, Manifeste und Berichte erhalten. Keine neue Firmwareentwicklung.

Nur diese Eingänge gezielt lesen, Pfade relativ zum Projekt:
 docs/project/rx134-c1-review.md
 private/rx134-c-eignung-offline-20261014T110737Z/
   REPORT.private.md, DELIVERY.private.json, PILOT-RX3-NICHT-AUSFUEHREN.private.md,
   relevante Build-/Image-/Config-/Rückwegbindungen und Manifestzeilen;
 private/tag2-abschluss-vier-rx-peer-20261013T184515Z/
   live_adapter.py, bounded_http.py, capture_once.py, analyze.py,
   preflight.py, contracts.py und benötigte Tests;
   pre-inventory.private.json, frozen-config-references.private.json,
   capture-config.private.json und gebundene RX3-/RX2-Originalbelege.
 Für den einmaligen POST den bereits ausgeführten begrenzten Uploadpfad aus
 private/rx2-install-csi-new-20261013-20261013T143100Z/run_once.py gezielt verwenden,
 niemals seinen RX2-Gesamtcollector ausführen.
 Für ROM-Lesen die lokale vorhandene esptool-/S3-Implementierung und bei Bedarf
 private/rx2-maintenance-run-20261012-20261012T180912Z/rom_recovery.py nur als
 Quellreferenz für geprüfte Lesefunktionen/Parser ansehen. Dessen Schreib-/NVS-/
 Gesamt-Recoverypfad niemals starten oder übernehmen.

Neue private Kopie, z.B. private/rx3-rueckweg-c1-pilot-<UTC>/; Ignore UND
Nichtversionierung prüfen, Ordner 0700/Dateien 0600. Originale nicht verändern.
Keine echten Adressen/MACs/Boot-/Configwerte, Tokens, Rohantworten oder Payloads
in öffentliche Dateien/Toolausgaben kopieren. Keine geheimen NVS-/Flashinhalte auswerten.
Keine Downloads, Installation, SDK-Neubauten, neuen Abhängigkeiten oder ProjectAtlas.
Toolchain-/esptool-/Helfer-/Kandidatenversionen und relevante Hashes binden.

Zuerst vorhandene gerätespezifische Schreib-/Readbacknachweise gezielt prüfen.
Keine weitere breite historische Suchrunde: nach einer begrenzten gezielten
Suche das konkrete fehlende Bindeglied benennen und nur mit A live klären.
Eine vor dem Rollback-Umbau gelesene Bootloaderkopie beweist nicht den jetzigen Stand.

Neue Helfer durch Fake-I/O testen, bevor sie Geräte anfassen:
 - Freigabe fehlt → null Liveaktionen; falscher Port/MAC/Node → kein Lesen/Schreiben.
 - Kein Stub, keine Baudwechsel, keine Erase-/Write-/Restorebefehle in A.
 - Begrenzte Readbereiche, Parser-/CRC-/Image-/Hashfehler und Abbruch/Cleanup.
 - Rückweg nicht belegt → null Uploads; nur RX3 und exakt gebundenes C1 erlaubt.
 - Maximal ein POST, keine Redirects/Proxies/Retry, POST-Timeout kein zweiter Upload.
 - Pilotprofil fordert exakt RX3+RX2; Vier-RX-G1-Startschranke bleibt unverändert.
 - Capture-Vor-/Nachlauf, 120-s-Ende, Deadline und eindeutiger Endzustand.

A. RÜCKWEG BINDEN – NUR NACH FREIGABE A LIVE

Erforderliche Hardware für den gesamten Liveauftrag:
 - Mac mit bestehendem Projekt-Netzweg und bestehendem Host wie beim letzten Lauf.
 - RX3 mit einem eindeutig zugeordneten Datenkabel an seinem COM/UART-Anschluss
   zum Mac beziehungsweise passendem vorhandenen USB-UART-Adapter und stabiler
   Stromversorgung. Bei nicht versorgendem Adapter separaten üblichen USB-
   Stromanschluss verwenden. Kein zweiter RX3-Datenport erforderlich.
 - TX1 mit Strom über seinen bisherigen USB-/Stromanschluss; kein UARTzugriff.
 - RX2 auf Original C mit Strom im Projektnetz als unveränderte Kontrolle;
   für diesen Auftrag keine Daten-/UARTverbindung zum Mac erforderlich.
RX1/RX4 und Radare benötigen keinen zusätzlichen Anschluss für diesen Piloten;
ihren bestehenden Zustand nicht ändern. Keine Kabel-/Strom-/Positionsaktion des Agenten.
Johann stellt den passenden Anschluss her. Kein serieller Zugriff auf TX1/RX2,
keine Portsuche durch Öffnen mehrerer Geräte. RX3-Port vor Eintritt anhand
der physischen Nutzerzuordnung/passiven USBdaten bestimmen, im ROM sofort
gegen erwartete RX3-Chip-/MACidentität prüfen. Fremdes Gerät nicht weiter lesen.

Falls frische gebundene Status-/Configantworten erreichbar sind, vor und nach
Wartung erfassen. Historische IPs nicht blind anfragen. Passive MAC-/Interface-/
Direktroutenbindung verwenden; keine Pings, Scans, ARP-Probes oder Netzänderungen.
Fehlende Erreichbarkeit nicht als Defekt ausgeben. Für B sind frische Bindungen zwingend.

Nur bei fehlendem Offline-Rückwegbeleg und erteilter A in ROM wechseln:
 - Ein Eintritt, ein normales Rückkehren; keine Reset-/Baud-/Portretryspirale.
 - Vorhandenen geprüften ROM-Leser verwenden, keine ad-hoc Sync-/Baudimplementation.
 - Bei 115200 ohne Stub bleiben. Maximal fünf initiale Sync-Versuche, keine
   weiteren Hardware-Resets dafür. Stub-/Security-/Identitykonflikt → Abbruch.
 - Bootloaderimage anhand S3-ROM-/Imageheader und gebundener lokalen Formate
   begrenzt lesen: höchstens 32 KiB pro Lesung, keine gesamte Flashsicherung.
 - Bootloader gegen exakt gebundene lokale Images und deren wirkliche Build-
   Rollback-/Security-/Partitionskonfiguration vergleichen. Ein bloßer String
   oder ungeprüfter SDK-Schalter reicht nicht. Unbekanntes Image nicht als bekannt behandeln.
 - Erst nach tragfähiger Bindung den daraus bekannten Partitionstabellenbereich
   lesen, höchstens 4 KiB je Lesung. Kein Erraten oder Scannen weiterer Adressen.
   MD5/Einträge, Appbereiche, Größen und Nichtüberlappung validieren.
 - Je Bereich höchstens zwei unabhängige identische Reads; Gesamtlesemenge
   maximal 72 KiB. Keine NVS-/Otadata-/Appvoll-/eFuse-Payloads lesen.
 - Keine Flasherase/-write, kein Bootloader-/Partitionstabellen-/Otadata-/NVS-
   Schreiben. Toolinterne transient benötigte Schritte nur im geprüften ROM-
   Lesepfad; keine RAM-Stub-/Programm-Uploads oder Schutzänderungen.
 - Normal zurückkehren, eigenen Port sauber schließen, bisherige Firmware
   und acht Configwerte soweit frisch erreichbar bestätigen. Keine zusätzlichen
   Steuerleitungsresets durch unbedachtes Öffnen/Schließen erzeugen.

Ein bestätigter automatischer Rückweg braucht: jetzige RX3-/Bootloaderbindung,
belegte Rollbackfunktion, gültige Partitionstabelle und bisherige gesunde App
als erhaltenes Rückkehrziel. next_partition ist ein Updateziel, keine alleinige
Bootauswahl. Kandidaten-Rollbackflags beweisen den installierten Bootloader nicht.
Ein Qualitätsfehler nach VALID ist kein garantierter automatischer Rückkehrauslöser.

Falls Bindung/Lesen/Rückkehr nicht ausreichend belegt werden: B nicht beginnen.
Keine manuelle Restoreaktion oder neue Recoveryfirmware erfinden. Einen konkret
geeigneten manuellen Rückweg zur getrennten Freigabe beschreiben, falls nötig.
Diese Vorlage erlaubt keine manuelle Flash-/Restoreausführung.

B. C1-EINZELPILOT – NUR BEI RÜCKWEG UND FREIGABE B

Exakter Uploadkandidat:
 private/rx134-c-eignung-offline-20261014T110737Z/build-C1/rx/esp32-csi-node.bin
 1.238.784 Byte
 BIN-SHA256 01414441fbb9d4126d3c342e028b2693fd0b718816f34d2cf295d2c9c17bd5b6
 erwarteter Runtime-/Partitionsdigest:
 9f5afbc5cdb040d4891f70368482a0aa1bd104f995c190588bdbbf20083f00fd
 Diagnose-Source-SHA:
 1edd6fcb06bc846fea1c407926066d629849b210e62da44379158a6a1baf8b84

Bisherige RX3-App:
 software/ruview/firmware/esp32-csi-node/build-rx-led/esp32-csi-node.bin
 BIN-SHA256 a9cd84fbaad594934ef0b2f94e553514829eee583effbd62f5c174ec1dacf03a
 Runtime-Digest e09d3a227dd7419b32009361b4d02ca49fa536f2a8d28816525854493a90b354
 Historisch ota_1 / VALID, Updateziel ota_0; FRISCH prüfen, keine Adresse hart übernehmen.
RX2 bleibt Original C, Runtime-Digest:
 e674e9fa055c05037c95a9df7c99dd2097b2c3544016ea99794888721a6a191a

Vor Upload frische bekannte MAC/Endpoint-/Interface-/Direktroute und Statusnode
RX3 prüfen. Stable Boot, altes Image, VALID, laufender/inaktiver Appbereich,
Slotgröße, Config und TX-Quelle gegen private Referenz binden. RX2 als Kontrolle
frisch binden; fehlt sie, vor Upload stoppen. Keine Reparatur oder neue Baseline
bei widersprüchlicher Config. Auth-/Securityverträglichkeit und Rückweg müssen passen.
Nur vorhandenen Token nach bestätigter Identität privat verwenden, keine Tokens
in argv/Logs; kein Auth-, Netz- oder Dienstumbau. Kein Upload eines Peer-/Default-
Builds oder unveränderten C statt C1. Offline-Siegel vor Livephase prüfen.

Genau ein roher App-OTA-POST /ota auf RX3; Größen-/Hashprüfung vor Sendebeginn.
Keine anderen Schreibmethoden oder Uploadziele. Timeout/Socketabbruch erlaubt
keinen Retry und beweist weder Installation noch Rollback. Antwort, gesendete
Bytezahl und spätere Statusbelege getrennt sichern. Die bisherige Apppartition,
Bootloader, Tabelle, NVS und Sicherheitseinstellungen nicht überschreiben.

Nach POST nur eine vorher begrenzte Statussequenz zur Zustandsklärung vorsehen:
maximal drei Status-GETs in insgesamt höchstens 180 s, keine schnelle Pollschleife.
C1-/neuen Boot-/Slot-/VALID-Zustand und Config/Diagnose binden. Für Stabilität
mindestens zwei erfolgreiche konsistente Statusantworten mit Abstand verwenden.
Ist bereits gesunde alte App zurück, nicht erneut installieren/überschreiben.
Falls RX3 nicht erreichbar wird, nur die mit B ausdrücklich erlaubte einmalige
normale Rückkehraktion des belegten automatischen Pfads; kein manueller Restore,
kein serielles Schreiben, keine weiteren Resets. Unklarer Endzustand → stoppen/berichten.

Ist C1 stabil VALID, Config und Quelle erhalten: genau ein passiver Capture
RX3 + Kontrolle RX2 am bestehenden Host. Bestehenden Decoder/Auswahl-/Endvertrag
verwenden; kleiner ausdrücklich benannter Zweiknoten-Pilotmodus mit Fake-I/O-
Tests, keine stillschweigende Lockerung des Vier-RX-G1-Collectors.
Frische RX3-C1- und RX2-C-Boot-/Image-/Quellen-/Zieltupel in neue Referenzkopie
eintragen; alte versiegelte Setups/Reports nicht ändern. Kein Hostpatchrollout.

Capture: 2 s Vorlauf, 120 s festes Messfenster, 2 s Nachlauf, maximal 10 s Cleanup.
Vorher >=2 GiB freier Platz, maximal 128 MiB PCAP, keine Rotation/zweite Aufnahme.
Header/Listening-Readiness, monotone Prozessanker und original PCAP-Zeitstempel;
50-ms-Uhrqualitätsschwelle erhalten, Vor-/Nachlauf keine Messgutschrift.
Keine Geräte-/APIanfragen, UARToperationen, Builds oder Eingriffe während Capture.
Kein Monitor-/Promiscuousmodus oder zweiter UDP-Empfänger. Nur eigenen Prozess schließen.

Pilotqualität vorab festgelegt, nur für diesen neuen RX3-Piloten:
 - vollständiger Capture, klare Identität/Quelle und keine verdeckte Epochengrenze;
 - exakt gebundenes 128er-Basisraster, >=5 passende Frames/s;
 - 3-s-Fenster, 1-s-Schritt, 5 s Einschwingen, >=15 passende Frames,
   >=2,5 s Abdeckung, innere Maxlücke <1 s;
 - mindestens 102/113 gültige eigene RX3-Fenster (>=90 %); RX2-Kontrolle
   ebenfalls mit >=5 passenden Frames/s und >=102/113 Fenstern qualifizieren;
   schlechte/fehlende Kontrolle verhindert die Freigabe weiterer Empfänger;
 - stabile C1-/VALID-/Boot-/Config-/Quellen-Endprüfung nach Capture.
Die frühere strenge Regel separat ausweisen, keinen historischen FAIL umschreiben.
Gültige Formatausschlüsse und ungeklärte Sequenzpositionen samt Nenner trennen.
Keine WLANverlust-/HT20-Kausalitäts-/Server-/Positionsbehauptung aus PCAP allein.
Zwei RX ersetzen keine G1-Abnahme aller vier.

Bei Qualitätsfehler nach gesundem C1/VALID kein automatischer Rückupload oder
weiterer Empfänger: gemessenen Zustand erhalten und Pilotqualität als FAIL ausweisen.
RX1/RX4 bleiben unverändert. Echte Peerstrecken bleiben offen und inaktiv.

BUDGET, ABORTS UND ABSCHLUSS

Freigabe A maximal 15 min, B maximal 30 min, zusammen maximal 45 min aktiver
Livearbeit ab erster aktuellen Hardware-/Netz-/Geräteaktion bis Cleanup/letztem GET.
Insgesamt maximal 24 RX-GETs ausschließlich RX3 und Kontroll-RX2:
/ota/status, authentisiertes /config, bei bestätigtem C/C1 /config?diag=1.
Vor-/Nachher- und Abschlussbedarf reservieren. Keine TX1-/RX1-/RX4-/Radar-/Host-API-
Abfragen, kein Fan-out. Begrenzte passive lokale Bindungsabfragen bleiben erlaubt.
GET connect <=3 s, gesamt <=7 s, Antwort <=16 KiB; POST connect <=3 s,
gesamt <=120 s, Antwort <=16 KiB. Keine Redirects/Proxies/Cookies/Retry.
Globale monotone Deadline für alle Unterprozesse, ROMphase und Cleanup.
Exklusive einmalige Verbrauchsmarker je Auftrag/Phase vor erster Aktion,
kein Budgetneustart durch neue Ordner/Prozesse. Eine Fehlanfrage bleibt verbraucht.
Fehlende Berechtigung nicht durch sudo-/Rechte-/Dienstumbau lösen.

Private Receipt-/Hash-/Read-/Clock-/PCAP-/Endzustandsbelege vollständig liefern.
Pfade/Toolausgaben auf nötige nicht sensible Metadaten begrenzen. Keine Git-
Bereinigung, Staging, Commits oder Pushes; alte Daten und fremde Änderungen erhalten.

Abschlussbericht mit getrennten Urteilen:
 RUECKWEG: BELEGT / NICHT_BELEGT;
 INSTALLATION: NICHT_GESTARTET / C1_BESTAETIGT / ALTE_APP_BESTAETIGT / UNKLAR;
 RX3_BETRIEB: BOOT_CONFIG_VALID_BESTAETIGT / NICHT_BESTAETIGT;
 PILOTQUALITAET: PASS / FAIL / NICHT_GEMESSEN;
 G1: WEITER_OFFEN, RX1/RX4 NICHT_INSTALLIERT, PEER NICHT_AKTIVIERT.
Freigaben, tatsächlich verbrauchte Zeit/GETs/POSTs/Resets/ROMreads/Captures,
Helfer-/Imagebindungen und konkrete offene Grenzen angeben. Niemals aus einem
Teilurteil ein vollständiges PASS machen oder Wiederherstellung behaupten.

Bei fehlender Freigabe nur erlaubte Offlinearbeit abschließen und konkret
benennen, welche Freigabe noch fehlt. Bei Hardware-/Rückwegfehler keine Reparatur-
oder Retryspirale. Nach Bericht genau einen nächsten Schritt nennen und stoppen.
Erst erfolgreicher Einzelpilot erlaubt eine Empfehlung für RX1/RX4, keine Aktion.
```

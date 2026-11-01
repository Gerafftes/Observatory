<!-- Publication copy: local paths and hardware identifiers were pseudonymized; private original retained. -->
# Gemeinsame RX-Firmware: installiert, Tag14 am Peerblocker gestoppt

**Abschließender Stand:** Alle vier RX sind mit derselben App installiert;
Q900, Short und die zusätzliche Basis sind bestanden. Der Paarlauf ist wegen
beider fehlender Peerlinks nicht bestanden. Die TX-only-/Default-Rückkehr ist
abgeschlossen und 21,797 s mit vier aktiven, fortschreitenden RX belegt.
Die beendete Peersitzung wurde bereits in der Vorprüfung sichtbar, dort aber
nicht gesperrt; diese lokale Prüflücke ist nach negativen Gegenfällen und
69 bestandenen Tests geschlossen. Die Ursache des vorzeitigen Sitzungsendes
bleibt offen. Keine weitere Messung oder Firmwareinstallation wurde gestartet.
Maßgeblich ist der [Ergebnisbericht](rx-unified-tag14-result-20261027.md);
die folgenden Abschnitte erhalten den Verlauf und seine damaligen Stände.

**Paarmitschnitt gestartet:** RX1/RX4 bestanden ihre Paarbereitschaft mit
Generation 2, `pair-trial`, bei etwa 90/151 s Bootalter und mehr als 60,2 s
VALID-Abstand. RX2/RX3 behalten ihre Basisqualifikation. Der Host wurde
kontrolliert auf das versiegelte Paar-Setup mit unveränderter geprüfter
RX-only-Binary umgestellt. Der 300-s-Recorderstart wurde am 27.10.2026 um
21:10:24,455 Uhr Europe/Berlin bestätigt. Einmal-Aufgaben zur Halbzeit
21:12:54 und Endauswertung 21:15:49 sind angelegt; die erledigte
Bereitschaftsaufgabe ist gelöscht. Paarabnahme und TX-only-/Default-Rückkehr
bleiben bis zu den tatsächlichen Abschlussbelegen offen.

**Basis bestanden, Paarvorbereitung läuft:** Die ausdrücklich zusätzlich
freigegebene Basisaufnahme `tag14-unified-20261027-baseline-02` bestand mit
27735 Rohframes, je Link und gemeinsam 173/173 Fenstern, zwei identischen
nativen Auswertungen und vollständigem PCAP-Abgleich. Endabweichung
20,7735 ms; konservative Uhrunsicherheit 2,493584 ms. Die 54 nativen
Sequenzlücken bleiben getrennt ausgewiesen, ohne RF-Verlustbehauptung.
Die bisherige ungültige Basis wurde nicht verändert oder umetikettiert.

RX1 und RX4 haben anschließend jeweils genau einen freigegebenen
Konfigurationswrite zu `pair-trial`, Generation 2, und einen geplanten
Software-Neustart bestätigt. RX2/RX3 bleiben unverändert. Die Bereitschaftsprüfung
mit 90 s Anlaufwartezeit und gespreizten VALID-Belegen läuft; ein Paarmitschnitt
ist noch nicht gestartet. Die erledigte Basis-Endaufgabe ist gelöscht, die
Einmal-Aufgabe zur Bereitschaft/Übergabe ist für 21:09:30 Uhr angelegt.
Paarabnahme, TX-only-Rückkehr und endgültiger Tag14-Abschluss bleiben offen.

**Tag14-Zwischenstand nach vollständiger Vier-RX-Q900:** RX4s ergänzte Q900
bestand bei 810,428/915,552 s; RX1–RX3 behielten ihre belegten Boots/Profile.
Der produktive Host wurde kontrolliert auf den geprüften RX-only-v4-Host mit
versiegeltem Basis-Setup umgestellt. Der erste tatsächliche Kurzlauf bestand:
4939 Rohframes, 23/23 gemeinsame Fenster, PCAP-Abgleich und Endabweichung
42,4165 ms innerhalb der unveränderten 50-ms-Grenze. Eine vorherige
PCAP-Vorbereitung scheiterte vor jedem Recorderstart an der unter macOS/en0
nicht unterstützten Nanosekundenanforderung; die geprüfte Korrektur verwendet
Mikrosekunden und rechnet 1000 ns konservative Quantisierung ein.

Die erste Basisaufnahme bleibt ungültig: tatsächliche Endabweichung
130,473417 ms. Je Link und gemeinsam lagen 173/173 Fenster vor; alle 28947
gespeicherten Payloads sind im PCAP vorhanden, keine ungeklärten inneren
Datagramme. Die 67 nativen Sequenzlücken werden separat ausgewiesen und sind
kein RF-Verlustnachweis. Der Planer hatte die verspätete Startantwort zur
Messdauer addiert. Nach einem am alten Stand fehlschlagenden Regressionstest
plant er nun vom kalibrierten nativen Produktionsstart mit konservativer
Offsetgrenze; 66 Python-Tests bestanden. Die 50-ms-Abnahmegrenze bleibt.

Der Nutzer hat genau eine zusätzliche Basisaufnahme freigegeben. Sie läuft
unter `baseline-capture-03/` mit eingefrorener `measurement-runtime-03/` und
neuer Recording-ID; die ungültige erste Aufnahme bleibt unverändert erhalten.
Einmal-Aufgaben für Halbzeit 21:03:14 und Endauswertung 21:05:09 sind angelegt.
Keine Paar-Konfigurationswrites oder weiteren Geräte-Neustarts wurden bisher
ausgeführt. Paarlauf, Rückkehr und vollständige Tag14-Abnahme sind offen.

**Ausdrücklich freigegebene RX4-Ergänzung läuft:** Der Nutzer hat genau einen
zusätzlichen Software-Neustart nur von RX4 und den vorbereiteten Q900-Nachweis
mit höchstens 40 GETs freigegeben. RX1–RX3 wurden davor lesend an ihre bisherigen
Q900-Boots/Profile gebunden und nicht neu gestartet. RX4s Neustart ist bestätigt;
der Prozess wartet zunächst fest 90 s und holt anschließend seine Zeitfenster
nach. Nachher werden auch RX1–RX3 nochmals auf unveränderte Bindungen geprüft.
Keine weiteren App- oder Konfigurationswrites im Q-Lauf. Eine neue geplante
Einmal-Fortsetzung dieses Chats ist für 20:44 Uhr Europe/Berlin angelegt;
die vorherige Einmal-Aufgabe ist gelöscht. Ende bleibt 23:10 Uhr. Erfolg ist
noch nicht belegt; Tag14 und Hostwechsel bleiben bis dahin ungestartet.

**Nach ausdrücklich freigegebenem lesendem Diagnosecheck:** RX4 beantwortete
alle vier erlaubten GET-Abfragen. Image-/Quell-/Profilbindung und VALID passen;
Boot-ID ist gegenüber dem Vorzustand geändert, Bootalter 1316,915 s,
Generation 1, NORMAL, 52285 geeignete Frames, Heapminimum 181004 B und keine
aktive Messsperre. Es erfolgten genau vier GETs, kein Reset und kein Write.
Die Ursache des früheren 30-s-Timeouts bleibt offen. RX4s Q900-Fenster dieses
Boots sind verpasst; der Diagnosecheck ersetzt sie nicht. Eine konkrete
Ergänzung nur für RX4 ist vorbereitet: ein zusätzlicher kontrollierter
Neustart, 90 s feste Anlaufwartezeit ohne Abfragewiederholung und neue Q900;
RX1–RX3 behalten ihre bereits belegten Boots/Profile, die davor und danach
lesend gebunden werden müssen. Diese Ergänzung war zu diesem Zeitpunkt noch
nicht freigegeben; die spätere ausdrückliche Freigabe und Ausführung stehen oben.

**Stoppbefund am 27.10.2026, 20:20 Uhr Europe/Berlin:** Die gemeinsame Q900
ist nicht bestanden. Alle vier Neustarts wurden bestätigt; die erste
RX4-`/capabilities`-Abfrage nach 30 s endete nach 10 s mit einem Timeout.
Es gab keine automatische Wiederholung. RX1–RX3 erreichten beide Fenster um
810/915 s mit NORMAL; ihre gespeicherten Antworten wurden anschließend mit
dem eingefrorenen Produktionshelfer offline erneut auf Identität, Profil,
Ressourcen, Buchhaltung und VALID-Abstand geprüft. Diese drei Teilbelege
ersetzen den fehlenden RX4-Beleg nicht.

Der Nutzer bestätigt RX4 weiterhin dauerhaft violett. Der Mac besitzt die
konfigurierte CSI-Zieladresse; seine passive ARP-Tabelle ordnet RX4 weiterhin
der bei der Abfrage benutzten Adresse zu. Ursache und aktueller authentisierter
RX4-Boot-/HTTP-Zustand sind damit nicht geklärt. Keine neuen Geräteabfragen,
Resets, App- oder Konfigurationswrites wurden zur Fehleranalyse ausgeführt.
Tag14-Aufnahmen und Hostwechsel wurden nicht begonnen; der bisherige Default
ist unverändert. Prüfprozess beendet, eigener Wachhalteprozess beendet und
die erledigte Einmal-Aufgabe gelöscht. Weitere Geräteaktionen benötigen eine
gezielte Fortsetzung innerhalb der unveränderten Grenzen.
Private Belege: `amendment-01/shared-q900-01/` und
`amendment-01/q900-failure-review-01/` im externen Kampagnenordner.

**Stand am 27.10.2026, 20:01 Uhr Europe/Berlin:** Der Nutzer hat alle vier RX
an ihren ursprünglichen Messpositionen mit dauerhaft blau/grün/rot/violett
bestätigt. Je ein geplanter Neustart wurde authentisiert bestätigt; die neue
gemeinsame Q900 läuft. Eine geplante Einmal-Fortsetzung dieses Chats ist für
20:17 Uhr angelegt. Tag14-Aufnahmen und produktiver Hostwechsel sind noch
nicht begonnen. Der Nutzer wünscht beim Aufnahmestart vier bis fünf Minuten
Begleitung, danach geplante Einmal-Kontrollen zur Hälfte, bei Übergaben und am
Ende statt fortlaufender Beobachtung. Die Frist 23:10 Uhr bleibt unverändert.

**Rolloutstand am 27.10.2026, 19:58 Uhr:** Alle vier RX haben denselben
korrigierten Apphash. RX4s beide frischen vollständigen 16-MiB-Sicherungen
waren bytegleich; die tatsächliche Bootauswahl war ota_1 / 0x220000.
Nach genau einem Appwrite bestanden vollständiger Slot-/Schutzbereichsvergleich
und Normalstart. Zwei Betriebsbeobachtungen bei 63,860 s und 125,060 s
bestätigten NORMAL, gleiche Boot-/Profilbindung und steigende geeignete
CSI-Zähler (2380 auf 4737); minimaler interner Heap 183004 B. Der USB-Port
ist geschlossen. Die abschließende physische Bestätigung aller vier RX am
Messplatz steht noch aus; gemeinsame Q900 und Tag14 sind noch offen.
Appwrites insgesamt: RX2 zwei, RX1/RX3/RX4 jeweils einer. Weitere Appwrites
oder Restores sind nicht freigegeben.

**Vorheriger Rolloutstand, 19:43 Uhr:** RX1 und RX3 haben denselben
korrigierten Apphash wie RX2. Jeweils genau ein Appwrite, vollständiger
Slot-/Schutzbereichsvergleich und zwei positive Betriebsbeobachtungen mit
mindestens 60,2 s Abstand sind belegt. RX3s erster Betriebscheck wurde durch
eine Chat-Unterbrechung nach der ersten Beobachtung beendet; ein separat
protokollierter rein lesender Check bestand anschließend ohne weiteren
Appwrite oder Reset. Der Nutzer bestätigte RX3 dauerhaft rot und wieder am
Messplatz. RX4 ist per USB verbunden; seine frische Inventur bestätigt
16 MiB und JEDEC 68/4018, die beiden vollständigen Sicherungen laufen noch.
RX4 ist noch nicht beschrieben. Gemeinsame Q900 und Tag14 sind noch offen.

Der neue native Host wurde um den bisherigen RX-only-Betrieb ergänzt, damit
beim späteren Wechsel keine mmWave-Dienste starten. 548 native Tests bestanden
(6 bestehende Tests ignoriert). Der lokale Startcheck des tatsächlich gebauten
Hosts bestätigt CSI-UDP, keine Radar-/Discovery-Sockets, HTTP 404 für die
Radar-Steuerroute und keine Anfrage an das konfigurierte lokale Radar-Testziel.
Die Prüfprozesse sind beendet; der produktive Default-Host ist noch unverändert.
Host-SHA256: `78c64630e1338a3060243068f4ed1a94944fd43a7d8b76b116f015a8aafb45dc`.
Private Belege: `amendment-01/native-rx-only/` im externen Kampagnenordner.

**Aktualisierung nach ausdrücklicher B-Ergänzung:** Der korrigierte Kandidat
`18c54935…48f1fe8` wurde als zweiter und letzter derzeit erlaubter RX2-Appwrite
installiert und vollständig zurückgelesen. Sein Pilot ist bestanden: gleiche
Boot-/Profil-/Imagebindung bei 810,409 s und 915,460 s, 104,895 s Abstand
zwischen den VALID-Belegen, 37014 geeignete Frames, minimaler interner Heap
161172 B und vom Nutzer bestätigte dauerhaft grüne RGB-Anzeige.
Die bedingte Fortsetzung ist damit geöffnet. RX2 wurde anschließend vom Nutzer
auf normale Versorgung umgesetzt; für Tag14 braucht dieser neue Boot eine
neue gemeinsame Q900. RX1 und RX3 erhielten exakt denselben korrigierten Apphash.
Das Ende bleibt 23:10 Uhr; die folgenden Abschnitte erhalten den ersten roten
Pilotbefund und die damalige, inzwischen ausdrücklich ergänzte Schreibgrenze.

Der neue Betriebs-/Schreibauftrag wurde am 27. Oktober 2026 erteilt. Sein
gemeinsames absolutes Ende ist **23:10 Uhr Europe/Berlin / 22:10 UTC**.
Die Fortsetzung mit RX1/RX3/RX4 und Tag14 ist an einen bestandenen Pilot gebunden.
Diese Bedingung war beim ersten Kandidaten **nicht erfüllt** und wurde erst
durch den oben dokumentierten korrigierten Pilot erfüllt. G/Gesamt bleibt pausiert.

## Tatsächlicher Gerätestand

- RX2 wurde über den frisch zugeordneten USB-Port und seine historische
  physische Kennung bestätigt: ESP32-S3, gemessene 16 MiB, JEDEC 68/4018.
- Beide frischen vollständigen 16-MiB-Sicherungen waren bytegleich. Ihre
  gespeicherten Bytes wurden verglichen; alte Sicherungen bleiben erhalten.
- Die aktuelle Partitionstabelle ist bytegleich mit der Release-Referenz.
  Beide OTA-Verwaltungssektoren haben gültige CRC/VALID-Zustände; die aus den
  tatsächlichen SDK-Funktionen gebildete Auswahl ergibt ota_0 / 0x20000.
- Die vorhandenen NVS-Werte wurden nur gelesen. Der tatsächliche
  Produktionscodec akzeptiert die daraus entstehende Basis: RX2, Generation 1,
  TX-only, 50 Hz, GPIO48, keine Peers. Ziel und TX-Filter passen zu den Bindungen.
- **Genau ein Appwrite** des ersten gemeinsamen Kandidaten wurde ausgeführt.
  SDK-Wiederholungen für ganzen Write und Flashblöcke waren auf einen Versuch
  begrenzt; die Fehlerpfade wurden vorab am tatsächlichen SDK geprüft.
- Der gesamte 2-MiB-Appslot entspricht dem erwarteten Nachzustand einschließlich
  Sektorrundung. Der Bereich vor dem Slot einschließlich Bootloader, NVS,
  Partitionstabelle und otadata sowie die obere Slotgrenze sind unverändert.
- Danach erfolgte ein Normalstart. Authentisierte Antworten bestätigen den
  laufenden Image-Digest, den erwarteten Quellhash, ota_0 / VALID und identische
  aktive/gespeicherte Konfiguration ohne ausstehenden Neustart.

Installierte App-Datei-SHA256:
`e339fb0ad8c1c6c05affdf57acae263476580b1efd056f22fa8850f41778430f`.
Laufender Image-Digest:
`4e3bc5c3f6dff62b492abf0fe8b8e513bc6f412a192cf58312e3d6df81d2f60c`.

Der erste USB-Leseversuch war zuvor durch einen falschen Attributnamen im
lokalen Partitionsparser gestoppt. Dieser Fehler wurde an der gespeicherten
Tabelle offline korrigiert. Dabei gab es keinen Appwrite. Die erste
Write-Freigabe wurde erst mit dem oben beschriebenen Installationsaufruf benutzt.

## Warum der Pilot gestoppt ist

Die erste Datenvorprüfung bei Bootalter 94,3 s ergab **TX1_STALE**, null
qualifizierte Frames und nur zwei zugelassene 64er-Frames. Die RGB-Anzeige
wurde vom Nutzer ebenfalls als blinkend oder andersfarbig gemeldet.
Q900 mit seinen 800–850-/900–920-s-Fenstern wurde deshalb nicht begonnen.

Die Ressourcen waren dabei unauffällig: interner Heap aktuell 198908 B,
Minimum 189536 B, größter Block 131060 B; Stackreserven HTTP 3080 B,
Callback 3536 B und Sender 6888 B. Keine Allokationsfehler, kein Reconnectfehler,
keine Transportverwerfung. Das sind die tatsächlich beobachteten Werte des
ersten Kandidaten, keine Laufzeitqualifikation des folgenden Korrekturkandidaten.

Ein begrenzter lesender Gegencheck im selben Boot mit zehn beantworteten
Host-Pings änderte weder Konfiguration noch Gerät. In dessen 4,76-s-Klammer
kamen 52 fremde Callbackframes und weiterhin null geeignete Frames hinzu.
Am unveränderten Default-Host wurden gleichzeitig RX1/RX3 mit etwa 31–32 Hz
beobachtet; RX4 war dort bereits stale. Daraus wird keine RF-Verlustrate abgeleitet.

Der Quellvergleich zeigt zwei beim Zusammenführen ausgelassene Betriebsdetails:

1. Der qualifizierte frühere RX-Pfad schaltete auf dem Board ohne Display von
   MGMT-only auf MGMT+DATA um. Der neue schlanke Startpfad rief diesen Schritt
   nicht auf; seine Initialisierung ließ den Filter auf MGMT-only.
2. Der frühere qualifizierte Build nutzte den vorhandenen Verkehrsgeber mit
   20-ms-Intervall. Im neuen gemeinsamen Profil war dieser abgeschaltet.

Der Livebefund beweist die fehlenden geeigneten CSI-Daten. Er isoliert nicht
den jeweiligen Anteil dieser beiden Änderungen. Die Korrektur stellt den
zuvor benutzten Erfassungs-/Verkehrsvertrag wieder her; sie ist noch kein
Hardware-Ursachennachweis oder Tag14-PASS.

## Vorbereitete Korrektur, noch nicht installiert

- Nur das gemeinsame RX-Profil aktiviert direkt MGMT+DATA in der bestehenden
  CSI-Initialisierung. Legacy-Displayprofile behalten ihr bisheriges Verhalten.
- Der vorhandene 20-ms-Verkehrsgeber ist wieder eingeschaltet; vor OTA und
  nach fehlgeschlagener Steuerungsinitialisierung wird er mit abgeschaltet.
- Der Regressionstest ruft die tatsächliche Produktionsinitialisierung auf und
  prüft das gesetzte DATA-Filterbit. Er scheitert am alten Stand und besteht
  nach der Korrektur. Eine zusätzliche Profil-/Buildprüfung bindet den Verkehrsgeber.
- Gesamte C-Hostsuite mit ASan/UBSan und 60 Pythontests bestanden.
- Zwei unabhängige ESP-IDF-Builds lieferten bytegleiche Appbytes. Wire v4,
  Masken, NVS-Schema, Hostdecoder und Partitionsdatei bleiben dieselben Verträge.

Korrigierter Kandidat: **909408 B**, weiterhin App-only im 2-MiB-Slot.

- App-Datei-SHA256: `18c54935a5b99a147755e59cce1aeb471287f9687c9fda37eb0891ae948f1fe8`
- Image-Digest: `4c99c8cb290192ac825c8246ae490f8fd7ade5cd5cea77c3450a4816c38d03df`
- Quellen-SHA256: `c8697d6175e84b0535970563f602ecd51465abb054cc5ef4e37cb64c94bbf40d`
- Statischer DIRAM-Bericht: 141223 B belegt, 200537 B frei; keine Behauptung
  über tatsächliche Laufzeitreserven dieses noch nicht installierten Kandidaten.

Belege liegen ausschließlich extern unter
`PRIVATE_EVIDENCE/rx-unified-live-20261027-run01/producer-fix/`:
`producer-fix.patch`, Vorher-/Nachherquellen, negativer Regressionstest,
`c-tests-after.log`, `python-tests-after.log`, `FIX-VERIFICATION.private.json`
und `release-candidate-v2/` mit Quellsnapshot, RELEASE, SDK-Konfiguration,
Ressourcenbericht und beiden Builds. Die Versionszeichenfolge ist weiterhin
`rx-unified-1`; maßgeblich unterscheiden die vollständigen Hashes die Kandidaten.

## Nächste Grenze

Der ursprüngliche eine RX2-Appwrite ist verbraucht. Keine Wiederholung, kein
Restore, kein Konfigurationswrite und kein weiteres RX wurden daraus abgeleitet.
Der produktive Host, TX1, Radar und das Host-Netz wurden nicht geändert;
Tag14-Aufnahmen wurden nicht gestartet. Die Q-Abfrage und alle USB-Sitzungen
sind beendet. Der derzeit installierte erste Kandidat bleibt auf RX2.

Für den neuen Kandidaten wird eine ausdrückliche Ergänzung benötigt:
**ein zusätzlicher RX2-Appwrite**, damit maximal zwei RX2-Appwrites in dieser
Kampagne; erst nach bestandenem korrigiertem Pilot je ein Appwrite desselben
neuen Kandidaten auf RX1/RX3/RX4 anstelle des alten Kandidaten. Frische
Vorprüfungen und Sicherungen bleiben Pflicht. Die vorhandenen Betriebsrechte
und das ursprüngliche Ende 23:10 Uhr werden dadurch nicht verlängert.
`AMENDMENT-DRAFT.private.json` ist eine vorbereitete Beschreibung und
**keine erteilte Freigabe**.

Die [Tag14-Anleitung](tag14-mit-rx-unified-20261027.md) gilt als Ablauf erst nach
dieser neuen Kandidatenbindung und bestandenem Pilot. Ein erneuter Pilotfehler
stoppt die Fortsetzung wieder; Offline-Tests ersetzen seine Messnachweise nicht.

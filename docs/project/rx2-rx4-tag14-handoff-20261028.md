<!-- Publication copy: local paths and hardware identifiers were pseudonymized; private original retained. -->
# Übergabe RX2 bis RX4 und anschließende Tag14 Messung

Stand 28.10.2026. **RX1 ist installiert und für den normalen Betrieb geprüft. Als Nächstes folgt RX2, danach RX3 und RX4.** Der Nutzer stellt den noch nicht durchgeführten RX1 Funkunterbrechungstest ausdrücklich zurück und möchte mit dem Rollout fortfahren. Das frühere Gate „erst nach bestandenem Funkunterbrechungstest weitere RX installieren“ ist für diese Fortsetzung damit aufgehoben. Der fehlende Nachweis wird nicht als bestanden markiert.

Diese Übergabe beschreibt den nächsten Auftrag. Sie installiert nichts, startet keine Messung und verlängert keine bisherigen Zeitfenster oder Schreibfreigaben.

## Verbindlicher Ausgangsstand

| Gerät oder Nachweis | Letzter belegter Stand |
|---|---|
| RX1 | Neues Image in ota_1; VALID; Profilgeneration 5, TX-only, 50 Hz; normale schwach blaue LED vom Nutzer bestätigt; aktueller CSI-Empfang am bestehenden Host nachgewiesen |
| RX1 Sicherung | Zwei vollständige bytegleiche 16-MiB-Sicherungen; alter Appslot ota_0 und dessen Bootdatensatz erhalten |
| RX1 Grundprüfung | Zwei authentisierte Gesundheitsbeobachtungen mit mindestens 60,2 s Abstand, gleicher Boot/Konfiguration und unveränderter Netzwerkzustandsgeneration; +2513 qualifizierte CSI-Frames und lokale Sendeannahmen |
| RX1 Ressourcen | Interner Heapminimalwert 149360 Byte, HTTP-Stackminimum 2520 Byte, Pingworker 2820 Byte; ein Ping-Socket und keine beobachteten Backendfehler in der Grundprüfung |
| RX1 eigener WLAN-Verlust | Nicht getestet; auf Nutzerwunsch zurückgestellt. Kein Live-Nachweis für den eigenen Disconnect/LOST_IP-Wiederanlauf |
| Mac WLAN-Abwesenheit | Automatische Join-/Rückkehrbefehle lieferten Fehler trotz Exitcode 0; deshalb kein regulärer Test-PASS. Nach bestätigter Rückkehr wieder RX1 NORMAL, gleicher Boot/Profil und weitere 31 s fortschreitende Daten ohne RX1-Reset; RX1 selbst hatte keinen Disconnect |
| RX2, RX3, RX4 | In diesem Auftrag nicht aktualisiert. Der Nutzer meldete sie mit Strom versorgt. Installierte Firmware, Profile, Erreichbarkeit und Positionen beim Fortsetzen frisch binden |
| Gemeinsame Qualifikation und Aufnahme | Für die neue Vier-RX-Generation noch nicht erfolgt. Keine Q900-, Short30-, Basis180-, Paar300- oder lange Abnahme aus den RX1-Grundprüfungen ableiten |

Die genannten Zustände stammen aus gespeicherten Nachweisen, nicht aus einer Geräteabfrage bei Erstellung dieser Übergabe. Boot, DHCP-Adresse und Host-PID sind beim Fortsetzen erneut zu bestätigen. Nicht erneut RX1 flashen, nur um den Rollout zu vereinheitlichen.

## Firmware und geprüfte Werkzeuge

Gemeinsames neues Image für alle vier RX:

- BIN: `PRIVATE_EVIDENCE/rx-recovery-corrections-20261028-run01/candidate-v2/rx-unified.bin`
- Größe: **910208 Byte**; Appslot **2097152 Byte**.
- App-BIN-SHA256: `0c5ee9b37374f88faf91c4e1cb29feed29b031e802336f4549a98be26d68b0dd`.
- Runtime Image-Digest: `4555f7436258a5f91e4aaa352deafc9d7ae0dc4d40e29b4819f8a9522ab6ddfd`.
- Kanonischer Quellhash: `d43b9c550192ace21cf59e77fc60eb60224be768aa13a2a11ca3d02f51414bf3`.
- ESP-IDF 5.4, Commit `67c1de1eebe095d554d281952fde63c16ee2dca0`.
- Offline: 92 Python-Tests, native ASan/UBSan-Regressionssuite, zwei bytegleiche vollständige BIN-/ELF-Builds und unabhängiger fokussierter Review. Gerätespezifische Liveabnahme bleibt pro RX erforderlich.

Der neue Pingpfad hält einen dauerhaften Task, feste Puffer und höchstens einen Socket. Der korrigierte Hosthelfer prüft die Netzwerkzustandsgeneration auch **nach bestätigtem Recorderende**, hält bis dahin die eigenen Messleases und verlangt bestätigte Freigaben. Für neue Messungen diesen geprüften Helfer verwenden:

`PRIVATE_EVIDENCE/rx-recovery-corrections-20261028-run01/work/software/ruview/scripts/rx_measurement.py`

Nicht versehentlich den älteren Helfer aus dem ursprünglichen Checkout oder einer früheren Livekopie verwenden. Der ursprüngliche Arbeitsbaum enthält parallele, fremde Änderungen; keinen Reset/Clean, keine breite Übernahme oder Bereinigung durchführen.

## Nächster Schritt mit RX2

1. **Versorgung und USB klären.** RX1 weiter versorgt lassen. Sein USB-Kabel nicht einfach abziehen, solange unklar ist, ob es die einzige Stromquelle ist. Ein Stromwechsel erzeugt einen neuen Boot und erfordert neue Bindung, keinen erneuten Firmwarewrite. RX2 möglichst zusätzlich erreichbar machen; nötigenfalls einen bewussten Versorgungswechsel vorher mit dem Nutzer abstimmen.
2. **Neue konkrete Rechte und Zeitfenster binden.** Unterbrechung/ROM-Lesen/RAM-Stub/Normalstarts und genau eine App-only-Installation für das physische RX2 getrennt ausdrücklich freigeben lassen, einschließlich nötiger Bootauswahl. Hartes Ende und Abschlussreserve festlegen. Die frühere RX1-Appfreigabe ist verbraucht; das alte Ende 17:15 Uhr und das abgeschlossene Mac-Testfenster gelten nicht für RX2. Keine pauschale Vier-RX-Schreibfreigabe unterstellen.
3. **Frische physische Bindung herstellen.** USB-Identität und exklusiven freien Port, ROM-MAC/Geräte-ID, tatsächliche Flashkapazität, Chiprevision, Sicherheitszustand, Partitionen, Bootauswahl und NVS/Profile prüfen. Ein sichtbarer USB-Port oder historischer DHCP-Eintrag ersetzt keine Gerätebindung. Keine Sicherheitseinstellung verändern.
4. **Zwei neue vollständige Sicherungen erstellen.** Gesamte gemessene Flashkapazität unabhängig zweimal lesen, privat speichern/rücklesen und bytegenau vergleichen. Bestehende Backups nicht überschreiben. Bei Uneindeutigkeit vor dem Write stoppen.
5. **Gerätespezifischen Installationsplan vorbereiten.** Profil/Identität/LED-Pin/Quellenfilter/Zieladresse erhalten. Aktives stabiles VALID-Image und vorhandenen Rückweg binden. Bootloaderverträglichkeit aus tatsächlichen Daten feststellen; nicht blind die RX1-Sonderannahmen kopieren. RX1 hatte 16 MiB, daraus folgt keine Kapazität von RX2/RX3. Source-, BIN- und Runtime-Digests nicht verwechseln.
6. **Einmal installieren und vollständig prüfen.** Einen geprüften inaktiven Appslot beschreiben; komplettes Slot-Rücklesen einschließlich Sektorrundung und erhaltenem Rest verlangen. Erst danach die erforderliche andere OTA-Bootrecord-Seite als NEW setzen und zurücklesen, bisherigen aktiven Record erhalten. Keine Bootloader-, Partitionstabellen-, NVS-/eFuse-/Sicherheitswrites. Keine Wiederholung oder automatische Wiederherstellung nach einer unklaren Schreibantwort.
7. **Normalstart und Grundabnahme.** Tatsächliches neues Image/Source, VALID, richtiger physischer RX2 und erhaltenes aktives/gespeichertes Profil; erwartete Rollenfarbe auch vom Nutzer bestätigen lassen. Zwei Gesundheitsbeobachtungen mit mindestens 60,2 s Abstand, gleicher Boot/Konfiguration, stabilem Netz, fortschreitenden qualifizierten CSI-/Transportzählern und frischem Hostempfang verlangen. Bestehende Heap-/Stack-/Bilanzgrenzen und neue Pingbackenddiagnose prüfen. Das ist Installationsbereitschaft, keine gemeinsame Q900.
8. **Abschließen und nächsten Schritt nennen.** Port schließen, Belege/Schreibbudget abschließen; RX2 an bestätigte Messposition und Versorgung bringen. Bei Stromwechsel seine Nachweise neu binden. Dann dieselbe Folge mit RX3, anschließend RX4. Für jeden Empfänger eigene aktuelle Rechte, Backups und Nachweise.

### Grenzen des wiederverwendbaren RX1 Installers

Die letzte tatsächlich ausgeführte RX1-Fassung liegt unter `PRIVATE_EVIDENCE/rx1-recovery-pilot-20261028-run01/usb_install_once.py`, Plan `INSTALL-PLAN-02.private.json`. Sie ist **RX1-spezifisch** und kein sofort ausführbarer RX2-Plan. Neue Rolle, physische Identität, USB-Bindung, reale Geometrie, vorherige Firmware/Profile, Pfade, Zeitfenster und Budget müssen konsistent adaptiert und offline geprüft werden. Nicht durch bloßes Ersetzen des Rollennamens starten.

Erhaltene Schutzdetails: Sicherheitswerte aus SDK-Tupeln und JSON-Listen wertgleich kanonisieren, Sicherheitsabfragen ohne Cache; exakt gehashte Apppuffer übergeben; Appadresse fest an Slotname binden; Bootsektor exakt aus frischen OTA-Daten vorbereiten und mit gepinntem SDK-Selektor nachweisen; aktive Recordseite erhalten; SDK-Gesamt-/Block-Schreibversuche jeweils eins. Normalstart direkt mit DTR=false und `HardReset(...).reset()`, nicht mit einem ungeprüften Download-/Retry-Resetpfad. Rücklese- oder Sicherheitsfehler niemals weginterpretieren.

## Gemeinsame Vorbereitung nach den vier Installationen

- Alle vier Empfänger an den bestätigten bisherigen Messpositionen und stabil versorgt; aktuelle Firmwarehashes, Boots, IDs, Profile und Host-/CSI-WLAN-Bindung erfassen. Keinen späteren Kabel-/Positionswechsel auf alte Belege aufsetzen.
- TX1 und Radar bleiben unverändert. Keine spontane Netzreparatur, Rate-/Altersgrenzänderung oder Firmwareentwicklung während der Aufnahme.
- Host, Auswerter, Speicherbudget, Aufnahmepfade, Uhrabbildung und PCAP-Rechte frisch binden. Der letzte Host lief als PID52164 mit BIN-Hash `78c64630e1338a3060243068f4ed1a94944fd43a7d8b76b116f015a8aafb45dc`; REST war **127.0.0.1:8080**, WebSocket **127.0.0.1:8765**, UDP5005. Diese Angaben sind historische Einstiegspunkte und keine aktuelle Prozess-/Portgarantie. Nicht REST auf dem WebSocket-Port prüfen und 404 als Geräteausfall werten.
- Neue Messfreigabe mit Start, gemeinsamem Hartende, Rechten für nötige Starts/Profil-/Hostübergaben, Aufnahme und sichere Default-Rückkehr festlegen. Mindestens 120 s Abschlussreserve und echte serielle Abhängigkeiten einplanen. Alte Zeitfenster nicht verlängern.
- RX1/RX4-Paar- und Rückkehrprofile sowie versiegelte Hostsetups aus tatsächlichen aktuellen Generationen vorbereiten. RX2/RX3 bleiben TX-only-Kontrolllinks. Die endliche Paarfrist bleibt **900 s ab dem zugehörigen Boot**; kein unbemerkter Neustart oder Recovery zur Verlängerung. Falls ein Neustart zum Aktivieren nötig ist, explizit freigeben und neue Bootbindung herstellen.

## Reihenfolge der Tag14 Abnahme

| Schritt | Dauer oder Beobachtung | Erforderlicher Nachweis |
|---|---|---|
| Frische gemeinsame Qualifikation | Q900 im tatsächlich verwendeten Basisboot | Erste Beobachtung bei 800–850 s Bootalter, letzte bei 900–920 s; mindestens 60,2 s VALID-Abstand; gleiches Image/Boot/Profil, Ressourcen und vollständige Bilanzen; bestehende Messlease auch für Qualifikation |
| Short30 | 30 s | Eingangstor mit aktuellen Rohframes, physischer Quellen-/Layout-/Maskenbindung, Zeit-/Ressourcen-/Bilanzprüfung; kein Ersatz für die folgenden Blöcke |
| Basis180 | 180 s, TX-only | Vier TX1-Kontrolllinks je mindestens 5 Hz; mindestens 156/173 gültige Fenster gemäß bestehendem Schema |
| RX1↔RX4 Paar300 | 300 s innerhalb unverlängerter Paarbootfrist | Pro Richtung mindestens 600 geeignete echte Rohframes, mindestens 2 Hz und mindestens 264/293 gültige Fenster; beide gerichteten Links und vier TX1-Kontrolllinks tatsächlich nachweisen |
| Abschluss | Sichere Dateischlüsse und mindestens 20 s Default-Nachweis | Nötige TX-only-/Default-Rückkehr nur mit vorheriger Freigabe; erwartete Boots/Profile, NORMAL, fortschreitende Geräte-/Hostzähler, bestätigte Lease-Freigaben und geschlossene eigene Prozesse |

Die Basis-Q900 darf nicht ungeprüft auf einen später neugestarteten Paarboot übertragen werden. Für RX1/RX4 nach dem freigegebenen Profil-/Bootübergang die echte Paarbereitschaft mit ausreichender Restfrist neu binden; keine 900-s-Vorqualifikation in einem auf 900 s begrenzten Paarboot verlangen. RX2/RX3-Belege bleiben nur bei identischem tatsächlichem Boot/Profil verwendbar.

Unverändert gelten 100 ms maximale Capture-Alterung im RX und 250 ms Hostalter, die vorhandenen 50-ms-Uhr-/Endverträge, echte HT-TX-128-/Non-HT-Peer-64-Masken, PCAP-/Rohdaten-/Sequenzabgleich und vollständige Fehler-, Ausschluss- und Datenlückenbilanz. Zwei deterministische native Auswertungen mit gleicher gebundener Eingabe und dem bestehenden Dateibudget verlangen. OS-Sendezähler, LED oder cumulative rates ersetzen keine physischen Peerframes. Ein Netz-/Recoverygenerationwechsel während eines Blocks macht ihn ungültig; auch die nach tatsächlichem Producer-Close gelesene Generation muss passen.

Bei fehlender Richtung, Maske, Bindung, Ressourcen-/Zeitgrenze, unklarem Write-/Stop-/Freigabe- oder Dateischlussfehler stoppen und sichtbar bilanzieren; keine automatische Wiederholungsserie. Historische RX1-Hostbeobachtung hatte Format-Ausschlüsse und unerklärte Sequenzlücken; diese nicht als behobenen RF-Verlust oder Nullverlust umetikettieren.

## Lange Aufnahme nur nach gesondertem Startauftrag

Nach erfolgreicher kurzer gemeinsamer Abnahme eine gegebenenfalls gewünschte längere Tag14-Aufnahme separat vereinbaren: konkrete Dauer, Beginn, Ende, Betriebsrechte und passende nicht nach 900 s endende Profile/Setups. Einen endlichen Paar-Versuch nicht durch Erholung am Leben halten und nicht als Dauerprofil verwenden. Die ersten vier bis fünf Minuten sowie Übergaben aktiv begleiten. Einmalige Halbzeit-/Endprüfungen in diesem Chat nur auf ausdrücklichen Auftrag einrichten. Jetzt keine Aufnahme starten oder Automationen anlegen.

## Maßgebliche gespeicherte Belege

- Offlinekorrektur, Tests und finaler Kandidat: `PRIVATE_EVIDENCE/rx-recovery-corrections-20261028-run01/REPORT.md`, `CLOSURE.json`, `candidate-v2/RELEASE.json`, `DELIVERY-MANIFEST.json`.
- Tatsächliche RX1-Installation und Grundbetrieb: `PRIVATE_EVIDENCE/rx1-recovery-pilot-20261028-run01/REPORT.private.md`, `RESULT.private.json`, `RX1-INSTALL-READY.private.json`, `HOST-RECEPTION-VERIFIED.private.json`, `CLOSE.private.json`.
- Begrenzte Mac-Rückkehrbeobachtung ohne reguläres Test-PASS: `PRIVATE_EVIDENCE/rx1-host-return-20261028-run01/REPORT.private.md`, `CLOSE.private.json`, `RETURN-HEALTH.private.json`.
- Ältere Messregeln und Negativbelege: `docs/project/rx-peer-clock-kurztest-und-langaufnahme-20261027.md`. Dessen alter Firmware-/Rolloutstand wird durch diese Übergabe für die Fortsetzung ersetzt; alte Messbelege bleiben historische Nachweise.

Geheimnisse, Rohsicherungen, NVS-Exporte und Rohmessdaten bleiben ausschließlich am vorgesehenen privaten Ort. Diese Übergabe enthält keine Zugangsdaten und erneuert keine verbrauchten Rechte.

**Unmittelbarer nächster Nutzerschritt: RX2 zusätzlich per USB erreichbar machen und die Stromversorgung von RX1 dabei erhalten beziehungsweise vorher klären. Danach frische RX2-Unterbrechungs-/Sicherungs- und Appwrite-Freigaben samt Zeitfenster konkret binden.**

<!-- Publication copy: local paths and hardware identifiers were pseudonymized; private original retained. -->
# RX-Firmware vereinheitlichen und für die restlichen Projekttage vorbereiten

## 1. Ziel, Entscheidungen und belastbarer Ausgangsstand

**Ziel ist ein gemeinsames ESP32-S3-App-Image für RX1–RX4.** Geräte-ID, Boardparameter, Netzkonfiguration und Peer-Rolle werden konfiguriert und erfordern keine getrennten Firmware-Builds.

Die nächste Überarbeitung umfasst die dokumentierten Anforderungen bis zur Zwei-Personen-Abnahme, einschließlich Vier-RX-Peerfähigkeit, Raumwechsel und Dauerbetrieb. Begrenzte Versuchsprofile und regulärer Peerbetrieb werden beide vorbereitet. ESP-NOW als neuer Mess-/Transportpfad, C6-Funktionen und weitere Forschungsvarianten bleiben außerhalb dieser Veröffentlichung.

**Die Analyse zeigt drei unterschiedliche Ausgangsebenen:**

- Der versionierte Repo-Code enthält bereits einen begrenzten Capture-Pool, ausgelagerte Verarbeitung und ausführlichere Transportstatistiken.
- Die eingefrorenen Tag14-Varianten enthalten zusätzliche Peer-, Reconnect-, Ressourcen- und Fehlerdiagnosefunktionen, die nicht vollständig im versionierten Build zusammengeführt sind.
- RX1–RX4 unterscheiden sich derzeit auch außerhalb ihrer Rollen: Reconnect-Implementierung, Transport-Synchronisation, OTA-Verarbeitung und Diagnoseumfang.

Deshalb wird keine vorhandene Variante unverändert zum gemeinsamen Standard erklärt. Die geeigneten Teile werden in **einem versionierten RX-Build** zusammengeführt.

Weitere festgelegte Grenzen:

- C und der gebundene ESP-IDF-5.4-Stand bleiben die Basis; keine Rustmigration oder gleichzeitige SDK-Hauptversionsmigration.
- Bestehende Partitionen und 2-MiB-Appslots bleiben erhalten. Physische Flashkapazität, Imageheader und Partitionslayout werden getrennt geprüft.
- Sicherheitskonfiguration, Authentisierung und bestehende Gerätegeheimnisse bleiben erhalten.
- Die aktuelle Run10-NoFlash-Lieferung und sämtliche historischen Belege bleiben unverändert als Referenz.
- „Finalisiert“ bedeutet: für die bekannten Anforderungen vollständig vorbereitet und geprüft. Unbekannte Hardwarefehler oder ein später unzureichender Positionsnutzen lassen sich nicht durch Planung ausschließen.

Es wurden keine Dateien geändert, keine Firmware gebaut und keine Geräte angesprochen. Eine reine Syntaxprüfung bestätigt allerdings: Beide vorhandenen `provision.py`-Varianten sind wegen `lazy import` mit dem vorhandenen Python 3.14 nicht parsebar.

## 2. Quellenvergleich und Übernahmeentscheidungen

Ausgangspunkt ist [rf-sensing-sources.md](./docs/research/rf-sensing-sources.md), ergänzt um den dort verlinkten G05-Übernahmeplan und offizielle Espressif-Dokumentation. Konkrete Implementierungen wurden gelesen, nicht nur Projektbeschreibungen.

| Quelle | Sinnvolle Nutzung | Entscheidung |
|---|---|---|
| **Espressif ESP-CSI und IDF-Dokumentation** | CSI-Konfiguration, tatsächliche Metadaten, Gain-Verfügbarkeit, WLAN-Ereignisse, NVS und OTA-Lebenszyklus | Technische Referenz. Beispielcode nicht unverändert übernehmen: Der gelesene Empfänger arbeitet unter anderem mit Ausgabe und Verarbeitung direkt im Callback sowie einem eigenen ESP-NOW-/HT40-Aufbau. Unser begrenzter Callback und der qualifizierte Datenvertrag bleiben maßgeblich. [Implementierung](https://github.com/espressif/esp-csi/blob/master/examples/get-started/csi_recv/main/app_main.c) |
| **wifi-sensing-kit, G05** | Mehrere zugelassene physische Sender, linkweise Auswertung und geführte Datensammlung | Prinzipien übernehmen. Den Patch mit Byte-19-Bit-7 und angehängter MAC nicht auf unseren bestehenden Trailer anwenden; ebenso keine pauschale feste Bin-Auswahl. Referenz bleibt der dokumentierte Commit `7fb77fc…`. [Firmwarepatch](https://github.com/andrisgauracs/wifi-sensing-kit/blob/7fb77fc0c0adb98bcf0a102d68fa8718b2f98625/firmware/ruview-v0.8.8-kit.patch) |
| **RuView, G03** | Bestehende modulare Firmware und neuere Konfigurations-/Onboardingideen | Gezielt vergleichen, keinen vollständigen Upstream-Import. Das gelesene serielle Onboarding deckt nur einen Teil unserer Konfiguration ab und ersetzt weder Topologiebindung noch Datenbudgets. [Implementierung](https://github.com/ruvnet/RuView/blob/main/firmware/esp32-csi-node/main/serial_onboarding.c) |
| **wifi-human-detector-esp32, G01** | Einfacher CSI-Erfassungspfad als Gegenvergleich | Kein geeigneter Ersatz: Amplitudenberechnung und serielle Ausgabe erfolgen direkt im Callback; unser Herkunfts-, Zeit- und Ressourcenvertrag fehlt dort. [Implementierung](https://github.com/thejanu-dileepa/wifi-human-detector-esp32/blob/main/src/main.cpp) |
| **csi_hr, G02** | Beispiel für hostseitige Verarbeitung gespeicherter CSI-Daten | Keine zusätzliche RX-Inferenz daraus ableiten. Der gelesene Verarbeitungspfad filtert den Amplitudenvektor eines Pakets; daraus folgt keine validierte zeitliche Vorverarbeitung für unseren Tracker. [Implementierung](https://github.com/nickbild/csi_hr/blob/main/read_and_process_csi.py) |
| **Wi-Fi-Motion-Detector, G04** | Einfaches Gegenbeispiel zur Quellenbewertung | Keine Firmwareübernahme: Der gelesene Hauptpfad verwendet RSSI-Schwellwerte statt einer CSI-Messpipeline. [Implementierung](https://github.com/ashus3868/Wi-Fi-Motion-Detector-ESP32/blob/main/main/fast_scan.c) |

Jev wurde einmal für zwei eng begrenzte Quellenaussagen eingesetzt. Seine Einschätzungen unterstützten den gelesenen Datenfluss; sie ersetzen weder Tests noch eine Freigabe. Bei späteren Übernahmen werden konkrete Revisionen und Lizenzhinweise festgehalten.

## 3. Priorisierter Änderungsplan mit betroffenen Dateien

Die folgenden RX-Dateinamen beziehen sich auf  
[`software/ruview/firmware/esp32-csi-node/`](./software/ruview/firmware/esp32-csi-node).

### P0 — Einen vollständigen und reproduzierbaren gemeinsamen Stand herstellen

| Arbeitspaket | Umsetzung und Begründung | Betroffene Dateien |
|---|---|---|
| **P0.1 Gemeinsamer Build** | Repo-Capture-Pool und Senderpfad mit den geprüften Tag14-Peer-, Reconnect- und Diagnosefunktionen zusammenführen. Keine vier Quellkopien mehr. Ein S3-Releaseprofil, ein Abhängigkeitsstand, identische Speicher-/Stackprüfungen und eine Firmwareidentität für alle RX. | `main/CMakeLists.txt`, `main/Kconfig.projbuild`, `sdkconfig.defaults*`, `dependencies.lock`; zu übernehmende `csi_peer.*`, `rx_diag.*`, `diag_http.*`, `status_led.*`, `wifi_reconnect.h` |
| **P0.2 Einheitliche Gerätekonfiguration** | ID, LED-Boardparameter, TX-Identität, Zieladresse, Peer-Liste und Betriebsprofil aus validierter Konfiguration beziehen. Fehlende Identität führt zu „nicht eingerichtet“, nicht still zu RX1. Gesamten Änderungswunsch vor dem ersten NVS-Schreibzugriff validieren. | `main/nvs_config.*`, `main/node_identity.*`, `main/config_server.*`, `main/main.c` |
| **P0.3 Konfiguration verlustfrei erhalten** | Versioniertes RX-Profil mit Konfigurationsgeneration einführen. Bestehende Schlüssel kontrolliert übernehmen; Geheimnisse und fremde NVS-Inhalte erhalten. Ungültige oder nicht migrierbare Zustände diagnostisch sperren. Automatisches vollständiges NVS-Löschen bei Initialisierungsfehlern aus dem RX-Releasepfad entfernen. | `main/main.c`, `main/nvs_config.*`, `provision.py`, Provisionierungstests |
| **P0.4 Reconnect und Ressourcen vereinheitlichen** | Gemeinsamen Reconnect-Zustandsautomaten aus dem geprüften Stand übernehmen: keine überlappenden Verbindungsversuche, definierte Timerbehandlung, erfolgreiche IP-Zuweisung hat Vorrang. Socket-Erneuerung, ENOMEM-Backoff, Taskstart/-ende und Fehlerzähler konsistent behandeln. WLAN-/Hostausfall darf keine unkontrollierte Resetfolge erzeugen. | `main/main.c`, `main/stream_sender.*`, `main/csi_transport.*`, `main/wifi_reconnect.*` |
| **P0.5 CSI-Pfad konsolidieren** | Callback auf Prüfung und begrenzte Kopie reduzieren. Keine Netzübertragung, Flashoperation oder umfangreiche Ausgabe dort. Herkunftsfilter vor der Quotenvergabe; feste Poolgrenzen, eindeutiger Besitz und vollständige Bilanz aller Verwerfungen. Roh-IQ und Gültigkeitsmaske getrennt erhalten; Längen-/Layoutfehler explizit zurückweisen. | `main/csi_collector.*`, `main/csi_transport.*`, `main/csi_samples.h`, zu übernehmende Timing-/Peer-Helfer |

Die Speicher-/Stackprüfungen werden vereinheitlicht, nicht zugunsten eines kleineren Images abgeschaltet. Der belegte Capture-Pool ist Ausgangspunkt; parallele Queue-Implementierungen aus verschiedenen Varianten werden nicht nebeneinander erhalten.

### P1 — Alle absehbaren RX-Funktionen jetzt einbauen

**Peerfähigkeit für alle vier RX**

- Feste RX1↔RX4-Verzweigungen und die Zweilink-Struktur durch eine begrenzte Tabelle ersetzen: ein TX1-Link und bis zu drei Peer-Links pro RX.
- Profile: TX-only, Paarversuch, Vier-RX-Versuch sowie ausdrücklich konfigurierter Paar-/Vier-RX-Dauerbetrieb.
- Werkseinstellung und unvollständige Konfiguration bleiben TX-only beziehungsweise nicht messbereit.
- Tag14-Versuchsprofile behalten ihr begrenztes 900-Sekunden-Fenster. Dauerbetrieb erhält eine eigene Profilidentität und darf nicht als Tag14-Versuchsprofil erscheinen.
- Bei 50 Hz Gesamtbudget bleiben 40 Hz für TX1 und höchstens 10 Hz insgesamt für Peers vorgesehen. Im Vier-RX-Profil teilen sich drei Peerquellen diesen Anteil fair; **nicht 10 Hz je Peer**.
- Physische Quellen, Empfänger, Quoten und Überlastung pro Link ausweisen. Ein fehlender Link darf nicht durch einen anderen verdeckt werden.

Betroffen: `csi_peer.*`, `csi_collector.*`, `csi_transport.*`, `nvs_config.*`, `config_server.*` und die zugehörigen Host-Peerdecoder.

**Messvertrag für spätere Modelle und Raumwechsel**

- Bootidentität, Gerätezeit, Callback-/Queue-/Sendetiming, tatsächliches CSI-Layout, Gültigkeitsmaske und Konfigurationsgeneration eindeutig binden.
- Profil-/Konfigurationswechsel erzeugen eine erkennbare neue Messgeneration. Hostfenster und Modelle dürfen Daten verschiedener Generationen nicht unbemerkt mischen.
- Bestehende Wireformate weiter lesen können. Erforderliche Erweiterungen werden versioniert; keine Umdeutung reservierter Bits oder alter Felder.
- Gain-/Clippinginformationen nur melden, wenn sie auf dem gebundenen S3-/SDK-Stand tatsächlich verfügbar sind. Unbekannt bleibt unbekannt; keine automatische Gainkompensation oder Kanaländerung während einer Messung.
- Gerätefirmware liefert Daten und Diagnose. Personenanzahl, Position, Training und Radarzuordnung bleiben auf dem Host.

Hostseitig betroffen: `csi.rs`, `csi_identity.rs`, `csi_sample_timing.rs`, `csi_diagnostics.rs`, `raw_csi_recording.rs`, `raw_csi_replay.rs` und die `position_*`-Vertragsprüfungen.

**Einheitliche Diagnose, Bedienung und Wartung**

- Gemeinsames Diagnoseschema für alle RX: Ressourcen, Reconnect, Transport, Peerzustand, Resetgrund und erste Allokationsfehlermeldung.
- Vorhandenen begrenzten Ereignisring wiederverwenden; keine zweite Diagnoseplattform. Keine laufenden Ereigniswrites nach NVS.
- Identitätsfarbe und Gesundheitsanzeige trennen. Netzwerkverlust, fehlende gültige CSI, Queueüberlastung und interner Fehler müssen unterscheidbar sein.
- Authentisierte Capability-/Profilabfrage ergänzen. Konfigurationsänderungen müssen gespeicherten und tatsächlich aktiven Stand auseinanderhalten.
- OTA auf einen gemeinsamen geprüften Pfad bringen: Größen-/Partitionsprüfung, begrenzte Verarbeitung, eindeutiger Uploadabschluss und Bootvalidierung. Keine Installation parallel zu einem gültigen Messblock; keine versteckten Wiederholungen.

Betroffen: `rx_diag.*`, `diag_http.*`, `status_led.*`, `ota_update.*`, `config_server.*` sowie bestehende Host-Konfigurationsadapter.

### P2 — Installer und überflüssige Bestandteile konsolidieren

| Befund | Geplante Behandlung |
|---|---|
| Zwei unterschiedliche `provision.py`-Implementierungen | Eine maßgebliche Implementierung behalten; den anderen Einstieg zunächst als kompatible Weiterleitung ersetzen. Gerätegebundene Konfiguration statt bloßem Cache pro serieller Port. |
| Python-3.15-Syntax bei vorhandenem Python 3.14 | Provisionierung auf den festgelegten unterstützten Interpreterstand bringen und beide bisherigen Einstiege prüfen. |
| Provisionierung erzeugt ein NVS-Image aus Hostzustand | Für bestehende Geräte kein blindes Überschreiben. Reguläre Änderungen über validierte Gerätekonfiguration; initiale USB-Provisionierung bleibt ein ausdrücklich gesonderter Vorgang. |
| `commands/flash.rs::verify_firmware()` liefert `verified: true` ohne aktuellen Gerätevergleich | Keine positive Verifikation ohne passenden tatsächlichen Verifikationsbeleg. Datei-Hash, Programmergebnis, Readback und Bootprüfung getrennt darstellen. |
| `install.sh` ist breit, aber durch `Makefile` referenziert | Nicht pauschal löschen. Einen klaren Observatory-/RX-Einstieg schaffen und vorhandene Aufrufer erhalten. |
| `sdkconfig.defaults.8mb_backup`, Hello-World-Firmware, alte Releasebins und konkurrierende Profile | Aus dem regulären Releasepfad nehmen beziehungsweise als Legacy kennzeichnen. Entfernung erst nach Referenzprüfung und nachgewiesenem Ersatz. |
| WASM, Swarm, lokale Edge-Inferenz, RX-Radarautodetektion, C6-Sync/TWT/LP-Core | Im gemeinsamen S3-Messrelease nicht initialisieren oder unnötig mitbauen. Andere Produktprofile und die getrennte Radarfirmware erhalten. |
| Private Sicherungen, eingefrorene Quellen, Claims, Stops und Negativbelege | Keine Bereinigung. Sie bleiben nachvollziehbare Historie. |

Zusätzlich betroffen: `software/ruview/scripts/provision.py`, `software/ruview/install.sh`, `software/ruview/Makefile` und `wifi-densepose-control/src/commands/flash.rs`.

## 4. Abdeckung der noch offenen Projekttage

Grundlage ist der [Tracking-Zeitplan](./docs/project/tracking-zeitplan-20261025.md). Die alten Kalenderdaten ändern nicht die weiterhin offenen Nachweise.

| Projekttage | Jetzt vorzubereitende RX-Fähigkeit | Später verbleibende Arbeit |
|---|---|---|
| **14 – Datenbasis und Paarvergleich** | Einheitliche Quellen-/Zeit-/Layoutdiagnose, vollständige Bilanz, begrenztes Paarprofil | Neue reale Qualifikation der vereinheitlichten Firmware |
| **15 – Radar** | Verlässliche Zeit- und Konfigurationsidentität; keine RX-Radarautodetektion als Nebenwirkung | Radar separat qualifizieren und Referenz zuordnen |
| **16–18 – Daten, Modelle, Blindvergleich** | Unveränderte Rohdaten, Masken, pro Link erkennbare Qualität und fehlende Daten | Datensammlung, Training, unabhängige Nutzenbewertung |
| **19–20 – Live-Tracking und möglicher Vier-RX-Ausbau** | Vier-RX-Peerfähigkeit bereits im gemeinsamen Image; Topologie durch Konfiguration auswählbar | Nur bei belegtem Nutzen aktivieren und passende neue Daten aufnehmen |
| **21–22 – Freeze und zweiter Raum** | Reproduzierbare Profilbindung, geprüfte Neukonfiguration und Neustart | Neuer Raumaufbau, Kalibrierung und Blindnachweis |
| **23–25 – Blocker, Dauerlauf, Übergabe** | Einheitliche Diagnose, Speichergrenzen, Dauerbetriebsprofil und Wiederanlauf | Mindestens 60 Minuten Gesamtlauf, Bedienprüfung und Abnahme |
| **26–27 – Zwei Personen** | Derselbe Mehrlink-Rohdatenvertrag ohne geräteseitige Personenannahmen | Hostmodell, Zwei-Personen-Daten, Tracking und ID-Wechsel auswerten |
| **Docker/Homebrew** | Konfigurierbares Hostziel, Capability-/Versionsprüfung, stabile Schnittstellen | Hostpaket und Zweitrechnernachweis; kein anderer RX-Build |

R1/R4/R7 und die Fehlerhistorie werden Pflichtumfang. R3/T1 erhalten ehrliche Quellen-/Sequenzzuordnung ohne erfundene Paketkorrelation. T3 wird innerhalb ausgewiesener Ressourcenlimits konfigurierbar. R2/R5 erhalten Diagnose und eindeutige Konfigurationsidentität, aber keine ungeprüfte automatische Signalveränderung. R6 bleibt überwiegend Hostarbeit; T4 bleibt gesonderte Forschung.

## 5. Tests, Einführung und Kriterien für den Firmware-Freeze

**Offline vor jeder Installation**

- Gleicher App-Hash für vier unterschiedliche gültige Geräteprofile.
- Konfigurationsmigration, fehlende/duplizierte Identität, ungültige Kombinationen, Stromausfall beim Speichern und Erhalt unbekannter NVS-Schlüssel.
- Reconnect-Rennen, fehlgeschlagener Verbindungsstart, AP-Verlust, Zielwechsel, Socketfehler und wiederholter Start/Stop ohne Ressourcenleck.
- Callback-/Poolgrenzen, Queueüberlastung, Teilinitialisierung, Abbruch und vollständige Zählerbilanz.
- CSI-Golden-Fixtures für tatsächliche Layouts, ungültige Präfixe, Boot-/Zeitwechsel, unbekannte Versionen und Host-Replaygleichheit.
- Paar- und Vier-RX-Fairness, TX-Reservierung, fehlende Partner, Ablauf des Versuchsfensters und getrennte Dauerbetriebssemantik.
- OTA-Abbruch, fehlerhafte Images, unveränderte Partitionen und eindeutige Verifikationszustände.
- Bestehende C-Tests unter `test/Makefile`, Provisionierungstests und betroffene Hosttests erweitern. Keine Tests nur gegen nachgebildete Implementierung.

**Release-Artefakt**

Ein eingefrorenes S3-App-Image mit Quellmanifest, SDK-/Abhängigkeitspins, Buildkonfiguration, Ressourcen-/Slotbericht, Profilbeschreibung und kompatiblen Hostversionen. Kein geratenes „fertig“, wenn Größen-, Speicher- oder Schnittstellenprüfungen scheitern.

**Spätere Hardwareeinführung**

Die Vereinheitlichung wäre ein neuer Installationsauftrag für RX1–RX4. Sie ist ausdrücklich etwas anderes als der bisher vorbereitete NoFlash-Lauf. Alte Schreibrechte und RX4s verbrauchter Appwrite-Anspruch werden nicht wiederverwendet. TX1 bleibt unverändert.

Nach separater Freigabe zunächst ein geeigneter RX als Pilot, danach die übrigen Geräte einzeln. Je Gerät Installation, Verifikation, Boot, aktive Konfiguration und Ressourcen belegen. Kein automatischer zweiter Schreibversuch oder Restore.

**Freeze erst nach realem Nachweis**

Gemeinsame Vier-RX-Datenbasis, gültiger Paarvergleich, Wiederanlauf und mindestens 60 Minuten Dauerlauf müssen mit dem neuen Stand bestehen. Vier-RX-Peerbetrieb braucht zusätzlich seinen eigenen Link-/Budgetnachweis; sein Positionsnutzen bleibt eine spätere empirische Entscheidung.

Danach sollen die restlichen Projekttage durch Konfiguration, Messungen, Modelle und Auswertungen bearbeitbar sein. Funktionale Firmwareänderungen werden nur bei nachgewiesenen Lücken wieder geöffnet; die betroffenen Abnahmen und Datenbindungen sind dann ausdrücklich zu erneuern.

# 01 Projektjournal

Dieses Journal sammelt den Verlauf des Projekts in Berichtssprache. Es soll später helfen, Einleitung, Methodik, Diskussion und Fazit zu schreiben.

## Einträge

### 2026-06-26 — Projektstand vor erstem Versuch A

**Ausgangslage**

- Vier ESP32-S3-Boards sind vorhanden.
- Ein weiteres ESP32-S3-Board fehlt noch und soll später als RX4 ergänzt werden.
- Ein mmWave-Modul ist als Referenzsensor vorhanden.
- Die Softwarebasis für RX und Server soll RuView sein.

**Geplante Versuchsidee**

- Versuch A nutzt einen kontrollierten ESP32-Sender.
- Drei ESP32 dienen zunächst als CSI-Empfänger.
- mmWave wird nicht als Hauptsensor gewertet, sondern als Referenz zur Plausibilitätsprüfung.

**Relevanz für die Problemfrage**

Der erste Versuch soll klären, ob unter kontrollierten Bedingungen messbare CSI-Veränderungen bei Bewegung und ruhiger Atmung auftreten.

**Offene Punkte**

- Stabilität des CSI-Streams mit mehreren RX-Nodes.
- Sichtbarkeit von Bewegung in den Daten.
- Sichtbarkeit eines Atemrhythmus bei ruhiger Person.
- Qualität der mmWave-Referenzwerte.

### 2026-06-26 — TX-Firmware erfolgreich geflasht

**Ausgangslage**

Der erste ESP32-S3 soll im Versuch A als kontrollierter WLAN-Sender verwendet werden.

**Durchführung / Änderung**

Die TX-Firmware wurde per Arduino/esptool auf einen ESP32-S3 geflasht.

**Beobachtung**

Der Flashvorgang wurde erfolgreich abgeschlossen. Bootloader, Partitionstabelle, `boot_app0` und Sketch wurden geschrieben und jeweils per Hash verifiziert. Der Sketch nutzt 861280 Bytes Programmspeicher und 45676 Bytes dynamischen Speicher.

**Erfolg**

Der ESP32-S3 ist grundsätzlich als TX-Modul vorbereitet.

**Problem / Fehlschlag**

Noch nicht geprüft ist, ob das SoftAP-WLAN sichtbar ist und ob die AP-MAC-Adresse korrekt ausgegeben wird.

**Konsequenz für den nächsten Schritt**

Seriellen Monitor öffnen, TX-Startausgabe prüfen, AP-MAC notieren und Laptop mit dem `csi-test`-WLAN verbinden.

**Relevanz für den Bericht**

Der erste Hardware-/Software-Schritt war erfolgreich. Der eigentliche Messbetrieb ist damit aber noch nicht validiert.

### 2026-06-26 — TX-Start schlägt wegen Brownout fehl

**Ausgangslage**

Nach erfolgreichem Flash sollte der TX-ESP32 als SoftAP starten.

**Durchführung / Änderung**

Der serielle Monitor wurde geöffnet und der ESP32 startete aus dem Flash.

**Beobachtung**

Der Bootloader startet, bricht aber mit `E BOD: Brownout detector was triggered` ab.

**Erfolg**

Der ESP32 bootet grundsätzlich aus dem Flash, erreicht aber keinen stabilen Betrieb.

**Problem / Fehlschlag**

Die Versorgungsspannung bricht beim Start oder beim Aktivieren von WiFi vermutlich ein. Der Brownout-Detector löst aus und setzt den ESP32 zurück.

**Konsequenz für den nächsten Schritt**

Stromversorgung verbessern: kurzes gutes USB-Kabel, direkter USB-Port oder aktiver USB-Hub, alternativ 5V-Netzteil mit ausreichender Stromreserve. Erst danach erneut prüfen, ob SoftAP-IP und AP-MAC ausgegeben werden.

**Relevanz für den Bericht**

Der Versuch zeigt früh, dass stabile Stromversorgung eine Voraussetzung für reproduzierbare WLAN-CSI-Messungen ist. Instabile Versorgung kann Messungen verhindern oder verfälschen.

### 2026-06-26 — Brownout bleibt trotz reduzierter TX-Power bestehen

**Ausgangslage**

Zur Reduktion möglicher WiFi-Stromspitzen wurde im TX-Sketch die Sendeleistung reduziert.

**Durchführung / Änderung**

Im Sketch wurde `WiFi.setTxPower(WIFI_POWER_11dBm);` ergänzt.

**Beobachtung**

Der ESP32-S3 startet weiterhin mit `E BOD: Brownout detector was triggered` neu.

**Problem / Fehlschlag**

Die Reduktion der WiFi-Sendeleistung reicht nicht aus oder der Brownout tritt bereits vor bzw. sehr früh während der WiFi-/Boardinitialisierung auf.

**Konsequenz für den nächsten Schritt**

Der Fehler muss jetzt isoliert werden: Minimal-Sketch ohne WiFi testen, danach SoftAP mit verzögertem Start testen und parallel die Stromversorgung verbessern.

**Relevanz für den Bericht**

Der Fehlschlag zeigt, dass Softwareparameter allein nicht ausreichen, wenn die Versorgung oder das Board-Setup instabil ist.

### 2026-06-26 — Brownout-Ursache eingegrenzt und TX erfolgreich gestartet

**Ausgangslage**

Der TX-ESP32 löste beim WiFi-/SoftAP-Start einen Brownout aus.

**Durchführung / Änderung**

Zuerst wurde ein Minimal-Sketch ohne WiFi getestet. Dieser lief stabil. Danach wurde der SoftAP-Sketch mit verzögertem WiFi-Start getestet. Mit dem ursprünglichen Kabel trat der Brownout beim WiFi-Start auf. Nach Wechsel des USB-Kabels startete der SoftAP erfolgreich.

**Beobachtung**

Der Minimal-Sketch gab wiederholt `alive` aus. Der SoftAP-Start funktionierte mit anderem Kabel:

- AP IP: `192.168.4.1`
- AP MAC: `AE:27:6E:A8:D2:64`

**Erfolg**

Der TX ist jetzt betriebsbereit und stellt das Test-WLAN bereit.

**Problem / Fehlschlag**

Das erste USB-Kabel war für den WiFi-Start bzw. die Stromspitzen ungeeignet.

**Konsequenz für den nächsten Schritt**

Für weitere Messungen nur das stabile Kabel bzw. eine stabile Stromversorgung verwenden. Als nächstes Laptop mit dem TX-WLAN verbinden und RX1-RX3 provisionieren.

**Relevanz für den Bericht**

Die Ursache zeigt, dass Versorgung und Kabelqualität praktische Einflussfaktoren sind. Für reproduzierbare Messungen müssen sie kontrolliert werden.

### 2026-06-26 — Python-Setup-Hürde: esptool fehlt

**Ausgangslage**

Nach erfolgreichem TX-Start sollte RX1 mit RuView-Firmware geflasht werden.

**Durchführung / Änderung**

Auf dem Mac wurde `python3 -m esptool --version` ausgeführt.

**Beobachtung**

Python meldete: `No module named esptool`.

**Problem / Fehlschlag**

Die für das manuelle Flashen/Provisionieren benötigten Python-Tools sind in der aktiven Python-Installation noch nicht vorhanden.

**Konsequenz für den nächsten Schritt**

Eine lokale virtuelle Python-Umgebung im RuView-Projekt anlegen und darin `esptool` sowie `nvs-partition-gen` installieren.

**Relevanz für den Bericht**

Der Aufbau benötigt neben Hardware auch eine reproduzierbare Toolchain. Fehlende Tools sind eine praktische Setup-Hürde, aber keine technische Grenze von WLAN-CSI.

### 2026-06-26 — Lokale RuView-Python-Toolchain eingerichtet

**Ausgangslage**

Für das Flashen und Provisionieren der RX-ESP32 werden `esptool` und ein NVS-Partition-Generator benötigt.

**Durchführung / Änderung**

Im RuView-Projekt wurde eine lokale `.venv` verwendet. Installiert wurden `esptool>=5.0` und `esp-idf-nvs-partition-gen`.

**Beobachtung**

Installierte Versionen:

- `esptool 5.3.0`
- `esp-idf-nvs-partition-gen 0.2.0`

**Erfolg**

Die lokale Toolchain ist für das Flashen und Provisionieren vorbereitet.

**Problem / Fehlschlag**

Der zuerst verwendete Paketname `nvs-partition-gen` war nicht verfügbar. Der korrekte Paketname ist `esp-idf-nvs-partition-gen`.

**Konsequenz für den nächsten Schritt**

RX1 kann mit RuView-Firmware geflasht und danach per `provision.py` für das Test-WLAN konfiguriert werden.

**Relevanz für den Bericht**

Die Einrichtung zeigt, dass eine reproduzierbare lokale Toolchain nötig ist. Fehlerhafte Paketnamen oder fehlende Tools können den Aufbau verzögern.

### 2026-06-26 — Rust/Cargo fehlt für RuView-Sensing-Server

**Ausgangslage**

Nach dem Flashen und Provisionieren von RX1 sollte der RuView-Sensing-Server gebaut bzw. gestartet werden.

**Durchführung / Änderung**

Im RuView-Ordner wurde versucht, den Server mit `cargo build --release -p wifi-densepose-sensing-server` zu bauen.

**Beobachtung**

Die Shell meldete `zsh: command not found: cargo`.

**Problem / Fehlschlag**

Rust/Cargo ist auf dem Mac noch nicht installiert oder nicht im PATH.

**Konsequenz für den nächsten Schritt**

Rust über `rustup` installieren und danach den Server erneut bauen. Alternativ könnte Docker genutzt werden, aber für den Live-ESP32-Test ist ein lokaler Rust-Build der direktere Weg.

**Relevanz für den Bericht**

Neben Python-Tools ist auch eine Rust-Toolchain erforderlich, um den RuView-Server lokal auszuführen.

### 2026-06-26 — RuView-Submodule nachgezogen und Sensing-Server erfolgreich gebaut

**Ausgangslage**

Der Build des RuView-Sensing-Servers brach ab, weil `vendor/rufield/crates/rufield-core/Cargo.toml` fehlte.

**Durchführung / Änderung**

Die Git-Submodule wurden rekursiv initialisiert und aktualisiert. Danach wurde der Server erneut mit `cargo build --release -p wifi-densepose-sensing-server` gebaut.

**Beobachtung**

Der fehlende `rufield-core`-Pfad wurde danach gefunden und kompiliert. Der Build endete mit `Finished release profile`.

**Erfolg**

Der RuView-Sensing-Server wurde erfolgreich lokal gebaut.

**Problem / Fehlschlag**

Der vorherige Fehler entstand durch unvollständig geladene Repository-Abhängigkeiten/Submodule.

**Konsequenz für den nächsten Schritt**

Der Server kann lokal gestartet werden und auf UDP-Port `5005` CSI-Daten vom provisionierten RX1 empfangen.

**Relevanz für den Bericht**

Die Softwarekette besteht nicht nur aus ESP32-Firmware, sondern auch aus einem lokalen Server zur Datenerfassung. Unvollständige Abhängigkeiten sind ein reproduzierbarer Setup-Faktor.

### 2026-06-26 — Erster gültiger ESP32-CSI-Frame im RuView-Server empfangen

**Ausgangslage**

RX1 war geflasht, provisioniert und sendete UDP-Pakete an den Mac auf Port `5005`.

**Durchführung / Änderung**

Der RuView-Sensing-Server wurde mit `RUST_LOG=debug` gestartet, um akzeptierte ESP32-Frames sichtbar zu machen.

**Beobachtung**

Der Server meldete `ESP32 frame from 192.168.4.3:54714: node=1, subs=64, seq=0`.

**Erfolg**

Mindestens ein empfangener UDP-Datagramm wurde vom RuView-Server als gültiger ESP32-CSI-Frame erkannt und verarbeitet.

**Problem / Fehlschlag**

Im Log ist bisher nur ein akzeptierter Frame sichtbar. Für Messreihen muss geprüft werden, ob kontinuierlich Frames verarbeitet werden oder ob viele UDP-Pakete nicht dem erwarteten Raw-CSI-Format entsprechen.

**Konsequenz für den nächsten Schritt**

Über `/api/v1/sensing/latest` und weitere Debug-Logs prüfen, ob laufend aktuelle Sensordaten entstehen. Danach erste einfache Bewegungstests durchführen.

**Relevanz für den Bericht**

Dies ist der erste Nachweis, dass die ESP32-RX-zu-Server-Kette grundsätzlich funktioniert.

### 2026-06-26 — RuView `/api/v1/sensing/latest` liefert verarbeitete ESP32-Daten

**Ausgangslage**

Nach dem ersten gültigen CSI-Frame musste geprüft werden, ob der Server daraus auch einen aktuellen Sensordatenzustand erzeugt.

**Durchführung / Änderung**

Der API-Endpunkt `http://localhost:8080/api/v1/sensing/latest` wurde per `curl` abgefragt.

**Beobachtung**

Die Antwort enthielt `source: esp32`, `type: sensing_update`, `node_id: 1`, `mean_rssi: -53.0`, `estimated_persons: 1`, eine Amplitudenliste mit Subcarrierwerten und eine Klassifikation `present_moving`.

**Erfolg**

Die Kette TX-WLAN → RX1-CSI → UDP → RuView-Server → API-Ausgabe funktioniert grundsätzlich.

**Problem / Fehlschlag**

Die Vitalwerte waren noch `null` und die ausgegebenen Personen-/Pose-Daten sind in diesem frühen Zustand nicht als zuverlässige Messung zu interpretieren. Für Atem- und Bewegungsanalyse wird ein stabiler Zeitverlauf benötigt.

**Konsequenz für den nächsten Schritt**

Als nächstes werden einfache Messreihen mit Zeitstempeln durchgeführt: leerer Raum, Bewegung vor dem Link, ruhiges Stehen/Sitzen und ruhige Atmung. Dabei werden `mean_rssi`, `motion_band_power`, `breathing_band_power`, `variance`, `presence` und Referenzbeobachtungen protokolliert.

**Relevanz für den Bericht**

Dieser Schritt belegt die technische Machbarkeit der Datenerfassung und trennt gleichzeitig Roh-/Feature-Ausgabe von noch nicht validierter Personen- oder Vitalzeichenerkennung.

### 2026-06-26 — Kontinuierlicher UDP-Strom besteht überwiegend aus Feature-State-Paketen

**Ausgangslage**

Der API-`tick` blieb nach wenigen gültigen CSI-Frames stehen, obwohl der RX-Knoten per Ping erreichbar war.

**Durchführung / Änderung**

Mit `sudo ping -i 0.1 192.168.4.3` wurde aktiver Datenverkehr erzeugt. Anschließend wurden UDP-Pakete auf Port `5005` per `tcpdump -X` betrachtet.

**Beobachtung**

Der Ping zu RX1 war stabil mit `0.0% packet loss`. Die kontinuierlich eintreffenden UDP-Pakete hatten `length 60` und begannen im Payload mit `06 00 11 c5`, also `0xC5110006` in Little Endian. Laut Firmware ist das ein ADR-081 `feature_state`-Paket, nicht ein ADR-018 Raw-CSI-Frame (`0xC5110001`).

**Erfolg**

Die Netzwerkverbindung zu RX1 ist stabil, und RX1 sendet kontinuierlich Sensordatenpakete an den Mac.

**Problem / Fehlschlag**

RuViews `/api/v1/sensing/latest` wird für die bisherigen Abfragen hauptsächlich durch Raw-CSI-Frames aktualisiert. Die kontinuierlichen 60-Byte-Feature-State-Pakete erklären, warum UDP sichtbar ist, aber der `tick` nicht kontinuierlich steigt.

**Konsequenz für den nächsten Schritt**

Für Versuch A muss entschieden werden, ob die Messung zunächst auf Feature-State-Werten basiert oder ob Raw-CSI als kontinuierlicher Stream erzwungen/anders geflasht werden soll.

**Relevanz für den Bericht**

Dieser Befund ist wichtig für die Methodik: Es muss klar getrennt werden zwischen echten Raw-CSI-Daten und bereits vorverarbeiteten Feature-Daten aus dem ESP32.

### 2026-06-26 — Kontinuierlicher Raw-CSI-Stream nach Entfernen des MAC-Filters

**Ausgangslage**

Der RX-Knoten sendete kontinuierlich Feature-State-Pakete, aber nur sporadisch Raw-CSI-Frames. Zuvor war ein MAC-Filter auf die TX/AP-MAC gesetzt.

**Durchführung / Änderung**

RX1 wurde ohne `--filter-mac` neu provisioniert. Danach wurde durch Ping-Verkehr zu `192.168.4.3` zusätzlicher WLAN-Datenverkehr erzeugt.

**Beobachtung**

Der Server loggte fortlaufende Raw-CSI-Frames, z. B. `node=1, subs=64, seq=118` bis `seq=129`, sowie ein Sync-Paket. Der Sequenzzähler stieg kontinuierlich.

**Erfolg**

Der gewünschte kontinuierliche Raw-CSI-Datenstrom wurde erreicht.

**Problem / Fehlschlag**

Der vorherige MAC-Filter war für diesen Testaufbau zu restriktiv. Er unterdrückte offenbar viele verwertbare CSI-Frames, weil nicht alle relevanten Datenframes von der gefilterten AP-MAC kamen.

**Konsequenz für den nächsten Schritt**

Für den ersten Versuch A bleibt RX1 ohne MAC-Filter. Messreihen können jetzt mit laufendem `seq`/`tick` gestartet werden.

**Relevanz für den Bericht**

Der Aufbau zeigt eine wichtige praktische Grenze: Eine zu starke Filterung verbessert zwar theoretisch die Eindeutigkeit des Links, kann aber in der Praxis den CSI-Datenstrom ausdünnen und Messungen unbrauchbar machen.

### 2026-06-27 — RX3 provisioniert und Brownout behoben, 3RX-Live-Test noch offen

**Ausgangslage**

Nach dem 2RX-Lauf fehlte im RuView-Server weiterhin ein dritter Knoten. Der Server hatte nur Frames von `node=1` (`192.168.4.2`) und `node=2` (`192.168.4.5`) gesehen. Der Mac war im Testnetz zuvor `192.168.4.4`; im normalen Internet-WLAN hatte er dagegen `192.168.178.123`.

**Durchführung / Änderung**

RX3 wurde ueber USB erneut provisioniert. Der Port enumerierte zuerst als `/dev/cu.usbmodem5C4C0893221` und spaeter als `/dev/cu.usbmodem101`. Die NVS-Konfiguration wurde mit `--reset` neu geschrieben:

- SSID: `csi-test`
- Target: `192.168.4.4:5005`
- Node ID: `3`
- Edge Tier: `0`
- Channel: `6`

Anschliessend wurde der serielle Bootlog geprueft.

**Beobachtung**

Die NVS-Werte wurden korrekt geladen: `node_id=3`, `edge_tier=0`, `csi_channel=6`, `target_ip=192.168.4.4`, `target_port=5005`. Beim ersten Check trat beim WiFi-/PHY-Start ein Brownout auf. Nach Wechsel bzw. Stabilisierung der Stromversorgung bootete RX3 ohne Brownout, verband sich mit `csi-test`, initialisierte CSI und meldete `CSI streaming active -> 192.168.4.4:5005`.

**Erfolg**

RX3 ist firmwareseitig und NVS-seitig korrekt als `node_id=3` eingerichtet. Der spaetere serielle Check zeigte `brownout_count=0`, `Connected to WiFi`, `CSI collection initialized` und aktives CSI-Streaming.

**Problem / Fehlschlag**

Der 3RX-Live-Test im RuView-Server ist noch nicht nachgewiesen. Solange der Mac im normalen WLAN bleibt, besitzt er nicht die Zieladresse `192.168.4.4`. Beim seriellen Check bekam RX3 selbst die IP `192.168.4.4`; dadurch wuerde RX3 seine UDP-Pakete an sich selbst statt an den Mac senden.

**Konsequenz für den nächsten Schritt**

Fuer den 3RX-Test muss der Mac wieder in das `csi-test`-Netz und eine stabile Ziel-IP bekommen. Robuster als DHCP ist eine feste Mac-IP im Testnetz, z. B. `192.168.4.50`, und danach erneutes Provisionieren von RX1, RX2 und RX3 auf `target_ip=192.168.4.50`. Erst danach ist `/api/v1/nodes` mit `total=3` bzw. Server-Logs mit `node=1`, `node=2` und `node=3` der eigentliche 3RX-Nachweis.

**Relevanz für den Bericht**

Der Befund trennt drei Ursachen sauber: Provisionierung ist korrekt, Stromversorgung kann WiFi-Starts verhindern, und DHCP-/Ziel-IP-Konflikte koennen einen funktionsfaehigen RX unsichtbar fuer den Server machen.

### 2026-06-27 — Vier RX-Knoten senden gleichzeitig Raw-CSI an RuView

**Ausgangslage**

Der vierte ESP32-S3 ist angekommen und wurde als RX4 eingerichtet. Im `csi-test`-Netz hatte der Mac die IP `192.168.4.5`.

**Durchführung / Änderung**

RX1 bis RX4 wurden auf die aktuelle Mac-Zieladresse `192.168.4.5:5005` provisioniert bzw. erneut provisioniert. Anschließend wurde für alle sichtbaren RX-IP-Adressen Ping-Verkehr erzeugt.

**Beobachtung**

Der RuView-Server empfing Raw-CSI-Frames von vier Nodes:

- `node=1` von `192.168.4.2`
- `node=2` von `192.168.4.3`
- `node=3` von `192.168.4.4`
- `node=4` von `192.168.4.6`

Alle gemeldeten Frames hatten `subs=64`.

**Erfolg**

Der Vier-Empfänger-Aufbau ist online. Alle vier RX-Knoten liefern gleichzeitig Raw-CSI-Daten an den RuView-Server.

**Problem / Fehlschlag**

RuView meldet weiterhin `Multistatic fusion failed`, weil die Zeitspreizung der Frames größer als das aktuelle Guard-Intervall ist. Der Server nutzt deshalb den per-node Fallback statt vollständig synchroner Multistatic-Fusion.

**Konsequenz für den nächsten Schritt**

Als nächstes kann die RuView-Visualisierung mit vier Nodes geöffnet und geprüft werden. Für belastbarere Position/Zonenmessungen müssen anschließend Node-Positionen, Traffic-Erzeugung und Timing/Synchronisation verbessert werden.

**Relevanz für den Bericht**

Dies ist der erste erfolgreiche 4RX-Meilenstein. Gleichzeitig zeigt er eine zentrale physikalisch-technische Grenze: Mehr Empfänger liefern mehr räumliche Information, aber zeitliche Synchronisation ist für echte Fusion kritisch.

### 2026-06-28 — OTA erreichbar, aber Host-IP-Konflikt bei Mehrknotenbetrieb erkannt

**Ausgangslage**

Nach dem 4RX-Aufbau wurde erneut geprüft, welche Geräte im `csi-test`-Netz erreichbar sind. Der Mac bekam per DHCP wechselnde Adressen; gleichzeitig waren ESPs auf `192.168.4.2` bis `192.168.4.5` erreichbar.

**Durchführung / Änderung**

Die OTA-Status-Endpunkte der RX-Knoten wurden über WLAN geprüft. `192.168.4.2`, `.3`, `.4` und `.5` antworteten auf `GET /ota/status`.

**Beobachtung**

`192.168.4.5` ist aktuell ein ESP32-Knoten und darf deshalb nicht als feste Mac-Ziel-IP verwendet werden. Andernfalls senden RX-Knoten ihre UDP-/CSI-Daten an einen anderen ESP statt an den RuView-Server.

**Erfolg**

Die RX-Knoten sind über WLAN administrierbar genug, um OTA-Status abzufragen. Damit ist ein Firmware-Update ohne erneutes USB-Anschließen grundsätzlich realistisch, sofern der OTA-Upload akzeptiert wird.

**Problem / Fehlschlag**

Die bisherige Strategie „Mac-Ziel-IP = aktuelle DHCP-IP“ ist bei mehreren ESPs nicht robust. DHCP kann die Zieladresse später an einen ESP vergeben.

**Konsequenz für den nächsten Schritt**

Als stabile Host-Adresse sollte eine freie Adresse außerhalb der bisherigen DHCP-Vergabe genutzt werden, z. B. `192.168.4.50`. Zusätzlich wird eine Firmware-Erweiterung vorbereitet, damit ausgewählte NVS-Werte künftig per HTTP `/config` über WLAN geändert werden können.

**Relevanz für den Bericht**

Der Befund ist ein gutes Beispiel für eine nicht-physikalische, aber messpraktisch wichtige Grenze: Ein Mehrknoten-CSI-System braucht nicht nur Funkempfang, sondern auch stabile Netzwerkadressierung und reproduzierbare Konfiguration.

### 2026-06-28 — Messreihen A0 bis A3 gespeichert und Timestamp-Fusion als Grenze eingeordnet

**Ausgangslage**

Der 4RX-Aufbau lieferte stabil Node-IDs `1`, `2`, `3` und `4`. Anschließend wurden vier erste Messreihen über den RuView-API-Logger gespeichert:

- A0: leerer Raum, 60 s
- A1: Person steht in der Mitte, 60 s
- A2: Person läuft langsam durch den Raum, 60 s
- A3: Person sitzt ruhig und atmet normal, 180 s

**Durchführung / Änderung**

Die Messdaten wurden automatisch als `raw_sensing.jsonl`, `summary.csv`, `metadata.json` und `errors.log` unter `data/raw/` gespeichert. Zusätzlich wurde ein Qualitätscheck unter `results/2026-06-28_A0-A3_qualitaetscheck.md` erstellt.

**Beobachtung**

Alle vier Nodes waren in allen Messreihen sichtbar. Die Messungen hatten keine Logger-Fehler. Die vollständigen 4x64-Subcarrier-Snapshots lagen je nach Messung zwischen 75,0 % und 96,7 %. A0 zeigte jedoch fast durchgehend `presence=True`, obwohl der Raum als leer dokumentiert wurde. Vitalwerte wurden sogar in A0 ausgegeben.

**Erfolg**

Die Softwarekette vom ESP32-CSI-Empfang über RuView bis zur Dateiablage funktioniert für vier RX-Knoten. Die Daten reichen für eine erste Auswertung zu Bewegung/Signaländerung und für die Dokumentation von Systemgrenzen.

**Problem / Fehlschlag**

RuView meldete bei Mehrknotenbetrieb weiterhin `Multistatic fusion failed`, weil die Zeitspreizung der Frames das Standard-Guard-Intervall von 60 ms überschreitet. Dadurch nutzt RuView den per-node sum/dedup fallback statt einer sauberen synchronen multistatischen Fusion.

**Konsequenz für den nächsten Schritt**

Für die Visualisierung wird als pragmatischer Workaround das Guard-Intervall des Servers erhöht:

- Standard: `60000 us` hard / `20000 us` soft
- Testwert: `WDP_GUARD_INTERVAL_US=500000`, `WDP_SOFT_GUARD_US=200000`

Das Ziel ist zunächst nur, die RuView-Visualisierung stabiler zu machen und weniger Fallback-Meldungen zu erzeugen. Für physikalisch saubere Positions- oder Atemauswertung bleibt spätere Synchronisation/TDM nötig.

**Relevanz für den Bericht**

Der Befund ist zentral für die Problemfrage: WLAN-CSI kann mit mehreren Empfängern Bewegungsinformationen liefern, aber genaue räumliche Fusion ist nicht allein eine Frage der Empfängeranzahl. Zeitliche Synchronisation und Paketankunft begrenzen die Genauigkeit.

### 2026-06-28 — G1 Guard-Intervall-Test mit 500 ms durchgeführt

**Ausgangslage**

Nach den Messreihen A0 bis A3 wurde der RuView-Server mit einem größeren Guard-Intervall für die multistatische Fusion gestartet:

- `WDP_GUARD_INTERVAL_US=500000`
- `WDP_SOFT_GUARD_US=200000`

**Durchführung / Änderung**

Die Messreihe G1 wurde mit vier erwarteten Nodes und laufender Person aufgenommen. Der Logger speicherte die Daten unter `data/raw/2026-06-28_01-24-12_G1_guard500ms_person_laeuft`.

**Beobachtung**

G1 enthält 60 CSV-Samples und 60 JSONL-Samples ohne Logger-Fehler. Alle Samples enthalten die Node-IDs `1`, `2`, `3` und `4`. 56/60 Samples enthalten vollständige 4x64-Subcarrier-Daten.

**Erfolg**

Die 4RX-Datenerfassung blieb auch mit größerem Guard-Intervall stabil. Der Qualitätscheck wurde unter `results/2026-06-28_G1_guard500ms_qualitaetscheck.md` gespeichert.

**Problem / Fehlschlag**

Der API-Logger speichert keine Serverlog-Zeilen. Ein nachträglich geprüfter Serverlog-Ausschnitt zum G1-Test enthielt 50 Logzeilen, 47 ESP32-Frame-Zeilen, alle vier Nodes und keine `Multistatic fusion failed`-Meldung. Das ist ein positiver Hinweis, aber noch kein vollständiger Log über die ganze Messdauer.

**Konsequenz für den nächsten Schritt**

500 ms bleibt vorerst als Visualisierungs-Workaround dokumentiert. Für spätere Tests sollte der Serverlog parallel vollständig in eine Datei geschrieben werden, damit die Rate der Fusion-Fallbacks über die gesamte Messdauer gezählt werden kann.

**Relevanz für den Bericht**

Der Test trennt Datenerfassung und Fusion: Vier Nodes können stabil Daten liefern, während die Qualität der Fusion zusätzlich vom gewählten Zeitfenster abhängt.

### 2026-06-28 — G2 mit besser verteilten RX-Modulen und springender Webvisualisierung

**Ausgangslage**

Die RX-ESP32-Module wurden besser im Raum verteilt. Der Server lief weiter mit `WDP_GUARD_INTERVAL_US=500000` und `WDP_SOFT_GUARD_US=200000`.

**Durchführung / Änderung**

Es wurden zwei G2-Messungen gespeichert:

- `G2_guard500ms_rx_besser_verteilt_person_laeuft`
- `G2_empty_rx_besser_verteilt`

Der Serverlog wurde parallel in `logs/G2_besser_verteilte_rx_server.log` gespeichert.

**Beobachtung**

Die technische Erfassung war stabil: beide G2-Messungen hatten 60/60 Samples mit den Node-IDs `1`, `2`, `3` und `4`. Vollständige 4x64-Subcarrier-Daten lagen bei 96,7 % bzw. 98,3 %. Im Serverlog standen 16.097 ESP32-Frames und 31 `Multistatic fusion failed`-Meldungen; die restlichen Fallbacks lagen bei Timestamp-Spreads knapp über 500 ms.

Die Webansicht zeigte jedoch eine stark springende Heatmap und mehrere Pose-/Personhypothesen. Auch der leere Raum wurde in 60/60 Samples als `presence=True` und `estimated_persons=1` klassifiziert.

**Erfolg**

Die bessere RX-Verteilung liefert sehr vollständige 4RX-Daten. Der 500-ms-Guard ist als Visualisierungs-Workaround grundsätzlich brauchbar, weil nur noch wenige Fusion-Fallbacks auftreten.

**Problem / Fehlschlag**

Die Webvisualisierung ist aktuell nicht als zuverlässige Positionserkennung verwendbar. Sie springt, weil die Klassifikation/Heatmap auch im leeren Raum eine Person annimmt und weil die reale RX-Geometrie noch nicht als `--node-positions` an den Server übergeben wurde.

**Konsequenz für den nächsten Schritt**

Als nächstes sollten die realen RX-Positionen im Raum gemessen und beim Serverstart mit `--node-positions "x,y,z;x,y,z;..."` gesetzt werden. Zusätzlich muss eine leere-Raum-Baseline/Kalibrierung genutzt werden. Bis dahin sollten für die Auswertung primär CSV-/Feature-Daten verwendet werden, nicht die springende Pose-Visualisierung.

**Relevanz für den Bericht**

Der Befund zeigt eine wichtige Grenze: Stabile CSI-Datenerfassung bedeutet nicht automatisch stabile räumliche Visualisierung. Für Positionsschätzung braucht das System Geometrie, Kalibrierung und robuste Filterung.

## Vorlage für neue Journaleinträge

### Datum — Kurzbeschreibung

**Ausgangslage**

-

**Durchführung / Änderung**

-

**Beobachtung**

-

**Erfolg**

-

**Problem / Fehlschlag**

-

**Konsequenz für den nächsten Schritt**

-

**Relevanz für den Bericht**

-

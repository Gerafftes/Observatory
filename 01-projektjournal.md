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

Seriellen Monitor öffnen, TX-Startausgabe prüfen, AP-MAC notieren und Laptop mit dem `CSI_SSID`-WLAN verbinden.

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

- AP IP: `CSI_AP_IP`
- AP MAC: `TX_MAC_REDACTED`

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

Der Server meldete `ESP32 frame from RX2_IP:54714: node=1, subs=64, seq=0`.

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

Mit `sudo ping -i 0.1 RX2_IP` wurde aktiver Datenverkehr erzeugt. Anschließend wurden UDP-Pakete auf Port `5005` per `tcpdump -X` betrachtet.

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

RX1 wurde ohne `--filter-mac` neu provisioniert. Danach wurde durch Ping-Verkehr zu `RX2_IP` zusätzlicher WLAN-Datenverkehr erzeugt.

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

Nach dem 2RX-Lauf fehlte im RuView-Server weiterhin ein dritter Knoten. Der Server hatte nur Frames von `node=1` (`RX1_IP`) und `node=2` (`RX4_IP`) gesehen. Der Mac war im Testnetz zuvor `RX3_IP`; im normalen Internet-WLAN hatte er dagegen `HOME_LAN_IP`.

**Durchführung / Änderung**

RX3 wurde ueber USB erneut provisioniert. Der Port enumerierte zuerst als `/dev/cu.usbmodem5C4C0893221` und spaeter als `/dev/cu.usbmodem101`. Die NVS-Konfiguration wurde mit `--reset` neu geschrieben:

- SSID: `CSI_SSID`
- Target: `RX3_IP:5005`
- Node ID: `3`
- Edge Tier: `0`
- Channel: `6`

Anschliessend wurde der serielle Bootlog geprueft.

**Beobachtung**

Die NVS-Werte wurden korrekt geladen: `node_id=3`, `edge_tier=0`, `csi_channel=6`, `target_ip=RX3_IP`, `target_port=5005`. Beim ersten Check trat beim WiFi-/PHY-Start ein Brownout auf. Nach Wechsel bzw. Stabilisierung der Stromversorgung bootete RX3 ohne Brownout, verband sich mit `CSI_SSID`, initialisierte CSI und meldete `CSI streaming active -> RX3_IP:5005`.

**Erfolg**

RX3 ist firmwareseitig und NVS-seitig korrekt als `node_id=3` eingerichtet. Der spaetere serielle Check zeigte `brownout_count=0`, `Connected to WiFi`, `CSI collection initialized` und aktives CSI-Streaming.

**Problem / Fehlschlag**

Der 3RX-Live-Test im RuView-Server ist noch nicht nachgewiesen. Solange der Mac im normalen WLAN bleibt, besitzt er nicht die Zieladresse `RX3_IP`. Beim seriellen Check bekam RX3 selbst die IP `RX3_IP`; dadurch wuerde RX3 seine UDP-Pakete an sich selbst statt an den Mac senden.

**Konsequenz für den nächsten Schritt**

Fuer den 3RX-Test muss der Mac wieder in das `CSI_SSID`-Netz und eine stabile Ziel-IP bekommen. Robuster als DHCP ist eine feste Mac-IP im Testnetz, z. B. `CSI_HOST_IP`, und danach erneutes Provisionieren von RX1, RX2 und RX3 auf `target_ip=CSI_HOST_IP`. Erst danach ist `/api/v1/nodes` mit `total=3` bzw. Server-Logs mit `node=1`, `node=2` und `node=3` der eigentliche 3RX-Nachweis.

**Relevanz für den Bericht**

Der Befund trennt drei Ursachen sauber: Provisionierung ist korrekt, Stromversorgung kann WiFi-Starts verhindern, und DHCP-/Ziel-IP-Konflikte koennen einen funktionsfaehigen RX unsichtbar fuer den Server machen.

### 2026-06-27 — Vier RX-Knoten senden gleichzeitig Raw-CSI an RuView

**Ausgangslage**

Der vierte ESP32-S3 ist angekommen und wurde als RX4 eingerichtet. Im `CSI_SSID`-Netz hatte der Mac die IP `RX4_IP`.

**Durchführung / Änderung**

RX1 bis RX4 wurden auf die aktuelle Mac-Zieladresse `RX4_IP:5005` provisioniert bzw. erneut provisioniert. Anschließend wurde für alle sichtbaren RX-IP-Adressen Ping-Verkehr erzeugt.

**Beobachtung**

Der RuView-Server empfing Raw-CSI-Frames von vier Nodes:

- `node=1` von `RX1_IP`
- `node=2` von `RX2_IP`
- `node=3` von `RX3_IP`
- `node=4` von `RX_DHCP_IP`

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

Nach dem 4RX-Aufbau wurde erneut geprüft, welche Geräte im `CSI_SSID`-Netz erreichbar sind. Der Mac bekam per DHCP wechselnde Adressen; gleichzeitig waren ESPs auf `RX1_IP` bis `RX4_IP` erreichbar.

**Durchführung / Änderung**

Die OTA-Status-Endpunkte der RX-Knoten wurden über WLAN geprüft. `RX1_IP`, `.3`, `.4` und `.5` antworteten auf `GET /ota/status`.

**Beobachtung**

`RX4_IP` ist aktuell ein ESP32-Knoten und darf deshalb nicht als feste Mac-Ziel-IP verwendet werden. Andernfalls senden RX-Knoten ihre UDP-/CSI-Daten an einen anderen ESP statt an den RuView-Server.

**Erfolg**

Die RX-Knoten sind über WLAN administrierbar genug, um OTA-Status abzufragen. Damit ist ein Firmware-Update ohne erneutes USB-Anschließen grundsätzlich realistisch, sofern der OTA-Upload akzeptiert wird.

**Problem / Fehlschlag**

Die bisherige Strategie „Mac-Ziel-IP = aktuelle DHCP-IP“ ist bei mehreren ESPs nicht robust. DHCP kann die Zieladresse später an einen ESP vergeben.

**Konsequenz für den nächsten Schritt**

Als stabile Host-Adresse sollte eine freie Adresse außerhalb der bisherigen DHCP-Vergabe genutzt werden, z. B. `CSI_HOST_IP`. Zusätzlich wird eine Firmware-Erweiterung vorbereitet, damit ausgewählte NVS-Werte künftig per HTTP `/config` über WLAN geändert werden können.

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

### 2026-07-18 — Fester 1TX-/4RX-Raumaufbau und Diagnose der Live-Visualisierung

**Ausgangslage**

Nach den G2-Tests wurden Raum, TX und alle vier RX fest vermessen. Das mmWave-Modul wurde bewusst auf eine spätere Phase verschoben. Ziel war zunächst eine nachvollziehbare visuelle Darstellung ausschließlich mit WLAN-CSI.

**Durchführung / Änderung**

Die Raummaße `4,02 m × 3,44 m × 2,59 m` und die fünf Gerätepositionen wurden in das RuView-Koordinatensystem übertragen. RuView wurde mit vier Node-Positionen, TX-Position und Raummaßen gestartet. Außerdem wurde eine leere-Raum-Kalibrierung durchgeführt und die Anzeige anschließend beim Sitzen und bei deutlicher Bewegung beobachtet.

Zur Diagnose wurden Server- und UI-Pfade lokal instrumentiert. Das adaptive Modell, die zeitliche Merkmalsextraktion, die per-RX-/globale Klassifikation, die Feldbewegungsabbildung und die Kalibrierungszuführung wurden überprüft.

**Beobachtung**

Der Screenshot vom 18. Juli zeigt eine laufende ESP32-Verbindung, vier farbige Gerätemarker, eine Punktwolke und `PRESENT_STILL` mit `81 %`. Zwei Marker erscheinen fast überlagert. Die Punktwolke folgte einer realen Bewegung nicht zuverlässig und bewegte sich später auch bei still sitzender Person.

Im Code wurden mehrere Fehler bestätigt: ein automatisch geladenes Modell mit nur `41,5 %` Genauigkeit, Selbstvergleich des aktuellen Frames, Übergewichtung statischer CSI-Merkmale, Verwendung der zuletzt eingetroffenen RX-Klasse als globale Klasse und eine zu niedrige Visualisierungsenergie für `present_moving`.

Nach lokalen Korrekturen blieben die Rohbewegungswerte von stiller und bewegter Person stark überlappend. Damit war die Klassifikation weiterhin nicht belastbar.

**Erfolg**

Der feste Aufbau, die vollständige 4RX-Verbindung und die Übertragung der realen Geometrie sind nachgewiesen. Die Diagnose konnte mehrere konkrete Softwarefehler isolieren und verhindern, dass ein schwaches Modell weiter als gültige Auswertung behandelt wird.

**Problem / Fehlschlag**

Die aktuelle CSI-Paketfolge enthält so starke Frame-zu-Frame-Änderungen, dass sie Körperbewegung überlagert. Eine reine Nachjustierung fester Schwellen würde Stillstand und Bewegung nicht zuverlässig trennen. Die genaue Quelle dieser Paketvariation ist noch nicht bestätigt.

**Konsequenz für den nächsten Schritt**

Vor neuen Bewegungsversuchen wird die RX-Firmware auf Absender-MAC-, Pakettyp- und CSI-Raster-Filterung geprüft. Erst nach einem sauberen Datenstrom werden neue gelabelte Referenzmessungen und die leere-Raum-Kalibrierung wiederholt.

**Relevanz für den Bericht**

Der Versuch trennt technische Konnektivität von Messvalidität: Vier empfangende Nodes, reale Geometrie und eine Live-Punktwolke reichen nicht aus, wenn die verwendeten CSI-Frames zeitlich oder nach Paketquelle nicht vergleichbar sind. Der negative Befund ist deshalb ein wichtiger Teil der Methodik- und Grenzendiskussion.

Ausführliche Diagnose und Bildnachweis: [results/2026-07-18_fester-raum_live-visualisierung_diagnose.md](results/2026-07-18_fester-raum_live-visualisierung_diagnose.md)

### 2026-07-26 — D4-Bewegungsmetrik und kontaminierter E0-Versuch

**Ausgangslage**

Nach der Diagnose vom 18. Juli wurden alle vier RX auf den kontrollierten TX mit der MAC-Adresse `TX_MAC_REDACTED` gefiltert. Vor weiteren Personenversuchen sollte zuerst geprüft werden, ob der leere Raum mit dem bereinigten Paketstrom und der D4-Bewegungsmetrik ruhig bleibt.

**Durchführung / Änderung**

Die Bewegung wird nun aus RMS-normalisierten zeitlichen Frame-Unterschieden und einer ebenfalls normalisierten zeitlichen Varianz berechnet. Anschließend wurde ein geplanter 60-Sekunden-Leerraumlauf mit RX1 bis RX4 aufgezeichnet. Nach Abschluss wurde gemeldet, dass der Raum währenddessen zweimal kurz betreten worden war.

**Beobachtung**

Alle vier RX waren in allen 237 Samples vorhanden. Global traten weder `ACTIVE` noch `PRESENT_MOVING` auf. `ABSENT` wurde 129-mal und `PRESENT_STILL` 108-mal ausgegeben. RX1 blieb vollständig `ABSENT`; RX2, RX3 und RX4 meldeten zeitweise `PRESENT_STILL`. RX3 meldete lokal in sieben Samples `PRESENT_MOVING`, erreichte damit aber nicht das globale Bewegungsquorum. Wegen der zwei nicht zeitlich markierten Raumzutritte können diese Anteile nicht als reine Leerraumwerte interpretiert werden.

**Erfolg**

Gegenüber dem vorherigen Lauf verschwanden die globalen groben Bewegungsalarme. Das ist ein positives Indiz für D4, aber wegen der vermischten Raumzustände noch kein gültiger Leerraumnachweis.

**Problem / Fehlschlag**

Der Versuch ist als Leerraum-Baseline ungültig, weil der Raum zweimal kurz betreten wurde. Die 45,6 % `PRESENT_STILL` dürfen deshalb nicht als Leerraum-Fehlerrate verwendet werden. Unabhängig davon bleibt die Aggregationsregel auffällig: Für Bewegung ist ein Quorum nötig, für Still-Präsenz genügt schon ein einzelner RX.

**Konsequenz für den nächsten Schritt**

Als nächstes wird unter unverändertem Aufbau zuerst ein vollständig ununterbrochener 60-Sekunden-Leerraumlauf wiederholt. Danach folgt der kontrollierte Lauf mit still sitzender Person.

**Relevanz für den Bericht**

Der Lauf zeigt, dass die Korrektur einer Signalmetrik grobe Fehlbewegung beseitigen kann, ohne automatisch eine zuverlässige Anwesenheitserkennung zu liefern. Einzel-RX-Klassifikation und globale Aggregation müssen getrennt bewertet werden.

Ausführliche Auswertung: [results/2026-07-26_D4-E0_leerraum.md](results/2026-07-26_D4-E0_leerraum.md)

### 2026-07-26 — Gültige E0b-Wiederholung im leeren Raum

**Ausgangslage**

Der erste E0-Versuch war durch zwei kurze Raumzutritte kontaminiert. Deshalb wurde er nicht als Leerraum-Baseline verwendet und unter unverändertem Aufbau vollständig wiederholt.

**Durchführung / Änderung**

Nach zehn Sekunden Vorlauf blieb der Raum während der gesamten 60-Sekunden-Aufnahme leer. Das Messende wurde für die außerhalb wartende Versuchsperson über Home Assistant durch zweimaliges Blinken der Küchen-Fackel signalisiert.

**Beobachtung**

Alle vier RX waren in allen 237 Samples vorhanden. Trotzdem wurde global 218-mal `PRESENT_STILL` und nur 19-mal `ABSENT` ausgegeben. RX1 blieb vollständig `ABSENT`. RX2 meldete in 12,2 %, RX3 in 39,7 % und RX4 in 84,4 % der Samples Präsenz. Global traten kein `PRESENT_MOVING` und kein `ACTIVE` auf.

**Erfolg**

Die Datenerfassung und das externe Abschlusssignal funktionierten zuverlässig. D4 verhindert weiterhin die zuvor dominierenden groben Bewegungsalarme.

**Problem / Fehlschlag**

E0b ist mit 92,0 % globaler Still-Präsenz im leeren Raum nicht bestanden. RX4 allein erzeugt in 45,6 % aller Samples die einzige Präsenzmeldung. Da für die globale Klasse `PRESENT_STILL` ein einzelner RX genügt, wird lokales Rauschen direkt als globale Anwesenheit ausgegeben.

**Konsequenz für den nächsten Schritt**

Vor einer Änderung von Schwelle oder Quorum wird ein gleich langer Positivlauf mit still sitzender Person aufgenommen. Danach werden die per-RX-Verteilungen von E0b und dem Positivlauf direkt verglichen.

**Relevanz für den Bericht**

Der Lauf trennt erfolgreich reduzierte Bewegungs-Fehlalarme von weiterhin unzuverlässiger Anwesenheitserkennung. Er zeigt außerdem, dass eine globale ODER-Verknüpfung mehrerer Empfänger die False-Positive-Rate stark erhöhen kann.

Ausführliche Auswertung: [results/2026-07-26_D4-E0b_sauberer-leerraum.md](results/2026-07-26_D4-E0b_sauberer-leerraum.md)

### 2026-07-26 — E0c A/B-Test mit mittigem Mac

**Ausgangslage**

E0b zeigte 0,0 % lokale Fehlpräsenz bei RX1, 12,2 % bei RX2, 39,7 % bei RX3 und insgesamt 84,8 % bei RX4. Es wurde vermutet, dass die Nähe des Macs zu RX4 dessen Messwerte beeinflusst.

**Durchführung / Änderung**

Der Mac wurde relativ mittig aufgestellt. Danach wurde unter ansonsten unverändertem Aufbau erneut 60 Sekunden lang der vollständig leere Raum gemessen. Das Messende wurde durch einmaliges kurzes Leuchten der Home-Assistant-Küchen-Fackel signalisiert.

**Beobachtung**

RX4 fiel von 84,8 % Fehlpräsenz auf 0,0 %. Sein Raw-Mittel sank von 0,121 auf 0,027 und der geglättete Mittelwert von 0,062 auf 0,001. Gleichzeitig verbesserte sich sein RSSI von −57,0 auf −52,0 dBm. RX1 blieb bei 0,0 %. RX2 und RX3 veränderten sich mit 13,1 % beziehungsweise 40,5 % Fehlpräsenz praktisch nicht. Global sank die Fehlpräsenz von 92,0 % auf 46,8 %.

**Erfolg**

Der A/B-Test isoliert für RX4 einen starken Einfluss des Mac-Standorts. Durch das Umstellen wurde dessen Fehlpräsenz vollständig beseitigt.

**Problem / Fehlschlag**

Der Test unterscheidet noch nicht zwischen Funkstörung und einer Änderung des Multipfadfeldes durch Metallgehäuse und Kabel. Außerdem bleiben die Fehlmeldungen von RX2 und RX3 bestehen.

**Konsequenz für den nächsten Schritt**

Der Mac bleibt mittig. E0c wird als aktuelle Leerraum-Referenz verwendet. Als nächstes folgt am selben Aufbau ein Positivlauf mit still sitzender Person, bevor Schwellen oder Quorum verändert werden.

**Relevanz für den Bericht**

Der Befund zeigt, dass nicht nur Personen, sondern auch Positionen aktiver Rechner und leitender Gegenstände das CSI-Muster einzelner Links stark verändern können. Eine stabile Versuchsanordnung muss deshalb auch den Auswerterechner und seine Kabel räumlich festlegen.

Ausführliche Auswertung: [results/2026-07-26_E0b-E0c_mac-position-ab-test.md](results/2026-07-26_E0b-E0c_mac-position-ab-test.md)

### 2026-07-26 — E1: still sitzende Person bei mittigem Mac

**Ausgangslage**

E0c wurde nach Beseitigung der RX4-Störung als aktuelle Leerraum-Referenz festgelegt. Vor einer Änderung der Klassifikationslogik musste geprüft werden, ob eine still sitzende Person unter identischem Aufbau eine trennbare Score-Verteilung erzeugt.

**Durchführung / Änderung**

Eine Person saß 60 Sekunden lang möglichst still ungefähr mittig im Raum. Der Mac und alle übrigen Komponenten blieben gegenüber E0c unverändert.

**Beobachtung**

Global stieg der Präsenzanteil von 46,8 % im Leerraum auf 79,7 % bei stiller Person. RX1 blieb vollständig `ABSENT`; RX2 sank sogar von 13,1 % auf 8,0 %. RX3 stieg von 40,5 % auf 72,2 %, RX4 von 0,0 % auf 38,8 %.

Der geglättete RX4-Mittelwert stieg von 0,001 auf 0,036. Seine deskriptive AUC betrug 0,982. Der Effekt blieb in den letzten 20 Sekunden mit einem Mittelwert von 0,039 bestehen und war daher nicht nur eine Folge des Hinsetzens.

**Erfolg**

RX4 trennt den getesteten Leerraum und die still sitzende Person deutlich. Bei einer vorläufigen geglätteten Schwelle von 0,01 lagen 3,0 % der Leerraum- und 83,5 % der Still-Samples darüber.

**Problem / Fehlschlag**

Die Links reagieren sehr unterschiedlich. Eine gemeinsame Schwelle und gleich gewichtete ODER-Fusion sind ungeeignet. Die aus einem Laufpaar abgeleitete 0,01-Schwelle ist außerdem noch nicht unabhängig validiert.

**Konsequenz für den nächsten Schritt**

Die künftige Präsenzlogik sollte per-RX-Leerraumreferenzen und eine Fusion der individuellen Abweichungen verwenden. Vor der endgültigen Übernahme eines Schwellwerts wird ein neues Leerraum-/Still-Paar unter unverändertem Aufbau aufgenommen.

**Relevanz für den Bericht**

Der Versuch zeigt, dass stille Anwesenheit grundsätzlich in einem einzelnen geeigneten CSI-Link sichtbar sein kann, während andere Links am selben Aufbau keine oder widersprüchliche Reaktion zeigen. Sensorgeometrie und link-spezifische Kalibrierung sind deshalb zentral.

Ausführliche Auswertung: [results/2026-07-26_E0c-E1_still-person-separation.md](results/2026-07-26_E0c-E1_still-person-separation.md)

### 2026-07-26 — E0d/E1b widerlegt feste RX4-Schwelle

**Ausgangslage**

Im ersten E0c/E1-Paar trennte RX4 Leerraum und still sitzende Person deutlich. Eine geglättete Schwelle von 0,01 sollte deshalb mit einem unabhängigen Laufpaar bestätigt werden.

**Durchführung / Änderung**

Ohne Hardware-, Mac- oder Softwareänderung wurden erneut 60 Sekunden leerer Raum und anschließend 60 Sekunden still sitzende Person aufgenommen.

**Beobachtung**

RX4 blieb sowohl im Leerraum als auch bei sitzender Person vollständig `ABSENT`; kein Sample überschritt 0,01. RX2 war dagegen bereits im Leerraum in 83,5 % der Samples präsent. RX3 stieg von 22,4 % Präsenz im Leerraum auf 78,1 % bei sitzender Person. Sein geglätteter Mittelwert stieg von 0,027 auf 0,055.

**Erfolg**

Die unabhängige Prüfung verhinderte die Übernahme einer überangepassten RX4-Schwelle. RX3 zeigte als einziger Link in beiden Laufpaaren einen konsistenten Mittelwertanstieg bei stiller Person.

**Problem / Fehlschlag**

Die Empfindlichkeit einzelner Links ist nicht stabil genug für eine fest an einen RX gebundene Sample-Schwelle. Die globale ODER-Fusion wurde im E0d-Leerraum durch RX2 erneut stark falsch ausgelöst.

**Konsequenz für den nächsten Schritt**

Die neue Logik muss per-RX-Leerraumreferenzen, längere Zeitfenster und eine Zuverlässigkeitsbewertung der Links kombinieren. Eine feste RX4-Schwelle wird nicht implementiert.

**Relevanz für den Bericht**

Der Versuch zeigt die Bedeutung unabhängiger Wiederholungen: Eine sehr gute Trennung in einem Laufpaar kann durch kleine Änderungen der Körperposition oder des Multipfadfeldes verschwinden. Reproduzierbarkeit ist daher wichtiger als ein einzelner hoher Kennwert.

Ausführliche Auswertung: [results/2026-07-26_E0d-E1b_unabhaengige-bestaetigung.md](results/2026-07-26_E0d-E1b_unabhaengige-bestaetigung.md)

### 2026-07-26 — D5-Offline-Replay und experimentelle Präsenzkalibrierung

**Ausgangslage**

Die feste RX4-Schwelle war im zweiten Laufpaar widerlegt. Gleichzeitig erzeugte RX2 im E0d-Leerraum eine stabile lokale Fehlpräsenz. Eine neue Regel durfte deshalb weder an einen bestimmten RX noch an eine globale ODER-Verknüpfung gebunden sein.

**Durchführung / Änderung**

Ein reproduzierbarer Offline-Replayer wurde erstellt. D5 lernt pro RX ausschließlich aus sechs vollständigen 10-Sekunden-Leerraumblöcken Median und MAD. Im Live-Pfad werden 10-Sekunden-Mittel relativ zu dieser Referenz bewertet. Mindestens zwei RX müssen zwei Sekunden lang zustimmen. Die Regel wurde zuerst E0c-only → E0d/E1b und danach in umgekehrter Richtung geprüft.

Im lokalen RuView-Server wurde D5 als explizit zu aktivierender Prototyp ergänzt. Die getrennten Endpunkte `/api/v1/classification/calibration/start|stop|status` beeinflussen die vorhandene SVD-FieldModel-Kalibrierung nicht. D4 bleibt für deutliche Bewegung zuständig.

**Funktionsweise von D5**

1. Während einer 60-Sekunden-Leerraumkalibrierung entstehen pro RX sechs getrennte 10-Sekunden-Mittelwerte des `smoothed_motion_score`.
2. Aus diesen Leerraumwerten werden pro RX Median, MAD und die robuste Skala `max(1,4826 × MAD; 0,005)` berechnet. Personendaten werden nicht zum Anpassen der Referenz verwendet.
3. Im Livebetrieb bildet jeder RX einen kausalen Mittelwert über die letzten zehn Sekunden. Seine Abweichung von der eigenen Leerraumreferenz wird als z-Wert berechnet.
4. Ein RX stimmt für Anwesenheit, wenn `z > 1`.
5. `PRESENT_STILL` erfordert mindestens zwei RX-Stimmen, die zwei Sekunden lang bestehen. Unter drei nutzbaren RX wird kein reduziertes Ein-RX-Quorum verwendet.
6. D4 bleibt für deutliche Bewegung (`PRESENT_MOVING` und `ACTIVE`) zuständig. D5 übernimmt nach erfolgreicher Kalibrierung nur die schwierigere Trennung zwischen `ABSENT` und `PRESENT_STILL`.

Für D5 zählen ausschließlich CSI-Frames, die den Subcarrier-Rasterfilter passiert haben und tatsächlich in die D5-Berechnung gelangt sind. Jeder verwendete RX muss mindestens 5 Hz akzeptierte D5-Daten liefern. Eine Unterbrechung von mindestens einer Sekunde verwirft das gesamte Livefenster; danach müssen erneut vollständige zehn Sekunden gesammelt werden. Evidenzverlust, Nodeverlust oder ein Subcarrier-Rasterwechsel dürfen keine zuvor erkannte Still-Präsenz festhalten.

Der Server stellt dafür folgende getrennte Schnittstellen bereit:

```text
POST /api/v1/classification/calibration/start
POST /api/v1/classification/calibration/stop
GET  /api/v1/classification/calibration/status
```

Der Status enthält pro RX die Referenz, das aktuelle 10-Sekunden-Mittel, den z-Wert, die Stimme sowie Frische und Datenrate der tatsächlich akzeptierten D5-Samples. Ohne abgeschlossene D5-Kalibrierung bleibt die bisherige D4-Logik aktiv (`legacy_d4`).

**Beobachtung**

Die primäre Prüfung erreichte 0,0 % Leerraum-Fehlpräsenz, 88,8 % Still-Recall und 94,4 % Balanced Accuracy. Die umgekehrte Prüfung erreichte 0,0 %, 89,8 % und 94,9 %. Ein strengerer `z > 3`-Kandidat fiel auf 15,5 % mittleren Recall. Ein überwachter Selektor erzeugte 20,8 % mittlere Leerraum-Fehlpräsenz.

**Erfolg**

Erstmals trennt eine vorab definierte, nur auf Leerraumdaten kalibrierte Regel beide vorhandenen Laufpaare ohne globale Leerraum-Fehlpräsenz. Der Pfadwechsel zwischen RX3/RX4 und RX2/RX3 wird durch das absolute Zwei-RX-Quorum abgefangen. Replay- und Live-Berechnung verwenden dieselben vollständigen Kalibrierblöcke.

Vor dem Livetest wurden die Runtime-Sicherungen in zwei unabhängigen Code-Audits geprüft. Veraltete RX-Stimmen, Evidenzverlust, Nodeverlust, eine zu niedrige akzeptierte Datenrate und Unterbrechungen des 10-Sekunden-Fensters fallen nun geschlossen auf keine bestätigte Still-Präsenz zurück. 709 Rust-Tests, 7 Python-Tests, der Release-Build und der isolierte API-Lebenszyklustest bestanden. Der finale Audit gab den kontrollierten Livetest frei.

**Problem / Fehlschlag**

Die Datengrundlage umfasst nur zwei Laufpaare aus derselben Sitzung und nur eine Sitzposition. Das Ergebnis ist daher noch kein Produktionsnachweis. Ein einzelner echter Funkpfad könnte vom Zwei-RX-Quorum übersehen werden; zwei gemeinsam driftende RX könnten weiterhin falsche Präsenz erzeugen.

D5 löst außerdem keine räumliche Ortung. Die Heatmap und die sichtbare Punktwolke werden durch diese Änderung nicht automatisch zur gemessenen Personenposition. D5 verbessert ausschließlich die Raumklassifikation `ABSENT` gegenüber `PRESENT_STILL`.

**Konsequenz für den nächsten Schritt**

Die D5-Parameter bleiben eingefroren. Nach dem finalen Server-Build folgt eine neue 60-Sekunden-Leerraumkalibrierung, danach je ein blinder Leerraum- und Still-Lauf sowie mindestens eine weitere Sitzposition. Erst diese neuen Daten entscheiden über eine Standardaktivierung.

Der finale Release wurde mit denselben Ports, Raummaßen und TX-/RX-Positionen neu gestartet. Alle vier RX lieferten anschließend wieder aktuelle Daten. D5 ist noch nicht real kalibriert; der Server arbeitet bis zum ersten erfolgreichen Kalibrierlauf weiterhin im Zustand `legacy_d4`.

**Relevanz für den Bericht**

D5 zeigt methodisch, wie aus negativen Wiederholungen eine überprüfbare Regel entsteht: nicht den besten Einzelsensor auswählen, sondern link-spezifische Referenzen, robuste Statistik, Zeitfenster und ein vorab festgelegtes Quorum verwenden. Gleichzeitig bleibt die kleine Stichprobe ausdrücklich als Grenze dokumentiert.

Ausführliche Auswertung: [results/2026-07-26_D5_offline-replay-und-experimentelle-praesenzkalibrierung.md](results/2026-07-26_D5_offline-replay-und-experimentelle-praesenzkalibrierung.md)

### 2026-07-26 — Reale D5-Kalibrierung und fehlgeschlagener Still-Livetest

**Ausgangslage**

Der D5-Offline-Replay hatte die vorhandenen historischen Laufpaare mit 0,0 % Leerraum-Fehlpräsenz und 89,3 % mittlerem Still-Recall getrennt. Mit eingefrorenen Parametern sollte deshalb die erste neue reale Aufnahme nach einer echten Leerraumkalibrierung folgen.

**Durchführung / Änderung**

Die reale D5-Kalibrierung wurde erfolgreich aktiv. Alle vier RX besaßen danach eine gültige Referenz und aktuelle Evidenz. Anschließend saß eine Person zunächst 59,7 Sekunden und danach weitere 29,9 Sekunden still. Beide Aufnahmen enthielten durchgehend RX1 bis RX4 und keine Logger-Fehler.

**Beobachtung**

Global wurden alle 350 Samples als `ABSENT` ausgegeben. Im ersten Lauf stimmte RX4 in 87 von 236 Samples für Präsenz; alle anderen RX blieben ohne Stimme. In der Fortsetzung stimmte RX3 in allen 114 Samples, RX2 aber nur einmal und RX1/RX4 gar nicht. Das benötigte Zwei-RX-Quorum kam deshalb nie zustande.

**Erfolg**

Die reale Kalibrierung, die per-RX-Diagnose und die Sicherheitsregel arbeiteten technisch nachvollziehbar. Eine einzelne RX-Stimme wurde nicht fälschlich als globale Präsenz übernommen.

**Problem / Fehlschlag**

Der Positivtest wurde mit 0,0 % Still-Recall vollständig verfehlt. Das positive Offline-Replay generalisierte nicht auf die neue Aufnahme. Der informative Link wechselte zudem von RX4 im ersten Abschnitt zu RX3 in der Fortsetzung.

**Konsequenz für den nächsten Schritt**

D5 wird nicht als Standard aktiviert. Das Quorum wird nicht isoliert anhand dieses Fehlschlags gelockert, weil die früheren Leerraumläufe die Gefahr hoher False Positives belegen. Benötigt wird eine zusammengehörige blinde Serie aus Leerraum und mehreren Still-Positionen unter derselben Kalibrierung.

**Relevanz für den Bericht**

Der Test zeigt, dass robuste Fusion zwei Fehler gleichzeitig vermeiden muss: einzelne driftende RX dürfen keinen Leerraumalarm erzeugen, wechselnde einzeln informative Links dürfen aber auch nicht zu vollständigen False Negatives führen. Offline-Erfolg auf wenigen Laufpaaren ersetzt keinen neuen Livetest.

Ausführliche Auswertung: [results/2026-07-26_D5_realer-still-livetest.md](results/2026-07-26_D5_realer-still-livetest.md)

## Vorlage für neue Journaleinträge

### 2026-07-29 — D6-/Positionspipeline und kontextfeste Dokumentation

**Ausgangslage**

Die bisherige Heatmap war kein belastbarer Positionsnachweis. Gleichzeitig
generalisierte D5 im realen Still-Livetest nicht und erreichte 0,0 %
Still-Recall.

**Durchführung / Änderung**

Die laufende Arbeit trennt jetzt Präsenz und Position. Für die Ortung wird ein
diskretes Fingerprint-Modell mit neun festen Bodenpunkten vorbereitet. Raw-CSI,
Sidecars, Setupbindung, Hash-Provenienz, Blindvorhersage und getrennte
Auswertung wurden als Offlinepipeline umgesetzt. Ein eigener
Arbeitsstand-/Wiedereinstiegsnachweis hält Implementiertes, Geprüftes und
Offenes getrennt fest.

**Beobachtung**

Die ESP32 sind aktuell ausgeschaltet und der Mac ist nicht im CSI-WLAN. Das
behindert die derzeitigen Offlineprüfungen nicht. Eine reale Aussage zur
Positionsgenauigkeit ist noch nicht möglich, weil die P01-bis-P09-Aufnahmen
noch fehlen.

**Erfolg**

Die neue Pipeline kann bei unzureichender Evidenz `unknown` oder `ambiguous`
liefern und muss keine scheinpräzise Position erzeugen. Trainings- und
Blinddaten sind durch getrennte Artefakte und Hashprüfungen voneinander
abgegrenzt.

**Automatisierte Prüfung**

Ein echter dateibasierter Test erzeugte eine 65-Sekunden-Leerraumaufnahme,
neun 35-Sekunden-Trainingsaufnahmen und neun davon getrennte
35-Sekunden-Blindaufnahmen mit vier RX, 5 Hz und 64 CSI-Bins. Der vollständige
Weg `inspect → build-index → predict → evaluate` ordnete alle neun
synthetischen Blindpositionen korrekt zu. Coverage und Accuracy betrugen 1,0,
Median- und p95-Fehler 0,0 m.

Danach bestand der vollständige Rust-Testlauf mit 852 bestandenen, 0
fehlgeschlagenen und 2 absichtlich ignorierten Tests. Auch
Sensing-UI-Lokalisierungstest, JavaScript-Syntaxprüfungen, Debug-Build,
tatsächliche CLI-Hilfe und Fehlerfälle, gezieltes Rustfmt der bearbeiteten
Module sowie `git diff --check` bestanden.

**Problem / Fehlschlag**

Die synthetischen Daten beweisen nur die technische Softwarekette. Der
Livepfad ist zwar implementiert, wurde aber noch nicht mit einem real
gemessenen und blind validierten Index betrieben. Die realen Messreihen stehen
noch aus. Der Stand ist deshalb noch kein validiertes Ortungssystem.

**Konsequenz für den nächsten Schritt**

Der Offlineweg ist bestanden. Als Nächstes wird der endgültige Aufbau
einschließlich Mac und Kabeln kanonisch eingefroren. Erst dann folgen
Leerraum-, Trainings- und Blindaufnahmen.

**Relevanz für den Bericht**

Die Arbeit dokumentiert den methodischen Unterschied zwischen einer optisch
plausiblen Heatmap und einer anhand unabhängiger Blinddaten überprüften
Positionsklassifikation.

Ausführlicher, fortlaufender Stand:
[`08-aktueller-arbeitsstand-d6-und-position.md`](08-aktueller-arbeitsstand-d6-und-position.md)

### 2026-07-29 — Abschlussaudit vor dem Hardwareübergang

**Ausgangslage**

Die Softwarekette war offline integriert, die reale Messserie aber noch nicht
gestartet. Vor dem Aufbau-Freeze sollte geprüft werden, ob andere RuView-
Ansichten oder alte Fallbacks trotzdem scheinbar reale Personen,
Klassifikationen oder Positionen ausgeben können.

**Durchführung / Änderung**

Der Serververtrag wurde an vier Stellen verschärft: Classification ist bei
aktivem Positions-Setup bis zur D6-Readiness fail-closed; Trainingsmanifest und
Index verlangen exakt P01 bis P09; ESP32-Personen entstehen nur noch als
neutraler Marker aus bestätigter Präsenz plus gültigem diskretem
`position_estimate`; die TX-Filteridentität ist als Hash über exakt sechs
binäre NVS-Bytes definiert. `GET /health/ready` zeigt die aktive Setup-
Identität jetzt auch ohne Positionsindex.

Observatory unterscheidet nun sichtbar zwischen `CONNECTING`, `LIVE ESP32`,
`SIMULATED` und `STALE`. Nur ein frischer expliziter ESP32-Frame erhält den
Live-Status. Der Hardwaremodus verwendet die validierte Raum-/TX-/RX-Geometrie
und einen neutralen Positionsmarker; die animierte Demofigur bleibt
ausschließlich Simulation.

Zusätzlich wurde ein kontrollierter Aufnahme-Runner angelegt. Er startet keine
Messung ohne aktive Setupbindung, frische ESP32-Quelle und exakt RX1 bis RX4.
Nach dem Stopp prüft er Mindestdatenrate, Drops, Vollständigkeit und die
Setupbindung des Sidecars.

**Beobachtung**

Die ESP32 blieben ausgeschaltet und der Mac blieb außerhalb des CSI-WLANs.
Lokale Provisioning-Zustände liefern nur Kandidaten; eine ältere doppelte
RX3-Datei ohne TX-Filter zeigt, warum die Boards später live geprüft werden
müssen.

**Erfolg**

Gezielte Tests bestätigten die neuen Classification-, Punkt-ID-, ESP32-
Personen-, TX-Filter- und Recorderverträge. Der Capture-Runner bestand seine
eigenen Fail-closed-Tests. Observatory-Quellen-, Geometrie-, Positions-,
Frische- und HUD-Zustände bestanden ihre separaten JavaScript-
Regressionstests.

**Problem / Fehlschlag**

Die tatsächlichen RX-/TX-Firmwarestände, das Live-Raster und die endgültige
Mac-/Kabel-/Raumrevision können ohne eingeschaltete Hardware noch nicht
belegt werden. Eine reale Positionsgüte ist weiterhin vollständig offen.

**Konsequenz für den nächsten Schritt**

Zuerst wird der Mac an seinen endgültigen Betriebsort gestellt. Danach werden
TX und RX1 bis RX4 eingeschaltet, die Live-Konfiguration geprüft und ein
25-Sekunden-Preflight ohne Positionslabel durchgeführt. Erst nach bestandenem
Preflight wird das Setup versiegelt und die 65-Sekunden-Leerraumaufnahme
gestartet.

**Relevanz für den Bericht**

Der Audit verhindert, dass eine funktionierende WebSocket-Verbindung, eine
alte Groblokalisierung oder ein lokaler Provisioning-Zustand als reale
Messleistung beziehungsweise Gerätewahrheit interpretiert wird.

Ausführlicher Stand:
[`08-aktueller-arbeitsstand-d6-und-position.md`](08-aktueller-arbeitsstand-d6-und-position.md)

### 2026-07-29 — Offline-Vorbereitung der Liveintegration

**Ausgangslage**

Die ESP32 sind ausgeschaltet und der Mac ist nicht mit dem CSI-WLAN verbunden.
Die reale Messserie kann deshalb noch nicht beginnen; die Software kann aber
reproduzierbar für diesen Übergang vorbereitet werden.

**Durchführung / Änderung**

Drei getrennte Arbeitspakete wurden umgesetzt: eine kanonische und gehashte
Setupbindung, ein Live-Positionskern auf Basis derselben Fenster und Merkmale
wie die Offlinepipeline sowie eine fail-closed Sensing-Darstellung. Der
fortlaufende Arbeitsstand beschreibt für jedes Paket die erlaubten Ausgaben
und die noch ausstehenden realen Nachweise.

**Beobachtung**

Dieser Schritt benötigt keine Livepakete. Die ausgeschaltete Hardware darf
deshalb weder als Fehler noch als erfolgreicher Livetest interpretiert werden.

**Erfolg**

Die drei Teile sind jeweils separat implementiert und geprüft. Die Setupbindung
bestand neben Unit-Tests und Build auch echte CLI-, HTTP- und
UDP-Smoke-Tests; ein absichtlicher 63-statt-64-Bin-Frame schrieb null Frames
und erzeugte einen unvollständigen Sidecar. Der Livekern bestand nach den
Review-Korrekturen `11/11` eigene Tests sowie alle Capture-/Paritäts- und
Positions-Tests. Der UI-Test bestätigte, dass Fehlerzustände, alte Grobdaten
und ein Verbindungsabbruch die Persondarstellung löschen. Der Wiedereinstieg ist
damit auch nach Abschluss der Integration eindeutig.

**Problem / Fehlschlag**

Die drei Arbeitspakete wurden inzwischen vollständig im Server zusammengeführt
und erneut gesamtgetestet. Es existiert weiterhin kein real gemessener
Positionsindex; deshalb bleibt die neue Livefunktion ohne echte Koordinaten.

Ein unabhängiger Cross-Review fand vor der Integration außerdem zwei
fail-closed Lücken: Zustandswechsel löschten den Raw-Frame-Puffer noch nicht,
und `_simulated: true` konnte im Browser eine zugleich als ESP32 bezeichnete
Quelle als Demo behandeln. Die Browserprüfung akzeptierte zudem noch zu
tolerante Koordinaten.

**Korrektur des Cross-Reviews**

Der Livekern behandelt jetzt jeden fail-closed Übergang als neue Datenepoche
und sammelt danach drei Sekunden ausschließlich neue Frames. `9/9` Live- und
`105/105` Positions-Tests bestanden. Die UI verwendet nur noch eine explizite
Simulations-Source-Allowlist, prüft P01 bis P09 und exakt drei numerische
Koordinaten innerhalb des Raums. Ihr Regressionstest deckt zusätzlich
Disconnect und parallele Init-/Dispose-Abläufe ab.

Während der Integration wurde außerdem verhindert, dass weiterlaufende
Edge-Vitals eine alte Position festhalten, obwohl kein Raw-CSI mehr ankommt.
Nach einer Sekunde ohne akzeptiertes Raw-CSI wird jetzt `stale` ohne
Koordinaten ausgegeben; Edge-Vitals erzeugt selbst keine Position.

**Gesamtprüfung**

Die Serverintegration ist abgeschlossen. Der vollständige Rust-Testlauf
bestand mit 852 bestandenen, 0 fehlgeschlagenen und 2 absichtlich ignorierten
Tests. Server-Build, reale CLI-Hilfe und Fehlerfälle, UI-Regressionstest,
JavaScript-Syntax, `git diff --check` sowie gezieltes Rustfmt der bearbeiteten
Module bestanden ebenfalls. Ein workspaceweiter Formatcheck bleibt wegen
bereits vorhandener Abweichungen in nicht betroffenen Workspace-/Vendor-Dateien
kein sinnvoller Abschlussnachweis.

**Konsequenz für den nächsten Schritt**

Für den Hardwareübergang fehlen nun die normale Mac-Betriebsposition, die
bestätigten Firmware-/Funk-/Rasterangaben und das daraus versiegelte
Setup-Artefakt. Der normale Mac-, Kabel- und Möbelaufbau wird in der
Leerraumreferenz mitgemessen; leer bedeutet nur ohne Person. Erst danach
folgen reale Leerraum-, Trainings- und Blindaufnahmen.

**Relevanz für den Bericht**

Die Trennung verhindert, dass ein synthetischer Softwaretest oder eine
funktionierende UI versehentlich als reale Ortungsleistung dargestellt wird.

### 2026-08-01 — Offline-Abschluss Software und Vorbereitung

**Ausgangslage**

Vor neuen Leerraum- und Positionsaufnahmen musste die Messkette sicherstellen,
dass ausschließlich Raw-CSI des kontrollierten TX und des versiegelten
1TX-/4RX-Aufbaus in Auswertung und Aufzeichnung gelangt. Gleichzeitig durfte
die Vorbereitung keine reale Erkennungsleistung behaupten, solange die ESP32
ausgeschaltet sind.

**Durchführung / Änderung**

Der RX-Firmwarevertrag wurde um einen strikt definierten 40-Byte-
Laufzeitnachweis ergänzt. Gemeinsamer Parser, Server, Recorder,
Offlinewerkzeuge und Capture-Runner prüfen Filterstatus, tatsächlich passende
Framequelle, Identitätsgültigkeit und Übereinstimmung mit dem versiegelten
Setup. Ein fehlender, beschädigter, widersprechender oder veralteter Nachweis
wird fail-closed vor Liveness, Classification, D4/D5/D6, Position und Recorder
abgewiesen. Öffentliche Statusausgaben enthalten nur Zustandsflags und Alter,
nicht die rohe TX-MAC oder ihren Hash.

Provisionierung und Zustandsdateien wurden zusätzlich gegen die Ausgabe von
MAC- und OTA-Geheimnissen abgesichert. Historische Raw-v1-Aufnahmen bleiben
lesbar; neue setupgebundene Positionsartefakte verlangen den vollständigen
Nachweis.

**Automatisierte Prüfung**

Bestätigt wurden die gezielten Firmware-Host-, Provisionierungs-, Parser-,
Server-, Positions-, Runner-, Sanitizer- und UI-Regressionsprüfungen. Der
synthetische dateibasierte Positionsweg ordnete weiterhin `9/9` Blindpunkte
korrekt zu. Die vollständige aktuelle Rust-Matrix bestand mit `1.118`
bestandenen, `0` fehlgeschlagenen und `3` bewusst ignorierten Tests: Server
`885/2`, Hardware `177/1`, CLI `33/0` und Pointcloud `23/0`, jeweils in der
Notation bestanden/ignoriert. Zusätzlich bestanden Provisionierung `27/27`,
Capture-Runner `8/8`, ADR-Hosttests `21/21`, Vitals-Hosttests `22/22`, die drei
UI-Vertragstests sowie der Source-Binding-Vertrag. Die `8/8` vorhandenen
mmWave-Prädikatstests sind grün, bedeuten aber keine Aktivierung oder Integration
des weiterhin zurückgestellten mmWave-Moduls. Die Summen vom 2026-07-29 bleiben
historische Momentaufnahmen.

Ein anschließender unabhängiger Audit schloss vier Vorbereitungslücken: Im
versiegelten Modus werden Edge-Vitals vor jeder Zustandsänderung ignoriert,
Null-Frame-Aufnahmen enden als unvollständig, Discovery verlangt intern eine
frische identische 0x07-TX-Bindung aller vier RX und meldet dennoch ausdrücklich
nur Inventur statt Mess-PASS, und ein NVS-Rewrite bricht bei unbekanntem
OTA-Schlüsselzustand vor dem Flashen ab. Die physische Zuordnung der beschrifteten
RX zu ihren Koordinaten bleibt ein manueller Aufbaucheck; eine selbst gemeldete
RX-ID kann eine räumliche Vertauschung nicht erkennen.

**Nachtrag: echter Target-Build**

ESP-IDF v5.4 wurde lokal und vom Projekt getrennt unter `.toolchains/`
installiert. Die aktuelle Firmware 0.7.0 kompilierte für ESP32-S3 mit 8-MB-
Layout (`1.129.872` Byte, `46 %` App-Partitionsreserve), ESP32-S3 mit 4-MB-
Layout (`913.920` Byte, `52 %` Reserve) und den CI-Forschungstarget ESP32-C6
(`1.054.736` Byte, `45 %` Reserve). Die beiden für den realen Aufbau relevanten
S3-Varianten wurden unter
`artifacts/ruview-firmware-0.7.0-2026-08-01/` mit Flashargumenten und
SHA-256-Prüfsummen gesichert. Die historischen `release_bins` 0.6.7 dürfen für
die neuen Messungen nicht verwendet werden, weil ihnen der TX-Binding-Nachweis
fehlt.

Der 8-MB-S3-Build lag `3.472` Byte über dem bisherigen CI-Grenzwert von
`1.100 KiB`, passte aber mit fast der Hälfte Reserve in die vorgesehene 2-MiB-
App-Partition. Der CI-Schutzwert wurde deshalb auf `1.120 KiB` aktualisiert;
die Firmware-Dokumentation nennt nun konsistent ESP-IDF v5.4, den aktuellen
Größenstand und die 2-MiB-Partition.

**Grenze / noch kein Nachweis**

Target-Kompilierung ist damit nachgewiesen. Flash-Größenerkennung, Flashen und
Boottest benötigen ein tatsächlich angeschlossenes, beschriftetes Board und
bleiben das erste physische Gate. Ebenfalls offen sind reale Classification-
Güte, P01-bis-P09-Fingerprints und die unabhängige Blindvalidierung.

**Konsequenz für den nächsten Schritt**

Der Mac wird an seiner normalen Betriebsposition betrieben. Mac, Kabel, Möbel,
Türstellung und andere statische Raumteile bleiben während Kalibrierung,
Training und Blindtest unverändert und werden als Leerraumhintergrund
mitgemessen; „leer“ bedeutet ausschließlich „ohne Person“. Nach Flash-
Größenerkennung, Flash, Boot- und Live-Konfigurationsprüfung folgen Discovery, versiegelter Preflight
und erst danach die D6-Leerraumaufnahme.

Ausführlicher Stand:
[`08-aktueller-arbeitsstand-d6-und-position.md`](08-aktueller-arbeitsstand-d6-und-position.md)

### 2026-08-01 — RX1 Flash- und Bootprüfung

**Getestet**

Das beschriftete RX1 wurde allein per USB verbunden. `esptool 5.3.0` erkannte
einen ESP32-S3 Revision 0.2 mit 16 MB physischem Flash und 8 MB PSRAM. Vor dem
Flashen wurde keine Löschung ausgeführt.

**Durchführung / Ergebnis**

RX1 erhielt die verifizierte Firmware 0.7.0 mit dem geprüften 8-MB-Layout; die
obere Hälfte des 16-MB-Flashs bleibt bewusst ungenutzt. Bootloader,
Partitionstabelle, OTA-Auswahl und App wurden geschrieben und jeweils per Hash
verifiziert. Der NVS-Bereich ab `0x9000` wurde nicht überschrieben.

Der Bootlog bestätigte Node-ID 1, Kanal 6, Edge-Tier 0, aktiven TX-MAC-Filter,
Zielserver `CSI_HOST_IP:5005` und den korrekten Headless-Pfad des Boards. Das
Image meldet Projekt `esp32-csi-node`, Version 0.7.0 und ESP-IDF v5.4. WLAN und
CSI konnten noch nicht geprüft werden, weil nur RX1 eingeschaltet und der
CSI-AP beziehungsweise TX nicht aktiv war.

**Offener Sicherheitszustand**

Der OTA-HTTP-Endpunkt läuft fail-closed, weil auf RX1 noch kein Security-
Namespace mit OTA-Schlüssel eingerichtet ist. Das betrifft nicht den separaten
lokalen Konfigurationsendpunkt für Ziel-IP-Änderungen, muss aber vor einem
späteren OTA-Firmwareupdate bewusst provisioniert werden. Es wurde kein
Schlüsselwert protokolliert.

**Nächster Schritt**

RX2, RX3, RX4 und TX werden einzeln zunächst mit `flash-id` inventarisiert und
erst danach mit der zu ihrer Flash-Größe passenden S3-Variante geflasht. Nach
allen Bootprüfungen folgen gemeinsames Einschalten, CSI-WLAN, Provisionierung,
Discovery und versiegelter Preflight.

### 2026-08-01 — RX2 Flash- und Bootprüfung

RX2 wurde als ESP32-S3 Revision 0.2 mit 16 MB Flash und 8 MB PSRAM erkannt.
Wie RX1 erhielt es ohne NVS-Löschung die verifizierte Firmware 0.7.0 mit dem
8-MB-Layout; alle geschriebenen Bereiche bestanden die Hashprüfung. Der
Bootlog bestätigte Node-ID 2, Kanal 6, Edge-Tier 0, aktiven TX-MAC-Filter,
Headless-Betrieb und Zielserver `CSI_HOST_IP:5005`. WLAN/CSI blieb bei
ausgeschaltetem TX beziehungsweise CSI-AP erwartbar inaktiv. Auch RX2 betreibt
OTA ohne Security-Namespace fail-closed; es wurde kein Schlüsselwert
protokolliert.

Nächster Board-Schritt: RX3 allein per USB anschließen, `flash-id`, passende
Variante flashen und Bootkonfiguration prüfen.

### 2026-08-01 — RX3 Flash- und Bootprüfung

RX3 wurde als ESP32-S3 Revision 0.2 mit 16 MB Flash und 8 MB PSRAM erkannt.
Das verifizierte 8-MB-Image der Firmware 0.7.0 wurde ohne NVS-Löschung
geschrieben; alle Bereiche bestanden die Hashprüfung. Der Bootlog bestätigte
Node-ID 3, Kanal 6, Edge-Tier 0, aktiven TX-MAC-Filter, Headless-Betrieb und
Zielserver `CSI_HOST_IP:5005`. Der WLAN-Ausfall war bei inaktivem TX/CSI-AP
erwartbar. OTA bleibt ohne Security-Namespace fail-closed.

Nächster Board-Schritt: RX4 allein per USB anschließen, inventarisieren,
flashen und bootprüfen.

### 2026-08-01 — RX4 Flash- und Bootprüfung

RX4 wurde als ESP32-S3 Revision 0.2 mit 16 MB Flash und 8 MB PSRAM erkannt.
Das verifizierte 8-MB-Image der Firmware 0.7.0 wurde ohne NVS-Löschung
geschrieben und vollständig per Hash geprüft. Der Bootlog bestätigte Node-ID 4,
Kanal 6, Edge-Tier 0, aktiven TX-MAC-Filter, Headless-Betrieb und Zielserver
`CSI_HOST_IP:5005`. Der WLAN-Ausfall war ohne aktiven TX/CSI-AP erwartbar.
OTA bleibt ohne Security-Namespace fail-closed.

Damit haben RX1 bis RX4 den Einzelboard-Flash- und Boottest bestanden. Vor
einem Flash des TX wird getrennt geprüft, ob er dieselbe CSI-Node-Firmware oder
eine eigene Sender-/AP-Firmware benötigt; eine RX-Firmware wird nicht ohne
diesen Nachweis auf den TX geschrieben.

### 2026-08-01 — TX-Firmwaregrenze auditiert

Der TX verwendet weiterhin die separate Arduino-SoftAP-Firmware und darf nicht
mit `esp32-csi-node` 0.7.0 überschrieben werden. Der erhaltene lokale Build ist
für ESP32-S3, 4-MB-Layout, DIO/80 MHz erstellt und sendet auf Kanal 6 ungefähr
50 kleine UDP-Broadcasts pro Sekunde. Die neue TX-Quellbindung verlangt keine
Senderänderung: Jeder RX filtert gegen die AP-Identität und ergänzt den
Laufzeitnachweis selbst.

Der TX wird deshalb zunächst ausschließlich inventarisiert und gebootet.
Geprüft werden stabiler Start ohne Brownout-Schleife, erfolgreicher SoftAP,
Kanal 6, DHCP/Gateway und Broadcast-Rate. Rohe AP-MAC und WLAN-Zugangsdaten
werden nicht protokolliert. Falls ein Reflash überhaupt nötig wird, wird davor
der vollständige aktuelle TX-Flash privat gesichert und ausschließlich der
erhaltene Senderbuild verwendet.

### 2026-08-01 — TX zerstörungsfrei inventarisiert und gebootet

Der TX wurde ohne BOOT-Taste und ohne Schreibzugriff per USB geprüft. `flash_id`
bestätigte einen ESP32-S3 Revision 0.2 mit 16 MB Flash und 8 MB PSRAM. Danach
wurde der normale Start seriell beobachtet. Die vorhandene Senderfirmware
bootete erfolgreich, startete den SoftAP auf `CSI_AP_IP` und blieb in einer
zusätzlichen Beobachtungsphase ohne Brownout- oder Reset-Schleife stabil.

Es wurde nichts geflasht oder konfiguriert. Die rohe AP-MAC wurde nur lokal
gesehen und weder hier noch in anderen Projektunterlagen gespeichert. Kanal,
DHCP/Gateway, Broadcast-Rate und die nicht-identifizierende Bindungskonsistenz
werden als Nächstes mit eingeschaltetem Gesamtaufbau geprüft.

### 2026-08-01 — TX-Netzpfad und feste Serveradresse geprüft

Der Mac verband sich bei weiterhin ausgeschalteten RX1 bis RX4 allein mit dem
TX-SoftAP. DHCP vergab `RX1_IP`; das Gateway `CSI_AP_IP` antwortete in
beiden Pingserien ohne Paketverlust. Ein unprivilegierter UDP-Empfänger zählte
im stabileren 10-Sekunden-Lauf 457 Pakete beziehungsweise `45,5 Hz`; alle
Pakete waren exakt 32 Byte groß. Die Ankünfte waren gebündelt, lagen insgesamt
aber nahe am 50-Hz-Soll und deutlich über dem späteren 5-Hz-Mindestgate je RX.

Da RX1 bis RX4 weiterhin fest an `CSI_HOST_IP:5005` senden, wurde ausschließlich
das CSI-WLAN-Interface des Macs von der DHCP-Adresse auf `CSI_HOST_IP/24` mit
Gateway `CSI_AP_IP` gesetzt und erfolgreich erneut geprüft. Hotspot und Kabel
blieben unberührt. Der Kanal ließ sich über die macOS-API nicht auslesen; Kanal
6 ist im Senderbuild festgelegt und wird im gemeinsamen RX-Lauf nochmals als
Laufzeiteigenschaft geprüft.

### 2026-08-01 — Letzte Softwarelücken vor dem realen Setup geschlossen

Ein unabhängiger Audit zeigte, dass die guarded Leerraumaufnahme D5/D6 noch
nicht selbst startete und beendete, Classification keine getrennte blinde
Truth-Auswertung besaß und der Positionsreport noch nicht jedes eingefrorene
PASS-Gate maschinenlesbar erzwang. Deshalb wurde noch keine reale P01-Aufnahme
begonnen.

`capture_position_run.py --kind empty` orchestriert nun Kalibrierungsstart,
verlustfreie 65-Sekunden-Aufnahme, sicheren Abschluss und die Prüfung gültiger
D5-/D6-Referenzen sowie frischer operationaler Evidenz für exakt RX1 bis RX4.
Classification erzeugt zuerst ein ungelabeltes Replay-Artefakt; ein separater,
an exakte Report-, Raw-, Sidecar-, Signal- und Setup-Hashes gebundener
Truth-Evaluator erzwingt danach 3 Leerraum- und 18 belegte Blindläufe. Eine
private `0600`-Truth-Vorlage wird aus dem Vorhersageartefakt erzeugt. Der
Positionsreport erzwingt nun alle Coverage-, Accuracy-, Wiederholungs-,
Abstentions- und Fehlergrenzen. Ein abschließender Gesamtbericht gibt nur PASS,
wenn Classification und Position für dasselbe versiegelte Setup bestehen.

Der vollständige Server-Binärtest bestand `394/394`, Runner und
Truth-Generator `18/18`; die öffentlichen Setup-/Trainingsvorlagen bestanden
die echten Rust-Schematests. Es wurde kein Commit, Branch, Push oder PR erzeugt.
Die vier aktuellen privaten RX-Provisionierungszustände und die alte, weiterhin
nicht verwendete Zustandsdatei wurden ohne Inhaltsänderung von `0644` auf
`0600` beschränkt.

### 2026-08-01 — Finalen Release-Server eingefroren

Nach Abschluss aller Softwareänderungen wurde der Server mit
`cargo +stable build --release -p wifi-densepose-sensing-server --bin sensing-server`
neu gebaut. Die exakt geprüfte Binärdatei wurde ohne Überschreiben unter
`artifacts/live-position-2026-08-01/sensing-server` archiviert, auf Modus
`0500` gesetzt und bytegleich mit dem Build-Ausgang verglichen.

Die Datei ist `5.954.240` Byte groß. Ihr SHA-256 lautet
`e5cb6302404aa35872071f1ac20e73c26db60281ce826fe9bf365b2b3d5c3823`;
die lokale `SHA256SUMS.txt`-Prüfung bestand. Ein CLI-Smoke-Test am archivierten
Artifact bestätigte Classification-Auswertung, Position-Auswertung,
Setup-Erzeugung und kombinierten Gesamtverdict. Ab jetzt wird für die reale
Serie kein Servercode mehr geändert. Ein notwendiger Fix würde bewusst ein
neues Artifact, Setup und eine neue Serie auslösen.

### 2026-08-09 — Gemeinsamer Liveempfang und Gridfehler gefunden

Der Mac stand an seiner normalen Betriebsposition, TX und RX1 bis RX4 waren
eingeschaltet und das CSI-WLAN verbunden. Da DHCP zunächst `RX_DHCP_IP`
vergeben hatte, wurde das WLAN-Interface erneut auf die von allen RX erwartete
Empfangsadresse `CSI_HOST_IP/24` mit Gateway `CSI_AP_IP` gesetzt. Der TX war
ohne Paketverlust erreichbar; alle vier RX erschienen anschließend frisch im
Server.

Die öffentliche Binding-Anzeige wechselte jedoch zwischen bestätigt und nicht
bestätigt. Eine getrennte 10-Sekunden-UDP-Inventur zeigte für RX1 bis RX4
ausschließlich vollständige, gültig gebundene CSI-Pakete und keine
Legacy-Pakete. Jeder RX lieferte überwiegend 64-Subcarrier-Frames sowie einen
kleineren Anteil gültiger 128-Subcarrier-Frames. Der Fehler lag im Server: Ein
vom aktiven Raster abweichendes, aber korrekt gebundenes Paket löschte die
frische TX-Bestätigung, vergiftete eine laufende Aufnahme und setzte den
Live-Positionstracker zurück. Deshalb wurde die geplante Discovery nicht mit
dem Build vom 2026-08-01 gestartet.

### 2026-08-09 — Binding und Raster getrennt, neuer Release eingefroren

Der Server validiert nun zuerst die vollständige TX-/RX-Identität und behandelt
anschließend die Rasterauswahl getrennt. Gültige Off-Grid-Frames halten den
Binding-Nachweis frisch, werden pro RX gezählt und vor D5/D6, Recorder und
Live-Position herausgefiltert. Fehlende, unvollständige, fehlerhafte oder zum
versiegelten Setup unpassende Bindings bleiben harte Fehler. Beim versiegelten
Setup werden Quellidentität und erwartetes Raster ebenfalls getrennt geprüft.

Die vollständige Server-Binärtestsuite bestand `397/397`; zusätzlich bestanden
die fokussierten Grid-Tests `7/7`, die Setup-Tests `14/14`, Format- und
Diffprüfung. Der neue Release liegt mit Modus `0500` unter
`artifacts/live-position-2026-08-09/sensing-server`, ist `5.954.240` Byte groß
und besitzt SHA-256
`91feb860f89f094ba16ea9d749e3a1e5378de1a25ceedd08cebeb67f2cd3484b`.
Der Build vom 2026-08-01 ist damit für reale Messungen abgelöst. Nächstes Gate
ist die unversiegelte 25-Sekunden-Discovery mit dem neuen Build.

### 2026-08-09 — Korrigierte 25-Sekunden-Discovery bestanden

Der neue Release wurde unversiegelt mit der dokumentierten Raum-, TX- und
RX-Geometrie gestartet. Über sechs aufeinanderfolgende API-Abfragen blieb die
gemeinsame TX-Bindung von RX1 bis RX4 durchgehend bestätigt, während die neuen
Off-Grid-Zähler erwartungsgemäß anstiegen. Damit war live nachgewiesen, dass
die gültigen 128-Subcarrier-Ausreißer den Binding-Status nicht mehr löschen.

Die anschließende Discovery `discovery-neutral-20260809-01` lief vollständig
über 25 Sekunden. Sie endete mit `2.612` Frames, `0` Drops, Status `completed`,
`incomplete=false` und ohne Integritätsfehler. Alle vier RX wurden mit demselben
stabilen Raster inventarisiert: `2437 MHz`, eine Antenne, `64` Subcarrier,
PPDU-Typ `0`, Layout-Flags `0`. Die per-RX-Framezahlen waren RX1 `623`, RX2
`626`, RX3 `645` und RX4 `718`; alle Dauer- und Mindest-Ratengates bestanden.

Diese Discovery ist ein Transport-, Binding- und Raster-Nachweis, noch kein
Mess-PASS für Classification oder Position. Vor der Leerraumkalibrierung wird
nun das vollständige reale Setup versiegelt. Noch benötigt werden die genaue
normale Mac-Position und die exakte Identität der tatsächlich laufenden
TX-Senderfirmware; danach folgt der versiegelte Preflight.

### 2026-08-09 — Mac-Position, Türzustand und TX-App exakt erfasst

Als Bezugspunkt für den Mac wurde die Mitte des Unterteils festgelegt. Der Mac
steht auf gleicher Höhe wie RX4 und 4 cm von RX4 entfernt auf der von RX2
wegführenden Linie. In der ursprünglichen Notation
`(Breite, Länge, Höhe)` ist seine Position damit
`(0,94 m, 0,00 m, 0,87 m)`; für RuView ergibt sich
`[4.02, 0.87, 2.50]`. Die Tür ist geschlossen und bleibt für Preflight,
Kalibrierung und Aufnahmen in diesem Zustand. Das CSI-WLAN ist verbunden und
der Mac verwendet wie vorgesehen `CSI_HOST_IP/24`.

Dieser Mac-Standort ist nicht der historische Aufbau „Mac mittig“ vom
2026-07-26. Frühere Leerraumreferenzen sind deshalb nicht auf die neue Serie
übertragbar. Mac, normale Gegenstände, Möbel und Kabel werden in der neuen
65-Sekunden-Leerraumkalibrierung als statischer Hintergrund mitgemessen.

Der TX wurde zuerst mit `flash_id` ohne BOOT-Taste und ohne Schreibzugriff als
ESP32-S3 Revision 0.2 mit 16 MB Flash und 8 MB PSRAM bestätigt. Weil
`flash_id` keine exakte Firmwareidentität liefert, wurde vor dem Versiegeln ein
zweiter ausschließlich lesender USB-Lauf durchgeführt. Ein 16-MB-Vollreadback
bei 460800 Baud brach bei ungefähr 7 % wegen serieller Paketstörung ab; die
unvollständige Temp-Datei wurde gelöscht und der TX blieb unverändert.

Bei 115200 Baud wurden danach Partitionstabelle und OTA-Auswahl gelesen. Der
TX bootet aus `app0` bei `0x10000`. Nur diese aktive 1280-KiB-App-Partition
wurde bei 230400 Baud vollständig ausgelesen; NVS und WLAN-Zugangsdaten waren
nicht Teil dieses App-Readbacks. Das gültige Image meldet Projekt
`arduino-lib-builder`, App-Version `43a8f6d`, Kompilierzeit
`2026-06-02 11:17:54` und ESP-IDF `v5.5.4`. Der SHA-256 des vollständigen
aktiven Partitions-Readbacks lautet
`a66a11ad8e299a962572c2bc8a9e4067599a8460c44ae0efb1deae07277994e5`.
Der eingebettete Image-Validierungshash
`586d81820c929ed236f9ea0c6bf389ff00b3cc0e69b60f21478f53a05cdeb285`
war gültig.

Die vier aktuellen privaten RX-Zustände für Node-ID 1 bis 4 verwenden Kanal 6
und denselben gesetzten TX-Filter. Dessen SHA-256 nach
`sha256-ruview-tx-filter-mac-v1` lautet, ohne die rohe Adresse in diesem neuen
Eintrag zu wiederholen,
`60c998af0f5f845bd2afaac558a7da831a3a34ec07544de0efc6d1e747fad86c`.
Eine alte doppelte RX3-Datei ohne Filter bleibt ausgeschlossen. Rohe MAC,
WLAN-Zugangsdaten und OTA-Schlüssel wurden in diesem neuen Eintrag nicht
wiederholt.

Nach der Prüfung wurden der unvollständige Vollreadback, Partitionstabelle,
OTA-Daten und aktive App-Kopie aus `/private/tmp` gelöscht. Es wurde nichts
geflasht, provisioniert oder konfiguriert. Der TX ist am Ende dieses
Zwischenstands noch für den Readback per USB angeschlossen; das Setup ist
daher bewusst noch nicht versiegelt und der Preflight wurde nicht gestartet.
Als Nächstes wird das USB-Datenkabel entfernt, der TX wieder ausschließlich
mit Strom versorgt und erst dann das endgültige Setup erzeugt.

Ausführlicher Nachweis:
[results/2026-08-09_D6_setupaufnahme-und-TX-firmwareidentitaet.md](results/2026-08-09_D6_setupaufnahme-und-TX-firmwareidentitaet.md)

### 2026-08-09 — Setup versiegelt und 25-Sekunden-Preflight bestanden

Der TX wurde nach dem Readback wieder ausschließlich mit Strom versorgt. Mac,
TX, RX1 bis RX4, Möbel, Kabel und die geschlossene Tür blieben danach
unverändert. Aus den dokumentierten realen Angaben wurde die private
Setup-Spezifikation erstellt und mit dem freigegebenen Serverbuild vom
2026-08-09 versiegelt.

Das resultierende Setup besitzt die ID `setup-0a49d75f122f9dc9` und den
SHA-256
`0a49d75f122f9dc9757aed7e175bb444056e7fdde6889bd8965288d1b9008a4e`.
Spezifikation und Siegel liegen mit Modus `0600` unter
`private/d6-20260809/`. Das Siegel bindet unter anderem die exakte Geometrie,
Mac-/Kabel-/Möbel-/Türrevisionen, Kanal und Raster, TX- und RX-Firmwarehashes,
den gemeinsamen TX-Filterhash sowie den Server-SHA-256
`91feb860f89f094ba16ea9d749e3a1e5378de1a25ceedd08cebeb67f2cd3484b`.

Ein erster Serverstart validierte das Siegel, konnte in der lokalen
Anwendungs-Sandbox aber UDP und WebSocket nicht binden und endete vor jeder
Aufnahme. Mit den erforderlichen lokalen Portrechten startete derselbe
unveränderte Release danach erfolgreich. `/health/ready` meldete Quelle
`esp32`, Status `ready` und das richtige aktive Setup. Exakt RX1 bis RX4 waren
aktiv; alle fünf Laufzeit-Bindingflags waren bei jedem RX wahr, das Binding-
Alter lag vor dem Lauf zwischen 15 und 36 ms.

Der neutrale Lauf `preflight-neutral-20260809-01` endete nach 25 Sekunden mit
2.545 Frames, 0 Drops, `completed`, `incomplete=false`, keinem
Integritätsfehler und passender Setup-ID samt Setup-Hash. RX1 lieferte 604,
RX2 647, RX3 684 und RX4 610 Frames. Alle vier RX verwendeten 2437 MHz, eine
Antenne, 64 Subcarrier, PPDU-Typ 0 und Layout-Flags 0. Die Rohdatei besitzt
SHA-256
`0bd38597fc59083d1a61a2e752202a7784bacc878ff449ceb7d8a278cfce31a3`,
die Metadatei
`5488fd47ded95bafc786cace4b88acb81c1e09289dc455a95c8c7d129df2280b`.

Damit ist der vorab definierte versiegelte Transport-, Binding-, Raster- und
Recorder-Preflight bestanden. Er ist noch kein Classification- oder
Positions-PASS. Zusätzlich blieb der allgemeine Engine-Trust wegen
wiederholter RX-Zeitstempelspreizung über dem 60-ms-Guardintervall auf
`Restricted` und unterdrückte Live-Roh-Ausgaben. Das entwertet den bestandenen
Raw-Recorder-Preflight nicht, muss aber vor der abschließenden Live-Anzeige
erneut geprüft werden.

Nächstes Gate ist die 65-Sekunden-Leerraumkalibrierung ohne Person. Sie startet
erst nach ausdrücklicher Bestätigung, dass der Raum während der vollständigen
Dauer leer bleibt.

Ausführlicher Nachweis:
[results/2026-08-09_D6_setup-siegel-und-preflight.md](results/2026-08-09_D6_setup-siegel-und-preflight.md)

### 2026-08-09 — Offline-Sidecar-Fix und neu versiegelter Preflight

Die erste 65-Sekunden-Leerraumaufnahme war live vollständig und verlustfrei,
wurde aber vom anschließenden strikten Positionsinspektor wegen der legitimen
Recorderfelder `max_duration_seconds` und `rx_summaries` abgelehnt. Vor P01
wurde deshalb angehalten. Die Aufnahme wurde nicht verändert.

Der Inspektor wurde minimal typisiert erweitert und prüft die
RX-Zusammenfassungen nun exakt gegen die Rohframes. Die zwei Regressionstests
und die vollständige Server-Binärtestsuite bestanden mit 398 von 398 Tests.
Wegen der geänderten ausführbaren Datei wurden ein separates Releaseartefakt
und ein neues Setup-Siegel erzeugt. Der physische Aufbau blieb unverändert.

Das neue Setup `setup-2beda4496ccfb547` bestand den erneuten versiegelten
Preflight `preflight-neutral-20260809-02` über 25 Sekunden mit 2.701 Frames,
0 Drops, vollständiger RX1-bis-RX4-Abdeckung und passender Setupidentität. Vor
P01 muss nun unter diesem neuen Siegel eine neue bestätigte
65-Sekunden-Leerraumkalibrierung erfolgen.

Ausführlicher Nachweis:
[results/2026-08-09_D6_sidecar-fix-neusiegelung-und-preflight.md](results/2026-08-09_D6_sidecar-fix-neusiegelung-und-preflight.md)

### 2026-08-09 — Neue Leerraumkalibrierung bestanden

Nach erneuter ausdrücklicher Leerraumbestätigung wurde unter dem neuen Siegel
`empty-neutral-20260809-02` über 65 Sekunden aufgenommen. Der Runner schrieb
6.102 Frames bei 0 Drops. Alle vier RX deckten die volle Dauer mit demselben
64-Subcarrier-Raster ab. Die mit dem eingefrorenen Release ausgeführte strikte
Offline-Inspektion bestand und band Raw-, Meta- und Signalhash eindeutig an
`setup-2beda4496ccfb547`.

Der Server war anschließend mit vier frischen D5-/D6-Referenzen operational.
Bei der Abschlussabfrage stimmte kein RX für Präsenz. RX3 hatte während der
Kalibrierung 55 bewegungsverdächtige Frames verworfen, erreichte aber dennoch
eine vollständige Referenz aus sechs Blöcken. Dieser Befund bleibt unverändert
dokumentiert. Als Nächstes darf die echte Trainingsserie an P01 beginnen.

### 2026-08-09 — Lokale Mess-UI verbunden

Die statische UI auf Port 3000 verwendete die Dockerportzuordnung 3000/3001
und konnte den versiegelten Server auf 8080/8765 deshalb nicht erreichen. Ein
lokaler Same-Origin-Proxy wurde auf Port 3002 gestartet, ohne den kalibrierten
Server neu zu starten. Die UI meldete danach API, Hardware und Streaming als
gesund sowie im Sensing-Tab reale ESP32-Hardware, vier aktive RX und eine
bestehende Verbindung. Die Positionsanzeige bleibt bis zum fertigen Index
absichtlich `UNCALIBRATED`.

### 2026-08-09 — Sensing-Raumansicht ausgerichtet

Die verbundene Sensing-UI zeigte TX und RX spiegelverkehrt, obwohl die
versiegelten Messkoordinaten korrekt waren. Ein erster Versuch mit einer
180°-Kameradrehung wurde als unzureichend verworfen, da eine Rotation keine
Spiegelung korrigiert. Die endgültige UI-Korrektur spiegelt nur die dargestellte
X-Koordinate (`x_display = Raumlänge - x`) für TX, RX, Positionskörper und
Signalfeld. Die live verbundene Ansicht zeigte danach RX2/RX4 links und
RX1/RX3 rechts. Kameraazimut, Setup, Kalibrierung und Messdaten blieben
unverändert.

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

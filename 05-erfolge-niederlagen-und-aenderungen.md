# 05 Erfolge, Niederlagen und Änderungen

Diese Datei sammelt bewusst auch negative Ergebnisse. Für den späteren Bericht sind sie wichtig, weil sie Grenzen des Systems begründen.

## Erfolge

| Datum | Erfolg | Bedeutung für die Arbeit |
|---|---|---|
| 2026-06-26 | TX-Firmware erfolgreich auf ESP32-S3 geflasht; Hash-Verifikation erfolgreich | Der kontrollierte Sender für Versuch A ist grundsätzlich vorbereitet |
| 2026-06-26 | TX-SoftAP startet mit anderem USB-Kabel erfolgreich; AP IP `192.168.4.1`, AP MAC `AE:27:6E:A8:D2:64` | Der kontrollierte Sender für Versuch A ist betriebsbereit |
| 2026-06-26 | Lokale RuView-`.venv` mit `esptool 5.3.0` und `esp-idf-nvs-partition-gen 0.2.0` eingerichtet | RX-Firmware kann geflasht und NVS-Provisioning vorbereitet werden |
| 2026-06-26 | RuView-Sensing-Server erfolgreich mit Cargo gebaut | Der PC/Mac kann nun als CSI-Empfangs- und Auswertungsserver für RX1 dienen |
| 2026-06-26 | Erster gültiger ESP32-CSI-Frame im RuView-Server erkannt: `node=1`, `subs=64`, `seq=0` | RX1 sendet nicht nur UDP-Pakete, sondern mindestens ein Paket wurde korrekt als CSI-Frame geparst |
| 2026-06-26 | `/api/v1/sensing/latest` liefert `source=esp32`, `node_id=1`, Amplitudenwerte und Feature-Werte | Die vollständige Softwarekette bis zur API-Ausgabe funktioniert für RX1 |
| 2026-06-26 | RX1 ist per Ping stabil erreichbar und sendet kontinuierlich UDP-Pakete an Port `5005` | Die Funk-/IP-Verbindung ist für weitere Tests grundsätzlich stabil |
| 2026-06-26 | Nach Entfernen des MAC-Filters sendet RX1 kontinuierlich Raw-CSI-Frames (`seq` steigt fortlaufend) | Messreihen mit echten Raw-CSI-Zeitverläufen sind nun möglich |
| 2026-06-27 | RX3 wurde als `node_id=3` provisioniert und bootet nach Stromversorgungswechsel ohne Brownout bis `CSI streaming active` | Der dritte RX ist firmwareseitig bereit; der verbleibende Blocker liegt im Host-IP-/Netzwerksetup |
| 2026-06-27 | Vier RX-Knoten senden gleichzeitig Raw-CSI an RuView: `node=1`, `node=2`, `node=3`, `node=4` | Der geplante 4RX-Aufbau ist erstmals vollständig online |
| 2026-06-28 | OTA-Status ist über WLAN auf mehreren RX-Knoten erreichbar (`/ota/status`) | Firmware-Updates ohne erneutes USB-Anschließen sind grundsätzlich möglich, falls der OTA-Upload akzeptiert wird |
| 2026-06-28 | Messreihen A0 bis A3 wurden automatisch in Dateien gespeichert | Es gibt nun Rohdaten und CSV-Zusammenfassungen für leeren Raum, stehende Person, Bewegung und ruhige Atmung |
| 2026-06-28 | G2 mit besser verteilten RX-Modulen liefert vollständige 4RX-Daten | Beide G2-Messungen enthalten 60/60 Samples mit Nodes 1,2,3,4 und >96 % vollständige 4x64-Subcarrier-Samples |

## Fehlschläge / Probleme

| Datum | Problem | Vermutete Ursache | Reaktion |
|---|---|---|---|
| 2026-06-26 | SoftAP-Funktion nach Flash noch nicht geprüft | Flash-Log bestätigt nur das Schreiben der Firmware, nicht den laufenden Betrieb | Seriellen Monitor öffnen, AP-IP und AP-MAC prüfen, Laptop mit `csi-test` verbinden |
| 2026-06-26 | TX-ESP32 startet mit `E BOD: Brownout detector was triggered` neu | Versorgungsspannung bricht beim Boot/WiFi-Start ein | Besseres USB-Kabel, aktiver USB-Hub oder externes 5V-Netzteil verwenden; danach SoftAP erneut prüfen |
| 2026-06-26 | Brownout bleibt trotz `WiFi.setTxPower(WIFI_POWER_11dBm)` bestehen | Stromversorgung weiterhin instabil oder Brownout entsteht vor/nicht nur durch TX-Power | Minimal-Sketch ohne WiFi testen; SoftAP-Start verzögern; aktiven Hub/externes Netzteil verwenden |
| 2026-06-26 | Brownout trat mit ursprünglichem USB-Kabel beim WiFi-Start auf, verschwand mit anderem Kabel | USB-Kabel verursachte Spannungsabfall bei WiFi-Stromspitze | Stabiles Kabel als Pflicht für weitere Messungen festhalten |
| 2026-06-26 | `python3 -m esptool --version` meldet `No module named esptool` | Python-Flash-Tools noch nicht installiert | Lokale `.venv` mit `esptool` und `nvs-partition-gen` anlegen |
| 2026-06-26 | Paket `nvs-partition-gen` nicht auffindbar | Falscher/alter Paketname | Korrektes Paket `esp-idf-nvs-partition-gen` installiert |
| 2026-06-26 | `cargo` nicht gefunden | Rust-Toolchain noch nicht installiert oder nicht im PATH | Rust via `rustup` installieren und Server erneut bauen |
| 2026-06-26 | RuView-Build findet `vendor/rufield/crates/rufield-core/Cargo.toml` nicht | Git-Submodule waren noch nicht initialisiert | `git submodule update --init --recursive` ausführen und Build erneut starten |
| 2026-06-27 | RX3 startete zuerst immer wieder mit `Brownout detector was triggered` beim WiFi-/PHY-Start neu | USB-Stromversorgung/Kabel reichte fuer die WiFi-Stromspitze nicht aus | Versorgung/Kabel geaendert; danach seriell `brownout_count=0` beobachtet |
| 2026-06-27 | Trotz korrekt laufendem RX3 ist der 3RX-Live-Test noch nicht nachgewiesen | Der Mac war im normalen WLAN (`192.168.178.123`), waehrend RX3 selbst `192.168.4.4` bekam und auf `target_ip=192.168.4.4` sendete | Fuer den Live-Test feste Mac-IP im `csi-test`-Netz setzen und alle RX auf diese Ziel-IP provisionieren |
| 2026-06-27 | `Multistatic fusion failed`, obwohl alle vier Nodes Raw-CSI senden | Zeitspreizung der Frame-Ankunft liegt über dem Guard-Intervall; Nodes sind noch nicht hinreichend synchronisiert | Für erste Visualisierung den Fallback akzeptieren; für spätere Positionsmessung Timing/Traffic/TDM verbessern |
| 2026-06-28 | `192.168.4.5` ist als Mac-Ziel-IP ungeeignet | Diese Adresse wurde aktuell von einem ESP32-Knoten belegt | Feste Mac-Ziel-IP außerhalb der bisherigen DHCP-Vergabe wählen, z. B. `192.168.4.50` |
| 2026-06-28 | Leerer Raum A0 wurde fast durchgehend als `presence=True` klassifiziert | Aktuelle RuView-Klassifikation ist für diesen Aufbau noch nicht kalibriert; Funkumgebung/Traffic erzeugt False Positives | Klassifikation nicht ungeprüft verwenden; eigene Auswertung/Schwellwerte und Wiederholungsmessungen nutzen |
| 2026-06-28 | Vitalwerte wurden auch im leeren Raum ausgegeben | Vital-Estimator interpretiert Signal-/Rauschanteile als BPM | Atem-/Herzfrequenz nur mit Referenzsensor oder manuellem Atemzählen bewerten |
| 2026-06-28 | Webansicht springt trotz besser verteilter RX-Module stark hin und her | RuView nimmt auch im leeren Raum `presence=True`/`estimated_persons=1` an; reale Node-Positionen sind noch nicht gesetzt | Web-Pose vorerst nicht als Messwert verwenden; `--node-positions` und Baseline/Kalibrierung testen |

## Änderungen am Aufbau

| Datum | Änderung | Grund | Auswirkung |
|---|---|---|---|
| 2026-06-26 | Rust/Cargo installiert und RuView-Submodule geladen | Lokaler RuView-Sensing-Server benötigt Rust-Abhängigkeiten und Vendor-Code | Server-Build ist erfolgreich; nächster Schritt ist Live-Empfang auf UDP-Port `5005` |
| 2026-06-27 | RX3-Stromversorgung/Kabel nach Brownout-Befund angepasst | WiFi-/PHY-Start loeste vorher Brownout aus | RX3 bootet stabil, verbindet sich mit `csi-test` und initialisiert CSI |
| 2026-06-27 | RX1-RX4 auf aktuelle Mac-Ziel-IP `192.168.4.5:5005` gebracht | DHCP hatte die Host-IP verändert; alte Target-IPs machten Nodes unsichtbar | Alle vier Nodes werden vom RuView-Server empfangen |
| 2026-06-28 | Remote-Konfiguration über HTTP `/config` als Firmware-Erweiterung vorbereitet | Künftige Target-IP-/Node-ID-Änderungen sollen ohne USB-Provisioning möglich sein | Nach OTA-Deployment können ausgewählte NVS-Werte per WLAN gesetzt und per Reboot aktiviert werden |
| 2026-06-28 | Für den nächsten Visualisierungstest wird ein größeres RuView-Guard-Intervall geplant | Standard 60 ms ist für die beobachtete WLAN-/ESP32-Zeitspreizung zu eng | Server mit `WDP_GUARD_INTERVAL_US=500000` und `WDP_SOFT_GUARD_US=200000` starten |

## Änderungen an Software / Parametern

| Datum | Änderung | Grund | Auswirkung |
|---|---|---|---|
| 2026-06-27 | RX3 per NVS mit `--reset`, `node_id=3`, `edge_tier=0`, `channel=6`, `target_ip=192.168.4.4`, `target_port=5005` provisioniert | Alte Provisioning-Reste ausschliessen und dritten RX eindeutig konfigurieren | RX3 liest die erwarteten NVS-Werte und startet als CSI-RX3 |
| offen | Feste Host-IP fuer 3RX-Test festlegen, vorgeschlagen `192.168.4.50` | DHCP vergab `192.168.4.4` an RX3, waehrend diese Adresse als Mac-Ziel-IP erwartet wurde | Nach Umstellung muessen RX1-RX3 auf dieselbe feste Mac-IP reprovisioniert werden |
| 2026-06-28 | Feste Host-IP auf eine freie Adresse außerhalb der beobachteten ESP-Adressen verschieben, vorgeschlagen `192.168.4.50` | `192.168.4.5` wurde von einem ESP belegt und ist daher keine sichere Host-Adresse | Reduziert künftige DHCP-/Ziel-IP-Konflikte |
| 2026-06-28 | Guard-Intervall-Testwert festgelegt: 500 ms hard / 200 ms soft | Pragmatiker-Workaround für RuView-Visualisierung bei unsynchronisierten ESP32-Nodes | Darf nicht als Beleg für echte synchrone Fusion interpretiert werden |
| offen | Reale RX-Positionen als `--node-positions` setzen | Die Webvisualisierung braucht die tatsächliche Geometrie der Empfänger | Erwartet stabilere und weniger willkürliche Feld-/Positionsanzeige |

## Beobachtete Grenzen

| Grenze | Beobachtung | Relevanz für Diskussion |
|---|---|---|
| Stromversorgung | TX-ESP32 löste Brownout beim Start mit schlechtem USB-Kabel aus; anderes Kabel löste das Problem | Stabile Versorgung ist Voraussetzung für reproduzierbare Messungen |
| kontinuierlicher CSI-Stream | Bisher ist ein gültiger CSI-Frame im RuView-Log sichtbar; kontinuierliche Verarbeitung muss noch geprüft werden | Für Bewegungs- und Atemmessungen reicht ein Einzel-Frame nicht aus; erforderlich ist ein stabiler Zeitverlauf |
| Vitalzeichen | API liefert aktuell noch `breathing_rate_bpm: null` und `heart_rate_bpm: null` | Atem-/Herzfrequenz ist noch kein belastbares Ergebnis; zunächst nur Signalreaktion und Bewegung prüfen |
| Paketformat | Kontinuierliche UDP-Pakete sind 60 Byte lang und beginnen mit `0xC5110006` (`feature_state`), nicht `0xC5110001` (`raw CSI`) | Für die Methodik muss getrennt werden zwischen vorverarbeiteten Features und echten Roh-CSI-Daten |
| MAC-Filter | Filter auf die TX/AP-MAC reduzierte den Raw-CSI-Strom stark; ohne Filter kommen fortlaufende Frames | Für frühe Tests ist kein MAC-Filter robuster; Filter erst später gezielt testen |
| Host-IP / DHCP | RX3 bekam im `csi-test`-Netz selbst `192.168.4.4`, obwohl diese Adresse als Mac-Ziel-IP provisioniert war | Fuer Mehrknotenmessungen braucht der Server-Host eine stabile Zieladresse; DHCP-Zufall kann RX-Daten unsichtbar machen |
| Mehrknoten-Synchronisation | Bei 4RX meldet RuView `Timestamp spread ... exceeds guard interval` und fällt auf per-node Fallback zurück | Für echte Positionsfusion reicht reiner Mehrknotenempfang nicht; zeitliche Synchronität ist ein zusätzlicher limitierender Faktor |
| Server-Guard-Intervall | Ein größeres Guard-Intervall kann Fallback-Meldungen reduzieren, akzeptiert aber stärker zeitversetzte Frames | Nützlich für Visualisierung, aber methodisch schwächer für präzise Positions-/Atemanalyse |
| Visualisierung/Pose | Die Webansicht zeigt springende Pose-/Personhypothesen; leerer Raum wird teils als Person interpretiert | Die Visualisierung ist aktuell nur qualitativ und nicht als exakte Positionsmessung nutzbar |
| Abstand | offen | |
| Bewegung während Atemmessung | offen | |
| mehrere Personen | offen | |
| Raum-/Möbelabhängigkeit | offen | |
| instabile Funkverbindung | offen | |

## Berichtsfähige Kernaussagen

Hier werden spätere Formulierungen gesammelt, die durch Messungen belegt werden müssen.

- WLAN-CSI reagiert auf Bewegung im Raum.
- Atemerkennung ist nur unter ruhigeren Bedingungen sinnvoll bewertbar.
- Mehrere Empfänger liefern wichtigere räumliche Information als ein einzelner Link.
- mmWave eignet sich als Referenz, beantwortet aber nicht die WLAN-CSI-Frage selbst.

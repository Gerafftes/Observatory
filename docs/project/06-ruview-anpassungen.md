# 06 RuView-Anpassungen – öffentlicher historischer Überblick

Diese Ableitung dokumentiert historische lokale Änderungen und ihre Grenzen. Test- und Buildzahlen sind gespeicherte damalige Angaben, keine in dieser Dokumentationsarbeit erneut ausgeführten Prüfungen. Implementierungsdetails und aktuelle Verträge stehen im [Softwareüberblick](../../software/README.md) und im [Live-Datenvertrag](../../software/ruview/ui/observatory/LIVE_DATA_CONTRACT.md).

## Diagnose und Klassifikation, 18.–26. Juli 2026

Die damaligen Anpassungen übertrugen Raum-/TX-/RX-Geometrie an die UI, verbesserten Cache-Verhalten und diagnostische per-RX-Werte, korrigierten den Vergleich zum vorherigen Frame und normalisierten statische Bewegungsmerkmale. Ein Modell mit nur rund 41,5 % Trainingsgenauigkeit wurde unter der 70-%-Grenze verworfen. Eine UI-Deadzone stabilisiert nur die Anzeige und belegt keine höhere Messgenauigkeit.

D4 verwendete RMS-normalisierte Framevergleiche. Der erste E0-Lauf war wegen zweier Raumzutritte kein gültiger Leerraumtest. E0b ergab danach 218/237 globale Still-Präsenzen im tatsächlich leeren Raum. Der Laptopstandort beeinflusste RX4; das zweite Paar E0d/E1b widerlegte aber die feste RX4-Schwelle. Die ODER-Aggregation einzelner RX ließ weiterhin Drift als globale Präsenz erscheinen.

## D5: positive Offlineauswertung, gescheiterter Live-Transfer

Die experimentelle Referenz nutzte sechs nicht überlappende 10-s-Leerraumblöcke pro RX, Median und max(1,4826 × MAD; 0,005), ein kausales 10-s-Fenster, z > 1, zwei absolute RX-Stimmen und zwei Sekunden Persistenz. Zwei vertauschte historische Replay-Folds erreichten im Mittel 0,0 % Leerraum-Fehlpräsenz, 89,3 % Still-Recall und 94,7 % Balanced Accuracy. Ein strengerer Kandidat z > 3 mit nur 15,5 % Recall sowie ein überwachter Selektor mit 20,8 % Fehlpräsenz wurden verworfen.

Die separate Klassifikationskalibrierung war vom SVD-FieldModel getrennt. Rasterwechsel, Evidenzverlust und Datenlücken mussten Referenzen beziehungsweise Livefenster verwerfen. Softwareprüfungen vor dem physischen Test ersetzten keine reale Leistungsprüfung.

Im anschließenden realen Still-Livetest wurden alle 350 Samples global ABSENT ausgegeben. RX4 trug zunächst zeitweise, danach RX3 dauerhaft; das Zwei-RX-Quorum wurde nie erreicht. D5 blieb experimentell. Eine Lockerung ohne zugehörigen neuen Leerraumlauf würde die bereits bekannten Fehlalarme nicht ausschließen.

## D6 und diskrete Position, ab 29. Juli 2026

D6 trennte Rohdatenerfassung, Präsenzmerkmale zur robusten Leerraumreferenz und Position für neun feste Punkte. Gain-normalisierte Subcarrier-Form, stabile Bin-Maske, signierte Residuen, separate RSSI-/RMS-Abweichungen und strikte Rasterbindung erweiterten den einseitigen D5-Score. Das beweist für sich keine reale Präsenzklassifikation.

Recorder und Sidecar müssen Rohframes, Zeitstempel, Raster, Setup und Serverstand binden. Eingebettete Labels oder Ground Truth sind in Positionsaufnahmen unzulässig. Der historische 9-Punkt-Ansatz verwendete 28 Merkmale je RX und 3-s-Fenster, mindestens 5 Hz beziehungsweise 15 Frames je RX, höchstens eine Sekunde Lücke und gemeinsame zeitliche Abdeckung. Sechs unabhängige 5-s-Blöcke je Punkt bildeten robuste Prototypen; alle vier RX wurden gleich gewichtet. Zwischen Punkten wird nicht interpoliert. Unknown und ambiguous sind erforderliche Ausgaben bei unzureichender Evidenz.

## Anzeige- und Evidenzgrenzen

Heatmap und grobe Feldspitzen sind Diagnosewerte. Ohne gültige Position, bestätigte Präsenz oder frische Daten darf keine Personposition erscheinen. TX-/RX-Marker zeigen nur den festen Aufbau. Historische synthetische Tests mit 9/9 richtigen Punkten, Coverage/Accuracy 1,0 sowie Fehler 0,0 m und der damalige Rust-Lauf mit 852 bestandenen, zwei ignorierten Tests sind Offlinebelege. Sie ersetzen keine realen Blindaufnahmen.

Der damalige Ausgabeaudit verlangte dasselbe für alle ESP32-Personenausgaben: fertige D6-Referenz, gültiges P01–P09-Ergebnis und keine prozeduralen Pose-Keypoints. CONNECTING, LIVE ESP32, SIMULATED und STALE wurden getrennt; ESP32-Frames verfielen nach drei Sekunden Browserzeit. Eine ausdrücklich beschriftete Simulation bleibt von Hardwaredarstellung getrennt. Die Filteridentität wurde als Hash über die sechs binären Filterbytes vereinheitlicht, ohne die Geräteadresse im Bericht wiederzugeben.

Die [D4/D5/D6-Gesamtauswertung](../../experiment-reports/2026-08-23_D4-D5-D6_technischer-bericht/README.md) führt reale Fehlerraten, technische Recorderbelege und fehlende Ground Truth getrennt auf.

## Historische Sicherheitsprüfung vom 2. September 2026

Die früheren Source-READMEs hielten die folgenden Ausgangsbefunde und berichteten Gegenmaßnahmen fest. Alle Test-/Fixangaben gehören zum damaligen Quellstand; diese Dokumentationszusammenführung führt keinen Scan oder neue Sicherheitsabnahme aus.

Die historische Sicherheitsprüfung vom 2. September 2026 enthielt 13 Listeneinträge; doppelte WebSocket- und REST-Zeilen ergeben elf eigenständige Befunde.

Im damaligen Ausgangszustand waren CSI-Konfiguration und persistente Konfigurationsschreibvorgänge ohne Authentifizierung erreichbar.

WASM-Lebenszyklus und OTA-Pfad waren unzureichend autorisiert.

Die damalige RVF-WASM-Signaturprüfung war ohne privaten Schlüssel fälschbar.

RuField-Liveereignisse verwendeten einen öffentlich ableitbaren Standardschlüssel.

HomeCore akzeptierte standardmäßig triviale oder beliebige nichtleere Bearer-Tokens.

Die Sensing-Bridge ließ Live-Telemetrie ohne erforderlichen Bearer-Schutz zu.

Der damalige Origin-Schutz setzte localhost auf verschiedenen Ports gleich; dadurch waren portübergreifende WebSocket- und mutierende REST-Zugriffe möglich.

Der berichtete Quellstand ließ ESP32-Schreib-, WASM- und OTA-Flächen ohne PSK fehlschlagen.

RVF verwendete im berichteten Quellstand Ed25519-Signaturen.

RuField erforderte einen deploymentspezifischen Schlüssel.

HomeCore akzeptierte nur explizit provisionierte Tokens.

Sensing-Bridge und nichtlokale Sensing-Binds verlangten Bearer-Schutz.

WebSockets und mutierende REST-Aufrufe verwendeten eine exakte Origin-Allowlist unabhängig von Host-/DNS-Rebinding-Prüfungen; Standard waren die lokalen UI-Origins des konfigurierten HTTP-Ports, weitere UI-Origins mussten vollständig konfiguriert werden.

Gespeicherte Quell-/Hostverifikation: Origin-Tests 24/24; Sensing-Server 471 bestanden; HomeCore 18 + 18 + 6 + 5 Tests; Python-/Firmware-Sicherheitsgrenztests 7 bestanden.

Diese Prüfungen ersetzten keinen nativen Firmware-Build: die ESP-IDF-v5.4-Matrix für ESP32-S3 mit 8 MB, ESP32-S3 mit 4 MB und ESP32-C6 mit 4 MB blieb lokal inconclusive.

Für diese historische Sicherheitsprüfung wurde weder geflasht noch OTA ausgeführt; eine vollständige Firmware- oder Produktionsfreigabe wurde nicht beansprucht.

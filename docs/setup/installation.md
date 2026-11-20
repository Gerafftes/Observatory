# Installation und erster Start

Diese Anleitung startet die Oberfläche mit simulierten Daten. Für Hardwarebetrieb sind zusätzlich ein bestätigter Aufbau, passende Firmware, Geräteprofile und aufbaugebundene Prüfungen erforderlich; siehe [Firmware und Flashen](firmware.md).

## Voraussetzungen

- Git, OpenSSL und eine für das Rust-Projekt geeignete Rust-/Cargo-Umgebung.
- Ein Browser und ein freier lokaler HTTP-Port `3002` sowie WebSocket-Port `3001`.
- Beim ersten Build gegebenenfalls Netzwerkzugriff für nicht gecachte Paketabhängigkeiten.

Die Herkunft des RuView-Snapshots und der eingebundenen Fremdquellen beschreibt der [Software-Einstieg](../../software/README.md). Die Vendor-Quellen liegen in diesem Projekt als Dateien vor; ein zusätzlicher RuView-Checkout ist für den beschriebenen Einstieg nicht erforderlich.

## Quellstand vor dem Start prüfen

Der Schnellstart wurde in einem frischen Checkout des öffentlichen GitHub-Commits
`11ccc9b227520545a07f9751940cec749d1ddb52` gebaut und ausgeführt.
Prüfe bei einem anderen Quellstand, ob das daraus gebaute Binary unter
`--help` die Optionen `--rx-session-only` und `--data-dir` anbietet.
Sind beide vorhanden, verwende den Aufruf unten mit diesen Optionen.

## Simulation starten

Aus dem passenden lokalen Arbeitsbaum mit einem neuen temporären Demo-Verzeichnis
(den ersten Pfad durch den eigenen Checkout-Pfad ersetzen):

```sh
cd /pfad/zum/lokalen/Observatory/software/ruview/v2 &&
observatory_v2="$PWD" &&
demo_dir="$(mktemp -d "${TMPDIR:-/tmp}/observatory-demo.XXXXXX")" &&
cd "$demo_dir" &&
WDP_RUFIELD_SIGNING_SEED="$(openssl rand -hex 32)" \
  cargo run --manifest-path "$observatory_v2/Cargo.toml" \
  --target-dir "$demo_dir/target" \
  -p wifi-densepose-sensing-server --no-default-features -- \
  --source simulate --rx-session-only --server-control-port 0 \
  --no-edge-registry --bind-addr 127.0.0.1 \
  --ui-path "$observatory_v2/../ui" --data-dir "$demo_dir/data" \
  --http-port 3002 --ws-port 3001
```

Öffne erst nach erfolgreichem Serverstart **genau diese Adresse**:

`http://127.0.0.1:3002/ui/index.html?server_control_port=3002#sensing`

Der URL-Parameter leitet die Anfragen der Browser-Steuerungsfelder zum Demo-Server statt zu einem möglicherweise bereits laufenden lokalen Steuerungshelfer. Dort sind die zugehörigen Helferrouten nicht vorhanden. Ohne den Parameter sucht die Oberfläche standardmäßig nach einem Helfer auf Port 8090. Beende den Demo-Server im Terminal mit `Ctrl+C`.

Der Server verwendet künstliche CSI-Daten. `--rx-session-only` schaltet die mmWave-Netzwerkdienste und -Steuerrouten ab; `--server-control-port 0` verhindert den separaten Steuerungshelfer. Die dazu ausgegebene Warnung über Port 0 ist bei diesem Demo-Aufruf erwartet. `--no-edge-registry` deaktiviert die Edge-Registry. Der Schlüssel gilt nur für diesen Start. Build und Laufzeitdaten liegen im neuen Demo-Verzeichnis, dessen Pfad `demo_dir` enthält. Das Verzeichnis bleibt nach dem Beenden erhalten.

Der Demo-Modus bietet keine mmWave-Steuerung und kein Starten oder Stoppen des Servers über die Oberfläche. Sind die Ports bereits belegt, wähle zwei andere freie Ports. Trage den neuen HTTP-Port sowohl in der Browseradresse als auch bei `server_control_port` ein. Cargo kann beim ersten Build Pakete herunterladen; die 3D-Ansicht lädt eine Bibliothek aus dem Internet. Ein neuer Build benötigt Zeit und Speicherplatz.

Die Simulation belegt weder angeschlossene Sensoren noch eine Messung oder Positionsgenauigkeit. Den Arbeitsablauf erklärt die [Benutzeranleitung](../user-guide/interface.md).

## Für einen realen Aufbau

1. [Hardware und Platinenrevision](../hardware/overview.md) auswählen und die tatsächliche Verdrahtung prüfen.
2. Den [Firmwarestand und die gerätespezifischen Voraussetzungen](firmware.md) prüfen. Eine bestehende Installation nicht mit einer Anleitung für ein leeres Board überschreiben.
3. Raum, Positionen und Identitäten im Setup erfassen; Server und Geräte im vorgesehenen Experimentnetz verbinden.
4. Datenquelle, Gerätezustände und Transport prüfen. Für echte Messungen Setup, Kalibrierung und Qualitätsgrenzen an den jeweiligen Versuch binden.

Ein vollständig reproduzierbarer allgemeiner Erstinstallationsweg für alle TX-, RX- und Radarknoten ist derzeit nicht nachgewiesen. Die Firmwareseite benennt die offenen Teile und die vorhandenen komponentenspezifischen Anleitungen.

## Umfang der vorhandenen Prüfung

Beim Schnellstarttest meldete die Status-API simulierte Daten, die
Browseroberfläche war erreichbar und der WebSocket-Handshake gelang. Die
mmWave-Statusroute lieferte HTTP 404. Beide TCP-Ports lauschten nur auf Loopback.
Der Test lief ohne angeschlossene Hardware. Die Optionen sind im
[Serverquelltext](https://github.com/Gerafftes/Observatory/blob/028f9d3f2703e1f3610e856f48fa0a709647818b/software/ruview/v2/crates/wifi-densepose-sensing-server/src/main.rs),
der Port-0-Abbruch im [Steuerungshelfer](https://github.com/Gerafftes/Observatory/blob/028f9d3f2703e1f3610e856f48fa0a709647818b/software/ruview/v2/crates/wifi-densepose-sensing-server/src/server_control.rs)
und der URL-Parameter im [Browser-Steuerungsdienst](https://github.com/Gerafftes/Observatory/blob/028f9d3f2703e1f3610e856f48fa0a709647818b/software/ruview/ui/services/server-control.service.js)
nachvollziehbar.

## Docker und Homebrew

Es gibt eine **separate lokale Offline-Vorbereitung vom 1. November 2026**
(`packaging-docker-homebrew-20261101-run02`), keine integrierte oder veröffentlichte
Installationsmethode. Deren Bericht und Paketvertrag dokumentieren Offlineprüfungen
für Startargumente, Guards, UI-Staging und den Ausschluss privater Dateien aus
einem synthetischen Docker-Kontext. Ein vorhandenes natives Binary wurde für
Staging und `--help` verwendet; in dieser Lieferung erfolgte kein neuer Build.
Beim Dokumentationsabgleich am 2. November lieferte dieses externe Binary mit
`--help` erneut beide Isolierungsoptionen. Es wurde dabei kein Server gestartet.

Ein Linux-Docker-Image samt BuildKit-, Link- und Laufzeitprüfung sowie eine echte
Homebrew-Installation mit Upgrade/Entfernung und einem zweiten Rechner stehen
aus. Die Docker-Vorlage ist netzlos und veröffentlicht keine Ports; sie bietet
noch keinen Browserzugang zu einem laufenden System. Laufdaten, dauerhafter
CAD-Katalog und Modelle haben getrennte externe Pfade. Die Einrichtung eines
neuen leeren CAD-Katalogs ist offen. Daraus folgt keine Paket-, Hardware- oder
Radarabnahme. Bericht, Paketvertrag und ihre Hashbindungen sind in der privaten
Übergabe zu diesem Dokumentationsabgleich verzeichnet; die Lieferung wird hier
nicht übernommen.

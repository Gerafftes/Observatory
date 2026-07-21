# 06 RuView-Anpassungen und lokale Patches

Dieses Projekt verwendet [ruvnet/RuView](https://github.com/ruvnet/RuView) als Softwarebasis für RX-Firmware und Sensing-Server.

Der RuView-Quellcode selbst bleibt ein separates Upstream-Repository. Dieses BLL-Repo dokumentiert nur, welche lokalen Änderungen im Projektverlauf vorbereitet oder getestet wurden.

## Lokale Firmware-Idee: HTTP-Config-Endpunkt

Motivation:

- Die RX-Knoten mussten mehrfach neu provisioniert werden, weil sich die Mac-IP im `csi-test`-Netz geändert hatte.
- OTA-Status war über WLAN erreichbar, der OTA-Upload war aber ohne PSK gesperrt (`403 Forbidden`).
- Ziel war, spätere Änderungen wie `target_ip`, `node_id`, `edge_tier` oder `csi_channel` ohne USB per WLAN setzen zu können.

Vorbereitete lokale Änderung:

- `config_server.c`
- `config_server.h`
- Registrierung in `main.c` auf dem bestehenden HTTP-/OTA-Server
- Beispiel-Endpoint: `POST /config?target_ip=192.168.4.50&reboot=1`

Status:

- Lokal vorbereitet, aber nicht auf die ESPs ausgerollt.
- OTA-Upload der aktuellen Firmware ist ohne gültigen OTA-PSK gesperrt.
- Für Einsatz wäre einmaliges USB-Flashen oder korrekt provisionierter OTA-PSK nötig.

## Lokale OTA-Größenkorrektur

Motivation:

- Der OTA-Status meldete eine künstliche `max_size` von 921.600 Byte.
- Das vorhandene Firmware-Binary war größer.
- Die reale OTA-Partition ist größer als diese künstliche Grenze.

Vorbereitete lokale Änderung:

- `ota_update.c` verwendet die reale Größe der nächsten OTA-Partition.
- Firmware-Upload wird nur abgelehnt, wenn das Binary größer als die reale OTA-Partition ist.

Status:

- Lokal vorbereitet, nicht auf ESPs ausgerollt.

## Server-Parameter für Visualisierung

Für die 4RX-Tests wurde der RuView-Server mit größerem Guard-Intervall gestartet:

```zsh
WDP_GUARD_INTERVAL_US=500000
WDP_SOFT_GUARD_US=200000
```

Beobachtung:

- Standard: 60 ms hard / 20 ms soft
- Testwert: 500 ms hard / 200 ms soft
- Mit 500 ms traten im G2-Serverlog nur 31 Fusion-Fallbacks bei 16.097 ESP32-Frame-Zeilen auf.

Einordnung:

- Nützlich für stabilere Webvisualisierung.
- Kein Ersatz für echte zeitliche Synchronisation.
- Für wissenschaftliche Auswertung muss dieser Parameter als methodische Einschränkung dokumentiert werden.

## Node-Positionen

RuView unterstützt:

```zsh
--node-positions "x,y,z;x,y,z;..."
```

Diese Geometrie ist für eine stabile räumliche Visualisierung hilfreich. Für die eigentliche Problemfrage dieser Arbeit ist sie aber nicht zwingend erforderlich, solange primär Bewegung, Signalveränderung und Atemband-Reaktion untersucht werden.

Einordnung:

- Ohne Node-Positionen: Bewegung/Signalreaktion kann trotzdem untersucht werden.
- Mit Node-Positionen: bessere Voraussetzung für Heatmap/Positionsanzeige.
- Die Webansicht wird aktuell nicht als exakter Messwert verwendet.

## Lokale Diagnose- und Klassifikationsänderungen vom 2026-07-18

Die folgenden Änderungen liegen im separaten lokalen RuView-Arbeitsbaum. Sie sind hier dokumentiert, aber nicht Bestandteil dieses Dokumentations-Repositories:

- Raummaße und TX-Position zusätzlich zu den RX-Positionen an die UI übertragen
- TX/RX-Marker beschriftet und räumlich nach den gemessenen Koordinaten dargestellt
- Service-Worker-Cache aktualisiert, damit geänderte UI-Dateien tatsächlich geladen werden
- Wolkenziel mit `0,45 m` Deadzone, `0,25 m` Zielstabilität und `1,5 s` Bestätigungszeit stabilisiert
- Kalibrierungsfeed für die Zustände `Uncalibrated` und `Collecting` aktiviert
- 128-/192-Werte für das 56-dimensionale Einzel-Link-Kalibrierungsmodell normalisiert
- adaptives Klassifikationsmodell unter `70 %` Trainingsgenauigkeit abgelehnt; vorhandenes Modell hatte nur rund `41,5 %`
- zeitlichen Bewegungsvergleich auf den tatsächlich vorherigen Frame korrigiert
- statische CSI-Merkmale aus dem unmittelbaren Bewegungsscore entfernt bzw. zur Signalleistung normalisiert
- globale Klasse als Konsens der aktiven RX statt aus dem zuletzt eingetroffenen Paket gebildet
- Feldenergie für `present_moving` von einem Defaultwert `0,05` auf `0,55` korrigiert; `present_still` liegt bei `0,30`
- diagnostische per-RX-Werte für Rohscore, geglätteten Score und Ruhe-Baseline ergänzt

Validierung:

- sieben neue Klassifikations-/Bewegungs-Unit-Tests bestanden
- Release-Build des Sensing-Servers erfolgreich
- vorhandenes 41,5-%-Modell wurde beim Serverstart nachweislich verworfen
- Live-Vergleich still/bewegt zeigte trotzdem starke Überlappung der Rohwerte

Einordnung:

Die lokalen Änderungen beheben nachweisbare Implementierungsfehler, liefern aber noch keinen belastbaren Klassifikator. Insbesondere die UI-Deadzone darf nicht als verbesserte Messgenauigkeit interpretiert werden. Der nächste technische Prüfpunkt liegt in der RX-seitigen Auswahl vergleichbarer Pakete des kontrollierten TX.

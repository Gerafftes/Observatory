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

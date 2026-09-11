# D6-Sidecar-Fix, Neusiegelung und Preflight — 2026-08-09

## Ausgangspunkt

Die erste 65-Sekunden-Leerraumaufnahme
`empty-neutral-20260809-01` wurde vom Live-Runner mit 4.760 Frames, 0 Drops und
passender damaliger Setupidentität sauber abgeschlossen. Die anschließende
strikte Offline-Inspektion lehnte jedoch die legitimen Recorderfelder
`max_duration_seconds` und `rx_summaries` als unbekannt ab. Deshalb wurde vor
P01 angehalten. Rohdatei und Sidecar wurden nicht verändert.

## Softwarekorrektur

Der Positionsinspektor akzeptiert die beiden expliziten Recorderfelder jetzt
im typisierten Sidecar-Schema. Die RX-Zusammenfassungen werden nicht nur
eingelesen, sondern vollständig aus den Rohframes neu berechnet und exakt mit
dem Sidecar verglichen. Unbekannte Felder, eingebettete Wahrheit, Verluste,
Rasteränderungen oder abweichende RX-Zusammenfassungen bleiben harte Fehler.

Die beiden betroffenen Regressionstests und anschließend die vollständige
Server-Binärtestsuite bestanden. Ergebnis: 398 bestanden, 0 fehlgeschlagen.
Die unveränderte erste Leerraumdatei ließ sich danach erfolgreich inspizieren;
der erzeugte private Inspektionsnachweis besitzt SHA-256
`6baf763722c61ed73dccac13d7a7b7aca23d12c9f44376bf630833ce1505dcb6`.

## Neues Artefakt und Setup

Da die ausführbare Serverdatei Teil des Setup-Siegels ist, wurde weder das alte
Artefakt noch das alte Siegel überschrieben. Der neue Release liegt unter
`artifacts/live-position-2026-08-09-sidecar-fix/` und besitzt:

- Größe: 5.954.256 Byte
- Server-SHA-256:
  `6554c5101bc7e920e9ce52ea5d845d2afd62b97f09d0c31917d1b1b61d14f8b5`
- Dateimodus: `0500`

Raum, Tür, Möbel, Kabel, Mac, TX, RX1 bis RX4, Firmware, Kanal, Filter und
CSI-Raster wurden unverändert in ein neues Siegel übernommen:

- Setup-ID: `setup-2beda4496ccfb547`
- Setup-SHA-256:
  `2beda4496ccfb547217f15ed62418d363aed8ddbc19221d872c4a89a1a3564a0`
- privater Siegeldatei-SHA-256:
  `2ded5d689c73ab3d0d1947ec14af6480ca86ecb86fb93cbe98fcf6ec7cb1d0d7`

Die erste Leerraumaufnahme bleibt ein unveränderter technischer Nachweis, kann
aber wegen ihrer alten Setupidentität nicht in die neue zusammenhängende
Trainings- und Blindserie eingehen.

## Neuer versiegelter Preflight

Der Lauf `preflight-neutral-20260809-02` bestand über 25 Sekunden:

| RX | Frames | Raster |
|---|---:|---|
| RX1 | 608 | 2437 MHz / 1 / 64 / PPDU 0 / Flags 0 |
| RX2 | 675 | 2437 MHz / 1 / 64 / PPDU 0 / Flags 0 |
| RX3 | 752 | 2437 MHz / 1 / 64 / PPDU 0 / Flags 0 |
| RX4 | 666 | 2437 MHz / 1 / 64 / PPDU 0 / Flags 0 |

Gesamtergebnis: 2.701 Frames, 0 Drops, `completed`, `incomplete=false`, kein
Writerfehler, kein Label, keine Ground Truth und exakt passende neue
Setupidentität.

- Raw-SHA-256:
  `636311c13a399565d1d16ddcce87709aa962e618eb9efb393488075d66a98c80`
- Meta-SHA-256:
  `e20969a6dd98bc62557ead6bde9739983ecf2606364214e2d34d27064ce9ae1e`

Der bekannte allgemeine Engine-Trust-Hinweis zur RX-Zeitstempelspreizung ist
weiter separat vor der finalen Live-Anzeige zu klären. Er verändert nicht den
bestandenen verlustfreien Recorder-Preflight.

## Neue Leerraumkalibrierung

Nach der ausdrücklichen Bestätigung, dass der Raum während der vollständigen
65 Sekunden ohne Person bleibt, wurde `empty-neutral-20260809-02` aufgenommen.
Der Live-Runner und die nachfolgende strikte Offline-Inspektion bestanden:

- Dauer: 65 Sekunden
- Frames: 6.102
- Drops: 0
- Status: `completed`
- `incomplete=false`
- Writerfehler: keiner
- Label/Ground Truth: nicht vorhanden
- RX1: 1.436 Frames
- RX2: 1.557 Frames
- RX3: 1.635 Frames
- RX4: 1.474 Frames
- Raster je RX: 2437 MHz / 1 Antenne / 64 Subcarrier / PPDU 0 / Flags 0
- Raw-SHA-256:
  `1d98c2fe78754d304693c507d0cd5b5d4eb1719b217e4576c3a8afa894a60871`
- Meta-SHA-256:
  `30d9753a35645efd82e26143b34522639b4ce6a1aea5427d266ee541b79f3a95`
- Signal-SHA-256:
  `2c4882012ce8bf2eba9cd98830c8bfe07e1597e0223f2a50fc44d353edcbdb3d`
- Inspektionsartefakt-SHA-256:
  `d83e6b44fa87b0748e4f018eb27ed9c5b6a16979cd3242a7953b98f83c107244`

Nach der Kalibrierung meldete der laufende Server `phase=ready`,
`decision_status=operational`, vier frische Referenzknoten und vier nutzbare
Liveknoten. D5 und D6 waren bei RX1 bis RX4 referenz- und evidenzbereit; bei
der Abschlussabfrage stimmte kein RX für Präsenz. RX3 verwarf während der
Kalibrierung 55 bewegungsverdächtige Frames, erzeugte aber trotzdem eine
vollständige Referenz aus sechs Blöcken und blieb unter dem D6-Anomaliegate.
Dieser Befund wird dokumentiert und nicht nachträglich wegparametriert.

## Nächstes Gate

Die versiegelte Serie ist jetzt bereit für die neun echten Trainingsaufnahmen.
Als Nächstes folgt P01 als unbeschriftete 35-Sekunden-Rohaufnahme; die
Punktzuordnung wird nur außerhalb der Rohdatei geführt.

## Lokal verbundene UI

Die zuvor unter `http://localhost:3000/` geöffnete statische UI erwartete API
und WebSocket auf den Dockerports 3000 und 3001. Der versiegelte Messserver
lief dagegen korrekt auf HTTP 8080 und WebSocket 8765. Deshalb zeigte diese
Seite fälschlich Offline-/Simulationszustände; die direkten Capture-Runner
waren davon nicht betroffen.

Ohne Neustart oder Änderung des kalibrierten Messservers wurde ein kleiner
Same-Origin-Proxy unter `RuView/scripts/run_local_sensing_ui.mjs` ergänzt und
auf `http://127.0.0.1:3002/` gestartet. Er liefert ausschließlich die lokale
RuView-UI aus und leitet lokale API- und WebSocketzugriffe an 8080
beziehungsweise 8765 weiter. Die sichtbare Prüfung bestand:

- Dashboard: API, Hardware, Inference und Streaming `HEALTHY`
- Datenquelle: `ESP32`, reale Hardware verbunden
- Sensing: `LIVE — ESP32 HARDWARE`, `Connected`
- exakt vier aktive Nodes
- Position: erwartungsgemäß `UNCALIBRATED`, da noch kein P01-bis-P09-Index
  gebaut wurde

Serverprozess, Serverbinärdatei, Setup-Siegel und Leerraumreferenz wurden dabei
nicht neu gestartet oder verändert.

### Korrektur der UI-Raumansicht

Bei der ersten verbundenen Sichtprüfung waren TX und RX spiegelverkehrt
dargestellt. Die versiegelten Koordinaten waren unverändert korrekt. Ein erster
rein visueller Versuch, den Kameraazimut um 180° zu drehen, wurde als
unzureichend verworfen: Eine Rotation ändert die Blickrichtung, bildet aber
keine echte Spiegelung ab.

Die endgültige Korrektur spiegelt deshalb ausschließlich für die Darstellung
die Raum-X-Koordinate mit `x_display = Raumlänge - x`. Sie wird einheitlich auf
TX, RX, Positionskörper und Signalfeld angewendet; Höhe und Z-Koordinate bleiben
unverändert. Nach dem Neuladen zeigte die weiterhin live verbundene UI RX2/RX4
links und RX1/RX3 rechts. Kameraazimut, Setup-, Rohdaten-, Kalibrierungs- und
Messkoordinaten blieben unverändert.

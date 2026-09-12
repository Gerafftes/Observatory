# 07 Screenshot-Nachweise

Diese Datei ordnet die vom Schreibtisch übernommenen Screenshots nach Zeitstempel und Inhalt. Die Bilder sind qualitative Belege und Kontextmaterial; belastbare Messwerte bleiben die Rohdaten/CSV-Dateien unter `data/raw/` und die Auswertungen unter `results/`.

Die Desktop-Originale wurden nicht verändert. Die sortierten Kopien liegen unter `skizzen/screenshots/`.

## Sortierte Ablage

| Zeitstempel | Datei | Inhalt | Gehört zur Dokumentation | Einordnung |
|---|---|---|---|---|
| 2026-06-23 22:33:57 | [2026-06-23_22-33-57_display-pinout.png](skizzen/screenshots/2026-06-23_22-33-57_display-pinout.png) | Display-Modul: technische Übersicht, Pinout und Leitungsfarben | Hardware-/Display-Notiz | Kein CSI-Messnachweis; nur optionale Hardware-Referenz. |
| 2026-06-26 15:43:09 | [2026-06-26_15-43-09_ruview-observatory-demo-ui.png](skizzen/screenshots/2026-06-26_15-43-09_ruview-observatory-demo-ui.png) | RuView Observatory im Browser, `DEMO`/`Auto-Cycle`, Vital- und Presence-Anzeige | UI-Erstprüfung vor echter Messung | Nicht als Messwert verwenden; zeigt nur die verfügbare Visualisierung. |
| 2026-06-26 16:03:42 | [2026-06-26_16-03-42_sensing-server-start-websocket.png](skizzen/screenshots/2026-06-26_16-03-42_sensing-server-start-websocket.png) | RuView-Sensing-Server startet mit `source=esp32`, HTTP `8080`, WebSocket `8765`, UDP `5005`; WebSocket verbindet sich | Projektjournal: lokaler Serverstart / Toolchain-Nachweis | Belegt, dass der Server lokal lief; unten ist zusätzlich ein vorheriger falscher Startpfad sichtbar. |
| 2026-06-26 16:07:18 | [2026-06-26_16-07-18_tcpdump-udp-monitor-start.png](skizzen/screenshots/2026-06-26_16-07-18_tcpdump-udp-monitor-start.png) | Server und UI sind verbunden; `tcpdump -i en0 udp port 5005` wurde zum Mithören gestartet | Debug-Nachweis Netzwerkempfang | Zeigt den Beginn des UDP-Monitorings, noch keinen ausgewerteten CSI-Frame. |
| 2026-06-26 16:08:13 | [2026-06-26_16-08-13_udp-60byte-traffic.png](skizzen/screenshots/2026-06-26_16-08-13_udp-60byte-traffic.png) | Wiederholte UDP-Pakete mit Länge 60 im `csi-test`-Netz | Projektjournal: frühe UDP-/Feature-State-Beobachtung | Passt zur späteren Einordnung, dass 60-Byte-Pakete sichtbar waren, aber noch nicht automatisch als Raw-CSI-Zeitverlauf reichen. |
| 2026-06-26 17:00:30 | [2026-06-26_17-00-30_websocket-pose-stream-real-backend.png](skizzen/screenshots/2026-06-26_17-00-30_websocket-pose-stream-real-backend.png) | UI-Activity-Log: realer Backend-WebSocket, Pose-Stream verbunden, unregistrierte Close-Warnung | Projektjournal: UI-/WebSocket-Verbindung | Belegt Verbindung der Webansicht zum lokalen Backend, aber keine Messqualität. |
| 2026-06-26 18:13:10 | [2026-06-26_18-13-10_first-valid-csi-frame-api-monitor.png](skizzen/screenshots/2026-06-26_18-13-10_first-valid-csi-frame-api-monitor.png) | `tcpdump`, Ping-Traffic, Serverlog mit `ESP32 frame ... node=1, subs=64` und API-Monitor mit `tick`, `presence`, RSSI und Feature-Werten | Projektjournal: erster gültiger ESP32-CSI-Frame und `/api/v1/sensing/latest` | Wichtiger technischer Nachweis für die Kette RX -> UDP -> Server -> API. |
| 2026-06-27 23:55:54 | [2026-06-27_23-55-54_four-rx-multistatic-fallbacks.png](skizzen/screenshots/2026-06-27_23-55-54_four-rx-multistatic-fallbacks.png) | Mehrere Ping-Fenster, Serverlog mit mehreren Nodes und `Multistatic fusion failed` wegen Timestamp-Spread über `60000 us` | Projektjournal: 4RX-Aufbau und Timing-/Fusion-Grenze | Belegt Mehrknotenbetrieb plus die Grenze der Standard-Fusion ohne saubere Synchronisation. |
| 2026-06-28 01:39:16 | [2026-06-28_01-39-16_g2-jumping-heatmap-multiple-poses.png](skizzen/screenshots/2026-06-28_01-39-16_g2-jumping-heatmap-multiple-poses.png) | RuView-Webvisualisierung mit mehreren grünen Feldmaxima und mehreren Pose-/Personhypothesen | `results/2026-06-28_G2_besser_verteilte_rx_qualitaetscheck.md` | Direkter Bildbeleg dafür, dass die Webansicht bei G2 qualitativ springt und nicht als exakte Positionsmessung genutzt werden sollte. |
| 2026-07-18 18:54:33 | [2026-07-18_18-54-33_fixed-room-live-sensing-failure.png](skizzen/screenshots/2026-07-18_18-54-33_fixed-room-live-sensing-failure.png) | RuView `Live WiFi Sensing` im festen 1TX-/4RX-Raumaufbau: Live-Verbindung, vier Marker, diffuse Punktwolke und `PRESENT_STILL 81 %` | `results/2026-07-18_fester-raum_live-visualisierung_diagnose.md` | Belegt den laufenden Daten-/UI-Pfad und die konkrete Fehlanzeige. Zwei Marker erscheinen nahezu überlagert; Wolke und Klassifikation waren nicht mit realer Bewegung bzw. Stillstand konsistent. Kein Positionsnachweis. |

## Protokoll-Einordnung

- Die Screenshots vom 2026-06-26 dokumentieren primär Aufbau, Serverstart, UDP-Debugging und den ersten gültigen CSI-/API-Nachweis.
- Der Screenshot vom 2026-06-27 gehört zum 4RX-Meilenstein und zur Grenze der multistatischen Fusion beim Standard-Guard-Intervall.
- Der Screenshot vom 2026-06-28 gehört zur G2-Auswertung und stützt die Beobachtung der stark springenden Webvisualisierung.
- Der Screenshot vom 2026-07-18 dokumentiert den späteren festen, vermessenen Aufbau. Reale Geometrie und Kalibrierungsversuch reichten weiterhin nicht für valide Bewegungsklasse oder Position. Die detaillierte Analyse trennt sichtbaren UI-Zustand, bestätigte Codefehler und noch offene Datenquellen-Hypothesen.
- `Display.png` wurde als Hardware-Referenz einsortiert, aber nicht als CSI-/Messprotokoll-Beleg verwendet.

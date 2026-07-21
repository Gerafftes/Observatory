# WLAN-CSI-Projekt: Berichtsdokumentation

![ESP32](https://img.shields.io/badge/ESP32-E7352C?style=flat-square&logo=espressif&logoColor=white)
[![Status](https://img.shields.io/badge/status-work%20in%20progress-orange)](#)
[![Production Ready](https://img.shields.io/badge/production%20ready-no-red)](#)

Stand: 2026-07-21

## Problemfrage

Wie zuverlässig kann ein ESP32-basiertes WLAN-CSI-System Bewegungen und Atemrhythmen im Raum erfassen, und welche physikalischen Grenzen ergeben sich dabei?

## Zweck dieses Ordners

Dieser Ordner ist nicht als Schritt-für-Schritt-Anleitung gedacht. Er sammelt Material für den späteren Bericht:

- Ziele und Annahmen
- Versuchsprotokolle
- Erfolge, Fehlschläge und Änderungen
- Messbeobachtungen
- Auswertungsgrundlagen
- Material für Diskussion und Fazit

## Hardware-Aufbau

Fünf ESP32-S3-Boards bilden die Grundlage des Aufbaus: ein TX-Board sendet, vier RX-Boards (RX-1 bis RX-4) empfangen die CSI-Rohdaten.

<img src="images/esp32-s3-boards.jpeg" alt="Fünf beschriftete ESP32-S3-Boards (RX-1, RX-2, RX-3, RX-4, TX) auf einer Schneidematte" width="480">

## Referenzsensor: HLK-LD2450 24G mmWave

Als Referenzsensor für Präsenz- und Bewegungserkennung dient ein HLK-LD2450-Modul (24 GHz mmWave-Radar) mit Antenne und Anschlusskabel.

<img src="images/hlk-ld2450-mmwave-sensor.jpeg" alt="HLK-LD2450 24G mmWave-Sensor mit Antenne und Anschlusskabel" width="480">

## Systemarchitektur (Skizze)

Geplante Erweiterung: eine zentrale Hub-PCB, an die TX- und alle RX-Boards angebunden werden, um die Verkabelung im Zielraum zu vereinfachen.

<img src="images/hub-pcb-schema.png" alt="KiCad-Schema: Hub-PCB verbindet ein TX- und vier RX-ESP32-S3-Boards" width="480">

## Ordnerstruktur

```
wifi-csi-dokumentation/
├── README.md
├── 00-status-und-annahmen.md
├── 01-projektjournal.md
├── 02-versuchslog.md
├── 03-messprotokoll.md
├── 04-auswertung-bis-problemfrage.md
├── 05-erfolge-niederlagen-und-aenderungen.md
├── 06-ruview-anpassungen.md
├── 07-screenshot-nachweise.md
├── templates/
│   └── messblatt.md
├── data/
│   ├── raw/
│   └── processed/
├── logs/
├── results/
├── images/
│   ├── esp32-s3-boards.jpeg
│   ├── hlk-ld2450-mmwave-sensor.jpeg
│   └── hub-pcb-schema.png
└── skizzen/
    └── screenshots/
```

## Aktueller Versuchsstand

- Vorhanden: 5 ESP32-S3-Boards
- Vorhanden: 1 mmWave-Modul als Referenzsensor
- Aktueller WLAN-CSI-Aufbau: 1 ESP32-TX und 4 ESP32-RX
- RX1 bis RX4 liefern Raw-CSI-Daten an den lokalen RuView-Server
- Messreihen A0 bis A3, G1 und G2 liegen als Rohdaten/CSV vor
- Screenshot-Nachweise sind unter `07-screenshot-nachweise.md` nach Zeitstempel und Inhalt einsortiert
- Die RuView-Webvisualisierung ist aktuell nur qualitativ zu verwenden; sie springt ohne Kalibrierung/Geometrie stark
- Geplant: eine Hub-PCB zur zentralen Anbindung aller Boards (siehe Skizze oben)
- Der feste Raumaufbau vom 2026-07-18 ist vermessen und in RuView eingetragen; mmWave bleibt vorerst zurückgestellt
- Der Vergleich „still sitzen“ gegen „deutliche Bewegung“ zeigte überlappende Roh-Bewegungswerte. Die aktuelle Klassifikation ist deshalb noch kein gültiges Messergebnis
- Der Screenshot des fehlgeschlagenen Live-Tests und die technische Ursachenanalyse liegen unter `07-screenshot-nachweise.md` und `results/2026-07-18_fester-raum_live-visualisierung_diagnose.md`
- Softwarebasis RX/Server: [ruvnet/RuView](https://github.com/ruvnet/RuView)

## Wichtige aktuelle Befunde

- Die 4RX-Datenerfassung funktioniert stabil genug für erste Auswertungen.
- Ein Guard-Intervall von 500 ms reduziert Fusion-Fallbacks deutlich, ist aber nur ein Visualisierungs-Workaround.
- Leerer Raum wird aktuell teilweise fälschlich als `presence=True` klassifiziert.
- Atem-/Herzfrequenzwerte sind ohne Referenzsensor noch nicht belastbar.
- Für eine echte Positionsanzeige wären Geometrie, Kalibrierung und bessere Synchronisation nötig.
- Feste Geometrie und eine leere-Raum-Kalibrierung allein haben die Trennung von Stillstand und Bewegung nicht gelöst.
- Vor weiteren Klassifikationstests muss geprüft werden, ob jeder RX ausschließlich vergleichbare CSI-Pakete des vorgesehenen TX verarbeitet.

## Screenshot des fehlgeschlagenen RuView-Livetests

[![RuView-Livetest mit fehlerhafter Punktwolke und nicht belastbarer Klassifikation](skizzen/screenshots/2026-07-18_18-54-33_fixed-room-live-sensing-failure.png)](results/2026-07-18_fester-raum_live-visualisierung_diagnose.md)

Der Screenshot zeigt den laufenden festen 1TX-/4RX-Aufbau. Obwohl RuView `PRESENT_STILL` mit `81 %` anzeigte, waren zwei Marker optisch fast überlagert, die Punktwolke reagierte nicht nachvollziehbar auf Bewegung und bewegte sich später auch beim stillen Sitzen. Das Bild ist daher ein Fehlernachweis und kein Beleg für eine korrekte Positionsbestimmung. Ein Klick auf das Bild öffnet die vollständige Diagnose.

## Dokumentationsregel

Jede relevante Beobachtung wird dokumentiert, auch wenn sie negativ ist. Gerade Fehlversuche sind wichtig, weil sie später die physikalischen und technischen Grenzen erklären.

Konkrete Arbeitsanleitungen gehören nicht in diesen Ordner, sondern werden separat im Chat oder in Arbeitsnotizen geführt.

# WLAN-CSI-Projekt: Berichtsdokumentation
[![Status](https://img.shields.io/badge/status-work%20in%20progress-orange)](#)
[![Production Ready](https://img.shields.io/badge/production%20ready-no-red)](#)

Stand: 2026-07-04

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

## Ordnerstruktur

```text
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
- Softwarebasis RX/Server: [ruvnet/RuView](https://github.com/ruvnet/RuView)

## Wichtige aktuelle Befunde

- Die 4RX-Datenerfassung funktioniert stabil genug für erste Auswertungen.
- Ein Guard-Intervall von 500 ms reduziert Fusion-Fallbacks deutlich, ist aber nur ein Visualisierungs-Workaround.
- Leerer Raum wird aktuell teilweise fälschlich als `presence=True` klassifiziert.
- Atem-/Herzfrequenzwerte sind ohne Referenzsensor noch nicht belastbar.
- Für eine echte Positionsanzeige wären Geometrie, Kalibrierung und bessere Synchronisation nötig.

## Dokumentationsregel

Jede relevante Beobachtung wird dokumentiert, auch wenn sie negativ ist. Gerade Fehlversuche sind wichtig, weil sie später die physikalischen und technischen Grenzen erklären.

Konkrete Arbeitsanleitungen gehören nicht in diesen Ordner, sondern werden separat im Chat oder in Arbeitsnotizen geführt.

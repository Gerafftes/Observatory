# WLAN-CSI-Projekt: Berichtsdokumentation

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

## Dokumentationsregel

Jede relevante Beobachtung wird dokumentiert, auch wenn sie negativ ist. Gerade Fehlversuche sind wichtig, weil sie später die physikalischen und technischen Grenzen erklären.

Konkrete Arbeitsanleitungen gehören nicht in diesen Ordner, sondern werden separat im Chat oder in Arbeitsnotizen geführt.

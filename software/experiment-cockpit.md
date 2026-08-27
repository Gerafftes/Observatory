# Experiment-Cockpit und mmWave-Kalibrierung

[English](experiment-cockpit.en.md)

Diese Seite beschreibt die UI-Ansichten und den reproduzierbaren Ablauf für
Setup-Profil, WiFi-Baseline, mmWave-Referenz, Blindaufnahmen und Evaluation.

## mmWave-Kalibrierungsassistent

Der Assistent führt durch Verbindung, Ausrichtung, Abdeckung, Zonen, Training,
Blindtest und Ergebnis. Der Screenshot zeigt den geprüften Fehlerzustand
`Server nicht erreichbar` mit HTTP 502, nicht eine verbundene Radaraufnahme.

<img src="../images/ui/mmwave-calibration-server-unreachable.png" alt="Siebenstufiger mmWave-Kalibrierungsassistent mit nicht erreichbarem Server und HTTP-502-Status" width="900">

## Experiment-Cockpit

Das Cockpit hält Setup-Profil, WiFi-Workflow, Aufnahmen und die getrennte
mmWave-Referenz in einer Ansicht. Die Aufnahmen stammen aus einem simulierten
Lauf ohne angeschlossene Sensoren.

<img src="../images/ui/experiment-cockpit-setup.png" alt="Experiment-Cockpit mit Setup-Profil, Statusübersicht und simuliertem Hardwarezustand" width="900">

<img src="../images/ui/experiment-cockpit-guide.png" alt="Experiment-Cockpit mit Raum-, TX- und RX-Positionen sowie wartender mmWave-Referenz" width="900">

<img src="../images/ui/experiment-cockpit-workflow-guide.png" alt="Experiment-Cockpit mit Workflow-Guide und zehn gesperrten beziehungsweise freigeschalteten Phasen" width="900">

## Kurzanleitung

1. **Setup-Profil öffnen:** Raummaße (Länge/Höhe/Breite), TX-Position und
   RX1–RX4-Positionen eintragen, danach **Neue Profilversion speichern**.
2. **Experiment-Run anlegen:** Versuchsname und gespeichertes Profil wählen,
   dann **WiFi-Experiment anlegen**.
3. **Setup versiegeln:** Der Guide speichert Profil und Hash für diesen Run.
   Das ist nur ein Software-Schritt.
4. **Leere WiFi-Baseline:** Raum leer halten, Leerkalibrierung starten und
   abschließen. Ohne CSI-Nodes stoppt der Ablauf bei **zu wenigen
   RX-Fingerprints**.
5. **mmWave-Kalibrierung:** Der Radar liefert separat Positionspakete,
   Abdeckung und CSI-Zeitbezug. Ohne Sensor bleibt der Status **Wartet auf
   mmWave**.
6. **Blindtest:** Eine reproduzierbare Reihenfolge erzeugen und neue CSI-
   Aufnahmen ohne Ground Truth sammeln. Ohne RX-Nodes gibt es keine gültigen
   Aufnahmen; der Software-Demo-Run bleibt nur eine Ansicht.
7. **Prediction und Truth trennen:** Zuerst nur die WiFi-Prediction
   registrieren, danach die getrennte Radar-/Positionswahrheit aufdecken.
8. **Evaluation und Report:** Accuracy, Coverage, Fehlerdistanz,
   Confusion Matrix und Qualitätsgates auswerten und anschließend den Report
   schreiben. Ohne Hardware bleibt er ausdrücklich
   **SOFTWARE-ONLY / UNVALIDATED** und beweist keine echte Messqualität.

> [!NOTE]
> Die Screenshots und der Demo-Ablauf dokumentieren Softwarezustände. Sie sind kein Nachweis eines verbundenen Radars, gültiger CSI-Aufnahmen oder realer Positionsgenauigkeit.

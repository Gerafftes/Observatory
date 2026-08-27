# Wie Observatory funktioniert

[English](architecture.en.md)

WLAN-Signale verändern sich durch Reflexion, Abschattung und Multipath. Ein
TX-Board erzeugt den kontrollierten Funkverkehr; RX1 bis RX4 messen die
komplexen CSI-Werte aus verschiedenen Raumpositionen. Observatory vergleicht
diese Messungen mit einer aufbaugebundenen Leerraumreferenz.

Die Positionsbestimmung ist absichtlich diskret. Statt zwischen ungemessenen
Koordinaten zu interpolieren, lernt D6 Fingerprints für neun markierte
Bodenpunkte. Reicht die Evidenz nicht aus oder passen mehrere Punkte ähnlich
gut, muss das System `unknown` oder `ambiguous` ausgeben.

Der mmWave-Sensor dient nur als unabhängige Referenz für Kalibrierung und
Blindbewertung. Dadurch wird verhindert, dass der WLAN-CSI-Prädiktor während
des Tests indirekt die richtige Antwort erhält.

```text
physischer Aufbau
→ Setup-Siegel
→ 25-s-Preflight
→ 65-s-Leerraumkalibrierung
→ P01–P09-Training
→ Positionsindex
→ Blindtests
→ gemeinsame Qualitätsgates
→ Live-Anzeige
```

Die Implementierungsdetails stehen im [Software-Überblick](software/README.md);
die reproduzierbaren UI-Schritte sind im
[Experiment-Cockpit-Guide](software/experiment-cockpit.md) beschrieben.

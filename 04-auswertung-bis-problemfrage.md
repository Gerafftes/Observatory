# 04 Auswertung bis zur Beantwortung der Problemfrage

## Problemfrage

Wie zuverlässig kann ein ESP32-basiertes WLAN-CSI-System Bewegungen und Atemrhythmen im Raum erfassen, und welche physikalischen Grenzen ergeben sich dabei?

## Auswertungsteil 1: Bewegung

Fragestellung:

```text
Kann das System unterscheiden zwischen leerem Raum, stiller Person und Bewegung?
```

Metriken:

- Trefferquote Bewegung erkannt / Bewegung vorhanden
- False Positives: Bewegung erkannt, obwohl keine Bewegung da war
- False Negatives: keine Bewegung erkannt, obwohl Bewegung da war
- Reaktion der einzelnen RX-Nodes im Vergleich

Erster Rohbefund vom 2026-06-26:

- `presence=True` war mit höheren Feature-Werten verbunden als `presence=False`.
- Mittelwerte aus der ersten unsortierten Beobachtung: Varianz ca. 32,8 statt 26,0; Motion-Power ca. 49,3 statt 38,4; Breath-Power ca. 53,0 statt 40,0.
- Bei starkem Signalbereich ab etwa `RSSI >= -65 dBm` wurde in der Stichprobe durchgehend Präsenz erkannt.
- Bei schwächerem Link um etwa `-80 bis -90 dBm` flackerte die Klassifikation deutlich.

Einordnung:

Diese Werte sind noch keine Trefferquote, weil die reale Situation nicht sauber gelabelt wurde. Sie zeigen aber, dass Bewegung/Anwesenheit im Signal sichtbar ist und dass Linkqualität ein zentraler Störfaktor ist.

Pilotmessung A0/A1/A2 vom 2026-06-26:

- Eine erste gelabelte Messung mit nur einem RX zeigte noch keine robuste Trennung der Phasen.
- Im ungefähr markierten Leerraum-Abschnitt wurde `presence=True` in ca. 81% der Zeilen ausgegeben. Das ist eine hohe False-Positive-Rate.
- Ruhiges Stehen lag bei ca. 92% `presence=True`, langsames Gehen bei ca. 78% `presence=True`.
- Die mittleren Feature-Werte der drei Phasen lagen relativ nah beieinander. Das spricht dafür, dass die aktuelle Einzel-Link-Geometrie und Linkqualität noch nicht ausreichen, um zuverlässig zu klassifizieren.

Interpretation:

Der Aufbau reagiert auf das Funksignal, aber die aktuelle Klassifikation ist bei schwachem Link um etwa -80 dBm nicht belastbar. Für die Problemfrage ist das ein wichtiger physikalisch-praktischer Grenzbefund: Ein einzelner Link kann zwar Änderungen zeigen, liefert aber ohne gute Geometrie/Kalibrierung viele Fehlalarme.

## Auswertungsteil 2: Atmung

Fragestellung:

```text
Kann ruhige Atmung als periodische Veränderung im CSI erkannt werden?
```

Metriken:

- geschätzte Atemfrequenz in Atemzügen/min
- Referenzwert mmWave/Fitnessuhr/manuelle Zählung
- absoluter Fehler in Atemzügen/min
- Signalqualität/Confidence

Wichtig:

Atmung nur bei ruhiger Person bewerten. Bewegung überlagert das Atemsignal stark.

## Auswertungsteil 3: Raumposition / Zonen

Fragestellung:

```text
Reagieren RX1, RX2 und RX3 unterschiedlich genug, um grobe Raumzonen zu unterscheiden?
```

Metriken:

- mittlere Signaländerung pro Zone
- Verwechslung zwischen links / Mitte / rechts
- Reaktion pro Funklink TX→RX1, TX→RX2, TX→RX3

## Auswertungsteil 4: Versuch A gegen Versuch B

Später wird Versuch B ergänzt:

```text
Versuch A: eigener ESP32-TX
Versuch B: normaler WLAN-Router als Sender
```

Vergleich:

- Paketstabilität
- Signalqualität
- Bewegungserkennung
- Atemfrequenzfehler
- Störanfälligkeit

## Physikalische Grenzen, die geprüft werden

- Abstand zwischen TX/RX und Person
- ungünstige Position außerhalb starker Funkpfade
- Mehrwegeausbreitung durch Wände/Möbel
- Bewegung überlagert Atmung
- mehrere Personen überlagern sich
- instabile Phase durch Hardware-/Clock-Effekte
- Raumwechsel erfordert neue Kalibrierung

## Kriterien für eine belastbare Antwort

Die Problemfrage kann beantwortet werden, wenn mindestens diese Daten vorliegen:

- mehrere Leerraum-Messungen
- mehrere Bewegungsmessungen
- mehrere Atemmessungen mit Referenz
- mindestens drei RX-Links
- später ideal: vier RX-Links
- Vergleich kontrollierter TX gegen Router/AP

## Erwartete Kernaussage

Voraussichtlich wird die Antwort nicht lauten:

```text
WLAN kann Menschen exakt und zuverlässig wie Radar erfassen.
```

Sondern eher:

```text
WLAN-CSI kann Bewegung und ruhige Atmung unter kontrollierten Bedingungen sichtbar machen. Die Zuverlässigkeit hängt stark von Position, Funkpfaden, Raum, Bewegung und Anzahl der Personen ab. Für robuste Ortung und Atemmessung sind mehrere Empfänger, Kalibrierung und Referenz-/Qualitätsbewertung nötig.
```

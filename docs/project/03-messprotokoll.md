# 03 Messprotokoll – öffentliche Methodik

Historische Versuchsplanung mit Stand 9. August 2026. Die folgenden Sollregeln sind keine Behauptung, dass alle Messreihen durchgeführt wurden. Ergebnisse stehen im [technischen Bericht](../../experiment-reports/2026-08-23_D4-D5-D6_technischer-bericht/README.md).

## Zweck und Dokumentation

Problemfrage: Wie zuverlässig kann ein WLAN-CSI-System Bewegungen und Atemrhythmen im Raum erfassen?

Vor jedem Lauf werden Datum/Uhrzeit, Raumskizze, TX-/RX-/Referenzgeometrie, Abstände, Kanal, logische Node-Rollen, Filterzustand und parallele Referenzmessung festgehalten. Währenddessen werden Start/Ende, Bewegungen, Störungen, Ausfälle und aktive RX notiert. Private Netzwerk- und Gerätekennungen gehören nicht in den öffentlichen Bericht.

## Frühe geplante Reihen

| Reihe | Sollumfang | Situation / Referenz |
|---|---:|---|
| A0 | 60 s | leerer Raum |
| A1 | 60 s | ruhiges Stehen, manuelles Label |
| A2 | 60 s | langsames Gehen, manuelles Label und Referenzsensor |
| A3 | 120 s | ruhiges Sitzen/Atmen, unabhängige Atemreferenz |
| A4-L/M/R | je 60 s | links / Mitte / rechts, manuelle Labels |
| G1 | 60 s | langsame Bewegung mit vier RX, Guard-Vergleich |

Die Sollwerte ersetzen nicht die tatsächlich aufgezeichneten Dauern; das Journal nennt für die frühe A3-Aufnahme 180 s.

Nach jedem Lauf: vollständige RX-Abdeckung, plausible Signalqualität und Referenzwerte prüfen. Bei fehlendem RX, Neustart, fehlendem UDP-Empfang, starker Bewegung während Atemmessung oder ausgefallener Referenz wird der Lauf als ungültig protokolliert und wiederholt. Ein größeres Guard-Intervall bleibt ein Visualisierungs-Workaround und ersetzt keine Knotensynchronisation.

## Tatsächlicher D5-Livetest vom 26. Juli 2026

| Lauf | Dauer | Ergebnis |
|---|---:|---|
| still sitzend | 59,7 s | 236/236 global ABSENT |
| Still-Persistenz | 29,9 s | 114/114 global ABSENT |

Alle vier RX, Referenzen und laufende Evidenz waren vorhanden; die Logger meldeten keine Fehler. Die Person war manuell als still sitzend gelabelt. Der Positivtest ist nicht bestanden. Ein separater blinder Leerraumlauf unter derselben Kalibrierung fehlte; daraus folgt keine Leerraum-FPR von null.

## Geplanter D6-/9-Punkt-Blindtest

Am 9. August war die vollständige Messserie noch nicht durchgeführt. Nach korrigiertem Sidecar und neu gebundener Setupserie bestanden ein ungefähr 25-s-Preflight (2.701 Frames) und eine ungefähr 65-s-Leerraumaufnahme (6.102 Frames) mit vier RX und null Recorder-Drops. Frühere Kalibrierungen mit anderem Laptopstandort oder anderer Setupserie dürfen nicht übernommen werden. Das nächste damalige Messgate war P01-Training.

### Aufnahmeumfang

| Teil | Anzahl | Mindestdauer je Lauf | Inhalt |
|---|---:|---:|---|
| Versiegelter Setup-Preflight | 1 | 25 s | Raum ohne Person und ohne Bewegung |
| D6-Leerraumkalibrierung | 1 | 65 s | Raum leer |
| Positionstraining | 9 | 35 s | je P01 bis P09 |
| Leerraumprüfung | 3 | 35 s | Raum leer |
| Positions-Blindtest | 18 | 35 s | zwei zufällige Durchgänge über P01 bis P09 |

Die ersten fünf Sekunden jedes Positions-/Prüflaufs dienen ausschließlich dem
Einpendeln. Alle Blindaufnahmen erhalten neutrale IDs. Punktlabel und Wahrheit
werden separat gespeichert und erst nach der Vorhersage zusammengeführt.

### Vorherige Qualitätsprüfung

- identische Setup-ID und identischer Setup-Hash
- RX1 bis RX4 vollständig
- gültige Datenrate, Zeitabdeckung, Lücken und Subcarrier-Raster
- kein Label und keine Ground Truth in Raw-Datei oder Sidecar
- keine identischen Roh-, Sidecar- oder Signal-Hashes zwischen Kalibrierung,
  Training und Blindtest

Strukturell ungültige Läufe werden vor der Vorhersage mit neuer neutraler ID
wiederholt und mit Abbruchgrund protokolliert. Nach Öffnen der Truth-Datei
werden keine Läufe ersetzt.

### Bestehensgrenzen

Classification:

- Leerraum-Fehlpräsenz aggregiert höchstens 5 %
- kein einzelner Leerraumlauf über 10 %
- bestätigte Präsenz in mindestens 16 von 18 belegten Blindläufen
- belegter Recall aggregiert mindestens 80 %
- kein Punkt in beiden Wiederholungen vollständig übersehen

Position:

- Coverage mindestens 16 von 18
- mindestens 15 von 18 Läufen exakt richtig
- Accuracy der entschiedenen Läufe mindestens 90 %
- je Punkt mindestens eine Wiederholung korrekt
- höchstens zwei Abstentionsausgaben
- Median-Bodenfehler 0,0 m
- p95 und jeder falsche entschiedene Punkt höchstens 1,30 m vom Sollpunkt

Nur wenn Classification und Position beide bestehen, darf der Positionsindex
im Live-Sensing aktiviert werden.

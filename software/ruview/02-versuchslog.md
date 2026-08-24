# 02 Versuchslog

Hier werden einzelne Messläufe dokumentiert. Ziel ist nicht perfekte Formulierung, sondern lückenlose Nachvollziehbarkeit.

## Übersicht

| Mess-ID | Datum | Aufbau | Situation | Ergebnis kurz | Status |
|---|---|---|---|---|---|
| A0 | 2026-06-26 | 1 TX, 1 RX | erster Raw-CSI-Lauf / wechselnde Anwesenheit | laufender `tick`, RSSI- und Feature-Werte sichtbar | erster Rohbefund |
| A1 | offen | 1 TX, 3 RX, mmWave | Person steht ruhig | offen | geplant |
| A2 | offen | 1 TX, 3 RX, mmWave | Bewegung | offen | geplant |
| A3 | offen | 1 TX, 3 RX, mmWave | ruhige Atmung | offen | geplant |
| A4 | offen | 1 TX, 3 RX, mmWave | Zonen links/Mitte/rechts | offen | geplant |
| G3 | 2026-07-18 | 1 TX, 4 RX, feste Geometrie, ohne mmWave | leer kalibriert, danach still sitzen und bewegen | Roh-Bewegungswerte überlappen; Live-Klasse und Wolke nicht valide | fehlgeschlagen, diagnostisch wertvoll |

## Detailprotokolle

### Mess-ID: A0

**Situation**

Leerer Raum.

**Ziel**

Grundrauschen und Stabilität der CSI-Daten erfassen.

**Beobachtungen**

- Raw-CSI-Stream läuft nach Entfernen des MAC-Filters.
- API-Monitor zeigt steigenden `tick` und wechselnde Feature-Werte.
- In der ersten Beobachtung lagen `presence=True`-Werte im Mittel höher als `presence=False`-Werte: Varianz ca. 32,8 statt 26,0; Motion-Power ca. 49,3 statt 38,4; Breath-Power ca. 53,0 statt 40,0.
- Starkes Signal (`RSSI` ca. -53 bis -63 dBm) wurde zuverlässig als Präsenz erkannt; schwaches Signal (`RSSI` ca. -80 bis -90 dBm) zeigte flackernde Klassifikation.

**Probleme**

- Die Situation war noch keine sauber getrennte Messreihe mit definierten Phasen.
- Klassifikation flackert bei schwachem Link zwischen `presence=True` und `presence=False`.
- Atem-/Herzfrequenz ist noch nicht belastbar bewertet.

**Zwischenergebnis**

- Die Signalreaktion ist sichtbar, aber für belastbare Aussagen müssen jetzt kontrollierte Phasen gemessen werden: leerer Raum, Bewegung, ruhiges Stehen/Sitzen, Atmung mit Referenz.

### Mess-ID: A0/A1/A2 Pilot — 2026-06-26

**Situation**

Erste gelabelte Pilotmessung mit einem TX und einem RX. Die Phasen wurden während des laufenden Terminal-Monitors markiert: leerer Raum, ruhig stehen, langsam gehen.

**Ziel**

Prüfen, ob sich die API-Features zwischen leerem Raum, ruhigem Stehen und langsamer Bewegung sichtbar unterscheiden.

**Beobachtungen**

- Der Raw-CSI-Stream lief stabil weiter; `tick` stieg über den gesamten Mitschnitt.
- Die Labels wurden teilweise in die Terminalausgabe hineingeschrieben. Die Phasengrenzen sind deshalb nur ungefähr.
- Grobe Auswertung:
  - leerer Raum, ca. Tick 237–784: `presence=True` in ca. 81% der Zeilen.
  - ruhig stehen, ca. Tick 829–1592: `presence=True` in ca. 92% der Zeilen.
  - langsam gehen, ca. Tick 1600–1993: `presence=True` in ca. 78% der Zeilen.
- Mittlere Feature-Werte:
  - leerer Raum: Varianz ca. 24,4; Motion-Power ca. 36,4; Breath-Power ca. 49,8.
  - ruhig stehen: Varianz ca. 26,1; Motion-Power ca. 39,6; Breath-Power ca. 51,3.
  - langsam gehen: Varianz ca. 23,2; Motion-Power ca. 35,7; Breath-Power ca. 47,6.

**Probleme**

- Der Leerraum wurde zu häufig als Präsenz erkannt. Damit ist die aktuelle Klassifikation für Trefferquoten noch nicht belastbar.
- Bewegung war in dieser Pilotmessung nicht klarer als ruhiges Stehen bzw. leerer Raum separierbar.
- Die Linkqualität war überwiegend schwach (`RSSI` meist etwa -80 dBm). Das begünstigt flackernde Klassifikationen.
- Die Labels wurden nicht sauber als eigene Spalte gespeichert.

**Zwischenergebnis**

Die technische Messkette funktioniert, aber die aktuelle räumliche Anordnung ist für belastbare Bewegungserkennung noch ungünstig. Für die nächste Messung sollten TX/RX/Person kontrollierter und mit stärkerem Link positioniert werden, bevor Trefferquoten berechnet werden.

### Mess-ID: A1

**Situation**

Eine Person steht ruhig im Testbereich.

**Ziel**

Prüfen, ob Anwesenheit gegenüber leerem Raum sichtbar wird.

**Beobachtungen**

-

**Probleme**

-

**Zwischenergebnis**

-

### Mess-ID: A2

**Situation**

Eine Person bewegt sich langsam durch den Testbereich.

**Ziel**

Prüfen, ob Bewegung robust sichtbar wird.

**Beobachtungen**

-

**Probleme**

-

**Zwischenergebnis**

-

### Mess-ID: A3

**Situation**

Eine Person sitzt ruhig und atmet normal.

**Ziel**

Prüfen, ob ein periodischer Atemrhythmus aus CSI ableitbar ist.

**Referenz**

mmWave / Fitnessuhr / manuelle Zählung.

**Beobachtungen**

-

**Probleme**

-

**Zwischenergebnis**

-

### Mess-ID: A4

**Situation**

Person steht in unterschiedlichen Raumzonen.

**Ziel**

Prüfen, ob RX1, RX2 und RX3 unterschiedliche Signaturen je Zone liefern.

**Beobachtungen**

-

**Probleme**

-

**Zwischenergebnis**

-

### Mess-ID: G3 — Fester Raumaufbau und Live-Visualisierung

**Situation**

Ein TX und vier RX waren fest im vermessenen Raum aufgebaut. Nach einer leeren-Raum-Kalibrierung saß eine Person still und bewegte anschließend Arme und Oberkörper deutlich. Das mmWave-Modul war nicht beteiligt.

**Ziel**

Prüfen, ob die RuView-Punktwolke in Ruhe stabil bleibt, bei Bewegung reagiert und die globale Klasse zwischen `present_still` und `present_moving` korrekt wechselt.

**Beobachtungen**

- Alle vier RX waren in der Live-API aktiv.
- Der Screenshot zeigt `PRESENT_STILL` mit `81 %`, gleichzeitig aber eine nicht validierte, diffuse räumliche Wolke und zwei optisch fast überlagerte Marker.
- Beim stillen Sitzen meldete die globale API in einer Stichprobe fünfmal `present_still` und einmal fälschlich `present_moving`.
- Die vier per-RX-Klassen meldeten zeitweise alle `present_moving` mit ungefähr `40 %`, obwohl die Person still saß.
- Nach mehreren lokalen Fehlerkorrekturen blieben die Rohscores stark überlappend:
  - still: je nach RX ungefähr `0,048` bis `0,894`
  - deutliche Bewegung: je nach RX ungefähr `0,059` bis `0,851`

**Probleme**

- Die globale Klassifikation war ursprünglich vom zuletzt eingetroffenen RX abhängig.
- Ein adaptives Modell mit nur `41,5 %` Trainingsgenauigkeit überschrieb die heuristische Klasse.
- Der Bewegungsalgorithmus verglich den aktuellen Frame zunächst mit sich selbst und nutzte zusätzlich statische, in diesem Aufbau saturierte Merkmale.
- Selbst nach Behebung dieser Fehler war im bisherigen Frame-zu-Frame-Score keine robuste Trennung zwischen still und bewegt sichtbar.
- Die leere-Raum-Kalibrierung ging durch notwendige Server-Neustarts verloren und muss nach Bereinigung des Datenstroms wiederholt werden.

**Zwischenergebnis**

G3 ist kein erfolgreicher Ortungsnachweis. Der Test belegt jedoch, dass die nächste Priorität vor UI-Tuning und neuer Kalibrierung bei der Vergleichbarkeit der CSI-Pakete liegt. Ein Absender-/Paket-/Rasterfilter in der RX-Pipeline muss zuerst geprüft werden.

**Nachweis**

- [Screenshot](skizzen/screenshots/2026-07-18_18-54-33_fixed-room-live-sensing-failure.png)
- [Ausführliche Diagnose](results/2026-07-18_fester-raum_live-visualisierung_diagnose.md)

### Mess-ID: D5-Live — Reale Leerraumkalibrierung und still sitzende Person

**Situation**

Nach erfolgreicher realer D5-Leerraumkalibrierung saß eine Person zunächst 59,7 Sekunden und anschließend weitere 29,9 Sekunden still. Mac, TX, RX und Serverparameter blieben unverändert.

**Ziel**

Prüfen, ob die im Offline-Replay erfolgreiche D5-Regel mit per-RX-Referenzen und Zwei-RX-Quorum eine neue reale still sitzende Person erkennt.

**Beobachtungen**

- Beide Aufnahmen enthielten durchgehend RX1 bis RX4; die Logger meldeten keine Fehler.
- Alle vier RX hatten eine gültige D5-Referenz und aktuelle Evidenz.
- Global wurden alle 350 Samples als `ABSENT` ausgegeben.
- Im ersten Abschnitt stimmte RX4 in 87 von 236 Samples für Präsenz; kein anderer RX stimmte zu.
- Im zweiten Abschnitt stimmte RX3 in allen 114 Samples; RX2 stimmte nur einmal zu.

**Probleme**

- Das geforderte Zwei-RX-Quorum kam nie zustande.
- Der informative Funkpfad wechselte zwischen den beiden direkt aufeinanderfolgenden Abschnitten.
- Für dieselbe neue Kalibrierung fehlt noch ein separat aufgezeichneter blinder Leerraumlauf.

**Zwischenergebnis**

Der reale D5-Positivtest ist mit 0,0 % Still-Recall nicht bestanden. D5 bleibt deaktiviert. Vor einer Änderung an Quorum oder Schwelle ist eine gepaarte blinde Serie aus Leerraum und mehreren Still-Positionen nötig.

**Nachweis**

- [Ausführliche Auswertung](results/2026-07-26_D5_realer-still-livetest.md)

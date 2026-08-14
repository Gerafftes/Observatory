# 08 Aktueller Arbeitsstand: D6 und Positionsbestimmung

Stand: 2026-08-14

Dieses Dokument ist der verbindliche Wiedereinstiegspunkt für die laufende
Arbeit an Classification und Positionsbestimmung. Es trennt bewusst zwischen:

- bereits implementiert
- automatisiert geprüft
- noch nicht mit dem realen 1TX-/4RX-Aufbau bewiesen

## Aktuelles Ziel

Der feste Raumaufbau soll zwei getrennte Aussagen liefern:

1. `ABSENT` oder Person anwesend
2. falls eine Person anwesend ist: welcher von neun vorher festgelegten
   Bodenpunkten passt am besten

Eine beliebig genaue, kontinuierliche Raumkoordinate wird nicht behauptet.
Wenn die Daten nicht ausreichen oder zu mehreren Punkten passen, muss das
System `unknown` beziehungsweise `ambiguous` ausgeben und darf keine
Personposition erfinden.

## Warum die bisherige Heatmap nicht genügt

Die bisherige Sensing-Heatmap war keine gemessene Personenposition. Sie
verwendete eine geometrische beziehungsweise visuelle Näherung und erzeugte
dadurch eine scheinbar räumliche Wolke, obwohl aus den CSI-Daten keine
eindeutige Position bestimmt worden war.

Folgen im bisherigen Livetest:

- Die Wolke blieb bei echter Bewegung nahezu am selben Ort.
- Beim stillen Sitzen bewegte sie sich trotzdem.
- Die angezeigte Classification und die sichtbare Position konnten einander
  widersprechen.

Die Heatmap darf deshalb höchstens als Diagnosedarstellung gelten. Für ESP32-
Live-Daten wird eine Person nur dargestellt, wenn eine separat geprüfte
Lokalisierung tatsächlich eine Position liefert.

## Fester Aufbau

Raum:

- Länge: `4,02 m`
- Breite: `3,44 m`
- Höhe: `2,59 m`

RuView-Koordinaten sind `[x=Länge, y=Höhe, z=Breite]`. Die untere linke Ecke
der festgelegten Draufsicht ist `(x=0, z=0)`.

| Gerät | Koordinate `[x, y, z]` |
|---|---|
| TX | `[1.51, 1.19, 0.39]` |
| RX1 | `[0.00, 0.50, 0.28]` |
| RX2 | `[4.02, 0.87, 0.97]` |
| RX3 | `[0.00, 0.74, 2.11]` |
| RX4 | `[4.02, 0.87, 2.46]` |

Geplante neun Messpunkte auf dem Boden:

| Punkt | Koordinate `[x, y, z]` |
|---|---|
| P01 | `[0.75, 0.00, 0.75]` |
| P02 | `[2.01, 0.00, 0.75]` |
| P03 | `[3.27, 0.00, 0.75]` |
| P04 | `[0.75, 0.00, 1.72]` |
| P05 | `[2.01, 0.00, 1.72]` |
| P06 | `[3.27, 0.00, 1.72]` |
| P07 | `[0.75, 0.00, 2.69]` |
| P08 | `[2.01, 0.00, 2.69]` |
| P09 | `[3.27, 0.00, 2.69]` |

## Gewählter Ansatz

Verwendet wird eine diskrete Fingerprint-Klassifikation:

1. Eine Leerraumaufnahme erzeugt für jeden RX und jedes gültige
   Subcarrier-Raster eine robuste D6-Referenz.
2. Von jedem späteren Frame wird die signierte Abweichung zur
   Leerraumreferenz gebildet. Positive und negative Änderungen gehen ein.
3. Drei-Sekunden-Fenster werden zu 28 Merkmalen je RX zusammengefasst.
4. Alle vier RX werden gleichberechtigt verwendet: `4 × 28` Merkmale.
5. Für jeden der neun Punkte wird aus sechs unabhängigen Fünf-Sekunden-Blöcken
   ein robuster Prototyp gelernt.
6. Eine Vorhersage liefert nur einen der festen Punkte, `unknown` oder
   `ambiguous`.
7. Eine zeitlich stabile Position erfordert Zustimmung in mindestens vier der
   letzten fünf Fenster.

Warum dieser Ansatz:

- Er nutzt reale, am konkreten Raum gemessene Signalmuster.
- Er behauptet keine Genauigkeit zwischen ungemessenen Punkten.
- Er kann Unsicherheit sichtbar machen.
- Training und blinde Prüfung können strikt voneinander getrennt werden.

## Bereits implementiert

Die aktuelle lokale RuView-Arbeit umfasst:

### Classification und Leerraumreferenz

- skaleninvariante D4-Bewegungsmerkmale
- D6-Fingerprint aus gain-normalisierter CSI-Form
- robuste Leerraumreferenzen mit stabilen Subcarrier-Masken
- signierte Residuen; Abweichungen in beide Richtungen bleiben erhalten
- getrennte Präsenz- und Positionslogik

### Verlustfreie Rohdatenerfassung

- Raw-CSI-JSONL mit unveränderten I/Q-Werten
- Sidecar-Metadaten mit Aufnahme-, Aufbau-, Geometrie- und Serverbindung
- strikte Schemas, die unbekannte Felder und versteckte Positionslabels
  ablehnen
- kryptografische Hashes für Rohdatei, Sidecar und den verwendeten
  Signalabschnitt
- atomisches Schreiben ohne Überschreiben vorhandener Ergebnisdateien

### Positionsmerkmale und Modell

- feste vier RX und exakt 28 Merkmale pro RX
- fünf Sekunden Einpendelzeit
- drei Sekunden Fensterlänge
- eine Sekunde Schrittweite
- mindestens 5 Hz und mindestens 15 Frames je RX und Fenster
- Prüfung von Lücken, gemeinsamer Abdeckung und identischem
  Subcarrier-Raster
- genau neun eindeutige Bodenpunkte
- robuste Median-Prototypen und gemeinsame diagonale Skalierung
- OOD-Prüfung für unbekannte Signale
- Abstandsmarge für mehrdeutige Signale

### Blinde Auswertung

- Trainingsmanifest ist die einzige Datei, die Punktlabels und lokale Pfade
  enthalten darf.
- Blinde Rohaufnahmen enthalten keine Wahrheit.
- Die Vorhersage akzeptiert kein Truth-Manifest.
- Die Wahrheit wird erst in einem getrennten Auswertungsschritt gelesen.
- Roh-, Metadaten- und Signal-Hashes verhindern versehentliche
  Wiederverwendung von Trainingsdaten als Blindtest.
- Ergebnisbericht enthält Coverage, Accuracy, Distanzfehler und
  `9 × 9`-Konfusionsmatrix.

### UI-Sicherheitsverhalten

- Bei fehlender oder veralteter ESP32-Evidenz werden Classification,
  Persondarstellung und Position geleert.
- Eine ESP32-Person wird nicht aus einer künstlichen Feldspitze erzeugt.
- Die statischen TX-/RX-Marker bleiben sichtbar, weil sie den Aufbau und keine
  erkannte Person darstellen.

## Offline-Werkzeuge

Der Server besitzt vier voneinander getrennte Offline-Modi:

```text
--position-inspect <empty-calibration|position>
--position-build-index <TRAINING_MANIFEST>
--position-predict <POSITION_INDEX>
--position-evaluate <PREDICTIONS>
```

Zusätzliche Eingaben:

```text
--position-capture <RAW_CAPTURE>
--position-truth <TRUTH_MANIFEST>
--position-output <NEW_OUTPUT_FILE>
```

Wichtige Regeln:

- `--position-inspect` erzeugt die manifestfähigen Hashes der Aufnahmen.
- `--position-build-index` liest Leerraum- und gelabelte
  Trainingsaufnahmen.
- `--position-predict` liest nur Index und ungelabelte Blindaufnahmen.
- `--position-evaluate` vergleicht danach Vorhersagen mit der separat
  gespeicherten Wahrheit.
- Jeder Modus schreibt eine neue Datei und überschreibt keine vorhandene
  Datei.

## Noch nicht bewiesen

Die folgenden Punkte sind ausdrücklich offen:

- Es existieren noch keine realen Aufnahmen für P01 bis P09.
- Das Modell besitzt deshalb noch keinen real gemessenen Positionsindex.
- Classification und Position sind noch nicht gemeinsam im Live-Server
  freigegeben.
- Der Sensing-Tab kann eine validierte diskrete Fingerprint-Position technisch
  anzeigen; ohne bestandenen realen Index bleibt dieser Pfad jedoch
  `uncalibrated` und zeigt keine Person.
- Die reale Genauigkeit, Coverage und Wiederholbarkeit sind unbekannt.

Die neue Pipeline ist damit derzeit ein offline prüfbarer Prototyp und noch
kein erfolgreich validiertes Ortungssystem.

## Automatisierter Prüfstand vom 2026-07-29

Bestanden:

- echter dateibasierter Pipeline-Test mit:
  - `1 × 65 s` synthetischem Leerraum
  - `9 × 35 s` synthetischen Trainingsaufnahmen
  - `9 × 35 s` davon getrennten synthetischen Blindaufnahmen
  - vier RX, 5 Hz und 64 CSI-Bins
- vollständiger Weg:
  `inspect → build-index → predict → evaluate`
- Trainings- und Blindsignale besitzen getrennte Signal-Hashes
- blinde Rohdateien enthalten weder Label noch Ground Truth
- synthetisches Ergebnis: `9/9` korrekt, Coverage `1,0`, Accuracy `1,0`,
  Median- und p95-Fehler `0,0 m`
- vollständige Sensing-Server-Tests: `312 bestanden`, `0 fehlgeschlagen`
- Debug-Build des echten `sensing-server`-Binaries erfolgreich
- tatsächliche CLI-Hilfe enthält alle vier Positionsmodi
- ungültige CLI-Aufrufe brechen mit Exit-Code `2` ab und schreiben keine Datei
- Sensing-UI-Lokalisierungstest bestanden
- Rustfmt-Prüfung und `git diff --check` ohne Fehler

Einordnung:

Der Test beweist, dass Dateien, Schemas, Hashbindungen, Merkmalsextraktion,
Modell, Blindheit und Auswertung technisch zusammenarbeiten. Er beweist nicht,
dass sich reale CSI-Fingerprints an P01 bis P09 ausreichend unterscheiden.

## Vorab festgelegte Gütegrenzen für die reale Prüfung

Diese Grenzen werden vor der ersten P01-Aufnahme festgeschrieben. Sie dürfen
nicht nach Sichtung der Blindvorhersagen gelockert werden.

### Umfang

- eine Leerraumkalibrierung mit mindestens 65 Sekunden
- drei neue Leerraum-Prüfaufnahmen mit je mindestens 35 Sekunden
- eine Trainingsaufnahme je P01 bis P09
- zwei voneinander getrennte Blindaufnahmen je P01 bis P09
- insgesamt 18 belegte Blindpositionen in zwei zufälligen Durchgängen

### Datenqualität vor dem Entschlüsseln der Wahrheit

Jede verwendete Aufnahme muss:

- exakt dieselbe Setup-ID und denselben Setup-Hash tragen
- RX1 bis RX4 mit gültigem, identischem Raster enthalten
- Dauer, Datenrate, Lücken- und Fensterregeln erfüllen
- ungelabelte Rohdaten und einen ungelabelten Sidecar besitzen
- von Kalibrier-, Trainings- und allen anderen Blindaufnahmen verschiedene
  Roh-, Sidecar- und Signal-Hashes besitzen

Eine strukturell ungültige Aufnahme darf vor der Vorhersage mit neuer neutraler
ID wiederholt werden. Der Abbruchgrund wird protokolliert. Nach Öffnen der
Truth-Datei werden keine Läufe mehr ersetzt.

### Classification-Gate

Für die nach der Einpendelzeit ausgewerteten Abschnitte:

- aggregierte Leerraum-Fehlpräsenz höchstens `5 %`
- keine der drei Leerraumaufnahmen über `10 %` Fehlpräsenz
- bestätigte Präsenz in mindestens `16/18` belegten Blindaufnahmen
- aggregierter Occupied-Recall mindestens `80 %`
- kein Punkt darf in beiden Blindwiederholungen vollständig als leer
  übersehen werden

### Positions-Gate

Alle 18 tatsächlich belegten Blindaufnahmen werden unabhängig vom
Classification-Ergebnis räumlich bewertet. Dadurch kann eine fehlerhafte
Presence-Ausgabe keine Positionsfehler aus der Auswertung entfernen:

- Coverage mindestens `16/18 = 88,9 %`
- Accuracy über alle 18 Läufe mindestens `15/18 = 83,3 %`
- Accuracy unter den entschiedenen Läufen mindestens `90 %`
- je Punkt mindestens eine der zwei Wiederholungen korrekt
- höchstens zwei Ausgaben `unknown`, `ambiguous` oder
  `insufficient_evidence`
- Median-Bodenfehler `0,0 m`
- p95-Bodenfehler höchstens `1,30 m`
- keine falsche entschiedene Position weiter als `1,30 m` vom Sollpunkt

Wenn ein Gate scheitert, bleibt die Live-Positionsanzeige deaktiviert. Das
Ergebnis wird als negativer Versuch dokumentiert; erst danach darf eine neue
Hypothese mit einer vollständig neuen Blindserie geprüft werden.

## Historischer Hardwarezustand vor dem Rollout

Stand 2026-07-29; nicht der aktuelle Gerätestand:

- TX und RX1 bis RX4 sind ausgeschaltet beziehungsweise nicht angeschlossen.
- Der Mac ist nicht mit dem CSI-WLAN verbunden.
- Das ist für die aktuelle Offline-Arbeit korrekt; dafür werden keine
  Livepakete benötigt.
- Vor der nächsten Aufnahme müssen alle fünf ESP32 laufen und der Mac mit dem
  CSI-WLAN verbunden sein.
- Der Mac muss dann an seiner endgültigen, später unveränderten
  Betriebsposition stehen. Die nur für frühere A/B-Tests gewählte Raummitte
  darf nicht stillschweigend als endgültiger Standort übernommen werden.

## Abgeschlossener Offline-Arbeitsschritt

Während die ESP32 ausgeschaltet waren und der Mac nicht im CSI-WLAN war, wurden
drei voneinander abgegrenzte Softwareteile fertiggestellt:

1. **Verbindliche Setupbindung**
   - Raum-, TX-, RX- und spätere Mac-Position werden in einem kanonischen
     Setup-Artefakt festgehalten.
   - Firmware-, Funk-, Subcarrier- und Serverstand werden per ID und Hash an
     dieses Artefakt gebunden.
   - Startparameter und Raw-Aufnahmen dürfen dem Artefakt nicht widersprechen.
     Abweichende RX- oder Subcarrier-Daten müssen die Aufnahme als unvollständig
     markieren, statt unbemerkt in Training oder Blindtest zu gelangen.
2. **Live-Positionskern**
   - Der Livepfad soll dieselben Fenster, Merkmale und Qualitätsregeln wie die
     bestandene Offlinepipeline verwenden.
   - D6-Präsenz bleibt ein vorgeschaltetes Gate.
   - Zulässige Ergebnisse sind ein exakter Punkt P01 bis P09 oder ein ehrlicher
     Zustand wie `unknown`, `ambiguous`, `insufficient`, `uncalibrated` oder
     `stale`.
3. **Ehrliche Sensing-Darstellung**
   - Die UI darf eine Person nur an einem tatsächlich entschiedenen Punkt
     anzeigen.
   - Zwischen Punkten wird nicht künstlich interpoliert.
   - Bei fehlender oder veralteter Evidenz verschwindet die Persondarstellung.
   - Die bisherige Heatmap bleibt höchstens als deutlich bezeichnete
     Signalanzeige sichtbar und gilt nicht als Position.

Die drei Teile sind inzwischen **jeweils separat implementiert und geprüft**:

- Die Setupbindung bestand Unit-Tests, Build sowie echte CLI-, HTTP- und
  UDP-Smoke-Tests. Ein absichtlich gesendeter Frame mit 63 statt 64 Bins schrieb
  null Frames und finalisierte die Aufnahme korrekt als `incomplete`.
- Der Live-Positionskern bestand nach den Review-Korrekturen `11/11` eigene
  Tests sowie alle Capture-/Paritäts- und Positions-Tests. Geprüft wurden unter
  anderem Live-/Offline-Merkmalsparität, Rasterfehler, fehlende RX, begrenzter
  Puffer, Reset, Veraltung und der 4-aus-5-Konsens.
- Die UI bestand den Sensing-Lokalisierungstest und JavaScript-Syntaxprüfungen.
  Alle fünf Fehlerzustände, ein fehlendes Positionsschema, die alte
  Groblokalisierung und ein Verbindungsabbruch löschen die Persondarstellung.

Die additive Serververbindung dieser drei Teile ist ebenfalls abgeschlossen.
Raw-CSI wird unabhängig von einer laufenden Aufnahme an den Livekern
weitergereicht; D6 bleibt dessen vorgeschaltetes Präsenzgate. Der
WebSocket-Datensatz enthält einen expliziten fail-closed Positionszustand, und
die UI zeigt nur bei `state=position` und bestätigter Präsenz einen Körper an.
Die Gesamtprüfung aus Rust-Tests, Server-Build, realer CLI-Prüfung,
UI-Regressionstest und Quelltextprüfungen ist bestanden. Das ist weiterhin nur
ein Nachweis der Softwarekette und kein realer Positionsnachweis.

### Befunde aus dem unabhängigen Cross-Review

Vor der Gesamtprüfung wurden zwei relevante Lücken gefunden:

1. Ein fail-closed Übergang setzte zwar den 4-aus-5-Konsens zurück, leerte aber
   noch nicht den Raw-Frame-Puffer. Nach `absent`, `stale`, unvollständiger
   Evidenz oder einem Eingabefehler hätten dadurch Frames aus der vorherigen
   Entscheidungsphase noch in ein späteres Drei-Sekunden-Fenster gelangen
   können.
2. Die UI behandelte `_simulated: true` auch dann als Demo, wenn die Quelle
   zugleich `esp32` behauptete. Außerdem akzeptierte sie noch konvertierbare
   Stringkoordinaten, mehr als drei Koordinaten und Werte außerhalb des Raums.

Beide Befunde sind **behoben und separat per Regressionstest geprüft**:

- `fail_closed` leert jetzt Rohframes, Frischdatenepoche, Konsens und
  Koordinaten. Danach werden drei Sekunden ausschließlich neue CSI-Daten
  gesammelt. `9/9` Live- und `105/105` Positions-Tests bestanden.
- Demo-Daten werden nur noch über die positive Source-Allowlist `simulated`,
  `simulate` oder `demo` erkannt. `_simulated: true` allein wird ignoriert.
  Punkt-ID, exakt drei numerische Koordinaten und Raumgrenzen werden strikt
  geprüft.
- `SensingTab.init()` und `dispose()` sind gegen parallele und wiederholte
  Aufrufe abgesichert. Der UI-Test deckt Simulationsspoofing, ungültige
  Positionen, Grenzen, Snap-/Löschregeln, Disconnect und Lifecycle-Races ab.

Die additive Serverintegration und ihre Gesamtprüfung sind abgeschlossen. Eine
reale Livefreigabe besteht trotzdem noch nicht, weil weder ein real gemessener
Positionsindex noch eine bestandene Blindprüfung vorliegen.

Während der Serverintegration wurde eine weitere Veraltungsgrenze gefunden:
Edge-Vitals-Pakete halten die ESP32-Verbindung aktiv, enthalten aber selbst
keine Raw-CSI-Fingerprints. Wenn Raw-CSI stoppt und Edge-Vitals weiterläuft,
darf deshalb eine zuletzt bestätigte Position nicht weitergereicht werden.
Diese Lücke ist behoben und per Regressionstest abgesichert: Nach einer Sekunde
ohne akzeptiertes Raw-CSI wird der Zustand `stale`, die Koordinaten werden
gelöscht, und Edge-Vitals kann niemals selbst eine neue Position erzeugen.

## Abschlussprüfung vor dem Hardwareübergang

Ein weiterer Quelltextaudit fand vier Stellen, die vor realen Aufnahmen
geschlossen werden mussten:

1. **Classification vor D6-Readiness:** Mit aktivem Positions-Setup konnte der
   alte D4-Fallback noch eine Anwesenheit behaupten. Jetzt liefert ein
   setupgebundener Lauf bis zur fertigen Leerraumreferenz ausschließlich
   `uncalibrated` beziehungsweise `calibrating`, jeweils ohne Präsenz und mit
   Konfidenz `0`. Normale RuView-Läufe ohne Positions-Setup behalten ihr
   bisheriges D4-Verhalten.
2. **Punkt-ID-Vertrag:** Der Server verlangte neun eindeutige IDs, die UI aber
   ausdrücklich P01 bis P09. Index und Trainingsmanifest akzeptieren jetzt
   serverseitig nur noch exakt P01 bis P09 in kanonischer Reihenfolge.
3. **Personenvertrag für ESP32:** Die Pose-/Observatory-Schnittstelle konnte
   aus der alten Groblokalisierung eine scheinbar echte Person mit
   prozeduralem Skelett erzeugen. ESP32-Daten dürfen jetzt nur noch bei
   bestätigter Präsenz und gültigem diskretem `position_estimate` einen
   neutralen Marker am exakten P01-bis-P09-Punkt erzeugen. Groblokalisierung,
   Heatmap und D4 dürfen weder Person noch Skelett erzeugen.
4. **TX-Filteridentität:** Der bisherige Schemename war vorhanden, die
   Bytefolge aber nicht eindeutig definiert und der gespeicherte Hash war noch
   kein Beleg für den tatsächlich laufenden Frame. Für
   `sha256-ruview-tx-filter-mac-v1` wird nun SHA-256 über genau die sechs
   binären Bytes des auf den RX geschriebenen NVS-Blob `filter_mac` in
   Netzwerkreihenfolge berechnet. Textform, Doppelpunkte, Groß-/Kleinschreibung
   und Nullterminierung gehen nicht in den Hash ein. Provisioning und Server
   teilen denselben festen Testvektor. Die neue Firmware hängt außerdem an
   jedes CSI-Datagramm einen strikt geprüften 40-Byte-Nachweis an. Er bestätigt,
   dass der Filter aktiv war, die Quell-MAC des Frames dem Filter entsprach und
   der Hash aus genau diesem laufenden Filter gebildet wurde. Bei aktivem Setup
   müssen alle drei Aussagen und der Setup-Hash übereinstimmen, bevor der Frame
   Classification, D4/D5/D6, Liveness, Position oder Recorder erreichen darf.
   Das ist eine Laufzeitbehauptung der kontrollierten Firmware, keine
   kryptographische Geräteauthentisierung gegen einen Angreifer.

Observatory besitzt dafür einen eigenen sichtbaren Evidenzvertrag:

- `CONNECTING`: WebSocket gewählt, aber noch kein verwertbarer Frame
- `LIVE ESP32`: nur ein höchstens drei Sekunden alter Frame mit exakter Quelle
  `esp32` und ohne Simulationsmarker
- `SIMULATED`: ausdrücklich synthetische Demo
- `STALE`: geschlossene Verbindung, Zeitüberschreitung oder unbekannte Quelle

Ein offener WebSocket allein ist kein Live-Nachweis. Im Hardwaremodus werden
nur die validierten realen Raum-, TX- und exakt RX1-bis-RX4-Koordinaten
verwendet. Die feste Demo-Geometrie, prozedurale Figuren und Szenarioprops
bleiben im Simulationsmodus. Bei gültiger Präsenz und Position zeigt
Observatory lediglich einen neutralen statischen Marker; ohne diese Evidenz
werden Marker und Hardwarefeld geleert. Das Feld ist sichtbar als
diagnostisches CSI-Feld gekennzeichnet und nicht als Personenposition.

Zusätzlich zeigt `GET /health/ready` die geladene Setup-Identität nun
unabhängig davon an, ob bereits ein Positionsindex geladen ist. Dadurch kann
vor der Leerraumaufnahme geprüft werden, dass der richtige Aufbau aktiv ist,
ohne vorher einen noch gar nicht existierenden realen Index zu benötigen.

Für reale Aufnahmen existiert jetzt ein kontrollierter Runner:

```text
python3 scripts/capture_position_run.py \
  --kind <discovery|preflight|empty|position> \
  --recording-id <NEUTRALE_ID>
```

Für `--kind empty` ist zusätzlich `--confirm-empty-room` erforderlich. Das
Werkzeug:

- verlangt `status=ready`, eine frische Quelle `esp32` und exakt die aktiven
  RX1 bis RX4
- verwendet `discovery` ausschließlich ohne Setup; `preflight`, `empty` und
  `position` ausschließlich mit dem daraus erzeugten versiegelten Setup
- verlangt bei versiegelten Läufen für jeden RX einen höchstens zwei Sekunden
  alten, vollständig passenden Laufzeit-TX-Nachweis
- sendet an den Recorder ausschließlich die neutrale Aufnahme-ID, niemals
  Punktlabel oder Ground Truth
- verwendet fest 25 Sekunden für Discovery und Preflight, 65 Sekunden für
  Leerraum und 35 Sekunden für eine Position
- akzeptiert nur mindestens 5 Hz und die volle Dauer **je RX**, ein je RX
  stabiles Raster, null verlorene Frames, übereinstimmende Gesamt-/Sidecar-/
  Raw-Zähler und exakt dieselbe Setup-ID samt Setup-Hash
- setzt zusätzlich einen serverseitigen Zeitwächter, sodass eine verlorene
  HTTP-Antwort keine unbegrenzt weiterlaufende Aufnahme hinterlässt

Der allgemeine Training-Tab ist für dieses Blindprotokoll nicht maßgeblich,
weil dort andere Labels und Trainingszwecke verwendet werden können.

### Read-only-Setup-Preflight vom 2026-07-29

Ohne eingeschaltete Hardware wurde nur die lokale, sensible Werte
unterdrückende Ausgangslage geprüft:

- Für die vier aktuellen RX-Kandidaten sind lokal Node-IDs 1 bis 4, Kanal 6,
  Raw-CSI-Modus und ein gemeinsamer TX-Filter hinterlegt.
- Eine ältere, doppelte RX3-Zustandsdatei ohne Filter existiert ebenfalls und
  darf nicht als Gerätewahrheit verwendet werden.
- Lokale Provisioning-Dateien beweisen nicht, welche Konfiguration aktuell auf
  den Boards läuft.
- Die vorhandene RX-Firmwarequelle meldet Version `0.7.0`; der tatsächlich
  geflashte Stand jedes Boards ist noch live zu belegen.
- TX-Artefakt, endgültiger Server-Build, tatsächliches CSI-Raster und
  endgültige Mac-/Kabel-/Möbel-/Türrevision sind noch nicht versiegelt.

Es wurden dabei keine SSIDs, Passwörter, Zieladressen, OTA-Schlüssel oder
rohen MAC-Adressen in die Dokumentation übernommen.

## Offline-Abschluss „Software und Vorbereitung“ vom 2026-08-01

Die Softwarevorbereitung ist bis zur Hardwaregrenze abgeschlossen. Dieser
Abschluss umfasst keinen neuen Funkversuch und keine Aussage zur realen
Erkennungs- oder Positionsgüte.

### Verbindlicher Datenweg

1. Die RX-Firmware hängt an jedes Raw-CSI-Datagramm einen strikt definierten
   40-Byte-Nachweis an. Er enthält Versions-/Statusfelder und den SHA-256-Wert
   der konfigurierten sechs binären TX-MAC-Bytes, aber nicht die rohe MAC.
2. Der gemeinsame Parser akzeptiert nur den vollständig vorhandenen und
   konsistenten Nachweis. Fehlerhafte oder abgeschnittene Trailer dürfen die
   nachfolgenden Datagramme nicht aus dem Takt bringen.
3. Bei versiegeltem Setup prüft der Server zusätzlich Setup-Identität,
   erwartete RX, Subcarrier-Raster und den Laufzeit-TX-Nachweis. Erst danach
   darf ein Frame Liveness, Classification, D4/D5/D6, Position oder Recorder
   beeinflussen.
4. Jede Ablehnung macht die Binding-Attestierung des betroffenen RX ungültig.
   Eine aktive Aufnahme wird als unvollständig abgeschlossen, statt den
   verworfenen Frame still zu ignorieren.
5. `/api/v1/nodes` veröffentlicht pro RX nur die Zustände
   `source_binding_attested`, `filter_enforced`, `source_matched_filter`,
   `identity_valid`, `identity_matches_setup` und `binding_last_seen_ms`.
   MAC-Adresse und Hash werden dort nicht ausgegeben. Nach zwei Sekunden ohne
   neuen gültigen Nachweis ist die Attestierung nicht mehr frisch.
6. Edge-Vitals-Pakete werden bei aktivem versiegeltem Setup vor jeder Änderung
   von Liveness, Features oder Classification ignoriert. Ohne Positions-Setup
   bleibt der allgemeine RuView-Fallback erhalten.
7. Eine Aufnahme mit null geschriebenen Frames wird immer als `incomplete`
   abgeschlossen.

Dieser Nachweis schützt die Messkette gegen versehentliche Fehlkonfiguration
und vermischte Paketquellen innerhalb der kontrollierten Firmware. Er ist keine
kryptographische Geräteauthentisierung gegen einen aktiven Angreifer.

Der unversiegelte Discovery-Lauf ist deshalb ausschließlich eine Inventur von
RX-IDs, Raster und Datenrate. Er meldet ausdrücklich nicht „messbereit“ und darf
nicht als bestandener Setup-Nachweis verwendet werden. Discovery startet nur,
wenn das nicht-identifizierende Server-Bool bestätigt, dass exakt RX1 bis RX4
frisch dieselbe vollständige 0x07-TX-Bindung melden. Erst der danach
versiegelte, bindungsgeprüfte Preflight gibt eine Messung frei; Hash und MAC
werden dabei nicht öffentlich ausgegeben.

### Firmware- und Provisionierungsgrenzen

- Provisionierung speichert die rohe Filter-MAC nicht mehr in NVS-Dumps,
  Mock-Logs oder der öffentlichen `GET /config`-Antwort. Dort wird nur noch
  gemeldet, ob ein Filter konfiguriert ist.
- Eine rohe MAC darf ausschließlich in der privaten Provisioning-Zustandsdatei
  mit Dateimodus `0600` verbleiben, damit partielle Wiederholungen
  reproduzierbar sind. Fehlerhafte Altzustände werden bei der Ausgabe
  geschwärzt.
- OTA-Schlüssel werden nicht im wiederverwendbaren Zustand abgelegt. Ein
  Schutz verhindert, dass ein bestehender OTA-Schlüssel bei einer partiellen
  Provisionierung unbemerkt gelöscht wird. Ist der Zustand fehlend, beschädigt
  oder nicht vertrauenswürdig, muss explizit ein neuer Schlüssel, Löschen oder
  „kein Schlüssel vorhanden“ bestätigt werden; andernfalls bricht der Rewrite
  vor NVS-Erzeugung und Flashen ab.
- Bestehende historische Raw-v1-Aufnahmen bleiben lesbar. Neu erzeugte,
  setupgebundene Positionsaufnahmen sowie `inspect`, `build-index` und
  `predict` verlangen dagegen den vollständigen, konsistenten TX-Nachweis und
  brechen bei fehlender oder abweichender Bindung ab.

### Bestätigte Offlineprüfungen

Folgende gezielte Prüfungen waren beim Dokumentationsabschluss bestätigt:

- vollständige Rust-Matrix: `1.118` bestanden, `0` fehlgeschlagen, `3` bewusst
  ignoriert
  - Sensing-Server: `885` bestanden, `2` ignoriert
  - Hardware: `177` bestanden, `1` ignoriert
  - CLI: `33` bestanden
  - Pointcloud: `23` bestanden
- Provisionierung `27/27`, ADR-110 `21/21` und Vitals-Hosttests `22/22`
- Hardwareparser `20/20` und maximaler UDP-Loopback `1/1`
- Setup `13/13`, Livekern `11/11`, Positionsaufnahme `13/13` und Pointcloud
  `6/6`
- synthetischer Positions-End-to-End-Weg `9/9` sowie fail-closed
  `inspect`/`build-index`/`predict` `17/17`
- Capture-Runner `8/8`
- Source-Binding-Vertrag, Server-Ablehnungspfade, Sanitizer-Lauf,
  Python-Kompilierung, JavaScript-Syntax und die separaten Sensing-/Observatory-
  UI-Regressionen bestanden
- die vorhandenen mmWave-Prädikatstests `8/8` bestanden; das ist ausdrücklich
  kein Integrations- oder Aktivierungsnachweis für das zurückgestellte Modul

Die Rust-Gesamtsumme enthält nur die vier oben aufgeschlüsselten Rust-Pakete;
separate Host-, Python- und UI-Tests werden nicht doppelt hinzugerechnet. Keine
dieser Zahlen ersetzt eine reale Gütemessung. Ältere Summen bleiben als
historische Stände vom 2026-07-29 erhalten.

### Realistische Leerraumreferenz

Der Mac steht für Kalibrierung, Training und Blindtest an seiner normalen
Betriebsposition, nicht künstlich in der Raummitte. Mac, Netzteile, Kabel,
Möbel, Türstellung und andere statische Bestandteile bleiben im Raum und werden
in der Leerraumreferenz mitgemessen. „Raum leer“ bedeutet nur, dass sich keine
Person darin befindet. Eine wesentliche spätere Änderung dieser statischen
Umgebung erfordert eine neue Leerraumreferenz und kann auch einen neuen
Positionsindex erforderlich machen.

### Noch im gemeinsamen Livebetrieb prüfbar

- Sichtprüfung der dauerhaft beschrifteten physischen Boards: RX1 bis RX4
  müssen an genau den im Setup hinterlegten Koordinaten stehen; die selbst
  gemeldete RX-ID kann eine physische Vertauschung nicht erkennen
- gemeinsame Laufzeitkonfiguration der unveränderten TX-Senderfirmware und der
  bereits geflashten RX1 bis RX4
- echtes CSI-Raster, Datenrate, Paketverluste und Frische aller vier RX
- Laufzeit-TX-Nachweis jedes realen RX gegen das versiegelte Setup
- D6-Leerraumreferenz im normalen Raumaufbau
- Trennung von leerem Raum und Person sowie reale P01-bis-P09-Fingerprints
- unabhängige Blindwerte für Coverage, Accuracy und Positionsfehler

ESP-IDF v5.4 ist lokal unter `.toolchains/` installiert. Die aktuelle Firmware
0.7.0 wurde erfolgreich für ESP32-S3 mit 8 MB (`1.129.872` Byte, `46 %`
Reserve), ESP32-S3 mit 4 MB (`913.920` Byte, `52 %` Reserve) und den CI-
Forschungstarget ESP32-C6 (`1.054.736` Byte, `45 %` Reserve) gebaut. Die
geprüften S3-Artefakte und SHA-256-Prüfsummen liegen unter
`artifacts/ruview-firmware-0.7.0-2026-08-01/`. RX1 bis RX4 haben
Flash-Größenerkennung, Flashen und Einzelboard-Bootprüfung bestanden. Der TX
benötigt keine RX-Firmware und hat die zerstörungsfreie Inventur sowie den
stabilen SoftAP-Boot bestanden.

RX1 hat dieses Gate inzwischen bestanden. Das Board meldete ESP32-S3 Revision
0.2, 16 MB physischen Flash und 8 MB PSRAM. Es wurde absichtlich mit dem
verifizierten 8-MB-Layout geflasht; Schreibvorgang und Hashprüfung waren
erfolgreich. Der Bootlog bestätigte Node-ID 1, Kanal 6, Edge-Tier 0, aktiven
TX-Filter und Zielserver `192.168.4.50:5005`. RX2 bestand anschließend
dasselbe Gate mit Node-ID 2; RX3 ebenso mit Node-ID 3. Beide meldeten dieselbe
Funk-/Filterkonfiguration. RX4 bestand es anschließend mit Node-ID 4. Die
fehlende WLAN-Verbindung war bei ausgeschaltetem TX beziehungsweise CSI-AP
erwartbar. Alle vier RX sind mit dem neuen Build geprüft. Der anschließende
Audit bestätigte die korrekte getrennte TX-Firmware und dass kein TX-Flash
erforderlich ist.

Der TX-Audit bestätigte inzwischen die getrennte Arduino-SoftAP-Firmware. Sie
bleibt unverändert; `esp32-csi-node` 0.7.0 ist ausschließlich die RX-Firmware.
Die neue Quellbindung benötigt keine Senderänderung, weil Filterung und
Laufzeitnachweis auf RX1 bis RX4 stattfinden. TX-Inventur und serieller Boot
sind inzwischen ebenfalls bestanden: ESP32-S3,
16 MB Flash, 8 MB PSRAM, stabiler Start und SoftAP auf `192.168.4.1` ohne
Brownout- oder Reset-Schleife. DHCP/Gateway und 32-Byte-Broadcastempfang sind
mit dem verbundenen Mac ebenfalls bestanden; `45,5 Hz` wurden im stabileren
10-Sekunden-Fenster gemessen. Die gemeinsame Discovery muss Kanal 6 und die
per-RX-Datenqualität noch zur Laufzeit bestätigen. Ein Reflash wäre ein
Fehlerbehebungsfall und setzt eine private Vollsicherung des aktuellen
TX-Flashs voraus.

## Nächste Schritte

### Schritt 1 — Offline-End-to-End-Test: abgeschlossen

Ein synthetischer, dateibasierter Test erzeugt:

- eine Leerraumaufnahme
- neun Trainingsaufnahmen
- neun davon getrennte Blindaufnahmen
- ein separates Truth-Manifest

Der Test muss den gesamten Weg
`inspect → build-index → predict → evaluate` ausführen, ohne die
Sicherheitsregeln abzuschwächen.

Ergebnis: bestanden. Alle neun synthetischen Blindpunkte wurden korrekt
zugeordnet.

### Schritt 2 — Gesamtprüfung: abgeschlossen

Danach:

- vollständige Rust-Tests des Sensing-Server-Binaries
- Debug-Build
- Prüfung der echten CLI-Hilfe
- Prüfung falscher CLI-Kombinationen und Exit-Codes
- `git diff --check`

Ergebnis:

- vollständiger Rust-Testlauf: `852` bestanden, `0` fehlgeschlagen,
  `2` absichtlich ignoriert
- Server-Debug-Build bestanden
- Sensing-UI-Regressionstest und JavaScript-Syntaxprüfungen bestanden
- echte CLI-Hilfe sowie Fehlerfälle für fehlende Setup-/Indexbindung und
  ungültigen SHA-256 mit korrektem Exit-Code bestanden
- `git diff --check` und gezieltes `rustfmt +stable --check` für alle
  bearbeiteten Rust-Module bestanden

Das im Repository gepinnte Toolchain enthält aktuell kein `cargo-fmt`.
Ein workspaceweiter Lauf mit dem stabilen Toolchain zeigt außerdem bereits
vorhandene Formatabweichungen in nicht betroffenen Workspace-/Vendor-Dateien.
Deshalb gilt hier die gezielte Prüfung der bearbeiteten Module, nicht eine
falsche Behauptung über den gesamten fremden Workspace. Die angezeigten
Compilerwarnungen stammen aus bereits vorhandenen, nicht betroffenen
WiFiScan-/Matter-Dateien.

### Schritt 3 — Aufbau einfrieren: als Nächstes

Vor dem ersten neuen Hardwarelauf werden einmalig festgehalten:

- normale Mac-Betriebsposition und Kabel; nicht die nur für frühere A/B-Tests
  verwendete Raummitte
- TX-/RX-Positionen
- Raummaße
- WLAN-Kanal und Subcarrier-Raster
- TX-Filteridentität nach dem exakt definierten Sechs-Byte-Hashschema, ohne
  die rohe MAC zu dokumentieren
- Firmware-/Serverstand
- Tür- und relevante Möbelstellung

Aus diesen Daten entsteht eine Setup-ID mit Hash. Jede Aufnahme muss exakt an
diesen Aufbau gebunden sein. Mac, Kabel, Möbel und andere statische Gegenstände
bleiben dabei ausdrücklich im Raum und werden von der Leerraumreferenz als
normaler Hintergrund mitgemessen. „Leerer Raum“ bedeutet nur „keine Person“.

Nach dem Einschalten ist die Reihenfolge:

1. TX und RX1 bis RX4 anschließen, den Mac an seine normale Betriebsposition
   stellen und mit dem CSI-WLAN verbinden. Danach die Board-Beschriftungen RX1
   bis RX4 gegen die dokumentierten Koordinaten kontrollieren.
2. Live-Konfiguration jedes RX prüfen: Node-ID 1 bis 4, gemeinsamer Kanal,
   Raw-CSI/Edge-Tier 0, kein Hopping/TDM und identische TX-Filteridentität.
3. Geflashte RX-Firmware, unveränderte TX-Senderfirmware und den tatsächlich
   gestarteten Server-Build belegen.
4. Den Server noch **ohne** Positions-Setup starten und eine 25-Sekunden-
   `discovery` aufnehmen. Sie muss alle vier RX mit mindestens 5 Hz und über
   die gesamte Dauer ein exakt stabiles Raster liefern.
5. Aus den so belegten realen Angaben das Setup-Artefakt erzeugen und seinen
   Hash als Grenze für alle folgenden Aufnahmen verwenden.
6. Den Server mit genau diesem Setup neu starten und einen 25-Sekunden-
   `preflight` aufnehmen. Zusätzlich zu Raster und Datenrate müssen nun alle
   vier frischen Laufzeit-TX-Nachweise zum Setup passen.
7. Den absichtlichen Ablehnungsfall einmal prüfen: Ein widersprechender
   RX-/Rasterframe darf null gültige Frames schreiben und muss die Aufnahme als
   unvollständig markieren.

Die Leerraumkalibrierung beginnt ausschließlich nach der ausdrücklichen
Bestätigung `Raum leer`. Jeder Eintritt während der 65 Sekunden macht die
Aufnahme ungültig und erfordert eine neue neutrale Aufnahme-ID. Eine
stillstehende Person kann vor der fertigen Referenz nicht zuverlässig
softwareseitig als solche erkannt und ausgeschlossen werden.

### Schritt 4 — Reale Trainingsserie

- Leerraum: mindestens 65 Sekunden
- anschließend drei getrennte Leerraum-Prüfaufnahmen: je mindestens 35 Sekunden
- P01 bis P09: je mindestens 35 Sekunden
- Person steht möglichst still und immer gleich ausgerichtet
- erste fünf Sekunden jeder Aufnahme sind Einpendelzeit

### Schritt 5 — Getrennter Blindtest

- zwei neue Aufnahmen je Punkt in zwei getrennten Durchgängen
- Reihenfolge vor jedem Durchgang zufällig festlegen
- neutrale Aufnahme-IDs statt Punktnamen
- Truth-Datei getrennt halten
- Vorhersagen erstellen, bevor die Wahrheit geöffnet wird

### Schritt 6 — Entscheidung

Nur wenn der Blindtest die vorab festgelegten Qualitätsgrenzen erreicht:

- Positionsindex in den Live-Server laden
- D6-Präsenz als vorgeschaltetes Gate verwenden
- nur stabile P01-bis-P09-Positionen an die UI senden
- bei `unknown`, `ambiguous`, fehlender Präsenz oder veralteten Daten die
  Persondarstellung löschen

Wenn der Blindtest scheitert, wird nicht durch eine schönere Heatmap
kaschiert. Dann werden die Fehler nach Punkt, RX und Merkmal ausgewertet.

## Wiedereinstieg nach einer Unterbrechung

1. Dieses Dokument lesen.
2. `git status --short` in `RuView` prüfen und fremde Änderungen nicht
   überschreiben.
3. Im Arbeitsplan den ersten noch offenen Schritt wählen.
4. Vor Hardwaretests ausdrücklich sagen, was getestet wird und wie lange es
   dauert.
5. Keine reale Messung beginnen, solange Aufbau-ID und normale Mac-Betriebsposition
   nicht bestätigt sind.
6. Erst nach bestandener Blindprüfung den realen Positionsindex in den bereits
   vorbereiteten Livepfad laden.

## Änderungsprotokoll

### 2026-07-29 — Kontextfester Offline-Arbeitsstand

- eigenen Wiedereinstiegspunkt angelegt
- feste Geometrie und neun Punkte dokumentiert
- Offlinepipeline, Sicherheitsregeln und offene Nachweise getrennt
- aktuellen ausgeschalteten Hardwarezustand festgehalten
- nächsten Übergang von Offlineprüfung zu realen Messungen definiert

### 2026-07-29 — Dateibasierter End-to-End-Nachweis

- strikte synthetische Raw-CSI- und Sidecar-Dateien erzeugt
- Leerraum, neun Trainingspunkte und neun getrennte Blindpunkte verarbeitet
- vollständige Offlinepipeline ohne Truth-Leakage erfolgreich durchlaufen
- `9/9` synthetische Blindpositionen korrekt
- vollständiger Testlauf mit `852` bestandenen, `0` fehlgeschlagenen und
  `2` absichtlich ignorierten Tests; UI-Test, Debug-Build, CLI und gezielte
  Formatprüfung bestanden
- nächster offener Schritt ist das kanonische Einfrieren des realen Aufbaus

### 2026-07-29 — Offline-Vorbereitung der Liveintegration abgeschlossen

- Hardwarezustand unverändert: alle ESP32 aus, Mac nicht im CSI-WLAN
- Setupbindung, Live-Positionskern und fail-closed UI als getrennte
  Arbeitspakete implementiert, verbunden und geprüft
- Livepfad akzeptiert nur einen an dasselbe versiegelte Setup gebundenen Index
  samt exaktem SHA-256; ohne realen Index bleibt er `uncalibrated`
- keine reale Messung, kein neuer Positionsindex und keine Aussage zur realen
  Genauigkeit
- nächster Schritt: normale Mac-Betriebsposition und reale Hardwareangaben bestätigen,
  daraus das versiegelte Setup erzeugen und erst danach Messungen beginnen

### 2026-07-29 — Cross-Review vor Livefreigabe

- Wiederverwendung alter Raw-Frames über fail-closed Zustandsgrenzen gefunden
- Simulationsflag konnte die reale ESP32-Quelle in der UI überstimmen
- zu tolerante Browserprüfung für Punktkoordinaten gefunden
- alle drei Befunde korrigiert und separat per Regressionstest geprüft
- im Integrationsreview zusätzlich mögliche stale Position bei weiterlaufenden
  Edge-Vitals ohne Raw-CSI gefunden; Korrektur und Regressionstest abgeschlossen
- Server-Gesamtintegration und Gesamtprüfung mit `852` bestandenen Tests
  abgeschlossen
- weiterhin keine Livefreigabe und keine reale Positionsaussage

### 2026-07-29 — Abschlussprüfung und reproduzierbarer Hardwareübergang

- Classification bei aktivem Positions-Setup vor fertiger D6-Referenz
  fail-closed auf `uncalibrated`/`calibrating` gesetzt
- Serververtrag auf exakt P01 bis P09 vereinheitlicht
- ESP32-Personen aus Groblokalisierung und prozedurale ESP32-Skelette entfernt
- Observatory auf `CONNECTING`/`LIVE ESP32`/`SIMULATED`/`STALE`, echte
  Frame-Geometrie und neutralen Positionsmarker umgestellt
- Setup-Readiness unabhängig vom noch fehlenden realen Index sichtbar gemacht
- TX-Filterhash exakt als SHA-256 über die sechs NVS-Bytes definiert und in
  Provisioning sowie Server mit demselben Testvektor geprüft
- kontrollierten Capture-Runner mit Setup-, RX-, Datenraten-, Drop- und
  Sidecar-Prüfung ergänzt
- Hardwarezustand unverändert; noch keine reale Aufnahme und kein
  Leistungsnachweis

### 2026-08-01 — ESP-IDF-Target-Build und Flashartefakte

- lokale, projektgetrennte ESP-IDF-v5.4-Toolchain installiert
- aktuelle Firmware 0.7.0 für S3-8-MB und S3-4-MB erfolgreich gebaut
- CI-Forschungstarget C6 ebenfalls erfolgreich kompiliert
- geprüfte S3-Artefakte außerhalb des Repositories mit SHA-256 gesichert
- veraltete v0.6.7-`release_bins` ausdrücklich für D5/D6 und Position gesperrt
- nächstes Gate: beschriftetes Board anschließen, Flash-Größe mit `flash-id`
  erkennen, erst danach passende Variante flashen und Boot prüfen

### 2026-08-01 — RX1 geflasht und gebootet

- RX1 als ESP32-S3 mit 16 MB Flash und 8 MB PSRAM inventarisiert
- verifiziertes 8-MB-Layout ohne NVS-Löschung geflasht
- alle geschriebenen Images per Hash bestätigt
- Firmware 0.7.0, Node-ID 1, Kanal 6, Edge-Tier 0 und TX-Filter bestätigt
- WLAN/CSI noch nicht bewertet, da TX beziehungsweise CSI-AP nicht aktiv war
- OTA bleibt ohne provisionierten Security-Namespace absichtlich fail-closed

### 2026-08-01 — RX2 geflasht und gebootet

- RX2 als ESP32-S3 mit 16 MB Flash und 8 MB PSRAM inventarisiert
- verifiziertes 8-MB-Layout ohne NVS-Löschung geflasht
- Firmware 0.7.0, Node-ID 2, Kanal 6, Edge-Tier 0 und TX-Filter bestätigt
- WLAN/CSI noch nicht bewertet, da TX beziehungsweise CSI-AP nicht aktiv war

### 2026-08-01 — RX3 geflasht und gebootet

- RX3 als ESP32-S3 mit 16 MB Flash und 8 MB PSRAM inventarisiert
- verifiziertes 8-MB-Layout ohne NVS-Löschung geflasht
- Firmware 0.7.0, Node-ID 3, Kanal 6, Edge-Tier 0 und TX-Filter bestätigt
- WLAN/CSI noch nicht bewertet, da TX beziehungsweise CSI-AP nicht aktiv war

### 2026-08-01 — RX4 geflasht und gebootet

- RX4 als ESP32-S3 mit 16 MB Flash und 8 MB PSRAM inventarisiert
- verifiziertes 8-MB-Layout ohne NVS-Löschung geflasht
- Firmware 0.7.0, Node-ID 4, Kanal 6, Edge-Tier 0 und TX-Filter bestätigt
- RX1 bis RX4 damit einzeln vollständig geflasht und bootgeprüft
- vor TX-Flash separate Prüfung der vorgesehenen Senderfirmware

### 2026-08-01 — TX-Firmwarepfad bestätigt

- TX nutzt getrennte Arduino-SoftAP-Firmware, nicht RuView RX 0.7.0
- SoftAP-Kanal 6 und ungefähr 50 UDP-Broadcasts/s sind der Sollzustand
- neue RX-TX-Bindung erfordert keine Änderung des Senders
- nächster Schritt ist ein reiner TX-Inventur- und Bootcheck
- vor jedem möglichen TX-Reflash zuerst vollständige private Flashsicherung

### 2026-08-01 — TX-Netzpfad und Hostadresse bestätigt

- DHCP und Gateway mit nur TX und Mac ohne Paketverlust geprüft
- 32-Byte-UDP-Broadcasts im stabileren 10-Sekunden-Fenster mit `45,5 Hz`
  empfangen
- CSI-WLAN-Interface anschließend auf die in RX1 bis RX4 gespeicherte
  Serveradresse `192.168.4.50/24` gesetzt
- TX weiterhin unverändert und nicht geflasht
- Kanal 6 im Senderbuild fest; Laufzeitbestätigung folgt im gemeinsamen
  RX-Discovery-Lauf

### 2026-08-01 — D6-, Blind- und Gesamtverdict-Gates abgeschlossen

- `--kind empty` startet und beendet D5/D6 nun fail-closed zusammen mit der
  ungelabelten verlustfreien Aufnahme und bestätigt gültige Referenzen sowie
  frische Evidenz für exakt RX1 bis RX4
- Classification-Replay auf Schema 2 erweitert: Setup-, Raw-, Sidecar- und
  Signalidentität werden ohne eingebettete Wahrheit ausgegeben
- separater Classification-Truth-Evaluator erzwingt exakt 3 Leerraum- und 18
  belegte Blindaufnahmen sowie alle vorab definierten Gütegrenzen
- privater No-clobber-Truth-Generator schreibt Modus `0600`
- Positionsreport erzwingt nun sämtliche Coverage-, Accuracy-,
  Wiederholungs-, Abstentions-, Median-, p95- und Maximalfehler-Gates
- kombinierter Bericht kann nur PASS ausgeben, wenn Classification und
  Position für denselben Setup-Hash bestehen
- vollständiger Server-Binärtest `394/394`, Python Runner/Generator `18/18`,
  öffentliche Setup-/Trainingsvorlagen gegen echte Rust-Schemata geprüft
- private Provisionierungszustände aller vier RX und die ignorierte Altdatei
  von `0644` auf `0600` beschränkt
- nächster Schritt: finalen Serverbuild einfrieren, dann gemeinsamer
  1TX-/4RX-Discovery- und versiegelter Preflight-Lauf

### 2026-08-01 — Finaler Serverbuild eingefroren

- Release-Build erst nach Abschluss sämtlicher Softwareänderungen erzeugt
- exakt verwendbare Binärdatei unter
  `artifacts/live-position-2026-08-01/sensing-server` archiviert
- Größe `5.954.240` Byte, SHA-256
  `e5cb6302404aa35872071f1ac20e73c26db60281ce826fe9bf365b2b3d5c3823`
- Artifact auf `0500` gesetzt, bytegleich verglichen, Checksumme und reale
  CLI-Optionen erneut geprüft
- nächster Schritt erfordert wieder Hardware und CSI-WLAN: Mac an normale
  Betriebsposition, TX und RX1 bis RX4 einschalten, CSI-Interface mit
  `192.168.4.50`, dann unversiegelte 25-Sekunden-Discovery

### 2026-08-09 — Gemeinsamer 1TX-/4RX-Lauf und korrigierter Serverbuild

- Mac an normaler Betriebsposition, TX und RX1 bis RX4 gemeinsam live geprüft
- nach DHCP-Adresse `192.168.4.6` das CSI-Interface wieder auf die dauerhaft von
  allen RX erwartete Adresse `192.168.4.50/24` gesetzt
- alle vier RX frisch empfangen; 10-Sekunden-Inventur enthielt ausschließlich
  vollständige `0x07`-Bindings, keine Legacy-CSI-Pakete
- pro RX dominantes 64-Subcarrier-Raster und kleinere Zahl gültiger
  128-Subcarrier-Frames beobachtet
- im Build vom 2026-08-01 gefunden: jeder gültige Off-Grid-Frame löschte den
  Binding-Status, markierte die Aufnahme unvollständig und setzte Live-Position
  zurück; deshalb keine irreführend scheiternde Discovery gestartet
- Quellidentität und Rasterauswahl im Server getrennt: gültige Off-Grid-Frames
  werden gezählt und vor D5/D6, Recorder und Live-Position gefiltert; falsche
  Bindings bleiben harte Fehler
- vollständige Server-Binärtests `397/397`, Grid-Tests `7/7`, Setup-Tests
  `14/14`, Format- und Diffprüfung bestanden
- neuer read/execute-only Release unter
  `artifacts/live-position-2026-08-09/sensing-server`, Größe `5.954.240` Byte,
  SHA-256
  `91feb860f89f094ba16ea9d749e3a1e5378de1a25ceedd08cebeb67f2cd3484b`
- Build vom 2026-08-01 bleibt historisch erhalten, ist aber nicht mehr
  messfreigegeben
- nächstes Gate: neue Binärdatei starten und die unversiegelte
  25-Sekunden-Discovery vollständig bestehen

### 2026-08-09 — Unversiegelte Discovery bestanden

- korrigierten Release mit der dokumentierten Raum-, TX- und RX-Geometrie
  gestartet; Setup und Positionsindex dabei absichtlich inaktiv
- gemeinsame TX-Bindung in sechs aufeinanderfolgenden Liveabfragen für RX1 bis
  RX4 stabil `true`; Off-Grid-Zähler stiegen gleichzeitig erwartungsgemäß
- Discovery `discovery-neutral-20260809-01` über 25 Sekunden erfolgreich:
  `2.612` Frames, `0` Drops, `completed`, nicht unvollständig, kein
  Integritätsfehler
- per-RX-Frames: RX1 `623`, RX2 `626`, RX3 `645`, RX4 `718`; alle
  Dauer- und Mindest-Ratengates bestanden
- identisches ausgewähltes Raster für RX1 bis RX4: `2437 MHz`, eine Antenne,
  `64` Subcarrier, PPDU-Typ `0`, Layout-Flags `0`
- Discovery bleibt ausdrücklich nur Inventur, nicht Mess- oder Güte-PASS
- nächstes Gate: genaue normale Mac-Position und exakte TX-Firmwareidentität
  ergänzen, Setup mit dem Serverbuild vom 2026-08-09 versiegeln und danach den
  versiegelten Preflight bestehen

### 2026-08-09 — Physisches Setup und aktive TX-App erfasst

- Mac-Bezugspunkt: Mitte des Unterteils
- Mac in ursprünglicher Notation `(Breite, Länge, Höhe)`:
  `(0,94 m, 0,00 m, 0,87 m)`
- Mac in RuView: `[4.02, 0.87, 2.50]`
- räumliche Beschreibung: gleiche Höhe wie RX4, 4 cm von RX4 entfernt auf der
  von RX2 wegführenden Linie
- Türzustand für die vollständige Serie: geschlossen
- CSI-WLAN verbunden; Mac weiterhin auf `192.168.4.50/24`
- jetzige Mac-Position unterscheidet sich vom historischen Aufbau „Mac
  mittig“; neue Leerraumreferenz zwingend, alte Kalibrierung nicht übertragbar
- TX per `flash_id` als ESP32-S3 Revision 0.2 mit 16 MB Flash und 8 MB PSRAM
  bestätigt; keinerlei Flash- oder Konfigurationsschreibzugriff
- erster vollständiger 16-MB-Readback bei ungefähr 7 % wegen serieller
  Paketstörung abgebrochen; unvollständige Temp-Datei gelöscht
- Partitionstabelle und OTA-Auswahl danach bei niedriger Baudrate gelesen;
  aktive App eindeutig `app0` bei `0x10000`
- aktive 1280-KiB-App-Partition vollständig gelesen und intern validiert:
  App-Version `43a8f6d`, Buildzeit `2026-06-02 11:17:54`, ESP-IDF `v5.5.4`
- SHA-256 des vollständigen aktiven Partitions-Readbacks:
  `a66a11ad8e299a962572c2bc8a9e4067599a8460c44ae0efb1deae07277994e5`
- gemeinsamer privater TX-Filterhash von RX1 bis RX4:
  `60c998af0f5f845bd2afaac558a7da831a3a34ec07544de0efc6d1e747fad86c`
- sämtliche temporären Readback-Dateien nach der Prüfung gelöscht; rohe MAC,
  WLAN-Zugangsdaten und OTA-Schlüssel in diesem neuen Nachweis nicht
  wiederholt
- Grenzstatus: TX noch per USB angeschlossen, deshalb Setup bewusst noch nicht
  versiegelt und versiegelter Preflight noch nicht gestartet
- nächster physischer Schritt: USB-Datenkabel entfernen, TX ausschließlich mit
  Strom versorgen, Aufbau unverändert lassen; danach Setup erzeugen und den
  binding-aware 25-Sekunden-Preflight ausführen

Detailnachweis:
[results/2026-08-09_D6_setupaufnahme-und-TX-firmwareidentitaet.md](results/2026-08-09_D6_setupaufnahme-und-TX-firmwareidentitaet.md)

### 2026-08-09 — Setup-Siegel und versiegelter Preflight bestanden

- TX nach dem Firmware-Readback wieder ausschließlich mit Strom versorgt
- Setup-Spezifikation und Siegel privat mit Modus `0600` unter
  `private/d6-20260809/` erzeugt
- Setup-ID `setup-0a49d75f122f9dc9`
- Setup-SHA-256
  `0a49d75f122f9dc9757aed7e175bb444056e7fdde6889bd8965288d1b9008a4e`
- Siegel bindet exakte Raum-, TX-, RX1-bis-RX4- und Mac-Geometrie,
  Firmware-, Server- und Filterhashes, Kanal/Raster sowie
  Kabel-/Möbel-/Türrevisionen
- erster Serverstart scheiterte nach erfolgreicher Siegelprüfung ausschließlich
  an fehlenden lokalen Sandbox-Portrechten; keine Aufnahme gestartet
- derselbe unveränderte Release danach mit lokalen Portrechten erfolgreich
  gestartet: `status=ready`, Quelle `esp32`, richtiges Setup aktiv
- exakt RX1 bis RX4 frisch aktiv; alle Laufzeit-Bindingflags passend
- Preflight `preflight-neutral-20260809-01` über 25 Sekunden bestanden:
  2.545 Frames, 0 Drops, `completed`, nicht unvollständig, kein
  Integritätsfehler, keine Labels
- per-RX-Frames: RX1 604, RX2 647, RX3 684, RX4 610
- je RX identisches Raster: 2437 MHz, eine Antenne, 64 Subcarrier, PPDU-Typ 0,
  Layout-Flags 0
- Raw-SHA-256
  `0bd38597fc59083d1a61a2e752202a7784bacc878ff449ceb7d8a278cfce31a3`
- Meta-SHA-256
  `5488fd47ded95bafc786cace4b88acb81c1e09289dc455a95c8c7d129df2280b`
- definierter Transport-/Binding-/Raster-/Recorder-Preflight damit PASS; noch
  keine Aussage zur Classification- oder Positionsgüte
- allgemeiner Engine-Trust separat wegen RX-Zeitstempelspreizung über 60 ms
  auf `Restricted`, Live-Roh-Ausgaben unterdrückt; vor abschließender
  Live-Anzeige erneut zu prüfen
- nächstes Gate: 65-Sekunden-Leerraumkalibrierung erst nach ausdrücklicher
  Bestätigung `Raum leer`

Detailnachweis:
[results/2026-08-09_D6_setup-siegel-und-preflight.md](results/2026-08-09_D6_setup-siegel-und-preflight.md)

### 2026-08-09 — Sidecar-Fix, Neusiegelung und neuer Preflight

- erste 65-Sekunden-Leerraumaufnahme live sauber abgeschlossen, vor P01 aber
  wegen einer strikten Offline-Sidecar-Inkompatibilität angehalten
- Rohdatei und Sidecar der ersten Aufnahme unverändert erhalten
- Inspektor akzeptiert die expliziten Recorderfelder `max_duration_seconds`
  und `rx_summaries` und verifiziert die RX-Zusammenfassungen gegen die
  tatsächlichen Rohframes
- vollständige Server-Binärtestsuite: 398 bestanden, 0 fehlgeschlagen
- neues read/execute-only Artefakt
  `artifacts/live-position-2026-08-09-sidecar-fix/sensing-server`, SHA-256
  `6554c5101bc7e920e9ce52ea5d845d2afd62b97f09d0c31917d1b1b61d14f8b5`
- physischer Aufbau unverändert neu versiegelt als
  `setup-2beda4496ccfb547`, Setup-SHA-256
  `2beda4496ccfb547217f15ed62418d363aed8ddbc19221d872c4a89a1a3564a0`
- neuer Preflight `preflight-neutral-20260809-02`: 25 Sekunden, 2.701 Frames,
  0 Drops, exakt RX1 bis RX4 und passende Setupidentität
- per-RX-Frames: RX1 608, RX2 675, RX3 752, RX4 666
- nächstes Gate: neue 65-Sekunden-Leerraumkalibrierung erst nach frischer
  Bestätigung, dass der Raum vollständig ohne Person bleibt

Detailnachweis:
[results/2026-08-09_D6_sidecar-fix-neusiegelung-und-preflight.md](results/2026-08-09_D6_sidecar-fix-neusiegelung-und-preflight.md)

### 2026-08-09 — Neue 65-Sekunden-Leerraumkalibrierung bestanden

- Nutzerbestätigung vor dem Start: Raum bleibt während der vollständigen
  65 Sekunden ohne Person
- Aufnahme `empty-neutral-20260809-02`: 6.102 Frames, 0 Drops, sauber
  abgeschlossen und exakt an `setup-2beda4496ccfb547` gebunden
- per-RX-Frames: RX1 1.436, RX2 1.557, RX3 1.635, RX4 1.474
- identisches stabiles 64-Subcarrier-Raster bei RX1 bis RX4
- strikte Offline-Inspektion mit dem versiegelten Release bestanden
- Signal-SHA-256
  `2c4882012ce8bf2eba9cd98830c8bfe07e1597e0223f2a50fc44d353edcbdb3d`
- Server danach operational mit vier frischen D5-/D6-Referenzen und vier
  nutzbaren Liveknoten; bei der Abschlussabfrage keine RX-Präsenzstimme
- RX3 verwarf 55 bewegungsverdächtige Kalibrierframes, erzeugte aber eine
  vollständige Sechs-Block-Referenz; keine Parameteranpassung vorgenommen
- nächstes Gate: echte Trainingsaufnahme an P01

### 2026-08-09 — Lokale UI mit versiegeltem Server verbunden

- alte UI-Adresse auf Port 3000 erwartete Docker-API/WS auf 3000/3001 und
  zeigte deshalb irreführend Offline beziehungsweise Simulation
- lokalen Same-Origin-Proxy `RuView/scripts/run_local_sensing_ui.mjs` auf Port
  3002 gestartet; Weiterleitung nur an lokalen HTTP-Port 8080 und WS-Port 8765
- kein Neustart und keine Änderung von Messserver, Siegel oder Kalibrierung
- Dashboard sichtbar `HEALTHY`, Quelle `ESP32`; Sensing sichtbar
  `LIVE — ESP32 HARDWARE`, `Connected`, vier aktive RX
- Position weiterhin korrekt `UNCALIBRATED`, bis der reale P01-bis-P09-Index
  gebaut und blind validiert ist

### 2026-08-09 — Spiegelverkehrte UI-Raumansicht korrigiert

- versiegelte TX-/RX-Koordinaten geprüft und unverändert gelassen
- erster Versuch mit 180° Kameraazimut als unzureichend verworfen: Rotation
  korrigiert keine Spiegelung
- endgültig nur die UI-X-Achse mit `x_display = Raumlänge - x` gespiegelt;
  Höhe und Z-Koordinate bleiben unverändert
- dieselbe Darstellungstransformation gilt für TX, RX, Positionskörper und
  Signalfeld; gespeicherte und versiegelte Koordinaten bleiben unverändert
- live sichtbar geprüft: RX2/RX4 links, RX1/RX3 rechts, TX und Raster in
  derselben korrigierten Raumansicht
- Messserver, Setup-Siegel, Leerraumreferenz und Rohdaten nicht verändert

### 2026-08-14 — mmWave-Hardware angeschlossen, WLAN-Gate noch offen

- ESP32-C3 über `/dev/cu.usbmodem1101` read-only identifiziert: Revision 0.4,
  4 MB Flash und USB Serial/JTAG; die rohe MAC wird nicht veröffentlicht
- vorhandene App `esp32-mmwave-node` Version `0.1.0` aus `ota_0` gebootet;
  Firmwarestart im Kalibrierungsmodus bestätigt
- geflashte App hat ELF-Präfix `66529d679`; sie unterscheidet sich vom
  aktuellen lokalen Build mit SHA-256
  `f72af68fb505f6355851941baf7c656d29aea05face8e09ed1aaec105a9ab086`
- keinerlei Flash-, OTA- oder Konfigurationsschreibzugriff durchgeführt
- Nutzer bestätigte angeschlossenen HLK-LD2450 und laufenden TX
- Mac war bei der Prüfung im Heimnetz auf `192.168.178.121`, nicht im
  CSI-Subnetz `192.168.4.0/24`
- `csi-test` ist als bevorzugtes WLAN gespeichert; der explizite
  Verbindungsversuch meldete jedoch `Could not find network csi-test`
- ESP initialisiert den WLAN-STA-Modus, erhält aber keine IP; deshalb werden
  HTTP-/UDP-Transport und die nachgelagerte UART-Radarschleife noch nicht
  gestartet
- LD2450-UART, Radarpositionen und Datenempfang am Mac sind damit noch nicht
  live nachgewiesen
- durch die neu hinzugekommene mmWave-Hardware und Verkabelung ist der
  endgültige Aufbau noch nicht als Setup v2 versiegelt
- nächstes Gate: TX-SoftAP `csi-test` wieder sichtbar machen, Mac und ESP ins
  CSI-Netz bringen und danach WLAN, UART-Radar und UDP getrennt verifizieren

### 2026-08-14 — mmWave-Knoten im CSI-Netz erreichbar

- Nutzer verband den Mac mit dem wieder sichtbaren TX-SoftAP `csi-test`
- TX unter `192.168.4.1`, ESP32-C3 unter `192.168.4.2` und Mac zunächst per
  DHCP unter `192.168.4.3` im selben `/24`-Netz nachgewiesen
- ESP verband sich auf Kanal 6 per WPA2-PSK mit dem TX; gemeldeter RSSI beim
  Start `-78 dBm`
- ESP erhielt seine IP und startete den HTTP-Status-/Modus-/OTA-Dienst auf
  Port `8032`
- read-only Statusabfrage erfolgreich: Knoten `MMWAVE1`, Sensor
  `HLK-LD2450`, Modus `calibration`, UART RX GPIO20, TX GPIO21 bei 256000 Baud
- laufende Firmware sendet Radarframes fest an `192.168.4.50:5010`; diese
  reservierte Adresse war im Netz bei der Vorprüfung unbeantwortet
- Versuch, `192.168.4.50` als Alias zu setzen, scheiterte ausschließlich an
  den macOS-Administratorrechten; es wurde keine Netzwerkkonfiguration
  verändert
- nächstes Gate: Administrator setzt die dokumentierte Mac-Adresse
  `192.168.4.50`; danach echtes LD2450-UDP-Paket empfangen und validieren

### 2026-08-14 — CSI-Zieladresse aktiv, noch keine LD2450-Frames

- Mac-Adresse `192.168.4.50/24` zusätzlich zu DHCP-Adresse `192.168.4.3`
  erfolgreich auf `en0` bestätigt
- ESP-Ziel `192.168.4.50:5010` und Statusdienst weiterhin erreichbar
- erster UDP-Empfangsversuch mit `nc` über 15 Sekunden ohne Paket
- unabhängiger, direkt an `192.168.4.50:5010` gebundener Socket-Empfänger mit
  10-Sekunden-Timeout ebenfalls ohne Paket
- Netzadressierung und Listenerfehler damit als primäre Ursache weitgehend
  ausgeschlossen
- der LD2450 erzeugt auch ohne erkanntes Ziel fortlaufend gültige Nullziel-
  Frames; fehlende Person oder Bewegung erklärt den Befund daher nicht
- nächstes Gate: Sensorversorgung und UART-Verbindung TX des LD2450 zu GPIO20
  prüfen; erst danach Radar-UDP erneut testen

### 2026-08-14 — Ursache der fehlenden Radarframes geklärt

- Nutzer korrigierte die vorherige Anschlussangabe: Der HLK-LD2450 war bei
  den UDP-Prüfungen tatsächlich nicht mit dem PCB verbunden
- die ausbleibenden Pakete sind damit erwartbares Verhalten und kein Nachweis
  für einen Defekt von Sensor, PCB oder UART
- ESP-WLAN, Statusdienst und Zieladressierung bleiben separat erfolgreich
  nachgewiesen
- nächster sicherer Schritt: ESP/PCB vollständig stromlos machen, LD2450 an
  PCB-01 anschließen, erst danach wieder mit Strom versorgen und den UDP-Test
  wiederholen

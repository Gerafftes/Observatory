# Observatory Control Center — Setup- und Experimentleitfaden

Stand: 2026-08-29

Dieses Dokument beschreibt den vollständigen Ablauf im Experiment-Cockpit —
zuerst ohne angeschlossene Hardware, danach mit ESP32-CSI-Nodes und LD2450-
mmWave-Referenz.

## 1. Grundregel: Demo ist kein Messbeweis

Das Cockpit trennt drei Dinge strikt:

1. WiFi-CSI ist der Eingang des WiFi-Modells.
2. mmWave liefert eine unabhängige Kalibrierungs- und Truth-Referenz.
3. Ein Software-only- oder Simulationslauf darf niemals als echte Sensor-,
   Positions- oder Modellvalidierung ausgegeben werden.

Ein Demo-Run muss deshalb sichtbar als SOFTWARE-ONLY / UNVALIDATED markiert
bleiben. Der Radar darf nicht heimlich in die WiFi-Prediction eingehen.

## 2. Was ohne Hardware möglich ist

Ohne ESP32-CSI-Nodes und ohne LD2450 können vollständig geprüft werden:

- Raumprofil mit Länge, Höhe, Breite und Koordinaten
- TX-/RX-Positionen und Profilversionen
- Run-Erstellung und Profilversionierung
- Guide-Navigation und Statusanzeigen
- SQLite-Run-Historie
- Fehler- und Offline-Zustände
- Software-only-Demo des Phasenablaufs
- Artefakt- und Report-Navigation

Nicht als bestanden gelten ohne Hardware:

- echte leere CSI-Baseline
- echte RX-Paket- und Loss-Werte
- mmWave-UART-, Radarframe- und UDP-Streaming
- CSI↔Radar-Zeitsynchronisierung
- reale Positionskalibrierung
- echte Blindaufnahmen
- Accuracy, Coverage, Fehlerdistanz oder Live-Position

## 3. Cockpit öffnen

Bei einem lokalen simulierten Sensing-Server:

    cd RuView/v2
    cargo build -p wifi-densepose-sensing-server --no-default-features
    ../../target/debug/sensing-server \
      --source simulated \
      --tick-ms 100 \
      --ui-path ../../ui \
      --http-port 3000

Danach öffnen:

    http://127.0.0.1:3000/ui/index.html#sensing

Für einen echten Hardwarelauf wird der Server mit --source esp32 gestartet.
Das darf erst nach dem Hardware- und Transportcheck geschehen.

## 4. Schritt-für-Schritt-Ablauf

### Schritt 1 — Setup-Profil

Im Bereich Setup-Profil eintragen:

- Raum: [Länge, Höhe, Breite] in Metern
- TX: [x, y, z]
- RX1 bis RX4: [x, y, z]
- mmWave / HLK-LD2450: [x, y, z] des festen Montagepunkts

Die Koordinaten müssen im gleichen Raumkoordinatensystem liegen. Der mmWave-Marker ist im CAD-Plan anklickbar, ziehbar und über den Inspector editierbar. Im CAD-Inspector kann **„mmWave darf außerhalb des Raums montiert werden“** deaktiviert werden. Dann gilt für mmWave ausschließlich der Innenraum; TX/RX behalten unabhängig davon den eingestellten Außenradius. Ist die Option aktiv, dürfen X/Z – wie bei TX/RX – innerhalb des Außenradius liegen; Y bleibt immer innerhalb der Raumhöhe. Danach:

Für den Abstand zwischen zwei Wänden im CAD-Inspector beide
gegenüberliegenden Wände auswählen: **Wand links (X = 0) + Wand rechts (X = L)**
setzt die **Raumlänge `L`**; **Wand oben (Z = 0) + Wand unten (Z = B)** setzt die
**Raumbreite `B`**. Danach den angezeigten **Raumabstand** setzen. Benachbarte
Wände bilden keinen eindeutigen Raumabstand; beim Verkleinern blockiert der
Editor Werte, durch die ein TX-, RX- oder mmWave-Marker außerhalb von Raum
beziehungsweise Außenradius läge.

1. Profilnamen eintragen.
2. Raum- und Node-Positionen prüfen.
3. **mmWave-Position berechnen** klicken. Die Empfehlung wird in die mmWave-Felder und den CAD-Plan übernommen; bei aktivem Innenraum-only bleibt sie innerhalb des Raums.
4. Setup-Profil speichern beziehungsweise Neue Profilversion speichern
   klicken.

Das optionale P01–P09-Raster befindet sich absichtlich eingeklappt unter
Legacy / Kontrolltest. Es ist nicht der reguläre mmWave-Kalibrierungsweg.

### Schritt 2 — Experiment-Run anlegen

Der Guide springt zu Experiment-Run anlegen.

1. Versuchsname eintragen.
2. Das gespeicherte Setup-Profil auswählen.
3. WiFi-Experiment anlegen klicken.

Der Run erhält eine eigene ID und übernimmt den Profil-Hash.
Jede echte Änderung erzeugt dabei eine unveränderliche Profilversion; ein
identisches erneutes Speichern erzeugt keine zusätzliche Version.

### Schritt 3 — Run mit dem aktiven Setup-v2 versiegeln

Nach dem Speichern des CAD-Profils übernimmt **Setup-v2-Entwurf laden** Raum,
TX, RX1–RX4, mmWave-Montagepunkt, Kanal und Umgebungsrevisionen aus genau der
gespeicherten Profilversion. Diese Werte müssen nicht ein zweites Mal
eingetragen werden. Der Entwurf bleibt absichtlich nicht versiegelbar, bis die
verifizierten Firmware-, Grid-, TX-Filter-, Recording-Host- und
mmWave-Transform-Identitäten ergänzt wurden.

Der Button bleibt gesperrt, solange der Server ohne `--position-setup` läuft.
Für einen echten Lauf zuerst eine vollständige Setup-v2-Spezifikation mit den
verifizierten Firmware-, Grid-, TX-Filter-, mmWave- und Server-Identitäten
erzeugen, mit demselben Server-Binary versiegeln und den Server mit genau
diesem Artefakt neu starten. Danach im Guide Setup versiegeln klicken.

Der Server prüft dabei die gemeinsam vorhandenen Profilfelder gegen das aktive
Runtime-Seal und hält anschließend fest:

- Profil-ID
- Profil-Hash
- Run-ID
- Setup-ID
- Setup-Hash
- Seal-Typ `active_position_setup_v2`

Ein Profil-Hash allein kann die Phase nicht mehr als bestanden markieren. Das
Runtime-Seal bindet die dokumentierte Identität, ersetzt aber weiterhin nicht
die anschließenden Live-Gates für RX-Transport, Synchronisation und Radar.

### Schritt 4 — Leere WiFi-Baseline

Für einen echten Lauf:

1. **Leerkalibrierung vorbereiten** klicken.
2. Messdauer festlegen (mindestens **60 Sekunden**) und den Vorlauf bis zum Start wählen.
3. **Countdown starten** klicken und den Raum während des Countdowns verlassen.
4. Raum während der gewählten Messdauer leer halten. Die Kalibrierung startet und endet automatisch.
5. Nur wenn der automatische Abschluss nicht bestätigt werden kann, **Leerkalibrierung abschließen** klicken.

Wenn für dieselbe Profilgeometrie und dasselbe versiegelte Setup bereits eine
gespeicherte Baseline existiert, zeigt der Guide vorher **Gespeicherte Baseline
verwenden** an. Diese Option lädt die geprüften D5/D6-Referenzen ohne neue
Messung und protokolliert die Phase als `REUSED`. Änderungen an TX, RX, Raum,
mmWave-Montage, Firmware, CSI-Grid oder TX-Filter machen die Baseline bewusst
ungültig; reine Profil-/P01–P09-Benennungen oder Kontrollraster-Änderungen
ändern den Baseline-Kontext nicht.

Die Referenzen liegen als gehashte Calibration-Bundles unter
`data/calibrations/`; die SQLite-Katalogdaten binden sie zusätzlich an
Profilkontext und Setup-Seal.

Die Anzeige nennt jederzeit die verbleibenden Sekunden bis zum Start beziehungsweise
bis zum automatischen Ende sowie die absolute Uhrzeit **„Sicher zurück ab“**.
Der Vorlauf ist bewusst getrennt von der Messdauer: Während des Vorlaufs werden
noch keine CSI-Daten für die Baseline gesammelt.

Ohne CSI-Nodes wird dieser Schritt erwartungsgemäß nicht bestanden. Typische
Meldung:

    Only 0 D6 RX fingerprints are usable; at least 3 required.

Diese Sperre ist korrekt. Man darf sie nicht mit einem Demo-Status umgehen und
danach von einer echten Baseline sprechen.

### Schritt 5 — mmWave-geführte Kalibrierung

Der Guide bietet mmWave-Assistent öffnen an.

Der Assistent erwartet:

- einen erreichbaren ESP32-C3-mmWave-Node
- gültige LD2450-Frames
- UDP-Pakete am Sensing-Server
- einen passenden Raum-Transform
- zeitlich zuordenbare Radarpositionen

Ohne Sensor bleibt der Status beispielsweise:

    Auf das erste gültige Radar-Paket warten
    No mmWave packet has been received.

Die kontinuierlichen Radarpositionen werden getrennt von den CSI-Daten
gespeichert. Das manuelle Abarbeiten von P01–P09 ist dabei nicht erforderlich.

### Schritt 6 — Blind-Reihenfolge

Nach erfolgreicher Kalibrierung wird eine reproduzierbare Blind-Reihenfolge
erzeugt. Der Seed wird im Run gespeichert.

Die Ground Truth bleibt während der Aufnahme verborgen. Die Position, an der
die Testperson steht, darf nicht in die Prediction-Datei oder in versteckte
Capture-Metadaten gelangen.

### Schritt 7 — Blindaufnahmen

Für jede Blindaufnahme:

1. Guide-Anweisung beachten.
2. CSI-Aufnahme starten.
3. Position beziehungsweise Radarreferenz nicht in den WiFi-Eingang geben.
4. Aufnahme stoppen.
5. Erst nach vollständiger Aufnahme zur nächsten Anweisung wechseln.

Ohne echte CSI-Pakete sind die Dateien unvollständig und dürfen nicht als
Blindtestdaten gelten.

### Schritt 8 — Prediction registrieren

Nach vollständigen Blindaufnahmen wird die Prediction-Datei erzeugt und im
Guide über den relativen Pfad unter data/ registriert.

Die Prediction enthält ausschließlich die WiFi-Modellvorhersagen. Der Server
prüft und speichert den SHA-256-Hash.

### Schritt 9 — Truth aufdecken

Erst nachdem Prediction registriert wurde, wird die getrennte Truth-Datei
registriert. Sie enthält die echte Positionsreferenz, bei einem validierten
mmWave-Lauf also die separat gespeicherte Radarwahrheit.

Vor diesem Schritt darf Truth nicht zur Prediction oder zum Training gelangen.

### Schritt 10 — Evaluation

Die Evaluation vergleicht Prediction und Truth. Erwartete Kennzahlen sind:

- Accuracy
- Coverage
- Fehlerdistanz
- Confusion Matrix
- unknown und ambiguous
- Qualitätsgates

Ohne echte Aufnahmen und Truth sind diese Werte nur Demo-Metadaten.

### Schritt 11 — Report

Der Report fasst Setup, Artefakt-Hashes, Run-Verlauf und Evaluation zusammen.

Ein Software-only-Report muss ausdrücklich enthalten:

    SOFTWARE-ONLY / UNVALIDATED

Er ist ein Test des Workflows, kein Nachweis für Sensor- oder Modellqualität.

## 5. Software-only-Durchlauf

Für die UI-Demonstration kann ein eigener Run mit einem eindeutigen Namen wie

    SOFTWARE-ONLY / NO HARDWARE walkthrough

verwendet werden. Dieser Run darf den kompletten Guide bis zum Report zeigen.

Dabei muss jeder künstlich abgeschlossene Phasenübergang mit einem Payload wie
software_only: true oder einer gleichwertigen Demo-Markierung versehen sein.

Der Demo-Run zeigt den Ablauf, beweist aber nicht:

- dass CSI empfangen wurde
- dass der LD2450 funktioniert
- dass die Radarposition stimmt
- dass das WiFi-Modell korrekt lokalisiert
- dass ein Blindtest bestanden wurde

## 6. Hardware-Ankunft: geordnete Fortsetzung

Wenn der Sensor angeschlossen wird, in dieser Reihenfolge vorgehen:

1. Strom trennen.
2. LD2450 TX mit ESP32-C3 GPIO20 verbinden.
3. ESP32-C3 GPIO21 mit LD2450 RX verbinden.
4. Gemeinsame Masse herstellen.
5. Versorgung mit 5 V und ausreichender Stromreserve prüfen.
6. UART auf 256000 Baud, 8N1 prüfen.
7. ESP32-C3 booten und Parser-/Firmwarestatus prüfen.
8. /ota/status beobachten: UART-Bytes, gültige Radarframes und UDP-Zähler
   müssen steigen.
9. Im UI erst bei streaming mit Transform- und Raumprüfung fortfahren.
10. Radar-Transform mit mehreren bekannten Bodenmarkierungen prüfen.
11. Setup-v2 mit Node-ID, Raumkoordinaten und Transform versiegeln.
12. CSI-RX-Nodes verbinden und TX-Bindung, CSI-Rate, Paketverlust und Sync
    prüfen.
13. Danach Leerkalibrierung, mmWave-Kalibrierung, Blindtest und Evaluation
    durchführen.

Wenn die RX-Firmware für dieselbe kontrollierte TX-Quelle mehrere gültige
Symbolformen liefert, kann der Server vor dem Setup-Seal explizit auf das
verifizierte Discovery-Grid begrenzt werden. Für den aktuellen Kanal-6-Aufbau
lautet der Startparameter:

    --csi-grid-pin 2437,1,64,0,0

Der Pin bindet Frequenz, Antennenanzahl, Subcarrier-Anzahl, PPDU-Typ und
Layout-Flags. Off-Grid-Frames halten die Source-Binding-Attestierung frisch,
werden aber vor Preflight, Aufnahme, D5/D6 und Live-Position verworfen. Der Pin
ist nur eine reversible Vorstufe und kann nicht zusammen mit
`--position-setup` verwendet werden; danach sind ausschließlich die im
Setup-v2 versiegelten RX-Grids maßgeblich.

Ein erfolgreicher Flash oder ein erfolgreicher Parser-Test beweist weder
WiFi-Transport noch Radar-Streaming oder Positionsgenauigkeit.

## 7. Status- und Fehlerinterpretation

| Anzeige | Bedeutung | Nächste Aktion |
| --- | --- | --- |
| CONTROL CENTER READY | Backend und Cockpit-Daten erreichbar | Workflow kann bedient werden |
| CONTROL CENTER OFFLINE | Sensing-Server nicht erreichbar | Server starten oder Verbindung prüfen |
| 0/4 Nodes | Keine aktiven CSI-Nodes | RX/TX/UDP-Transport prüfen |
| Wartet auf mmWave | Keine gültigen Radar-UDP-Pakete | Versorgung, UART, Parser und UDP prüfen |
| uart_idle | Keine UART-Bytes vom LD2450 | Versorgung, TX/RX-Leitung, GPIO und Baudrate prüfen |
| invalid_frames | UART-Bytes vorhanden, aber Frames ungültig | Baudrate und Leitung prüfen |
| udp_blocked | Radarframes gültig, aber kein UDP vom ESP | Zielserver, WLAN und UDP-Konfiguration prüfen |
| streaming | UART, Parser und UDP liefern Daten | Transform- und Sync-Prüfung beginnen |
| SOFTWARE-ONLY / UNVALIDATED | Demo-/Simulationslauf | Keine Hardwarequalität daraus ableiten |

## 8. Abnahmekriterien für einen echten Lauf

Der Lauf ist erst als real validiert zu betrachten, wenn alle folgenden Punkte
vorliegen:

- vier erwartete RX-Nodes sind aktiv
- TX-Quelle ist attestiert
- CSI-Rate und Paketverlust sind plausibel
- Zeit-/Mesh-Sync ist gültig
- leere WiFi-Baseline ist bestanden
- mmWave liefert gültige und frische Radarframes
- Raum-Transform wurde physisch geprüft
- CSI und Radar sind zeitlich synchronisiert
- Kalibrierung ist vollständig
- Prediction und Truth wurden getrennt verarbeitet
- Blindtest besteht die definierten Qualitätsgates

Bis dahin bleiben Live-Position und Modellqualität gesperrt oder als
UNVALIDATED markiert.

## 9. Aktueller Implementierungsstand

Das Cockpit enthält inzwischen:

- einen kontextabhängigen Guide für Vorbereitung und alle zehn Workflow-Phasen
- direkte Sprünge zu Profil-, Run- und Artefaktfeldern
- direkten Sprung zum mmWave-Assistenten
- eingeklapptes P01–P09-Raster als Legacy-/Kontrolltest-Fallback
- sichtbare `READY`, `RUNNING`, `PASS`, `OFFLINE` und `UNVALIDATED`-Zustände
- getrennte Software-only-Demo-Runs
- erklärende Info-Symbole mit Hover- und Tastaturhinweisen

Der Browser-Durchlauf wurde ohne angeschlossene Hardware mit einem simulierten
Server geprüft. Die UI-Regressionstests liefen zuletzt mit 68 bestandenen
Tests. Das bestätigt den Softwareablauf, nicht die Funk- oder Radarqualität.

## 10. Relevante Dateien und Tests

| Datei | Zweck |
| --- | --- |
| ui/components/ObservatoryControlCenter.js | Cockpit, Guide und Workflow-Aktionen |
| ui/components/MmwaveCalibrationAssistant.js | Radarstatus, Kalibrierungsreferenz und Transportdiagnose |
| ui/services/experiment.service.js | Profile, Runs, Phasen und Artefakte |
| v2/crates/wifi-densepose-sensing-server/src/mmwave_calibration.rs | mmWave-Status und Kalibrierungslogik |
| v2/crates/wifi-densepose-sensing-server/src/calibration_persistence.rs | Gehashte, kontextgebundene D5/D6-Baselines |
| v2/crates/wifi-densepose-sensing-server/src/experiment.rs | SQLite-Workflow und Phasenpersistenz |
| ui/observatory/LIVE_DATA_CONTRACT.md | Vertrauensgrenzen für Live-Rendering |

UI-Regressionstests aus dem Repository:

    node --test ui/tests/*.test.mjs
    node --test ui/observatory/tests/*.test.mjs
    git diff --check

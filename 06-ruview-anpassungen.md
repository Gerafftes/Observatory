# 06 RuView-Anpassungen und lokale Patches

Dieses Projekt verwendet [ruvnet/RuView](https://github.com/ruvnet/RuView) als Softwarebasis für RX-Firmware und Sensing-Server.

Der RuView-Quellcode selbst bleibt ein separates Upstream-Repository. Dieses BLL-Repo dokumentiert nur, welche lokalen Änderungen im Projektverlauf vorbereitet oder getestet wurden.

## Lokale Firmware-Idee: HTTP-Config-Endpunkt

Motivation:

- Die RX-Knoten mussten mehrfach neu provisioniert werden, weil sich die Mac-IP im `CSI_SSID`-Netz geändert hatte.
- OTA-Status war über WLAN erreichbar, der OTA-Upload war aber ohne PSK gesperrt (`403 Forbidden`).
- Ziel war, spätere Änderungen wie `target_ip`, `node_id`, `edge_tier` oder `csi_channel` ohne USB per WLAN setzen zu können.

Vorbereitete lokale Änderung:

- `config_server.c`
- `config_server.h`
- Registrierung in `main.c` auf dem bestehenden HTTP-/OTA-Server
- Beispiel-Endpoint: `POST /config?target_ip=CSI_HOST_IP&reboot=1`

Status:

- Lokal vorbereitet, aber nicht auf die ESPs ausgerollt.
- OTA-Upload der aktuellen Firmware ist ohne gültigen OTA-PSK gesperrt.
- Für Einsatz wäre einmaliges USB-Flashen oder korrekt provisionierter OTA-PSK nötig.

## Lokale OTA-Größenkorrektur

Motivation:

- Der OTA-Status meldete eine künstliche `max_size` von 921.600 Byte.
- Das vorhandene Firmware-Binary war größer.
- Die reale OTA-Partition ist größer als diese künstliche Grenze.

Vorbereitete lokale Änderung:

- `ota_update.c` verwendet die reale Größe der nächsten OTA-Partition.
- Firmware-Upload wird nur abgelehnt, wenn das Binary größer als die reale OTA-Partition ist.

Status:

- Lokal vorbereitet, nicht auf ESPs ausgerollt.

## Server-Parameter für Visualisierung

Für die 4RX-Tests wurde der RuView-Server mit größerem Guard-Intervall gestartet:

```zsh
WDP_GUARD_INTERVAL_US=500000
WDP_SOFT_GUARD_US=200000
```

Beobachtung:

- Standard: 60 ms hard / 20 ms soft
- Testwert: 500 ms hard / 200 ms soft
- Mit 500 ms traten im G2-Serverlog nur 31 Fusion-Fallbacks bei 16.097 ESP32-Frame-Zeilen auf.

Einordnung:

- Nützlich für stabilere Webvisualisierung.
- Kein Ersatz für echte zeitliche Synchronisation.
- Für wissenschaftliche Auswertung muss dieser Parameter als methodische Einschränkung dokumentiert werden.

## Node-Positionen

RuView unterstützt:

```zsh
--node-positions "x,y,z;x,y,z;..."
```

Diese Geometrie ist für eine stabile räumliche Visualisierung hilfreich. Für die eigentliche Problemfrage dieser Arbeit ist sie aber nicht zwingend erforderlich, solange primär Bewegung, Signalveränderung und Atemband-Reaktion untersucht werden.

Einordnung:

- Ohne Node-Positionen: Bewegung/Signalreaktion kann trotzdem untersucht werden.
- Mit Node-Positionen: bessere Voraussetzung für Heatmap/Positionsanzeige.
- Die Webansicht wird aktuell nicht als exakter Messwert verwendet.

## Lokale Diagnose- und Klassifikationsänderungen vom 2026-07-18

Die folgenden Änderungen liegen im separaten lokalen RuView-Arbeitsbaum. Sie sind hier dokumentiert, aber nicht Bestandteil dieses Dokumentations-Repositories:

- Raummaße und TX-Position zusätzlich zu den RX-Positionen an die UI übertragen
- TX/RX-Marker beschriftet und räumlich nach den gemessenen Koordinaten dargestellt
- Service-Worker-Cache aktualisiert, damit geänderte UI-Dateien tatsächlich geladen werden
- Wolkenziel mit `0,45 m` Deadzone, `0,25 m` Zielstabilität und `1,5 s` Bestätigungszeit stabilisiert
- Kalibrierungsfeed für die Zustände `Uncalibrated` und `Collecting` aktiviert
- 128-/192-Werte für das 56-dimensionale Einzel-Link-Kalibrierungsmodell normalisiert
- adaptives Klassifikationsmodell unter `70 %` Trainingsgenauigkeit abgelehnt; vorhandenes Modell hatte nur rund `41,5 %`
- zeitlichen Bewegungsvergleich auf den tatsächlich vorherigen Frame korrigiert
- statische CSI-Merkmale aus dem unmittelbaren Bewegungsscore entfernt bzw. zur Signalleistung normalisiert
- globale Klasse als Konsens der aktiven RX statt aus dem zuletzt eingetroffenen Paket gebildet
- Feldenergie für `present_moving` von einem Defaultwert `0,05` auf `0,55` korrigiert; `present_still` liegt bei `0,30`
- diagnostische per-RX-Werte für Rohscore, geglätteten Score und Ruhe-Baseline ergänzt

Validierung:

- sieben neue Klassifikations-/Bewegungs-Unit-Tests bestanden
- Release-Build des Sensing-Servers erfolgreich
- vorhandenes 41,5-%-Modell wurde beim Serverstart nachweislich verworfen
- Live-Vergleich still/bewegt zeigte trotzdem starke Überlappung der Rohwerte

Einordnung:

Die lokalen Änderungen beheben nachweisbare Implementierungsfehler, liefern aber noch keinen belastbaren Klassifikator. Insbesondere die UI-Deadzone darf nicht als verbesserte Messgenauigkeit interpretiert werden. Der nächste technische Prüfpunkt liegt in der RX-seitigen Auswahl vergleichbarer Pakete des kontrollierten TX.

## D4: skaleninvariante Bewegungsmetrik und kontaminierter E0-Versuch vom 2026-07-26

Nach Aktivierung des TX-MAC-Filters auf RX1 bis RX4 wurde die zeitliche Bewegungsmetrik auf RMS-normalisierte Frame-Vergleiche umgestellt. Auch die Varianz im Zeitfenster wird auf normalisierten Frames berechnet. Dadurch sollen reine Verstärkungs- und Pegelsprünge nicht mehr wie Körperbewegung gewertet werden.

Validierung:

- 9 gezielte D4-Tests bestanden
- 188 Tests des Sensing-Server-Binaries bestanden
- Release-Build erfolgreich
- geplanter 60-Sekunden-Leerraumlauf mit 237 Samples, vier durchgehend vorhandenen RX und keinen Logger-Fehlern; nachträglich als Mischlauf markiert, weil der Raum zweimal kurz betreten wurde

Ergebnis:

- Vor D4: in den ersten 60 Sekunden des vergleichbaren Leerraumlaufs 124-mal `ACTIVE`, 111-mal `PRESENT_MOVING`, zweimal `PRESENT_STILL` und kein `ABSENT`
- Nach D4: kein globales `ACTIVE` oder `PRESENT_MOVING`, 129-mal `ABSENT` und 108-mal `PRESENT_STILL`
- RX1 blieb zu 100 % `ABSENT`
- RX2 bis RX4 erzeugten weiterhin zeitweise lokale Still-Fehlklassifikationen

Einordnung:

Der Mischlauf deutet darauf hin, dass D4 die groben Bewegungs-Fehlalarme weitgehend reduziert. Eine gültige Leerraum-Fehlerrate kann daraus wegen der zwei Raumzutritte nicht berechnet werden. Die globale Aggregation setzt weiterhin `PRESENT_STILL`, sobald mindestens ein RX diese Klasse meldet. Vor einer Anpassung dieser Regel werden zuerst ein sauberer E0-Leerraumlauf und anschließend ein Positivlauf mit still sitzender Person benötigt.

Ausführliche Auswertung: [results/2026-07-26_D4-E0_leerraum.md](results/2026-07-26_D4-E0_leerraum.md)

### Gültige E0b-Wiederholung

Der vollständig leere 60-Sekunden-Lauf E0b widerlegt die Annahme, dass die verbliebenen `PRESENT_STILL`-Meldungen des ersten Mischlaufs nur von den zwei Raumzutritten stammten:

- 237 Samples, vier durchgehend vorhandene RX, keine Logger-Fehler
- global 218-mal `PRESENT_STILL` (92,0 %) und 19-mal `ABSENT` (8,0 %)
- RX1: 100,0 % `ABSENT`
- RX2: 12,2 % lokale Still-Präsenz
- RX3: 39,7 % lokale Still-Präsenz
- RX4: 84,4 % lokale Still-Präsenz und einmal `PRESENT_MOVING`

D4 unterdrückt damit grobe Bewegungs-Fehlalarme, aber nicht die Anwesenheits-Fehlalarme. Die globale Still-Aggregation wirkt als ODER-Verknüpfung: Ein einzelner RX genügt. Vor einer Änderung wird ein Positivlauf mit still sitzender Person benötigt, um die Trennbarkeit der Score-Verteilungen zu prüfen.

Ausführliche Auswertung: [results/2026-07-26_D4-E0b_sauberer-leerraum.md](results/2026-07-26_D4-E0b_sauberer-leerraum.md)

### E0c: Einfluss des Mac-Standorts

Nach mittigem Aufstellen des Macs wurde E0b unter ansonsten gleichen Bedingungen wiederholt:

- RX4: Fehlpräsenz von 84,8 % auf 0,0 %
- RX4 Raw Mean: von 0,121 auf 0,027
- RX4 Smoothed Mean: von 0,062 auf 0,001
- RX2 und RX3: praktisch unveränderte Fehlpräsenz
- global: Fehlpräsenz von 92,0 % auf 46,8 %

Der Mac-Standort ist damit für RX4 ein nachgewiesener starker Einflussfaktor. Ob der Effekt durch Funkaktivität oder durch das geänderte Multipfadfeld von Gehäuse und Kabeln entsteht, ist mit diesem einzelnen A/B-Wechsel noch nicht getrennt. Für folgende Tests bleibt der Mac mittig.

Ausführliche Auswertung: [results/2026-07-26_E0b-E0c_mac-position-ab-test.md](results/2026-07-26_E0b-E0c_mac-position-ab-test.md)

### E1: Link-spezifische Trennung stiller Anwesenheit

Der Vergleich zwischen E0c und einer 60 Sekunden still sitzenden Person zeigt stark unterschiedliche Link-Reaktionen:

- RX1: keine positive Reaktion
- RX2: keine nutzbare Trennung
- RX3: Smoothed Mean von 0,037 auf 0,051
- RX4: Smoothed Mean von 0,001 auf 0,036, deskriptive AUC 0,982

Bei RX4 blieb der erhöhte Wert bis zum Ende der Aufnahme bestehen. Eine vorläufige RX4-Schwelle von 0,01 lag bei 3,0 % der Leerraum- und 83,5 % der Still-Samples überschritten. Die aktuelle 0,04-Klasse verliert einen großen Teil dieser Empfindlichkeit.

Konsequenz: Die Präsenzlogik sollte nicht alle RX mit einer identischen Schwelle und ODER-Verknüpfung behandeln. Benötigt werden per-RX-Leerraumreferenzen, eine Fusion der relativen Abweichungen und eine zeitliche Mindestdauer. Der Schwellenkandidat muss mit einem neuen Laufpaar bestätigt werden.

Ausführliche Auswertung: [results/2026-07-26_E0c-E1_still-person-separation.md](results/2026-07-26_E0c-E1_still-person-separation.md)

### E0d/E1b: unabhängige Prüfung verwirft RX4-Festschwelle

Im zweiten Leerraum-/Still-Paar blieb RX4 in beiden Zuständen vollständig unter 0,01. Der sehr gute RX4-Effekt aus E0c/E1 war damit nicht reproduzierbar und darf nicht als feste Klassifikationsregel übernommen werden.

RX3 zeigte dagegen in beiden Paaren einen Anstieg des geglätteten Minutenmittels:

- E0c → E1: 0,037 → 0,051
- E0d → E1b: 0,027 → 0,055

RX2 demonstrierte zugleich Link-Instabilität: In E0d meldete es im leeren Raum in 83,5 % der Samples Präsenz. Die nächste Serveränderung muss deshalb per-RX-Referenzen, längere Zeitfenster und eine Zuverlässigkeitsbewertung kombinieren. Eine feste RX4-Schwelle wird nicht implementiert.

Ausführliche Auswertung: [results/2026-07-26_E0d-E1b_unabhaengige-bestaetigung.md](results/2026-07-26_E0d-E1b_unabhaengige-bestaetigung.md)

## D5: experimentelle Leerraumreferenz und RX-Quorum

Auf Basis von E0c/E1 und E0d/E1b wurde ein reproduzierbarer Offline-Replayer eingeführt. Der D5-Kandidat lernt ausschließlich aus Leerraumdaten:

- sechs vollständige, nicht überlappende 10-Sekunden-Blöcke pro RX
- Median und robuste Skala `max(1,4826 × MAD; 0,005)`
- kausales 10-Sekunden-Livefenster
- RX-Stimme bei `z > 1`
- mindestens zwei absolute RX-Stimmen
- zwei Sekunden Ein-/Ausschaltpersistenz

Leakage-sicheres Ergebnis der vertauschten Laufpaare:

- mittlere Leerraum-Fehlpräsenz: 0,0 %
- mittlerer Still-Recall: 89,3 %
- mittlere Balanced Accuracy: 94,7 %

Der strengere Vergleichskandidat `z > 3` wurde wegen nur 15,5 % Still-Recall verworfen. Ein überwachter RX-Selektor wurde ebenfalls verworfen, weil seine mittlere Leerraum-Fehlpräsenz im Cross-Fold 20,8 % betrug.

Die lokale RuView-Integration bleibt ausdrücklich experimentell und wird erst durch eine separate Klassifikationskalibrierung aktiviert:

```text
POST /api/v1/classification/calibration/start
POST /api/v1/classification/calibration/stop
GET  /api/v1/classification/calibration/status
```

Die vorhandene `/api/v1/calibration/*`-Schnittstelle bleibt dem SVD-FieldModel vorbehalten. D5 verlangt sechs vollständige Kalibrierblöcke, 20 Samples je Block, drei frische Referenzen und mindestens 5 Hz tatsächlich akzeptierte D5-Samples. Ein Wechsel des Subcarrier-Rasters verwirft die Referenz des betroffenen RX. Eine Unterbrechung akzeptierter Samples von mindestens einer Sekunde verwirft das Livefenster; danach sind wieder vollständige zehn Sekunden nötig. Evidenz- oder Nodeverlust löscht eine zuvor gesetzte Still-Präsenz. Ohne D5-Kalibrierung bleibt das bisherige D4-Verhalten unverändert.

Vor dem physischen Livetest bestanden 709 Rust-Tests, 7 Python-Replayer-Tests, der Release-Build und ein isolierter API-Lebenszyklustest. Der abschließende unabhängige Code-Audit gab D5 für den kontrollierten Livetest frei. Eine reale erfolgreiche D5-Kalibrierung ist damit noch nicht vorweggenommen.

Der Offline-Replay ist positiv, aber wegen nur zwei Laufpaaren, einer Sitzung und einer Sitzposition noch kein Produktionsnachweis. Die Parameter werden vor den nächsten blinden Leerraum-/Still-Läufen eingefroren.

Ausführliche Auswertung: [results/2026-07-26_D5_offline-replay-und-experimentelle-praesenzkalibrierung.md](results/2026-07-26_D5_offline-replay-und-experimentelle-praesenzkalibrierung.md)

### Reale D5-Validierung

Nach dem positiven Offline-Replay wurde die separate D5-Kalibrierung im realen Aufbau erfolgreich abgeschlossen. Im anschließenden Still-Livetest waren für alle vier RX die Referenz und die laufende Evidenz verfügbar.

Ergebnis:

- 236 Samples beziehungsweise 59,7 Sekunden still sitzende Person: global 236-mal `ABSENT`
- weitere 114 Samples beziehungsweise 29,9 Sekunden still sitzende Person: global 114-mal `ABSENT`
- erster Abschnitt: nur RX4 stimmte zeitweise für Präsenz, 87 von 236 Samples
- zweiter Abschnitt: RX3 stimmte in 114 von 114 Samples, RX2 nur einmal
- das Zwei-RX-Quorum wurde nie erfüllt
- globaler Still-Recall über beide Aufnahmen: 0,0 %

Einordnung:

Die technische Fail-closed-Regel arbeitete wie vorgesehen, weil eine einzelne RX-Stimme nicht als globale Präsenz ausgegeben wurde. Die reale Klassifikationsleistung ist dennoch nicht ausreichend: Der informative Funkpfad wechselte zwischen RX4 und RX3, sodass die echte Person vollständig übersehen wurde. Das positive Offline-Replay ist damit nicht auf die neue Aufnahme übertragbar.

D5 bleibt experimentell und wird nicht als Standard aktiviert. Das Quorum wird ohne neuen zugehörigen Leerraumlauf nicht gelockert, da eine Ein-RX-Regel die bereits gemessenen Leerraum-Fehlalarme wieder zulassen könnte.

Ausführliche Auswertung: [results/2026-07-26_D5_realer-still-livetest.md](results/2026-07-26_D5_realer-still-livetest.md)

## D6 und diskrete Positionspipeline vom 2026-07-29

Nach dem nicht generalisierenden D5-Livetest wurde die nächste Änderung nicht
als weitere einzelne Schwellenkorrektur aufgebaut. Die lokale RuView-Arbeit
trennt jetzt drei Ebenen:

1. verlustfreie, aufbaugebundene Rohdatenerfassung
2. D6-Präsenzmerkmale relativ zu einer robusten Leerraumreferenz
3. eine davon getrennte Positionsklassifikation für neun feste Punkte

### D6-Fingerprint

D6 vergleicht nicht nur einen skalaren Bewegungswert. Für jeden RX und jedes
gültige CSI-Raster wird die gain-normalisierte Subcarrier-Form mit einer
Leerraumreferenz verglichen. Die Referenz enthält außerdem eine stabile
Bin-Maske, damit verrauschte Subcarrier nicht als künstliche Nullevidenz in die
Positionsmerkmale gelangen.

Wichtig gegenüber D5:

- signierte Residuen bleiben erhalten
- positive und negative Abweichungen können Veränderung tragen
- RSSI- und CSI-RMS-Abweichung werden getrennt erfasst
- ein Rasterwechsel kann nicht stillschweigend dieselbe Referenz
  weiterverwenden

D6 ersetzt damit die einseitige Regel `z > 1` durch reichhaltigere
CSI-Formmerkmale. Eine erfolgreiche reale Presence-Classification ist dadurch
noch nicht bewiesen.

### Verlustfreie Aufnahmen

Der Server kann validierte ESP32-UDP-Frames verlustfrei als Raw-CSI-JSONL
speichern. Erfasst werden unter anderem:

- RX-ID und Zeitstempel
- Frequenz, Antennen- und Subcarrierzahl
- Sequenz, RSSI, Noise Floor, PPDU-Typ und Layout-Flags
- jedes signierte I/Q-Paar in ursprünglicher Reihenfolge

Zu jeder Aufnahme entsteht ein Sidecar. Für die Positionspipeline muss es eine
Setup-ID und einen Setup-Hash, die feste Geometrie und den Serverstand binden.
Raw- und Sidecar-Schema lehnen unbekannte Felder ab. Positionsaufnahmen werden
verworfen, wenn sie ein eingebettetes Label oder eine Ground Truth enthalten.

### Neun feste Punkte statt kontinuierlicher Scheinpräzision

Die Ortung verwendet P01 bis P09 aus
[`00-status-und-annahmen.md`](00-status-und-annahmen.md). Pro Drei-Sekunden-
Fenster entstehen 28 Merkmale je RX. Erforderlich sind:

- RX1 bis RX4
- mindestens 5 Hz
- mindestens 15 Frames je RX und Fenster
- höchstens eine Sekunde Lücke
- ausreichende gemeinsame zeitliche Abdeckung
- identisches, erwartetes Subcarrier-Raster

Sechs unabhängige Fünf-Sekunden-Blöcke je Punkt bilden robuste
Median-Prototypen. Die vier RX werden gleich gewichtet. Neben P01 bis P09 sind
ausdrücklich `unknown` für außerhalb der bekannten Verteilung und `ambiguous`
für nicht ausreichend getrennte Kandidaten vorgesehen.

Eine Position wird nicht zwischen Punkten interpoliert. Der Ansatz soll zuerst
beweisen, ob der feste Aufbau grobe Raumbereiche reproduzierbar unterscheiden
kann.

### Getrennte Offlinebefehle

```text
--position-inspect <empty-calibration|position>
--position-build-index <TRAINING_MANIFEST>
--position-predict <POSITION_INDEX>
--position-evaluate <PREDICTIONS>
```

Der Workflow ist absichtlich blind:

1. `inspect` prüft Aufnahme und Sidecar und erzeugt manifestfähige Hashes.
2. `build-index` darf Trainingspfade und Punktlabels lesen.
3. `predict` akzeptiert ausschließlich Index und ungelabelte Blindaufnahmen.
4. `evaluate` liest erst danach die separat gespeicherte Wahrheit.

Rohdatei-, Sidecar- und Signalabschnitt-Hashes verhindern, dass
Trainingsaufnahmen versehentlich im Blindtest wiederverwendet werden. Die
Ausgabe wird atomisch in eine neue Datei geschrieben; vorhandene Artefakte
werden nicht überschrieben.

### Sicherheitsverhalten der Sensing-Ansicht

Die bestehende Heatmap bleibt eine Diagnoseansicht und ist kein Nachweis einer
gemessenen Personposition. Bei ESP32-Daten darf die UI keine Person aus einer
künstlichen Feldspitze ableiten. Ohne gültige Position, ohne Präsenz oder bei
veralteten Daten werden Personposition und Pose geschlossen entfernt. Die
TX-/RX-Marker bleiben sichtbar, weil sie den fest vermessenen Aufbau zeigen.

### Prüfstatus

Implementiert:

- D6-Referenz und signierte Projektion
- Raw-Recorder und Replay
- Positionsfenster und Qualitätsgates
- robuster 9-Punkt-Klassifikator
- Inspection, Indexbau, blinde Vorhersage und getrennte Auswertung
- Leakage- und No-Clobber-Sicherungen
- kanonische, gehashte Setupbindung für Geometrie, Geräte- und Funkstand
- fail-closed Live-Positionskern mit D6-Gate, 4-aus-5-Konsens und
  Raw-CSI-Veraltungsgrenze
- ehrliche Sensing-Darstellung ohne künstliche Position bei fehlender Evidenz

Automatisiert bestanden:

- dateibasierter End-to-End-Test mit 65 Sekunden Leerraum, neun Trainings- und
  neun getrennten Blindaufnahmen
- vollständiger Weg `inspect → build-index → predict → evaluate`
- synthetisches Ergebnis `9/9` korrekt, Coverage und Accuracy `1,0`,
  Median-/p95-Fehler `0,0 m`
- vollständiger Rust-Testlauf mit `852` bestandenen, `0` fehlgeschlagenen und
  `2` absichtlich ignorierten Tests
- Sensing-UI-Test, JavaScript-Syntax, Debug-Build, echte CLI-Prüfung,
  gezieltes Rustfmt der bearbeiteten Module und `git diff --check`

Noch offen:

- endgültigen Mac-Standort und vollständiges Setup-Manifest einfrieren
- reale Leerraum-, P01-bis-P09-Trainings- und Blindaufnahmen
- vorab festgelegte Gütegrenzen mit Blinddaten prüfen
- erst danach den bestandenen realen Index in den bereits vorbereiteten
  Live-Serverpfad laden

Der aktuelle ausgeschaltete Hardwarezustand ist für diese Offlinephase
beabsichtigt. Er ist kein Hindernis für Code- und Schemavalidierung, erlaubt
aber selbstverständlich noch keine Aussage über die reale Ortungsleistung.
Ohne realen Index meldet der Livepfad absichtlich `uncalibrated` und zeigt
keine Personenposition.

Fortlaufender Wiedereinstieg:
[`08-aktueller-arbeitsstand-d6-und-position.md`](08-aktueller-arbeitsstand-d6-und-position.md)

### Abschlussaudit der Live-Ausgaben

Ein späterer Audit zeigte, dass die Sensing-Ansicht zwar bereits nur den
diskreten Positionszustand verwendete, andere RuView-Ausgaben aber noch alte
Grobdaten als scheinbare Person weiterreichen konnten. Deshalb gelten jetzt
einheitlich folgende Verträge:

- Bei aktivem Positions-Setup ist Classification bis zur fertigen
  D6-Leerraumreferenz `uncalibrated` oder `calibrating`, niemals D4-Präsenz.
- Ein Positionsmodell muss exakt P01 bis P09 enthalten.
- ESP32-`persons[]` darf nur bei bestätigter Präsenz und einem gültigen
  diskreten P01-bis-P09-Ergebnis entstehen.
- Der ESP32-Marker enthält keine prozeduralen Pose-Keypoints; Heatmap und
  Groblokalisierung bleiben reine Diagnosewerte.
- `GET /health/ready.position_setup` zeigt das aktive versiegelte Setup auch
  dann, wenn noch kein realer Index geladen ist.

Observatory zeigt den Transportzustand nicht mehr pauschal als `LIVE`.
`CONNECTING`, `LIVE ESP32`, `SIMULATED` und `STALE` werden getrennt. Ein
ESP32-Frame verfällt nach drei Sekunden lokaler Browserzeit. Im Hardwaremodus
werden nur validierte Raum-, TX- und exakt RX1-bis-RX4-Koordinaten dargestellt.
Die frühere feste Demo-Geometrie, animierte Standardfigur und Szenarioprops
bleiben der ausdrücklich beschrifteten Simulation vorbehalten. Reale Hardware
erzeugt höchstens einen neutralen statischen P01-bis-P09-Marker.

Für den späteren Hardwareübergang wurde außerdem
`scripts/capture_position_run.py` ergänzt. Der Runner verwendet ausschließlich
neutrale Aufnahme-IDs und prüft Setup, frische RX1 bis RX4, Mindestdatenrate,
verlorene Frames sowie den abgeschlossenen setupgebundenen Sidecar. Der
allgemeine Training-Tab ersetzt dieses Blindprotokoll nicht.

Die TX-Filteridentität
`sha256-ruview-tx-filter-mac-v1` ist nun eindeutig als SHA-256 über exakt die
sechs binären NVS-Bytes definiert. Provisioning und Server prüfen denselben
Testvektor; die rohe MAC muss nicht im Bericht erscheinen.

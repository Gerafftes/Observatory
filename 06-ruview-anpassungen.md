# 06 RuView-Anpassungen und lokale Patches

Dieses Projekt verwendet [ruvnet/RuView](https://github.com/ruvnet/RuView) als Softwarebasis für RX-Firmware und Sensing-Server.

Der RuView-Quellcode selbst bleibt ein separates Upstream-Repository. Dieses BLL-Repo dokumentiert nur, welche lokalen Änderungen im Projektverlauf vorbereitet oder getestet wurden.

## Lokale Firmware-Idee: HTTP-Config-Endpunkt

Motivation:

- Die RX-Knoten mussten mehrfach neu provisioniert werden, weil sich die Mac-IP im `csi-test`-Netz geändert hatte.
- OTA-Status war über WLAN erreichbar, der OTA-Upload war aber ohne PSK gesperrt (`403 Forbidden`).
- Ziel war, spätere Änderungen wie `target_ip`, `node_id`, `edge_tier` oder `csi_channel` ohne USB per WLAN setzen zu können.

Vorbereitete lokale Änderung:

- `config_server.c`
- `config_server.h`
- Registrierung in `main.c` auf dem bestehenden HTTP-/OTA-Server
- Beispiel-Endpoint: `POST /config?target_ip=192.168.4.50&reboot=1`

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

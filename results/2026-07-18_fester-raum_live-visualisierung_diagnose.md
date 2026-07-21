# Diagnose des RuView-Livetests im festen Raumaufbau

Datum des Versuchs: 2026-07-18

Dokumentiert: 2026-07-21

## Kurzfazit

Der feste Aufbau mit einem TX und vier RX war online und wurde mit realen Raumkoordinaten in RuView dargestellt. Die Live-Visualisierung war trotzdem nicht als Personenortung oder Bewegungsklassifikation verwendbar. Zwei Marker erschienen nahezu übereinander, die Punktwolke folgte einer realen Bewegung nicht zuverlässig und bewegte sich später auch beim stillen Sitzen. Die Klassen `PRESENT_STILL` und `PRESENT_MOVING` widersprachen wiederholt der beobachteten Situation.

Die Untersuchung fand mehrere konkrete Softwarefehler. Nach lokalen Korrekturen blieb jedoch ein grundlegender Datenqualitätsbefund bestehen: Die aus aufeinanderfolgenden CSI-Frames berechneten Bewegungswerte überlappten bei stiller und deutlich bewegter Person stark. Ein weiteres Verschieben fester Schwellen wäre deshalb keine belastbare Lösung.

## Aufbau

- Raum: `4,02 m × 3,44 m × 2,59 m` (Länge × Breite × Höhe)
- Hardware: `1 × ESP32-TX`, `4 × ESP32-RX`
- mmWave: bewusst noch nicht eingesetzt
- RuView-Server: lokale Release-Version, ESP32-Quelle, UDP `5005`, HTTP `8080`, WebSocket `8765`
- Testparameter für die Fusion: `WDP_GUARD_INTERVAL_US=500000`, `WDP_SOFT_GUARD_US=200000`

Verwendete RuView-Koordinaten:

| Gerät | `[x, y, z]` in m |
|---|---|
| RX1 | `[0.00, 0.50, 0.28]` |
| RX2 | `[4.02, 0.87, 0.97]` |
| RX3 | `[0.00, 0.74, 2.11]` |
| RX4 | `[4.02, 0.87, 2.46]` |
| TX | `[1.51, 1.19, 0.39]` |

## Bildnachweis

![RuView Live WiFi Sensing mit fehlerhafter Punktwolke und Klassifikation](../skizzen/screenshots/2026-07-18_18-54-33_fixed-room-live-sensing-failure.png)

SHA-256 des unveränderten Desktop-Originals und der Repository-Kopie:

```text
dfae667701e71ee685ac2856d5ba07ccd4e35489623d365dcb0f5b54bde68ad7
```

Im Screenshot sichtbar:

- Live-Verbindung zur ESP32-Hardware
- RSSI von `-48,0 dBm`
- Varianz `313,536`
- Motion Band `289,616`
- Breathing Band `663,587`
- Spectral Power `1003,121`
- Klassifikation `PRESENT_STILL` mit angezeigten `81 %`
- zwei optisch beinahe überlagerte Gerätemarker
- eine räumliche Punktwolke, deren Lage nicht als reale Personenposition validiert werden konnte

Der Screenshot beweist einen laufenden Daten- und UI-Pfad. Er beweist weder eine korrekte Personenerkennung noch eine korrekte Position.

## Beobachtete Fehler

1. Zwei farbige Gerätemarker lagen in der Ansicht nahezu übereinander, obwohl die eingetragenen physischen Koordinaten verschieden waren. Im damaligen Screenshot fehlten eindeutige Beschriftungen, weshalb die betroffenen Rollen allein aus dem Bild nicht sicher zugeordnet werden können.
2. Die Punktwolke änderte ihre Position nicht nachvollziehbar, wenn sich die Testperson bewegte.
3. Beim späteren stillen Sitzen bewegte sich die Wolke trotzdem fortlaufend.
4. Die globale Klassifikation wechselte zeitweise auf `PRESENT_MOVING`, obwohl die Testperson still saß.
5. Einzelne RX-Klassifikationen meldeten dauerhaft `present_moving` mit ungefähr `40 %`, während die globale Klassifikation zeitweise `present_still` zeigte.
6. Eine zuvor durchgeführte leere-Raum-Kalibrierung stabilisierte die Bewegungsentscheidung nicht ausreichend.

## Im Code bestätigte Ursachen

### Unzuverlässiges adaptives Modell

RuView lud automatisch `v2/data/adaptive_model.json`. Das Modell war mit `3316` Frames trainiert, hatte aber nur `0,41496` bzw. rund `41,5 %` Trainingsgenauigkeit. Es durfte trotzdem die heuristische Klassifikation überschreiben. Lokal wurde eine Mindestgenauigkeit von `70 %` eingeführt; das vorhandene Modell wird damit abgelehnt.

### Falscher zeitlicher Vergleich

Der aktuelle CSI-Frame wurde vor der Merkmalsextraktion in die Historie eingefügt. Die Bewegungsberechnung verglich ihn anschließend mit `frame_history.back()` und damit mit sich selbst. Der eigentliche zeitliche Differenzanteil war dadurch immer null. Lokal wurde auf den vorherigen Historieneintrag umgestellt.

### Statische Signalstruktur wurde als Bewegung bewertet

Absolute zeitliche Varianz, `motion_band_power` und die Zahl der Schwellenüberschreitungen wurden mit sehr niedrigen Festwerten normiert und auf `1,0` begrenzt. In diesem Aufbau lagen die realen Werte häufig weit über diesen Demo-Skalen. Dadurch konnten statische Mehrwegeigenschaften dauerhaft als Bewegung in den Score eingehen. Lokal wurde der Score auf zeitliche, zur Signalleistung normierte Änderungen begrenzt.

### Gesamtklasse stammte vom zuletzt eingetroffenen RX

Im ESP32-Pfad wurde die globale Klasse nicht aus allen vier RX gebildet. Sie entsprach der Klassifikation des zuletzt verarbeiteten UDP-Frames. Unterschiedliche Paketankunft konnte deshalb die Anzeige umschalten. Lokal wurde eine Mehrheits-/Quorumsentscheidung über alle aktiven RX ergänzt.

### Vertauschte Bewegungswirkung in der Visualisierung

Für die Feldvisualisierung wurde `active` auf `0,8` und `present_still` auf `0,3` abgebildet. `present_moving` fiel jedoch in den Default-Zweig mit `0,05`. Damit erzeugte `present_moving` weniger Feldbewegung als `present_still`. Lokal wurde `present_moving` auf `0,55` gesetzt.

### Kalibrierung nahm zunächst keine Frames an

Die Feldkalibrierung wurde zwar gestartet, aber der Feed akzeptierte den Anfangszustand nicht. Zusätzlich passten eingehende 128-/192-Werte nicht zur erwarteten 56-dimensionalen Einzel-Link-Konfiguration. Lokal wurden `Uncalibrated` und `Collecting` als Feed-Zustände akzeptiert und Frames auf 56 Werte normalisiert. Danach ließ sich die Kalibrierung mit vier Baseline-Eigenwerten abschließen.

## Lokale UI-Stabilisierung

Die Punktwolke erhielt testweise:

- Positions-Deadzone: `0,45 m`
- maximale Zielstreuung für eine bestätigte Bewegung: `0,25 m`
- Bestätigungszeit: `1,5 s`

Damit blieb das übernommene Wolkenziel beim stillen Sitzen zeitweise stabil, obwohl das rohe Ziel weiter sprang. Dies ist eine Darstellungsstabilisierung, keine Lösung des zugrunde liegenden CSI-Problems.

## Vergleich still gegen Bewegung

Nach den Softwarekorrekturen wurden pro RX diagnostische Werte für Rohbewegung, Ruhe-Baseline und geglättete Bewegung ausgegeben.

Beispiele beim stillen Sitzen:

- RX3 Rohscore: `0,084` bis `0,894`
- RX4 Rohscore: `0,074` bis `0,754`
- RX2 Rohscore: `0,077` bis `0,507`
- RX1 Rohscore: `0,048` bis `0,449`

Während deutlicher Arm- und Oberkörperbewegung wurden folgende Bereiche beobachtet:

- RX3 Rohscore: `0,078` bis `0,766`
- RX4 Rohscore: `0,062` bis `0,771`
- RX2 Rohscore: `0,059` bis `0,823`
- RX1 Rohscore: `0,060` bis `0,851`

Die Bereiche überlappten stark. Auch die geglätteten Werte trennten die Phasen nicht robust.

## Interpretation und Grenze der Aussage

Bestätigt ist, dass der bisherige Frame-zu-Frame-Score in diesem Aufbau Stillstand und Bewegung nicht trennt. Noch nicht bestätigt ist die genaue Ursache der starken Paketvariation.

Arbeitshypothesen für die nächste Untersuchung:

- Die RX-CSI-Callbacks verarbeiten Pakete mehrerer WLAN-Quellen statt ausschließlich des vorgesehenen TX.
- Vergleichbare Pakete wechseln zwischen unterschiedlichen CSI-/PPDU- oder Subcarrier-Rastern.
- Unsynchronisierte Paketankunft und Rasterwechsel dominieren die körperbedingte Änderung.

Diese Punkte müssen in Firmware und Rohdaten geprüft werden, bevor weitere Schwellen oder ein neues Modell festgelegt werden.

## Nächste Schritte

1. In der RX-Firmware prüfen, welche Absender-MACs und Pakettypen den CSI-Callback erreichen.
2. RX-seitig ausschließlich Pakete des kontrollierten TX und ein stabiles CSI-Raster zulassen.
3. Paketquelle, PPDU-Typ, Subcarrier-Anzahl und Sequenz in einer kurzen Diagnoseaufnahme protokollieren.
4. Danach neue gelabelte Aufnahmen erstellen: leerer Raum, still sitzende Person, deutliche Bewegung.
5. Nur bei messbarer Trennung eine adaptive Baseline oder ein neues Klassifikationsmodell trainieren.
6. Leere-Raum-Kalibrierung erneut etwa 60 Sekunden durchführen.
7. Erst dann Wolkenstabilität und Positionsänderung im festen Aufbau erneut bewerten.

## Berichtsfähiges Ergebnis

Der Versuch zeigt, dass ein vollständiger 1TX-/4RX-Datenpfad, reale Sensorgeometrie und eine visuell plausible Punktwolke noch keine valide Bewegungserkennung ergeben. Ohne Kontrolle der Paketquelle und zeitlich vergleichbarer CSI-Frames können Netzwerk- und Signalvariationen stärker sein als die durch eine Person verursachte Änderung. Negative Ergebnisse der Live-Visualisierung müssen daher getrennt von der grundsätzlichen Funktionsfähigkeit des CSI-Datenempfangs bewertet werden.

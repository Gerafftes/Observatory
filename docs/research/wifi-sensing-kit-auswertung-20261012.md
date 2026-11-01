# Nutzen von wifi sensing kit für das Observatory Tracking

Stand: 12. Oktober 2026. **G05 liefert eine brauchbare Vorlage für geführte Datensammlung, einfache CSI-Merkmale und den Vergleich zusätzlicher Funkstrecken.** Besonders relevant ist Johanns Vorschlag, auch zwischen den RX zu messen. Kontinuierliche Positionen und Mehrpersonen-Tracking müssen wir weiterhin selbst nachweisen. Die Umsetzung bleibt auf vier RX und einen dedizierten TX ausgerichtet; eine gesonderte Variante kann die RX zusätzlich als Sender nutzen.

**Aktuelle Projektentscheidung:** Johann hat RX↔RX am 12. Oktober zum festen Entwicklungsziel gemacht: RX1↔RX4 umsetzen und testen; bei nachgewiesenem Positionsnutzen auf alle vier RX erweitern. Die Offline-Machbarkeitsanalyse ist fertig, Implementierung und Messnachweise stehen aus. Diese Entscheidung ersetzt die frühere Einordnung als bloßer optionaler Versuch; sie erteilt keine Hardwarefreigabe.

## Gebundene Quelle und Aussagekraft

Geprüft wurde [andrisgauracs/wifi-sensing-kit](https://github.com/andrisgauracs/wifi-sensing-kit) bei Commit [`7fb77fc0c0adb98bcf0a102d68fa8718b2f98625`](https://github.com/andrisgauracs/wifi-sensing-kit/tree/7fb77fc0c0adb98bcf0a102d68fa8718b2f98625), Commitdatum 11. Oktober 2026. Die Bewertung beruht auf den öffentlich gelesenen Text- und Quelldateien, nicht auf einer Ausführung oder Hardwaremessung. Im geprüften Baum sind keine zugehörigen Messdatensätze oder automatisierten Tests enthalten. Eine unabhängige Reproduktion der berichteten Leistung liegt hier nicht vor.

Das [README](https://github.com/andrisgauracs/wifi-sensing-kit/blob/7fb77fc0c0adb98bcf0a102d68fa8718b2f98625/README.md) beschreibt einen Raum, eine Person und drei ESP32-S3. Die genannten 79,5 % beziehen sich auf acht Bewegungs-/Anwesenheitsklassen mit Kameralabels, nicht auf Bodenkoordinaten. Ein Umzug oder eine geänderte Aufstellung erfordert dort erneutes Training. Ein Mehrpersonen-Nachweis wird daraus nicht abgeleitet.

Der Raumkartenpfad klassifiziert kalibrierte Orte. `Calib.estimate()` mittelt deren Koordinaten anhand der Klassenstimmen. Dass dabei eine gleitende Koordinate erscheint, belegt keine Ortungsleistung an unbekannten Zwischenpositionen. Für unser Ziel bleiben kontinuierlich gelabelte Wege und getrennte Blindwege erforderlich. Quelle: [server.py, Calib](https://github.com/andrisgauracs/wifi-sensing-kit/blob/7fb77fc0c0adb98bcf0a102d68fa8718b2f98625/kit/server.py#L261-L323).

## Sinnvolle Übernahmen

| Ansatz | Nutzen für Observatory | Konkrete Einordnung |
|---|---|---|
| Ein anderer Sensor erzeugt Trainingslabels; WiFi schätzt später eigenständig | Passt zu unseren beiden mmWave-Sensoren und der getrennten Referenzauswertung | Radarqualität, Zeitfehler und Referenzlücken ausdrücklich speichern. Die Webcam wird kein zusätzliches Projektziel. |
| Getrennte Trainings- und Testsitzung, einfache Baselines, Vergleich einzelner Links | Früh feststellen, ob räumliche Information vorhanden ist und welcher RX hilft | Echte Sitzungs-IDs für Training, Validierung und Blindtest. Skalierung, Masken, Modellwahl und Lagkorrektur ausschließlich aus zulässigen Trainings-/Validierungsdaten. Alle vier RX bleiben für die reguläre Abnahme nötig. |
| Normierte Amplitudenform sowie zeitliche Streuung und Änderungen | Kleine, nachvollziehbare Merkmalsfamilie für R6 | Bestehende RMS-Normalisierung und Leerraumprojektion zuerst verwenden. Höchstens eine gezielte Erweiterung an geeigneten Bins vergleichen; kein pauschaler Wechsel auf die Mittelwert-Normalisierung des Kits. |
| Geführte kurze Aufnahmeabschnitte mit Zähler gespeicherter Fenster | Verhindert, dass ein abgelaufener Countdown als erfolgreiche Kalibrierung gilt | Fortschritt anhand gültiger Vier-RX-Fenster mit gültiger Referenz und räumlicher Abdeckung anzeigen. Gründe für Ausschlüsse getrennt; nur fehlende Trainingsbereiche nacherheben. Blinddaten bleiben eingefroren. |
| Linkweise Empfangsrate und festgehaltene Linkmenge | Nützlich für RX-Beitrag, Platzierung und spätere RX↔RX-Variante | Identität und Qualität pro physischem Sender→Empfänger-Paar. Ein Ausfall darf nicht durch andere frische Links verborgen werden. |

Belege: [Methodenbeschreibung](https://github.com/andrisgauracs/wifi-sensing-kit/blob/7fb77fc0c0adb98bcf0a102d68fa8718b2f98625/docs/how-it-works.md), [evaluate.py](https://github.com/andrisgauracs/wifi-sensing-kit/blob/7fb77fc0c0adb98bcf0a102d68fa8718b2f98625/kit/evaluate.py) und [geführte Aufnahme](https://github.com/andrisgauracs/wifi-sensing-kit/blob/7fb77fc0c0adb98bcf0a102d68fa8718b2f98625/kit/web/movement.html#L281-L356). Die Übertragung auf Koordinaten und Radare ist unsere Planung, keine im Kit belegte Leistung.

## CSI zwischen den RX

RX bezeichnet bei uns eine Projektrolle; dieselben ESP32-S3 können zusätzlich Funkpakete senden. CSI entsteht an dem RX, der einen geeigneten Frame empfängt. Wenn RX2 sendet und RX3 diesen Funkframe erfasst, entsteht eine Messung der Strecke RX2→RX3. Der Rückweg RX3→RX2 ist eine weitere gerichtete Beobachtung; beide Richtungen sind nicht zwangsläufig unabhängige Positionsinformation.

| Topologie | Theoretische gerichtete Strecken zu unseren vier RX | Einschränkung |
|---|---|---|
| Dedizierter TX zu vier RX | 4 | Ausgangsbasis; aktuelle Hardwarequalität noch gesondert zu bestätigen |
| Zusätzlich alle RX untereinander | 12 zusätzlich: 4 × 3, also insgesamt 16 | Geometrische Möglichkeiten, keine Zusage tatsächlich nutzbarer Strecken oder vervierfachter Datenrate |
| Zunächst zwei RX zusätzlich senden lassen | Zwei gegenseitige Richtungen gezielt untersuchen | Kleiner Vergleich vor einer möglichen Ausweitung; übrige RX weiter als Qualitätskontrolle |

Die zwölf zusätzlichen Richtungen entsprechen sechs RX-Paaren. Es werden keine weiteren Boards benötigt; TX1, RX1–RX4, Mac und die Radarreferenz für einen späteren Positionsvergleich genügen grundsätzlich. Physische Hörbarkeit, Kanal, PHY-/CSI-Layout und Aufnahmebudget müssen trotzdem geprüft werden. TX1 wird dadurch nicht automatisch zum fünften Empfänger.

Im Kit werden Router- und Board-Sender über eine MAC-Liste zugelassen und dem Host zugeordnet. Der [Firmwarepatch](https://github.com/andrisgauracs/wifi-sensing-kit/blob/7fb77fc0c0adb98bcf0a102d68fa8718b2f98625/firmware/ruview-v0.8.8-kit.patch) fügt dafür Metadaten hinzu, aber keinen eigenen koordinierten Peer-Sendeplan. Die [Methodenbeschreibung](https://github.com/andrisgauracs/wifi-sensing-kit/blob/7fb77fc0c0adb98bcf0a102d68fa8718b2f98625/docs/how-it-works.md) benennt ein gemeinsames Aufnahmebudget von ungefähr 50 Frames/s je Board. Mehr Links teilen sich dieses Budget.

**Lokaler Quellbefund:** In [csi_collector.c](../../software/ruview/firmware/esp32-csi-node/main/csi_collector.c) verwirft der aktive Ein-MAC-Filter Frames anderer Sender vor dem Ratenbegrenzer (Zeilen 447–462). Der Standardwert des gemeinsamen Limits ist 50 Hz (Zeilen 125–140). Der vorhandene optionale Self-Ping erzeugt Gatewayverkehr (ab Zeile 491). Das beweist weder seinen aktuellen Gerätestatus noch bereits nutzbare Peer-CSI. Ein IP-Paket von RX2 an RX3 kann außerdem über den AP weitergeleitet werden: Der Funkabschnitt mit dem AP als Sender ist kein Beleg für RX2→RX3-CSI. Maßgeblich ist der physische 802.11-Sender, nicht die IP-Quelladresse.

Eine spätere Variante braucht daher einen begrenzten Senderkreis, sichere Quellenzuordnung und ein Gesamtbudget pro RX. Sie darf den bestehenden Herkunftsschutz nicht abschalten. Für einen physischen Sender bekannte Funkheader sind noch keine verifizierte TX-Anwendungssequenz. Metadaten dürfen nicht nach Zeitnähe erfunden werden. Ohne passende verifizierte Sendersequenz lässt sich eine linkweise Paketverlustquote nicht seriös aus der globalen RX-Sequenz ableiten: Deren Zwischennummern können zu anderen Links gehören. R3 und die getrennte Verlustbilanz bleiben erhalten.

## Codegrenzen bei einer Übernahme

Die folgenden Befunde begrenzen eine direkte Wiederverwendung. Sie stellen keinen neuen Fehlernachweis für unsere laufenden Geräte dar.

| Befund im geprüften Kit | Beleg | Regel für unsere Umsetzung |
|---|---|---|
| Erste 64 IQ-Paare und feste 52-Bin-Auswahl; für ältere Frames Senderannahme aus Paketlänge | `server.py` Zeilen 26–39 und 82–100 | Keine Ableitung von Sender oder Layout allein aus Länge. R1/R4 pro tatsächlichem Layout, LTF und Gültigkeitsmetadaten. Original-IQ behalten. |
| Je Link nach den ersten 20 Frames Auswahl der häufigsten Länge; vorhandener Puffer wird beim Wechsel nicht als neue Kalibrierung behandelt | `Link.add()`, Zeilen 43–59 | Länge ersetzt keine Konfigurationsidentität. Verschiedene Layouts getrennt behandeln; Wechsel invalidieren betroffene Fenster und Modelle. |
| `Calib.train()` skaliert mit allen Runden vor dem Rundenvergleich; Bewegungs-Rundenvergleich skaliert mit allen Trainingsrunden | Zeilen 270–281 und 451–474 | In jedem Validierungsfold Skalierung nur auf dessen Trainingsanteil fitten. Der separate Bewegungstest skaliert korrekt nur anhand von `train`; diese Pfade unterscheiden. |
| Fehlende Linkmerkmale können bis zu drei Sekunden aus dem Cache kommen | `feature()` und `move_feature()` | Keine alten Merkmale als frische Vier-RX-Evidenz verwenden. Pro Link Alter/Abdeckung prüfen und `unknown` ausgeben. |
| Fenster verwenden `time.time()`; Schätzung und fortgeschriebene Sechserhistorie werden im SSE-Handler mit etwa 15 Hz aufgerufen | Zeilen 86, 192–203, 212, 303–323 und 549–563 | R7: monotone Zeit, deterministische Schätzticks und zeitbasierte Filter. Browserzahl oder Renderfrequenz darf den Schätzzustand nicht verändern. Offlinevergleich muss denselben zeitlichen Pfad nutzen. |
| Firmware setzt Byte 19 Bit 7 und hängt sechs MAC-Bytes an den CSI-Payload | `ruview-v0.8.8-kit.patch` | Keinen Patch auf unsere v2/v3-Trailer anwenden. Zuerst kompatiblen Hostvertrag festlegen; bestehende reservierte Bits und Herkunftsprüfung erhalten. |

Quellbelege: [server.py](https://github.com/andrisgauracs/wifi-sensing-kit/blob/7fb77fc0c0adb98bcf0a102d68fa8718b2f98625/kit/server.py) und [Firmwarepatch](https://github.com/andrisgauracs/wifi-sensing-kit/blob/7fb77fc0c0adb98bcf0a102d68fa8718b2f98625/firmware/ruview-v0.8.8-kit.patch). Espressif beschreibt in der [S3-CSI-Dokumentation v5.4.1](https://docs.espressif.com/projects/esp-idf/en/v5.4.1/esp32s3/api-guides/wifi.html#wi-fi-channel-state-information), dass bei `first_word_invalid` die ersten vier CSI-Bytes ungültig sind; eine feste DC-/Guard-Maske ersetzt diese Prüfung nicht.

Im lokalen [d6_fingerprint.rs](../../software/ruview/v2/crates/wifi-densepose-sensing-server/src/d6_fingerprint.rs) bestehen bereits RMS-Normalisierung mit Maske und Leerraumprojektion. [position_capture.rs](../../software/ruview/v2/crates/wifi-densepose-sensing-server/src/position_capture.rs) setzt aktuell vier RX, drei Sekunden Fenster und eine Sekunde Schritt voraus. Das Kit ist kein Anlass, diese Gates still zu ändern. Neue kürzere Fenster erhalten einen eigenen geprüften Vertrag. Für einen einfachen kontinuierlichen Ausgang bleibt höchstens ein kleiner Koordinatenregressor plus ein begründeter Vergleichskandidat vorgesehen; die Ortsklassifikation des Kits allein erfüllt diesen Vertrag nicht.

## Entscheidung und Einordnung in den Termin

1. **Bis 13. Oktober, offline:** Den kleinsten RX1↔RX4-Pfad auf dem vorhandenen Datenvertrag implementieren und mit Quellen-/Layout-/Budgetfällen prüfen. Die Machbarkeitsbewertung nicht als neue allgemeine Planungsrunde wiederholen. Hardwareinstallation bleibt gesondert freizugeben.
2. **14. Oktober, Reserve 15. Oktober:** Den verpflichtenden RX1↔RX4-Vergleich nach eigener Freigabe in 20–30 Minuten durchführen. Physische Quellen, beide Funkrichtungen, gültige Frames, Fehler, Datenalter und Gesamtbudget belegen. Fehlende Bereitschaft bleibt eine offene Terminabweichung; Radar-/Positionsarbeit läuft parallel.
3. **16.–18. Oktober:** Ausgangsbasis und Paarprofil mit geprüfter Radarreferenz, gleicher Raumkonfiguration, ausgewiesen gleichem Ressourcenbudget und ganzen getrennten Sitzungen vergleichen. Positionsmaß/Mindestnutzen vor Datenaufnahme einfrieren. Besseres gewähltes Positionsmaß ohne schlechtere Coverage/Latenz und mit weiter erfüllter Vier-RX-Basis nachweisen; reine Framezahl reicht nicht. Fehlende Daten sind kein negativer Befund.
4. **Bei positivem Nutzen 19.–20. Oktober:** Peerbetrieb auf RX1–RX4 erweitern, gerichtete Linkmatrix und Quellen-/Raten-/Positionsregression prüfen. Neue Topologie erhält eigene Daten, Referenzen und Modellartefakte; Freeze am 21. Oktober nur mit passendem Nachweis. Bei belegtem fehlendem Nutzen Nichtübernahme begründen. Kein automatisch freigegebenes ESP-NOW-/Heimnetzexperiment.

G05 ergänzt die vorhandenen Quellen: P10 liefert weiterhin eine Rastermodell-Baseline, RF-Pose das Prinzip fremdsensorischer Trainingslabels und Pilot Leerraum-/Linkvergleiche. Keine davon belegt unseren konkreten Mehrpersonenaufbau. Das feste RX↔RX-Entwicklungsziel erhöht den Termindruck; es lockert die Positions-/Datenkriterien vom 25./27. Oktober nicht.

Der [aktuelle Gesamtplan](../project/tracking-zeitplan-20261025.md) enthält die festen Entwicklungsschritte und die nach der abgeschlossenen RX2-Wartung noch offenen CSI-/Peer-Gates. Die Quellenanalyse selbst umfasst keine Geräteaktionen. Bei tatsächlicher Übernahme von Quellcode sind MIT-Hinweis und die [Drittkomponentenhinweise](https://github.com/andrisgauracs/wifi-sensing-kit/blob/7fb77fc0c0adb98bcf0a102d68fa8718b2f98625/NOTICE.md) zu erhalten.

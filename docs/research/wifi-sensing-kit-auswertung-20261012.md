# Nutzen von wifi sensing kit für das Observatory Tracking

Stand: 12. Oktober 2026. **G05 wurde als Vorlage für geführte Datensammlung, einfache CSI-Merkmale und den Vergleich zusätzlicher Funkstrecken eingeordnet.** Die folgende Analyse gibt den damaligen Prüfstand wieder. Kontinuierliche Positionen und Mehrpersonen-Tracking müssen für Observatory unabhängig nachgewiesen werden.

Die damalige Entwicklungsentscheidung sah zuerst RX1↔RX4 vor; eine Erweiterung auf vier RX war an nachgewiesenen Positionsnutzen gebunden. Die Quellenanalyse dokumentierte eine Offline-Machbarkeitsbewertung, keine ausgeführte Implementierung oder Hardwaremessung. Sie erteilt keine Gerätefreigabe.

## Gebundene Quelle und Aussagekraft

Geprüft wurde [andrisgauracs/wifi-sensing-kit](https://github.com/andrisgauracs/wifi-sensing-kit) bei Commit [`7fb77fc0c0adb98bcf0a102d68fa8718b2f98625`](https://github.com/andrisgauracs/wifi-sensing-kit/tree/7fb77fc0c0adb98bcf0a102d68fa8718b2f98625), Commitdatum 11. Oktober 2026. Die Bewertung beruht auf den öffentlich gelesenen Text- und Quelldateien, nicht auf einer Ausführung oder Hardwaremessung. Im damals geprüften Baum waren keine zugehörigen Messdatensätze oder automatisierten Tests enthalten. Eine unabhängige Reproduktion der berichteten Leistung lag der Bewertung nicht zugrunde; sie wurde für diese öffentliche Fassung nicht nachgeholt.

Das [README](https://github.com/andrisgauracs/wifi-sensing-kit/blob/7fb77fc0c0adb98bcf0a102d68fa8718b2f98625/README.md) beschreibt einen Raum, eine Person und drei ESP32-S3. Die genannten 79,5 % beziehen sich auf acht Bewegungs-/Anwesenheitsklassen mit Kameralabels, nicht auf Bodenkoordinaten. Ein Umzug oder eine geänderte Aufstellung erfordert dort erneutes Training. Ein Mehrpersonen-Nachweis wird daraus nicht abgeleitet.

Der Raumkartenpfad klassifiziert kalibrierte Orte. `Calib.estimate()` mittelt deren Koordinaten anhand der Klassenstimmen. Dass dabei eine gleitende Koordinate erscheint, belegt keine Ortungsleistung an unbekannten Zwischenpositionen. Für unser Ziel bleiben kontinuierlich gelabelte Wege und getrennte Blindwege erforderlich. Quelle: [server.py, Calib](https://github.com/andrisgauracs/wifi-sensing-kit/blob/7fb77fc0c0adb98bcf0a102d68fa8718b2f98625/kit/server.py#L261-L323).

## Sinnvolle Übernahmen

| Ansatz | Nutzen für Observatory | Konkrete Einordnung |
|---|---|---|
| Ein anderer Sensor erzeugt Trainingslabels; WiFi schätzt später eigenständig | Passt zu unseren beiden mmWave-Sensoren und der getrennten Referenzauswertung | Radarqualität, Zeitfehler und Referenzlücken ausdrücklich speichern. Die Webcam wird kein zusätzliches Projektziel. |
| Getrennte Trainings- und Testsitzung, einfache Baselines, Vergleich einzelner Links | Früh feststellen, ob räumliche Information vorhanden ist und welcher RX hilft | Echte Sitzungs-IDs für Training, Validierung und Blindtest. Skalierung, Masken, Modellwahl und Lagkorrektur ausschließlich aus zulässigen Trainings-/Validierungsdaten. Alle vier RX bleiben für die reguläre Abnahme nötig. |
| Normierte Amplitudenform sowie zeitliche Streuung und Änderungen | Kleine, nachvollziehbare Merkmalsfamilie für die Merkmalsauswertung | Bestehende RMS-Normalisierung und Leerraumprojektion zuerst verwenden. Höchstens eine gezielte Erweiterung an geeigneten Bins vergleichen; kein pauschaler Wechsel auf die Mittelwert-Normalisierung des Kits. |
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

**Lokaler Quellbefund vom 12. Oktober 2026:** Die damalige Analyse von [csi_collector.c](https://github.com/Gerafftes/Observatory/blob/028f9d3f2703e1f3610e856f48fa0a709647818b/software/ruview/firmware/esp32-csi-node/main/csi_collector.c) beschrieb einen Ein-MAC-Filter vor dem Ratenbegrenzer, ein gemeinsames Standardlimit von 50 Hz und optionalen Self-Ping für Gatewayverkehr. Der Link zeigt den Quellpfad im Projekt und bindet keinen unveränderten heutigen Quellstand. Der damalige Befund beweist weder einen aktuellen Gerätestatus noch nutzbare Peer-CSI. Ein IP-Paket von RX2 an RX3 kann über den AP weitergeleitet werden: Ein Funkabschnitt mit dem AP als Sender belegt kein RX2→RX3-CSI. Maßgeblich ist der physische 802.11-Sender, nicht die IP-Quelladresse.

Eine spätere Variante braucht daher einen begrenzten Senderkreis, sichere Quellenzuordnung und ein Gesamtbudget pro RX. Sie darf den bestehenden Herkunftsschutz nicht abschalten. Für einen physischen Sender bekannte Funkheader sind noch keine verifizierte TX-Anwendungssequenz. Metadaten dürfen nicht nach Zeitnähe erfunden werden. Ohne passende verifizierte Sendersequenz lässt sich eine linkweise Paketverlustquote nicht seriös aus der globalen RX-Sequenz ableiten: Deren Zwischennummern können zu anderen Links gehören. Die getrennte Verlustbilanz bleibt erhalten.

## Codegrenzen bei einer Übernahme

Die folgenden Befunde begrenzen eine direkte Wiederverwendung. Sie stellen keinen neuen Fehlernachweis für unsere laufenden Geräte dar.

| Befund im geprüften Kit | Beleg | Regel für unsere Umsetzung |
|---|---|---|
| Erste 64 IQ-Paare und feste 52-Bin-Auswahl; für ältere Frames Senderannahme aus Paketlänge | `server.py` Zeilen 26–39 und 82–100 | Keine Ableitung von Sender oder Layout allein aus Länge. Prüfung pro tatsächlichem Layout, LTF und Gültigkeitsmetadaten. Original-IQ behalten. |
| Je Link nach den ersten 20 Frames Auswahl der häufigsten Länge; vorhandener Puffer wird beim Wechsel nicht als neue Kalibrierung behandelt | `Link.add()`, Zeilen 43–59 | Länge ersetzt keine Konfigurationsidentität. Verschiedene Layouts getrennt behandeln; Wechsel invalidieren betroffene Fenster und Modelle. |
| `Calib.train()` skaliert mit allen Runden vor dem Rundenvergleich; Bewegungs-Rundenvergleich skaliert mit allen Trainingsrunden | Zeilen 270–281 und 451–474 | In jedem Validierungsfold Skalierung nur auf dessen Trainingsanteil fitten. Der separate Bewegungstest skaliert korrekt nur anhand von `train`; diese Pfade unterscheiden. |
| Fehlende Linkmerkmale können bis zu drei Sekunden aus dem Cache kommen | `feature()` und `move_feature()` | Keine alten Merkmale als frische Vier-RX-Evidenz verwenden. Pro Link Alter/Abdeckung prüfen und `unknown` ausgeben. |
| Fenster verwenden `time.time()`; Schätzung und fortgeschriebene Sechserhistorie werden im SSE-Handler mit etwa 15 Hz aufgerufen | Zeilen 86, 192–203, 212, 303–323 und 549–563 | Monotone Zeit, deterministische Schätzticks und zeitbasierte Filter. Browserzahl oder Renderfrequenz darf den Schätzzustand nicht verändern. Offlinevergleich muss denselben zeitlichen Pfad nutzen. |
| Firmware setzt Byte 19 Bit 7 und hängt sechs MAC-Bytes an den CSI-Payload | `ruview-v0.8.8-kit.patch` | Keinen Patch auf unsere v2/v3-Trailer anwenden. Zuerst kompatiblen Hostvertrag festlegen; bestehende reservierte Bits und Herkunftsprüfung erhalten. |

Quellbelege: [server.py](https://github.com/andrisgauracs/wifi-sensing-kit/blob/7fb77fc0c0adb98bcf0a102d68fa8718b2f98625/kit/server.py) und [Firmwarepatch](https://github.com/andrisgauracs/wifi-sensing-kit/blob/7fb77fc0c0adb98bcf0a102d68fa8718b2f98625/firmware/ruview-v0.8.8-kit.patch). Espressif beschreibt in der [S3-CSI-Dokumentation v5.4.1](https://docs.espressif.com/projects/esp-idf/en/v5.4.1/esp32s3/api-guides/wifi.html#wi-fi-channel-state-information), dass bei `first_word_invalid` die ersten vier CSI-Bytes ungültig sind; eine feste DC-/Guard-Maske ersetzt diese Prüfung nicht.

Die damalige Analyse beschrieb im lokalen [d6_fingerprint.rs](https://github.com/Gerafftes/Observatory/blob/028f9d3f2703e1f3610e856f48fa0a709647818b/software/ruview/v2/crates/wifi-densepose-sensing-server/src/d6_fingerprint.rs) RMS-Normalisierung mit Maske und Leerraumprojektion sowie in [position_capture.rs](https://github.com/Gerafftes/Observatory/blob/028f9d3f2703e1f3610e856f48fa0a709647818b/software/ruview/v2/crates/wifi-densepose-sensing-server/src/position_capture.rs) vier RX, drei Sekunden Fenster und eine Sekunde Schritt. Diese Quellpfade sind Orientierung, kein Nachweis eines seitdem unveränderten Codes oder Gerätestands. Das Kit ist kein Anlass, bestehende Gates still zu ändern. Neue kürzere Fenster benötigen einen eigenen geprüften Vertrag. Die Ortsklassifikation des Kits allein belegt keine kontinuierliche Ortung.

## Übertragbarkeit und späterer Projektstand

Die technische Übernahmeidee war ein begrenzter Vergleich mit RX1↔RX4 vor einer möglichen Erweiterung. Mehr Frames oder Funkstrecken allein belegen keinen Positionsnutzen; getrennte Trainings-, Validierungs- und Blinddaten sowie eine unabhängige Referenz bleiben erforderlich. Die damaligen Termine und operativen Auftragsbindungen werden mit dieser Quellenanalyse nicht fortgeschrieben.

Den später dokumentierten technischen Paarlauf fasst der [Tag14-Abschluss vom 29. Oktober](../project/rx-unified-tag14-result-20261029.md) zusammen. Er ist von einem Positionsnachweis und einer Vier-RX-Mesh-Abnahme getrennt. Der [Projektstatus](../project/status.md) ordnet die gespeicherten Ergebnisse ein.

Die öffentliche Fassung vom 4. November 2026 ergänzt keine neue externe Quellenprüfung, Implementierung oder Messung. Bei einer tatsächlichen Übernahme von Quellcode sind MIT-Hinweis und die [Drittkomponentenhinweise des gebundenen Quellenstands](https://github.com/andrisgauracs/wifi-sensing-kit/blob/7fb77fc0c0adb98bcf0a102d68fa8718b2f98625/NOTICE.md) zu erhalten.

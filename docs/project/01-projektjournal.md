# 01 Projektjournal – öffentliche Chronologie

Dies ist ein gekürzter historischer Auszug des lokalen Projektjournals vom 26. Juni bis 9. August 2026. Er dokumentiert damalige Beobachtungen und offene Nachweise. Lokale Betriebsdetails und Belegablagen sind hier ausgelassen; das vollständige Original bleibt erhalten. Die Chronologie ist keine aktuelle Betriebs- oder Sicherheitsfreigabe.

## 26.–28. Juni 2026: Datenpfad und frühe Grenzen

Am 26. Juni wurden TX-Startprobleme durch Brownouts untersucht. Eine reduzierte Sendeleistung allein beseitigte sie nicht; später konnte der TX starten. Nach Einrichtung der Toolchain erreichten erste gültige CSI-Frames den Server. Ein kontinuierlicher UDP-Strom war zunächst überwiegend Feature-State-Telemetrie; erst die weitere Filterdiagnose erschloss einen kontinuierlichen Raw-CSI-Strom. Telemetrieempfang und Rohdatenerfassung wurden damit ausdrücklich unterschieden.

Am 27. Juni sendeten vier RX gleichzeitig Raw-CSI. Am 28. Juni wurde ein Hostadresskonflikt beim Mehrknotenbetrieb erkannt. Die frühen Reihen A0–A3 erfassten Leerraum, Stehen, langsames Gehen und ruhiges Sitzen/Atmen. Zeitstempel-Fusion und Linkqualität blieben Grenzen. Ein 500-ms-Guard-Intervall in G1 stützte einen Visualisierungs-Workaround, ersetzte aber keine physische Synchronisation. In G2 blieb die Webvisualisierung trotz besser verteilter RX sprunghaft.

## 18. Juli 2026: Anzeige ist keine Ortung

Ein fester 1TX-/4RX-Aufbau und die Livevisualisierung wurden untersucht. Ein sichtbarer Datenpfad bewies keine korrekte Personposition. Stabilisierung oder Glättung einer Anzeige darf nicht mit höherer Messgenauigkeit gleichgesetzt werden.

## 26. Juli 2026: D4 und D5 scheitern an realer Generalisierung

D4 normalisierte die Bewegungsmetrik, um reine Pegelsprünge zu unterdrücken. Der erste E0-Lauf war durch zwei Raumzutritte kontaminiert und liefert keine gültige Leerraum-Fehlerrate. Die saubere Wiederholung E0b zeigte weiterhin 218/237 globale Still-Präsenzen im leeren Raum (92,0 %). Ein A/B-Wechsel des Laptopstandorts änderte einzelne RX-Reaktionen; das zweite Laufpaar E0d/E1b widerlegte eine feste RX4-Schwelle. Ein einzelner vermeintlich bester Link war nicht stabil.

D5 lernte eine robuste Leerraumreferenz und verlangte zwei RX-Stimmen. Zwei historische Replay-Folds erreichten im Mittel 0,0 % Leerraum-Fehlpräsenz und 89,3 % Still-Recall. Der anschließende reale Still-Livetest scheiterte: 236 plus 114 Samples blieben sämtlich global ABSENT. Der informative Link wechselte zwischen RX4 und RX3; das gleichzeitige Zwei-RX-Quorum wurde nie erreicht. Für diese Kalibrierung fehlte außerdem ein gepaarter blinder Leerraumlauf. Das positive Replay belegt daher keinen erfolgreichen Transfer auf neue Messungen.

## 29. Juli bis 1. August 2026: Offlinepipeline

D6 trennte aufbaugebundene Rohdatenerfassung, Präsenzmerkmale relativ zur Leerraumreferenz und diskrete Positionsklassifikation für P01–P09. Geprüft wurden Recorder, Sidecars, Setupbindung, Qualitätsgates sowie getrennte Schritte für Inspection, Indexbau, Vorhersage und Auswertung. Synthetische Tests und Builds sind Softwarebelege; reale Trainings- und Blindaufnahmen sowie Ortungsleistung waren damit nicht nachgewiesen. Fehlende Evidenz musste zu uncalibrated, unknown oder ambiguous führen und durfte keine künstliche Personposition erzeugen.

## 9. August 2026: zwei getrennte technische Setupserien

Gemeinsamer Liveempfang zeigte zunächst einen Rasterfehler. Filterbindung und CSI-Raster wurden getrennt behandelt. Eine Discovery-Aufnahme war technisch vollständig, hatte jedoch keine Setupbindung. Es folgten Preflight und Leerraumaufnahme einer ersten Setupserie. Nach einer Sidecar-Inkompatibilität wurde ein korrigiertes Serverartefakt separat eingefroren und eine zweite Setupserie gebunden. Die Serien dürfen nicht gemeinsam kalibriert werden.

Der zweite Preflight enthielt 2.701 Frames über rund 25 Sekunden; die neue Leerraumaufnahme 6.102 Frames über rund 65 Sekunden. Beide hatten laut damaliger Inspektion vollständige RX1–RX4-Abdeckung und keine Recorder-Drops. Die neue Leerraumaufnahme blieb nur für dasselbe Setup verwendbar. Eine verbundene Mess-UI und ausgerichtete Raumansicht waren vorbereitende Schritte, kein bestandener blinder Positionsnachweis.

Die spätere [technische Gesamtauswertung](../../experiment-reports/2026-08-23_D4-D5-D6_technischer-bericht/README.md) trennt die Nenner und fasst die Grenzen zusammen. [Aktueller Projektstatus](status.md) und [Messprotokoll](03-messprotokoll.md) ergänzen diese historische Chronologie.

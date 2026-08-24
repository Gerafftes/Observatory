# D5: Offline-Replay und experimentelle Präsenzkalibrierung — 2026-07-26

## Ziel

Die bisherige globale Klassifikation setzt `PRESENT_STILL`, sobald ein einzelner RX lokal Präsenz meldet. E0b, E0c und E0d zeigen, dass dadurch ein einzelner driftender Funkpfad den leeren Raum als belegt ausgeben kann. D5 soll deshalb leeren Raum und still sitzende Person trennen, ohne eine feste Schwelle an RX3 oder RX4 zu binden.

Status: **experimenteller Prototyp, noch kein Produktionsnachweis**.

## Vorab festgelegte D5-Regel

Die Regel verwendet zum Lernen ausschließlich Leerraumdaten:

1. 60 Sekunden leeren Raum aufnehmen.
2. Pro RX sechs vollständige, nicht überlappende 10-Sekunden-Mittelwerte bilden.
3. Pro RX berechnen:
   - Leerraum-Median,
   - `MAD`,
   - robuste Skala `max(1,4826 × MAD; 0,005)`.
4. Im Live-Betrieb den `smoothed_motion_score` pro RX kausal über 10 Sekunden mitteln.
5. Ein RX stimmt für Präsenz, wenn seine Abweichung größer als eine robuste Skala ist (`z > 1`).
6. `PRESENT_STILL` benötigt mindestens zwei absolute RX-Stimmen für zwei Sekunden.
7. Das Quorum wird bei einem Ausfall nicht auf einen RX abgesenkt. Unter drei frischen, kalibrierten RX ist D5 `degraded/unknown`.

Die vorhandene D4-Mehrheitsregel für deutliche Bewegung (`PRESENT_MOVING`/`ACTIVE`) bleibt unverändert.

## Leakage-sicheres Replay

Vier bereits aufgezeichnete 60-Sekunden-Läufe wurden verwendet:

| Lauf | Zustand | Rolle |
|---|---|---|
| E0c | leer | Leerraumkalibrierung des primären Folds |
| E1 | still sitzend | Entwicklungsbeobachtung |
| E0d | leer | unveränderte Validierung |
| E1b | still sitzend | unveränderte Validierung |

Die primäre Prüfung lernt ausschließlich aus E0c und wertet E0d/E1b unverändert aus. Zusätzlich wurde die Richtung E0d → E0c/E1 als Symmetrieprüfung gerechnet. Sie ist kein Ersatz für einen späteren, zeitlich neuen Blindversuch.

## Ergebnis

Ausgewertet wird erst nach dem vollständigen 10-Sekunden-Livefenster:

| Kalibrierung → Prüfung | Leerraum-Fehlpräsenz | Still-Recall | Balanced Accuracy |
|---|---:|---:|---:|
| E0c → E0d/E1b | 0,0 % | 88,8 % | 94,4 % |
| E0d → E0c/E1 | 0,0 % | 89,8 % | 94,9 % |
| Mittel | 0,0 % | 89,3 % | 94,7 % |

Im primären Fold erschien der erste stabile Alarm 2,03 Sekunden nach dem vollständigen 10-Sekunden-Fenster. Im Rückwärts-Fold waren es 5,09 Sekunden. Die erwartete Gesamtreaktionszeit liegt damit in diesen Daten ungefähr zwischen 12 und 15 Sekunden nach Beginn eines neuen Zustands.

Der wichtige Pfadwechsel wird abgefangen: Im ersten Personenlauf reagierten vor allem RX3 und RX4, im zweiten RX2 und RX3. Im E0d-Leerraum war RX2 allein auffällig und erreichte deshalb kein Quorum.

## Verworfene Varianten

- Eine strengere Variante mit `z > 3` und Skalenboden `0,002` hatte zwar ebenfalls 0,0 % Leerraum-Fehlpräsenz, aber nur 15,5 % mittleren Still-Recall.
- Ein überwachter, auf Leerraum **und** Personendaten trainierter RX-Selektor erzeugte in der vertauschten Prüfung 20,8 % mittlere Leerraum-Fehlpräsenz.
- Ein fester bester RX wurde verworfen: RX4 war in E0c/E1 sehr informativ, reagierte in E1b aber nicht mehr.

## Reproduzierbarkeit

Der Offline-Replayer liegt unter:

```text
scripts/evaluate_d5_replay.py
```

Er gibt eine maschinenlesbare JSON-Auswertung sowie eine kurze deutsche Zusammenfassung aus. Sieben Python-Tests prüfen unter anderem:

- keine Verwendung von Personendaten für die Leerraumreferenz,
- vollständige, nicht überlappende Kalibrierblöcke,
- Median/MAD/Skalenberechnung,
- vertauschte Laufpaare,
- Ausschluss unvollständiger Blöcke.

## Experimentelle Serverintegration

D5 ist im lokalen RuView-Arbeitsbaum als separate Präsenzschicht umgesetzt. Ohne explizite D5-Kalibrierung bleibt das bisherige D4-Verhalten aktiv. Die vorhandene SVD-/FieldModel-Kalibrierung wird nicht wiederverwendet.

Neue Endpunkte:

```text
POST /api/v1/classification/calibration/start
POST /api/v1/classification/calibration/stop
GET  /api/v1/classification/calibration/status
```

Zusätzliche Sicherheitsbedingungen:

- sechs vollständige 10-Sekunden-Blöcke,
- mindestens 20 Score-Samples pro RX und Block,
- mindestens drei frische RX-Referenzen,
- mindestens 5 Hz tatsächlich in D5 akzeptierte RX-Datenrate,
- 10 Sekunden Live-Warm-up,
- neues vollständiges 10-Sekunden-Fenster nach mindestens einer Sekunde Unterbrechung,
- Referenzverlust beim Wechsel des Subcarrier-Rasters.

Der Status-Endpunkt unterscheidet `legacy_d4`, `calibrating`, `operational` und `degraded_unknown`. Per-RX-Diagnosewerte enthalten Referenz, 10-Sekunden-Mittel, z-Wert, Stimme sowie Frische und Datenrate der tatsächlich akzeptierten D5-Samples. Bei Evidenzverlust wird eine zuvor gesetzte Still-Präsenz nicht festgehalten, sondern sofort verworfen.

## Technische Verifikation vor dem Livetest

- 7 von 7 Python-Replayer-Tests bestanden.
- 709 Rust-Tests des vollständigen Serverpakets bestanden; 2 Tests sind im Projekt bewusst als ignoriert markiert.
- Der optimierte Release-Build wurde erfolgreich erzeugt.
- Ein isolierter API-Lebenszyklustest bestätigte `legacy_d4 → collecting`, sechs geforderte Kalibrierblöcke, die Ablehnung eines zweiten Starts und die Ablehnung eines zu frühen Stopps ohne drei nutzbare RX.
- Ein unabhängiger Code-Audit fand nach den Korrekturen keine verbleibenden D5-Blocker und gab den kontrollierten Livetest frei.

Eine erfolgreiche reale D5-Kalibrierung ist damit noch nicht behauptet. Sie ist der nächste physische Test.

## Grenze des Ergebnisses

Die vier Läufe stammen aus derselben Sitzung, derselben Raumgeometrie und nur einer Sitzposition. Zeitlich benachbarte Samples sind außerdem autokorreliert und keine unabhängigen Einzelversuche. Das positive Replay rechtfertigt einen eingefrorenen Prototyp, aber noch keine dauerhafte Standardaktivierung.

## Nächster Entscheidungstest

Nach unverändertem Build und ohne weitere Parameteranpassung folgen:

1. neue 60-Sekunden-D5-Leerraumkalibrierung,
2. neuer blinder Leerraumlauf,
3. neuer blinder Lauf mit stiller Person,
4. Wiederholung an mindestens einer anderen Position.

Erst wenn diese neuen Läufe die vorab festgelegten Grenzen von höchstens 10 % Leerraum-Fehlpräsenz und mindestens 80 % Still-Recall bestehen, kann D5 als neuer Standard erwogen werden.

# D5: realer Still-Livetest nach Leerraumkalibrierung — 2026-07-26

## Ziel

Nach dem positiven Offline-Replay sollte D5 erstmals mit einer neuen realen Leerraumkalibrierung und anschließend mit einer still sitzenden Person geprüft werden. Die D5-Parameter blieben gegenüber dem Replay unverändert:

- per-RX-Leerraumreferenz aus vollständigen 10-Sekunden-Blöcken,
- RX-Stimme bei `z > 1`,
- mindestens zwei gleichzeitige RX-Stimmen,
- zwei Sekunden Persistenz.

Status: **Livetest nicht bestanden**.

## Datengrundlage

Nach erfolgreicher realer Kalibrierung wurden zwei unmittelbar aufeinanderfolgende Aufnahmen mit derselben still sitzenden Person ausgewertet:

| Lauf | Zustand | Rohsamples | Dauer |
|---|---|---:|---:|
| D5 E1 | Person sitzt still | 236 | 59,7 s |
| D5 E1 Persistenz | Person sitzt weiter still | 114 | 29,9 s |

Rohdaten:

- `data/raw/2026-07-26_23-28-03_D5_E1_still_sitzend/`
- `data/raw/2026-07-26_23-30-21_D5_E1_still_persistenz/`

Beide Aufnahmen enthalten in jedem Sample RX1 bis RX4. Die jeweiligen `errors.log`-Dateien sind leer. Für alle vier RX waren die D5-Referenz und die laufende Evidenz in sämtlichen ausgewerteten Samples verfügbar. Der API-Datensatz meldete je RX sieben abgeschlossene Referenzblöcke.

## Ergebnis

### Globale Ausgabe

| Lauf | `ABSENT` | `PRESENT_STILL` | Still-Recall |
|---|---:|---:|---:|
| D5 E1, 59,7 s | 236 | 0 | 0,0 % |
| D5 E1 Persistenz, 29,9 s | 114 | 0 | 0,0 % |
| zusammen | 350 | 0 | 0,0 % |

Die Person wurde während beider Aufnahmen global kein einziges Mal als anwesend ausgegeben.

### D5-Stimmen pro RX

| RX | Stimmen D5 E1 | Anteil | Stimmen Persistenz | Anteil |
|---|---:|---:|---:|---:|
| RX1 | 0 / 236 | 0,0 % | 0 / 114 | 0,0 % |
| RX2 | 0 / 236 | 0,0 % | 1 / 114 | 0,9 % |
| RX3 | 0 / 236 | 0,0 % | 114 / 114 | 100,0 % |
| RX4 | 87 / 236 | 36,9 % | 0 / 114 | 0,0 % |

Im ersten Lauf reagierte zeitweise nur RX4. In der anschließenden Persistenzaufnahme reagierte durchgehend nur RX3. RX2 stimmte lediglich in einem einzelnen Sample zu. Dadurch kamen nie zwei ausreichend lange gleichzeitig zustimmende RX zustande.

### Abweichung von der Leerraumreferenz

| RX | mittlerer z-Wert D5 E1 | maximaler z-Wert D5 E1 | mittlerer z-Wert Persistenz | maximaler z-Wert Persistenz |
|---|---:|---:|---:|---:|
| RX1 | 0,002 | 0,015 | 0,128 | 0,366 |
| RX2 | −1,239 | 0,655 | −1,297 | 1,014 |
| RX3 | −0,353 | 0,433 | 1,986 | 2,650 |
| RX4 | 0,738 | 1,990 | 0,142 | 0,362 |

Die Messung bestätigt erneut, dass der informative Funkpfad während einer unveränderten Sitzung wechseln kann. Diesmal wechselte die Reaktion von RX4 zu RX3, ohne dass beide Links gleichzeitig das festgelegte Quorum erfüllten.

## Bewertung

Der reale Still-Livetest widerlegt die Übertragbarkeit des positiven Offline-Replays auf diese neue Aufnahme. Das Replay erreichte auf den zwei vorhandenen historischen Laufpaaren einen mittleren Still-Recall von 89,3 %. Im neuen realen Positivtest lag der globale Still-Recall dagegen bei 0,0 %.

Das Ergebnis zeigt zwei gegensätzliche Grenzen:

1. Die frühere globale ODER-Verknüpfung erzeugte hohe Leerraum-Fehlpräsenz, weil ein einzelner driftender RX genügte.
2. Das D5-Zwei-RX-Quorum verhindert diese Einzel-RX-Fehlalarme, kann aber eine echte Person übersehen, wenn jeweils nur ein Funkpfad deutlich reagiert.

Die technische Sicherheitslogik funktionierte dabei wie vorgesehen: Eine einzelne RX-Stimme wurde nicht als globale Anwesenheit ausgegeben. Inhaltlich ist die Klassifikation dennoch unbrauchbar, weil die reale Person vollständig übersehen wurde.

## Grenzen des Versuchs

- Für diese neue Kalibrierung wurde noch kein separat aufgezeichneter blinder Leerraumlauf dokumentiert. Die False-Positive-Rate derselben Kalibrierung ist daher noch nicht bestimmt.
- Die beiden Positivaufnahmen stammen unmittelbar nacheinander von derselben Sitzposition.
- Der Test bewertet nur `ABSENT` gegen `PRESENT_STILL`.
- Er belegt weder eine räumliche Ortung noch eine belastbare Heatmap oder Vitalzeichenerkennung.

## Konsequenz

- Der reale D5-Livetest gilt als nicht bestanden.
- D5 wird nicht als Standard aktiviert.
- Das Zwei-RX-Quorum wird nicht allein anhand dieses Positivlaufs gelockert, weil sonst die bereits nachgewiesenen Leerraum-Fehlalarme zurückkehren können.
- Als nächstes müssen unter derselben Kalibrierung ein blinder Leerraumlauf und mehrere kontrollierte Still-Positionen als zusammengehörige Testserie aufgezeichnet werden.
- Erst danach kann geprüft werden, ob eine zeitliche Link-Auswahl, eine andere robuste Fusion oder zusätzliche Merkmale die False-Negative-Rate senken, ohne die Leerraum-Fehlpräsenz wieder zu erhöhen.

## Einordnung für den Bericht

Der Fehlschlag ist ein zentraler Reproduzierbarkeitsbefund. Gute Offline-Kennwerte auf wenigen, zeitlich nahen Laufpaaren reichen nicht aus. Im realen Betrieb können sich informative CSI-Links trotz unverändertem Aufbau ändern. Eine robuste Mehrlink-Klassifikation muss deshalb sowohl einzelne driftende Empfänger abweisen als auch wechselnde, nur einzeln reagierende Funkpfade berücksichtigen.

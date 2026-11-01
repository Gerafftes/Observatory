<!-- Publication copy: local paths and hardware identifiers were pseudonymized; private original retained. -->
# RX-Phasenzeitkorrektur – Offlineabschluss vom 16.10.2026

**PASS_OFFLINE für die Phasenzeit-Anforderungen einschließlich Hostwechsel, Capture, Recorder und Cleanup.** Die endgültige zweite Fassung besteht die vollständige Suite mit **138 Tests, 0 Fehlern und 0 Failures**. Der unabhängige Reviewer hat dieselben 138 Tests frisch ausgeführt und drei eigene Zusatzproben bestanden. Keine offenen Befunde im begrenzten Änderungsumfang. Keine Hardwarezugriffe, echten Hoststarts, neuen Messungen oder Automationsänderungen im Korrekturauftrag.

## Ursache und Korrektur

Beim erhaltenen gescheiterten Hardwarestart wurde nach fortgeschrittener Uhr erneut die volle Phasendauer als Einzelanforderung verlangt. Damit konnte schon `controls` trotz ausreichender verbleibender Zeit an `single dispatch exceeds phase deadline` scheitern. Die unveränderte Originalruntime reproduziert den Fehler offline in neun betroffenen Phasenunterfällen sowie im echten controls-Aufruf mit realer monotoner Uhr.

Der Sessionkern bietet jetzt das vorhandene absolute Phasenende beziehungsweise das reservierte Abbruchaktionsfenster an. Der Adapter prüft dagegen die aktuell verbleibende Zeit nach seinen Speicherprüfungen. Controls, alle drei Hostwechsel, beide Captures und normaler sowie fehlerbedingter Cleanup verwenden dieselbe gebundene Grenze. HTTP-/Upload- und Recorderaufrufe behalten außerdem ihre ursprüngliche Einzeldeadline über Bild-, Credential-, Metadaten- und Hostprüfungen hinweg; Curl erhält nur die dann verbleibende Zeit. Native Host- und Capturestarts prüfen nach ihren Dateivorbereitungen unmittelbar vor dem unteren Prozessstart erneut.

Die Capturevorbereitung reserviert das vollständige Messfenster plus erforderlichen Vor-/Nachlauf. Die Proben folgen absoluten Ticks statt fortlaufend verlängerter relativer Sleeps. Marker entstehen weiterhin aus echten Aufrufen der bestehenden Samplefunktion. Das 50-ms-Zeitgate bleibt unverändert. Ein expliziter nächster Tick beseitigt zusätzlich die vom unabhängigen Reviewer gefundene Float-/Nullsleep-Schleife der ersten Korrekturfassung.

## Ausgeführte Prüfungen

Die 138 Tests umfassen alle 122 bisherigen Regressionen und 16 neue Phasenzeitmethoden. Geprüft werden die tatsächlichen Aufrufpfade von `Session.execute`, `RealAdapter` und `NativeOS`; nur die unteren Geräte-, Datei-, Prozess- und Uhrprimitiven werden synthetisch ersetzt. Die OS-Sperre gegen Netzwerk, Gerätedateien, reale Dispatchprogramme und Schreiben in Originale beziehungsweise Lieferung wurde vor Runtimeimport aktiviert und geprüft.

- Vollständige Abläufe mit fortschreitender Uhr, großem nichtnull Startwert, Kosten für Uhr-/Datei-/Prozessschritte und vorhandenem sowie abwesendem Anfangshost: vollständige 480/960 s, zwei modellierte Uploads, vier Recordercontrols, drei Hostwechsel, Default-Endbeleg. Ein Fall modelliert zusätzlich zwölf vorab verbrauchte GETs im selben Budget.
- Tatsächlicher controls-/Hostpfad mit realer monotoner Uhr, langsamere Dateiprimitiven, exakt abgelaufene und überschrittene Normal-/Abbruchfenster, ablaufende native Startvorbereitung und Recorderstop mit nur zwei Sekunden Phasenrest.
- Später tatsächlicher Messmarker bleibt unter dem unveränderten Zeitgate ein Fehler. Die ursprüngliche statische FakeClock durchläuft die gesamte endgültige Runtime ohne Nullsleep-Schleife.
- Drei zusätzliche unabhängige Proben: verspäteter Credentialprovider getrennt für RX1 und RX4 verhindert den jeweiligen späten POST; vollständiger Original-FakeClock-Ablauf ohne Nullsleep. Reservierter Verbrauch, ursprünglicher T0 und echter Abbruchendpfad bleiben erhalten.

Der [unabhängige schriftliche Review](./private/rx-phasenzeit-independent-review-v2-20261016T173151Z/INDEPENDENT-REVIEW.md) bindet die 21 eingefrorenen Pythonquellen und alle eigenen geschlossenen Belege. Die [Testübersicht](./private/rx-phasenzeit-offline-20261016T173151Z/TEST-INDEX.private.json) bindet beide Gesamtsuiten, Zusatzproben und erhaltene Gegenläufe per Pfad, Größe und SHA-256.

Die erste Korrekturfassung ist ausdrücklich abgelehnt. Ihre begrenzt reproduzierte Nullsleep-Schleife und unvollständigen Suites sind erhalten und zählen nicht als PASS. Auch der ursprüngliche Livefehler und die roten Gegenläufe der Originalruntime bleiben unverändert. Die endgültige Fassung besteht die entsprechenden Gegenfälle; kein Fehlerbeleg wurde durch neue grüne Ergebnisse überschrieben.

## Gebundene Lieferung und Grenzen

Private Lieferung: [rx-phasenzeit-offline-20261016T173151Z](./private/rx-phasenzeit-offline-20261016T173151Z/REPORT.private.md).

| Quelle | SHA-256 |
|---|---|
| `adapter/real_adapter.py` | `15390b00c4048e391f5a7fba4b2a729466070a3b87721b103c51b9e165f1bfe4` |
| `adapter/session_core.py` | `dfa592c2d5d4c6b5f78ee140cd02e34c53b6b8d16ec9866cde0237e5721f7857` |

Die fünf Fachhelfer, alle Firmware-/Hostkandidaten, Limits und Sicherheitsgates bleiben unverändert. Kein Firmware- oder Hostneubau war erforderlich: verändert wurden ausschließlich Pythonlaufzeit und isolierte Prüfmittel. Weiterhin gelten 6.000 s einschließlich 600 s Endreserve, 120 RX-GETs einschließlich zwölf End-GETs, zwei Appschreibversuche, zwei Captures mit 480/960 s, vier Recordercontrols, drei Hostwechsel und 70.922.518.528 Byte Startbedarf. Autorisierungs-, Identitäts-, Config-, Slot-/VALID-, Speicher- und Endgates wurden nicht gelockert.

Die neue [gesperrte Hardwareübergabe](./private/rx-phasenzeit-offline-20261016T173151Z/HARDWARE-CONTRACT.PHASE-TIME.LOCKED.private.json) enthält ausschließlich falsche Livegrants und Ablaufzeit null. Der [vorbereitete Hardwareauftrag](./docs/project/agent-auftrag-rx1-rx4-hardwarelauf-20261016.md) verweist jetzt auf diese endgültige Runtime. Die historische gleichnamige Vorgängerübergabe bleibt erhalten; sie ist kein aktueller Einstieg. Ein alter verbrauchter Wrapper mit ursprünglichem Importpfad darf nicht unverändert wiederverwendet werden.

Der nächste fachliche Schritt ist ein später ausdrücklich beauftragter Livevorlauf mit diesen Quellen, neu gebundenem Wrapper, gemeinsamem T0 und frischem Speicher-/Host-/Vier-RX-Preflight. Dieser Offlineabschluss führt ihn nicht aus. Synthetische Writer und Testuhr belegen keine Funkqualität, aktuelle Geräteerreichbarkeit oder reale Dateisystemlatenz. Der bestehende Hardwareabnahmeplan bleibt offen; reale Latenzen dürfen weiterhin einen begrenzten Abbruch auslösen.

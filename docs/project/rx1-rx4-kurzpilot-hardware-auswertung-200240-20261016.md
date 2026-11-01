<!-- Publication copy: local paths and hardware identifiers were pseudonymized; private original retained. -->
# RX1/RX4-Kurzpilot: begrenzter Abbruch vor OTA und Messung

**Lauf `rx-shortpilot-20261016T200240Z`: ABGEBROCHEN. Funkqualität: NICHT_BEURTEILBAR.**

T0: **16.10.2026, 22:04:30.481117 Uhr Europe/Berlin**. Runnerende: **22:05:25.766768 Uhr**, nach **55.286 s** ab T0. Einmaliger Startnachweis nach 56.489 s: Runner bereits mit Exit 1 beendet. Der Liveabschnitt wurde nicht verlängert.

## Belegte Ursache

Die frische Laufvorbereitung erzeugte beziehungsweise prüfte das im Vertrag gebundene Host-Arbeitsverzeichnis `hosts/default` nicht. `NativeOS.launch` erreichte `subprocess.Popen(..., cwd=...)`; dieser Aufruf warf `FileNotFoundError` für genau dieses Verzeichnis. Das hätte die ausführende Orchestrierung vor T0 erkennen müssen. Es ist kein nachgewiesener Firmware-, Signierer- oder Funkfehler. Der geprüfte Kurzrunner und die Lieferung wurden nicht verändert.

Der normale Defaultstart verbrauchte den ersten Hostversuch. Der vorbereitete begrenzte Abbruchpfad erreichte anschließend `host_base_end`; der zweite nummerierte Permit und das zweite leere Hostlog sind gespeichert. Ein bestätigter Default-/TX-only-Endzustand wurde nicht erreicht. Es gab keinen erneuten Loggerstart und keine nachträgliche Reparatur oder Liveprüfung.

## Aktionsbilanz

| Aktion | Tatsächlich gespeichert | Grenze |
|---|---:|---:|
| RX-GETs | 20: 12 Preflight + 8 Controls | 120 |
| App-OTA-POSTs | 0 | 2, höchstens einer pro Ziel |
| Captures | 0 | 180/300 s, höchstens zwei |
| Recordersteuerungen | 0 | 4 |
| Hoststartversuche | 2, einschließlich begrenztem Endpfad | 3 |
| Bestätigte Hostbereitschaft | 0 | — |
| Lokale Status-/Node-GETs | 0 | 8 |
| Loggerstarts | 1 | 1 |

TX1 wurde nicht direkt abgefragt. RX1/RX4 erhielten keinen Upload. Es gab keinen Geräte-, Konfigurations- oder Sicherheitsschreibdispatch, keinen Reset oder Restore, keine weiteren Agenten und keine anderen Chats. Die frischen vier RX-Images/Boots/Slots/VALID- und acht Configbindungen wurden im Preflight gespeichert; Controls bestätigten danach acht weitere Status-/Configantworten. Diese Beobachtungen sind keine spätere Endklammer.

## Bewertung und offene Endbelege

- **Basis:** keine Aufnahme; feste 173 Fenster und Schwelle 156 bleiben erhalten. Pro RX sind Rate und qualifizierte Fenster nicht beurteilbar.
- **Paar:** keine Aufnahme; feste 293 Fenster und Schwelle 264 bleiben erhalten. RX1→RX4 und RX4→RX1 haben keine bewertbare Paketzahl, Rate oder Fensterzahl. Die Grenzwerte 600 Pakete und 2 Hz wurden nicht geprüft.
- **Ende:** `RUNNER-EXIT` liegt vor; zwölf End-GETs, finale Images/VALID/Configs/Diagnosen, Default-Rückkehr und aktuelle Geräte-Endzustände fehlen. Keine durch diesen Lauf gestartete Firmware-Peersitzung. Keine Capture-/Recorderwriter gestartet. Der Wachhalter war mit `-w` ausschließlich an den Runner gebunden; ein gesonderter Endmarker des Wachhalters fehlt.
- **Abnahme:** kein Funk-PASS/FAIL ableitbar; fehlende Messdaten ergeben NICHT_BEURTEILBAR. Keine Gesamt-G1-, Radar-, Tracking- oder Tagesabnahme.

## Rückkehr und gesicherte Originale

Die einzige neue Rückkehr wurde im selben Chat für **16.10.2026, 22:41:30 Uhr Europe/Berlin** bestätigt. Der Scheduler arbeitet sekundengenau; die Rundung zum gespeicherten T0+37-Minuten-Anker beträgt −0,481117 s. Nach dem Startfehler wurde die ungenutzte Automation **`rx-kurzpilot-einmalig-offline-auswerten` bestätigt PAUSED**. Gespeicherter Schedulerzustand: `next_run_at=null`, `last_run_at=null`. Es ist kein Rückkehrtermin mehr aktiv. Die Auswertung erfolgte sofort ausschließlich anhand lokaler gespeicherter Dateien; kein späterer Livezugriff.

Private Laufbindung und Originale: [Laufordner](./private/rx-kurzpilot-hardware-20261016T200240Z). Fehlertrace, Journal, Start-/Endmarker, Automationsbelege und vollständige Hash-/Identitätsbilanz: [OFFLINE-RESULT.private.json](./private/rx-kurzpilot-wrapper-20261016T200240Z/OFFLINE-RESULT.private.json). Die Originalantworten und acht Configwerte bleiben in der privaten Ablage. Geschlossene Originale werden über [CLOSED-ORIGINALS.private.json](./private/rx-kurzpilot-wrapper-20261016T200240Z/CLOSED-ORIGINALS.private.json) nach Größe und SHA-256 gebunden.

Die 2.893 Produktionsquellbindungen und Kandidaten entsprachen vor T0 der versiegelten Lieferung. Der Gesamtverifier scheiterte ausschließlich an seitdem veränderten historischen Arbeitsbaumreferenzen `tracking-progress.html` und `tracking-zeitplan-20261025.md`; die gezielte Lieferprüfung war ohne Abweichung. Vier kleine vorhandene Offline-Ablaufprüfungen bestanden, deckten das fehlende echte Arbeitsverzeichnis jedoch nicht ab. Originale und fremde Arbeitsbaumänderungen bleiben erhalten.

## Genau nächster Auftrag

**Ausschließlich offline:** Startvorbereitung um die Erzeugung und Prüfung sämtlicher gebundener Host-CWD-/Datenverzeichnisse ergänzen. Eine OS-isolierte echte `NativeOS.launch`-Probe muss mit anfangs fehlenden Verzeichnissen beginnen und normalen Defaultstart sowie Default-Abbruchpfad prüfen. Diesen Fehlerlauf erhalten. Erst danach einen neuen begrenzten Hardwarelauf gesondert freigeben. Dieser Lauf wird weder repariert noch fortgesetzt.

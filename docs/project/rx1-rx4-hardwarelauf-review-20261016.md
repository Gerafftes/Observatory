<!-- Publication copy: local paths and hardware identifiers were pseudonymized; private original retained. -->
# RX1/RX4-Hardwarelauf: Stop im passiven Preflight

**Der einmalige Lauf wurde am 16. Oktober 2026 um 18:38:17 Uhr Europe/Berlin vor dem ersten RX-GET gestoppt. Die vorgeschriebene frische passive MAC-/Endpointbindung für RX1 fehlte. Keine Installation, Hostaktivierung oder Messung hat begonnen. Basis und Paarvergleich sind NICHT_BEURTEILBAR.**

Auftrag: [agent-auftrag-rx1-rx4-hardwarelauf-20261016.md](agent-auftrag-rx1-rx4-hardwarelauf-20261016.md). Die unmittelbare menschliche Startnachricht erteilte A/B/C für genau diesen Lauf. Kein weiterer Agent oder Chat wurde verwendet.

## Bindungen und Offlinevorbereitung

Arbeitsbaum und Root-Anweisungen wurden gelesen; vorhandene Änderungen und Backups erhalten. Vor der neuen privaten Ablage wurde `/private/` als wirksam ignoriert und ohne versionierte Dateien bestätigt. Neue Ablage mit Verzeichnissen 0700 und Dateien 0600; keine Tokens oder Geräteexporte im öffentlichen Bericht.

Der positive unabhängige [Review der nativen Endkorrektur](native-abbruchpfad-review-20261016.md) wurde gelesen. Aktuelle Runtime, Sessionkern, Binder, Kandidaten-/Quellenmanifest, UI-Bindung, Candidate-Pin und Speicherplan wurden nach ihren vorgegebenen Hashes geprüft. Beide fertigen Hostprogramme und die beiden freigegebenen App-Dateien wurden geprüft. Keine Firmware-/Host-Neubauten oder Toolinstallationen.

| Bestandteil | Geprüfter SHA-256 |
|---|---|
| Runtimeadapter | `dbbbc887eff5b7b211f650a51e8f76d529021e41f13fbe56f650156e572bb96c` |
| Sessionkern | `508fa41ba6a04e8f529031547e00474c4deb7a6f7ed211b2a37e7152a57ce792` |
| Default-Host | `0750152f3b66596c5120b6a84bcff00c082e66cbf4bdb4d56cea4564c68cda2c` |
| Peerhost | `476ce0ec5bf1f173f870f0caa27a6da6ae1cc691bf64923b5e89d1944e5db472` |
| RX1-Appdatei, nicht installiert | `b50a10e796c9ef909155eca803e0061e52b8a2aa3974e736173e9b2b687bb14d` |
| RX4-Appdatei, nicht installiert | `dc8f3073c38fcdaaa226ce9a5ce019beaf2bbc366954f2ffd37282674df5eabd` |

Die historischen Vier-RX-Endreferenzen sowie die vorhandenen gerätespezifischen doppelten Bootloader-/Partitionsreadbacks für RX1/RX4 wurden lokal gegengeprüft. Dies bestätigt die gespeicherten Rückwegbelege; es ersetzt keine aktuelle Gerätebindung. Keine neuen ROM-/seriellen Zugriffe.

Drei kleine Fake-I/O-Tests der Orchestrierung bestanden: derselbe monotone Ursprung und die zwölf zusätzlichen Preflight-GETs bleiben vor `Session.execute` erhalten; der vollständige geplante Normalpfad enthält 94 RX-GETs. Vor Dispatch wird dauerhaft protokolliert. Zu später Vorlauf und zu viele zusätzliche GETs sperren den Start. Die geprüfte Runtime wurde nicht verändert. Diese Tests sind keine Live-Messung.

Vor T0 wurden 84.667.736.064 Byte freier Speicher beobachtet. Der tatsächliche gemeinsame Volume-/Pfad-/Blockguard wurde mit unverändertem Startbedarf 70.922.518.528 Byte unmittelbar im Preflight erneut erfolgreich ausgeführt. Der Platz wurde nicht exklusiv reserviert; keine Bereinigung.

## Tatsächlicher Liveverbrauch und Stopgrund

T0: **2026-10-16T16:38:17.230096Z**, entsprechend **18:38:17.230096 CEST**. Stopmarker: **2026-10-16T16:38:17.238303Z**. Monotoner aktiver Vorlauf bis Stopmarker: rund **0,0082 s**. Keine spätere Livefortsetzung und kein Zurücksetzen der Uhr oder Zähler.

Ausgeführt wurden ausschließlich ein passives Interface-Lesen mit `ifconfig` und ein Lesen des bestehenden Nachbarcaches mit `arp -an`. Die gespeicherte Interfaceantwort zeigt ein aktives Interface und die erwartete Hostzieladresse. Im gespeicherten Cache war RX1 nicht an seine bekannte physische MAC und das gebundene Interface zuzuordnen. Daher Stop vor Route-, Hostbesitz-, Credentials- und HTTPzugriffen. Die Offlineauswertung derselben Cacheantwort zeigt auch für RX3 keine entsprechende Zuordnung.

| RX | Eindeutige bekannte Zuordnungen im gespeicherten Cache | Aktuelles Image/Boot/Slot/VALID/Config/Diagnose |
|---|---:|---|
| RX1 | 0 – erster Stopgrund | nicht abgefragt |
| RX2 | 1 | nicht abgefragt |
| RX3 | 0 – zusätzlich offline festgestellt | nicht abgefragt |
| RX4 | 1 | nicht abgefragt |

Eine fehlende Cachezuordnung beweist weder Nichterreichbarkeit noch eine defekte oder ausgeschaltete Hardware. Die beiden vorhandenen Cachezuordnungen sind allein keine vollständigen Gerätebindungen.

| Aktion | Tatsächlich | Obergrenze |
|---|---:|---:|
| Passive lokale Interface-/Cachekommandos | 2 | begrenzter Preflight |
| RX-GETs | 0 | 120, davon 12 Ende |
| App-OTA-POSTs | 0 | höchstens 1 RX1, danach 1 RX4 |
| Captures | 0 | 2 mit 480/960 s |
| Recordersteuerungen | 0 | 4 |
| Hostwechsel | 0 | 3 |
| Lokale Host-Status-/Nodes-GETs | 0 | 8 |

TX1/RX2/RX3, RX1/RX4, sämtliche Konfigurations- und Sicherheitswerte wurden durch diesen Lauf nicht verändert. Anfangshost und finaler Default wurden nicht beobachtet oder gestartet; ein bestätigter Default-Endzustand wird daher nicht behauptet. Es existieren keine von diesem Lauf gestarteten Host-, Capture-, Recorder-, Wachhalte- oder Runnerprozesse.

## Rückkehr und Urteile

Die Vorbereitung erreichte das Gate für den unbeaufsichtigten Interventionsablauf nicht. **Keine Automation wurde angelegt, kein Rückkehrtermin bestätigt und keine spätere Ausführung geplant.** Eine Pausierung entfällt, weil keine neue Rückkehr existiert. Alte Automationen wurden nicht verändert. Die gespeicherten Stopbelege wurden unmittelbar einmal offline ausgewertet; keine nachträgliche Geräte-/Hostabfrage, kein Ersatzlauf.

| Prüfung | Urteil | Begründung |
|---|---|---|
| Frischer Vier-RX-Preflight | **STOP / Gate nicht erfüllt** | RX1 ohne passive MAC-/Endpointbindung; RX3 ebenfalls ohne Cachezuordnung |
| Basis, feste 473 gemeinsame Fenster | **NICHT_BEURTEILBAR** | Aufnahme nicht gestartet; keine Fensterpopulation erzeugt |
| Paar, feste 953 gemeinsame Fenster | **NICHT_BEURTEILBAR** | Aufnahme nicht gestartet |
| RX1 → RX4 | **NICHT_BEURTEILBAR** | keine Peeraufnahme oder geeignete Pakete belegt |
| RX4 → RX1 | **NICHT_BEURTEILBAR** | keine Peeraufnahme oder geeignete Pakete belegt |
| Vier TX1-Kontrolllinks | **NICHT_BEURTEILBAR** | keine aktuellen Pakete aufgenommen |
| Neue Installation RX1/RX4 | **NICHT_GESTARTET** | null OTA-Versuche |
| Finaler Default / Firmware-Peerende | **NICHT_BEURTEILBAR** | keine Host-/Firmwareintervention; keine End-GETs |
| Drei vollständige Kriterien vom 14. Oktober | **WEITER_OFFEN** | keine neue Mess- oder Attestationsevidenz |

Es gibt keine PCAP-/Server-/Raw-/Peerpopulation und deshalb keine neuen Raten-, Verlust-, Sequenz-, Ressourcen- oder Positionsaussagen. Die festen Qualitätsnenner und Schwellen bleiben unverändert. Historische Ergebnisse werden nicht als Ergebnis dieses Laufs übernommen.

## Konkreter nächster Schritt

Zuerst einen eng begrenzten **Bindungsauftrag für RX1 und RX3** durchführen, mit separat ausdrücklich zugelassenem Mittel zur frischen MAC-/Endpointzuordnung. Die vorliegende Freigabe erlaubt bei fehlender passiver Bindung ausdrücklich keine Probes, Scans, Reparatur oder zusätzliche Geräteaktion. Wenn der passive Cache weiterhin nicht genügt, muss der Folgeauftrag genau festlegen, ob beispielsweise je eine unauthentisierte Statusabfrage an einem bereits belegten bekannten Endpoint zur anschließenden Identitätsbindung erlaubt ist. Keine Zugangsdaten vor bestätigter MAC-/Interface-/Routen-/Nodebindung; keine Firmware-, Config-, Host- oder Netzänderungen aus diesem Bindungsauftrag.

Erst nach erfolgreicher Zuordnung ist ein **neuer, ausdrücklich begrenzter Basis-/Installations-/Paarlauf** sinnvoll. Diesen gestoppten Lauf nicht fortsetzen und seine Uhr oder Budgets nicht erneuern. Die aktuelle Ursache ist eine fehlende Beobachtungsbindung, keine belegte Funk- oder Firmwarefehlfunktion. Kein weiterer Offline-Neubau ist durch diesen Befund begründet.

## Private Originale

Run-ID: `rx-paar-hardware-20261016T163400Z`. Die kanonische Messablage wurde noch nicht erzeugt. Die ausschließlich private Wrapper-/Vorbereitungs-/Stopablage ist:

`./private/rx-paar-hardware-20261016T163400Z-wrapper/`

- `DIRECT-AUTHORIZATION.private.json`, `WRAPPER-SOURCES.private.json`, `OFFLINE-READY.private.json`: Umfang, Orchestrierungsquellen und lokale Prüfungen.
- `T0.private.json`, `journal.private.jsonl`: gemeinsamer Zeitanker, ausdrücklich markierte vorherige Fake-I/O-Ereignisse und tatsächliche zwei Livekommandos/Stop.
- `command-1/2.stdout.private`, zugehörige stderr-Dateien: geschlossene unveränderte Interface-/Cacheantworten.
- `preflight-stop.private.json`: ursprünglicher Fehler und Fehlerpfad.
- `OFFLINE-STOP-EVALUATION.private.json`: Zeiten, Zähler, Cachezuordnungsanzahlen und getrennte Urteile.
- `CLOSED-FILES.private.json`: Größen, Hashes und Rechte der geschlossenen eigenen Belege. Keine fortlaufende Datei als unveränderlich versiegelt.

<!-- Publication copy: local paths and hardware identifiers were pseudonymized; private original retained. -->
# Neuer RX1/RX4-Lauf: Startabbruch durch Phasenzeitbudget

**Der neue Lauf wurde mit den ausdrücklich erteilten A/B/C-Freigaben vorbereitet, der frische Vier-RX-Preflight bestand und eine einmalige Rückkehr in diesen Chat wurde eingerichtet. Der unabhängige Runner stoppte jedoch bereits in `controls`, vor Hoststart, OTA oder Capture: `single dispatch exceeds phase deadline`. Die ungenutzte Automation ist bestätigt PAUSED. Es gibt keinen zweiten Startversuch. Basis und Paar sind NICHT_BEURTEILBAR.**

Auftrag: [agent-auftrag-rx1-rx4-hardwarelauf-20261016.md](agent-auftrag-rx1-rx4-hardwarelauf-20261016.md). Run-ID: `rx-paar-hardware-20261016T171652Z`. Dies ist ein neuer eigenständiger Lauf nach der erfolgreichen [RX1/RX3-Wiederanmeldung](rx1-rx3-wiederanlauf-review-20261016.md), keine Fortsetzung mit erneuerter Uhr des früheren gestoppten Laufs.

## Bestandene Vorbereitung und frischer Preflight

Arbeitsbaum und wirksame Root-Anweisungen wurden geprüft; vorhandene Änderungen und Backups erhalten. Neue private Ablagen sind wirksam ignoriert und unversioniert; Verzeichnisse 0700, Dateien 0600. Kein anderer Agent oder Chat, keine Neubauten oder Toolinstallation.

Der [positive unabhängige Review der nativen Endkorrektur](native-abbruchpfad-review-20261016.md) wurde gelesen. Runtimeadapter, Sessionkern, Candidate-Pin, Binder, Kandidaten-/Quellenmanifest, UI-Bindung, Speicherplan, beide Hostprogramme und beide Appdateien bestehen die vorgegebenen SHA-256-Prüfungen. Bestehende gerätespezifische doppelte Bootloader-/Partitionstabellenreadbacks von RX1/RX4 stimmen mit den erhaltenen Rückwegbelegen überein.

| Unveränderte Runtime | SHA-256 |
|---|---|
| `real_adapter.py` | `dbbbc887eff5b7b211f650a51e8f76d529021e41f13fbe56f650156e572bb96c` |
| `session_core.py` | `508fa41ba6a04e8f529031547e00474c4deb7a6f7ed211b2a37e7152a57ce792` |
| Default-Host | `0750152f3b66596c5120b6a84bcff00c082e66cbf4bdb4d56cea4564c68cda2c` |
| Peerhost | `476ce0ec5bf1f173f870f0caa27a6da6ae1cc691bf64923b5e89d1944e5db472` |

Die neue kleine Wrapperkopie bewahrt die gemeinsame monotone Uhr und die zusätzlichen Preflight-GETs. Vier Fake-I/O-Tests bestehen: volle Session mit zwölf vorverbrauch­ten GETs, Ablehnung eines zu späten Vorlaufs, Ablehnung zu vieler GETs und höchstens zwei lokale Status-/Nodespaare mit tatsächlichem Vier-RX-Empfang als Startbedingung. Diese Tests deckten den nun beobachteten nativen Startfehler nicht ab.

Die Hostinputs wurden aus den vorhandenen realen Referenzen an TX-Quellhash und CSI-Raster gebunden. `--server-control-port 0` verhindert den zusätzlichen Browser-Control-Helper. Der geprüfte Host-/Firmwarecode wurde nicht verändert. Voller gemeinsamer Speicher-/Volume-/Pfad-/Blockguard mit unverändertem Startbedarf 70.922.518.528 Byte bestand vor Gerätezugriffen und nochmals in `controls`. AC-Stromversorgung des Macs wurde vor dem unabhängigen Runnerstart bestätigt.

**T0: 16. Oktober 2026, 19:19:10.976966 Uhr CEST.** Der neue passive Interface-/Cache-/Routen-/Prozess-/Portpreflight bestätigt einen abwesenden Anfangshost und vier eindeutig gebundene RX. Die zwölf tatsächlichen Preflight-GETs bestätigen C1 auf RX1/RX3/RX4 und C auf RX2, bekannte aktuelle Boots, ota_0 / VALID, die erhaltenen acht Configwerte und passende Diagnose-/Quell-/Bootbindungen. Frische Inputs und gesperrter Bindervertrag wurden in neuer Ablage erzeugt; die neue Autorisierung gilt ausschließlich für diesen Vertragsdigest und T0 + 6.000 s.

## Einmalige Rückkehr und unabhängiger Startversuch

Die aktuelle Codex-Automationsschnittstelle erzeugte eine ACTIVE-Heartbeat-Automation mit `destination=thread` und der tatsächlich eigenen Chat-ID. Ein einzelner künftiger Termin mit COUNT=1 wurde über Werkzeugaufruf, angezeigte Karte und übereinstimmende persistierte Definition gebunden:

- ID: `rx-paarlauf-16-10-einmalig-auswerten`.
- Geplant: **16. Oktober 2026, 21:01:11 Uhr CEST**, entsprechend 19:01:11 UTC. Das ist T0 + 102 Minuten, auf die nächste volle Sekunde aufgerundet.
- Livegrenze unverändert T0 + 100 Minuten; die zwei Minuten Abstand sind keine zusätzliche Livefreigabe.
- Prompt: einmalige Offlineauswertung im selben Chat, sofortige Pausierung, exklusiver Auswertungsmarker, keine neue Hardwareaktion, Messung, Agenten oder Folgetermine.

Der Runner wurde genau einmal mit geschlossenem stdin, eigener Sitzung und privaten dauerhaften stdout-/stderr-/Journaldateien gestartet. Die ursprüngliche Uhr und zwölf GETs wurden übernommen. Ein eigenes Wachhalteprogramm war an diesen Runner gebunden. Nach sieben Sekunden wurde genau ein begrenzter Startnachweis geprüft, kein fortlaufendes Beobachten der Messphasen.

Dieser Nachweis zeigte den Runner bereits mit Exit 2 beendet. `RUN-FAILED.private.json` und `RUNNER-EXIT.private.json` liegen vollständig vor. Der Fehler trat bei **T0 + 175,647 s**, ungefähr **19:22:06.624 Uhr CEST**, in `controls` auf. Keine Host-, Capture- oder Recorderkinder wurden gestartet. Der eigene Wachhalter wurde im Runner-finally beendet. Es erfolgte keine Wiederholung und keine neue Uhr.

Die ungenutzte Automation wurde unmittelbar über die aktuelle Schnittstelle auf **PAUSED** gesetzt; Werkzeugantwort und persistierter Status bestätigen denselben Chat und unveränderten Einzeltermin. **Null geplante Rückkehr-Aufrufe fanden statt. Der Termin 21:01:11 ist nicht mehr aktiv.** Gemäß dem Fehlerpfad des Auftrags erfolgte stattdessen genau eine unmittelbare Offlineauswertung der lokalen Startfehlerbelege.

## Konkreter Softwarebefund

Der gespeicherte tatsächliche Trace endet in der unveränderten Runtime:

1. `Session.gate('controls')` beginnt das 80-s-Phasenbudget.
2. `RealAdapter.action('controls', 80)` führt `storage_start()` aus und schreibt den einmaligen Verbrauchsmarker.
3. Danach ruft `real_adapter.py:1193` `check_initial_absent(self.gate(cost))` mit erneut vollen **80 Sekunden** auf. Der alternative vorhandene-Hostpfad verwendet dieselbe volle Kostenanforderung in Zeile 1195.
4. `Session.dispatch_guard` prüft korrekt `now + seconds <= phase_started + cost` (`session_core.py:203–205`). Sobald seit Phasenbeginn echte Zeit vergangen ist, passt eine zusätzliche volle 80-s-Anforderung nicht mehr. Der Guard sperrt, bevor die erste native Hostabfrage dieses Sessionabschnitts dispatcht wird.

Das ist **kein Ablauf der 100-Minuten-Gesamtfrist** und kein neuer Geräte-/WLANfehler. Der Aufrufer fordert nach bereits vergangener Vorbereitungszeit das vollständige Phasenbudget erneut an. Eine Vergrößerung der Gesamtfrist oder ein Zurücksetzen der Uhr wäre keine sachgerechte Korrektur.

Ein kleiner zusätzlicher Offlinegegenbeleg verwendet die **unveränderten tatsächlichen `Session.gate`-/`dispatch_guard`-Methoden**, ausschließlich mit injizierter unterer Uhr. Bei null Zeitfortschritt wird die volle 80-s-Anforderung akzeptiert; bereits bei 1 µs, 1 ms oder 1 s Fortschritt wird exakt der beobachtete Phasenfehler ausgelöst. Eine um die verstrichene Zeit begrenzte Anforderung passt in diesen reinen Kerntest. Das ist eine Reproduktion der Zeitlogik, kein neuer positiver Native-/Vollpfadtest oder eine implementierte Korrektur.

Im selben tatsächlichen Adapter existieren weitere volle Phasenkosten-Anforderungen: `host_switch` mit 60 s, `capture` mit `duration+20` und `cleanup` mit `cost`. Sie sind gezielte Prüfpunkte desselben Folgeauftrags. In diesem Lauf wurden sie nicht erreicht; weitere tatsächliche Fehler dort werden daher nicht als beobachtet ausgegeben.

## Verbrauch, Endzustand und Urteil

| Aktion | Tatsächlich |
|---|---:|
| RX-GETs dispatcht | **12**, nur Vorlauf |
| RX-GETs im Kern reserviert | **20**: zwölf Vorlauf plus acht nicht dispatchte `controls`-GETs |
| App-OTA / sonstige Geräte-POSTs | **0 / 0** |
| Captures / Recordersteuerungen | **0 / 0** |
| Hoststarts/-wechsel / lokale Host-Status-/Nodes-GETs | **0 / 0** |
| Reset-/ROM-/serielle-/Flash-/Netz-/Securityaktionen | **0** |
| Runnerstarts | **1**, beendet mit Exit 2 |
| Geplante Rückkehr-Aufrufe | **0**, Automation PAUSED |

TX1, alle RX, Konfigurations-/Sicherheitswerte und mmWave wurden nicht verändert. Anfangshost war im Vorlauf frisch abwesend; kein Default wurde gestartet oder als Endzustand bestätigt. Es bestehen keine von diesem Lauf geöffneten Capture-/Recorderressourcen. Originale, Fehlerpräfixe, Verbrauchsmarker und gesperrte historische Eingänge wurden erhalten. Die Runtimequellen sind auch nach der Auswertung nach Hash unverändert.

| Prüfung | Urteil |
|---|---|
| Frischer Vier-RX-Vorlauf | **PASS im beobachteten Vorlauf** |
| Automationsanlage und richtige Chatbindung | **PASS**, danach wegen Startfehler PAUSED |
| Einmaliger unabhängiger Runnerstart | **START_FAIL vor Host/OTA/Capture** |
| 480-s-Basis, feste 473 Fenster | **NICHT_BEURTEILBAR**, nicht gestartet |
| 960-s-Paar, feste 953 Fenster; beide Peerlinks | **NICHT_BEURTEILBAR**, nicht gestartet |
| Vier TX1-Kontrolllinks / Server- und Writerbilanz | **NICHT_BEURTEILBAR**, keine Aufnahme |
| Drei vollständige Kriterien vom 14. Oktober | **WEITER_OFFEN** |

## Nächster konkreter Schritt

**Vor einem weiteren Hardwarestart ist eine eng begrenzte Offlinekorrektur der Phasen-Dispatchanforderungen im vorhandenen Adapter erforderlich.** Den tatsächlichen unveränderten Startfehler und den neuen Kerngegenbeleg erhalten; sämtliche relevanten Aufrufer durch die echte Session mit fortschreitender Uhr und ihren tatsächlichen lokalen Vorbereitungs-/Datei-/Prozesskosten prüfen. Die bestehenden absoluten Phasen-/Gesamtfristen, 600-s-Endreserve, Aktionslimits, Speicher-, Identitäts-, Authentisierungs- und Endpfadgates bleiben bestehen. Keine pauschale Guardabschwächung, Fristverlängerung oder neue Uhr.

Die minimale Korrektur anschließend mit den betroffenen ursprünglichen Adapter-/Session-/Native-Endpfadtests und einem realistisch fortschreitenden vollständigen positiven Start-/Ablaufgegencheck unabhängig prüfen lassen, ohne Hardwareaktivierung. Kein Firmware-/Host-Neubau ist durch diesen Python-Aufruferbefund begründet. Der [Hardwareauftrag](agent-auftrag-rx1-rx4-hardwarelauf-20261016.md) untersagt ausdrücklich das spontane Umschreiben der geprüften Runtime während dieses Laufs; deshalb wurde hier keine Korrektur angewandt und kein zweiter Liveversuch gestartet.

## Private Originale

Wrapper-/Scheduler-/Fehlerablage:
`./private/rx-paar-hardware-20261016T171652Z-wrapper/`

Kanonische neue Inputs und Session-Startbelege:
`./private/rx-paar-hardware-20261016T171652Z/`

Maßgeblich sind `T0.private.json`, `preflight-complete.private.json`, originale `command-*.stdout.private`/stderr und GET-Receipts, `WRAPPER-SOURCES.private.json`, neue `LIVE-CONTRACT.private.json`, `AUTHORIZATION.private.json`, Scheduler-Toolbelege und `scheduled-return.private.json`, `LAUNCHED.private.json`, `RUNNER-STARTED.private.json`, `START-PROOF.private.json`, `RUN-FAILED.private.json`, `RUNNER-EXIT.private.json`, `AUTOMATION-PAUSED.private.json`, `CORE-TIME-COUNTEREXAMPLE.private.json` und `OFFLINE-FAIL-EVALUATION.private.json`. Offline-Fake-I/O-Ereignisse im Journal liegen vor T0 und sind keine Livebelege.

`FINAL-CLOSED-FILES.private.json` bindet die geschlossenen Dateien beider neuen Ablagen einschließlich Fehlerbelegen und Berichtshash. Keine fortlaufende Datei wird als unveränderlich versiegelt; es läuft kein finaler Default-Host. Tokens und originale Geräte-/Netzantworten bleiben privat.

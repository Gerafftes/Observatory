<!-- Publication copy: local paths and hardware identifiers were pseudonymized; private original retained. -->
# RX1/RX4: einmalige Offlineauswertung des Laufs 180528

**Ablauf: FAIL_START. Messqualität und vollständige Abnahme: NICHT_BEURTEILBAR.** Der Lauf `rx-paar-hardware-20261016T180528Z` endete bereits beim ersten Default-Hoststart und dessen begrenzter Default-Rückkehr. Es entstanden keine Basis-/Paaraufnahmen und es fand kein OTA-Schreibversuch statt.

Grundlage ist der [Hardwareauftrag](agent-auftrag-rx1-rx4-hardwarelauf-20261016.md). Diese Auswertung gehört ausschließlich zum Lauf 180528; ältere Läufe und synthetische Vorbereitungsbelege sind keine Messdaten dieses Laufs. Bei dieser Rückkehr gab es keinen Geräte-/Hostkontakt und keine neue Messung.

## Identität und einmalige Rückkehr

Die gespeicherte Run-ID, der ausführende Chat `01a1458d-a711-7f73-a7fa-0926c6afabee` und die Automation `rx-phasenzeitlauf-16-10-einmalig-auswerten` stimmen überein. Die Automation wurde über die aktuelle App-Schnittstelle auf **PAUSED** gesetzt; der persistierte Zustand wurde bestätigt. Eine exklusiv angelegte Auswertungsmarke verhindert einen zweiten Auswertungsdurchlauf. Kein Folgetermin und keine Änderung einer anderen Automation.

| Zeitpunkt / Dauer | Gespeicherter Befund, Europe/Berlin |
|---|---|
| Ursprüngliches T0 | 16.10.2026, **20:07:03,603** |
| Vier-RX-Preflight abgeschlossen | nach rund **1,816 s** |
| Runner zur Session bereit | nach **55,340 s**, ursprüngliche Uhr und zwölf GETs übernommen |
| Abbruchmarker | nach **166,917 s** |
| Runner-Exitmarker | **20:09:50,525**, monotone Gesamtdauer **166,920 s = 2:46,920 min** |
| Maximale freigegebene Livezeit | 6.000 s; keine Verlängerung |
| Geplanter einmaliger Rückkehrtermin | **21:49:04** = T0 + 102 min, auf volle Sekunde gerundet |
| Eingang der Rückkehr | **21:49:25,035** |
| Persistierte Pausierung | **21:50:54,230** |

Die Wartezeit bis zum festen Rückkehrtermin war keine Messdauer. Tatsächliche Capturezeit: **0 von vorgesehenen 1.440 Sekunden**.

## Tatsächlicher Verbrauch und Startfehler

| Aktion | Verbrauch | Grenze |
|---|---:|---:|
| RX-GETs | **20**: zwölf Preflight, acht Session-Kontrollen; jeweils Originalantwort und Hash vorhanden | 120 |
| App-OTA-POSTs | **0**, RX1 und RX4 jeweils null | 2 |
| Passive Captures | **0** | 2 |
| Recordersteuerungen | **0** | 4 |
| Hostwechsel/-startversuche | **2**, beide Default; zweiter im Abbruchpfad | 3 |
| Zusätzliche lokale Host-API-GETs | **0** | 8 |
| Vollständige End-GET-Serie | **nicht erreicht** | zwölf vorgesehen |

Beide privaten Hostlogs enthalten unmittelbar beim Start dieselbe Meldung: `WDP_RUFIELD_SIGNING_SEED is required`. Der gebundene Startpfad versorgte seine Kinder nicht mit dem erforderlichen RuField-Signierschlüssel. Beide Default-Kinder erreichten deshalb keine bestätigte Bereitschaft. Der Adapter wartete anschließend bis zur jeweiligen Bereitschaftsgrenze und endete mit `Blocked: host readiness deadline/cleanup reserve`. Die zusätzliche Warnung zum Server-Control-Port ist getrennt von diesem belegten fatalen Initialisierungsfehler zu behandeln.

Der Startfehler ist aus den beiden Originallogs belegt. Er liefert keinen negativen Funkbefund. Die Phasenzeitkorrektur allein hatte diese tatsächliche Hostinitialisierung nicht abgedeckt.

## RX-Bindung und Endzustand

Alle vier RX bestanden den damaligen frischen Preflight einschließlich Image-/Boot-/Sourcebindung, `ota_0`/`VALID` und acht typgenauen Konfigurationswerten. Die anschließenden acht Kontroll-GETs sind ebenfalls mit HTTP 200 und unveränderten Originalhashes gespeichert.

| RX | Image im Preflight, SHA-256-Präfix | Preflight | Installation durch diesen Lauf | Vollständiger Endbeleg |
|---|---|---|---|---|
| RX1 | `9f5afbc5cdb040d4…` | PASS | nicht begonnen | NICHT_BEURTEILBAR |
| RX2 | `e674e9fa055c0503…` | PASS | keine Schreibaktion vorgesehen/ausgeführt | NICHT_BEURTEILBAR |
| RX3 | `9f5afbc5cdb040d4…` | PASS | keine Schreibaktion vorgesehen/ausgeführt | NICHT_BEURTEILBAR |
| RX4 | `9f5afbc5cdb040d4…` | PASS | nicht begonnen | NICHT_BEURTEILBAR |

TX1 wurde durch diesen Lauf weder abgefragt noch verändert. Es gab keine RX-Konfigurationsschreibzugriffe. Das beweist keinen späteren aktuellen Gerätezustand: Die zwölf End-GETs und die vorgesehene Endklammer fehlen.

Der Zwischenmarker des Abbruchpfads meldet `CHILD_REAPED_NO_HOST` nach dem ersten Kind. Nach dem ebenfalls fehlgeschlagenen zweiten Default-Versuch lautet der abschließende Marker **`UNCONFIRMED_AFTER_OWN_CHILD_CLEANUP`**, der Abbruchabschluss **`UNASSESSABLE: deadline/end/cleanup failure`**. Ein Runner-Exitmarker liegt vor, ein regulärer `RUN-COMPLETE`-Marker fehlt. **Der gewünschte weiterlaufende Default/TX-only-Endzustand ist nicht bestätigt.** Der Peerhost wurde in diesem Lauf nicht gestartet; Capture und Recorder wurden nicht begonnen. Es wurden bei der Rückkehr keine aktuellen Prozess-/Port- oder APIabfragen zur nachträglichen Heilung ausgeführt.

## Messurteile mit unveränderten Nennern

| Gegenstand | Festes Kriterium | Urteil dieses Laufs |
|---|---|---|
| Basis, 480 s | mindestens 426/473 gemeinsame Fenster | NICHT_BEURTEILBAR: keine Aufnahme |
| Paar, 960 s | mindestens 858/953 gemeinsame Fenster | NICHT_BEURTEILBAR: keine Aufnahme |
| RX1 → RX4 | mindestens 1.920 geeignete Pakete, 2 Hz, 858/953 Fenster, Hostalter ≤250 ms | NICHT_BEURTEILBAR |
| RX4 → RX1 | dieselben getrennten Kriterien | NICHT_BEURTEILBAR |
| TX1 → RX1 | Basis/Paar getrennt, mindestens 5 Hz und gültige physische/Identitäts-/Zeitgates | NICHT_BEURTEILBAR |
| TX1 → RX2 | dieselben Kriterien | NICHT_BEURTEILBAR |
| TX1 → RX3 | dieselben Kriterien | NICHT_BEURTEILBAR |
| TX1 → RX4 | dieselben Kriterien | NICHT_BEURTEILBAR |

Geeignete Paketanzahlen, gemessene Raten und qualifizierte Fenster bleiben **unbekannt**, statt eine leere Aufnahme als gemessene Nullrate zu behandeln. Die Nenner bleiben 473/953. Es gibt keine PCAP-/Socket-/Server-/Raw-/Peerwriter-Bilanzen oder zulässigen Vor-/Nach-Differenzen für Messfenster. Aus dem fehlenden Messlauf folgt keine Funkverlustursache und kein Positionsbefund.

Die drei vollständigen Kriterien des unveränderten Abschlussplans vom 14. Oktober bleiben einzeln offen:

1. **Layout, Gültigkeitsmaske und Firmwarezeit: NICHT_BEURTEILBAR.** Keine echte Vier-RX-Aufnahme; RX2/RX3-Attestation und paketkorrelierte Firmwarezeiten bleiben zusätzliche offene Voraussetzungen.
2. **Vollständige Fehler-/Ressourcenbilanz: NICHT_BEURTEILBAR.** Capture-/Writer-/Stufendifferenzen fehlen. TX1 sowie die vollständigen CPU-/Heap-/Stack-/Firmwarezeitproduzenten sind durch diesen Lauf nicht belegt.
3. **Vollständiger tatsächlicher Peerbefund: NICHT_BEURTEILBAR.** Beide gerichteten Funk-/Transportbelege, vier TX1-Kontrolllinks, Firmwarefrische und bestätigter Endzustand fehlen.

Kein Gesamt-G1-, Tracking-, Radar- oder Tagesabschluss wird daraus abgeleitet.

## Offlineverfahren und Belegbindung

Die Auswertung verwendete den unveränderten gehashten Dateileser und dessen Prüf-/Reportfunktionen. Netzwerkzugriff, Gerätezugriff und Schreibzugriffe außerhalb der privaten Auswertungsablage waren dabei durch die OS-Sandbox gesperrt. Lauf-/Thread-/Automationsidentität, 20 originale Antwortbelege, Quellbindungen, Programm-/Apphashes und Abschlussmarker wurden abgeglichen. **126 Journalzeilen vor T0** stammen aus Vorbereitungstests; sie wurden aus den Livezählungen ausgeschlossen. Synthetische `offline-work`-Aufnahmen wurden nicht als echte Aufnahmen verwendet.

Eine vollständige wissenschaftliche Normalisierung/Auswertung wurde mangels tatsächlicher Capture-/Raw-/Writeroriginale nicht ausgeführt. Ein zusätzlicher isolierter Importversuch des unveränderten alten Normalisierereinstiegs scheiterte außerdem an `ModuleNotFoundError: No module named 'analyze'`; dessen damalige Ablage enthält das erwartete `historical`-Unterverzeichnis nicht. Dieser zusätzliche Offlinebefund und sein Fehlerpräfix bleiben privat erhalten; keine spontane Reparatur oder Datenersetzung. Er ändert die belegte Ursache des Hardware-Startabbruchs nicht.

143 geschlossene Original-/Vorbereitungsdateien sind einzeln nach Größe und SHA-256 gebunden; Daten bleiben an ihren Originalpfaden. Die Auswertung behauptet keine Versiegelung des gesamten Arbeitsordners.

Wesentliche Hashbindungen:

- damaliger Adapter: `15390b00c4048e391f5a7fba4b2a729466070a3b87721b103c51b9e165f1bfe4`
- damaliger Sessionkern: `dfa592c2d5d4c6b5f78ee140cd02e34c53b6b8d16ec9866cde0237e5721f7857`
- Default-Host: `0750152f3b66596c5120b6a84bcff00c082e66cbf4bdb4d56cea4564c68cda2c`
- damaliger Peer-Host, nicht gestartet: `476ce0ec5bf1f173f870f0caa27a6da6ae1cc691bf64923b5e89d1944e5db472`
- Hostlog 1: `f2d7955945cbe768758a93a6443b326b2428afec1b7283c6e7c15016a52662fc`
- Hostlog 2: `1dcd0c9288c77299653406d884b6d327daad7e70cf4f542904c41e1937bb4f1a`

Vollständige Original-, Programm-, App- und Quellbindungen: [privater Belegindex](./private/rx-paar-hardware-20261016T180528Z-wrapper/offline-evaluation/originals-and-sources.private.json). Einzelurteile, Zähler, Zeitanker und Originalreferenzen: [privater Ergebnisbericht](./private/rx-paar-hardware-20261016T180528Z-wrapper/offline-evaluation/report.private.json). Pausierungsbestätigung: [Automation PAUSED](./private/rx-paar-hardware-20261016T180528Z-wrapper/automation-paused.private.json).

## Kleinster begründeter nächster Schritt

Die belegte Hoststartlücke wurde inzwischen in einem gesonderten Offlineauftrag korrigiert. Der anschließend vorliegende [unabhängige Koordinatorreview des 35-Minuten-Kurzprofils](hoststart-kurzpilot-koordinatorreview-20261016.md) bewertet diese neue Lieferung als **PASS_OFFLINE**. Ihre normale Hostinitialisierung und die eigene Auswertung wurden isoliert geprüft; das ist keine erfolgreiche Funkmessung.

Der nächste Schritt ist ein **neuer konkreter Hardwareauftrag für genau das geprüfte Profil `rx-shortpilot35-v1`** mit passenden getrennten Betriebs-/Host- und RX1/RX4-Schreibfreigaben. Er muss frische Geräte-/Image-/Boot-/Slot-/Config-/Rückweg-/Host-/Speicher- und vollständige Programm-/Leserbindungen herstellen. Die alten verbrauchten Freigaben und der alte 480/960-s-Einstieg sind kein Startpfad für die neuen Kandidaten. Der vorbereitete Kurzpilot sieht 180/300 s Aufnahmen, höchstens 35 Minuten Livezeit und eine einmalige Offline-Rückkehr nach 37 Minuten vor. **Hier wurde weder dieser Auftrag gestartet noch eine weitere Rückkehr eingerichtet.**

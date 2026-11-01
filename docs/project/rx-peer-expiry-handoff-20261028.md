<!-- Publication copy: local paths and hardware identifiers were pseudonymized; private original retained. -->
# Übergabe Peer Capture Expiries und Tag 14

Stand: 28.10.2026. **Die neue Firmware ist auf allen vier RX installiert. Q900, Short30 und Basis180 bestanden. In der Paarvorbereitung entstehen auf RX1 und RX4 weiterhin lokale Capture-Expiries und Abandoned-Zähler. Deshalb wurden der 60-s-Peerpilot und Paar300 nicht gestartet. Tag 14 bleibt offen.**

Johann übernimmt die Korrektur und meldet sich danach zurück. Diese Übergabe dokumentiert Befunde und Prüfaufträge; sie implementiert keinen weiteren Fix und führt keine Hardwareaktion aus. Geräte- und Hostzustände unten sind die zuletzt gespeicherten Nachweise, keine neue Live-Abfrage bei Erstellung dieses Dokuments.

## 1. Letzter bestätigter Endzustand

| Gegenstand | Bestätigter Stand |
|---|---|
| RX1 und RX4 | Neue Firmware `VALID`, TX-only ohne Peers, Profilgeneration **9**, Diagnose `NORMAL` |
| RX2 und RX3 | Dieselbe neue Firmware `VALID`, TX-only ohne Peers, Profilgeneration **1**, Diagnose `NORMAL` |
| Abschlussbeobachtung | **21,850567 s**, gleiche Boots, Profile und Netzgenerationen; steigende Geräte- und Hostzähler, passende Quellbindungen |
| Eigene Aufnahmen und Messleases | Keine aktiv; eigene PCAP-Prozesse geschlossen |
| Host | Geprüfter RX-only-Host mit Reorder-Korrektur und versiegeltem Return-Setup läuft weiter |
| Automationen | Q900-Zwischen- und Abschlussaufgabe nach Ausführung gelöscht; keine weitere Messung geplant |
| TX1, Radar, Host-Netz, Gerätesicherheit | Durch diesen Lauf nicht verändert |

Die normale Anzeige nach der Rückkehr wurde auch vom Nutzer beobachtet. Sie gehört zum neuen **TX-only-Boot**. Zurückgesetzte Fehlerzähler nach diesem Neustart belegen keine Korrektur des Paarpfads.

Der Hardwareauftrag ist beendet; je RX wurde genau ein App-Upload ausgeführt. Alte Fristen und verbrauchte Schreibansprüche nicht für einen weiteren Installationsversuch übernehmen. Eine spätere Installation oder Messung braucht eine frische tatsächliche Bindung und passende neue Freigaben. Keine automatischen Upload-, Profilwrite-, Neustart- oder Messwiederholungen.

## 2. Richtige Arbeitsbasis und Identitäten

**Nicht vom ursprünglichen Checkout auf den installierten Stand schließen.** Bei Erstellung dieser Übergabe unterschieden sich dort unter anderem `csi_peer.c/.h`, `csi_transport.c`, `rx_budget.c` und `csi_collector.c` vom tatsächlich installierten Quellmanifest. Der Checkout enthält umfangreiche fremde Änderungen und unversionierte Dateien; kein Reset, Clean oder pauschales Überschreiben.

Maßgeblicher installierter Stand:

```text
PRIVATE_EVIDENCE/rx-peer-admission-fix-offline-20261028-run01/
  candidate/source/                 eingefrorene Buildquellen
  candidate/SOURCE-MANIFEST.json     Dateihashes und kanonischer Quellhash
  candidate/RELEASE.json            Build-, Image- und Slotbindung
  candidate/sdkconfig              tatsächlich verwendete SDK-Konfiguration
  candidate/rx-unified.bin          tatsächlich installierte App
  work/software/ruview/firmware/esp32-csi-node/
                                   zugehörige Arbeitskopie mit Tests und Doubles
  models/                          bisherige synthetische Lastmodelle
  peer-admission.patch              Änderung gegen die damalige candidate-v2-Basis
```

Für die Korrektur eine neue isolierte Kopie dieser Basis verwenden. Eingefrorene Quellen, Testausgaben, Messdateien und Negativbelege erhalten. Die Arbeitskopie und Modelle sind Ausgangsmaterial, keine Garantie, dass ein späterer Edit noch dem installierten Image entspricht.

| Identität | Wert |
|---|---|
| App-Datei-SHA256 | `c929c401c7d2ec22c86954c478e7e04d8f1a08270ac9131f0b1ccd68337908c5` |
| ESP-Image-Digest | `8d2c449c98f9136c00e064ba88de7aa68fc36361a3661ae2fa84f9e0a8c36c5f` |
| Kanonischer Quellhash | `08e3f6c38f4d22582010855771f974503b56f14f809e93f6d1e49509bf88482e` |
| Appgröße und Slot | 910.608 Byte; 2.097.152 Byte Appslot |
| SDK | ESP-IDF 5.4, Commit `67c1de1eebe095d554d281952fde63c16ee2dca0` |
| Host-BIN-SHA256 | `3aa730bc260f49b1f6ce1297ec0f8466a6051de7b9a2458321217ab93eca4d00` |

Der Host liegt unter `PRIVATE_EVIDENCE/rx-host-reorder-offline-20261028-run01/`. Die im Offline-Release verbliebene Kennzeichnung `hardware_validated=false` nicht nachträglich in einen vollständigen Peer-PASS umdeuten: Installation, TX-only und Basis sind live belegt, der Paarpfad ist nicht abgenommen.

## 3. Verlauf und Abgrenzung zu früheren Fehlern

1. **Früherer Hostabbruch durch Paketumordnung:** Der Host brach bei RX2 `41128 → 41127` die Rohaufnahme ab. Die begrenzte Reorder-Behandlung wurde in Host und Audit korrigiert. Sie bleibt erhalten; der aktuelle Abbruchgrund ist geräteseitiger Peertransport, kein erneuter Nachweis dieses Hostfehlers. Der vorherige vollständige Paarmitschnitt hatte sieben Umordnungen ohne Recorderabbruch verarbeitet.
2. **Zu wenige geeignete Peerframes:** Auf der vorherigen Firmware konkurrierten 128-Bin-HT-Peerframes mit den benötigten 64-Bin-Legacy-Frames um dieselbe Quote. Die Produktionscode-Nachstellung zeigte mit TX-Konkurrenz etwa 1,10/1,30 geeignete Frames/s. Das bewies einen hinreichenden Mechanismus unter den Modellannahmen, keine exakte Rekonstruktion verworfener Live-Callbacks.
3. **Installierte Korrektur:** Peer-Kandidaten werden vor Queue/Budget nach dem vorgesehenen Legacy-Layout ausgewählt. Andere ansonsten gültige Peerlayouts werden als `layout_rejected` bilanziert und erreichen die Peer-Rohaufnahme nicht mehr. Frühe geeignete Captures dürfen innerhalb ihrer ursprünglichen Lebensdauer auf eine reguläre Budgetgelegenheit warten. Quoten und reale Zulassungsprüfungen blieben bestehen.
4. **Aktueller Hardwarebefund:** Der TX-only-Pfad und die Basis funktionieren. Im Paarprofil entstehen fortlaufend Capture-Expiries und lokale Fehlermeldungen. Der nächste Fix muss diesen Befund lösen und die bislang zu optimistische Zeitmodellierung korrigieren.

Die bisherige Layoutauswahl ist eine bewusste Änderung der Peer-Aufnahmepopulation. Sie ist keine Lockerung der Auditmaske und keine physische Qualifikation jedes angenommenen Kandidaten. TX-Layouts sowie IQ-Bytes, Metadaten und Zeitstempel angenommener Captures bleiben unverändert.

## 4. Live-Belege des aktuellen Fehlers

Maßgeblicher Lauf: `PRIVATE_EVIDENCE/rx-peer-qualified-live-20261028-run01/`.

| Phase | Ergebnis |
|---|---|
| Vier-RX-Q900 | Bestanden, erste Beobachtungen ca. 810,4–810,5 s, letzte ca. 915,5 s Bootalter; Leases freigegeben |
| Short30 | 4.416 Rohframes, 23/23 gemeinsame Fenster, Rohdaten/PCAP/Zeit bestanden; Endabstand 37,807 ms; 1 unerklärte Sequenzlücke |
| Basis180 | 26.707 Rohframes, 173/173 gemeinsame Fenster; zwei übereinstimmende native Audits, PCAP und Zeit bestanden; Endabstand 31,342 ms; 10 unerklärte Sequenzlücken |
| Paarvorbereitung | Profilgeneration 8 auf RX1/RX4 gespeichert und aktiv, beide Neustarts bestätigt; laufende lokale Peertransportfehler |
| Peerpilot60 und Paar300 | **Nicht gestartet**; dafür gibt es keine neue Rohaufnahme und keinen neuen PCAP-Qualitätsbeleg |
| Rückkehr | Generation 9 auf RX1/RX4, passende Host-/Setupbindung und 21,85 s TX-only-Endprüfung bestanden |

Sequenzlücken sind separat bilanziert und kein RF-Verlustnachweis. Das native Core-Audit verbrauchte 16,150167 s des ausdrücklich erlaubten 60-s-Rechenbudgets; Messdauer und physische Grenzen wurden nicht erhöht.

### Fehler bereits in der ausgegebenen Paarbereitschaft

Die Datei `pair-qualification-01/receipt.private.json` dokumentiert:

| RX | Erste und letzte Beobachtung | LED zuerst / zuletzt | Zuwachs Expired | Zuwachs Abandoned | Zuwachs Peer-Queue-Drops |
|---|---|---|---:|---:|---:|
| RX1 | 90,152 → 151,566 s | LOCAL_TRANSPORT / NORMAL | 18 | 18 | 24 |
| RX4 | 90,081 → 151,518 s | LOCAL_TRANSPORT / LOCAL_TRANSPORT | 30 | 30 | 38 |

Trotzdem wurde `pair-profile-transition-02/READY.private.json` ausgegeben. **Das ist eine eigenständige belegte Prüflücke**, nicht bloß ein später zufällig aufgetretener LED-Zustand.

### Danach fortlaufende Fehler und Nutzerbeobachtung

| RX | Bootalter ca. | Expired = Abandoned | Peer-Queue-Drops | Budget-Drops | LED im Snapshot |
|---|---:|---:|---:|---:|---|
| RX1 | 187,19 s | 59 | 944 | 209 | LOCAL_TRANSPORT |
| RX1 | 278,91 s | 88 | 989 | 286 | LOCAL_TRANSPORT |
| RX4 | 187,40 s | 100 | 135 | 78 | NORMAL |
| RX4 | 279,60 s | 143 | 196 | 118 | LOCAL_TRANSPORT |

Boots, aktive/gespeicherte Profile und Netzgenerationen blieben zwischen diesen Beobachtungen gleich. TX1-CSI war frisch, die endlichen Peersitzungen offen; die Zähler der **rohen Peer-Sende-API** meldeten keine Fehler. Daraus weder einen vollständig fehlerfreien UDP-Sender noch verlustfreie Funkübertragung ableiten.

`status_led.c` zählt unter anderem `frames_abandoned` zu den lokalen Transportfehlern und hält die Anzeige für das bestehende Fehlerfenster aktiv. Zwischen Fehlern kann `NORMAL` erscheinen. Eine Momentaufnahme mit normaler LED entkräftet fortlaufende Zählerdeltas nicht. Die LED-Prüfung oder den Zähler nicht zur Symptombeseitigung abschalten.

## 5. Bestätigte Zeitmodell-Lücke und offene Ursache

**Bestätigt:** In der tatsächlichen Buildkonfiguration steht [`CONFIG_FREERTOS_HZ=100`](PRIVATE_EVIDENCE/rx-peer-admission-fix-offline-20261028-run01/candidate/sdkconfig:1239), entsprechend 10 ms Tickperiode. Der Requeue-Pfad verwendet [`vTaskDelay(1)`](PRIVATE_EVIDENCE/rx-peer-admission-fix-offline-20261028-run01/candidate/source/main/csi_transport.c:410). Das bedeutet einen FreeRTOS-Tick, nicht pauschal 1 ms.

Die bisherigen [Doubles](PRIVATE_EVIDENCE/rx-peer-admission-fix-offline-20261028-run01/work/software/ruview/firmware/esp32-csi-node/test/stubs/esp_stubs.h:151) setzen standardmäßig `ESP_TEST_TICK_US=1000`; `vTaskDelay` verschiebt dort die simulierte Zeit entsprechend. Das [Lastmodell](PRIVATE_EVIDENCE/rx-peer-admission-fix-offline-20261028-run01/models/analyze_tx.py:23) injiziert Callbacks und verarbeitet die Queue in einem festen 1-ms-Raster.

**Arbeitshypothese:** Eine nominell innerhalb der 100-ms-Capturefrist verfügbare Gelegenheit wird wegen Tickphase, Requeue-Warten oder späterer Verarbeitung nicht rechtzeitig tatsächlich genutzt. Die reine Zukunftsprobe reserviert nichts und garantiert weder Worker-Wakeup noch Serialisierung oder erfolgreichen Socketzugriff bis zur Deadline.

**Noch nicht bewiesen:** Dass die Tickabweichung jede reale Expiry erklärt oder der einzige Auslöser ist. Es fehlt eine vollständige Zeitspur der verworfenen Captures. Die erfolgreiche 1-ms-Nachstellung von ca. 9,68/9,50 geeigneten Frames/s ist kein Nachweis für den tatsächlichen 100-Hz-Scheduler.

Der Fix darf weder die 100-ms-Altersgrenze erhöhen noch die Originalzeit eines Captures beim Requeue erneuern. Ebenso wenig einfach den Produktionstick auf 1.000 Hz umstellen und daraus eine Abnahme ableiten: Das wäre eine weitere Systemänderung mit eigener Ressourcen- und Zeitvalidierung.

## 6. Relevante Quellpfade

Die Firmwarepfade dieser Tabelle liegen unter der eingefrorenen Basis `candidate/source/main/` aus Abschnitt 2.

| Datei oder Funktion | Prüfpunkt |
|---|---|
| `rx_budget.c::rx_budget_can_admit_before` ab Zeile 55 | Reine Kopien; früheste nominelle Gelegenheit und optional Sekundenwechsel; unveränderte Quoten; keine Zusage einer rechtzeitigen realen Ausführung |
| `csi_peer.c::csi_peer_deferred_admit` ab Zeile 93 | Aktuelle Zeit unter Lock; Alter >100.000 µs; reale Zulassung oder `WAIT_BUDGET`; ursprüngliches `callback_us` |
| `csi_transport.c::process_slot` ab Zeile 392 | Requeue, ein Tick Pause, belegter Peerplatz, Scheduling und Besitzbilanz; Verhalten bei EXPIRED, Stop und Requeuefehler |
| `csi_transport.c::send_csi_frame` ab Zeile 279 | Altersprüfung vor Sendung und nach Retry; Trial-Ende; absolute Deadline für den Sender |
| `stream_sender.c::stream_sender_send_until` und Lockpfad | Verbleibende Frist nach Lock-Wartezeit, Socket-/Backoff- und Sendelaufzeit |
| `csi_collector.c` und `csi_peer_reject_layout` | Peer-Kandidatenauswahl vor Queue; `layout_rejected`; TX-/Fremd-/Invalid-Verhalten erhalten |
| `status_led.c::led_task` | Fehlerfenster aus u.a. Abandoned; echte Fehler nicht durch geänderte Anzeige verdecken |

Beim Lesen der Zähler beachten: `CSI_PEER_EXPIRED` wird in `process_slot` derzeit nicht wie `BUDGET_DROP` oder `ABORTED` vorzeitig ausgebucht. Der Pfad läuft noch durch die Serialisierung bis zur Send-Altersprüfung und wird dort als Abandoned/Expired abgeschlossen. „Serialisiert“ bedeutet daher nicht „an den Host gesendet“. Das ist ein Prüfpunkt für Diagnose und Aufwand des Ablaufpfads, kein Beweis für die erste Ursache der Fristüberschreitung.

Tatsächlich verwendeter Python-Helfer, nicht versehentlich eine ältere Checkout-Version:

```text
PRIVATE_EVIDENCE/rx-recovery-corrections-20261028-run01/work/software/ruview/scripts/rx_measurement.py
```

Dort relevant: `preflight` ab Zeile 182, `need_live_peer_session` ab 261, `qualify_node` ab 266, `_qualify_node` ab 274 und die funktionale Qualifikation danach. Der aktuelle Live-Wrapper liegt unter `rx-peer-qualified-live-20261028-run01/profile_transition.py`. Historisch gebundene Helfer nicht in-place verändern; neue Kopie und neue Bindung für den nächsten Lauf erstellen.

## 7. Arbeitsaufträge für die Korrektur

Aufwandseinschätzung: **Hauptfehler 4/5**, weil Zeitmodell und reale Deadline über mehrere Verarbeitungsschritte zusammenpassen müssen und der genaue Live-Auslöser noch zu isolieren ist. **Bereitschaftshelfer etwa 2/5**, weil konkrete negative Fixtures und klar fehlende Prüfungen vorliegen. Das ist eine technische Einschätzung, keine Aufwandsgarantie.

### A. Deadlinefehler mit realistischem Zeitmodell nachstellen und beheben

1. Von der installierten Quellbasis ausgehen. Reproduzierer mit tatsächlichen 100 Hz und unterschiedlichen Tickphasen erstellen; 1.000 Hz höchstens als getrennte Vergleichskonfiguration behalten.
2. Taskblockierung und Callback-/Worker-Ausführung getrennt modellieren. Nicht lediglich das bestehende Modell mit `-DESP_TEST_TICK_US=10000` kompilieren und unverändert 1-ms-Ereigniszeiten zurücksetzen: Nach einem Delay könnte sonst die simulierte Uhr rückwärts laufen und einen künstlichen Budget-/Sitzungsfehler erzeugen.
3. Zeitpunkt der nominellen Gelegenheit, reales Wakeup, tatsächliche Zulassung, Serialisierungsende und Senderversuch unterscheiden. TX-Abstand, Peerfrist, volle Sekundenquoten, Meshrotation, belegte Queue, Lock-Warten und realistische Verarbeitungskosten einbeziehen. Alle realen Admission-/Sendprüfungen bleiben maßgeblich.
4. Die bisher mögliche Expiry vor der Änderung reproduzieren und durch einen passenden Fix verhindern. Wenn ein Capture innerhalb der ursprünglichen Frist nicht mehr nutzbar ist, keine falsche Liefergarantie geben und nicht bis zum sicheren Expiry blockieren. Gewollte Budgetverwerfung und tatsächliches Ablauf-/Transportversagen nachvollziehbar unterscheiden.
5. Falls die vorhandenen Daten nicht zur Zuordnung reichen, den **ersten** Fehler im bestehenden Diagnosepfad begrenzt erfassen: Link, ursprüngliche Callbackzeit, aktuelle Zeit, Deadline, `next_total_us`, `next_peer_us`, Queue-/Dequeue-/Sendestufe und Ergebnis. Keine neue Loggingplattform, keine Geheimnisse, keine NVS-Schreiberei im Messpfad.

Der Hardwarelauf zeigt die nötige Regression. Eine reine Umbenennung von Expired/Abandoned oder eine geänderte LED ist kein Fix.

### B. Bereitschaftsprüfung unabhängig davon schließen

`_qualify_node` prüft derzeit gültige Identität/Ressourcen, gleiche Boots/Profile, offene Sitzung und den **globalen** Producerfortschritt. Es prüft dort weder `NORMAL` in beiden Paarbeobachtungen noch das Wachstum der harten Peertransportfehler. Globaler TX-Fortschritt kann einen schlechten Peerpfad verdecken. `profile_transition.py` übernimmt das Ergebnis und schreibt READY ohne zusätzliche Paar-Gesundheitsprüfung.

Vor dem nächsten Versuch erforderlich:

- `driver_ok` und `NORMAL` für die maßgeblichen Beobachtungen prüfen.
- Über denselben Boot-/Profil-/Netzgenerationszeitraum Deltas der relevanten harten Fehler prüfen, insbesondere `expired`, `abandoned` und Serialisierungs-/unerwartete Abbruchfehler. Fehlende, rückläufige oder gesättigte Zähler nicht als Nullfehler behandeln.
- Den Fortschritt je benötigtem Peerlink ausweisen; globale TX-Zähler oder ein offenes Peerfenster allein sind keine Peerbereitschaft. Die vollständige physische Güte bleibt Aufgabe des Rohdaten-/Pilot-Audits.
- Auch den funktionalen Qualifikationspfad berücksichtigen: Er prüft bereits die LED an beiden Endpunkten, braucht aber ebenfalls eine sinnvolle Delta-Prüfung. Zwei normale Momentaufnahmen können Fehler dazwischen übersehen.
- Erwartete `layout_rejected`- und `budget_drops` nicht pauschal zu Hardwarefehlern erklären; auch Queue-Drops nach Ursache und notwendiger Rate/Abdeckung beurteilen. Es geht nicht um die Forderung, jeden absichtlich gedrosselten Callback zu übertragen.
- Fehlererkennung sperrt den Folgestart und gibt eigene Leases sauber frei; sie löst keinen automatischen Neustart, Upload oder Wiederholungsversuch aus. Rückkehrdiagnosen müssen weiterhin möglich sein.

Die gespeicherten Paarbeobachtungen aus Abschnitt 4 eignen sich unmittelbar als negative Fixtures. RX1 prüft den Fall „anfangs Fehler, später NORMAL, aber Fehlerzuwachs“; RX4 prüft Fehler an beiden Endpunkten. Beide dürfen keine neue Paarfreigabe liefern.

### C. Ablaufkorrekturen dauerhaft übernehmen

- **MAC-Serialisierung:** Die erste Paaranfrage war fälschlich mit 12 statt 17 Zeichen vorbereitet. HTTP 400 und frische Rücklesung bewiesen unveränderte Konfiguration/Boots; erst danach wurden dieselben MAC-Bytes im Doppelpunktformat erfolgreich übertragen. Diesen Formatvalidator im nächsten Generator testen; die alten unveränderten `TRANSITIONS.private.json` nicht erneut verwenden. Korrigierter Stand: `setups/TRANSITIONS.MAC-corrected.private.json` im aktuellen Lauf.
- **Rückkehrbindung:** Der alte Basishost konnte die zwischenzeitlichen Paarprofile nicht attestieren. Die sichere Rückkehr wurde direkt authentisiert mit physischer MAC/IP- und exakter Profilbindung geprüft, danach der passende Return-Host gestartet. Diese Reihenfolge erhalten, statt bei einer erwarteten Host-/Profilabweichung blind neu zu schreiben.
- **Startanzeige:** RX1 war bei der ersten Installationsnachprüfung nach 9,06 s noch in START und danach ohne weitere Mutation gesund. Künftige Werkzeuge sollen den begrenzten Startabschluss abwarten; START ist weder pauschal ein Firmwaredefekt noch ein Grund für einen zweiten Upload.
- **Arbeitskopie und Bindungen:** Fix, Tests und nächster Operator müssen dieselbe neue Revision verwenden. Der ursprüngliche Checkout ist weiterhin abweichend; kein erfolgreicher Test einer anderen Kopie als Beleg für die auszuliefernde App.

MAC-Format und Rückkehr sind in diesem Lauf geklärt und keine fortbestehenden Gerätefehler. Der Bereitschaftshelfer ist dagegen noch zu korrigieren.

## 8. Erforderliche Offline-Nachweise und Rücklieferung

- Bestehende Grenzen erhalten: bei 50 Hz Gesamtprofil 40 Hz TX-Reserve und höchstens 10 Hz **insgesamt** für Peers, unveränderte Mesh-Verteilung sowie Gesamt-/Peer-Mindestabstände; 100 ms Capturealter und 900 s Trial. Für Paar300 bleiben je Peer mindestens 600 geeignete Frames und 264/293 gültige 3-s-Fenster einschließlich der ursprünglichen Abdeckungs-/Lückenregeln nötig. Host-Abschlussgrenze bleibt 50 ms; das separat genehmigte native Rechenbudget beträgt 60 s.
- Testfall muss am installierten Stand das falsche Verhalten sichtbar machen und am Fix bestehen. Bestehende Profile, Sender, Ping, WLAN, OTA und Schutzprüfungen erhalten.
- Originale Capturezeit und IQ-Daten inklusive ungültigem Präfix bleiben durch Requeue unverändert. Exakte Deadline, Deadline +1 µs, Wakeup kurz vor/nach Tick, Serialisierungs- und Lockzeit bis zum Sendepfad sowie Trial-Ende testen.
- Requeuefehler, Stop, Slotbelegung, mehrere Peerlinks, dazwischenliegende Zulassungen, Sekundenwechsel, negative/überlaufende Zeiten und vollständige Queue-/Pool-/Zählerbilanz prüfen. Zukunftsproben dürfen den echten Budgetzustand nicht verändern.
- 100-Hz-Lastmodell mit den gespeicherten Lastgrößen und mehreren Tickphasen/Seeds ausführen. Geeignete Raten **und** die ursprüngliche Zeitfensterabdeckung ausweisen; keine Liveabnahme aus synthetischen Summenraten ableiten.
- Bereitschaftsgegenfälle mit den tatsächlich gespeicherten Fehlerdeltas müssen scheitern; gesunde Fälle und die sichere Leasefreigabe/Rückkehr müssen bestehen.
- Native C-Tests mit ASan/UBSan, passende Python-Tests, zwei bytegleiche Builds mit gepinntem SDK, Image-Digest-/Slotprüfung und Ressourcenvergleich. Kein Simulator- oder Test-PASS durch Lockerung von Grenzen.
- Keine eFuses, Secure-Boot-/Verschlüsselungs-/Anti-Rollback-Einstellungen oder Download-/Debug-Sperren ändern. Keine Authentisierungs- oder Messlease-Sperre umgehen und keine unbekannten NVS-Inhalte bereinigen.

Bisherige Ausgangsnachweise waren 13 native Unified-/Ping-Ausführungen, 51 Python-Tests und 48 synthetische Lastläufe sowie zwei bytegleiche Firmwarebuilds. Sie bestanden unter ihren damaligen Annahmen; der aktuelle Livefehler und die Ticklücke begrenzen deren Aussagekraft. Die zugehörigen Dateien liegen im Offline-Fixverzeichnis. Beispiele für bestehende Ziele: `run_rx_unified` und `run_csi_ping` im `test/Makefile`. Ausgaben einer erneuten Ausführung in **ein neues** externes Verzeichnis schreiben, nicht in die historischen `native-tests/`- oder Modellordner.

Für die Rückmeldung bitte liefern:

1. Arbeitskopie/Revision oder Patch **gegen die hier gebundene Basis** und kurze Erklärung des tatsächlich nachgewiesenen Auslösers.
2. Testbefehle und Ergebnisse, insbesondere 100-Hz-Reproduzierer, Deadline-Gegenfälle und Bereitschaftsfixtures; verbleibende Modellannahmen ausdrücklich nennen.
3. Neue App-Datei, `RELEASE.json`, `SOURCE-MANIFEST.json`, vollständige Hashes, SDK-/Compiler-/Konfigurationsbindung und Ressourcenvergleich.
4. Änderung der Rohdatenpopulation oder Diagnosebedeutung ausdrücklich benennen; keine still geänderten Felder oder Zählerdefinitionen.

Vor einem neuen Hardwarelauf zuerst neue Lieferung und Testbindung prüfen, dann Rechte, Istzustand, Host-/Setupbindung und notwendige frische Qualifikationen festlegen. Der Unified-Host verlangt eine konsistente Vier-RX-Firmwarebindung. Neue Firmware, neue Boots oder geänderte Profile/Netzgenerationen machen alte passende Qualifikationsbelege nicht automatisch wiederverwendbar. Die Rückkehr hat insbesondere RX1/RX4 neue Boots und Generation 9 gegeben.

## 9. Belegübersicht und bereits erledigte Punkte

Alle folgenden Pfade sind privat extern; keine NVS-, Flashsicherungs-, Credential- oder Rohdateninhalte ins Repository kopieren.

| Verzeichnis | Zweck |
|---|---|
| `PRIVATE_EVIDENCE/rx-peer-qualified-live-20261028-run01/` | Aktueller Livefehler und sicherer Endzustand |
| `PRIVATE_EVIDENCE/rx-peer-pilot-preparation-20261028-run01/` | Je RX `rxN-install-01/PASS.private.json`, Upload-/Boot-/Profilnachweise; historische Sicherungen nur referenziert |
| `PRIVATE_EVIDENCE/rx-peer-admission-fix-offline-20261028-run01/` | Installierter Quellstand, Modelle, Tests, reproduzierter Kandidat und Grenzen des bisherigen Fixes |
| `PRIVATE_EVIDENCE/rx-peer-budget-diagnosis-offline-20261028-run01/` | Vorherige Budgetkonkurrenz und Kontrollmodelle |
| `PRIVATE_EVIDENCE/rx-host-reorder-offline-20261028-run01/` | Bereits korrigierter Host/Audit-Reorder-Pfad |
| `PRIVATE_EVIDENCE/rx-tag14-repeat-20261028-run01/` | Vorheriger vollständiger Paar300 mit zu wenigen geeigneten Peerframes, nicht der aktuelle Expiry-Lauf |

Im aktuellen Liveverzeichnis zuerst lesen:

- `REPORT.final.private.txt`, `RESULT.final.private.json`, `RESULT-HASHES.final.private.json`.
- `pair-qualification-01/receipt.private.json` und `pair-profile-transition-02/READY.private.json` für die falsche Bereitschaftsfreigabe.
- `TURQUOISE-FIRST.private.json`, `TURQUOISE-SECOND.private.json`, `PEER-PILOT-BLOCKED.private.json` für den fortlaufenden Fehler.
- `shared-q900-01/receipt.private.json`, `short-audit-01/PASS.private.json`, `baseline-audit-01/PASS.private.json` für die bestandenen Teilphasen.
- `default-final-20s-01/PASS.private.json`, `host-return-02/PROCESS.private.json`, `return-profile-transition-01/READY.private.json` für den Endzustand.
- `CONFIG-400-UNCHANGED.private.json`, `pair-profile-transition-01/STOP.private.json`, die versionierten Operator-Manifeste und `operator-before-MAC-correction/` für die erhaltene Ablaufhistorie. Der alte STOP gehört zur klar abgewiesenen Anfrage; späteres READY liegt unter `pair-profile-transition-02/`.

Bei Erstellung wurden die eingefrorenen Firmwaredateien/App und das finale Live-Ergebnishashmanifest erneut geprüft. Es erfolgte keine neue Geräteabfrage.

**Keine weitere belegte Änderung an TX1, Radar, Host-Netz oder am bereits korrigierten Reorder-Algorithmus ist aus diesem Fehler abzuleiten.** Der zurückgestellte eigene WLAN-Verlusttest von RX1 und ein längerer Stabilitätsnachweis bleiben gesonderte offene Nachweise; sie werden durch TX-only-NORMAL nicht als bestanden erklärt und sind kein Beweis für die Ursache der jetzigen Peer-Expiries. G/Gesamt und spätere Tracking-/Radaraufgaben bleiben außerhalb dieser Übergabe.

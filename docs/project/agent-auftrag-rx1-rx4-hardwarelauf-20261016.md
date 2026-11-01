<!-- Publication copy: local paths and hardware identifiers were pseudonymized; private original retained. -->
# Agent-Auftrag: Vier-RX-Preflight, Basis, RX1/RX4-Installation und Paarvergleich

Projekt: `.`

Führe nach den unten ausdrücklich erteilten Freigaben genau **einen** begrenzten Hardwarelauf durch. Kein weiterer Agent, kein anderer Chat, keine neue Messreihe. Der Messprozess arbeitet selbstständig mit dauerhaften privaten Logs; du beobachtest ihn nicht fortlaufend. Richte genau eine geplante Rückkehr **in diesen ausführenden Agentenchat** ein, werte danach die gespeicherten Daten aus und entscheide anhand der Belege über den nächsten Schritt. Keine automatische weitere Hardwareaktion aus dieser Entscheidung.

## Ziel und ausdrückliche Freigaben

**Das ist die offene Aufgabe:** Die Offlinebasis, ihre native Endkorrektur und die nach dem gescheiterten Start vom 16.10. korrigierten Phasenzeiten sind unabhängig bestanden. Der echte gemeinsame Basis-/Paarvergleich fehlt. Der aktuelle Phasenzeitauftrag endet vollständig offline und startet diesen Hardwareauftrag nicht erneut. Vor einem später ausdrücklich beauftragten Lauf müssen Erreichbarkeit, Firmware-, Speicher- und Hostzustände frisch geprüft werden; gespeicherte Zustände sind historisch.

**Das erwarte ich:** Vier gebundene RX, vollständige Basis von 480 s mit bisheriger Firmware, danach RX1 und RX4 nacheinander installieren/VALID bestätigen, anschließend 960 s echter zweigerichteter Paarvergleich. TX1, RX2 und RX3 sowie alle acht RX-Konfigurationswerte unverändert. Am Ende Default/TX-only bestätigt, Peer beendet, keine offenen Capture-/Recorderressourcen. Vollständige Originale und Fehlerpräfixe erhalten; anschließend eine einmalige Offlineauswertung und konkrete Folgeentscheidung.

Dieser Auftrag ist vorbereitet; **die unmittelbare menschliche Startnachricht muss A, B und C für genau diesen Lauf ausdrücklich freigeben**. Ein gelesenes Dokument oder eine frühere Freigabe ersetzt das nicht:

- **A – Betrieb und Host:** Bis zu 100 Minuten Gesamt-Liveabschnitt einschließlich möglicher Unterbrechung von RX1/RX4 und Hoststart/-wechsel. Maximal drei Wechsel, auch bei zuvor abwesendem Host. Der geprüfte Default/TX-only-Host darf als ausdrücklich gewünschter Endzustand weiterlaufen. Bei vorhandenem Host nur frisch gebundene Prozessidentität anfassen.
- **B – Geräte schreiben:** Genau ein authentisierter App-OTA-POST auf RX1 und erst nach dessen erfolgreicher Prüfung genau einer auf RX4, einschließlich regulärer OTA-Neustarts. Ausschließlich gebundene App in den inaktiven Appslot; keine weiteren Schreibziele, Uploadwiederholungen oder manuelle Resets/Restores.
- **C – Lesen, Mitschnitte und Rückkehr:** Die hier begrenzten Vier-RX-/lokalen Hostabfragen, genau zwei passive Captures, vier lokale Recordersteuerungen, dauerhafte private Protokollierung und eine einmalige geplante Rückkehr in denselben Chat zur **Offlineauswertung** sind freigegeben.

Sind A/B/C erteilt, nach erfüllten technischen Gates innerhalb dieses Umfangs selbstständig fortfahren; keine erneute Grundsatzbestätigung pro RX oder Phase einholen. Fehlt eine Freigabe, vor den davon abhängigen Aktionen stoppen und genau den fehlenden Umfang benennen.

## Maßgebliche Quellen und Programme

Arbeitsbaum und wirksame `AGENTS.md` zuerst, danach gezielt:

1. `./docs/project/rx-phasenzeit-review-20261016.md` und `./private/rx-phasenzeit-independent-review-v2-20261016T173151Z/INDEPENDENT-REVIEW.md` samt `REVIEW-RECEIPT.private.json`: positiver unabhängiger Phasenzeitreview, 138 frisch bestandene Tests plus drei eigene Zusatzproben. Die bisherige native Endkorrektur bleibt als Vorgänger über `./docs/project/native-abbruchpfad-review-20261016.md` und ihre privaten Bindungen erhalten.
2. Aktuelle Runtime `./private/rx-phasenzeit-offline-20261016T173151Z/`: `REPORT.private.md`, **`HARDWARE-CONTRACT.PHASE-TIME.LOCKED.private.json`**, `LIVE-START.NICHT-AUSFUEHREN.private.md`, `CANDIDATES-FINAL.private.json`, `SOURCE-MANIFEST.private.json`, `TEST-INDEX.private.json`, `SEAL.private.json` und `adapter/real_adapter.py` sowie `adapter/session_core.py`. Die zusätzlich unverändert aufbewahrte Datei `HARDWARE-CONTRACT.LOCKED.private.json` ist ausschließlich die historische Vorgängerübergabe.
3. Runtimeadapter SHA-256 **15390b00c4048e391f5a7fba4b2a729466070a3b87721b103c51b9e165f1bfe4**; Sessionkern **dfa592c2d5d4c6b5f78ee140cd02e34c53b6b8d16ec9866cde0237e5721f7857**. Wrapperimport, Quellenmanifest und Autorisierung müssen dieselbe endgültige Fassung binden. Den verbrauchten alten Wrapper und die abgelehnte erste Phasenzeitfassung nicht unverändert starten; ihre Gegenbelege bleiben erhalten.
4. Unveränderte Hostprogramme unter `./private/rx-livevorbereitung-offline-20261016-20261016T143646Z/bin/`: `sensing-server-default`, SHA **0750152f3b66596c5120b6a84bcff00c082e66cbf4bdb4d56cea4564c68cda2c**; `sensing-server-peer`, SHA **476ce0ec5bf1f173f870f0caa27a6da6ae1cc691bf64923b5e89d1944e5db472**. Beide benötigen das geprüfte kurzlebige Launch-Permit des Adapters. Kein Umgehen der Startgates.
5. Der in der aktuellen Hardwareübergabe gehashte Dateibinder `./private/rx-livevorbereitung-offline-20261016-20261016T143646Z/scripts/bind_inputs.py` darf frische Inputs erzeugen. Er liefert zunächst ausschließlich gesperrte Grants. Erst die tatsächlichen A/B/C-Freigaben und bestandenen Bindungsprüfungen erlauben eine neue begrenzte Autorisierung. Alte gesperrte Vorlagen und Manifeste unverändert erhalten. Synthetische Schemas niemals als aktuelle Gerätebeweise benutzen.
6. Historische echte Geräte-/Acht-Felder-/Rückwegbelege gezielt über die bestehenden Bindungen verfolgen, insbesondere die abgeschlossene Ablage `./private/rx1-rx4-c1-vier-rx-restart-20261014T175514Z/`. Keine alten verbrauchten Liveeinstiege starten. Der vollständige unveränderte Abnahmeplan steht unter `./private/rx-phasenzeit-offline-20261016T173151Z/configs/RX-ABNAHME-14.private.md`.

Keine neue allgemeine Firmware-/Repo-/Backupanalyse, kein Neubau und keine Installation von Tools. Nur bei einer konkret belegten technischen Lücke vor dem Eingriff stoppen, nicht die geprüfte Runtime spontan umschreiben oder Schutzgates lockern.

## Hardware und feste Grenzen

TX1 und RX1–RX4 bleiben an ihren festen Messpositionen mit normaler Stromversorgung. TX1 benötigt keine Mac-USB-Verbindung; kein COM-/USB-Datenkabel zu einem ESP nötig. Mac am Netzteil im bestehenden Projekt-WLAN; Rechner und Codex-App bleiben für Hintergrundlauf und lokale Rückkehr verfügbar. Keine dauerhafte Energie-/Netzänderung. Ein vorhandenes kurzlebiges Wachhalteprogramm darf ausschließlich an den eigenen Runner gebunden werden und muss danach enden.

Vor neuen privaten Dateien Git-Ausnahme und Nichtversionierung prüfen. Neue Ablage etwa `./private/rx-paar-hardware-20261016-<UTC>/`, Ordner 0700, Dateien 0600. Lauf-/Input-/Analysepfade frisch und exklusiv; bestehende Ablagen nicht überschreiben. Gerätewerte, Tokens, originale Antworten, IQ und PCAP bleiben privat. Tokens niemals in argv, Chat, Automationsprompt oder allgemeinen Logs ausgeben.

**Eine gemeinsame monotone Livegrenze:** maximal **6.000 s ab erster aktuellen Geräte-/Interface-/Routen-/Prozessaktion bis Endprüfung/Cleanup**, einschließlich Vorlauf. **600 s Ende bleiben reserviert.** Keine neue Uhr oder Zählung beim Hintergrundstart, Phasenwechsel, Fehler oder bei der geplanten Rückkehr. Offlinevorbereitung vor T0 und reine Offlineauswertung nach beendetem Liveabschnitt zählen getrennt.

| Aktion | Gemeinsame Obergrenze |
|---|---:|
| RX-GETs inklusive zusätzlichem Preflight und aller Endbelege | **120**, davon **12 für Ende reserviert**; geprüfter normaler Sessionpfad 82 |
| App-OTA | **2**, genau höchstens einer pro RX1/RX4 |
| Passive Captures | **2**, Messfenster exakt **480/960 s** |
| Lokale Recorder-POSTs | **4**, pro Aufnahme genau ein Start/Stop; unsichere Versuche verbrauchen Budget |
| Hostwechsel | **3**, kein ungezählter Bootstrap |
| Zusätzliche lokale Status-/Node-GETs | höchstens **8**, nur `/api/v1/status` und `/api/v1/nodes`, außerhalb der Captures |

Alle Versuche zählen vor Dispatch. Zusätzlicher Preflight verbraucht dieselben globalen RX-GET-/Zeitbudgets; dessen Verbrauch muss in der Session berücksichtigt sein. Vor erster Liveaktion diese Einbindung und Dauerprotokollierung mit kleinem Fake-I/O-Test prüfen. Nur eine kleine Orchestrierung um die geprüften Klassen/`Session.execute` erlauben, keine zweite fachlich abweichende Runtime. Keine simulierte Echtzeit oder Budgetrücksetzung. Vor jeder Aktion müssen Restablauf, Endbelege und Rückkehr noch innerhalb der Grenzen liegen.

Alle Phasenanforderungen verwenden das bereits gebundene absolute Phasenende; im Abbruchpfad gilt das reservierte Endaktionsfenster. Speicher-, Identitäts-, Metadaten- und Credentialprüfungen verbrauchen dieselbe Zeit. Unmittelbar vor HTTP-, Host- oder Capture-Dispatch erneut prüfen, ohne Einzeldeadline oder Phase zu verlängern. Beide Capturefenster bleiben vollständig 480/960 s; feste Abtastticks und echte Messmarker samt unverändertem 50-ms-Zeitgate beibehalten. Zu spätes Vorbereiten führt zum begrenzten Abbruch, nicht zu verkürztem Mess-PASS.

Startbedarf unverändert **70.922.518.528 Byte = 66,051742554 GiB** auf den tatsächlich gemeinsamen gebundenen Zielpfaden. Aktuellen Volume-/Pfad-/Block-/Freiplatzguard vor Credentials und Gerätedispatch und anschließend wie implementiert prüfen. Wrapper-/Schedulerbelege in den vorhandenen kleinen Vorbereitungs-/Sidecarbudgets führen; keine zusätzlichen unbegrenzten Logs. Keine alten Löschfreigaben, Cloud-/Swap-/Cachebereinigungen oder passend abgesenkte Reserve.

## 1. Lokale Vorbereitung und frischer Vier-RX-Preflight

A/B/C dokumentieren, Quellen-/Programme-/Apphashes und bestehende Rückweg-/Bootloader-/Partitionsbelege lokal prüfen. Ohne belegten zum physischen Gerät passenden Rückweg kein Upload. Keine neuen ROM-/seriellen Sicherungs- oder Rollbackexperimente. Frische erwartete Gerätedaten aus echten Referenzen und begrenzten Beobachtungen binden; keine Beispiel-MACs, historischen PIDs oder geratenen DHCP-Adressen übernehmen.

Ab T0 zunächst nur freigegebene begrenzte passive Interface-/Nachbarcache-/Direktrouten- und Prozess-/Portprüfungen sowie notwendige bekannte RX-GETs. Zielendpoint vor Credentials an bekannte physische MAC/Node/Interface binden. Keine Pings, Scans, ARP-Probes oder Netzumbauten. Fehlende Bindung ist ein Stopgrund, kein Defektnachweis.

Alle vier RX müssen eindeutig und frisch nach bekanntem Image, Boot, Slot/VALID, Kanal/Zieltupel, acht typgenauen Configwerten und TX-Quelle gebunden sein. Die acht Werte sind `target_ip`, `target_port`, `node_id`, `csi_channel`, `edge_tier`, `tdm_slot`, `tdm_total`, `filter_mac_configured`. Bestehende geprüfte Referenzen erhalten; bei Widerspruch nicht neu einfrieren oder umkonfigurieren. RX2/RX3 bleiben bis zum Ende auf dem jeweiligen frisch bestätigten Boot/Image/Configstand. TX1 wird nicht separat per HTTP/seriell abgefragt oder verändert; Quellen-/Setupbindung aus bestehenden Belegen und aufgenommenen Daten führen.

Anfangshost explizit vorhanden oder abwesend prüfen. Vorhanden: vollständige PID-/Generation-/Binary-/argv-/CWD-/Input-/HTTP-/WebSocket-/UDP-Besitzbindung. Abwesend: der geprüfte frische Abwesenheitsnachweis inklusive Endpunkten, nochmals unmittelbar vor erstem Start. Fremder/inzwischen erschienener Besitzer sperrt; keine fremden Kills. Historisches Connection refused ist kein heutiger Abwesenheitsbeweis.

Frischen vollständigen REAL-Vertrag mit gehashten Ziel-/Programm-/Profil-/Exportdateien binden und kontrollieren. Historische gesperrte Dateien nicht überschreiben. Neue Autorisierung nur für die direkten A/B/C-Freigaben, diesen Vertragsdigest und die erforderliche endliche Laufzeit erstellen; sichere Default/TX-only-Rückkehr ausdrücklich erlauben. Vor erstem Start Inputs und Abwesenheit/Prozessgeneration noch gültig prüfen.

Die zusätzlichen lokalen Status-/Nodeabfragen innerhalb der Hostphase begrenzen: nach autorisiertem Defaultstart und vor erster Aufnahme sowie nach finaler Rückkehr, höchstens zwei Abfragepaare je Zeitpunkt. Vier reale RX und tatsächlichen Empfang prüfen; keine simulierte Quelle oder erfundene Nodes. Bei Nichterreichbarkeit/fehlendem Empfang mit den ohnehin erlaubten Belegen begrenzt diagnostizieren und stoppen, keine zusätzliche Messung oder Reparaturspirale. Diese Prüfung kann der protokollierte Runner übernehmen.

## 2. Hintergrundlauf und genau eine geplante Rückkehr

Der Agent schaut während der langen Phasen nicht ständig zu: **kein wiederholtes Tailen/Pollen durch Agententools, keine langen Sleep-/Waitschleifen und keine minütlichen Fortschrittsmeldungen.** Die bereits implementierten Sicherheits-, Deadline-, Daten-/Speicher- und Kindprozessprüfungen müssen im Messprogramm weiterhin kontinuierlich arbeiten. Ihr Abbruch-/Aufräumpfad muss ohne spätere Agentenrückkehr funktionieren.

Nach erfolgreicher Vorbereitung, aber vor Freigabe des automatisierten Interventionsablaufs genau eine gültige zukünftige Rückkehr über die aktuelle Codex-Automationsschnittstelle anlegen und bestätigen. Das Werkzeug zur Laufzeit suchen/Schema beachten. **Heartbeat im selben ausführenden Agentenchat, ausdrücklich `destination=thread`**, gegebenenfalls belegte eigene Thread-ID; kein neuer Chat oder Ersatz-Cronjob. Nicht den Koordinatorchat oder eine fremde historische Thread-ID wählen. Bestehende alte Messautomationen nicht reaktivieren. Nur eine passende Automation dieses neuen Run-IDs anlegen.

Rückkehrtermin **T0 + 102 Minuten**: 100 Minuten maximale Livezeit plus zwei Minuten Abstand für abgeschlossene Logs. Die zwei Minuten sind keine zusätzliche Livefreigabe. Tatsächliche Uhrzeit zur Laufzeit aus T0/aktueller UTC und Europe/Berlin bestimmen; keine feste Uhrzeit aus diesem Dokument übernehmen. Toolantwort muss einen künftigen einmaligen Termin und richtigen Chat bestätigen. ID, Zielchat, UTC-/Berliner Rückkehrzeit, Run-ID und Pfade privat speichern. Kein bereits verbrauchter oder rein wiederkehrender Termin. Wenn die einmalige Rückkehr nicht verlässlich eingerichtet werden kann, den unbeaufsichtigten Interventionsablauf nicht starten.

Der menschlich lesbare Automationsprompt muss inhaltlich enthalten:

> Dies ist die einmalige Auswertung des freigegebenen RX-Hardwarelaufs mit der gespeicherten Run-ID. Pausiere diese Rückkehr nach ihrem einzigen Aufruf und arbeite im selben Chat. Prüfe die gebundenen Prozess-/Abschlussmarker und privaten Logs, führe nur die Offlineauswertung aus und berichte Ergebnis, offene Nachweise und nächsten Schritt. Keine neuen Geräteabfragen, Uploads, Resets, Captures, Hostwechsel, weiteren Agenten oder Folgetermine. Ein fehlender Abschluss ist kein Auftrag für einen Ersatzlauf.

Den geprüften Ablauf als dauerhaft protokollierten, vom Terminal-/Agententurn unabhängigen eigenen Prozess starten, mit geschlossenem stdin, privaten stdout/stderr und gehashten Inputs. Prozessidentität/Startzeit und einmaligen Verbrauchsmarker sichern. Einen einzigen begrenzten Startnachweis prüfen: Runner aktiv, Logdateien vorhanden, tatsächliche Phase/Run-ID passend. Ein bloßer Hintergrund-PID ist kein Startnachweis. Bei gescheitertem Start keine zweite Ausführung; ungenutzte Rückkehr pausieren und direkt berichten.

Danach dem Nutzer nur Start, Mess-/Gesamtzeit und den **bestätigten Rückkehrzeitpunkt** nennen und den Agententurn beenden. Nicht bis Messende weiter pollen. Der Runner protokolliert Phasenbeginn/-ende, Originalantworten, monotone und Unixanker, Zähler vor/nach Dispatch, Byte-/Dateigrenzen, Prozessenden, Abbrüche und Aufräumen dauerhaft. Wichtige Phasen-/Endmarker flushen; bestehende Raw-/Tracewriter und deren Abschluss nicht durch neue per-Frame-Synchronisation oder große Kopien verändern. Ein Ausfall des Agenten oder verspäteter Rückkehrtermin darf keine Grenzen verlängern.

## 3. Fester automatisierter Ablauf – keine menschliche Livebeobachtung nötig

1. **Default und Basis:** bisherigen Firmwarestand aller vier RX prüfen, Default-Host mit privatem Datenpfad, `--source esp32`, `--rx-session-only` und gebundener schreibgeschützter UI starten. Vollständige Vier-RX-Vorklammer, **480 s Basis**, Nachklammer. Keine Geräte-/zusätzlichen Host-GETs während Capture einschließlich Vor-/Nachlauf/Cleanup; nur die vier ausdrücklich gezählten Recordersteuerungen und die geprüften internen Prozess-/Dateikontrollen des Messprogramms. Messvor-/nachlauf und Einschwingen nicht doppelt gutschreiben.
2. **RX1 installieren:** unmittelbar davor MAC/Boot/Image/Config/inaktiven Slot und Rückweg erneut mit vorgesehenen Mitteln bestätigen; genau ein roher App-OTA-POST. Binärbytes und Runtime-/Partitionsdigest getrennt behandeln. Begrenzte VALID-Folge bei den vorgesehenen 0/60/120 s, neuer konsistenter Boot und erwarteter Slot, danach acht Configwerte und Diagnose/Source prüfen. Erst nach Erfolg weiter.
3. **RX4 installieren:** derselbe einmalige Ablauf; kein paralleler Upload. Bei RX1-Fehler RX4 nicht mehr installieren.
4. **Paar:** frische vollständige Vier-RX-Vorklammer; begrenzten Peerhost starten, **960 s Paarvergleich**, beide Richtungen getrennt und vier TX1-Kontrolllinks aufnehmen. Quoten unverändert 50 gesamt/40 TX1/10 Peer, vorhandene Schutzfristen und physische Layout-/Maskengates beibehalten. Kein Ratensweep.
5. **Ende:** vor Paar-Nachklammer bestätigten Default/TX-only-Host wiederherstellen, Peerhost beenden; Firmware-Peerende gemäß bestehender Bootfristen abwarten, zwölf End-GETs und Cleanup mit unveränderten Configs/VALID/Sessionende bestätigen. Capture-/Recorderenden vollständig prüfen. Kein Peerbetrieb über das Auftragsende hinaus. Default darf gemäß A weiterlaufen.

Exakte Apps aus dem geprüften Kandidatenmanifest:

| Ziel | App unter `./private/rx-budget-vorklammern-offline-20261015T184011Z/` | Byte | BIN-SHA-256 |
|---|---|---:|---|
| RX1 | `build-rx1-peer/esp32-csi-node.bin` | 1.271.904 | `b50a10e796c9ef909155eca803e0061e52b8a2aa3974e736173e9b2b687bb14d` |
| RX4 | `build-rx4-peer/esp32-csi-node.bin` | 1.271.904 | `dc8f3073c38fcdaaa226ce9a5ce019beaf2bbc366954f2ffd37282674df5eabd` |

Runtime-/Partitionsdigest RX1 **3037163302097329165d1a5fd436ca79e3ae28c8086284247bb200a5993d3648**, RX4 **aefa88f79cce2989ae21e84b10e6c332af50f7d58db488f4dc5ea79316fc41d2**; Diagnosequellhash **65393ffab3fd9e20268f38202318393910fbe5f395f6870852ed3f434b52e825**. Keine andere App, kein Bootloader-/Partitions-/NVS-/Securityschreiben. Neue Bootloaderbytes im Buildordner sind ausdrücklich kein Installationsziel. Nach VALID besteht keine pauschale Garantie einer Qualitätsrückkehr zur alten App.

## 4. Fehlerverhalten und Belege

Geprüfte Gegenmaßnahmen der nativen Endkorrektur beibehalten. Fehlender Platz, falsche Identität, Capture-/Writerfehler, unerwarteter Boot/Config/Slot, Timeout oder Deadlinefehler sperrt weitere normale Arbeit. Nur bereits erlaubte begrenzte Endaktionen und sicher nachgewiesen eigene Kinder behandeln; keine Zusatz-OTA, manuellen Resets, ROMzugriffe, Restores, Netzeingriffe oder Ersatzcaptures. Fehlende Belege bleiben unbestätigt. Ein Stop der Ressourcen ist kein Mess-PASS.

PCAP, Raw-/Peeroriginale, Hosttrace, Recorder-/Writer-Enden, GET-/Config-/Diagnoseoriginale samt Anfrageintervallen, Quellen-/Programm-/Profil-/Geometrie-/Boot-/Zeitepochen, Prozess-/Budget-/Speicherjournale und sämtliche Fehlerpräfixe erhalten. Keine ungebundenen Exportduplikate und keine stille Trunkierung als Erfolg. Geschlossene Dateien nach Größe/Hash binden; ein weiterlaufendes finales Default-stdout-Log nur mit eindeutigem geschlossenen Präfix/Bytegrenze hashen und als weiterlaufend kennzeichnen. Keinen gesamten weiterhin veränderlichen Ordner als unveränderlich versiegelt behaupten.

## 5. Einmalige Rückkehr, Offlineauswertung und Entscheidung

Bei der geplanten Rückkehr zunächst Run-/Thread-/Automationsidentität prüfen, eine exklusive Auswertungsmarke setzen und diese Automation pausieren. Bei doppeltem Aufruf vorhandenen Abschluss verwenden, keine zweite Auswertung oder Liveaktion starten. Bestätigten Pausierungs-/Einmalstatus speichern. Keine anderen Automationen ändern.

Nur gebundene lokale Abschluss-/Fehler-/Prozessmarker und gespeicherte Dateien lesen. Kein erneuter RX-/TX-/Host-APIkontakt zur nachträglichen Heilung. Fehlt der vollständige Abschluss oder läuft der Runner unerwartet noch, den Lauf ausdrücklich nicht als bestanden ausgeben, Zustand aus lokalen Belegen berichten und keine neuen Livebudgets/Termine erzeugen. Die eigentliche sichere Beendigung muss bereits im Runner liegen.

Die vorhandenen gehashten Leser/Normalisierer/Auswerter auf die tatsächlich erzeugten Originale anwenden. REAL, Boots, Layouts, Rollen und Zeitepochen strikt halten; historische synthetische Offline-Rebindings nicht auf neue Messdaten anwenden. Ursprüngliche Daten und Fehlergründe erhalten. Kein Nachmessen. Getrennt bewerten:

- **Basis und Paarqualität:** feste 473/953 gemeinsame Fenster; Mindestwerte **426/473** beziehungsweise **858/953**, vier TX1-Kontrolllinks je mindestens 5 Hz, unveränderte physische/Identitäts-/Zeitgates. Beide Peerlinks je mindestens **1.920 geeignete Pakete, 2 Hz und 858/953 qualifizierte Fenster**, Hostalter höchstens 250 ms. Fehlende Pflichtbelege NICHT_BEURTEILBAR, schlechte Werte FAIL; Nenner nicht verkleinern.
- **RX1/RX4 und Kontrollen:** tatsächlich installierte Image-/Boot-/Slot-/VALID-/Config-/Diagnosebindungen, RX2/RX3 unverändert, keine TX1-Eingriffe, Default bestätigt und Peerende belegt. HTTP 200 oder ACK allein ist kein Installations-/Empfangsbeweis.
- **Bilanz:** Beobachtung, Formatausschlüsse, ungeklärte Sequenzlücken, PCAP, Serverannahme, Raw-/Peerwriter und deren eigene Nenner getrennt. Keine Funkverlustursache aus Sequenzlücken ableiten. Basis→Paar ist eine kombinierte Firmware-/Peerintervention und kein isolierter Ratensperren- oder Positionsnachweis.
- **Drei vollständige Punkte vom 14.:** Layout/Maske/Firmwarezeit, vollständige Fehler-/Ressourcenbilanz und tatsächlicher Peerbefund einzeln beurteilen. Fehlende RX2/RX3-Attestation, paketkorrelierte Firmwarezeit oder TX-/CPU-/Heap-/Stackproduzenten bleiben offen; kein automatisches 5/5 oder Gesamt-G1-/Tracking-/Radar-PASS aus guten Funkraten.

Abschlussbericht mit Zeiten, tatsächlichem Verbrauch, Original-/Quell-/Programmhashes, vollständigen und fehlenden Endbelegen, Tabellen pro RX/gerichtetem Link und eigenständigen PASS/FAIL/NICHT_BEURTEILBAR-Urteilen liefern. Automationsstatus und einmalige Rückkehr belegen. Entscheidung: bei belastbarem Funk-PASS den kleinsten noch fehlenden Nachweis des 14. priorisieren; bei FAIL eine belegte Hauptursache oder klar begrenzte offene Hypothese samt genauem Folgeauftrag nennen. Keine neue Installation, Messung oder Agentenbeauftragung selbst ausführen.

Harte Grenzen bleiben: TX1/RX2/RX3 und alle Config-/Sicherheitswerte unverändert; keine seriellen/ROM-/manuellen Reset-/Restore-/Provisionierungsaktionen, eFuses, Bootloader-/Partitionsänderungen, Radaraktivierung, fremden Prozesskills, Netzwerk-/System-/Cachebereinigungen oder Git-Clean/Reset/Staging/Commit/Push. Vorhandene Änderungen, Backups und Belege erhalten. Nach einmaligem Bericht stoppen.

Planungsgrundlagen für die einmalige Rückkehr: [OpenAI-Dokumentation zu geplanten Aufgaben](https://learn.chatgpt.com/docs/automations?surface=app) und die aktuell verfügbare Codex-App-Automationsschnittstelle. Rechner/App-Verfügbarkeit und die tatsächliche zukünftige Ausführung zur Laufzeit verifizieren; heute durch die bloße Erstellung dieses Auftrags keine Automation anlegen. Ergebnisorientierte Aufgabenabgrenzung gemäß der ursprünglich genannten [OpenAI-Promptquelle](https://developers.openai.com/blog/rethinking-skills-and-prompts-for-gpt-6-astra).

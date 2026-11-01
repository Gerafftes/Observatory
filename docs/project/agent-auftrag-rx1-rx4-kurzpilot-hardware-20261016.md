<!-- Publication copy: local paths and hardware identifiers were pseudonymized; private original retained. -->
# Agent-Auftrag: 35-Minuten-Hardwarepilot mit 15-Minuten-Peerfrist

Projekt: `.`

Führe nach den ausdrücklich für diesen Auftrag erteilten Freigaben genau einen Hardwarelauf mit dem geprüften Profil **`rx-shortpilot35-v1`** durch. Keine weiteren Agenten, anderen Chats oder zusätzlichen Messreihen. TX1, RX2 und RX3 bleiben unverändert. Der Runner arbeitet selbstständig mit dauerhaften privaten Logs. Nach genau einer Rückkehr in diesen ausführenden Chat ausschließlich offline auswerten und berichten.

## Ziel und Freigaben

**Das ist die offene Aufgabe:** Der bisherige Lauf scheiterte vor OTA und Capture am Hoststart. Hoststartkorrektur, neue Kurzprofilkandidaten und vollständige Zeit-/Abbruchpfade sind inzwischen unabhängig offline geprüft. Echte Vier-RX-Basisdaten und beide RX1↔RX4-Funkrichtungen fehlen für dieses Profil.

**Das erwarte ich:** Frischer Vier-RX-Preflight, 180 Sekunden Basis mit der bisherigen Firmware, RX1 und danach RX4 nacheinander installieren und VALID bestätigen, anschließend 300 Sekunden Paarvergleich. Innerhalb derselben maximal 2.100 Sekunden Peerende und bestätigten Default/TX-only-Endzustand herstellen, Ressourcen schließen und Belege sichern. Danach die gespeicherten Originale einmalig auswerten. Ein Zeit-/Qualitätsfehler erzeugt keinen Zusatzversuch.

Dieses Dokument ist ein vorbereiteter Auftrag. Seine Ausführung benötigt die unmittelbare menschliche Startnachricht mit folgenden getrennten Freigaben für genau diesen neuen Lauf; frühere Langlauf-Freigaben nicht übernehmen:

- **A – Betrieb und Host:** Höchstens 35 Minuten gesamter Liveabschnitt einschließlich Vorlauf, möglicher RX1-/RX4-Unterbrechung, regulärer OTA-Neustarts, maximal drei Hoststarts/-wechsel und aller Endaktionen. Der geprüfte Default/TX-only-Host darf am Ende weiterlaufen. Nur frisch gebundene bestehende Hostidentität beziehungsweise nachgewiesen eigene Kinder behandeln. Ein vorhandenes kurzlebiges Wachhalteprogramm darf ausschließlich an den eigenen Runner gebunden werden und muss mit ihm enden; keine dauerhaften Energieeinstellungen ändern.
- **B – Geräteschreiben:** Höchstens ein authentisierter App-OTA-POST auf RX1 und erst nach dessen erfolgreicher Prüfung höchstens einer auf RX4. Ausschließlich die unten gebundenen neuen 900-Sekunden-Apps in den jeweiligen inaktiven Appslot. Keine Uploadwiederholung, kein Bootloader-/Partitions-/NVS-Schreiben, kein manueller Reset oder Restore.
- **C – Beobachtung und Rückkehr:** Die hier begrenzten Geräte-/Hostabfragen, zwei passive Captures mit 180/300 Sekunden, vier lokale Recordersteuerungen, dauerhafte private Logs und genau eine geplante Rückkehr in denselben Chat zur Offlineauswertung.

Sind A/B/C erteilt, nach bestandenen technischen Gates innerhalb dieses Umfangs selbstständig fortfahren. Keine weitere Grundsatzbestätigung pro Gerät oder Phase. Fehlt eine Freigabe, nur den davon abhängigen Umfang sperren und konkret benennen. Das bloße Lesen dieser Datei setzt keine Freigabe.

## Verbindliche Quellen und Kandidaten

Arbeitsbaum und wirksame `AGENTS.md` zuerst, danach gezielt:

1. `./docs/project/hoststart-kurzpilot-koordinatorreview-20261016.md` und `./private/rx-kurzpilot-koordinatorreview-20261016T193748Z/REVIEW.private.json`: unabhängiger positiver Review mit frisch bestandenen 32 Kurz-/Endtests, drei Zusatzproben und acht echten isolierten Main-Proben.
2. Ausschließlich die Lieferung `./private/rx-hoststart-kurzpilot-offline-20261016-20261016T183145Z/`: `REPORT.private.md`, `HARDWARE-CONTRACT.SHORTPILOT.LOCKED.private.json`, `CANDIDATES-FINAL.private.json`, `SOURCE-MANIFEST.private.json`, `SOURCE-GRAPH-V3.private.json`, `TEST-INDEX.private.json`, `SEAL.private.json` und `evidence/TIME-PROOF.private.json`. Der alte versiegelte Reviewstatus der Übergabe bleibt historisch; den neuen Koordinatorreview als zusätzlichen Nachweis binden, Originaldateien nicht umschreiben.
3. Aus dieser Lieferung die bestehenden `adapter/session_core.py`, `adapter/real_adapter.py`, `scripts/bind_inputs.py`, `scripts/future_runner.py`, `scripts/shortpilot_wrapper.py`, `scripts/host_checks.py`, `scripts/close_log.py` sowie erforderliche Erfassungs-/Auswertehelfer verwenden. Kein verbrauchter Langlaufwrapper, keine zweite abweichende Runtime und keine spontane Quellkorrektur während des Hardwarelaufs.

Feste Laufzeitbindungen:

| Datei innerhalb der Kurzlieferung | SHA-256 |
|---|---|
| `adapter/real_adapter.py` | `98cb0a091940315009b1bf266c9f61f2eae56e3023dd7291af5ca25053066aab` |
| `adapter/session_core.py` | `8617246465b30e8632eb3828494f8a078567300481dd16e452fb2839958d868e` |
| `scripts/bind_inputs.py` | `ea9631c9520ea4455909378dce45c897956ee53ea738e46271a8a9dc118714f9` |
| `scripts/future_runner.py` | `e05286c2cda0b0b0f1da71581daa7c910c01400931e03bf86a63d14e0f7018fe` |

Die zwei Appziele liegen innerhalb derselben Kurzlieferung:

| Ziel | Datei | Byte | BIN-SHA-256 |
|---|---|---:|---|
| RX1 | `build-rx1-peer/esp32-csi-node.bin` | 1.271.936 | `2aedad1fd077157c79c3a5f88dcd7ec95a176bdd57d508bbbef3d105aefd4e42` |
| RX4 | `build-rx4-peer/esp32-csi-node.bin` | 1.271.936 | `a6cce728891cec9df15c2f53a70b77104fa577112028c114d336a0b958b554b7` |

RX1 Runtime-/Partitionsdigest: `81fbe2e67eb7e0c5a9d3987fd3d94386321a46d40cf62727f0639706f0d9d8ea`. RX4: `f4402f2c958bd2f337e777695af96349072a09c01ddc32dd614ea4537a82b04b`. Gemeinsamer Diagnosequellhash: `002f6eca3828ccdff8cb85ccadcdf44412cc84bab40617fa5dd3421477a63d11`. Diese Bindungen unterscheiden sich von den alten 3.000-Sekunden-Kandidaten.

Defaultprogramm: `./private/rx-livevorbereitung-offline-20261016-20261016T143646Z/bin/sensing-server-default`, SHA `0750152f3b66596c5120b6a84bcff00c082e66cbf4bdb4d56cea4564c68cda2c`.

Peerprogramm: `./private/rx-hoststart-kurzpilot-offline-20261016-20261016T183145Z/bin/sensing-server-peer-shortpilot-v2`, SHA `570daa996a4f19373994379a6ebc6835f2f1844c2949fba096155e722f81dba8`.

Normale Programme mit echter monotoner Uhr verwenden; keine `offline-probe`-Programme, Socket-Testbibliothek, `DYLD_INSERT_LIBRARIES` oder synthetischen Gerätedaten in den Livepfad übernehmen. Der geprüfte Startpfad stellt den Sitzungssignierer ausschließlich im Environment eigener Kinder bereit. Signatur-, Permit-, Host-/Origin- und Authentisierungsprüfungen erhalten. Keine produktiven Signiersecrets lesen oder globale Umgebung ändern.

Historische Rückweg-/Bootloader-/Partitions-/Acht-Felder-Belege nur über die vorhandenen Bindungen gezielt verfolgen, insbesondere `./private/rx1-rx4-c1-vier-rx-restart-20261014T175514Z/` und die durch das Kurzmanifest referenzierten Nachweise. Keine neue Backup-/ROM-/Firmwareanalyse. Keine Neubauten, Toolinstallation oder allgemeine Offlinerunde aus diesem Auftrag ableiten. Bei konkreter Lieferlücke vor Eingriff stoppen und sie belegen.

## Hardware, Vorbereitung und gemeinsame Budgets

TX1 und RX1–RX4 an ihren festen dokumentierten Messpositionen am Strom. Kein USB-/COM-Datenkabel zu den ESPs erforderlich. Mac am Netzteil im bestehenden Projekt-Netzweg; Rechner und Codex-App bleiben für den Hintergrundlauf und die lokale Rückkehr verfügbar. Keine Radaraktivierung, kein Umbau des WLANs, Routings oder produktiven Netzes.

Offlinevorbereitung **vor T0** abschließen: Lieferung/Hashes prüfen, bestehende Referenzen zuordnen, kleine neue private Lauf-/Wrapperablage und Eingaben vorbereiten, aktuelle Automationsschnittstelle und ausführenden Chat identifizieren, vorbereitete Orchestrierung mit vorhandenen kleinen Offlineprüfungen binden. Keine Geräte-/aktuellen Interface-/Routen-/Prozessbeobachtungen als ungezählten Vorlauf tarnen.

Vor neuen privaten Dateien effektive Git-Ausnahme und bereits versionierte Pfade prüfen. Frische Ablage etwa `./private/rx-kurzpilot-hardware-20261016-<UTC>/` und gebundene private Wrapperablage; 0700 für Verzeichnisse, 0600 für Daten. Bestehende Lieferungen, Programme, Backups und Fehlerbelege erhalten. Geheimnisse, originale Geräteantworten, Configexporte, IQ und PCAP bleiben privat; keine Tokens/Signierschlüssel in argv, Chat, Automationsprompt oder allgemeinen Logs.

**T0 liegt unmittelbar vor der ersten aktuellen Live-Preflightaktion.** Monotonen und Unixanker zusammen sichern. Alle Vorlauf-, Interface-/Nachbarcache-/Direktrouten-/Prozess-/Portprüfungen, RX-Abfragen, Autorisierungs-/Schedulerübergaben und Runnerkosten zählen ab diesem T0. Kein Reset der Uhr oder Zähler beim Hintergrundstart.

| Aktion | Gesamte Grenze dieses Laufs |
|---|---:|
| Gesamter Liveabschnitt einschließlich Ende/Cleanup | **2.100 s / 35 min ab T0** |
| Vorlauf bis Sessionübergabe | **höchstens 145 s**, zusätzlich tatsächlichen Restpfad prüfen |
| Reserviertes Ende | **150 s: zwölf End-GETs × 10 s + 30 s Cleanup** |
| RX-GETs | **höchstens 120 insgesamt**, davon zwölf End-GETs reserviert; normal 82 plus geplanter Vorlauf zwölf = 94 |
| App-OTA | **höchstens zwei**, zuerst RX1, dann RX4, jeweils **20 s** |
| VALID-Serie je Ziel | **0/60/120 s**, 10 s je GET, **131 s Phasenbudget** |
| Captures | **genau höchstens zwei**, vollständige Messfenster **180/300 s**, jeweils höchstens 20 s Zusatzbudget |
| Recordersteuerungen | **höchstens vier**, je Aufnahme ein Start und ein Stop |
| Hoststarts/-wechsel | **höchstens drei**, jeweils **20 s**, auch bei anfangs abwesendem Host |
| Zusätzliche lokale Status-/Node-GETs | **höchstens acht**, außerhalb Captures, nur die zwei gebundenen Endpunkte |
| Firmware-Peerfrist | **900 s seit jeweiligem Boot**, RX1 und RX4 |
| Host-Peerfrist | **360 s**, Hostwechsel/Paar/Default-Rückkehr müssen hineinpassen |

Jeder Versuch verbraucht sein Budget vor Dispatch, auch bei unklarer Antwort. Die vollständige sekundengenaue Phasentabelle ist im gebundenen Zeitnachweis festgelegt; konservatives Ende **T0 + 2.097 s**, nur drei Sekunden bis zur absoluten Grenze. Keine dieser Obergrenzen verlängern. Restpfad, früheste RX1-Peerfrist, späteste beider Bootfristen und Endreserve vor jeder Phase und dem tatsächlichen Dispatch prüfen. Ein zu langsamer Upload/Start oder eine nicht mehr vollständig passende Aufnahme führt zum begrenzten Abbruch, nicht zu verkürzten Daten oder einer neuen Uhr.

Startbedarf **70.922.518.528 Byte** auf den tatsächlich gemeinsamen Zielpfaden frisch bestätigen. Volume, Pfade, Blockgröße und freier Platz müssen zum gebundenen Guard passen. Vor Credentials und Dispatch sowie danach wie implementiert prüfen. Keine Reserve senken, keine Cache-/Cloud-/Swap-/Backupbereinigung oder alten Löschfreigaben übernehmen. Die bisherige Freiplatzbeobachtung ist historisch.

## 1. Frischer Preflight und neue Laufbindung

Ab T0 ausschließlich die freigegebenen begrenzten passiven Interface-/Nachbarcache-/Direktrouten-/Prozess-/Portprüfungen und bekannten RX-GETs ausführen. Kein Ping, Scan, ARP-Probe oder geratenes DHCP-Ziel. Vor Credentials aktuelle bekannte physische MAC/Node/Interface/Direktroute eindeutig binden. Fehlender Cacheeintrag ist ein Stopgrund und allein kein Ausfallbeweis.

Alle vier RX frisch auf Image, Boot, Slot/VALID, Kanal/Zieltupel, TX-Quelle, Diagnose und acht typgenaue Configwerte binden: `target_ip`, `target_port`, `node_id`, `csi_channel`, `edge_tier`, `tdm_slot`, `tdm_total`, `filter_mac_configured`. Bestehende unabhängige Referenzen verwenden; Widersprüche nicht durch neue Referenz oder Konfigurationsänderung passend machen. RX2/RX3 bleiben auf dem frisch bestätigten Image/Boot/Configstand. TX1 nicht separat per HTTP oder seriell kontaktieren/verändern; vorhandene Quellen-/Setupbindung und empfangene Daten verwenden.

Vor beiden späteren Uploads passenden nachgewiesenen Rückweg, Partitionstabelle, Boot-/Slotzustand und den jeweiligen gebundenen Appkandidaten bestätigen. Ohne belegten Rückweg kein Upload. Keine neue serielle Sicherung, kein Restore oder Rollbackexperiment.

Anfangshost ausdrücklich vorhanden oder abwesend binden. Vorhanden: PID, Generation, Binaryhash, argv, CWD, Inputs und exklusive UDP-/HTTP-/WebSocket-Besitzbindung. Abwesend: geprüfter aktueller Abwesenheitsnachweis und erneute Prüfung unmittelbar vor Start. Ein fremder/inzwischen erschienener Besitzer sperrt; kein fremder Kill.

Nur mit dem neuen Kurzprofilbinder frischen **REAL-Vertrag** und neue Pfad-/Kandidaten-/Programm-/Profil-/Exportbindungen erzeugen. Der Binder liefert zunächst falsche Grants. A/B/C und den positiven Koordinatorreview anschließend als separate belegte Autorisierung für genau diesen Vertragsdigest und höchstens T0 + 2.100 s binden. Keine synthetische Testberechtigung oder alten Livevertrag kopieren. Original-T0 und sämtliche vorverbrauchten GETs in `restore_session` übernehmen.

Hostargumente aus dem neuen Binder erhalten: privater Datenpfad, `--source esp32`, `--rx-session-only`, `--no-edge-registry`, `--server-control-port 0`, gebundene schreibgeschützte UI sowie kurzlebiges passendes Launch-Permit. Nur die geprüften Quellen/Klassen/`Session.execute` ausführen.

## 2. Genau eine Rückkehr und ein unabhängiger Runnerstart

Vor dem unbeaufsichtigten Interventionsablauf genau eine neue bestätigte Rückkehr über die aktuelle Codex-Automationsschnittstelle anlegen: **Heartbeat im selben ausführenden Chat, `destination=thread`, einmaliger zukünftiger Termin bei T0 + 2.220 s = 37 min**. Die zwei Minuten Abstand sind ausschließlich für abgeschlossene Logs und erlauben keine weitere Liveaktion. Tatsächliche UTC-/Europe-Berlin-Uhrzeit aus dem gespeicherten Anker berechnen; keinen festen Termin oder historischen Chat-ID-Wert aus früheren Läufen übernehmen.

ID, eigener Zielchat, Run-ID, Einmaligkeit, Rückkehrzeit und Eingabepfade privat binden. Keine alte Messautomation reaktivieren und keinen Ersatz-Cronjob/neuen Chat anlegen. Ist genau diese Rückkehr nicht verlässlich bestätigbar oder der Vorlauf inzwischen zu lang, unbeaufsichtigten Ablauf nicht starten.

Der menschlich lesbare Automationsprompt muss enthalten:

> Dies ist die einmalige Offlineauswertung des RX-Kurzpilots mit der gebundenen Run-ID in diesem Chat. Setze vor Auswertung eine exklusive Auswertungsmarke und pausiere diese Rückkehr nach ihrem einzigen Aufruf. Lies nur gebundene lokale Abschluss-/Fehler-/Prozessmarker und Originaldateien, werte aus und berichte Ergebnis, offene Nachweise und nächsten Schritt. Keine neuen Geräte-/Host-APIabfragen, OTA, Resets, Captures, Hostwechsel, weiteren Agenten, anderen Chats oder Folgetermine. Fehlender Abschluss erteilt keinen Ersatzlauf.

Den geprüften `future_runner.py` genau einmal mit geschlossenem stdin, eigener Prozesssitzung, privaten begrenzten stdout/stderr/Journaldateien und einmaligem exklusivem Verbrauchsmarker starten. Geprüfte Quell- und Inputbindungen erhalten. Eventueller freigegebener Wachhalter nur für diesen Prozess. Genau einen begrenzten Startnachweis prüfen: gebundene Run-ID/Phase, dauerhafte Logs und tatsächlicher Runnerzustand; PID allein genügt nicht. Bei Startfehler keine Wiederholung, ungenutzte eigene Rückkehr pausieren, gesicherte Logs offline auswerten und berichten.

Nach erfolgreichem Start nur Beginn, maximale Livezeit und **bestätigten Berliner Rückkehrtermin** nennen, dann den Agententurn beenden. Währenddessen kein fortlaufendes Tailen/Pollen, keine langen Agenten-Warteschleifen. Der Runner selbst muss Zeit-/Mengen-/Speicher-/Prozessprüfungen und sichere Abbrüche durchführen. Eine verspätete Agentenrückkehr verlängert keinerlei Freigaben.

## 3. Fester automatisierter Ablauf

1. **Basis:** Default-Host innerhalb des verbrauchten Hostbudgets bereitstellen; über die begrenzten lokalen Status-/Nodeprüfungen vier reale RX mit tatsächlichem Empfang bestätigen. Vollständige Vier-RX-Vorklammer, **180 s Basis** vor Installation, Nachklammer. Keine simulierte Quelle. Keine zusätzlichen Geräte-/Host-GETs während Capture einschließlich Vor-/Nachlauf/Schluss; nur gezählte Recordersteuerungen und geprüfte interne Kontrollen. Einschwingen nicht doppelt gutschreiben.
2. **RX1:** unmittelbar vor Upload erneut Identität, Boot, Image, Config, inaktiven Slot und Rückweg bestätigen. Genau ein authentisierter roher App-OTA-POST, höchstens 20 s. Neuer konsistenter Boot, erwartete App/Slot und VALID-Serie bei 0/60/120 s; danach acht Configwerte, Diagnosequellbindung, Profilkennung und 900-s-Frist bestätigen. Bei Fehler keine RX4-Installation.
3. **RX4:** derselbe einmalige Ablauf, erst nach erfolgreichem RX1-Abschluss. Kein paralleler Upload, keine verkürzte VALID-Folge.
4. **Paar:** vollständige frische Vier-RX-Vorklammer, Wechsel zum 360-s-Peerhost, **300 s Paaraufnahme**. RX1→RX4 und RX4→RX1 getrennt sowie vier TX1-Kontrolllinks erfassen. Unverändert OFDM6, 50 gesamt/40 TX1/10 Peer und 6/2-Poolaufteilung. Keine Raten-, Platzierungs- oder Konfigurationsversuche.
5. **Ende:** Default/TX-only vor Paar-Nachklammer bestätigt wiederherstellen, Peerhost beenden, Firmware-Peerende anhand der konservativen gebundenen Bootfristen abwarten. Zwölf End-GETs bestätigen alle Images/VALID/Configs und beide `session_ended`; Capture-/Recorder-/Writerenden und Cleanup innerhalb des Gesamtbudgets sichern. Die neuen Apps dürfen auf RX1/RX4 installiert bleiben, ihre Peersitzungen müssen beendet sein. Kein automatischer Restore. Default darf gemäß A weiterlaufen.

Native Speicher-/Phasen-/Abbruchkorrekturen erhalten. Bei fehlendem Platz, Timeout, falscher Identität, unerwartetem Boot/Slot/Config oder Writer-/Capturefehler weitere normale Arbeit sperren. Nur bereits autorisierte begrenzte Endaktionen sowie nachgewiesen eigene Kinder behandeln. Fehlende Bestätigung bleibt UNBESTÄTIGT; kein erfolgreicher Default-/Peerendzustand aus einem bloßen Stop ableiten.

## 4. Originale, einmalige Offlineauswertung und Abnahme

PCAP, Raw-/Peeroriginale, Hosttraces, Recorder-/Writerenden, Status-/Config-/Diagnoseantworten mit Anfrageintervallen, ursprüngliche T0-/Unixanker, Boot-/Layout-/Quellepochen, Prozess-/Budget-/Speicherjournale und Fehlerpräfixe erhalten. Geschlossene Dateien nach Größe/Hash binden. Einen weiterlaufenden Default-Log nur über sein gebundenes geschlossenes Präfix hashen, nicht als unveränderlich abgeschlossene gesamte Datei ausgeben. Keine großen Exportduplikate oder stille Trunkierung als Erfolg.

Bei der bestätigten Rückkehr eigene Run-/Thread-/Automationsbindung prüfen, exklusiven Auswertungsmarker setzen und diese eine Automation pausieren. Nur gespeicherte lokale Belege auswerten. Bei doppeltem Aufruf vorhandenen Abschluss verwenden; keine neue Messung. Fehlt ein Endbeleg oder läuft der Runner laut lokalen Markern unerwartet weiter, dies offen melden; keine nachträglichen Geräteabfragen, Zusatzbudgets oder Rückkehrtermine.

Mit den gebundenen Kurzprofil-Lesern, Normalisierern und Auswertern getrennt bewerten:

- **Basis:** feste **173** gemeinsame Fenster, mindestens **156/173**, vier TX1-Kontrolllinks jeweils mindestens 5 Hz; unveränderte Layout-/Masken-/Identitäts-/Zeitgates.
- **Paar:** feste **293** gemeinsame Fenster, mindestens **264/293**; beide Peerlinks jeweils mindestens **600 geeignete Pakete**, 2 Hz und **264/293** qualifizierte Fenster, Hostalter höchstens 250 ms. Vier TX1-Kontrolllinks weiterhin jeweils mindestens 5 Hz. Fehlende Daten/ausgeschlossene Formate verkleinern die Nenner nicht.
- **Betrieb:** RX1/RX4 erwartetes Image/Boot/Slot/VALID/Config/900-s-Profil, RX2/RX3 unverändert, keine TX1-Eingriffe, Default und Peerende belegt, alle Capture-/Recorderressourcen geschlossen. HTTP 200 oder ACK allein genügt nicht.
- **Bilanz:** Transportbeobachtung, Formatausschlüsse, ungeklärte Sequenzlücken, PCAP, Serverannahme und einzelne Writer mit ihren eigenen Nennern getrennt. Keine behauptete Funkverlustursache allein aus Sequenzlücken. Basis→Paar ist eine kombinierte Firmware-/Peerintervention, kein isolierter Ratensperren-A/B- oder Positionsnachweis.
- **Grenzen der Tagesabnahme:** Kurzpilot, tatsächliche Paarstrecken, reale Vier-RX-Layout-/Masken-/Firmwarezeitbindung und vollständige Fehler-/Ressourcenbilanz getrennt. Fehlende Physicalattestation oder Firmware-/TX-/CPU-/Heap-/Stackbelege bleiben offen. Kein automatisches „14. fertig“, 5/5, Gesamt-G1, Radar- oder Tracking-PASS.

Qualitätsmangel = FAIL; fehlender Pflichtbeleg = NICHT_BEURTEILBAR. Technisch beendeter Transport ist kein Qualitäts-PASS. Abschlussbericht mit tatsächlichen Zeiten ab T0, Aktionsverbrauch, pro RX/gerichtetem Link Raten/Fenstern, Identitäten, vollständigen/fehlenden Endbelegen, Quell-/Programm-/Originalhashes und bestätigtem Automationsstatus liefern.

Entscheidung im Bericht: Bei belastbarem Funk-PASS kleinsten noch fehlenden Nachweis priorisieren. Bei FAIL eine belegte Ursache oder klar begrenzte Hypothese samt genauem nächsten Auftrag benennen. Keine neue Installation, Reparatur, Messung oder Agentenbeauftragung selbst aus dieser Entscheidung ableiten.

Harte Grenzen: TX1, RX2, RX3, alle acht RX-Configwerte und sämtliche Sicherheitseinstellungen unverändert. Keine manuellen Resets, seriellen/ROM-/Restore-/Provisionierungsaktionen, eFuses, Bootloader-/Partitionsschreibzugriffe, Radaraktivierung, fremden Prozesskills, Netz-/System-/Cachebereinigungen oder Git-Clean/Reset/Staging/Commit/Push. Originale und vorhandene Änderungen erhalten. Nach einmaligem Bericht stoppen.

Grundlage für die Rückkehr im bestehenden Chat: [offizielle OpenAI-Dokumentation zu geplanten Aufgaben](https://learn.chatgpt.com/docs/automations). Aktuelle Toolantworten müssen den tatsächlichen einmaligen Termin und richtigen Zielchat bestätigen; diese Auftragserstellung legt keine Automation an.

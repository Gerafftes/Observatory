<!-- Publication copy: local paths and hardware identifiers were pseudonymized; private original retained. -->
# Agent-Auftrag: RX1/RX4 auf C1 und gemeinsamer Vier-RX-Lauf

Projekt: `.`

**Freigabestatus: A und B einschließlich des beschriebenen 30-Minuten-Vier-RX-Leselaufs sind ausdrücklich erteilt.** Johann hat am 14. Oktober 2026 im Koordinatorchat bestätigt: „Beide Freigaben A und B für RX1 und RX4 einschließlich des beschriebenen 30-Minuten-Vier-RX-Leselaufs sind erteilt.“ Die Freigaben gelten einmalig ausschließlich im nachstehenden Umfang. Keine erneute Grundsatzbestätigung verlangen; technische Gates und tatsächliche Hardwarezuordnung bleiben erforderlich. Kein Agent wurde durch den Koordinator gestartet, keine Hardwareaktion durch den Koordinator ausgeführt.

## Ziel und Ausgangslage

**Das ist der Fehler:**

Der letzte vollständige Vier-RX-Lauf bestand nur 97/1.793 gemeinsame Fenster (5,41 %). RX1 und RX4 lieferten mit der bisherigen App nur 4,007 beziehungsweise 3,307 passende 128er-CSI/s. Die Formatkorrektur ist auf RX2 und inzwischen auch auf RX3 praktisch bestätigt: RX3 mit C1 erreichte im 120-s-Piloten 16,833 passende CSI/s und 113/113 eigene Fenster; RX2 als unveränderte C-Kontrolle 15,575/s und 113/113. Alle gemeinsamen Zweiknoten-Pilotfenster bestanden. RX3 war am Laufende am 14. Oktober 2026 um 18:46:18 Europe/Berlin C1 / ota_0 / VALID, Config erhalten. Diese historischen Belege ersetzen keine aktuelle Gerätebindung.

**Das erwarte ich:**

1. Eigene aktuelle Rückweg-, Geräte-, Config- und Slotbindungen für RX1 und RX4.
2. Genau eine C1-App-OTA-Installation je RX, nacheinander: RX1 vollständig bestätigen, danach RX4.
3. Genau einen vollständigen 30-Minuten-Lauf mit TX1 und allen vier RX, einschließlich Vor-/Nachlauf und Endprüfung.
4. Getrennte Urteile für Rückweg, Installation, Betrieb und gemeinsame CSI-Qualität. Keine Positions-/Server-/Peerabnahme aus dem Capture ableiten.

TX1-Firmware und -Konfiguration bleiben unverändert, RX2 bleibt auf Original C, RX3 auf C1. Johann darf TX1 vor Beginn des Liveauftrags vom Wartungsplatz an seinen normalen Standort zurückstellen und über den üblichen USB-Stromanschluss an einem vorhandenen Netzteil versorgen; keine Mac-Datenverbindung nötig. Dies ist eine Vorbereitung durch Johann, keine zusätzliche TX1-Aktionsfreigabe für den Agenten. Keine Geräte-Konfigurationsänderung. Peerprofil bleibt deaktiviert. Starte keine weiteren Agenten, schreibe keine anderen Chats an und lege keine Automation an. Nach dem Abschlussbericht stoppen.

## Freigabeumfang und feste Grenzen

Die folgenden Freigaben sind **erteilt**. Quelle ist Johanns oben zitierte direkte Nachricht im Koordinatorchat vom 14. Oktober 2026. Quelle und Umfang einmalig privat dokumentieren und nach Offlinevorbereitung sowie erfüllten technischen Gates selbstständig ausführen. Alte RX3-Freigaben und Verbrauchsmarker bleiben beendet; diese Nachricht autorisiert ausschließlich den neuen RX1-/RX4-Gesamtauftrag. Keine Dauerfreigabe und keine Erweiterung bei Fehlern.

| Abschnitt | Erteilter Umfang | Harte Grenze |
|---|---|---|
| A: RX1-/RX4-Rückweg | Betriebsunterbrechung und begrenztes ROM-Lesen ohne Stub bei 115200; danach normal zur bisherigen App zurückkehren | Je RX höchstens 15 Minuten, zusammen höchstens 30 Minuten |
| B: RX1-/RX4-Installation | Betriebsunterbrechung einschließlich OTA-Neustart und jeweils genau ein authentisierter C1-App-OTA-POST | Je RX höchstens 15 Minuten, zusammen höchstens 30 Minuten |
| C: gemeinsamer Leselauf | Vor-/Nachher-GETs und ein passiver Vier-RX-Capture am bestehenden Host; ausdrücklich zusammen mit A/B für diesen Gesamtauftrag freigegeben | Höchstens 45 Minuten aktiver Abschnitt, davon genau 1.800 Sekunden Messzeit |

A/B gelten ausschließlich für RX1 und RX4. C enthält ausschließlich lesende RX-GETs und passive lokale Bindungs-/Captureaktionen. **Keine manuelle Flash-/Restorefreigabe.** App-OTA schreibt ausschließlich den frisch gebundenen inaktiven Appslot und die regulären OTA-Auswahlmetadaten; Bootloader, Partitionstabelle, bisheriger Appslot, NVS und Sicherheitswerte sind keine Schreibziele.

Globale monotone Livegrenze: **höchstens 120 Minuten** ab erster aktuellen Netz-/Geräte-/Portaktion bis letztem Cleanup/End-GET, einschließlich Kabelwechsel und Wartezeit. Phasengrenzen zusammen höchstens 105 Minuten; verbleibende 15 Minuten sind keine zusätzlichen Geräteaktionen. Offlinevorbereitung vorher und Offlineauswertung nachher zählen nicht zur Livezeit. Vor jedem Abschnitt Zeit für Restablauf, normalen Rückkehrstart, Capture und Endprüfung reservieren; C benötigt vor seinem Start noch 45 Minuten. Deadline niemals durch neuen Prozess oder Ausgabeordner zurücksetzen.

Gesamtbudgets: höchstens **64 RX-GETs**, **zwei OTA-POSTs (maximal einer pro Ziel-RX)** und **ein Capture**. Je RX höchstens ein ROM-Eintritt, fünf initiale Syncversuche ohne erneuten Hardware-Reset, ein normaler A-Rückkehrstart und bei nachgewiesenem automatischem Rückkehrpfad höchstens ein zusätzlicher normaler B-Rückkehrreset. Je RX maximal vier Flashbereichslesungen und 72 KiB, zusammen maximal acht Lesungen und 144 KiB. OTA-eigene Neustarts separat zählen. Alle Fehlaktionen verbrauchen ihr Budget; keine Upload-/Capture-/Port-/Baudretryspirale.

## Relevante Eingänge – gezielt lesen

Arbeitsbaum und wirksame `AGENTS.md` zuerst prüfen. Vorhandene Änderungen, Backups, Quellen, Builds, Manifeste und historische Berichte erhalten. Keine Git-Bereinigung, Staging, Commits oder Pushes. Pfade im Folgenden relativ zum Projekt.

1. `docs/project/rx3-c1-pilot-review-20261014.md` und `docs/project/rx134-c1-review.md`: bereits geprüfter Pilot und gemeinsamer Kandidat. Keine neue allgemeine Firmwareentwicklung, kein SDK-Neubau.
2. `private/rx3-rueckweg-c1-pilot-20261014T163119Z/`: zunächst `REPORT.private.md`, `DELIVERY.private.json`, `execution-seal.private.json`; danach gezielt `guards.py`, `rom_readonly.py`, `bounded_upload.py`, `pilot_capture.py`, `pilot_analyze.py`, benötigte Tests und gebundene Bootloaderreferenzen. `references.private.json`, `pilot-inventory.private.json` und zugehörige Originalantworten liefern die neueren RX2-/RX3-Bindungen. **`pilot.py` niemals starten:** sein RX3-Auftrag ist beendet und seine Freigaben verbraucht.
3. `private/tag2-abschluss-vier-rx-peer-20261013T184515Z/`: `frozen-config-references.private.json`, `pre-inventory.private.json`, `capture-config.private.json` und benötigte Originalantworten; dazu der getestete Vier-RX-Pfad `live_adapter.py`, `bounded_http.py`, `capture_once.py`, `preflight.py`, `contracts.py`, `analyze.py`, `analyze_strict.py`, `pcap_offline.py` und Tests. Keine alte Liveeinstiegsdatei ausführen.
4. `private/rx134-c-eignung-offline-20261014T110737Z/`: Kandidaten-BIN, Delivery, nötige Manifest-/Image-/Buildbindungen. Alte Bootloaderreferenzen mit BIN/ELF/Map/config gezielt verwenden; die RX3-Leseprobe selbst ist kein Beweis des RX1-/RX4-Bootloaders.
5. `docs/project/tracking-zeitplan-20261025.md`: nur G1 und unmittelbar betroffene offene Nachweise. Keine breite Chat-, Historien-, NVS- oder Backupinventur. Kein ProjectAtlas, keine Downloads, Installation oder neuen Abhängigkeiten.

Die acht Configfelder sind `target_ip`, `target_port`, `node_id`, `csi_channel`, `edge_tier`, `tdm_slot`, `tdm_total`, `filter_mac_configured`. RX1/RX4 besitzen inzwischen vollständige eingefrorene Acht-Felder-Referenzen aus dem abgeschlossenen Vier-RX-Lauf; nicht erneut nach angeblich fehlenden historischen Exporten suchen. Ihre Herkunft als damalige frische Startreferenz erhalten. RX2/RX3 zusätzlich gegen die neueren Pilotbelege prüfen. Bei abweichender aktueller Config keine neue Baseline einfrieren und keinen Umbau vornehmen: stoppen.

Neue private Ablage, beispielsweise `private/rx1-rx4-c1-vier-rx-<UTC>/`: wirksamen Ignore **und** Nichtversionierung zuerst prüfen, Ordner 0700/Dateien 0600. Originaldateien nicht verändern. Reale Endpoint-/MAC-/Boot-/Configwerte, Tokens, Rohantworten, ROMdaten und IQ bleiben privat und aus Toolausgaben heraus. Keine Tokens in Prozessargumenten. Neue Referenzkopien mit Herkunft und Hash binden, kein altes Setup oder Kalibrierungsartefakt überschreiben.

## 1. Offlinevorbereitung vollständig abschließen

Den erfolgreich ausgeführten RX3-ROM-/Uploadpfad in einer neuen lokalen Kopie gezielt für genau RX1 und RX4 anpassen. Keine neue Sync-/Baudimplementation. Labels, menschliche Portzuordnung, erwartete MAC, Runtime-/Diagnosehash, Rückweg und einmalige Budgets pro Ziel explizit führen; keine globale RX3→RX1-Ersetzung ohne Tests. Den bestehenden Vier-RX-Collector erhalten und nur die bewusst freigegebene Diagnosebindung für die drei C1-RX ergänzen. Ein Zweiknotenmodus darf nie als G1 laufen.

Vor Livebeginn Fake-I/O-Tests ausführen und Eingänge, Helfer, Referenzen, Freigabequelle und Kandidat versiegeln. Nötige Fälle:

- Fehlende A/B-Freigabe, ungebundener/mehrdeutiger Port, falsche MAC/Node, fremdes Ziel oder unzulässiger ROMbefehl verhindern Geräteaktionen.
- Eigene Rückweg-/Config-/VALID-Gates je RX; RX1 ungeklärt verhindert RX4. RX3s Rückweg nicht auf RX1/RX4 übertragen. Nur exaktes C1 auf den richtigen inaktiven Appslot zulassen.
- Ein POST pro RX, Timeout/Teilübertragung ohne Retry; alte App/Rollback und unerreichbares Gerät getrennt bewerten. Je C1- und C-Diagnose eigene erwartete Hashes, geerbten Rollenstring nicht zur physischen Gerätezuordnung verwenden.
- Genau vier eindeutige RX im Filter, frische Boot-/Config-/Zieltupel, Endprüfung für alle vier; Teilmenge oder widersprüchliche Quelle verhindert Capture. Kein HTTP/UART während Capture.
- Monotone globale und Phasendeadlines, reservierter Rückkehr-/Endprüfungsspielraum, einmalige Verbrauchsmarker, Vor-/Nachlauf und Captureende. Bestehende Epochengrenzen, Formatausschlüsse und Sequenznenner bleiben erhalten.

Nur danach den neuen Liveeinstieg genau einmal ausführen. Alte Einstiege und Verbrauchsmarker bleiben unangetastet. Keine Freigaben durch einen neuen Ordner erneuern. Testartefakte und reproduzierbare genaue Befehle privat liefern.

## 2. Hardware und frische Startbindung

Benötigt: Mac mit vorhandenem Projekt-Netzweg, TX1 sowie RX1–RX4 eingeschaltet. **RX1 und RX4 nacheinander über ihren eindeutig zugeordneten COM-/UART-Datenanschluss zum Mac**, mit stabiler Versorgung. Alle RX behalten eine vom wechselnden COM-Datenkabel unabhängige übliche Stromversorgung. TX1/RX2/RX3 benötigen für diesen Auftrag keine serielle Verbindung. Beide Radare werden nicht gebraucht.

Johanns Vorbereitung vor dem Livebeginn:

1. TX1 bei Bedarf an seinen normalen Standort stellen und über den üblichen USB-Stromanschluss mit einem vorhandenen USB-Netzteil versorgen. USB zum Mac ist nicht erforderlich. Nach einem Stromwechsel etwa zwei Minuten für Start und Wiederverbindung einplanen; Zeitablauf allein ist kein Funktionsnachweis.
2. RX1–RX4 durchgehend über ihre üblichen Stromanschlüsse versorgen. RX2/RX3 in ihrer bestehenden Aufstellung lassen, keine Resetknöpfe drücken. RX1/RX4 müssen beim späteren COM-Kabelwechsel eingeschaltet bleiben.
3. Zunächst ausschließlich RX1 über COM/UART als Zielgerät mit dem Mac verbinden und dem Agenten die Zuordnung bestätigen. Ein noch angeschlossenes RX3-COM-Kabel vorher vom Mac lösen, ohne RX3s Stromversorgung zu unterbrechen. Keine zusätzlichen Datenports nötig.
4. Mac im bisherigen Projekt-WLAN und am bestehenden Host-Netzweg betreiben. TX1s USB-Verbindung war für Strom/Diagnose; die Messdaten kommen über WLAN. Keine neuen Netzparameter oder Hostdienste einstellen.
5. Erst nach ausdrücklicher Meldung des Agenten, dass RX1 vollständig bestätigt und der Port geschlossen ist, das COM-Datenkabel zu RX4 wechseln und die neue Zuordnung bestätigen. Während der anschließenden Messung nichts umstecken oder bewegen.

Eine Rückkehr von TX1 aus dem Wartungsplatz verändert die Funkaufstellung und kann durch den Stromwechsel einen neuen TX-Boot und neue DHCP-Endpunkte verursachen. Vorherige Erreichbarkeit oder direkte Vergleichbarkeit nicht voraussetzen. Die neue Messbasis erst nach dieser Vorbereitung frisch binden, den tatsächlichen Standort als Nutzerangabe privat festhalten und den früheren Wartungsaufbau als historische Abweichung erhalten. Der Agent stellt TX1 nicht selbst um und greift weder seriell noch schreibend auf TX1 zu. Konfigurationsreferenzen bleiben unverändert; tatsächliche neue Endpunkte nur über die bekannte MAC-/Routenbindung übernehmen. Kein alter PCAP-/Boot-/Zeitabschnitt wird mit dem neuen Lauf verbunden. Der normale Standort allein beweist noch keine vermessene Geometrie oder Kalibrierung.

Johann stellt die Zielkabelzuordnung her. Falls nur ein Datenkabel verfügbar ist, RX1 vollständig beenden und Port schließen; dann auf Johanns bestätigte Zuordnung zu RX4 warten. **RX1 muss beim Abziehen des COM-Datenkabels über seinen üblichen separaten Stromanschluss weiter versorgt bleiben**; falls das Datenkabel seine einzige Versorgung ist, vorab die erforderliche stabile Versorgung mit Johann klären, keinen ungeplanten Stromausfall erzeugen. Kein serielles Öffnen verschiedener Ports zur Gerätesuche. Passive USBinventur verwenden und anschließend im ROM sofort S3-/MACidentität prüfen. USB-/Portbindung nach jedem Kabelwechsel neu binden. Ab erster Liveaktion die vorbereitete Aufstellung und Versorgung beibehalten; TX1, RX2, RX3 weder umstecken noch resetten oder bewegen. Keine eigenmächtige Kabel-, Strom- oder Positionsänderung.

Vor irgendeinem ROM-Eintritt alle vier RX frisch am bestehenden Interface binden: bekannte MAC, eindeutiger Endpoint, direkte Route, Statusnode, bekanntes Image, VALID, laufender/inaktiver Slot, Config und gemeinsame TX-Quelle. Historische IPs sind Hinweise, keine aktuelle Identität. Nur passive lokale Interface-/Nachbarcache-/Direktroutenabfragen; keine Pings, Scans, ARP-Probes oder Netzänderungen. Fehlender Cache/Endpoint → vor Eingriff stoppen, kein Defekturteil und keine Adressraterei.

Anfangszustände erwarten, aber frisch prüfen:

| Gerät | Runtime-/Partitionsdigest | Laufender Slot / Zustand |
|---|---|---|
| RX1, RX4 bisherige App | `e09d3a227dd7419b32009361b4d02ca49fa536f2a8d28816525854493a90b354` | Historisch ota_1 / VALID; aktuelle Tabelle/Status maßgeblich |
| RX2 Original C | `e674e9fa055c05037c95a9df7c99dd2097b2c3544016ea99794888721a6a191a` | ota_0 / VALID |
| RX3 C1 | `9f5afbc5cdb040d4891f70368482a0aa1bd104f995c190588bdbbf20083f00fd` | ota_0 / VALID |

RX2/RX3 müssen bis zur Endprüfung ihren frisch gebundenen Boot und ihre Config behalten. TX1-Quellenbindung aus bestehenden Belegen und CSI prüfen; keine TX1-Status-/UARTaktion in diesem Auftrag. Unerwarteter Anfangsstand → kein automatisches Überschreiben, sondern dokumentierter Abbruch.

## 3. RX1 vollständig bearbeiten, danach RX4

Für **jeden RX einzeln in dieser Reihenfolge** A und B durchführen; erst nach bestätigtem gesundem C1-Betrieb auf RX1 mit RX4 fortfahren.

**A – Rückweg:** Nach frischer gesunder alter App-/Configbindung genau den zugeordneten COM-Port öffnen. Ein ROM-Eintritt, 115200, ohne Stub, höchstens fünf initiale Syncversuche ohne weitere Resets. Vor Flashlesung S3, erwartete MAC und zulässigen Sicherheitszustand prüfen. Keine eFuse-/Securityänderung oder Payload-Dumps.

Höchstens zweimal den begrenzten Bootloaderbereich bis 32 KiB lesen. Imageheader, Segmente, Prüfsumme und angehängten Hash gegen exakt gebundene lokale BIN sowie ihre tatsächlichen ELF/Map/config-Belege prüfen. Erst nach passender Bootloader-/Tabellenoffsetbindung höchstens zweimal den bekannten Partitionstabellenbereich bis 4 KiB lesen. Beide Wiederholungen müssen bytegleich sein; MD5, Slots, Grenzen und Nichtüberlappung validieren. Keine geratenen Adressen, weiteren Bereiche, NVS-/OTAdata-/Appvollsicherung, Baudwechsel, Stub oder Schreib-/Erasebefehle.

Belegte aktuelle Bootloader-Rollbackfunktion und alte gesunde VALID-App als erhaltenes Rückkehrziel sind Pflicht. `next_partition`, Kandidatenflags und RX3s Bootloader allein genügen nicht. Der automatische Rückweg gilt vor Appvalidierung; keine garantierte Qualitätsrückkehr nach VALID behaupten. Kein absichtlich fehlschlagender Boottest.

Genau einmal normal zur bisherigen Firmware zurückkehren, Port sauber schließen, alte Runtime-/VALID-/Configbindung mit mindestens zwei konsistenten Statusantworten mit Abstand bestätigen. Fehlender Rückweg oder unbestätigte Rückkehr → kein OTA und keine Bearbeitung des zweiten RX. Kein manueller Restore.

**B – C1 installieren:** Rückweg, aktuelle Identität, alte gesunde App, stabiler Boot, acht Configwerte, TX-Quelle und inaktiven ausreichend großen Appslot frisch prüfen. Bei erfüllten Gates exakt einen rohen App-OTA-POST, ohne Redirects, Proxies oder Retry.

Exakter Kandidat für beide RX:

```text
private/rx134-c-eignung-offline-20261014T110737Z/build-C1/rx/esp32-csi-node.bin
Größe: 1.238.784 Byte
BIN-SHA256: 01414441fbb9d4126d3c342e028b2693fd0b718816f34d2cf295d2c9c17bd5b6
Runtime-/Partitionsdigest: 9f5afbc5cdb040d4891f70368482a0aa1bd104f995c190588bdbbf20083f00fd
Diagnose-Source-SHA256: 1edd6fcb06bc846fea1c407926066d629849b210e62da44379158a6a1baf8b84
```

Größe und SHA direkt vor jedem Sendebeginn prüfen. Keine C-, Default- oder Peerfirmware statt C1. Antwortcode, Prozessende und gesendete Bytezahl getrennt sichern; HTTP 200 allein beweist keine Installation. Statusdigest ist der Partitionsdigest, nicht der BIN-Dateihash.

Nach jedem POST eine vorab begrenzte Statusfolge: höchstens drei Status-GETs in insgesamt 180 Sekunden. Mindestens zwei konsistente erfolgreiche Antworten mit Abstand müssen C1, neuen Boot, erwarteten laufenden Slot und VALID bestätigen. Danach acht Configfelder typgenau gegen die vorherige Referenz und Diagnose gegen C1-/Source-/Bootbindung prüfen. Historischer Rollenstring `RX2-v2` ist kein physischer Nodebeweis.

Bei zweifacher Nichterreichbarkeit darf ausschließlich der für genau diesen RX nachgewiesene automatische Rückkehrpfad mit dem oben freigegebenen einen zusätzlichen normalen B-Neustart bedient werden; keine weitere ROMphase, kein manueller Restore, kein zweiter POST. Die Rückkehraktion schafft kein neues Status-/Zeitbudget. Ist die alte gesunde App zurück, dies bestätigen und Gesamtauftrag stoppen. Bei unbekanntem Image, geänderter Config, unbestätigtem Boot oder Ablauf der Frist stoppen; keine Installation des anderen RX.

Nach RX1-Erfolg Port schließen und bestätigten Kabelwechsel zu RX4 abwarten. Vor RX4 Eingriff Kontrollzustände unverändert halten. Nach RX4-Erfolg sind beide Geräte auf C1; keine zusätzliche 120-s-Pilotaufnahme. Ihre CSI-Qualität wird im folgenden gemeinsamen Lauf gemessen.

## 4. Genau ein vollständiger 30-Minuten-Vier-RX-Lauf

Nur nach bestätigter Installation und gesunder Config-/Boot-/Sourcebindung beider Zielgeräte. Alle vier RX direkt vor Capture mit Status, Config und passender C-/C1-Diagnose neu binden. Exakt vier eindeutige physische Quellen, gemeinsame TX-Quelle und passende Zieltupel im Capturefilter; keine Teilmenge, Mehrheits- oder Erstpaketwahl. Private Referenzkopien gezielt aktualisieren: RX1/RX3/RX4 C1, RX2 C, frische Boots und Endpunkte. Configreferenzen nicht ersetzen.

Capture am bestehenden Host/Interface: **2 s Vorlauf, 1.800 s festes Messfenster, 2 s Nachlauf, höchstens 10 s Cleanup**. Mindestens 2 GiB freier Platz; PCAPlimit 128 MiB, kein zweiter Capture oder Rotation. Ausreichend Zeit für Endprüfung reservieren. Nur bestehenden Berechtigungszustand nutzen; kein sudo-/Rechte-/Dienstumbau.

Listening-/Headerbereitschaft, monotone Prozessanker, originale PCAP-Zeitstempel und 50-ms-Uhrqualitätsschwelle erhalten. Prozess und Logs dürfen dauerhaft mitschreiben; keine neue geplante Rückkehr/Automation. Vor-/Nachlauf geben keine Messgutschrift. Kein Monitor-/Promiscuousmodus, zweiter UDP-Empfänger, Hostpatchrollout oder Neustart des Sensing-Servers.

Während des gesamten Captureprozesses bis Cleanupende: keine Geräte-/API-GETs, UARTaktionen, Resets, Builds oder anderen Eingriffe. Nur eigene Captureprozesse beenden. Danach alle vier RX mit Status, Config und Diagnose prüfen: gleiche Messboots, erwartetes C/C1, VALID und acht unveränderte Werte. Fehlender Endbeleg bleibt unbestätigt; kein neues Capture zur Heilung.

Den Original-PCAP offline mit beiden bestehenden Regeln auswerten. Auswahlvertrag unverändert: passendes gebundenes 128er-Basisraster, 3-s-Fenster, 1-s-Schritt, 5 s Einschwingen, mindestens 15 passende Frames je RX, mindestens 2,5 s Abdeckung und gemeinsame Abdeckung, mindestens 5 Hz, größte innere Lücke kleiner als 1 s, keine verborgene Boot-/Quellen-/Epochengrenze. Unbekannte physische LTF-/Maskenangaben nicht erfinden.

CSI-Qualität bestanden nur bei vollständiger/zeitlich gültiger/gebundener Aufnahme und mindestens **1.614/1.793 gültigen gemeinsamen Fenstern (90 %)**. Zusätzlich pro RX mindestens fünf passende Frames/s und mindestens 90 % eigene gültige Fenster berichten/prüfen. Alle Fenster samt Fehlergründen behalten; Nenner bei Ausfall nicht verkleinern. Alte strenge Auswertung getrennt ausweisen; keine früheren FAILs ändern.

Je RX gültige eigene CSI, Rasterkandidaten, Formatausschlüsse, Duplikate, verspätete Pakete, Boot-/Quellenepochen, ungeklärte Sequenzpositionen **mit eigenem Nenner**, Rate, Datenlücken, Fensterzahlen und Endzustand berichten. Format- und Sequenzbilanz nach Möglichkeit unabhängig aus gespeicherten Metadaten nachrechnen. PCAPabschluss, Kernel-/Interface-Drops beziehungsweise unbekannte Angaben und beobachteten Nachlauf dokumentieren. Keine Funkverlustursache oder Socket-/Serverannahme aus PCAP allein behaupten.

Bei CSI-Qualitäts-FAIL trotz gesunder C1-/VALID-Geräte gemessenen Stand erhalten: kein Rückupload, Restore, Parameterwechsel oder zweiter Versuch. Diagnose anhand gespeicherter Daten liefern.

## 5. Budgets, Abschlusskriterien und Lieferung

HTTP nur auf RX1–RX4: `/ota/status`, authentisiertes `/config`, `/config?diag=1` ausschließlich bei bekanntem C/C1; POST `/ota` ausschließlich RX1/RX4. Tokens erst nach frischer MAC-/Routen-/Statusbindung verwenden. GET connect höchstens 3 s, insgesamt 7 s, Antwort 16 KiB; POST connect 3 s, insgesamt 120 s, Antwort 16 KiB. Keine Redirects, Proxies, Cookies oder automatischen Retries. Keine TX1-/Radar-/Host-APIabfrage.

Planbare GET-Bilanz maximal 52: Start Status/Config für vier RX plus zwei Kontrollstatus und zwei Kontrolldiagnosen = 12; normale A-Rückkehr je zwei Status + Config = 6; B-Abnahme je maximal drei Status + Config + Diagnose = 10; unmittelbar vor und nach Capture je Status/Config/Diagnose für alle vier = 24. Bis zur harten Grenze 64 ausschließlich vorab benannte Kontroll-/Abbruch-Endbelege reservieren; keine Retryreserve. Endbedarf und Deadline vor Aktionen prüfen, Verbrauch vor Anfrage zählen.

Bei Abbruch nach einem Eingriff dessen normale Rückkehr/Cleanup und erlaubte Endzustandsprüfung priorisieren. Keine spontanen Wiederherstellungsaktionen. Verbrauchsmarker pro Auftrag und Zielphase exklusiv anlegen; keine doppelte Ausführung. Technische Gatefehler sind keine Einladung, Schutz- oder Authprüfungen zu lockern.

Abschlussbericht und Delivery liefern mit getrennten Urteilen:

```text
RX1_RUECKWEG / RX4_RUECKWEG: BELEGT / NICHT_BELEGT
RX1_INSTALLATION / RX4_INSTALLATION:
  NICHT_GESTARTET / C1_BESTAETIGT / ALTE_APP_BESTAETIGT / UNKLAR
RX1_BETRIEB / RX4_BETRIEB: BOOT_CONFIG_VALID_BESTAETIGT / NICHT_BESTAETIGT
RX2_KONTROLLE / RX3_KONTROLLE: UNVERAENDERT_BESTAETIGT / NICHT_BESTAETIGT
VIER_RX_CAPTURE: VOLLSTAENDIG / UNVOLLSTAENDIG / NICHT_GESTARTET
G1_CSI_QUALITAET: PASS / FAIL / NICHT_BEURTEILBAR
G1_RESTNACHWEISE: konkrete noch offene Belege des Gesamtplans
PEER: NICHT_AKTIVIERT; TRACKING: NICHT_ABGENOMMEN
```

G1_CSI-PASS bestätigt ausschließlich den vorab festgelegten Vier-RX-Transport-/Fenstervertrag. Physische Maske, vollständige Fehler-/Ressourcenbilanz, Serverannahme, Geometrie, Radarreferenz und Positionsleistung sind damit nicht automatisch abgeschlossen. Gesamt-G1 nur schließen, wenn seine übrigen verlangten Belege ebenfalls vorliegen; sonst ausdrücklich offen führen, ohne den erfolgreichen Teilbefund zu verwerfen.

Lieferung: neue private Helfer und Tests, genaue Befehle/Receipts, Ausführungssiegel, ROM-/Status-/Config-/Diagbindungen, Captureproof, Original-PCAP, beide Auswertungen, unveränderte Produktdateien/alte Belege und Abschlussmanifest. A/B-Quelle, tatsächlich verbrauchte Zeit, GETs/POSTs/Syncs/ROMreads/Resets/Captures sowie bekannte Grenzen angeben. Keine Zugangsdaten oder Rohpayloads im öffentlichen Bericht. Genau einen nächsten konkreten Schritt nennen und stoppen; kein weiterer Rollout, Peerexperiment oder Folgeagent.

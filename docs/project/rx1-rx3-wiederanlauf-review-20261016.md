<!-- Publication copy: local paths and hardware identifiers were pseudonymized; private original retained. -->
# RX1/RX3-Wiederanlauf und Vier-RX-Gerätepreflight vom 16. Oktober 2026

**Beide Wiederanläufe wurden durch Johann jeweils einmal manuell per RST ausgeführt und bestätigt. Danach bestehen die AP-Anmeldung, aktuelle MAC-/IP-Bindung und der Gerätepreflight aller vier RX. Kein OTA, Hoststart oder Netzumbau. Der Messlauf wurde nicht begonnen.**

Die zuvor [fehlende Anmeldung von RX1/RX3](rx1-rx3-ap-clientreview-20261016.md) ist im neuen Prüfzeitpunkt behoben. Die Ursache des vorherigen Zustands bleibt offen; der erfolgreiche Wiederanlauf beweist keine dauerhafte Fehlerbehebung oder Funkqualität.

## Freigabe, physischer Zugriff und tatsächliche Wiederanläufe

Die unmittelbare menschliche Freigabe erlaubte je einen gezielten Wiederanlauf von RX1 und RX3 samt anschließender AP-/IP-/Vier-RX-Prüfung und untersagte OTA, Hoststarts und Netzänderungen. Arbeitsbaum und Root-Anweisungen wurden zuerst gelesen. Vor der neuen privaten Ablage wurde die wirksame Git-Ausnahme und Nichtversionierung geprüft.

Die lokale passive USB-Prüfung fand keinen verfügbaren USB-Seriell-Geräteknoten für einen RX und keine passende RX1-Bridge. Kein Port wurde geöffnet. Ein Versuch, die vorhandene SDK-Umgebung nur zur Portinventur zu verwenden, scheiterte an fehlendem `serial`; der Fehler ist erhalten. Anschließend erfolgte die Inventur mit nativem IOKit, ohne Installation.

Die gebundene C1-Quelle bietet keinen eigenständigen Remote-Reset. `POST /config?reboot=1` ohne Configschlüssel wird abgewiesen; ein Neustart in diesem Handler setzt einen vorherigen NVS-Configwrite voraus. Dieser Weg wurde nicht aufgerufen. Keine Scans, geratenen Endpoints, Bootloader-, ROM-, Stub-, Resetleitungs- oder Flashzugriffe.

Wegen des fehlenden physischen Zugriffs wurde keine erneute Unterbrechungsfreigabe angefordert, sondern die Ausführung des bereits freigegebenen RSTs durch Johann. Seine ausdrückliche Antwort im selben Chat lautete: **„Beide jeweils einmal per RST neu gestartet“**. Verbrauch: **ein bestätigter manueller RST je RX1/RX3**, null durch den Agenten erzeugte Resetimpulse, null serielle Portöffnungen. Eine UART-Aufzeichnung zur unabhängigen Zählung aller tatsächlichen Firmwareboots liegt nicht vor. Die danach gelesenen aktuellen Boot-IDs sind an ihre Diagnosen gebunden; ein frischer unmittelbar vor RST gelesener Bootvergleich wird nicht behauptet.

## Frischer AP-/IP-Abgleich und Gerätepreflight

Die gemeinsame AP-/Geräteprüfung begann am **16. Oktober 2026 um 19:12:31.838 Uhr CEST** und endete um **19:12:34.086 Uhr**. Gesamtdauer nach demselben monotonen AP-Zeitanker: rund **2,247 s**, innerhalb der vorab begrenzten 180 s für AP und Vier-RX-Preflight.

Vor Verwendung der gebundenen AP-Zugangsdaten wurden Interface, direkte Route, bekannte physische AP-MAC und erwartete TX-Image-/Quell-/Node-/MAC-Hashidentität geprüft. Ein unauthentisierter Identitäts-GET und ein authentisierter Stationslisten-GET gehören zum selben AP-Boot. Die geschützte Liste meldet einen aktuellen Snapshot und **genau fünf Stationen: Mac und RX1–RX4**. Aktuelle IPs wurden aus der eindeutigen passiven Nachbarcachezuordnung zu den bekannten physischen RX-MACs gewonnen, nicht aus historischen IP-Hinweisen.

Vor den RX-Zugangsdaten wurden alle vier AP-Assoziationen und eindeutigen Endpoints gebunden. Je RX wurden die direkte Route und der passive Cache erneut geprüft; nach den drei RX-GETs wurde die MAC-/Endpointzuordnung passiv nachgeklammert. Pro RX genau einmal: unauthentisierter `GET /ota/status`, authentisierter `GET /config`, authentisierter `GET /config?diag=1`. Keine Wiederholung, Weiterleitung oder Proxyverwendung.

| RX | Aktuelle AP-/MAC-/IP-Bindung | Bekanntes Image | Slot / Zustand | Acht Configwerte | Diagnose |
|---|---|---|---|---|---|
| RX1 | **PASS** | **C1** | **ota_0 / VALID** | **8/8 typgleich erhalten** | **Boot/Image/Quelle vollständig gebunden** |
| RX2 | **PASS** | **C** | **ota_0 / VALID** | **8/8 typgleich erhalten** | **Boot/Image/Quelle vollständig gebunden** |
| RX3 | **PASS** | **C1** | **ota_0 / VALID** | **8/8 typgleich erhalten** | **Boot/Image/Quelle vollständig gebunden** |
| RX4 | **PASS** | **C1** | **ota_0 / VALID** | **8/8 typgleich erhalten** | **Boot/Image/Quelle vollständig gebunden** |

Geprüft wurden Node, korrekt geformter aktueller Boot, erwarteter Runtime-Digest, laufender und nächster Slot, VALID, 2-MiB-Appslot und gemeldete IP. Die acht Configwerte wurden gegen die erhaltenen historischen Endreferenzen typgenau verglichen: `target_ip`, `target_port`, `node_id`, `csi_channel`, `edge_tier`, `tdm_slot`, `tdm_total`, `filter_mac_configured`.

Die Diagnoseprüfung umfasst Schema/Rolle, `complete=true`, zum Status passendes Image und Boot, erwarteten Quellhash, geordnete Gerätezeitfelder, fünf Formatklassen, vollständige 14er-Stufen-/8er-Featurezähler und gültige Grid-Untermengen. `RX2-v2` als historischer Rollenstring ist keine physische RX-Zuordnung. Die Einzeloriginale und zugehörigen Receipts wurden nach Hash und Querverbindungen nochmals offline geprüft.

**Urteil: PASS_ALL_FOUR_DEVICE_PREFLIGHT.** Das ist die aktuelle Geräte-/AP-/Config-/Diagnosebindung. Es ist keine vollständige Mess-, CSI-Qualitäts-, Hostprozess-, Serverempfangs-, Setup-/Geometrie-, Kalibrierungs-, Ressourcen- oder Positionsabnahme. Die unmittelbaren frischen Host-/Speicher-/Laufgates müssen vor dem späteren Messstart erneut gelten.

## Tatsächlicher Verbrauch

| Aktion | Verbrauch |
|---|---:|
| Bestätigte manuelle RSTs | RX1 **1**, RX3 **1** |
| Agenten-Resetimpulse / serielle Portöffnungen | **0 / 0** |
| AP-GETs | **2** |
| RX-GETs | **12**, drei pro RX |
| Passive lokale AP-/Preflightabfragen | **17** |
| Zusätzliche passive USB-Inventur | ein fehlgeschlagener SDK-Modulaufruf und ein erfolgreicher nativer IOKit-Aufruf, ohne Portöffnung |
| POSTs / OTA / Configwrites | **0 / 0 / 0** |
| Hoststarts/-wechsel, Captures, Recordersteuerungen | **0** |
| Netzwerk-/Securityänderungen, ROM-/Stub-/Flashaktionen | **0** |

TX1, RX2, RX4 und mmWave wurden nicht unterbrochen oder umkonfiguriert. RX1/RX3 behalten ihr bekanntes Image und sämtliche exportierten Configwerte. Keine neuen Firmware-/Hostbuilds oder Toolinstallationen. Kein anderer Agent, Chat oder Termin. Vorhandene Änderungen und Backups bleiben erhalten; neue private Verzeichnisse 0700, Dateien 0600.

## Nächster Schritt für den Messlauf

**Der konkrete RX1-/RX3-Anmeldeblocker ist im neuen Prüfzeitpunkt geschlossen.** Als nächster Auftrag kann der vollständige [Basis-/Installations-/Paarlauf](agent-auftrag-rx1-rx4-hardwarelauf-20261016.md) als neuer begrenzter Lauf gestartet werden: frischer Vier-RX-/Host-/Speicherpreflight, 480 s Basis mit bisheriger Firmware, RX1 und RX4 nacheinander genau einmal App-OTA mit VALID-Prüfung, 960 s Paarvergleich, bestätigter Default-/Peer-Endzustand und einmalige geplante Offlineauswertung.

Dieser neue Start muss zu den dafür erteilten A/B/C-Freigaben passen und verwendet eigene explizite Laufbindungen; der alte gestoppte Hardwarelauf wird nicht mit erneuerter Uhr fortgesetzt. Den jetzigen kurzen Gerätepreflight nicht als bereits verbrauchten Teil einer laufenden Basisaufnahme oder als dauerhaft gültige Bindung darstellen. Kein weiterer Offline-Neubau ist aus diesen erfolgreichen Gerätebefunden erforderlich.

Der vorliegende Auftrag endet wie verlangt mit diesem Bericht. Keine Messung oder Installation wurde automatisch begonnen.

## Private Originale

`./private/rx1-rx3-wiederanlauf-20261016T170847Z/`

- `DIRECT-AUTHORIZATION.private.json`, `HUMAN-RESET-CONFIRMATION.private.json`: Freigabe und tatsächliche menschliche RST-Bestätigung.
- `USB-INVENTORY.*`, `USB-NATIVE.*`, `USB-ACCESS-EVALUATION.private.json`: erhaltene fehlgeschlagene und erfolgreiche passive Zugriffsinventur.
- `INPUT.private.json`, `READ-SOURCES.private.json`, `read_ap_once.py`, `four_rx_preflight.py`: Quellen-, Eingangs- und Ablaufbindungen; ursprünglicher AP-Leser unverändert in neuer Ablage.
- `READ.consumed`, `PREFLIGHT.consumed`, `command-*`, `preflight-command-*`: gemeinsame Zeitanker, Limits, Dispatch-/Original-/Hashbelege.
- `AP-GET-1/2.body.private.json`, `RX1/2/3/4-*.original.private.json`, `RX1/2/3/4.preflight.private.json`: tatsächliche Antworten und Einzelurteile.
- `RESULT.private.json`, `FOUR-RX-RESULT.private.json`, `FINAL-VALIDATION.private.json`: AP-/IP-Bindungen, Gesamturteil, Offlinequerprüfung und tatsächlicher Verbrauch.
- `FINAL-MANIFEST.private.json`: Größen, Rechte, Hashes der geschlossenen eigenen Belege sowie Berichtshash. Gerätewerte, Tokens und originale Antworten bleiben privat.

Die bytegenauen HTTP-Originale sind die jeweiligen `command-*.stdout.private` beziehungsweise `preflight-command-*.stdout.private`. Die zusätzlich benannten `*.body.private.json` und `RX*-*.original.private.json` sind kanonisch gespeicherte Parseansichten derselben Antworten; ihre Bytes werden nicht mit den ursprünglichen HTTP-Bodybytes gleichgesetzt.

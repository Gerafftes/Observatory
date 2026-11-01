<!-- Publication copy: local paths and hardware identifiers were pseudonymized; private original retained. -->
# RX1/RX3: lesender AP-/DHCP-Abgleich am 16. Oktober 2026

**RX1 und RX3 sind in der frisch gelesenen geschützten Stationsliste des Projekt-APs nicht angemeldet. Aktuelle IPs konnten ihren bekannten MACs nicht zugeordnet werden. Am AP sind genau Mac, RX4 und RX2 angemeldet. Der nächste notwendige Schritt für die Messung ist die Wiederanmeldung von RX1/RX3, anschließend ein vollständiger frischer Vier-RX-Preflight.**

Die unmittelbare Freigabe beschränkte diesen Auftrag auf das Lesen der vorhandenen AP-/DHCP-Clientinformationen und den Abgleich mit den bekannten RX1-/RX3-MACs. Keine RX-Abfrage, kein Geräteschreibzugriff, Hoststart oder Netzumbau. Der [gestoppte Hardwarelauf](rx1-rx4-hardwarelauf-review-20261016.md) und die [zwei vorherigen Statusversuche](rx1-rx3-bindungsreview-20261016.md) wurden nicht wiederholt oder fortgesetzt.

## Tatsächlich gelesene Quellen

Das Projekt-WLAN wird vom TX1-SoftAP bereitgestellt. Dessen vorhandener HTTP-Leseendpunkt `/status` liefert bei gültiger Authentisierung `ap_association.stations`. Die vorhandene Implementierung und eine gespeicherte tatsächliche Antwort zeigen pro Station ausschließlich eine MAC-Adresse. Ein auslesbarer IP-/DHCP-Leasebestand ist über diesen vorhandenen Zugang nicht enthalten. Es wurden keine neuen Endpunkte ausprobiert, keine fremden Router abgefragt und keine Firmwareänderung vorbereitet.

Der bekannte AP-Zugang und dessen Authentisierungsdatei wurden aus bestehenden gebundenen privaten Belegen ausgewählt. Vor Zugangsdatengebrauch wurden Interface, bekannte AP-MAC, direkte Route und die erwartete AP-Image-/Quell-/Node-/MAC-Hashidentität geprüft. Zuerst erfolgte ein unauthentisierter Identitäts-GET, anschließend genau ein authentisierter GET für die geschützte Liste. Keine Zugangsdaten in argv, Toolausgabe oder allgemeinem Bericht. Beide Antworten gehören zum selben Boot.

Zusätzlich wurden rein passiv das Interface, der Nachbarcache vor/nachher, die direkte AP-Route und der bestehende eigene DHCP-ACK des Macs gelesen. Der eigene DHCP-ACK ist **keine Client-Leasetabelle des APs**. Die gespeicherten Originalantworten bleiben privat.

## Aktueller Befund

Abfragezeit: **18:56:41.905 bis 18:56:42.001 Uhr CEST**, 16. Oktober 2026; monotone Gesamtdauer rund **0,095 s**. Die AP-Liste meldet `status=current`, erfolgreiche Getter, unveränderte Assoziationsgeneration während ihres Snapshots und genau drei eindeutige Stationen. Der Offlineabgleich mit den bekannten RX-MACs und der frisch gelesenen Mac-Interface-MAC ergibt **Mac, RX4 und RX2**, keine unbekannte Station.

| Gerät | In aktueller geschützter AP-Liste | Bekannte MAC im passiven Nachbarcache | Aktuelle IP-Zuordnung |
|---|---|---|---|
| RX1 | **Nein** | **Nein** | **NICHT_BELEGT** |
| RX3 | **Nein** | **Nein** | **NICHT_BELEGT** |

Dies ist ein konkreter Beleg ihrer fehlenden Anmeldung an diesem Projekt-AP zum Abfragezeitpunkt. Stromversorgung, Bootzustand, WLAN-Authentisierungsfehler, ein anderer AP oder Firmwarefehler wurden nicht geprüft und sind keine festgestellten Ursachen. Es gibt keine neue RX-Image-/Boot-/Slot-/VALID-/Configaussage. Auch die AP-Anmeldung von RX2/RX4 ersetzt deren Geräte- und Empfangsprüfung nicht.

Die Empfehlung im vorherigen Bericht, eine vorhandene AP-/DHCP-Clientliste zu prüfen, war als möglicher unabhängiger Zuordnungsweg formuliert. Nun ist deren tatsächliche Grenze geklärt: Der vorhandene AP-Zugang liefert eine aktuelle MAC-Stationsliste; die beiden gesuchten Stationen fehlen, und eine IP-Leasetabelle wird nicht ausgegeben.

## Verbrauch und Erhaltung

| Aktion | Tatsächlich |
|---|---:|
| AP-GETs | **2**: einmal Identität, einmal geschützte Liste |
| RX-GETs, einschließlich RX1/RX3 | **0** |
| Passive lokale Abfragen | **5** |
| POSTs / Geräteschreibzugriffe | **0** |
| Hoststarts/-wechsel, Captures, Recordersteuerungen | **0** |
| Netz-/Sicherheitsänderungen, Reset-/serielle Zugriffe | **0** |

Arbeitsbaum und wirksame Root-Anweisungen wurden gelesen. Die private Ablage ist wirksam ignoriert und unversioniert; neue Verzeichnisse 0700, Dateien 0600. Vorhandene Änderungen, Originale und Backups wurden erhalten. Kein weiterer Agent, Chat, geplanter Termin oder Dienststart.

## Nächster Schritt, damit gemessen werden kann

1. **RX1 und RX3 an ihren festen Messpositionen wieder am Projekt-AP anmelden.** Zuerst ihre Versorgung und sichtbaren Betriebszustand prüfen. Falls sie versorgt sind, ist ein gezielter, separat ausdrücklich freigegebener Wiederanlauf ausschließlich dieser beiden RX der kleinste praktische nächste Auftrag. Ein Reset kann den Betrieb unterbrechen und ist durch diesen lesenden Auftrag nicht erlaubt. TX1, RX2, RX4, mmWave und Host-Netz dabei erhalten. Keine OTA oder Konfigurationsänderung aus diesem Befund ableiten.
2. **Danach die neue Anmeldung und Adressbindung bestätigen.** Geschützte AP-Liste und passiver Cache müssen die bekannten MACs wieder eindeutig zeigen. Erst mit belegtem aktuellem Endpoint sind frische RX-Identitäts-/Boot-/Slot-/VALID-/Config-/Diagnoseprüfungen im dafür freigegebenen Vier-RX-Preflight sinnvoll. Fehlt die Zuordnung weiterhin, ist ein begrenzter WLAN-/Bootfehlernachweis erforderlich; keinen geratenen IP-Sweep oder Upload beginnen.
3. **Nach bestandenem Vier-RX-Preflight den vollständigen Messlauf neu starten:** 480 s Basis mit bisheriger Firmware, danach RX1 und RX4 nacheinander einmal installieren und VALID bestätigen, anschließend 960 s Paarvergleich und bestätigter Default-/Peer-Endzustand. Frischer einmaliger Lauf mit dem dafür ausdrücklich erteilten Umfang und originalen Zeit-/Aktionsgrenzen; den gestoppten Lauf nicht mit erneuerter Uhr fortsetzen.

Dieser Auftrag endet mit dem Bericht. Es wurden weder Wiederanlauf noch Messung gestartet. Ein Firmware-Neubau ist durch den aktuellen Befund nicht begründet.

## Private Belege

`./private/rx1-rx3-ap-dhcp-20261016T165551Z/`

- `INPUT.private.json`, `SOURCE.private.json`, `read_ap_once.py`: direkte Freigabe, historische Zugang-/Identitätsbindungen und ausgeführte Quelle.
- `READ.consumed`, `command-*.before.private.json`, `command-*.receipt.private.json`: einmaliger Start, Zählung vor Dispatch, Prozess-/Zeit-/Hashbelege.
- `command-*.stdout.private`, `command-*.stderr.private`, `AP-GET-1/2.body.private.json`: originale Interface-/Cache-/Route-/DHCP-/AP-Antworten.
- `RESULT.private.json`, `EVALUATED.private.json`: tatsächliche API-/Cachebefunde und Offlinezuordnung der drei vorhandenen Stationen.
- `FINAL-MANIFEST.private.json`: Größen, Rechte und SHA-256 der geschlossenen eigenen Dateien sowie Berichtshash.

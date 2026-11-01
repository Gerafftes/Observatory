<!-- Publication copy: local paths and hardware identifiers were pseudonymized; private original retained. -->
# Koordinatorreview: enge Cache-Löschliste

**Phase A bestanden. Die konkrete Liste ist fachlich für eine gesonderte Phase-B-Freigabe geeignet. Noch nichts gelöscht oder verschoben; Phase B bleibt ohne Johanns ausdrückliche Freigabe gesperrt.**

Stand: 16. Oktober 2026, etwa 11:05 Uhr Europe/Berlin. Geprüft wurden der Abschluss des bestehenden Chats und die [private Lieferung](../../private/cachebereinigung-vorbereitung-20261016-20261016T083636Z/REPORT.private.md), insbesondere Schutzkatalog, vollständige Inhaltssnapshots, Prüfreceipts und Siegel. Keine Agenten gestartet/angeschrieben, keine Builds, Hardware-, Host- oder Netzaktivierung.

## Ergebnis der unabhängigen Nachprüfung

- Integritätsprüfung der vollständigen neuen Lieferung: **70 Dateien bestanden**. Die [gesperrte genaue Liste](../../private/cachebereinigung-vorbereitung-20261016-20261016T083636Z/DELETE-ALLOWLIST.private.json) hat SHA-256 `6d1f0f0bf3d2ce32a095753349fe986a369d168f21f0e1d793a2e8ad9c06cd46`.
- **68.631 geschützte reguläre Dateien** frisch nach SHA-256 und Geräte-/Inode-/Größen-/Zeit-/Rechte-/Linkmetadaten geprüft; keine Abweichung. Dabei 43.304 eindeutige Inodes mit 32.478.099.491 Byte tatsächlich gehasht; bei mehreren Namen desselben Inodes dessen Hash wiederverwendet. Sieben separat gebundene Quelllinks unverändert geprüft.
- **Beide vorgeschlagenen Teilbäume vollständig erneut geprüft:** 10.994 beziehungsweise 13.328 Dateien, insgesamt **24.322**. Exakter Eintragssatz, Identitäten, Metadaten und Datei-SHA-256 stimmen mit den Snapshots überein. Keine Symlink-/Mountabweichung, kein versionierter Pfad und keine Überschneidung mit Schutzpfaden oder geschützten Inodes.
- Die gespeicherten, versiegelten tatsächlichen Prüfungen des Agenten wurden geprüft: v3.1 samt v3-Basis unter OS-Leseverbot aller fünf `incremental`-Teilbäume bestanden; kleine Datei-Auswertung mit bytegleichem Bericht und zwölf Fenstertabellen; vier positive C→Rust→Wire-Fälle bestanden. Policy, Prüfeinstieg und Receipts passen zusammen. Diese Ausführungen wurden im Koordinatorreview nicht als erneut ausgeführt ausgegeben. Normale Hosts wurden nicht gestartet; deren Bindungen sind statisch geprüft.

Fertige Host-/Datei-/Testprogramme, Firmware-BIN/ELF/Config, Quellen, Messdaten, Backups und maßgebliche Belege bleiben außerhalb der Löschliste. Alte Testprogramme unter `debug/deps` bleiben am Ort erhalten; fehlende historische Ausführungshashes bleiben ausdrücklich unbekannt. Die geerbten historischen Referenzwarnungen sind klassifiziert und keine aktuelle maßgebliche Manifestabweichung. Zusätzliche Programmkopien oder Neubauten sind für diese Auswahl nicht erforderlich.

## Genau begrenzter späterer Umfang

| Reihenfolge | Teilbaum unter `./private/` | Zurechenbare Allokationsobergrenze |
|---|---|---:|
| 1 | `peer-c1-review-korrektur-offline-20261015T050734Z/host-target/debug/incremental` | 5.252.829.184 Byte |
| 2 | `offline-tagespaket-20261015T061442Z/host-target/debug/incremental` | 7.072.215.040 Byte |

Zusammen höchstens **11,478592 GiB** nach Abzug extern verbleibender Hardlinks; APFS-Blockteilung/Snapshots können den tatsächlichen Gewinn bis auf null verringern. Keine Zusage vollständiger Rückgewinnung. Die übrigen drei Caches und alle `debug/deps` bleiben ausgeschlossen. Keine pauschale Bereinigung der ursprünglich gemeldeten 118,4 GB.

Frische Freiplatzbeobachtung um **11:04:47 Uhr**: **64.815.067.136 Byte = 60,363735 GiB**. Unveränderter Startbedarf **70.922.518.528 Byte = 66,051743 GiB**; Lücke **6.107.451.392 Byte = 5,688007 GiB**. Das vorgeschlagene Stopziel der gehashten Liste ist **70 GiB frei** als Puffer, kein neuer Softwareguard. Freiplatz bleibt eine Momentaufnahme.

## Entscheidung und nächster Schritt

**Phase B ist freigabefähig, aktuell nicht freigegeben.** Nach direkter Freigabe genau dieser Liste und ihres 70-GiB-Stopziels frische Schutz-/Snapshotbindungen und Platz prüfen, parallele Builds zuverlässig ausschließen und dann nur innerhalb der zwei unveränderten Einträge fortfahren. Die zwei gespeicherten leeren Aktivitätsaufnahmen beweisen keine dauerhafte Ruhe. Ist diese nicht sicherzustellen oder hat sich ein Snapshot verändert, vor dem betroffenen Eingriff stoppen; keine Prozesse beenden und keine Locks entfernen.

Nach jedem Eintrag reale freie Bytes prüfen und bei erreichtem Ziel den Rest überspringen. Bei erschöpfter Liste keinen Umfang erweitern. Abschließend Schutzartefakte und dieselbe kleine Datei-/v3.1-Prüfung kontrollieren; nach deren Outputs verbleibende Deckung ausweisen. Kein erneuter voller Lastlauf oder großer Neubau allein für die Bereinigung nötig.

Danach folgt der gebündelte RX-Messauftrag: eigene geprüfte ausführbare Livekopie/-hostprogramme, frische Geräte-/Host-/Geometriebindungen und passende separate Freigaben; 480 s Basis und 960 s RX1↔RX4-Vergleich innerhalb des bestehenden Gesamtbudgets. Die gesperrten Offlineoriginale nicht aktivieren. Hardware: TX1 und alle vier RX an festen Positionen mit Strom, Mac im bestehenden Projekt-WLAN und am Netzteil; kein COM-/USB-Datenkabel und kein Radar für diesen Funkvergleich. Der Review ist keine Gerätefreigabe und kein neuer Onlinezustandsnachweis. Die drei offenen vollständigen Livekriterien des 14. Oktober bleiben offen, weiterhin **2/5**.

**Schwierigkeit der begrenzten Cachebereinigung: 2/5.** Kein neuer Codeblocker aus diesem Abschluss; der folgende Hardwaregesamtlauf bleibt 4/5.

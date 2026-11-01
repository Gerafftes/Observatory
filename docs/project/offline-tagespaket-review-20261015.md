# Koordinatorreview des Offline-Tagespakets vom 15. Oktober

**Dual-Radar-Prüfung und TX-only-Metadatenkandidat sind offline bestätigt. Der RX1↔RX4-Realadapter ist implementiert, benötigt aber eine gezielte Recorder-Vertragskorrektur vor einem Hardwarelauf.** Der Bericht wird deshalb nur teilweise bestätigt. Keine Geräteabfrage, Installation, Hostaktivierung oder Agentenbeauftragung durch diesen Review.

Maßgeblich: `private/offline-tagespaket-20261015T061442Z/`, insbesondere Bericht, Delivery, Quell-/Kandidaten-/Abschlussmanifest und tatsächliche Adapter-/Hosteingänge. Ältere B1–B4 bleiben geschlossen; die neue Abweichung liegt im Adapter, nicht in der Firmware. Privater Gegenbeleg: `private/offline-tagespaket-koordinatorreview-20261015/`.

## Getrennte Ergebnisse

| Paket | Koordinatorurteil | Konsequenz |
|---|---|---|
| P1 Realadapter | **NICHT_READY** | Implementierung und 26 bestehende Tests bestätigt, aber ein realer Metadatenvertrag ist falsch abgebildet. Noch kein Paarhardwarelauf. |
| P2 Dual-Radar-Vertrag | **READY_OFFLINE** | 74 netzfreie Kalibrier-/G2-Tests pro Hostvariante wiederholt bestanden. Reale Sensorfunktion, Genauigkeit und gemeinsame Referenz bleiben offen. |
| P2 G2-Hardwareplan | **ZUR KONKRETEN FREIGABEPRÜFUNG GEEIGNET** | Getrennte Einzel- und gemeinsame Prüfung, feste Punkte/Nenner, begrenzter Hostwechsel und Ende. Noch keine Livefreigabe oder laufende Hardware. |
| P3 TX-only-Metadaten | **KANDIDAT_GEBAUT_UND_GEPRUEFT** | Gemeinsames S3-Image für RX2/RX3 mit NVS-Identität, Layoutattestation an und Peer aus. Keine Installation; spätere eigene Rückweg-/Boot-/Config-/Liveprüfung nötig. |
| Reale Bilanz und Tracking | **WEITER OFFEN** | Keine neue RF-, Ressourcen-, Vier-RX-Physical-, Radar- oder Positionsabnahme. |

## Bestätigte Adapterabweichung

`scripts/real_adapter.py`, Capture-Endprüfung: Der Adapter verlangt einen positiven Wert aus `frame_count` oder `frames`. Der tatsächliche Host schreibt beim Abschluss seiner `.raw-csi.v1.meta.json` jedoch **`frames_written`** (`main.rs`, `finalize_recording_metadata_in_dir`). Die Testattrappe erfindet hier ein `frames`-Feld und verdeckt die Abweichung.

Der erhaltene Gegenbeleg führt denselben `run_session → Session.execute → RealAdapter`-Einstieg mit vollständig gesperrter echter I/O aus. Einziger Unterschied zur guten Attrappe: Das Recorder-Endmetadatum verwendet das tatsächliche `frames_written`. Ergebnis: **`recorder durable end proof`**, Abbruch nach der Basisaufnahme, **null OTA-POSTs**, begrenzter Basis-/Rückkehrpfad mit zwei Hostwechseln. Ein funktionierender realer Recorder würde damit vor dem ersten Upload als fehlerhaft behandelt. Das ist kein beobachteter Gerätefehler.

Kleinster Folgeauftrag: In einer neuen privaten Kopie das kanonische Hostfeld `frames_written` typgenau und positiv prüfen, mit Stopantwort und dauerhaftem Endbeleg abgleichen; Fixtures aus dem tatsächlichen Hostvertrag ableiten. Guter Abschluss, null/fehlende/typfalsche/widersprüchliche Zähler und unvollständige Enden durch denselben Einstieg regressieren. Die 34 Kern- und 26 Adapterprüfungen erhalten, endgültige Kandidatenbytes erneut durch Fake-I/O führen und Adapter-/Planbindungen neu versiegeln. Keine Firmwareänderung oder erneuter SDK-/Hostneubau allein für diesen Pythonfehler nötig. Originale und Gegenbeleg erhalten; keine Hardware oder Agentenstarts aus dem Review ableiten.

## Unabhängige Prüfung und Grenzen

- **3.555 Abschlussmanifestdateien, acht Symlinks und 3.296 Quellgraphdateien** ohne Abweichung geprüft. **44.711 Ursprungsdateien, vier ursprüngliche Symlinks und der eingefrorene Gitstatus mit 48 Dirty-Einträgen** vor den Koordinator-Dokumentationsänderungen erhalten. Deliverybindungen und beide Hostbindungen an den tatsächlich nachgerechneten 958-Dateien-Hostgraph stimmen.
- Vier Firmware-BIN-/ELF-/Configbindungen und Runtimefooter geprüft. Neues TX-only-Image **1.239.120 Byte**, **858.032 Byte Reserve** zur gespeicherten passenden 2-MiB-Partitionsreferenz. `CSI_LAYOUT_ATTESTATION=y`, `CSI_PEER_ENABLE=n`; Firmware-/Vendorquellen unverändert. Das ist keine aktuelle Geräte-/Slotbindung.
- **34/34 Ablaufprüfungen und 26/26 bestehende Adaptertests** unter gesperrter echter Datei-/Netz-/Prozess-/Geräte-/Credential-I/O wiederholt. Zusätzlich neuer, oben beschriebener Vertragsgegenbeleg erfolgreich reproduziert und privat gespeichert. Bestehendes Test-PASS hebt diesen Befund nicht auf.
- Finale Rusttestprogramme in eigenen temporären Kopien ausgeführt: **74 netzfreie Radar-/G2-Prüfungen je Default-/Peervariante**, je **ein expliziter C→Rust-RX2/RX3-Metadateninterop**, außerdem fünf Masken-/Replay- und zwei Legacyregressionen bestanden. Keine produktiven Server gestartet. Ein Loopback-Fixture wurde beim ersten zu breiten Filter erwartungsgemäß vom Netzverbot blockiert; nach ausdrücklichem Ausschluss dieses Fixtures bestehen die genannten 74 netzfreien Tests. Keine Produktregression daraus abgeleitet.
- Gespeicherte vollständige Hostlogs: je **561 PASS, null FAIL, sechs ignored, sieben gefilterte Socketfixtures**; getrennte Loopback-/Interopbelege erhalten. Keine neue vollständige Cargo-/SDK-Buildserie oder frische Nativekompilierung durch den Koordinator behauptet. Gezielte Wiederholungen nicht zu den Vollsuitezahlen addiert.

Alle wiederholten Rust-/Gegenbelegprogramme liefen mit Prozess-Netzverbot; reale Credentials wurden nicht gelesen. Temporäre Tests und private Reviewablage enthalten nur notwendige Test-/Reviewbelege. Die versiegelte Agentenlieferung blieb unverändert.

## Nächster Hardwareblock und Terminlage

**Heute Abend zuerst Dual-LD2450-G2**, als eigener konkret freizugebender Auftrag mit höchstens **110 Minuten einschließlich 15 Minuten Endreserve**. Benötigt: beide LD2450 samt physisch zugeordneten ESP-Knoten MMWAVE1/MMWAVE2, stabile Stromversorgung, Mac am bisherigen Hostweg, Maßband/Bodenmarken und eine Person. Zuerst beide Sensoren einzeln an bekannten Punkten, danach bei Eignung gemeinsam; feste Ziele Median ≤0,30 m, p90 ≤0,50 m und Coverage ≥90 %. Keine neue Transformations-/Geräteschreibfreigabe durch diesen Review. Nötige konkrete Werte müssen vor einer später separat erlaubten Änderung vorbereitet werden.

Der kleine P1-Adapterfix kann bis dahin offline erledigt werden. Er blockiert die getrennte Radarprüfung nicht. RX2/RX3-Metadateninstallation und RX1↔RX4-Paarfunk bleiben eigene spätere Aufgaben mit eigenen Freigaben. Heute keine weiteren Tageshäkchen: 14. Oktober weiterhin **1/5**, reale G2-Punkte offen.

Gegenüber dem ursprünglichen Plan besteht Rückstand bei den Hardwareabnahmen. Bei ein bis zwei Stunden täglich passen **110 Minuten Radar** und **bis zu 100 Minuten Paarlauf** nicht gemeinsam in einen Abend. Die für den 16. Oktober geplante vollständige Datensammlung beider Profile ist daher gefährdet und muss nach tatsächlichen Gates gestaffelt werden. Offlinevorbereitung nutzt die Zeit bis zum Anschluss; sie ersetzt diese Messungen nicht. Der 25. Oktober bleibt Ziel, ist derzeit keine belastbar zugesagte Fertigstellung.

**Schwierigkeit: 4/5** für den nächsten Hardware-/Projektabschnitt, einschließlich bisheriger Fehlversuche und getrennter physischer Nachweise. Kein Agent gestartet.

<!-- Publication copy: local paths and hardware identifiers were pseudonymized; private original retained. -->
# Koordinatorreview: Recorder-Endvertrag korrigiert

**Der bestätigte Recorderfehler ist offline geschlossen. Die RX1↔RX4-Vorbereitung ist damit technisch abgeschlossen; der reale Paarversuch bleibt offen.** Keine Hardwarezugriffe, Installation, Hostaktivierung oder Agentenbeauftragung durch den Koordinator.

Maßgebliche neue Lieferung: `./private/recordervertrag-korrektur-offline-20261015T083911Z/`. Der Abschlussbericht und der jüngste abgeschlossene Agententurn wurden geprüft. Die frühere Ablehnung in [Tagespaketreview](offline-tagespaket-review-20261015.md) bleibt als Vorherbefund erhalten und wird für diesen konkret korrigierten Fehler durch den neuen Review abgelöst.

## Bestätigte Wirkung

Der tatsächliche Capture-Endpfad liest jetzt `frames_written` aus Stopantwort und dauerhaftem Endmetadatum. Beide Werte müssen identische positive Integer im u64-Bereich sein. Bool, Float, String, null/negative Zähler, fehlende kanonische Felder und widersprüchliche Aliase werden abgelehnt. Aufnahme-ID, gestoppter/erfolgreicher Zustand, `completed`, fehlende Drops/Writerfehler sowie bestehende Start-/End-/Epochen-/Clockbindungen werden geprüft. Der Host wurde nicht an die Testattrappe angepasst; die vorhandenen Rustproduzenten sind die Quelle des Vertrags.

Der Ursprungscode und der Vorhergegenbeleg sind erhalten. Bei erneuter kontrollierter Ausführung scheitert der alte Adapter weiterhin am gleichen Endbeleg, vor dem ersten OTA-POST, mit zwei begrenzten Hostwechseln. Die korrigierte Implementierung besteht dieselbe reale Hostform unter Fake-I/O. Der gemeinsame Sessionkern, Firmware, Hostbinarys, Paarprofil und die unabhängig geprüften P2-/P3-Pakete bleiben unverändert.

## Unabhängig wiederholt

- **72 Abschlussmanifestdateien und 16 Quellbindungen**, sieben Deliverybindungen, beide tatsächlichen Hostproduzenten und neue Adapter-/Provenancebindungen stimmen. **3.555 Ursprungsmanifestdateien, acht Symlinks, fünf zusätzliche Original-/Reviewartefakte, 50 Dirty-Dateien und 24 referenzierte Kandidatendateien** unverändert; Gitstatus vor den Koordinator-Dokumentationsänderungen gleich. Kandidatenmanifest bytegleich zur geprüften Tagesbasis.
- **34 Kernprüfungen, 26 bisherige Adapterszenarien und zehn neue Recorderregressionstests** unabhängig erneut bestanden. Die zehn neuen Tests enthalten **292 Szenariodurchläufe**: ein guter Gesamtablauf, 290 kontrollierte neue Ablehnungen und ein Altadaptergegenfall. Szenariodurchläufe werden nicht als zusätzliche unabhängige Tests ausgegeben.
- Der echte finale Adaptereinstieg mit tatsächlichen gebundenen Kandidaten-/Host-/Profilbytes erneut unter Fake-I/O ausgeführt: **42 Geräte-GETs, zwei unterschiedliche korrekt große OTA-POSTs, zwei Captures, vier lokale Recordersteuerungen, drei Hostwechsel**. Null native Dispatches; Originalreceipts wurden durch Abfangen ihrer abschließenden Ausgabe erhalten.
- Der unveränderte alte Gegenbeleg wurde separat erneut unter I/O-Sperre ausgeführt und reproduziert. Alle wiederholten Test-/Simulationsprozesse liefen mit Prozess-Netzverbot und gesperrten echten Datei-/Credential-/Socket-/Prozess-/Geräteeinstiegen während des Dispatchs. Nur benötigte lokale Quellen/Fixtures/Kandidaten wurden vorher geladen. Keine realen Tokens, APIs oder Geräte gelesen.

Kein neuer SDK-/Hostbuild und keine unnötige Wiederholung vollständiger Rust-/Native-Suiten. Der Patch ändert nur Recorderprüfung, Testattrappen und notwendige Prüf-/Lieferhelpers; der Ablaufkern ist bytegleich. Die endgültigen Bindungen wurden vor der Dokumentationsfortschreibung geprüft. Die versiegelte Lieferung blieb unverändert.

## Urteil und nächste Schritte

| Bereich | Urteil |
|---|---|
| Recorder-Endvertrag | **KORRIGIERT_UND_REGRESSIERT** |
| P1 Realadapter / Gesamtintegration | **READY_OFFLINE / BESTANDEN** |
| P2/P3 und Kandidaten erhalten | **BESTÄTIGT** |
| Paarhardwarevorbereitung | **TECHNISCH ABGESCHLOSSEN**, spätere eigene Freigaben und aktuelle Preflightbindungen erforderlich |
| Reale Radar-/Peer-/Positions-/Ressourcenabnahme | **WEITER OFFEN** |

Damit ist der Tagespunkt **„RX1↔RX4-Versuch vorbereiten“** abgeschlossen. Die fünf Punkte des 14. Oktober stehen nun bei **2/5**: Vier-RX-CSI-Lauf und technische Paarvorbereitung erledigt. Layout-/Masken-/Zeitvertrag mit realer Vier-RX-Bindung, vollständige reale Fehler-/Ressourcenbilanz und tatsächlicher Paarfunk bleiben offen. Kein Hardware-PASS aus der Simulation und keine Änderung der Abnahmekriterien.

**Nächster Hardwareblock bleibt die getrennte Dual-LD2450-G2-Prüfung**, zuerst Sensoren einzeln, danach gemeinsam; höchstens 110 Minuten einschließlich 15 Minuten Ende/Reserve. Benötigt beide LD2450 samt physisch zugeordneten ESP-Knoten MMWAVE1/MMWAVE2, stabile Versorgung, Mac am bisherigen Hostweg, Maßband/Bodenmarken und eine Person. Vor Livebeginn konkreten Auftrag mit passenden Host-/Aufnahme-/Lese-/gegebenenfalls Unterbrechungsfreigaben erstellen. Keine Radar-/Transformationsschreibfreigabe aus diesem Review.

RX1↔RX4-Lauf und RX2/RX3-Metadateninstallation bleiben getrennte spätere Hardwareaufgaben. Der Paar-Liveeinstieg ist weiterhin LOCKED; in einem neuen Hardwareauftrag braucht es aktuelle konkrete Erwartungs-/Prozess-/Socket-/Endpointbindungen und passende Freigaben. Keine automatische Fortsetzung, kein weiteres allgemeines Offlinekorrekturpaket aus den jetzt geprüften Befunden begründet. Terminrisiko des Gesamtplans bleibt bestehen.

**Schwierigkeit des nächsten Hardwareblocks: 4/5.** Kein Agent gestartet.

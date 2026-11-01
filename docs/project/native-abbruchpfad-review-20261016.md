<!-- Publication copy: local paths and hardware identifiers were pseudonymized; private original retained. -->
# Koordinatorreview: nativer Abbruchpfad und Hardwarebereitschaft

**Die Offlinekorrektur ist unabhängig bestanden. Der begrenzte RX1↔RX4-/Vier-RX-Hardwarelauf ist technisch freigabefähig. Kein weiterer Offlineblocker aus diesem Korrekturauftrag; frischer Preflight und passende ausdrückliche Geräte-/Hostfreigaben bleiben vor der Ausführung erforderlich.**

Stand: 16. Oktober 2026, Europe/Berlin. Geprüft wurden der Abschluss des bestehenden Chats „RX-Livevorbereitung offline abschli“ und die [neue finale Lieferung](../../private/rx-native-endkorrektur-offline-20261016-20261016T154149Z/REPORT.private.md). Kein Agent gestartet oder angeschrieben, keine Geräte-/Netz-/Hostaktivierung, Löschung oder Neubauten durch den Koordinator.

## Unabhängig ausgeführt und geprüft

Den gelesenen neuen Prüfeinstieg in ein frisches privates Ergebnisziel ausgeführt, unter OS-Netz-/Geräte-/Originalschreibsperren. **3.619 Lieferobjekte bestehen** nach Dateisatz, Rechten und Hashes; Quellen, referenzierte Originale sowie neue Adapter-/Sessionbindungen sind geprüft. Der Prüfer hat **alle 16 nativen Endpfadtests tatsächlich erneut ausgeführt: null Fehler, null Fehlschläge**. [Eigener Prüflauf](../../private/rx-native-endkorrektur-koordinatorreview-20261016/CHECK.private.json), [Reviewbindungen](../../private/rx-native-endkorrektur-koordinatorreview-20261016/REVIEW.private.json).

Die native Gegenprüfung verwendet echte `NativeOS.guard`, `RealAdapter.storage_guard`, Stop-/Mitschnitt-/Recorder- und Session-Abbruchmethoden. Nur untere Datei-/Uhr-/Popen-/Kernelgrenzen sind injiziert; keine No-op-Ersetzung der zu prüfenden Guards. Beide Reserveszenarien mit null beziehungsweise 8 MiB eigenen Dateien bestehen: Bei einem Byte unter der verbleibenden Reserve wird ein registriertes eigenes Kind beendet; der volle normale Speichervergleich lehnt denselben Fehlbetrag weiterhin korrekt ab. Die alten Gegenbelege und deren Vorherreproduktion sind unverändert gebunden.

Zusätzlich frisch bestanden: eigener Stop/Poll/Reap bei Nullplatz, Stopfehler und Timeout; wirklicher Capture-`finally` mit einmaligem Recorderstop; `Session.finish_abort` vor/nach modelliertem Upload; ENOSPC mit eigenem Kindaufräumen und ehrlich unbestätigtem Ende; Abweisung fremder/ausgetauschter Handles, gefälschter Cleanupnamen, Upload-/Start-/URLmissbrauch sowie falscher Besitz-/Auth-/Vertrags-/Mengen-/Zeit-/Volume-/Originalbindungen. Default-Rückkehr benötigt die getrennt aus vorhandenen Caps abgeleitete Endressource und den tatsächlichen reservierten Enddispatch.

Der gespeicherte vollständige betroffene Regressionslauf mit **106 Tests** ist über Quellen-/Receipthashes geprüft; er wurde im Koordinatorreview nicht zusätzlich erneut ausgeführt. Die 16 oben genannten nativen Tests sind frisch. Keine neuen großen Last-/Rust-/SDKläufe erforderlich oder ausgeführt.

Beide fertigen normalen Hostprogramme nach ihren unveränderten SHA-256 geprüft und erneut nur mit `--help` beziehungsweise ohne Launch-Permit unter OS-Sperren ausgeführt: Hilfe Exit 0, fehlendes Permit Exit 78. Default `0750152f3b66596c5120b6a84bcff00c082e66cbf4bdb4d56cea4564c68cda2c`, Peer `476ce0ec5bf1f173f870f0caa27a6da6ae1cc691bf64923b5e89d1944e5db472`. Kein Host-/Firmware-Neubau, kein produktiver Start und keine gültige Livefreigabe erzeugt.

## Speicher und Freigabeentscheidung

Nach den unabhängigen Prüfausgaben wurden **84.636.823.552 Byte = 84,637 GB = rund 78,82 GiB** frei beobachtet. Der echte unveränderte Startguard besteht bei Bedarf **70.922.518.528 Byte = 66,051743 GiB**; Marge **13.714.305.024 Byte = rund 12,77 GiB**. Dies ist keine exklusive Reservierung. Vor späterem Zugriff erneut auf den tatsächlich gebundenen Schreibzielen prüfen. Keine weitere Speicherbereinigung aus diesem Review erforderlich.

Der neue Runtimeadapter ist ausschließlich `./private/rx-native-endkorrektur-offline-20261016-20261016T154149Z/adapter/real_adapter.py`, SHA-256 `dbbbc887eff5b7b211f650a51e8f76d529021e41f13fbe56f650156e572bb96c`. Sessionkern SHA-256 `508fa41ba6a04e8f529031547e00474c4deb7a6f7ed211b2a37e7152a57ce792`. Die alte fehlerhafte Runtime nicht verwenden. [Hardwareübergabe](../../private/rx-native-endkorrektur-offline-20261016-20261016T154149Z/LIVE-START.NICHT-AUSFUEHREN.private.md) und Vertrag bleiben historisch korrekt `LOCKED_NOT_AUTHORIZED`; dieser positive Review ersetzt keine menschliche Unterbrechungs-/Schreibfreigabe.

## Nächster konkreter Schritt

Den separat freizugebenden Hardwareauftrag jetzt auf dieser neuen Runtimebasis konkretisieren: frische Host-/Geräte-/MAC-/Interface-/Config-/Boot-/Image-/Slot-/Geometriebindungen; vorhandenen oder abwesenden Anfangshost prüfen und den gebundenen Default starten; zuerst die Basis 480 s mit dem bisherigen Firmwarestand samt Vorklammer/Nachklammer erfassen; danach RX1 und RX4 nacheinander mit den bereits gebundenen Apps installieren und VALID bestätigen; anschließend Paarvergleich 960 s, abschließend bestätigter Default/TX-only-Host und beendeter Peerbetrieb. Diese Reihenfolge folgt exakt STEPS; der Basis→Paar-Vergleich bleibt eine kombinierte Firmware-/Peerintervention.

Bestehende Obergrenzen bleiben zwei App-Schreibzugriffe nur RX1/RX4, zwei Captures, vier Recordersteuerungen, drei Hostwechsel, 82 geplante normale/höchstens 120 GETs einschließlich zwölf End-GETs, maximal 6.000 s einschließlich 600 s Ende. Zusätzliche nötige Preflightabfragen sind ausdrücklich in eine gemeinsame endliche Zählung einzubeziehen, keine ungezählten Vorläufe oder Budgeterneuerung. Geräteunterbrechung/App-Schreiben und Hostunterbrechung/-aktivierung sowie zulässige Lese-/Captureaktionen müssen für den konkreten Lauf freigegeben sein. TX1/RX2/RX3 unverändert; keine Resets, ROM-/seriellen Zugriffe, Restore-, Radar- oder Netzumbauten.

Hardware: TX1 und RX1–RX4 an festen Positionen mit normaler Stromversorgung, Mac am Netzteil im bestehenden Projekt-WLAN; keine COM-/Mac-USB-Datenkabel und keine Radare für den Funkvergleich. Die frühere Nichterreichbarkeit von `127.0.0.1:8080` ist ein historischer Hostbefund; kein neuer Onlinezustand wurde im Review erhoben. Der vorbereitete Abwesenheitsfall macht einen ungeprüften Reset unnötig.

**Dieser Lauf kann den fehlenden echten zweigerichteten Funk-/Transportbefund liefern. Er schließt die drei vollständigen Kriterien vom 14. Oktober nicht automatisch:** Vier-RX-Attestation, paketkorrelierte Firmwarezeit und vollständige Ressourcenbelege bleiben bei fehlenden Produzenten getrennt offen. Kein RX2-/RX3-Metadatenrollout oder Positions-/Radar-PASS aus einem guten Funkergebnis. Tagesstatus bleibt bis zum tatsächlichen Nachweis **2/5**.

**Schwierigkeit des nächsten Hardwaregesamtlaufs: 4/5**, unter Berücksichtigung der bisherigen Anläufe und der nun geprüften Abbruch-/Rückkehrpfade.

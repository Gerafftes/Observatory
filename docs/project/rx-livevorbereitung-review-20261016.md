# Koordinatorreview: Laufkopien, Neubauten, Tests und Speicher

**Laufkopien, beide Host-Neubauten und aktuelle Speicherdeckung bestehen die unabhängige Lieferprüfung. Der Hardwarelauf ist noch nicht freigabefähig: ein neu reproduzierter Fehler im nativen Abbruch-/Endpfad muss gezielt korrigiert werden.**

Stand: 16. Oktober 2026, Europe/Berlin. Geprüft wurde der Abschluss des Chats „RX-Livevorbereitung offline abschli“ und die [private finale Lieferung](../../private/rx-livevorbereitung-offline-20261016-20261016T143646Z/REPORT.private.md). Keine Hardware-/Geräte-/Hostaktivierung, kein Agent gestartet oder angeschrieben, keine Bereinigung oder Neubauten durch den Koordinator.

## Bestanden

- Den gelieferten Prüfeinstieg unabhängig unter dessen OS-Netz-/Geräte-/Originalschreibsperren in einen neuen privaten Ergebnisordner ausgeführt: **5.436 Einträge bestanden**, einschließlich Quellen-/Abhängigkeitsgraphen und tatsächlichen finalen Binärdateien. [Eigenes Prüfreceipt](../../private/rx-livevorbereitung-unabhaengig-check-20261016/CHECK.private.json).
- Die neuen Default-/Peerhostprogramme sind getrennt gehasht: 62.394.544 beziehungsweise 62.701.216 Byte. Die unabhängige Prüfung hat beide echten Programme mit `--help` erfolgreich und ohne Permit mit Exit 78 abgewiesen gestartet, ohne produktive Aktivierung. Keine Verwechslung mit alten pauschal gesperrten Programmen.
- Der Quellvergleich zeigt die explizite Launch-Permit-/Vertragsbindung, eigenen Datenpfad, RX-Modus ohne Radarreceiver/-Discovery/-Poller sowie vorhandenen/abwesenden Anfangshost. Originale und Produktquellen wurden in dieser Lieferung nicht geändert.
- Die gehashten gespeicherten Nachweise enthalten **94 Regressionen, zwölf neue Tests, 17 tatsächliche CLI-Proben**, zwei vollständige synthetische Adapter-CLI-Dateiläufe mit injizierten unteren Grenzen sowie fünf Datei-/Writerfehlerfälle. Diese Suiten werden nicht als im Koordinatorreview nochmals vollständig ausgeführt ausgegeben. Die vorhandenen Tests erfassen den unten gezeigten nativen Endfehler nicht.
- Beim unabhängigen Liefercheck **84.576.813.056 Byte = 84,577 GB = 78,768 GiB frei**. Unveränderter Bedarf **70.922.518.528 Byte = 66,051743 GiB**; Marge **13.654.294.528 Byte = 12,717 GiB**. Der tatsächliche Startguard besteht. Keine neue Speicheroptimierung erforderlich; Platz vor einem späteren Start weiterhin frisch prüfen.

## Ein blockierender Befund: verbleibende Aufnahmereserve sperrt eigenes Aufräumen

In [NativeOS.guard](../../private/rx-livevorbereitung-offline-20261016-20261016T143646Z/adapter/real_adapter.py:151) wird Zeile 159 `a.storage_guard()` für jede native Aktion aufgerufen. Auch `NativeOS.terminate()` und der native Mitschnitt-/Recorder-/Endpfad benutzen diesen Guard. Die bisherige gezielte Ausnahme höher im Adapter für Endaktionen wird dadurch unten wieder aufgehoben.

**Unabhängig ausführbar reproduziert:** Mit gültigen ausschließlich im Speicher gehaltenen Testbindungen, echten `NativeOS`-/`RealAdapter.storage_guard`-Implementierungen und einem künstlichen eigenen Kindprozess erreicht der Stop bei ausreichendem Platz dessen `terminate()`. Bei einem Byte unter der **verbleibenden** Reserve erreicht dieselbe Endaktion den Stop nicht mehr: `calculated storage reserve absent`. Zusätzlich mit 8 MiB bereits vorhandenen eigenen Ausgaben reproduziert; deren Verbrauch wird richtig von der Reserve abgezogen. Das Problem betrifft damit den tatsächlichen nativen Gatepfad, nicht die reale derzeitige Speicherdeckung.

Alle echten Prozess-/Geräteprimitive waren gesperrt beziehungsweise durch harte Fehler ersetzt. Kein echter Prozess gestartet oder beendet und keine gültige Livefreigabe auf Datenträger erzeugt. [Reproduzierbare Gegenbelege und Reviewbindungen](../../private/rx-livevorbereitung-koordinatorreview-20261016/REVIEW.private.json). Erste Probe und Gegenprobe bleiben unverändert erhalten.

**Auswirkung:** Wenn während eines späteren Laufs ein Speicherfehler auftritt, können gerade die notwendigen begrenzten Stop-/Aufräumaktionen eigener Prozesse verhindert werden. Eine gespeicherte Fake-Session mit korrektem Endzustand beweist diesen NativeOS-Pfad nicht. Die Lieferung darf deshalb trotz positivem allgemeinen Liefercheck nicht zum Hardwarelauf freigegeben werden.

## Konkreter nächster Schritt

Nur den Python-Adapter und seine tatsächlichen nativen Endpfadtests auf einer neuen kleinen privaten Kopie korrigieren. Neue Mess-/Start-/OTAaktionen müssen bei fehlender Reserve weiter gesperrt bleiben. Der bereits autorisierte begrenzte Endpfad für nachgewiesen eigene Kinder und reservierte Stop-/Endaktionen benötigt eine passende eigene, enge Prüfung; keine pauschale Entfernung des Speicherschutzes und keine Umgehung von Auth-, Identitäts-, Besitz-, Mount-, Mengen- oder Fristgrenzen. Fremde Prozesse dürfen weiterhin nie angefasst werden. Endzustand nur bei tatsächlichem Nachweis bestätigen; andernfalls ehrlich NICHT_BEURTEILBAR.

Die Gegenbelege auf der Korrektur erhalten und vorher/nachher ausführen. Native Cleanup-/Capture-/Recorderstopfälle unter Speicherabfall, gebundene Reserven und unzulässige neue Arbeit gezielt mit injizierten unteren Primitiven testen. Danach kleine Regressionen/Liefercheck und die aktuelle Speicherdeckung erneut prüfen. Für diesen Befund ist **kein neuer Rust-/Firmware-Neubau begründet**; fertige neue Hostprogramme und ihre Hashes weiterverwenden, solange deren Quellen unverändert bleiben. Keine erneute allgemeine CSI-, Speicher- oder Firmwareanalyse.

Danach unabhängiger Review und separat freigegebener Hardwareauftrag: RX1/RX4 nacheinander installieren, 480 s Basis und 960 s Paarvergleich, TX1/RX2/RX3 unverändert. Die drei vollständigen Punkte vom 14. Oktober werden dadurch nicht automatisch alle geschlossen: fehlende Vier-RX-Attestation, paketkorrelierte Firmwarezeit und vollständige Ressourcenproduzenten bleiben im [Abschlussplan](../../private/rx-livevorbereitung-offline-20261016-20261016T143646Z/RX-ABNAHME-14.private.md) ausdrücklich offen. Ein echter Funk-/Transportteilbefund ist getrennt sinnvoll bewertbar.

Hardware bleibt vorbereitet an festen Positionen mit Strom; für diesen RX-Lauf Mac im Projekt-WLAN am Netzteil, keine COM-/USB-Datenkabel oder Radare. Kein neuer Onlinezustandsnachweis. Tagesstatus weiterhin **2/5**. **Schwierigkeit der gezielten Korrektur: 3/5; Hardwaregesamtlauf: 4/5.**

# Koordinatorreview: CSI-Datenbasis und C1-Paarvorbereitung

**Noch nicht bereit für einen Hardwarelauf.** Die jüngere Gegenprüfung widerruft die erste READY_OFFLINE-Einschätzung. Vier konkrete Code-/Bindungsfehler und zusätzliche Planlücken müssen in einer neuen Offline-Korrekturlieferung geschlossen werden. Kein neuer Hardwareanschluss oder Installationsversuch ist dafür nötig.

Grundlagen: [ursprüngliche Offline-Lieferung](../../private/csi-datenbasis-peer-c1-offline-20261014T191140Z/REPORT.private.md), tatsächliche Quellen/Builds sowie der neuere [Gegenprüfungsbericht](../../private/csi-datenbasis-peer-c1-review-20261014T201219Z/REVIEW.private.md). Beide Abschlussnachrichten aus Agentenchat „14“ wurden gelesen; der neuere Befund hat Vorrang. Dieser Review war ausschließlich lokal. Keine Hardwareaktion, Geräte-/Netzabfrage, Installation, Hostaktivierung oder Agentenstart.

## Getrenntes Bereitschaftsurteil

| Paket | Ergebnis | Begründung |
|---|---|---|
| CSI-Layout/Maske/Zeit | **NICHT_READY für die Gesamtintegration** | Layoutattestation und maskierte Teilpfade vorhanden; Rückkehr nach Layoutwechsel und allgemeine Kalibrier-/Replaypfade fehlerhaft |
| Fehler-/Ressourcenbilanz | **TEILWEISE** | Gespeicherte Format-/Sequenz-/Diagnoseauswertung belegt; reale TX-, Socket-, Interface-, CPU-, Heap- und Queuebelege nicht vollständig |
| RX1↔RX4 auf C1 | **NICHT_READY** | Kandidaten gebaut und gebunden; Senderprofil sowie harte Sendefrische noch nicht abgesichert |
| Hardwareauftrag | **NICHT_FREIGABEFAEHIG** | Zuerst vier Fehler korrigieren, neue Tests/Buildbindungen und konkrete Start-/Budget-/Endzustandsregeln liefern |

## Vier bestätigte Blocker

1. **Senderprofil fehlt.** Die geplante Abnahme fordert non-HT/20 MHz/LLTF64; der tatsächliche Peerpfad setzt keine passende PHY-Senderate. Das lokal gebundene SDK dokumentiert für `esp_wifi_80211_tx()` standardmäßig 1 Mbps. Bandbreitensetzung allein bindet die benötigte OFDMrate nicht. Die öffentliche Ratesetzer-API verlangt die Reihenfolge nach WiFiinitialisierung und vor WiFistart. Diese Produktionsstartfolge wird vom vorhandenen Testzweig mit unbedingtem ESP_FAIL nicht abgedeckt. Benötigt: explizites Profil nur im Paarpfad, Fehlerabbruch, Start-/Fehler-/Defaulttests und LLTF64-C/Rust-Interop.
2. **Layoutwechsel sperrt die Rückkehr.** Tatsächlicher Guard und Layoutqualifikation: gültige 128er-Form → gültige 64er-Form → 100 gültige 128er-Frames bei gleichem Host/Boot/Quelle/Processing. Alle 100 Rückkehrframes werden dauerhaft als retired verworfen. Ursache ist das gemeinsame Retirement von langlebiger Identität und wechselnder Frameform. Benötigt: beides trennen, Fenstergrenzen weiterhin erkennen, gültige Formrückkehr nach frischem Fenster erlauben und alte Boots weiterhin sperren; Tests am tatsächlichen Aufnahme-/Liveingress.
3. **100-ms-Grenze wird nach Backoff verletzt.** Tatsächlicher Transport mit synthetischer Uhr: erster Sendversuch bei 95 ms Alter scheitert, zehn Millisekunden Wartezeit, zweiter Send bei 105 ms gelingt. Die spätere Expiredzählung verhindert den schon erfolgten Send nicht. Benötigt: Alter nach jeder Wartezeit unmittelbar vor Send prüfen, Grenzfälle testen und verfallene Frames korrekt bilanzieren.
4. **Physische Maske fehlt im allgemeinen Kalibrierpfad.** Die neue spezielle Positionsprojektion maskiert, der gemeinsame D6-/Präsenz-/Replayeingang ruft weiter den unmaskierten Wrapper auf. Extremwerte ausschließlich in DC/Padding verändern dort die Referenz; er berücksichtigt 128/128 statt der im Repro physisch zulässigen 108/128 Bins. Derselbe Datensatz bleibt mit der maskierten Methode unverändert. Benötigt: Maske und Layoutidentität durch den gesamten vorhandenen gemeinsamen Kalibrier-/Live-/Replaypfad führen; unbekannte Formen nur diagnostisch zulassen; End-to-End-Gegenproben und Referenzbindungen prüfen.

Diese Fälle sind offline bearbeitbar. Die fehlenden realen Ressourcenbelege sind keine Ursache dieser vier Ablehnungen. Die Befunde beziehen sich auf die neuen, nicht aktivierten Kandidaten; der bestätigte installierte C/C1-Stand wird hier nicht ersetzt.

## Hardwareplan vor einer späteren Freigabe präzisieren

Die feste Basis-/Paarmessung mit 480/960 Sekunden, getrennte RX-/Hostfreigaben und gesperrter Liveeinstieg sind vorhanden. Noch erforderlich:

- Konkrete maximale Statusanzahl, Abstände, Timeouts sowie mindestens zwei unabhängige stabile VALID-/Boot-/Configbelege je Ziel festlegen.
- Vor jeder nächsten Aktion deren Kosten und Restzeit einschließlich gesamter Paaraufnahme und Endprüfung prüfen; Aktionen vor Versand verbuchen. Der derzeitige Boolvalidator ersetzt keinen geprüften Ablauf mit monotonen Deadlines und RX1-vor-RX4-Gates.
- Den Endzustand des Peer-Senders ausdrücklich festlegen. Die Firmware startet ihn beim Boot und sendet nach Messende weiter. Daraus folgt keine Freigabe zum Dauerbetrieb und keine zusätzliche Restore-/Reset-/Uploadaktion. Begrenzten Betrieb beziehungsweise dessen ausdrücklich geregeltes Ende offline vorbereiten und testen.

## Unabhängige Koordinatorprüfung

Alle **5.875 Lieferdateien**, **16 Symlinks**, beide Quellgraphen und **3.340 gebundene Eingänge** erneut geprüft; keine Abweichung. Zusätzlich alle **37 Gegenprüfungsdateien** gehasht. Drei S3-BIN-/ELF-/Configbindungen, beide Hostbinarys und selbst berechnete ESP-Imagefooter stimmen. Größen passen zur gespeicherten 2-MiB-Referenz, ohne frischen Geräteslotnachweis zu behaupten. Gespeicherte finale Rustlogs mit je 539 bestandenen, fünf ignorierten und sieben separat gefilterten Tests sowie die ausgewiesenen separaten Ausführungen sind konsistent. Keine erneute vollständige Rust-/SDK-Suite in diesem Koordinatorreview behauptet.

Guard-/Layout- und D6-Extraktionen stimmen exakt mit den geprüften Lieferquellen überein; D6-Extraktion entfernt nur Serialisierungsdekoration. Alle drei Gegenbeispiele in neuen temporären Kopien frisch kompiliert und erneut reproduziert, Transport unter ASan/UBSan. Originale unverändert.

Der verschachtelte Prozess-Netzsandboxwrapper war in dieser Koordinatorumgebung nicht verfügbar. Für diese Wiederholung wurden ausschließlich die zuvor gelesenen synthetischen SDK-/Uhr-/Frameharnesses verwendet; vor Ausführung die importierten Binarysymbole auf Netzwerk-/Prozessstarteinstiege geprüft, kein solcher Einstieg vorhanden. Keine SDK-/Firmware-/Serverneubauten oder Livekontakte. Der ursprüngliche Gegenreview liefert seinen eigenen Prozess-Netzverbotsnachweis. Gitstatus vor den Koordinator-Dokumentationsänderungen stimmt mit der eingefrorenen Lieferbasis überein.

Der historische Vier-RX-CSI-Befund **1.786/1.793 = 99,61 %**, strenge Regel **1.780/1.793**, bleibt bestätigt. Er attestiert keine neue physische Maskierung, Peerfunkqualität oder Positionsleistung. Tag 3 bleibt **1/5**; kein zusätzlicher Tagespunkt wird aus dieser fehlgeschlagenen Freigabeprüfung geschlossen.

## Nächster Schritt und Hardware

**Ein gebündelter Offline-Korrekturauftrag für die vier Blocker und Planlücken.** Die drei reproduzierten Gegenbeispiele als erhaltene Vorherkontrollen verwenden; nach gezielten Korrekturen tatsächliche Eingänge prüfen, passende Host-/S3-Kandidaten neu bauen und neue Hashes/Configs binden. Danach erneut Freigabefähigkeit prüfen; erst anschließend separater Hardwareauftrag. Keine breite Neuimplementierung oder Hardwaremessung als Ersatz.

Aktuell keine Hardware für diese Korrektur erforderlich. Für den späteren Lauf: TX1 und alle vier RX an ihren normalen Plätzen mit stabiler Stromversorgung, Mac im bestehenden Projekt-WLAN/Hostweg; RX1-/RX4-COMwege nur als vorbereitete Diagnoseressource, im geplanten OTA-Standardweg kein serieller Zugriff. RX2 bleibt C, RX3 C1; TX1 unverändert. Radare werden erst für den späteren Positionsnutzenvergleich gebraucht. Neue passende RX-Unterbrechungs-/Schreib- und Hostaktivierungsfreigaben bleiben Voraussetzung des noch gesperrten Hardwareauftrags.

**Schwierigkeit des nächsten Korrekturblocks: 4/5**, einschließlich der bisherigen Entwicklungs- und Prüfversuche. Die Fehler sind jetzt konkret reproduziert, betreffen aber mehrere Produktionspfade und den kontrollierten Betriebsabschluss. Kein Agent gestartet.

# Koordinatorreview: Formatauswahl und Vier-RX-Preflight

Stand: 13. Oktober 2026. **Beide Offline-Pakete sind abgesichert. Ein gemeinsamer Hardwarelauf ist der sinnvolle nächste Schritt; vor dem ersten Gerätezugriff fehlt noch die geprüfte Anbindung des echten I/O-Adapters.** G1 bleibt offen.

Grundlage: [Agentenbericht](../../private/formatauswahl-vier-rx-preflight-offline-20261013-v1/REPORT.private.md), Ausführungskopien, Produktpatch, Testlogs und gespeicherter PCAP. Keine Geräte- oder aktuellen Netzabfragen durch diesen Review.

## Unabhängig geprüft

- Alle 79 Manifestdateien außerhalb der Buildbäume, neun aktuelle Produktdatei-Hashes und 20 Eingangsbindungen stimmen. Der gebaute Hostkandidat stimmt mit seinem Lieferhash überein. Die übrigen Buildbaumdateien wurden nicht vollständig erneut gehasht.
- Alle 374 Dateien des ursprünglichen 30-Minuten-Abschlussmanifests stimmen weiterhin hinsichtlich Größe und SHA-256.
- Die 60 Python-Tests wurden unter dem mitgelieferten Offline-I/O-Schutz erneut ausgeführt und bestanden.
- Sieben gezielte Rust-Regressionen bestanden erneut: fünf Auswahl-/Epochentests, ein Test zum Reset bei ausgeschlossenen Frames und ein Test der tatsächlichen Recorder-/Live-Einstiege. Die versiegelte Testbinary wurde vor Ausführung gehasht und in einer neuen privaten temporären Kopie ausführbar gemacht; die Originaldatei bleibt unverändert mit Modus 0600. Ein erster direkter Start der Originaldatei war deshalb nicht ausführbar, kein Testfehler. Die Kopie wurde anschließend entfernt.
- Die gehashten vollständigen Rustlogs weisen 1.056 bestandene und sechs ignorierte Tests aus; Offline-Build erfolgreich. Die vollständige Suite wurde im Koordinatorreview nicht nochmals ausgeführt.
- Erneutes vollständiges Dekodieren des Original-PCAP reproduziert das alte Summary exakt. Neue Auswahl: dieselben 54.309 gültigen CSI, 54.101 passende 128er, 208 ausgeschlossene 64er und 267 ungeklärte Sequenzpositionen. Fenster alt 1.487/1.793, neue ausdrücklich diagnostische Auswahl 1.793/1.793. Gemeinsames Vier-RX-Urteil weiterhin NOT_ASSESSABLE.

## Bewertung der Änderungen

**FORMAT_SELECTION_OFFLINE_READY: READY.** Der Diagnosehelfer bilanziert gültige Fremdformate weiterhin vollständig und verwendet nur passende Frames für Abdeckung und Rate. Identitäts-/Epochengrenzen bleiben sichtbar. Die Produkt-Mischrasterprüfung bleibt bestehen; in den Feature-Eingang dürfen keine unterschiedlichen Raster gelangen.

Zusätzlich schützt ein kleiner Hostpatch die Epochengrenzen auch vor dem frühen Off-grid-Return. Boot-/Quellen-/verfügbare Konfigurations-/Radio-/Hostepochenwechsel leeren Live-Fenster und Konsens und nutzen den bestehenden Recorder-Rejection-Pfad. Reine gültige 64-/192er-Abweichungen bei sonst stabiler Identität erzeugen keinen solchen Wechsel. Dieser Hostpatch ist offline gebaut, nicht aktiviert. Der Review hat ihn weder installiert noch einen Dienst gestartet. Physische LTF-/Bin-/Maskenattestierung und vollständige monotone Live-Zeitumstellung bleiben offen.

**FOUR_RX_PREFLIGHT_OFFLINE_READY: READY.** Bekannte MAC und vorgesehenes Interface bestimmen den eindeutigen Snapshot-Endpunkt. Vier RX, eigene Images/Slots/Boots/Configreferenzen, Zieltuplen und vollständiger Filter sind zwingend. Teilmengen, Widersprüche, Endpunktwechsel, fehlende Budgets oder Timeouts stoppen vor Capture. Authentisierung setzt bestätigte Antwortidentität voraus. Die Tests belegen diese Regeln mit Fake-I/O, nicht die gegenwärtige Erreichbarkeit der Geräte.

**Operative Grenze:** `preflight.OfflinePlan` verlangt einen injizierten Adapter; ein echter Adapter ist in diesem Paket nicht implementiert. Die früheren Live-Einstiege sind in der neuen Kopie deaktiviert. Daher ist noch kein unverändert startbares Live-Messwerkzeug geliefert. Das ist im Bericht ausdrücklich ausgewiesen und mit dem Offlineumfang vereinbar. Die Anbindung darf die getestete Startschranke nicht umgehen oder durch den alten Teilmengen-Collector ersetzen.

## Nächster konkreter Schritt

Ein einziger neu freizugebender Auftrag soll die Adapteranbindung und anschließend den gemeinsamen rein lesenden Vier-RX-Lauf bündeln:

1. Zuerst den kleinsten echten Lese-/Capture-Adapter an den geprüften Ablauf anschließen. Vor dem ersten Livezugriff denselben Adapter über ersetzte Prozess-/Netzgrenzen testen: vier RX positiv, Teilfilter negativ, Auth-Reihenfolge, Zeit-/GET-Budgets, einmaliger Verbrauch und Cleanup. Neue private Kopie und Bindungen versiegeln.
2. Am vorgesehenen Aufbau TX1, RX1–RX4 und den Mac verwenden. TX1/RX2 standen zuvor am Wartungsplatz; die tatsächlichen Messpositionen vorab durch Johann bestätigen lassen. Keine Gerätebewegung, Stromunterbrechung oder Netzänderung durch den ausführenden Agenten ableiten. Für diesen passiven Leselauf sind weder Radar noch UART erforderlich.
3. Im später erlaubten Liveabschnitt frische MAC-/Interface-/Routen-/Antwort-/Zielbindungen aller vier RX prüfen. RX2s C/VALID frisch bestätigen; keine historische Statusantwort als aktuellen Nachweis verwenden. Fehlt ein RX, vor dem langen Capture stoppen, keine Reparatur und kein Retry.
4. Genau ein gemeinsamer Capture: 2 s Vorlauf, 1.800 s Messfenster, 2 s Nachlauf; maximal 24 RX-GETs und 45 Minuten aktiver Rahmen. Gültige Rasterfenster, Formatausschlüsse, ungeklärte Lücken und fehlende Beobachtungen getrennt berichten. Keine Firmware-/Config-/Hostdienständerung; der neue Hostpatch ist für den passiven Transportlauf nicht erforderlich.

G1 erfordert weiterhin den gemeinsamen Nachweis aller vier RX und die vereinbarten weiteren Daten-/Systemgrenzen. Weder der RX2-Replay noch ein positiver Preflight ersetzen die Live-Abnahme. Ein passiver PCAP beweist weiterhin keine Serverzustellung oder Positionsgenauigkeit. Keine automatische Fortsetzung; in diesem Review keine neue Livefreigabe und kein Agentenstart.

**Schwierigkeit des nächsten gebündelten Hardwareauftrags: 3/5.** Frühere Fehlversuche sind berücksichtigt. Die bisher offenen Auswahl- und Zulassungsregeln sind jetzt getestet; Adapteranbindung, frische Vier-RX-Bindung und tatsächliche gleichzeitige Datenqualität müssen noch gelingen.

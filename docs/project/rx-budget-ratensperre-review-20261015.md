# Koordinatorreview: Dateibudget, Vorklammern und Ratensperre

**Der angeforderte Offlineblock besteht die unabhängige Gegenprüfung. Der Hardwarelauf kann aktuell noch nicht starten: Der eingefrorene Ablauf verlangt 80 GiB freien Speicher, vor diesem Review waren nur 28,29 GiB verfügbar. Zusätzlich fehlt die separat geprüfte und freigegebene Livekopie; Adapter und normale Hostbinarys sind weiterhin absichtlich gesperrt.**

Stand: 15. Oktober 2026, Europe/Berlin. Grundlage ist die Lieferung [rx-budget-vorklammern-offline-20261015T184011Z](../../private/rx-budget-vorklammern-offline-20261015T184011Z/REPORT.private.md). Autoritativ sind deren CANDIDATES-FINAL, SOURCE-MANIFEST-RELEASE, finaler Quellgraph, Adapter-/Dateiprogrammbindungen und MANIFEST/SEAL. Gleichnamige ältere Zwischenbindungen ersetzen diese nicht. Kein Agent gestartet oder angeschrieben; keine Hardware, Installation oder Hostaktivierung durch den Koordinator.

## Dateibudget und Last

Endliche Grenzen: Trace 8 GiB, Raw 4 GiB, Peer 512 MiB, PCAP 1 GiB. Tracequeue unverändert 512 Records, Datensatz höchstens 16 KiB, Payload höchstens 4 KiB, begrenzter Traceabschluss zwei Sekunden. Die unabhängige Rechnung bestätigt die Grenzen einschließlich 20 s Phasenreserve und 44 konservativ reservierten Rand-/Queuedatagrammen.

Basis: viermal 50 CSI/s; Paar: TX1-Anteile 40/50/50/40 plus zweimal zehn Peerframes/s, insgesamt weiterhin 200 CSI-Datagramme/s. Dazu nominal bis zu zehn Syncdatagramme/s; Peer wird nicht nochmals auf das gemeinsame 50-Hz-Limit addiert. Bei maximaler Datensatzgröße beträgt der gebundene Tracebedarf für den Paarpfad 6.745.096.192 Byte unter 8.589.934.592 Byte. Auch Raw-, Peer- und PCAPrechnung passen zu ihren endlichen Grenzen. Zusätzlicher Verkehr kann die Reserve verbrauchen; Überlauf bleibt ein sichtbarer Fehler, keine stille Stichprobe.

Der versiegelte tatsächlich ausgeführte 480-/960-s-Dateiproduzentenlauf besteht mit schneller synthetischer Zeit, 200 CSI/s plus Syncverkehr. Paar: **695.075.058 Tracebytes, 392.640 versuchte und geschriebene Records, null Queuedrops, kein Writerfehler**; beide Richtungen je 9.600 Peerpakete. Die hohe Last wurde in diesem Review anhand versiegelter Dateien, Receipts, Quell-/Binarybindungen und Rechnung geprüft, **nicht vollständig erneut erzeugt**. Die aufgezeichneten Prozessspitzen und etwa 2.030 s Offline-Wallzeit sind keine Gerätewerte und kein Echtzeitdurchsatznachweis.

## Vorklammern und Ablauf

Vollständige Status-/Acht-Felder-Config-/Diagnoseklammern aller vier RX vor und nach beiden Aufnahmen. Vor dem Paar liegt die Vier-RX-Vorklammer vor Öffnung des begrenzten Peerhosts; die Nachklammer folgt nach dessen Rückkehr. Beobachtete Anfrageintervalle und maximal 200 s Alter bis Messbeginn sind ausdrücklich gebunden. Wiederholte/rückläufige Diagnosesnapshotzeiten desselben Boots/Images/Sources werden abgewiesen. Endantworten füllen keine fehlende Vorklammer rückwirkend.

Neuer Vertrag: **82 normale GETs, maximal 120 einschließlich zwölf reservierter End-GETs**. Genau zwei App-POSTs ausschließlich RX1/RX4, zwei Captures mit 480/960 s, vier Recordersteuerungen und drei Hostwechsel. Konservative Restpfadrechnung 4.900 s unter 6.000 s einschließlich 600 s Endreserve; Peerhostpfad 1.100/1.200 s. Neue Normalzahlen sind sichtbar, historische Verträge erhalten.

## Ratensperre und Neubauten

Default prüft gültige Eingänge und TX-Quelle vor dem gemeinsamen phasenbezogenen Zeitentscheid. 500 µs Frühtoleranz, kein Nachholguthaben, explizite Initialisierung für Zeitnull und Ablehnung ungültiger/rückläufiger Uhrwerte. Paar-Gesamt-/Peerzeitentscheide werden zunächst auf Kopien geprüft und gemeinsam übernommen; TX1 passiert danach keine zweite Collectorsperre. Die bestehenden 50/40/10-Quoten sowie acht Poolplätze mit 6/2-Reservierung bleiben erhalten.

Die Unterscheidung der Pfade ist korrekt: Der alte Default-Fremdframefehler wird nicht als im alten Paarpfad reproduzierter Fehler ausgegeben. Dort reproduzieren die strikten Zeitentscheidungen und die zusätzliche TX1-Collectorsperre die Jitterverluste. Diagnosepopulationen vor beziehungsweise nach Paarzulassung bleiben getrennt. Neuer Peerstatus v2 ergänzt disjunkte Zeit-/Quoten-/Sessionverwerfungen; der Statuspuffer wächst auf 2.048 Byte. Nicht exportierte Fremdzähler bleiben unbekannt.

Vier lokale S3-Neubauten sind gebunden; jeweils BIN-/ELF-/Source-/Configbindung, gültiger Appfooter und Platz unter der gespeicherten 2-MiB-Referenz unabhängig bestätigt:

| Variante | Appbytes | Verwendung |
|---|---:|---|
| TX-only Default | 1.238.800 | Offlinekontrolle |
| RX1-Paar | 1.271.904 | Neuer Kandidat im gesperrten Hardwarevorschlag |
| RX4-Paar | 1.271.904 | Neuer Kandidat im gesperrten Hardwarevorschlag |
| TX-only Metadaten RX2/RX3 | 1.239.136 | Nur offline, kein Rollout im Paarauftrag |

Effektive Configwerte und Partitionstabellen stimmen mit den jeweiligen Referenzen überein. NVS-/Security-/Bootverhalten wurde nicht verändert. Neue Bootloaderbytes unterscheiden sich und sind ausdrücklich kein Installationsgegenstand. Das gespeicherte Netzverbot vor dem ersten SDK-Aufruf und die vier erfolgreichen Buildreceipts sind geprüft. Kein vollständiger SDK-/Hostneubau durch den Koordinator wiederholt.

## Unabhängige Ausführung und Grenzen

- Rein lesender Lieferverifier unter Prozess-Netzverbot bestätigt alle **37.837 Manifestpositionen**, finalen Kandidaten/Quellbindungen und Receipts. Eigene Prüfoutputs liegen außerhalb der schreibgeschützten Lieferung.
- Aus den tatsächlichen alten und neuen Produktionsquellen frisch mit ASan/UBSan kompiliert: **144 Ratenfallläufe**, alle **72 Nachherfälle PASS**, alte Erfolge und **52 erwartete Vorherfehler** reproduziert. Außerdem alle **sechs** nativen Peer-/Frische-/PHY-/Partnerregressionsgruppen neu kompiliert und bestanden.
- Echte Kern-/Adapter-/Recorder-/Klammerpfade mit gesperrten realen Datei-/Netz-/Geräte-/Prozesseinstiegen: **34 + 26 + 10 + 8 Prüfungen**, einschließlich **292 Recorder-Szenariodurchläufen**, erneut bestanden.
- Tatsächliche finale App-/Host-/Profilbytes in den echten Adapter eingebunden und ausschließlich mit Fake-I/O ausgeführt: **82 GETs, zwei POSTs, zwei Captures, drei Hostwechsel**, keine Native-I/O. Vier alte Runtime-/Source-/Policy-/Schemabindungen werden vor dem ersten Dispatch abgewiesen.
- Geschlossene synthetische RX-Produzentenfiles erneut normalisiert, ausgewertet und verglichen; fachliche Runergebnisse stimmen exakt. Alle **zwölf** manipulierten Originaldatei-Klammerfälle werden abgewiesen. Vier frisch aus dem korrigierten C-Pfad erzeugte HT128-/LLTF64-Pakete bestehen Rustparser, physische Maske und bytegleichen Rückweg.
- Gespeicherte volle Hosttestsuiten je **568 PASS**, sechs ignored und sieben ausgeklammerte Netzfixtures sowie hohe Last-/Radar-/Grenzfehlerreceipts sind gehasht. Diese Vollsuiten, Dauerlastproduktion und Diagnosesnapshot-Stress wurden im Review nicht nochmals komplett ausgeführt. Keine solche Wiederholung behauptet.

Der öffentliche Arbeitsstatus war vor den Koordinator-Dokumentationsänderungen unverändert. Die Ursprungserhaltung ist in der Lieferung separat dokumentiert; im Review waren alle Original-/Lieferwrites durch das OS gesperrt. Zwei Fehler in eigenen Prüfglue-Annahmen (Tuple-Rückgabe und durch die Sandbox untersagte Hardlinks) wurden in eigenen Prüfkopien korrigiert; die endgültigen Wiederholungen bestehen. Kein Produktfehler daraus abgeleitet. [Private Reviewbelege](../../private/rx-budget-ratensperre-koordinatorreview-20261015/REVIEW.private.json).

## Startbedingungen und nächster Schritt

Der vorhandene Plan ist eine geprüfte **Vorlage für einen getrennt freizugebenden Hardwareauftrag**, kein direkt startbarer Livebefehl. `LIVE_LOCKED=True` im Adapter und der unbedingte Exit 78 im normalen Host verhindern die Aktivierung. Die gesonderte Liveübergabe muss eine eigene erlaubte Kopie, konkret ausführbare Livehost-Builds und neue Hash-/Profil-/Adapterbindungen herstellen und prüfen; gesperrte Originale unverändert lassen. Ein Dateiproduzentenprogramm ersetzt keinen Livehost. Keine Sperre entfernen oder Freigabe aus diesem Review ableiten.

**Aktueller Speicherblocker:** Das geplante Ziel verlangt mindestens **80 GiB freien Platz je betroffenem Ausgabedateisystem**. Vor Review 28,29 GiB frei; während/nach den eigenen erhaltenen synthetischen Prüfkopien rund 25–27 GiB. Damit würde die vorhandene Startschranke vor Geräte-I/O abbrechen. Geeignetes Ziel mit mindestens 80 GiB frei bereitstellen, gegebenenfalls externe SSD, und alle zugehörigen Aufnahme-/Kopie-/Normalisierungs-/Auswertungspfade konsistent neu binden. Keine Löschung von Backups/Rohdaten oder stille Absenkung der Grenze. Neu prüfen unmittelbar vor Start.

Hardware: TX1 und RX1–RX4 an festen normalen Positionen mit stabiler Stromversorgung; Mac im bestehenden Projekt-WLAN, für den langen Lauf mit Stromversorgung. Kein COM-/Mac-USB-Datenkabel zu den ESPs erforderlich, keine Radare für diesen reinen Funkvergleich. Eine externe SSD benötigt gegebenenfalls ihren eigenen Datenanschluss.

Nach geklärtem Speicherziel den begrenzten Hardwareauftrag samt geprüfter Liveübergabe und frischen Geräte-/Host-/Geometriebindungen konkretisieren. Neue getrennte Freigaben für RX1-/RX4-Unterbrechung und Appschreiben sowie Hostunterbrechung/-aktivierung und die benannten Lese-/Captureaktionen bleiben erforderlich. TX1/RX2/RX3 unverändert; kein zusätzlicher RX2/RX3-Rollout.

Ein späterer Basis→Paarlauf ändert gleichzeitig Firmware und Peerbetrieb. Er kann einen realen kombinierten Funk-/Transportbefund liefern, **keinen isolierten Ratensperrengewinn**. Firmwarefrische pro Paket, RX2/RX3-Legacyattestation, vollständige Ressourcenbilanz und Positionsnutzen bleiben eigene offene Nachweise. Der 14. Oktober bleibt **2/5**; keine neue Liveabnahme oder Tageshäkchen aus diesem Offlineabschluss.

**Schwierigkeit des nächsten Gesamtschritts: 4/5**, einschließlich bisheriger Versuche, zwei Installationen, Hostwechseln und begrenztem Ende. Keine aktuelle Freigabe und kein Hardwarestart durch diesen Review.

**Nachfolgende Speicherprüfung auf Johanns Nachfrage:** Die [rein lesende Analyse](rx-speicheranalyse-offline-20261015.md) präzisiert die obige Startbedingung. 80 GiB ist ein pauschaler Guard der aktuellen Version, kein belegter unvermeidbarer Messbedarf. Die fünf genannten großen Einträge sind rund 118 GB ausgenommene Buildcaches; der vollständige hohe synthetische Messlauf umfasst 4,54 GB einschließlich Kopien und doppeltem Bericht. Künftige kanonische Ablage vermeidet darin 1,87 GB identische Exportkopien; sieben komplette Originaldateien bestehen außerdem einen verlustfreien gzip-Roundtrip (1,99 GB auf 80,6 MB). Keine Löschung oder Änderung der bestehenden Lieferung. Vor einer SSD-Beschaffung zuerst eine kleinere konsistent hergeleitete Ablage-/Startbudgetvariante offline qualifizieren; die jetzige Schranke bleibt bis dahin technisch aktiv.

# Koordinatorreview: korrigierte C1-Paarbasis vom 15. Oktober

**Die bekannten Softwareblocker sind offline geschlossen. Die neueste Lieferung ist technisch zur Vorbereitung und Freigabeprüfung eines begrenzten Hardwareauftrags geeignet.** Noch kein realer Peer-Funktest, keine Installation oder Hostaktivierung. Vor Livebeginn fehlt die konkrete I/O-Adapteranbindung an den geprüften Ablaufkern; sie gehört als kleiner Offline-Vorlauf in denselben späteren Hardwareauftrag.

Maßgebliche Lieferung: [Abschlussbericht vom 15. Oktober](../../private/peer-c1-review-korrektur-offline-20261015T050734Z/REPORT.private.md), Quellen und manifestegebundene Kandidaten. Diese Lieferung korrigiert die ursprünglichen B1–B4 sowie vier anschließend direkt bestätigte Reviewbefunde zu Legacyaufnahme, laufender Hostfrist, verspätetem Abbruchdispatch und unvollständigen Erwartungen. Direkte Nutzeraufträge und beide Abschlussstufen im Chat „Peer-C1-Blocker offline beheben“ wurden gelesen; der neueste korrigierte Graph ist maßgeblich.

Dieser Koordinatorreview erfolgte ausschließlich lokal. Keine Geräte-/Netzabfrage, kein serieller Zugriff, Reset, Capture, Upload, Hostwechsel oder Agentenstart. Clientdatum 15. Oktober 2026, Europe/Berlin.

## Inhaltliche Ergebnisse

| Bereich | Offlineurteil | Geprüfte Wirkung |
|---|---|---|
| B1 Senderprofil | **GESCHLOSSEN** | Feste öffentliche OFDMrate 6 Mbit/s nur im Paarprofil, tatsächliche Init/Rate/Startreihenfolge und Fehlerabbruch; Default ohne Ratensetzung/Sender |
| B2 Layout-Rückkehr | **GESCHLOSSEN** | Zulässige 128→64→128-Rückkehr ohne dauerhafte Pensionierung; alte Identitäten bleiben gesperrt; Legacyaufnahme erhält gültige diagnostische Raster ohne Featuregutschrift |
| B3 Sendefrische | **GESCHLOSSEN** | Nach Backoff Prüfung direkt vor Übergabe; bei 95+10 ms kein zweiter Send, 100-ms-Grenze und begrenztes Firmwareende geprüft |
| B4 Gemeinsame Maske | **GESCHLOSSEN** | Vorhandene Kalibrier-/Live-/Replayeingänge tragen physische Zulassung und Maske; ungültige Bins verändern Referenz/RMS/Features nicht |
| VALID/Restbudget/Abbruch | **READY_OFFLINE** | Vollständige eingefrorene Erwartungen vor erster Aktion, aktive Hostdeadline in Restprojektion, keine Endaktionen nach Ablauf und kein Wiederholungs-Hostwechsel |
| Peer-Endzustand | **READY_OFFLINE** | Firmware endet nach 3.000 s ab Boot, Hostannahme nach 1.200 s; Endklammern und Wartezeit Bestandteil des Plans |
| Reale Fehler-/Ressourcenbilanz | **TEILWEISE** | Gespeicherte Beobachtungen erhalten; fehlende reale TX-/Socket-/Interface-/CPU-/Heap-/Queuewerte weiterhin unbekannt |

Die neue Legacyregression verwendet tatsächlichen Parser, Grid-Gate, gemeinsamen Ingress und Datei-Writer mit 520 synthetischen Frames über vier RX und 128→64→128 auf den Legacykontrollen. Diagnostische Aufnahme bricht nicht ab; daraus entsteht keine erfundene physische Form oder Kalibrier-/Livegutschrift. Versiegelte Aufnahme und Quellen-/Radio-/Zeit-/Speichergrenzen bleiben geprüft.

Die drei neueren Ablaufgegenfälle wurden zusätzlich direkt wiederholt: Eine 400-s-Pause sperrt den Paarcapture vor Sessionverbrauch; bei Warten bis Sekunde 6.001 keine End-GETs oder späteren Dispatches; leere Kandidatenerwartungen erzeugen kontrollierten Abbruch vor allen Aktionen. Die vollständigen 34 Tests decken außerdem Gleichheitsgrenzen, Kostenreserve, Antwort-/Adapterfehler und Endzustände ab.

## Unabhängig wiederholte Prüfung

- Alle **4.638 Manifestdateien**, **vier Symlinks**, drei Vorher-/Nachherquellgraphen samt Patches und **24.894 erhaltene Ursprungsdateien** nach Hash, Länge und zutreffendem Modus geprüft. Keine Abweichung. Gitstatus vor den Koordinator-Dokumentationsänderungen stimmt mit dem eingefrorenen Lieferstand überein.
- Drei S3-BIN-/ELF-/Configbindungen samt unabhängig berechneten Runtimefootern geprüft: Default 1.238.784 Byte, RX1/RX4-Paar je 1.271.184 Byte, jeweils unter der gespeicherten 2-MiB-Referenz. Firmwarequellen sind zur vorherigen Korrekturlieferung bytegleich. Deren bereits frisch gebaute S3-Kandidaten wurden korrekt unverändert übernommen; kein erneuter SDK-Neubau behauptet. Beide tatsächlich betroffenen Hostbinarys sind neu gebaut und gebunden.
- Manifestbindungen von Hardwareplan, Sessionvertrag, Sourcegraph, Kandidaten und finalem Gegencheck stimmen. Gespeicherte Hauptlogs belegen jeweils **549 PASS, null FAIL, fünf ignored, sieben gefilterte Socketfixtures**. Die 15 separaten erfolgreichen Receipts enthalten sieben Socketfixtures pro Variante und einen expliziten Interopfall; die Gesamtzahlen 556/557 sind konsistent. Vier andere private CAD-/Seal-/Specfixturetests bleiben offen.
- Den tatsächlichen Pythonablaufkern unter der gelieferten engen Leseliste und gesperrten realen I/O-Einstiegen erneut ausgeführt: **34/34 PASS**, dazu die drei direkten finalen Gegenchecks bestanden. Originalskripte nicht als schreibende Gesamteinstiege ausgeführt.
- Sechs native Programme in einem neuen temporären Verzeichnis frisch mit ASan/UBSan kompiliert und ausgeführt: **120 Peerchecks**, **136 Alters-/Endzustandsassertions**, tatsächlicher Start, Default sowie eigene/AP-Partnerablehnung bestanden. Produktionseingänge laufen mit synthetischen SDK-/Uhrdoubles, keine Hardware.
- Finale ausgelieferte Rusttestausführung in temporärer Binarykopie wiederholt: **zwei Legacyingress-, ein versiegelter Legacyaufnahme-, fünf physische Layout-/Masken-/Replay- und ein Hostendtest** bestanden. Zusätzlich **ein C→Rust-Interoptest** mit vier durch die frisch kompilierten C-Harnesses erzeugten LLTF64-/HT128-Paketen in beiden Richtungen bestanden. Diese zehn gezielten Tests werden nicht zu den gespeicherten Vollsuitezahlen addiert.

Alle wiederholten Native-/Rusttests liefen mit wirksamem Prozess-Netzverbot. Temporäre Testdateien enthalten ausschließlich synthetische Daten; Originalquellen, Binarys, Logs und Manifeste wurden nicht verändert. Keine neue vollständige Rust-/SDK-Suite in diesem Review behauptet.

## Hardwareauftrag und noch nötige Vorbereitung

Der neue gesperrte Hardwareplan ist eine belastbare Ablaufgrundlage, aber noch kein ausführbarer Realadapter. `session_core.Session.execute()` ist injizierbar und geprüft; `live_entry()` bleibt ausdrücklich gesperrt. **Vor Livebeginn die vorhandenen begrenzten HTTP-/OTA-/Capture-/Hostwerkzeuge gezielt an genau diesen Kern anbinden, mit Fake-I/O prüfen und gemeinsam versiegeln.** Keine neue allgemeine Firmwareentwicklung daraus ableiten. Dieser Vorlauf kann im selben Folgeauftrag vollständig offline erfolgen; erst nach erfolgreichem Adaptercheck und passenden Freigaben darf dessen Livephase beginnen.

Feste Liveplanung: höchstens **100 Minuten** einschließlich letzter zehn Minuten Endreserve; **acht Minuten Basis und 16 Minuten Paarvergleich**, genau zwei Aufnahmesessions und je ein OTA-POST für RX1/RX4. Maximal 120 GETs, davon zwölf fürs Ende reserviert; normal geplant 42. Drei begrenzte Host-/Profilwechsel: Basis, Paar, Rückkehr zur Basis. Konservative Gesamtprojektion **4.660 s = 77 min 40 s**, aktive Host-/Gerätefristen begrenzen zusätzliche Verzögerungen. Neue passende RX1-/RX4-Unterbrechungs-/Schreib- sowie Hostunterbrechungs-/Aktivierungsfreigaben erforderlich; keine davon wird hier erteilt.

Die Abnahme prüft beide realen Richtungen getrennt: mindestens 1.920 gültige gebundene Pakete je Richtung und mindestens 2 Hz; die vier TX1-Basislinks behalten die vorhandenen Qualitätsgrenzen. Gesunde neue VALID-Apps werden bei schlechter Peerquote nicht spontan restauriert. Nach Ende kein unbegrenzter Peer-Senderbetrieb: gebundenes Firmware-/Hostende und tatsächliche Endbelege sind Pflicht.

Benötigte Hardware für diesen späteren Lauf: **TX1 und RX1–RX4 an normalen Positionen mit stabiler Stromversorgung, Mac im bisherigen Projekt-WLAN/Hostweg.** Im geplanten OTA-Standardablauf kein serieller Zugriff; RX1-/RX4-COMwege können als vorhandene, gesondert zu regelnde Diagnoseressource bereitliegen. Radare werden für den reinen Linktest nicht benötigt. Keine heutigen Online-/Slot-/Boot-/Configzustände aus dem gestrigen Endbeleg ableiten; spätere frische Preflightbindungen bleiben zwingend.

## Projektgrenze und nächster Schritt

Historischer Vier-RX-CSI-Teilbefund weiterhin **1.786/1.793 = 99,61 %**, streng **1.780/1.793**. Er wird nicht als neuer Hardwarelauf oder physische Positionsabnahme ausgegeben. RX2/RX3 besitzen aus den gespeicherten Quellen keine neue Layoutattestation; diagnostische Lesbarkeit ist keine Vier-RX-Physicalkalibrierung. Für spätere Positionsdatensammlung müssen alle nötigen physischen Eingänge, Geometrie und Radarreferenz gesondert qualifiziert sein. Gesamt-G1, echte Peerqualität, Ressourcen- und Trackingabnahme bleiben offen.

**Nächster konkreter Schritt:** den begrenzten Hardwareauftrag für RX1/RX4 erstellen, mit kleinem abgeschlossenen Offline-Vorlauf zur Realadapteranbindung, Fake-I/O-Test und Versiegelung. Danach erst die konkret freigegebene Livephase. Kein weiteres allgemeines B1–B4-Korrekturpaket ist durch die geprüften Befunde begründet.

Die alten Tageshäkchen werden hier nicht erweitert: `data-rules` enthält noch Live-/Vier-RX-Bindungen, `data-accounting` die fehlende reale Bilanz, `peer-trial` den fehlenden Realadapter und die neuen Freigaben; echter Paartest bleibt ungemessen. **Schwierigkeit des nächsten Gesamtlaufs: 4/5**, einschließlich bisheriger Versuche, zwei OTA-Vorgängen, Hostwechseln und begrenztem Betriebsende. Kein Agent gestartet.

# Offline-Speicherprüfung: Buildcaches, Laufdaten und die 80-GiB-Schranke

**Nein, die fünf genannten Größen sind kein notwendiger Umfang der Messergebnisse. Sie entsprechen praktisch exakt den jeweiligen Rust-Buildcaches. Die 80 GiB sind eine pauschal implementierte Startschranke, kein nachgewiesener Mindestbedarf des Messverfahrens.** Die vorherige Empfehlung einer externen SSD allein wegen dieser Zahl war deshalb verfrüht; vor einer Beschaffung zuerst die Ablage und den daraus abgeleiteten Startetat verbessern.

Stand: 15. Oktober 2026, Europe/Berlin. Ausschließlich lokale Dateiprüfung; keine Hardware, Agenten, Löschungen, Verschiebungen, Builds oder Hostaktivierung. Ursprungsdaten, versiegelte Lieferungen und aktuelle Startschranke unverändert. [Private Prüfzahlen](../../private/rx-speicheranalyse-offline-20261015/FINDINGS.private.json).

## Was die fünf großen Einträge enthält

GB bezeichnet hier dezimale Größen wie in Johanns Liste; GiB wird für die binären Grenzen des Codes verwendet.

| Lieferung | Großer Unterordner | Gemeldete Cachebelegung | Für Auswertung/Installation maßgebliche Belege |
|---|---|---:|---|
| RX-Budget-Vorklammern | `build-cache` | **34,95 GB** | außerhalb des Caches: App-/ELF-/Configkandidaten, Host-/Datei-/Testbinarys, Quellen, Receipts, Manifeste und ausgewählte Testbelege |
| RX-Radar-Export-Korrektur | `build-cache` | **27,37 GB** | finale Host-/Dateiprogramme, Quellen, Exporte, Regressionen und Originalbelege |
| Abendläufe-Auswertung | `build-cache` | **21,41 GB** | Datei-Auswerter, Quellen, Eingangsverträge, Gegenbeispiele und Reports |
| Offline-Tagespaket | `host-target` | **19,16 GB** | Host-/Firmwarekandidaten, Quellen, Adapter-/Radar-/Metadatennachweise |
| Peer-C1-Review-Korrektur | `host-target` | **15,47 GB** | finale Host-/Firmwarekandidaten, Quellen, Ablauf-/Peerregressionen und Bindungen |
| **Summe** | | **118,37 GB** | **kein Messdaten-Mindestbedarf** |

Das sind rund **80,5 %** der gemeldeten Belegung aller fünf vollständigen Lieferordner. Alle fünf autoritativen Manifeste schließen diese Caches ausdrücklich aus; darin ist kein maßgeblicher Manifestdateieintrag gebunden. Finale App-/Host-/Auswerteprogramme liegen separat. Die Mach-O-Ladelisten von elf vorhandenen Host-/Auswertebinarys enthalten keine direkte `build-cache`-/`host-target`-Referenz. Die aktuelle Lieferung hält auch ihre beiden Host-Testprogramme separat unter `bin/` vor.

Caches sparen spätere Kompilierungszeit, sind aber keine zu vervielfältigenden Auslieferungsartefakte. Ein Cache kann für zukünftige Builds gezielt wiederverwendet werden. Das ist **keine Freigabe zum Löschen**: vor einer später autorisierten Bereinigung exakte gebundene Artefakte, Testbinarys, Buildargumente und externe Pfadbezüge erhalten. Ganze Lieferordner sind keine Cacheordner. APFS kann Blöcke teilen; die addierte Anzeige beweist daher keinen gleich großen Gewinn an freiem Platz.

## Wie groß der vollständige Lastlauf wirklich ist

Gemessen wurde der bereits versiegelte hohe synthetische Lastlauf mit 480 s Basis und 960 s Paar, 200 CSI-Datagrammen/s plus Syncverkehr. Keine neue Simulation oder große Prüfkopie erzeugt.

| Bestandteil | Logische Dateigröße |
|---|---:|
| Beide Phasen mit PCAP, Trace, Raw und Peer im Export | **1,989 GB** |
| Zusätzliche identische Ursprungsdateien in den Host-/Writerablagen | **1,866 GB** |
| Normalisierte Beobachtungen und Metadaten | **0,492 GB** |
| Auswertung und deterministische Wiederholung | zusammen **0,194 GB** |
| **Vollständiger gespeicherter Lasttest** | **4,541 GB** |

Die oft genannten **695 MB sind nur der Paar-Trace**, nicht der komplette Lauf. Die fünf großen Quelldatei-/Exportpaare sind über identische versiegelte Hashes und Größen gebunden. Würden zukünftige Writer von Anfang an in eine kanonische endgültige Ablage schreiben und alle Rollen dieselbe geschlossene Datei über Hash/Bezug nutzen, entfielen in genau diesem Beispiel rund 1,866 GB zusätzliche Kopien. Rechnerischer Umfang dann etwa **2,676 GB**, weiterhin mit beiden Phasen, allen Originalinformationen und beiden Auswertungen. Das ist eine nachgewiesene Bytegleichheit und ein Ablagevorschlag, **noch keine implementierte neue Adapterversion**.

## Verlustfreie Kompression vollständig geprüft

Sieben zentrale Originalbelegdateien beider Phasen (Trace, Raw, Peer und PCAP) wurden vollständig blockweise mit gzip Stufe 1 komprimiert und sofort wieder dekomprimiert. Originalbytezahl, entpackte Bytezahl und SHA-256 stimmen in allen sieben Fällen mit den versiegelten Ausgangsbelegen überein. Begrenzter Arbeitsspeicher; keine Archive geschrieben und keine Quelldateien ersetzt.

**1.989.042.698 Byte → 80.612.439 Byte**, rund **95,95 %** weniger für diese Dateien. Sämtliche IQ-Werte, Paketbytes, Zeit-/Statusinformationen und Fehlbelege bleiben beim Roundtrip erhalten. Diese Quote gilt für synthetische Testdaten; reale IQ-Daten können schlechter komprimierbar sein. Eine kleinere garantierte Startreserve darf nicht aus diesem Mittelwert abgeleitet werden.

Der heutige Live-/Auswertepfad liest Klartextdateien. Komprimierte kanonische Belege erfordern einen explizit geprüften Dateivertrag: Hash des komprimierten Artefakts **und** bisheriger Originalbytehash/Originalgröße/Zeilenzahl; vollständige Hashprüfung nach dem Entpacken; streamende Parser; Ablehnung von Beschädigung/Abschneiden. Bloßes Umbenennen in `.gz` oder spätere Löschung der Originale wäre keine fertige Lösung. Bestehende Belege bleiben erhalten.

## Warum die 80 GiB zu pauschal sind

Der Adapter prüft bei `controls` wortwörtlich `free_bytes(path) >= 80 * 1024**3` für Ausgabeordner und Hostarbeitsordner. Er berechnet keinen Speicherplan aus tatsächlichen Rollen, Phasen oder Dateisystemzuordnung. Bei einem gemeinsamen Dateisystem prüft er dieselbe freie Menge zweimal; dies reserviert nicht zweimal 80 GiB. Bei getrennten Dateisystemen fordert er unabhängig vom jeweiligen Anteil je 80 GiB.

Mit den heutigen unveränderten **Dateiobergrenzen** können beide Phasen zusammen bis zu 26,5 GiB an ursprünglichen Trace-/Raw-/Peer-/PCAPdateien belegen. Die zusätzliche Archivkopie der Host-/Raw-/Peerdateien kann weitere 24,5 GiB belegen: **51 GiB vor Normalisierung und Berichten**. Das erklärt einen großzügigen Sicherheitsaufschlag, ist aber weder der beobachtete Verbrauch noch eine Herleitung genau der Zahl 80.

Die generische 16-KiB-Datensatzgrenze ist ebenfalls eine Obergrenze, keine typische Zeilengröße; der geprüfte Paar-Trace schrieb tatsächlich rund 695 MB. Gleichwohl nicht einfach den Guard auf 1–5 GiB setzen: Dateigrenzen, abweichende Datenformen, Fehlerverkehr, Ausgabeexpansion und behaltene Originale müssten weiter korrekt budgetiert sein. Keine Messung verkürzen, IQ reduzieren, Pakete samplen, Gegenbeweise weglassen oder Fehler zu Erfolg umdeuten.

## Konkrete Reihenfolge für eine kleinere neue Vorbereitung

1. **Neue Liefer-/Livekopien nur aus einer festen Artefaktliste erstellen.** Quellen, finale App-/Host-/Dateiprogramme, Config/Manifest/Logs und notwendige Eingänge übernehmen beziehungsweise unveränderlich referenzieren; `build-cache`, `host-target` und vollständige frühere Testläufe nicht in jede neue Aufgabe kopieren. Einen bestehenden passenden Buildcache gezielt wiederverwenden; neue finale Programme vor weiterer Cachebenutzung separat sichern und binden.
2. **Künftige Aufnahmen kanonisch ablegen.** Originalwriter und Exportrollen auf dieselben exklusiven Sessiondateien binden; den Hash-/Vollständigkeits-/Boot-/Quellen-/Recordervertrag erhalten. Keine existierenden Dateien verschieben oder entfernen. Fehlende, veränderliche oder falsch referenzierte Originale müssen weiterhin scheitern. Das spart im geprüften Lauf nachweislich 1,866 GB, ohne ein Paket auszulassen, und vermeidet Kopierarbeit innerhalb der knappen Capture-Endfrist.
3. **Speicherplan je tatsächlichem Dateisystem berechnen.** Behaltene Basis, Paar, Trace/Raw/PCAP/Peer, Normalisierung, Berichte, temporäre Dateien und endliche Reserve zählen; gemeinsame Dateisysteme zusammenfassen und unveränderte vorhandene Inputs/Caches nicht erneut als neue Aufnahmebytes ansetzen. Vor erster Geräteaktion und vor jeder Phase prüfen. Die heute unveränderten Grenzen nicht still absenken. Erst nach deren konsistenter Umsetzung und Grenz-/Fehler-/Datei-Endtests einen neuen kleineren Mindestwert festlegen.
4. **Kompression als zusätzlich geprüfte Option.** Zuerst kanonische Ablage und Cachekopien lösen; die gemessene starke Kompression ersetzt keine Worst-case-Rechnung. Für spätere Archive Dateileser und Integritätsbelege gemeinsam anpassen, vorhandene Originale erhalten und End-/Laufzeitbudgets unverändert prüfen.

Ergebnis der beauftragten Prüfung: **verlustfreie Reduktion ist möglich und anhand realer gespeicherter synthetischer Dateien belegt.** Es ist kein Belegverlust erforderlich, um die künftige Ablage erheblich kleiner zu machen. Die derzeitige 80-GiB-Startschranke wurde nicht geändert; keine neue kleinere Liveversion oder Hardwarefreigabe behauptet. Der nächste sinnvolle Schritt ist die begrenzte Offline-Umsetzung dieser Ablage-/Budgetänderung, nicht automatisch eine neue SSD oder das Löschen alter Belege. **Schwierigkeit: 3/5**; echte Originalintegrität und der knappe Captureabschluss bleiben zu beachten.

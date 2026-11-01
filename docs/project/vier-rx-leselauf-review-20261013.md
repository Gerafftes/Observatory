# Koordinatorreview: 30-Minuten-Leselauf vom 13. Oktober

**G1 bleibt offen und ist nicht bestanden. RX2 läuft weiterhin auf C / ota_0 / VALID.** Die Aufnahme ist vollständig, enthält aber ausschließlich RX2. RX1, RX3 und RX4 wurden erst nach Aufnahmeende an ihren aktuellen, anhand bekannter MACs zugeordneten Endpunkten bestätigt. Ihre CSI-Raten sind nicht gemessen.

Grundlage: [Abschlussbericht und private Originalbelege](../../private/tx1-vier-rx-30min-20261013-20261013T154314Z/REPORT.private.md). Alle 374 Dateien des Abschlussmanifests stimmen hinsichtlich Größe und SHA-256. Die acht ursprünglichen Antwort-Receipts und drei zusätzlichen Statusantworten stimmen mit ihren Originalbodys überein. RX2s drei Statusoriginale bestätigen dasselbe C-Image, denselben Boot und ota_0 / VALID; beide Konfigurationsoriginale enthalten dieselben acht Werte. Die neuen Gegenrechnungen dieses Reviews wurden ausschließlich lesend im Speicher ausgeführt; Originale und ursprüngliches Urteil bleiben erhalten.

| Empfänger | CSI-Nachweis über 30 Minuten | Zustand am Abschluss |
|---|---|---|
| RX1 | Nicht aufgenommen; Rate und Fenster unbekannt | Bisherige e09d-App, ota_1 / VALID; ein Status nach Capture |
| RX2 | 54.101 passende 128er-Frames, 30,056/s; 1.487/1.793 Fenster = 82,93 % nach bisheriger Regel | C, ota_0 / VALID; Boot und acht Konfigurationswerte unverändert |
| RX3 | Nicht aufgenommen; Rate und Fenster unbekannt | Bisherige e09d-App, ota_1 / VALID; ein Status nach Capture |
| RX4 | Nicht aufgenommen; Rate und Fenster unbekannt | Bisherige e09d-App, ota_1 / VALID; ein Status nach Capture |

## Zwei getrennte Blocker

**Aufnahmezulassung:** Die historischen Endpunkte waren teilweise vertauscht beziehungsweise veraltet. Der ausgeführte Filter enthielt nur einen Empfänger. Die erlaubte Teilaufnahme war für das Ziel einer gemeinsamen Vier-RX-Abnahme nicht ausreichend. Die inzwischen bestätigte private Endpunktdatei ist ein neuer Startpunkt, keine nachträgliche Messgutschrift. Ein neuer Lauf muss vor dem Start alle vier erwarteten MACs, Antwortidentitäten und tatsächlichen Zieltuplen bestätigen und genau diese vier RX im Filter enthalten. Andernfalls vor dem langen Capture stoppen.

**Formatauswahl:** Zusätzlich zu den passenden Frames kamen 208 gültige 64er-Frames an, 0,383 % der gültigen CSI. Alle 306 abgelehnten RX2-Fenster haben ausschließlich den Grund `non_matching_grid_observed`. Die aktuelle Regel verwirft ein ganzes Fenster, sobald ein solches Fremdraster darin vorkommt. Ein einzelner Frame kann mehrere überlappende Fenster betreffen.

Die unabhängige Offline-Gegenrechnung mit der gespeicherten Paketmetadatenfolge reproduziert exakt 1.487/1.793 und diese 306 Ablehnungsgründe. Wird ausschließlich die bereits als passend gebundene 128er-Teilfolge an dieselbe Fensterfunktion übergeben, bestehen **1.793/1.793** die verbleibenden Kriterien: unveränderte Identitäts-/Bootbindung, mindestens 15 Frames, mindestens 2,5 s Abdeckung, mindestens 5 Hz und keine innere Lücke ab einer Sekunde. Dies ist eine diagnostische Gegenrechnung, kein neues PASS und keine vollständige physische Layout-/Maskenattestierung. Eine neue Auswahlregel muss vor ihrer Anwendung ausdrücklich spezifiziert und getestet werden; weder 64er-Frames umdeuten noch Grenzen nachträglich lockern.

Die 267 ungeklärten Sequenzpositionen bleiben gesondert bilanziert. Sie beweisen keine Funkverlustursache. Formatausschlüsse bleiben gültiger Transport und gehören in die vollständige Empfangs-/Sequenzbilanz, auch wenn sie keine Positionsfeatures liefern.

## Geschlossener Teilpunkt und offene Systemnachweise

Das Capture-Ende ist in diesem Lauf beobachtbar: zwei Sekunden Nachlauf, 58 echte Records nach der Endmarke, letzter RX2-Rasterframe 30,747 ms vor der Endmarke. Das letzte Dreisekundenfenster hat 2,934232 s Rasterabdeckung und scheitert ausschließlich an der Formatbeimischung. Der frühere Endkanten-FAIL bleibt historisch unverändert; seine Ursache ist damit nicht rückwirkend bewiesen.

RX2 und TX1 standen laut direktem Nutzerhinweis am Wartungsplatz. Vor einer G1-Abnahme für den vorgesehenen Aufbau sind ihre tatsächlichen Positionen mit dem Aufbauprofil abzugleichen. Dieser Review verändert weder Geräte noch Profil. Serverannahme, physischer LTF-/Bin-/Maskenbezug, Ressourcenreserve und frische TX-Fehlerbilanz bleiben eigene offene Nachweise. RX↔RX und Radarreferenz wurden nicht geprüft.

## Nächster konkreter Schritt

Ein gebündelter Offline-Auftrag soll zuerst die exakte Layoutauswahl zwischen Aufnahme, Fensterprüfung und Live-Pfad prüfen und einen minimalen, getesteten Auswahlvertrag liefern. Dabei alle Rohframes und Sequenzkonten erhalten, nur vollständig passende Frames für Features verwenden und Boot-/Quellen-/Epochenwechsel weiterhin trennen. Wenn eine physische Format-/Identitätsänderung vorliegt, darf sie nicht als harmloser Ausschluss verschwinden. In derselben Vorbereitung den Vier-RX-Preflight so absichern, dass ein Teilfilter nicht erneut 30 Minuten Messzeit verbraucht. Keine Hardwarezugriffe, Installation oder Dienstaktivierung aus diesem Review.

Danach genau einen neu freigegebenen gemeinsamen Vier-RX-Lauf am vorgesehenen Aufbau durchführen. C bleibt auf RX2. Ein erneutes Flashen ist aus den vorliegenden Befunden nicht begründet. G1 erfordert weiterhin alle vier RX gleichzeitig und mindestens 90 % gültige gemeinsame Fenster; die Diagnosegegenrechnung ersetzt diesen Nachweis nicht.

**Schwierigkeit: 4/5** für die nächste gebündelte Klärung und Absicherung. Die hohe Bewertung berücksichtigt die wiederholten Teilversuche und die erforderliche Übereinstimmung von Formatvertrag, Live-/Offlineauswertung und Vier-RX-Aufnahme. Der RX2-Boot-/Installationszustand ist dabei bereits geklärt.

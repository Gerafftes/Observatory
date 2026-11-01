# Koordinatorreview: Abschluss der drei Tag-2-Aufgaben

**Tag 2: 7/7 Aufgaben abgeschlossen. G1: nicht bestanden.** Geräteidentitäten und gemeinsame Diagnose sind mit aktuellen Laufbelegen abgeschlossen; das RX1↔RX4-Profil ist tatsächlich offline implementiert, getestet und gebaut. Peerinstallation, beide realen Funkrichtungen und Positionsnutzen bleiben offen.

Grundlage: [Abschlussbericht](../../private/tag2-abschluss-vier-rx-peer-20261013T184515Z/REPORT.private.md), Original-PCAP, ausgeführte Helfer, Antwortreceipts und private Quell-/Buildlieferung. Keine neuen Hardwarezugriffe durch diesen Review.

## Drei getrennte Abschlüsse

| Trackerpunkt | Urteil | Beleg und Grenze |
|---|---|---|
| `identities` | ABGESCHLOSSEN | Vier aktuelle MAC-/Antwort-/Image-/Boot-/Configbindungen, konsistente Endprüfung; alle Images VALID. RX2 weiterhin C / ota_0. RX1/RX3/RX4 mit bisheriger App / ota_1. Acht Werte je RX typgenau unverändert; bei RX1/RX3/RX4 gilt die frische Startreferenz dieses Laufs, keine behauptete historische Gleichheit. |
| `joint-diagnostic` | ABGESCHLOSSEN mit Qualitätsfinding | Vollständiger gleichzeitiger Vier-RX-Capture und Auswertung, vollständiger Filter, 22 GETs, ein Capture, 30,131 Minuten aktiver Abschnitt. Diagnoseabschluss bedeutet kein bestandenes Datenfenstergate. |
| `peer-implementation` | ABGESCHLOSSEN, offline | Konkreter begrenzter Peer-Sender/Empfänger, versionierter Hostpfad, Quoten und Poolgrenzen implementiert; C/Rust-Interop, Defaultisolation und drei S3-Builds belegt. Keine Installation, Aktivierung oder tatsächliche Peerfunkbeobachtung. |

Geräteangaben gelten für das Laufende am 13. Oktober um etwa 21:29 Uhr Europe/Berlin. Anschließend war laut Abschlussverlauf das Abstecken der Hardware erlaubt; der Review behauptet keinen jetzigen Onlinezustand und verlangt keinen Reset.

## Gemeinsames Datenfenstergate

| RX | Gültige CSI | Passende 128er | Passende Frames/s | Eigene gültige Fenster | Größte innere Rasterlücke |
|---|---:|---:|---:|---:|---:|
| RX1 | 32.817 | 7.212 | 4,007 | 302/1.793 | 2,925 s |
| RX2 | 60.081 | 52.138 | 28,965 | 1.793/1.793 | 0,335 s |
| RX3 | 34.103 | 7.026 | 3,903 | 449/1.793 | 3,846 s |
| RX4 | 24.531 | 5.953 | 3,307 | 323/1.793 | 6,640 s |

**Gemeinsam: 97/1.793 = 5,41 %, erforderlich mindestens 1.614/1.793.** RX1/RX3/RX4 liefern gültigen Transport, aber überwiegend 64-/192er-Formate und insgesamt weniger als fünf passende 128er-Frames pro Sekunde. RX2 auf C besteht alle eigenen Auswahlfenster. Die genaue Ursache der Formatverteilung bei den drei bisherigen Apps ist damit eingegrenzt, aber nicht vollständig bewiesen.

Die gültigen Formatausschlüsse und die ungeklärten Sequenzpositionen bleiben getrennt. Kein Ausfall aller drei Empfänger und keine bewiesene WLAN-Verlustursache. Der neue Lauf hat vollständige Endabdeckung mit 184 echten Nach-Endmarkenrecords und null Kernel-Drops; Interface-Drops unbekannt. Serverzustellung, physische Masken, TX-Fehler-/Ressourcen-/Geometrienachweise und Positionsleistung sind keine durch diesen PCAP erfüllten Nachweise.

## Unabhängige Prüfung

- Alle **21.265 Manifestdateien** hinsichtlich Größe und SHA-256 sowie alle 24 Bindungen des Live-Ausführungssiegels erneut geprüft. Originale unverändert.
- Original-PCAP mit ausgewähltem und altem strengem Vertrag neu dekodiert; beide Summaries exakt reproduziert. Die tatsächlichen Endpunkte stammen dabei aus der ausgeführten Capture-Konfiguration und stimmen mit der Lauf-Inventur überein. Die vorbereitenden historischen Referenzadressen weichen bei RX1/RX3/RX4 ab und sind keine Ausführungsautorität.
- Alle 22 HTTP-Originalbodys stimmen mit ihren Receipts; zwölf Statusantworten, acht Configantworten und zwei RX2-Diagnosen. Keine Geräteanfrage während des Capture-Lebenszyklus. Image-, Boot- und typgenaue Configkontinuität bestätigt.
- Alle **75 Python-Tests** unter Offline-I/O-Schutz erneut bestanden.
- Gebundene native Testbinarys erneut ausgeführt und anschließend auch der finale Quellstand mit den gelieferten Argumenten in neuen privaten temporären Verzeichnissen kompiliert/getestet: **141 Peerchecks plus je sechs Vorher-/Defaultkontrollen**, null Fehler. Alle drei Default-TX1-Kontrollframes byteidentisch. Der Agentenbericht nennt 143; die tatsächlichen Receipts und beide Wiederholungen weisen 141 für den Peerstand aus. Dies ist eine korrigierte Zählerangabe, kein fehlgeschlagener Test. Keine alten Test-/Buildartefakte überschrieben.
- Gehashte Rustlogs: Default und Peer jeweils 1.062 bestanden, sieben ignoriert; der C/Rust-Interopfall anschließend separat bestanden. Vollständige Rust-/SDK-Builds im Koordinatorreview nicht erneut ausgeführt. Drei gebundene ESP32-S3-Builds und zwei getrennte Hostbuilds liegen vor; der Arbeitsbaum enthält weiterhin den geschützten bisherigen Stand, neue Peerquellen liegen privat.

Ein beim ersten SDK-Build versuchter CMake-Download verletzt die vorgesehene Offlinegrenze; er scheiterte mit null Byte und ohne Installation. Der Agent hat den Pfad danach gesperrt und die finalen SDK-Builds zusätzlich mit OS-Netzverbot durchgeführt. Diese dokumentierte Abweichung wird nicht als Geräteaktion oder erfolgreiche Installation ausgegeben und nicht aus dem Bericht entfernt.

## Nächster konkreter Schritt

**Die Formatwahl von RX1/RX3/RX4 gezielt offline gegen den erfolgreichen C-Stand prüfen und die Übernahme der bewährten HT20-/NULL-Korrektur vorbereiten.** Bestehende C-Artefakte und aktuelle Geräte-/Configbelege wiederverwenden; nicht die gesamte RX2-Entwicklung wiederholen. Je Empfänger Zielplattform, App-/Slotgröße, NVS-gesteuerte Identität/Quelle, Konfiguration und vorhandenen Rückweg prüfen. Ein Ergebnis kann die Eignung eines gemeinsamen vorhandenen Kandidaten sein; drei neue Firmwarevarianten sind nicht automatisch erforderlich.

Das ist eine Offline-Vorbereitung für den G1-Blocker, keine Freigabe zum Flashen. Ein späterer begrenzter Installations-/Vergleichsauftrag braucht eigene passende Unterbrechungs-/Schreibfreigaben. RX2 und TX1 bleiben unverändert; das neue Peerprofil bleibt während dieser Basisqualifizierung deaktiviert. Hardware muss für die nächste Offlineprüfung nicht erneut angeschlossen werden.

Der echte RX1↔RX4-Paartest ist jetzt vorbereitet durch die Offlineimplementierung, aber weiter offen. Solange die Vier-RX-Basis G1 nicht erfüllt, darf zusätzlicher Peerbetrieb die Basisdiagnose nicht verdecken. Radar-/Positionsarbeit bleibt eigener Pflichtstrang; Tag 2 mit 7/7 bedeutet kein fertiges Tracking.

**Schwierigkeit des nächsten Problems: 4/5.** Der Fehlerbereich ist jetzt klarer; drei Empfänger müssen noch qualifiziert werden, und die bisherigen Wiederholungen sowie die schlechte gemeinsame Fensterquote bleiben berücksichtigt. Kein Agent gestartet und keine weitere Liveaktion durchgeführt.

<!-- Publication copy: local paths and hardware identifiers were pseudonymized; private original retained. -->
# Hoststart und 35-Minuten-Kurzpilot – Offlineabschluss

Die Startkorrektur und das eigene Profil `rx-shortpilot35-v1` sind offline umgesetzt, gebaut und geprüft. Die Lieferung bleibt **LOCKED_NOT_AUTHORIZED** bis zum unabhängigen Koordinatorreview und einem neuen passenden Hardwareauftrag. In dieser Korrektur gab es keine Hardwarezugriffe, produktiven Hostaktivierungen, Installationen, Messungen oder Automationsänderungen.

## Belegter Startfehler und Korrektur

Beide echten Hostlogs des vorherigen Laufs `20261016T180528Z` melden den fehlenden `WDP_RUFIELD_SIGNING_SEED`. Das Hauptprogramm beendet diesen Fall mit Exit 78. Der gespeicherte Runnerabschluss nach etwa 166,9 Sekunden weist **0 OTA und 0 Captures** aus. Seine verspätete Meldung `host readiness deadline/cleanup reserve` verdeckte den frühen Prozessabbruch. Die zusätzliche Warnung zum Server-Control-Port 0 ist davon getrennt und nicht die belegte Abbruchursache.

Die neue Python-Startschicht erzeugt einen Signierschlüssel mit `os.urandom(32)` einmal pro Sitzung und übergibt ihn ausschließlich im Environment eigener Hostkinder. Er steht weder im Vertrag noch in argv, Permit, allgemeinen Logs oder Berichten. Innerhalb der Sitzung bleibt der Schlüssel über Default→Peer→Default erhalten. Produktive Secrets werden nicht übernommen. Die bestehende RuField-Signatur- und Datenschutzlogik bleibt unverändert; ihre Oberfläche veröffentlicht den jeweiligen öffentlichen Prüfschlüssel. RX-Aufzeichnung und Peerzulassung verwenden diese RuField-Signieridentität nicht als fremde feste Vertrauensbindung.

Früh beendete eigene Kinder werden jetzt vor weiteren Bereitschaftsversuchen erkannt und mit Exitcode und privatem Logverweis gemeldet. Der vorbereitete Binder setzt ausdrücklich `--server-control-port 0`, `--no-edge-registry` und `--rx-session-only`. Port 0 verhindert den zusätzlichen Controllerstart bereits vor dessen Dateierzeugung. Die Registrykonstruktion selbst lädt noch nichts; ein späterer Abruf wäre bedarfsgetrieben. Sie wird für dieses Profil vollständig deaktiviert. mmWave, Discovery und Gerätepoller bleiben im RX-Sitzungsmodus ausgeschlossen.

## Tatsächliche isolierte Startprüfung

Beide vorgesehenen Hostprogramme wurden durch den tatsächlichen `NativeOS.launch`-/Permit-/Main-Pfad gestartet. Fehlender und ungültiger Schlüssel führen jeweils zu Exit 78. Ein zufälliger Sitzungsschlüssel erreicht die folgende Initialisierung; ohne Ersatz der Socket-Primitiven endet der Prozess erwartungsgemäß an der OS-Netzwerksperre.

Die positive Bereitschaftsprüfung ersetzt ausschließlich die unterste Socket-Schicht durch eigene anonyme Unix-Socketpaare. Das echte Programm durchläuft Konfigurationsinitialisierung, Tokio, UDP-/WebSocket-/HTTP-Listeneraufbau und den echten HTTP-Statushandler bis **HTTP 200**. Es gibt keine produktive Portbindung und keinen Kontakt zu einem Projektendpoint. Die Testbibliothek verweigert das Laden ohne bereits wirksame OS-Netzwerksperre und wird keinem Hardwarekandidaten hinzugefügt. Der zuerst abgewiesene falsche Host-Header bleibt als Gegenbeleg erhalten; Host-/Originprüfungen wurden nicht abgeschaltet.

## Neues Profil und Zeitgrenzen

| Bestandteil | Neue feste Grenze |
|---|---:|
| Basis / Paaraufnahme | **180 / 300 s**, jeweils zusätzlich höchstens 20 s für Start, Vor-/Nachlauf und Abschluss |
| Firmware-Peerfrist | **900 s seit Boot**, RX1 und RX4 |
| Host-Peerfrist | **360 s**, einschließlich Wechsel, Paar und Default-Rückkehr |
| Gesamter Liveabschnitt ab ursprünglichem T0 | **2.100 s** |
| Vorlauf bis zur Sessionübergabe | **145 s**, Verbrauch und Uhr werden übernommen |
| Jeder Upload / jeder Hostwechsel | **20 s**; langsamere Abläufe brechen ab |
| VALID | unverändert bei **0/60/120 s**, je Phase höchstens **131 s** |
| Endreserve | **150 s = zwölf End-GETs × 10 s + 30 s Cleanup** |
| Künftige einmalige Rückkehr | **T0 + 2.220 s = 37 min**, erst unter späterem Liveauftrag anzulegen |

Der konservative Zuteilungsplan endet bei **T0 + 2.097 s**; drei Sekunden bleiben bis zur absoluten Grenze. Übergabe- und sonstige Uhrfortschritte verbrauchen dasselbe Budget. Jeder tatsächliche Eintritt wird erneut gegen Restablauf und Fristen geprüft. Die zwölf End-GETs und Cleanup bleiben reserviert; im Abbruchpfad wird die nötige Default-Rückkehr mit weiteren 20 Sekunden vor dem Warten berücksichtigt. Ein langsamerer Upload oder Start wird nicht durch verlängerte Fristen passend gemacht.

Die früheste RX1-Frist wird konservativ vor dem OTA-Dispatch angesetzt: Erst der neue Upload kann das zuvor frisch ausgeschlossene neue Image starten. Die späteste Bootgrenze stammt aus einem tatsächlich beobachteten konsistenten neuen VALID-Boot; sein Boot liegt vor dieser Antwort. Deshalb ist letzte VALID-Antwort + 900 s eine sichere obere Wartegrenze. Uploadende und Boot werden nicht gleichgesetzt. Im Zuteilungsplan endet der vollständige Paarabschnitt bei 1.527 s und damit 78 s vor der frühesten RX1-Frist von 1.605 s.

Die sekundengenaue Tabelle einschließlich letzter Eintrittssekunden und Abbruchhüllen liegt in der [privaten Lieferung](./private/rx-hoststart-kurzpilot-offline-20261016-20261016T183145Z/REPORT.private.md). Die Eintrittswerte gelten jeweils für den dort gebundenen vorherigen Zustand und sind keine unabhängig aufsummierbaren Verzögerungen. Die durchgehende Runtime prüft stets den tatsächlichen Zustand.

## Programme, Auswertung und Prüfungen

Zwei neue Appkandidaten für RX1/RX4 sind gebaut, jeweils **1.271.936 Byte**, unter 2 MiB. Effektive SDK-Konfigurationen unterscheiden sich ausschließlich durch 900 Sekunden und die neue explizite Kurzprofilkennung; Partitionen, acht NVS-Konfigurationswerte, OFDM6, 50/40/10-Ratenpolitik und 6/2-Aufteilung bleiben erhalten. Bootloader sind keine Installationsziele. Das Default-Hostprogramm bleibt bytegleich. Der Peer-Host musste wegen seiner fest gebundenen 1.200-Sekunden-Frist und des strikten alten Profils neu gebaut werden. Datei-Testprogramme mit virtueller Uhr sind separat gebunden und keine normalen Hardwarekandidaten.

Die tatsächliche Fensterfunktion bestätigt **173/293 feste Fenster**. Die aufgerundeten 90-Prozent-Schwellen sind **156/264**; je Peerlink gelten mindestens **600 geeignete Pakete**, 2 Hz und 264 qualifizierte Fenster. Die vier TX1-Kontrolllinks behalten mindestens 5 Hz, das Hostalter bleibt auf 250 ms begrenzt. Physische Zuordnung, Masken, Identität und Zeitgates bleiben erhalten.

Abgeschlossene Belege:

- **138/138** unveränderte Langruntime-Regressionen auf der eingefrorenen Phasenzeitbasis.
- **32/32** Kurz-/Endpfadtests: vorhandener/abwesender Anfangshost, ursprünglicher nichtnull T0, zwölf vorverbrauchte GETs, echte Session-/Adapter-/NativeOS-Pfade, beide Aufnahmen, Hostabfragen, langsame Phasen, früher Exit, zu langsamer Upload, Speicher-/Volume-/Besitzfehler und begrenzter Dateischluss des Default-Logpräfixes innerhalb Cleanup.
- **30 C-Prüfungen**, einschließlich 900-s-Grenzen, Bootwechsel, deaktiviertem Peer, maximalen Zählern, Statusserialisierung, unveränderten Ratenfällen und frisch erzeugten Paketen beider Richtungen/Layouts.
- **9 Rust-Peertests plus 1 ausdrücklich ausgeführter C→Rust-Rundlauf**, einschließlich 360-s-Ende und Ablehnung alter/widersprüchlicher Profile.
- **8 abschließende echte Main-Startfälle** über beide Hostvarianten; zwei positive HTTP-Bereitschaftsnachweise auf isolierten Socketpaaren.
- Echte Datei-Erzeuger→Rust-Parser→Normalisierung→Report für 180/300 s: synthetische Qualitätsrechnung bestanden, Gesamturteil wegen fehlender echter Firmwarezeit weiterhin **NICHT_BEURTEILBAR**. Writer-, Zeit-, Format- und fehlende Dateibelege erzeugen keinen falschen PASS und verkleinern die Nenner nicht; der Formatgegenfall bleibt im Qualitätsunterurteil FAIL.

## Gesperrte Übergabe

Der [Belegindex](./private/rx-hoststart-kurzpilot-offline-20261016-20261016T183145Z/TEST-INDEX.private.json), die [gesperrte Hardwareübergabe](./private/rx-hoststart-kurzpilot-offline-20261016-20261016T183145Z/HARDWARE-CONTRACT.SHORTPILOT.LOCKED.private.json) und der [kleine Verifier](./private/rx-hoststart-kurzpilot-offline-20261016-20261016T183145Z/verification/verify.py) binden die tatsächlichen Quellen, Programme und geschlossenen Prüfergebnisse. Alle Livegrants sind falsch, Ablaufzeit ist null. Zeitlich begrenzte isolierte Starttest-Permits sind abgelaufen.

Der Startbedarf bleibt unverändert **70.922.518.528 Byte**. Der aktuelle lokale Dateisystemguard besteht; vor einem späteren Lauf sind Zielpfade, Volume und freier Platz erneut frisch zu binden. Das alte Langprofil, seine Programme und Fehlerbelege sowie der vorhandene Arbeitsbaum sind erhalten. Der nächste Schritt ist ausschließlich der unabhängige Koordinatorreview. Die Tagesabnahme vom 14. Oktober und sämtliche echten Hardware-/Funknachweise bleiben offen.

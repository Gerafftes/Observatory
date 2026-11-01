<!-- Publication copy: local paths and hardware identifiers were pseudonymized; private original retained. -->
# Koordinatorreview: Hoststart und 35-Minuten-Kurzpilot

**PASS_OFFLINE im geprüften Umfang. Das neue Kurzprofil ist technisch für einen gesondert autorisierten Hardwarelauf vorbereitet.** Keine Geräte-/Hostaktivierung und kein Agentenstart durch diesen Review. Eine erfolgreiche Funkmessung ist weiterhin offen.

Geprüfte Lieferung: `./private/rx-hoststart-kurzpilot-offline-20261016-20261016T183145Z/`. Grundlage ist der [Agentenabschluss](hoststart-kurzpilot-offline-review-20261016.md). Eigene neue Prüfbelege liegen ausschließlich unter `./private/rx-kurzpilot-koordinatorreview-20261016T193748Z/`; Originale und deren geschlossene Manifeste bleiben erhalten.

## Ergebnis

| Bereich | Unabhängiger Befund |
|---|---|
| Hoststart | Belegten Exit 78 wegen fehlendem Signierschlüssel nachvollzogen. Neue Startschicht erzeugt pro Sitzung 32 sichere Zufallsbytes, übergibt sie nur an eigene Kinder und behält die Signieridentität über die drei Hostwechsel. Frühe Prozessabbrüche werden erkannt. |
| Tatsächlicher Startpfad | Acht Main-Proben frisch wiederholt: fehlende/ungültige Schlüssel bleiben Exit 78; Default und Peer erreichen jeweils echten HTTP 200 über ausschließlich isolierte anonyme Socketpaare. OS-Netz-/Gerätesperren wirksam; keine produktiven Ports oder Projektendpoints. |
| Firmware | Neue RX1-/RX4-Appkandidaten mit je 1.271.936 Byte und 900 Sekunden Peerfrist gebunden; Profile, Konfigurationen, Partitionen, Apps und ELF-Dateien durch frischen Integritätscheck bestätigt. Kein Bootloader als Installationsziel. |
| Hostneubauten | Default bytegleich erhalten. Notwendiger Peer-Neubau mit 360 Sekunden Hostfrist und strikter neuer Profilidentität gebunden. Datei-Testprogramm bleibt gesondert. |
| Tests | Alle 32 gelieferten Kurz-/nativen Endpfadtests frisch bestanden. Zusätzlich drei eigene Proben bestanden: erschöpfte globale Restzeit blockiert vor erster Aktion; langsamer RX4-Upload erzeugt keinen dritten POST oder Ersatzcapture; getrennte Sitzungen erhalten verschiedene Signierer. |
| Vorhandene Fachbelege | 138 Langregressionen auf unveränderter Langbasis, 30 C-Prüfungen, neun Rust-Peertests und ausdrücklich ausgeführter C→Rust-Rundlauf nach Hash gebunden; keine Behauptung, diese im Review neu kompiliert zu haben. |
| Auswertung | Eigenes Pilotprofil: 180/300 Sekunden, 173/293 feste Fenster, Schwellen 156/264, beide Peerlinks jeweils mindestens 600 geeignete Pakete und 2 Hz, vier TX1-Links jeweils mindestens 5 Hz. Synthetisches Gesamturteil bleibt korrekt NICHT_BEURTEILBAR. |

Die Lieferung wurde vollständig erneut mit ihrem kleinen OS-isolierten Verifier geprüft: 2.893 Produktionsquellbindungen, Kandidaten, geschlossene Testbelege, Originalbindungen, versiegelte Dateien und gesperrter Hardwarevertrag bestanden. Alle ursprünglichen Testberechtigungen abgelaufen; Livegrants falsch und Ablaufzeit null.

## Was die 35 Minuten tatsächlich bedeuten

- **Ein einziges T0**, einschließlich Vorlauf und übernommenem Aktionsverbrauch; höchstens 145 Sekunden bis zur Sessionübergabe.
- 180 Sekunden Basis und 300 Sekunden Paaraufnahme; pro Aufnahme höchstens 20 Sekunden Zusatzbudget.
- RX1 danach RX4 installieren, jeweils höchstens ein später autorisierter App-OTA-Versuch; unveränderte VALID-Serie bei 0/60/120 Sekunden.
- Upload und Hostwechsel jeweils höchstens 20 Sekunden. Ein langsamerer Ablauf bricht ab; es gibt keinen spontanen Zusatzversuch.
- Firmware-Peerende nach 900 Sekunden seit Boot, Host-Peerfrist 360 Sekunden. Früheste und späteste Bootgrenzen werden getrennt gerechnet. Der Paarvergleich endet im konservativen Plan 78 Sekunden vor der frühesten RX1-Frist.
- Zwölf End-GETs und 30 Sekunden Cleanup erhalten zusammen 150 Sekunden Endreserve. Die gegebenenfalls nötige Default-Rückkehr wird davor separat reserviert.
- Der konservative Phasenplan endet bei **2.097 Sekunden = 34:57 Minuten**, die gesamte Ablaufgrenze liegt bei **2.100 Sekunden = 35 Minuten**. Normal- und Abbruchgates erhalten die ursprüngliche Uhr. Übergabe-, Journal- und lokale Prüfkosten verbrauchen dieselben Phasenbudgets.

Das ist eine gebundene Obergrenze der vorbereiteten Ablaufsteuerung mit geprüftem Abbruchverhalten, keine Garantie für einen erfolgreichen Funklauf. Es bleiben nur drei Sekunden zwischen dem konservativen Zuteilungsplan und der absoluten Grenze; weitere Verzögerungen müssen innerhalb der einzelnen Obergrenzen liegen oder führen zum Abbruch. Synthetische Uhren und isolierte Prozessstarts beweisen keine aktuelle WLAN-/OTA-Latenz oder Geräteerreichbarkeit.

## Nächster Schritt

Neuer konkreter Hardwareauftrag für genau dieses Profil mit passenden getrennten Freigaben für Betriebsunterbrechung/Host und RX1-/RX4-Appschreiben. Der bisherige Langlauf und verbrauchte Freigaben sind kein Einstieg für die neuen Kandidaten. Erst frische Geräte-, Image-, Boot-, Slot-, Acht-Felder-, Rückweg-, Hostbesitz-, Speicher- und Pfadbindungen; bei bestandenem Preflight innerhalb des neuen begrenzten Auftrags selbstständig fortfahren.

Benötigt werden TX1 und alle vier RX am Strom sowie der Mac am Netzteil im bestehenden Projekt-Netzweg. Nur RX1 und RX4 sind spätere OTA-Ziele. TX1, RX2 und RX3 bleiben unverändert. Kein serieller Anschluss für diesen Netz-/OTA-Pilot vorgesehen; mmWave ist für diesen Funk-/Transportvergleich nicht erforderlich.

Der künftige Runner loggt dauerhaft und arbeitet selbstständig; genau eine Rückkehr im selben Chat ist für T0 + 37 Minuten vorgesehen. Sie liegt außerhalb der 35-Minuten-Livegrenze und erlaubt nur Offlineauswertung. Noch keine Automation oder Messung gestartet. Die Tagesabnahme vom 14. Oktober bleibt 2/5; ein Kurzpilot schließt die vollständigen Layout-/Firmwarezeit-/Ressourcen- und Trackingnachweise nicht automatisch.

Private eigene Prüfmatrix, aktuelle freie Bytes und Hashbindungen: [REVIEW.private.json](./private/rx-kurzpilot-koordinatorreview-20261016T193748Z/REVIEW.private.json). Eine erste Zusatzprobe scheiterte an ihrer eigenen exakt auf die Vorlaufgrenze gesetzten Testvorbereitung; der monotone Prüftick überschritt bereits diese Grenze. Dieser Prüfmittelbefund ist erhalten. Die korrigierte frische Probe prüft die eigentliche nachfolgende Restzeitgrenze erfolgreich; kein Produktfehler wurde dafür verändert.

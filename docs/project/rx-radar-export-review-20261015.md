<!-- Publication copy: local paths and hardware identifiers were pseudonymized; private original retained. -->
# Koordinatorreview: RX-Korrekturen und RX-/Radar-Erfassungsexporte

**Beide RX-Auswertefehler sind unabhängig bestätigt behoben. Die Erfassungsexporte funktionieren offline vom tatsächlichen Produzenten bis zum Bericht. Der nächste konkrete Hardwareauftrag kann die getrennte Radar-G2-Prüfung vorbereiten.** Der volle RX-Gesamtnachweis bleibt begrenzt; vor dem langen Paarlauf ist zusätzlich das Dateibudget abzusichern. Keine Hardware-, Netz-/Geräteabfrage, Installation, Hostaktivierung oder Agentenbeauftragung durch diesen Review.

Lieferung: `./private/rx-radar-export-korrektur-offline-20261015T152415Z/`. Bericht, Bereitschaftsmatrix, Erfassungsübergabe, Validierungsindex, tatsächliche neue Quellen und jüngster abgeschlossener Agententurn geprüft. Private Reviewreceipts: `./private/rx-radar-export-koordinatorreview-20261015/`.

## Ergebnisse

| Bereich | Unabhängiges Urteil |
|---|---|
| Layout-Rückkehr | **KORRIGIERT_UND_REGRESSIERT**: 2.274 Rückkehrframes wieder gutgeschrieben, RX2 jetzt 2.874 passende Frames und 470/473 Fenster |
| Getrennte Streckenverträge | **KORRIGIERT_UND_REGRESSIERT**: TX1-128 und beide LLTF64-Peerstrecken zusammen passend; je Richtung 1.920 Pakete, 2 Hz, 953/953 Fenster |
| RX-Dateiexporte/Clockbindung | **IMPLEMENTIERT UND INTEGRIERT**: Originalantworten, Start-/Endmeta, Raw-/Peerdateien, tatsächliche Host-/Writerereignisse und nicht gleichgesetzte monotone Ursprünge |
| Radar-Paket-/Hostzeitexport | **IMPLEMENTIERT UND INTEGRIERT**: durchgehender tatsächlicher Fleet-/Writerpfad, Einzel-/Gemeinschafts- und Hilfsphasen; keine ACK-/Medianersatzzeitreihe |
| Radarhardwarevorbereitung | **GEEIGNET FÜR KONKRETEN FREIGABEAUFTRAG**; aktuelle reale Bindungen und Messangaben erst im späteren Auftrag |
| RX-Hardwarevorbereitung | **TEILURTEILE VORBEREITET**, kein Gesamt-PASS: Firmwarefrische, Diagnose-Vorklammern und vollständiges Dateibudget bleiben zu beachten |

Die beiden eingefrorenen RX-Gegenfälle wurden mit der aktuellen tatsächlichen Datei-CLI erneut ausgeführt. Es wurde kein Raster-/Maskenschutz gelockert: gültige Layout-Rückkehr erholt sich, Übergangsfenster und echte Identitäts-/Konfigurationswechsel bleiben getrennt; TX1 und Peer besitzen eigene vorab deklarierte Verträge. Der bereits geschlossene Recordervertrag mit `frames_written` und der Sessionkern bleiben unverändert.

## Tatsächlich wiederholte Prüfung

- Netzgesperrte Nur-Lese-Prüfung der Versiegelung: **12.896 Lieferdateien, 988 Quelldateien**, keine Abweichung; **8.010 Ursprungsdateien, zwölf ursprüngliche Symlinks und 54 Dirty-Dateien** erhalten. Aktuellen Gitstatus zusätzlich gegen den gespeicherten Anfangsstatus verglichen: vor den Koordinator-Dokumentationsänderungen bytegleich.
- Beide RX-Gegenfälle und beide Normalisierungs-/Auswertestrecken aus gespeicherten tatsächlichen Produzentenoutputs erneut ausgeführt. RX: Hostzeit und physische/Fensterqualität PASS, Firmwarefrische und Lifecycleklammern ausdrücklich NICHT_BEURTEILBAR; Gesamt NICHT_BEURTEILBAR. Radar: beide Einzelstufen 180/180 und gemeinsame Stufe 180/180, synthetisch PASS.
- **Fünf Produzenten-/Export-Testgruppen vollständig neu ausgeführt**, mit neuen privaten temporären Dateien: tatsächlicher RX-Produzent bis Bericht; Radar-Gutlauf und sieben Varianten mit schlechter, stale, mehrdeutiger oder fehlender zweiter Quelle sowie Writer-/Größen-/Queuefehlern; unterschiedliche Clockursprünge und Fehler; Default ohne Export/fehlgeschlagener Start; kontrollierte Adapterabbruchfälle. Keine versiegelten Testoutputs überschrieben.
- **34 Kernprüfungen, 26 bisherige Adapterszenarien und zehn Recorderregressionstests mit 292 Szenariodurchläufen** unter gesperrten realen I/O-Einstiegen erneut bestanden. Die 292 Szenarien sind keine 292 zusätzlichen eigenständigen Tests.
- Je **fünf Rust-Exporttests** aus eigenen temporären Kopien der finalen Default-/Peertestprogramme unter Prozess-Netzverbot erneut bestanden. Gespeicherte vollständige Hostlogs belegen je 566 PASS, sechs ignored und sieben gefilterte Loopbackfixtures. Diese Vollsuiten und die 38 gespeicherten Replays wurden hier nicht nochmals vollständig ausgeführt. Keine neuen Cargo-/SDKbuilds oder normalen Hoststarts durch den Koordinator.

Die Testprogramme verwenden ausschließlich eingespeiste synthetische Datagramme/Uhren und tatsächlich geschlossene lokale Dateien. Nur die gesonderten gebundenen Datei-Testbinarys laufen; die normalen Hostkandidaten und Liveadapter bleiben gesperrt. Die Rusttestbinarys verwenden trotz Kopie einen eincompilierten Testausgabepfad: 38 dabei neu erzeugte synthetische Dateien wurden anschließend ausschließlich aus diesen neuen Testordnern in die temporäre Reviewablage verschoben. Keine ursprüngliche Datei überschrieben; Relokationsreceipt privat erhalten und exakte Inventar-/Hash-/Modusversiegelung danach erneut bestätigt. Für weitere Wiederholungen den kompilierten Ausgabepfad berücksichtigen. Der neue Writer ist standardmäßig aus, begrenzt und weist unvollständige Abschlüsse sichtbar aus.

## Verbleibende RX-Grenzen und Dateibudget

Der unveränderte 42-GET-Plan liefert keine vollständige Diagnose-/Source-Vorklammer aller RX vor der Basis und keine frische RX2-/RX3-Diagnose vor dem Paar. Die neue Normalisierung markiert dies korrekt. Ein späterer konkreter RX-Auftrag muss aktuelle Vorklammern ausdrücklich planen oder die betroffenen Aussagen offen lassen; keine zusätzlichen Queries aus diesem Review. Geräte-Ein-/Ausgangszeiten für den 100-ms-Firmwarefrischenachweis fehlen weiterhin. Hostzeiten und Code-/SDKtests ersetzen sie nicht; keine neue Firmwareinstallation daraus ableiten. RX2/RX3-C/C1-Legacyframes erhalten keine erfundene physische Attestation. Ressourcen und Positionsnutzen bleiben unbekannt.

**Zusätzlicher praktischer Vorbehalt für den 960-s-RX-Paarlauf:** Die erfolgreiche synthetische Produzentenaufnahme verwendet insgesamt 24 TX1-Frames/s plus vier Peerframes/s und schreibt etwa 96 MB Trace. Aus den tatsächlich erzeugten Eventgrößen lässt sich die feste Grenze von **256 MiB = 268.435.456 Byte** nicht als ausreichende Reserve für die zuletzt beobachtete höhere RX-Rate bestätigen. Die Hochrechnung mit 74,1692 passenden TX1-Frames/s plus vier Peerframes/s ergibt etwa **270.309.757 Byte**, bei 80 plus vier etwa **290.568.725 Byte**. Tatsächlicher Hintergrundverkehr und andere Event-/Identitätslängen können abweichen.

Das ist eine **Offlineprojektion**, kein beobachteter realer Überlauf und keine neue RF-Messung. Der Writer würde eine Grenzüberschreitung korrekt als unvollständig markieren. Vor dem vollen RX-Lauf deshalb einen begründeten endlichen Dateietat beziehungsweise verlustfreien kompakteren Export festlegen und mit passender Last testen. Keine Aufnahme kürzen, Belege still verwerfen oder nach Datenansicht den Nenner ändern. Dieser RX-Vorbehalt blockiert den getrennten Radarauftrag nicht.

## Nächster Schritt und genaue Hardware

**Als Nächstes einen konkreten Radar-G2-Hardwareauftrag erstellen**, einschließlich frischer Host-/Quellen-/Boot-/Setupbindung, realem Punkt-/Phasenprotokoll, gesonderten Host-/Aufnahme-/Lese-/gegebenenfalls Unterbrechungsfreigaben und kontrollierter Hostrückkehr. Die ausgelieferten normalen Hostbinarys sind bewusst gesperrt; ein späterer Auftrag muss eine ausdrücklich geprüfte Livekopie und deren finale Hashbindung vorbereiten. Der Review startet keinen Host und entsperrt keine Lieferung.

Der bekannte Radarplan hat höchstens **110 Minuten einschließlich 15 Minuten Ende/Reserve**. Zuerst beide Sensoren einzeln, dann gemeinsam an den unveränderten P1–P9-Punkten prüfen. Keine neue Gerätekonfiguration, Kalibrierung oder Transformationsschreibaktion ohne eigenen konkreten Umfang. Aktuelle physische Genauigkeit und Radar-G2 bleiben bis dahin offen.

Benötigt für diesen ersten Block:

| Anzahl | Hardware / Hilfsmittel | Anschluss und Zweck |
|---|---|---|
| 2 | Bereits verkabelte Radarknoten **MMWAVE1 und MMWAVE2**, je **HLK-LD2450 + ESP32-C3 SuperMini** mit vorhandener Trägerplatine/Breadboardverdrahtung | Als vollständige bestehende Einheiten an den vorgesehenen Positionen betreiben |
| 2 | Passende USB-Stromkabel und zwei stabile USB-Netzteilanschlüsse | Beim SuperMini USB-C am ESP; Versorgung über die vorhandene getestete Schaltung, Daten über WLAN |
| 1 | Mac mit bisherigem Projekt-WLAN/Hostweg | Empfang, gebundene Aufnahme und spätere Dateiauswertung; kein serieller Zugriff erforderlich |
| 1 Satz | Maßband oder Lasermaß, Klebeband/Bodenmarken und zwei feste Ablage-/Befestigungsplätze | Raum/Sensorlagen und unabhängige Punkte vermessen, Sensoren während der Messung stabil halten |
| 1 | Person für Standpunkte und Wege | Einzelpersonenreferenz; kein zweiter Proband für diesen Block nötig |

Die ESP32-C3-/LD2450-Zuordnung und USB-Versorgung entsprechen der bestehenden [Radarfirmware](../../software/ruview/firmware/esp32-mmwave-node/README.md) und den vorhandenen C3-Trägerplatinen. Daraus wird kein aktueller angeschlossener Gerätezustand behauptet. Keine neue Platine, zusätzlicher Radarsensor oder USB-UART-Adapter nötig. Die USBkabel müssen für diesen WLANlauf nicht als Datenkabel am Mac hängen.

**TX1 am normalen Platz mit seiner bisherigen Versorgung und dem bestehenden Projekt-Netzweg belassen.** Wenn die RX bereits versorgt sind, können sie dort bleiben; im Radarblock werden sie nicht geändert. Für den späteren getrennten RX-Paarlauf werden **TX1 + RX1, RX2, RX3, RX4** an ihren festen Positionen, stabile Versorgung und derselbe Mac benötigt. Im vorgesehenen WLAN-/OTAablauf keine COMverbindung und kein Mac-USB für TX/RX nötig; spätere eigene Schreib-/Unterbrechungsfreigaben gelten weiterhin. Beide vollständigen Hardwarebudgets nicht in ein bis zwei Stunden versprechen.

Die Tagespunkte vom 14. Oktober bleiben bei **2/5**; keine reale G1-/G2-/Peer-/Positionsabnahme aus den Offlineergebnissen. **Schwierigkeit des nächsten Radar-Hardwareblocks: 4/5.** Kein Agent gestartet.

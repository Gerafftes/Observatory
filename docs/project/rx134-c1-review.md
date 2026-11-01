# Koordinatorreview: gemeinsamer C1-Kandidat für RX1, RX3 und RX4

**Offlineprüfung bestanden. C1 ist ein gemeinsamer getesteter Kandidat für die drei Empfänger; eine uneingeschränkte Installationsfreigabe folgt daraus noch nicht.** Als nächster konkreter Schritt muss ein RX3-spezifischer Rückweg gebunden werden. Danach ist RX3 als einzelner Pilot vorgesehen. TX1 und RX2 bleiben unverändert, Peer bleibt deaktiviert und G1 offen.

Grundlage: [Abschlussbericht](../../private/rx134-c-eignung-offline-20261014T110737Z/REPORT.private.md), Quellen-/Buildbindungen, Patch, Testreceipts und vorhandene Laufbelege. Hardwareaktionen des Agenten und dieses Reviews: null.

## Ergebnis des Quell- und Artefaktvergleichs

Original C kann bei bestimmten NVS-Initialisierungsfehlern NVS löschen und erneut initialisieren. C1 ersetzt diesen Block durch `ESP_ERROR_CHECK(nvs_flash_init())`. Bei Fehler stoppt die App vor Configladen, Identität und WLAN; sie formatiert oder migriert NVS nicht und startet nicht mit Ersatzwerten weiter. Ein dauerhafter Fehler kann zum Neustart führen. Die normale Startfolge bleibt erhalten. Geändert sind ausschließlich `main/main.c` und die abgeleitete Diagnose-Quellidentität; das Original bleibt unverändert.

Ein gemeinsames Image genügt softwareseitig: eigener NVS-Node, TX-Quelle und Konfiguration bleiben erhalten. Native Tests prüfen RX1/RX3/RX4 und Kontrollfall RX2. Die HT20-Initialisierung ist als begründeter Ansatz gebunden; eine isolierte Ursache der realen Formatverteilung ist damit nicht bewiesen. Keine neue Funk-, Server- oder Positionsabnahme.

| C1-Identität | Wert |
|---|---|
| App-BIN | `private/rx134-c-eignung-offline-20261014T110737Z/build-C1/rx/esp32-csi-node.bin` |
| Größe | 1.238.784 Byte |
| Reserve gegenüber gespeichertem 2-MiB-Updatebereich | 858.368 Byte |
| BIN-Datei-SHA256 | `01414441fbb9d4126d3c342e028b2693fd0b718816f34d2cf295d2c9c17bd5b6` |
| Erwarteter Status-/Partitions-Image-Digest | `9f5afbc5cdb040d4891f70368482a0aa1bd104f995c190588bdbbf20083f00fd` |
| ELF-Datei-SHA256 | `aab9056fd4dc13d6f0630393d75cb2d5b37352256ba3c56812a99ee8c909697c` |
| Diagnose-Source-SHA256 | `1edd6fcb06bc846fea1c407926066d629849b210e62da44379158a6a1baf8b84` |

Dateihash, ELF-Appdescriptor und Statusdigest sind verschieden: Der vorhandene Status verwendet `esp_partition_get_sha256`. Der erwartete Statusdigest stimmt mit dem verifizierten angehängten Imagehash überein. Die Größe ersetzt keine frische Slotprüfung am Gerät.

## Unabhängig wiederholte Prüfung

- Alle **4.297 Manifestdateien** hinsichtlich Größe und SHA-256 geprüft; zusätzlich **109 gebundene Eingangsartefakte** aus altem Build, C und C1 verifiziert.
- Alle **16 geerbten nativen Testausführungen** erneut bestanden. Die gebundenen Binarys wurden in neuen privaten temporären Kopien ausgeführt; Originale blieben unverändert.
- Die vier NVS-Harnessvarianten aus den finalen Quellen neu kompiliert und ausgeführt: C und C1, jeweils mit tatsächlichem Default 1 und abweichendem Testdefault 2. **4 × 28 = 112 Fälle bestanden.** Initialisierungsfehler, normale Starts, spätere ID-/MAC-Überschreibung und fehlende/falsch typisierte Zulassungswerte sind enthalten.
- Positive C/C1-Paket- und Zustandsstreams der NVS-Harnesses sowie Callback-/Transport-/Edge-Paketstreams sind bytegleich. Diagnose-Quellidentität absichtlich neu.
- Der ursprüngliche OS-Netzsandbox-Wrapper ließ sich in dieser Koordinator-Prüfumgebung nicht verschachteln (`sandbox_apply: Operation not permitted`); dies war kein nativer Testfehler. Für die lokale Wiederholung wurden synthetische Harnesses direkt verwendet und vor Ausführung ihre importierten Symbole auf echte Netzwerk-/Prozess-/Device-I/O-Einstiege geprüft. Kein solcher Einstieg vorhanden. Der vollständige SDK-Build wurde hier nicht erneut ausgeführt; dessen gehashtes Erfolgsreceipt und No-Fetch-/OS-Netzverbotsnachweis stammen aus dem Agentenlauf.
- Die gespeicherten 92 Zulassungsassertions und 22 Originalantwortbindungen sind im gehashten Abschluss enthalten. Keine erneute Geräteabfrage, kein neuer Capture und keine unnötige Host-/UI-Gesamtsuite.

## Installationsvoraussetzungen

Für alle drei RX bleibt **BEDINGT** zutreffend: Vor einem Schreibauftrag muss der tatsächliche Rückweg für den jeweiligen Empfänger feststehen und die frische Geräte-/Config-/Quellen-/Slotbindung bestehen. Eine alte Appdatei liegt gebunden vor; frühere Bootloaderkopien sind kein Nachweis des heutigen installierten Bootloaders. `VALID`, `next_partition` und Rollbackflags im Kandidaten allein sichern keine Rückkehr bei unerreichbarer App.

C1 kann bei NVS-Fehler vor HTTP abbrechen. Ein Rückupload über HTTP genügt deshalb allein nicht. Tragfähig sind entweder ein gerätegebundener belegter automatischer Rückweg oder ein konkret begrenzter manueller Rückweg mit gesonderter Freigabe. Keine pauschale Wiederholung von Vollsicherungen und keine weiteren Firmwareänderungen aus dieser Lücke ableiten.

**Einordnung der RX1-ID-Frage:** Der Exportwert 1 beweist seine physische NVS-Herkunft nicht. Allerdings sind der tatsächlich gebundene alte NVS-Lader und der von C gleich; C1 ändert ihn ebenfalls nicht. Alle drei Builds verwenden den Default 1, und die geprüften NVS-relevanten Builddefaults ändern sich nicht. Daraus folgt kein belegter Kompatibilitäts- oder Identitätswechsel bei RX1, auch wenn die Herkunft des Werts offen bleibt. Eine zusätzliche NVS-Ausleserunde allein wegen dieses gleichen Defaults ist nicht begründet. Frische ID-/Config-/Quellenprüfung vor und nach einem späteren Pilot bleibt zwingend. Dies ist eine fachliche Einordnung aus dem Artefaktvergleich, keine neue Aussage über physische Schlüsselpräsenz.

## Nächster Schritt

**RX3-Rückwegprüfung und anschließenden Pilot möglichst in einem konkret begrenzten Auftrag bündeln.** Vorhandene gerätespezifische Schreib-/Readbackbelege zuerst gezielt nutzen; falls sie nicht reichen, die erforderliche schonende Hardwareprüfung beziehungsweise einen geeigneten manuellen Rückweg genau festlegen. Erst nach erfüllter Rückwegvoraussetzung und ausdrücklicher Unterbrechungs-/Schreibfreigabe C1 auf genau RX3 installieren und Boot, Config sowie passende CSI-Fenster prüfen. Bei fehlendem Rückweg kein Upload; bei unklarem Ergebnis keine weitere Installation oder Wiederholung.

Für eine notwendige Hardwareprüfung ist zunächst RX3 mit passender Datenverbindung zum Mac relevant; für den anschließenden Funkvergleich kommen TX1 und RX2 als unveränderte Kontrolle hinzu. Die genaue Anschluss-/Wartungsanweisung gehört in den neuen begrenzten Auftrag. Dieser Review spricht keine Hardware- oder Schreibfreigabe aus. RX1/RX4 erst nach erfolgreichem Pilot separat zulassen, keine gleichzeitige Blindinstallation.

Tag 2 bleibt 7/7 abgeschlossen. G1 bleibt wegen der zuletzt gemessenen 5,41 % gemeinsamen Fenster offen; C1s reale Wirkung ist noch nicht gemessen. **Schwierigkeit des nächsten Schritts: 4/5**, einschließlich der bisherigen Versuche und des noch zu bindenden Rückwegs. Kein Agent gestartet.

# D6-Setupaufnahme und TX-Firmwareidentität — 2026-08-09

## Zweck

Vor dem versiegelten D6-Preflight wurden die noch fehlende normale
Mac-Position, der Türzustand und die tatsächlich gestartete TX-Firmware
erfasst. Dieser Eintrag dokumentiert den Stand unmittelbar vor dem finalen
Wechsel des TX zurück auf reine Stromversorgung.

Das Setup ist zu diesem Zeitpunkt ausdrücklich **noch nicht versiegelt**. Der
versiegelte 25-Sekunden-Preflight wurde noch nicht gestartet.

## Physischer Aufbau

Die Positionen verwenden zunächst die bestehende Notation
`(Breite, Länge, Höhe)`. Bezugspunkt des Macs ist die Mitte seines Unterteils.

Der Mac steht auf gleicher Höhe wie RX4 und 4 cm von RX4 entfernt auf der von
RX2 wegführenden Linie:

| Objekt | Breite | Länge | Höhe |
|---|---:|---:|---:|
| RX2 | 2,47 m | 0,00 m | 0,87 m |
| RX4 | 0,98 m | 0,00 m | 0,87 m |
| Mac | 0,94 m | 0,00 m | 0,87 m |

Mit der festgelegten Transformation `x = 4,02 m - Länge`, `y = Höhe` und
`z = 3,44 m - Breite` ergibt sich für den Mac:

`[x, y, z] = [4.02, 0.87, 2.50]`

Weitere festgelegte Zustände:

- Tür: geschlossen
- CSI-WLAN: verbunden
- IP-Adresse des Macs im CSI-WLAN: `CSI_HOST_IP/24`
- Möbel und normale statische Gegenstände: bleiben für Preflight,
  Leerraumkalibrierung und alle späteren Aufnahmen unverändert im Raum

Die jetzige Mac-Position unterscheidet sich vom historischen Aufbau
„Mac mittig“ vom 2026-07-26. Dessen Leerraumreferenzen dürfen deshalb nicht
für dieses Setup wiederverwendet werden. Die neue 65-Sekunden-
Leerraumkalibrierung muss den jetzigen Mac-, Kabel-, Möbel- und Türzustand als
Hintergrund erfassen.

## Zerstörungsfreie TX-Inventur

### Hardwareidentität

Der TX wurde ohne BOOT-Taste und ohne Schreibbefehl per USB mit `esptool 5.3.0`
erkannt:

- Chip: ESP32-S3, Revision 0.2
- physischer Flash: 16 MB
- eingebettetes PSRAM: 8 MB
- Quarz: 40 MHz

Die rohe Chip- beziehungsweise AP-MAC wird in diesem neuen Nachweis nicht
wiederholt. Ältere Journalteile enthalten noch historische Klartextangaben.

### Warum ein zweiter Readback nötig war

`flash_id` belegt die Hardware, nicht aber die exakten Bytes der gestarteten
Senderfirmware. Das strikte Setup-Schema verlangt einen SHA-256-Wert über das
tatsächlich eingesetzte Firmwareartefakt. Deshalb wurde der TX vor dem
Versiegeln ein zweites Mal ausschließlich lesend per USB geprüft.

Ein erster Versuch, den vollständigen 16-MB-Flash bei 460800 Baud auszulesen,
brach bei ungefähr 7 % wegen serieller Paketstörung ab. Die unvollständige
temporäre Datei wurde gelöscht. Dieser Abbruch schrieb nichts auf das Board.

Danach wurde bei 115200 Baud nur die Partitionstabelle gelesen. Sie beschreibt
ein 4-MB-Firmwarelayout innerhalb des physisch 16 MB großen Flashs:

| Partition | Offset | Größe |
|---|---:|---:|
| `nvs` | `0x9000` | 20 KiB |
| `otadata` | `0xe000` | 8 KiB |
| `app0` | `0x10000` | 1280 KiB |
| `app1` | `0x150000` | 1280 KiB |
| `spiffs` | `0x290000` | 1408 KiB |
| `coredump` | `0x3f0000` | 64 KiB |

Die OTA-Auswahldaten zeigen Sequenz 1 und damit `app0` als gestartete
App-Partition. Nur diese 1280-KiB-Partition wurde anschließend bei 230400 Baud
vollständig ausgelesen. NVS, WLAN-Zugangsdaten und die übrigen Datenpartitionen
wurden nicht in diesen Firmware-Readback übernommen.

### Identität der gestarteten TX-App

| Merkmal | Wert |
|---|---|
| Ziel | ESP32-S3 Arduino SoftAP-Sender |
| Projektname im Image | `arduino-lib-builder` |
| App-Version | `43a8f6d` |
| Kompilierzeit | `2026-06-02 11:17:54` |
| ESP-IDF | `v5.5.4` |
| aktive Partition | `app0`, Offset `0x10000`, 1280 KiB |
| SHA-256 des vollständigen aktiven Partitions-Readbacks | `a66a11ad8e299a962572c2bc8a9e4067599a8460c44ae0efb1deae07277994e5` |
| eingebetteter Image-Validierungshash | `586d81820c929ed236f9ea0c6bf389ff00b3cc0e69b60f21478f53a05cdeb285` |
| ELF-SHA-256 laut App-Metadaten | `787ac82dd095673ca6fd8b86e712309e10bc050029ad40bd5d1aa963b27c8a81` |
| Image-Prüfung | gültig |

Für das Setup-Siegel wird der SHA-256 des vollständigen aktiven
Partitions-Readbacks als Identität des real gestarteten Senderartefakts
verwendet. Er ist ein Reproduzierbarkeitsnachweis dieses kontrollierten
Aufbaus, aber keine kryptographische Geräteauthentisierung gegen einen aktiven
Angreifer.

## RX-Bindung

Die vier aktuellen privaten Provisionierungszustände von RX1 bis RX4 melden
jeweils Node-ID 1 bis 4, Kanal 6 und denselben gesetzten TX-Filter. Nach dem
definierten Schema `sha256-ruview-tx-filter-mac-v1` ist die gemeinsame
Filteridentität, ohne die rohe Adresse in diesem neuen Nachweis zu
wiederholen:

`60c998af0f5f845bd2afaac558a7da831a3a34ec07544de0efc6d1e747fad86c`

Eine ältere doppelte RX3-Zustandsdatei ohne Filter bleibt ausdrücklich
historisch und wird nicht als Gerätewahrheit verwendet. Der Filterhash bindet
die kontrollierte Firmwarekette, veröffentlicht aber nicht die rohe MAC.

## Datenschutz und Aufräumen

- Es wurde nichts geflasht, provisioniert oder konfiguriert.
- Der von `esptool` geladene Stub lief nur temporär im RAM.
- Nach jedem Lesevorgang wurde der TX per RTS normal zurückgesetzt.
- Der unvollständige 16-MB-Readback, Partitionstabelle, OTA-Auswahldaten und
  aktive App-Kopie wurden nach Hash- und Metadatenprüfung aus `/private/tmp`
  gelöscht.
- Rohe MAC, WLAN-SSID, WLAN-Passwort und OTA-Schlüssel wurden nicht in diesen
  neuen Nachweis übernommen.

## Aktueller Grenzstatus

Zum Ende dieses Eintrags ist der TX für den Readback noch per USB mit dem Mac
verbunden. Dieser temporäre Kabelzustand gehört **nicht** in das versiegelte
Messsetup.

Der nächste zulässige Ablauf ist:

1. USB-Datenkabel entfernen und TX wieder ausschließlich mit Strom versorgen.
2. Mac, TX, RX1 bis RX4, Möbel, übrige Kabel und geschlossene Tür unverändert
   lassen.
3. Setup-Spezifikation mit dem Serverartefakt vom 2026-08-09 erzeugen und
   versiegeln.
4. Server mit exakt diesem Setup neu starten.
5. Versiegelten 25-Sekunden-Preflight ohne Person und ohne Bewegung ausführen.

Erst ein bestandener Preflight gibt die 65-Sekunden-Leerraumkalibrierung frei.

# Hardware

[English](README.en.md)

Der WLAN-CSI-Aufbau besteht aus fünf ESP32-S3-Boards. Ein separates
ESP32-C3-Board bindet den HLK-LD2450 als unabhängigen mmWave-Referenzsensor an.

## Referenzsensor

<img src="../images/hlk-ld2450-mmwave-sensor.jpeg" alt="HLK-LD2450 24G mmWave-Referenzsensor" width="560">

## Platinen

PCB-01 verbindet den ESP32-C3 mit dem mmWave-Referenzpfad. Die folgende
Fertigungsvorschau dokumentiert den früheren Platinenstand:

<img src="../images/pcb-01-preview.webp" alt="Fertigungsvorschau von PCB-01 mit ESP32-C3-Footprint, C1, C2 und Anschluss U2" width="560">

Die [Gerber- und Bohrdaten von PCB-01](pcb-01/) liegen mit SHA-256 und
Fertigungshinweis im Repository.

Die überarbeitete [PCB-02 mit KiCad-Quellen, Prüfberichten und Bestellarchiv](pcb-02/)
verwendet wieder SMD-Kondensatoren, Standard-Pinheader-Pads für den ESP32-C3
und dieselben Außenmaße sowie Montagebohrungen wie PCB-01.

> [!IMPORTANT]
> Für den aktuellen Aufbau ist ausdrücklich **PCB-02** zu verwenden. PCB-01 bleibt als frühere Fertigungsvorschau dokumentiert und ist nicht die vorgesehene aktuelle Revision.

## Breadboard-Aufbau

Die folgende Gegenüberstellung zeigt die aktuelle PCB-02-Revision neben dem
vorläufigen Breadboard-Aufbau:

<table>
<tr>
<td><img src="pcb-02/preview/PCB-02-top.png" alt="PCB-02-Top-Ansicht mit USB-Anschluss, U1, C1 und C2" width="460"><br><strong>PCB-02 — aktuelle Revision</strong><br>Diese überarbeitete Platine ist für den aktuellen mmWave-Aufbau zu verwenden.</td>
<td><img src="../images/mmwave-breadboard-setup.jpeg" alt="Vorläufiger Breadboard-Aufbau mit HLK-LD2450 und ESP32-C3" width="460"><br><strong>Vorläufiger Breadboard-Aufbau</strong><br>Das Foto dokumentiert den provisorischen mmWave-Aufbau auf dem Breadboard.</td>
</tr>
</table>

Alle Dateien und Bezugsangaben stehen auf der separaten
[Breadboard-Seite](breadboard/README.md): STL, Montagefoto, Heat-Set-Insert,
Installation Tip, Holzschrauben und die beiden mmWave-Kondensatoren.

Die Seite dokumentiert außerdem die verwendeten Mengen und die noch offenen
Validierungen zu Druck, Einpress-Temperatur und Passprobe.

## ESP32-S3-Gehäuse

Als Gehäuse für die ESP32-S3-Boards ist das externe MakerWorld-Modell
[*ESP32 S3 Wroom Case*](https://makerworld.com/de/models/1456361-esp32-s3-wroom-case#profileId-1517915)
von MakerWorld-Nutzer [`aiekick`](https://makerworld.com/de/@aiekick) vorgesehen.

Das Modell steht laut Quellseite unter der **MakerWorld Standard Digital File
License**. Die STL wird deshalb nicht erneut in diesem Repository bereitgestellt.
Weitere Hinweise stehen unter [`esp32-s3-case/`](esp32-s3-case/).

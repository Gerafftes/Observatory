# Hardware

[Deutsch](README.md)

The WiFi CSI setup consists of five ESP32-S3 boards. A separate ESP32-C3 board
connects the HLK-LD2450 as an independent mmWave reference sensor.

## Reference sensor

<img src="../images/hlk-ld2450-mmwave-sensor.jpeg" alt="HLK-LD2450 24 GHz mmWave reference sensor" width="560">

## PCBs

PCB-01 connects the ESP32-C3 to the mmWave reference path. The following
manufacturing preview documents the earlier board revision:

<img src="../images/pcb-01-preview.webp" alt="PCB-01 manufacturing preview with ESP32-C3 footprint, C1, C2, and connector U2" width="560">

The [PCB-01 Gerber and drill files](pcb-01/) are available with a SHA-256
checksum and manufacturing note.

The revised [PCB-02 with KiCad sources, validation reports, and ordering archive](pcb-02/)
restores the SMD capacitors, uses standard pin-header pads for the ESP32-C3,
and keeps the PCB-01 outer dimensions and mounting-hole positions.

> [!IMPORTANT]
> **PCB-02 is the required revision for the current setup.** PCB-01 remains documented as the earlier manufacturing preview and is not the current target revision.

## Breadboard setup

The comparison below shows the current PCB-02 revision next to the temporary
breadboard setup:

<table>
<tr>
<td><img src="pcb-02/preview/PCB-02-top.png" alt="PCB-02 top view with USB connector, U1, C1, and C2" width="460"><br><strong>PCB-02 — current revision</strong><br>This revised board is the one to use for the current mmWave setup.</td>
<td><img src="../images/mmwave-breadboard-setup.jpeg" alt="Temporary breadboard setup with HLK-LD2450 and ESP32-C3" width="460"><br><strong>Temporary breadboard setup</strong><br>The photo documents the provisional mmWave wiring on the breadboard.</td>
</tr>
</table>

All files and sourcing references are collected on the separate
[breadboard page](breadboard/README.en.md): STL, setup photo, heat-set insert,
installation tip, wood screws, and both mmWave capacitors.

The page also documents the used quantities and the still-open validation items
for printing, insertion temperature, and fit.

## ESP32-S3 enclosure

The external MakerWorld model
[*ESP32 S3 Wroom Case*](https://makerworld.com/de/models/1456361-esp32-s3-wroom-case#profileId-1517915)
by MakerWorld user [`aiekick`](https://makerworld.com/de/@aiekick) is intended as
an enclosure for the ESP32-S3 boards.

The source page lists the model under the **MakerWorld Standard Digital File
License**, so the STL is not redistributed in this repository. See
[`esp32-s3-case/`](esp32-s3-case/) for the usage note.

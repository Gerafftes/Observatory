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

The [breadboard CAD file (`Breadboard-Body.stl`)](breadboard/Breadboard-Body.stl)
is included in the repository. Print orientation, material, and fit have not
yet been validated; additional breadboard documentation will follow when
available.

### Fastening hardware used

- Heat-set insert `M1.6 × 2.5`
- Installation tip `M1.6 × 1`
- QUARKZMAN wood screws, `M1.6 × 8 mm`, slotted round head, brass,
  self-tapping (`40 pieces`)

These details document the hardware used with this model. Print settings,
insertion temperature, and fit have not yet been validated.

### Breadboard mmWave components

| Part | Quantity | Price |
|---|---:|---:|
| **X7R-2.5 100N MUR**<br>100 nF multilayer ceramic capacitor, 50 V, X7R, 10%, 2.5 mm pitch, ammo | 1 | €0.10 |
| **FC-A 10U 50**<br>Radial electrolytic capacitor, 10 µF, 50 V, 2.5 mm pitch, 105 °C, 1000 h, 20% | 1 | €0.15 |

## ESP32-S3 enclosure

The external MakerWorld model
[*ESP32 S3 Wroom Case*](https://makerworld.com/de/models/1456361-esp32-s3-wroom-case#profileId-1517915)
by MakerWorld user [`aiekick`](https://makerworld.com/de/@aiekick) is intended as
an enclosure for the ESP32-S3 boards.

The source page lists the model under the **MakerWorld Standard Digital File
License**, so the STL is not redistributed in this repository. See
[`esp32-s3-case/`](esp32-s3-case/) for the usage note.

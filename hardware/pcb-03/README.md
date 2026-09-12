# PCB-03 – ESP32-C3 / LD2450 carrier

PCB-03 is the final layout revision based on PCB-02. The LD2450 connector and
its UART mapping are rotated by 180° so the sensor is aligned with the intended
board orientation:

- ESP GPIO21/TX → LD2450 RX
- ESP GPIO20/RX ← LD2450 TX
- C1 (100 nF) and C2 (10 µF) remain SMD parts.
- F.Cu and B.Cu contain GND planes; the antenna keep-out is marked `ANT KEEP CLEAR`.
- The PCB keeps the 26.45 × 36.42 mm outline and the two case-compatible holes
  at (163.50, 89.00) mm and (172.00, 89.00) mm.

![PCB-03 top preview](preview/PCB-03-top.png)

## Ordering and validation

- [PCB-03 Gerber and Excellon ordering archive](PCB-03-order-ready.zip)
- [KiCad PCB source](PCB-03.kicad_pcb)
- DRC: 0 violations and 0 unconnected items.

The design-file checks do not replace testing of the manufactured PCB and the
assembled ESP32-C3/LD2450 hardware.

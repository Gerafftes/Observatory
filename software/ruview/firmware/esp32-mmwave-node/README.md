# ESP32-C3 HLK-LD2450 calibration/reference node

Firmware for the `PCB-01` carrier with an ESP32-C3 Super Mini and HLK-LD2450.
It streams all three raw radar target slots over UDP and exposes authenticated
WiFi OTA on the same port and paths used by the RuView RX nodes.

## Hardware verified from PCB-01

- LD2450 TX -> ESP32 GPIO20 (UART1 RX)
- ESP32 GPIO21 (UART1 TX) -> LD2450 RX
- LD2450 supply: 5 V, at least 200 mA
- LD2450 UART: 256000 baud, 8 data bits, no parity, 1 stop bit

The ESP32 console is deliberately routed to USB Serial/JTAG. Do not change it
back to UART0: GPIO20/21 are connected to the radar on this PCB and boot logs on
GPIO21 could be interpreted as radar commands.

## Measurement contract

Every valid 30-byte radar frame becomes one UDP JSON packet using schema
`ruview.mmwave.ld2450.v1`. It contains:

- device boot ID, sequence and monotonic microsecond timestamp;
- NTP Unix timestamp when available (`0` until synchronized);
- explicit `calibration` or `reference` mode;
- all three unfiltered sensor slots in local millimetres;
- transformed room X/Z coordinates and the transform parameters.

The LD2450 slots are not persistent person identities. Consumers must not treat
slot 1, 2 or 3 as a stable track ID.

Changing the mode takes effect immediately in subsequent UDP packets and is
persisted across restarts.

`calibration` data may label synchronized WiFi-CSI training samples. For a blind
accuracy run, first freeze the WiFi model, switch the node to `reference`, and
record with `--expected-mode reference`. The receiver fails immediately if the
mode is wrong, helping prevent reference leakage into training.

## Configure and build

Install ESP-IDF 5.4, then:

```sh
idf.py set-target esp32c3
idf.py menuconfig
idf.py build
idf.py -p /dev/cu.usbmodem... flash monitor
```

Under **RuView mmWave node**, set the WiFi, UDP collector, stable node ID, a
strong OTA token, and the radar-to-room transform. The initial USB flash is
required once. Later app updates use WiFi OTA and preserve NVS/mode.

Transform definition:

- raw `x_mm`: the LD2450 signed lateral coordinate;
- local `y_mm`: forward from the radar;
- room `x`: room length axis;
- room `z`: room width axis;
- yaw: direction of radar-forward, measured from room +X toward room +Z.

Set **Invert raw radar X** from a known left/right floor mark so transformed
local-right has the correct sign. Do not infer that sign from the UI drawing or
from whether the sensor PCB is mounted face-up or face-down.

Validate the transform against at least 2-3 measured floor marks before using
the radar as an accuracy reference.

## Operate and update over WiFi

```sh
python3 tools/ota_update.py 192.0.2.60 --status
python3 tools/ota_update.py 192.0.2.60 --token "$MMWAVE_OTA_PSK" --mode calibration
python3 tools/ota_update.py 192.0.2.60 --token "$MMWAVE_OTA_PSK" --mode reference
python3 tools/ota_update.py 192.0.2.60 --token "$MMWAVE_OTA_PSK" \
  --firmware build/esp32-mmwave-node.bin
```

Endpoints on port 8032:

- `GET /ota/status` (read-only)
- `PUT /mode` with body `calibration` or `reference` (Bearer token)
- `PUT /transform` with JSON `origin_x_mm`, `origin_z_mm`, `yaw_mdeg`, and
  `raw_x_inverted` (Bearer token)
- `POST /ota` with the app binary (Bearer token)

Mode, transform, and OTA writes fail closed when no token was configured. The
transform is committed to NVS, so moving the node does not require rebuilding
the firmware. The transport is plain HTTP on
the isolated experiment LAN; do not expose port 8032 to the internet.

The read-only status includes cumulative `uart_bytes_received`,
`radar_frames_valid`, `udp_packets_sent`, and `udp_send_failures` counters. They
separate an idle or incorrectly wired UART from parser failures and UDP delivery
problems without changing the measurement packet schema.

## Record

```sh
python3 tools/mmwave_receiver.py --expected-mode calibration \
  --require-single-target \
  --output calibration-01.mmwave.jsonl
python3 tools/mmwave_receiver.py --expected-mode reference \
  --require-single-target \
  --output blind-reference-01.mmwave.jsonl
```

The recorder adds the host receive timestamp required to align radar frames
with CSI captured on the same Mac. Preserve both the sensor and host timestamps;
UDP receive time alone includes network jitter.

Zero-target frames are retained because the LD2450 can lose a person who stops
moving. More than one target aborts the recommended single-person recording;
that avoids silently assigning CSI to the wrong radar slot.

## Parser test

```sh
sh test/run_tests.sh
```

The test includes the official HLK-LD2450 protocol example (`x=-782 mm`,
`y=1713 mm`, speed `-16 cm/s`) and stream resynchronization.

## Primary hardware references

- [Hi-Link HLK-LD2450 product and downloads](https://www.hlktech.com/Goods-226.html)
- [Hi-Link HLK-LD2450 manual V1.02](https://h.hlktech.com/download/HLK-LD2450-24G/1/HLK%20LD2450%201T2R%E8%BF%90%E5%8A%A8%E7%9B%AE%E6%A0%87%E6%A3%80%E6%B5%8B%E8%BF%BD%E8%B8%AA%E6%A8%A1%E7%BB%84%E8%AF%B4%E6%98%8E%E4%B9%A6%20V1.02%20.pdf)

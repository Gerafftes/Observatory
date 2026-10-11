# ESP32-C3 HLK-LD2450 calibration/reference node

Firmware for ESP32-C3 Super Mini / HLK-LD2450 reference nodes. The current
carrier design is [PCB-03](../../../../hardware/pcb-03/README.md); the UART
wiring was first verified on PCB-01. The firmware streams all three raw radar
target slots over UDP and exposes authenticated WiFi OTA on the same port and
paths used by the RuView RX nodes.

## Hardware verified from PCB-01

- LD2450 TX -> ESP32 GPIO20 (UART1 RX)
- ESP32 GPIO21 (UART1 TX) -> LD2450 RX
- LD2450 supply: 5 V, at least 200 mA
- LD2450 UART: 256000 baud, 8 data bits, no parity, 1 stop bit

The ESP32 console is deliberately routed to USB Serial/JTAG. Do not change it
back to UART0: GPIO20/21 are connected to the radar on this PCB and boot logs on
GPIO21 could be interpreted as radar commands.

## Onboard identity LED

After boot, the firmware drives the ESP32-C3 Super Mini's active-low blue LED
on GPIO8 from the configured stable node ID:

- `MMWAVE1`: one short pulse every three seconds;
- `MMWAVE2`: two short pulses every three seconds;
- any other or missing node ID: continuous fast warning blink.

The red power LED remains hardware-controlled. GPIO8 is configured only after
startup so the firmware does not change the ESP32-C3 boot-strapping state.

## Measurement contract

Every valid 30-byte radar frame becomes one UDP JSON packet using schema
`ruview.mmwave.ld2450.v1`. It contains:

- device boot ID, sequence and monotonic microsecond timestamp;
- NTP Unix timestamp when available (`0` until synchronized);
- explicit `calibration` or `reference` mode;
- all three unfiltered sensor slots in local millimetres;
- legacy transformed room X/Z coordinates and the transform parameters.

For the RuView sensing server, `node_id` plus the raw local coordinates are
authoritative. The server applies the matching setup-profile transform for
that node; the transformed fields remain in the packet for compatibility with
older collectors and standalone diagnostics. Saving a new server profile does
not require writing a transform back to the ESP.

The LD2450 slots are not persistent person identities. Consumers must not treat
slot 1, 2 or 3 as a stable track ID.

Changing the mode takes effect immediately in subsequent UDP packets and is
persisted across restarts.

`calibration` data may label synchronized WiFi-CSI training samples. For a blind
accuracy run, first freeze the WiFi model, switch the node to `reference`, and
record with `--expected-mode reference`. The receiver fails immediately if the
mode is wrong, helping prevent reference leakage into training.

## MMWAVE1 and MMWAVE2 upload

Both nodes use the same [firmware source](main/) (entry point:
[app_main.c](main/app_main.c)), [configuration options](main/Kconfig.projbuild), and
[OTA upload tool](tools/ota_update.py). The compiled node ID is different, so
build a separate image for each node. No device-specific `.bin` files are
committed to this repository.

Install ESP-IDF 5.4, activate its environment, and run these commands from this
directory. Each build keeps its own `sdkconfig` and binary under the ignored
`build/` directory; an existing project-root `sdkconfig` is left untouched.
In **RuView mmWave node** in each `menuconfig`, set the WiFi credentials, the
current UDP collector address and port (`55010`), a strong OTA token, and the
**Stable node ID** shown below. Use the token shared with the sensing server if
collector discovery and automatic target correction are needed. Do not copy one
node's image to the other node.

| Node | Stable node ID in `menuconfig` | App image after `build` |
| --- | --- | --- |
| MMWAVE1 | `MMWAVE1` | `build/mmwave1/esp32-mmwave-node.bin` |
| MMWAVE2 | `MMWAVE2` | `build/mmwave2/esp32-mmwave-node.bin` |

### MMWAVE1: configure, build, and initial USB flash

```sh
idf.py -B build/mmwave1 -DSDKCONFIG=build/mmwave1.sdkconfig -DIDF_TARGET=esp32c3 menuconfig
idf.py -B build/mmwave1 -DSDKCONFIG=build/mmwave1.sdkconfig build
idf.py -B build/mmwave1 -DSDKCONFIG=build/mmwave1.sdkconfig -p /dev/cu.usbmodem... flash monitor
```

### MMWAVE2: configure, build, and initial USB flash

For the MMWAVE2 board diagnosed on 2026-09-26, enable **Limit WiFi TX power**
and set **Maximum WiFi TX power** to `40` (quarter-dBm units, 10 dBm). The
high/low/high SoftAP comparison reproduced failure at `78` and successful
association at `40`; see the [diagnostic report](../../../../experiment-reports/mmwave2-wifi-20260926.md)
and [standalone AP probe](tools/ap_probe/README.md). This option is off by
default for other boards. Preserve it in MMWAVE2's private build configuration
when rebuilding; it is applied before the first association attempt.

```sh
idf.py -B build/mmwave2 -DSDKCONFIG=build/mmwave2.sdkconfig -DIDF_TARGET=esp32c3 menuconfig
idf.py -B build/mmwave2 -DSDKCONFIG=build/mmwave2.sdkconfig build
idf.py -B build/mmwave2 -DSDKCONFIG=build/mmwave2.sdkconfig -p /dev/cu.usbmodem... flash monitor
```

Replace `/dev/cu.usbmodem...` with the port of the intended ESP32-C3 and
connect only that node for the USB flash. `flash` writes the build's bootloader,
partition table, OTA initialization data, and app; reserve it for initial
provisioning or a deliberate full USB reflash. Check the node ID in the boot
log and the corresponding LED pattern before connecting the other node.

The radar-to-room transform remains available for standalone diagnostics and
backward-compatible packets, but the RuView server uses the saved setup
profile as its source of truth. Later app updates can use WiFi OTA with the
matching node image and preserve the node's NVS/mode.

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

Find each node's current IP address on the experiment WLAN. Read its status
first and confirm `node_id` before uploading the matching app image. Set
`MMWAVE_OTA_PSK` to the shared server/node token in your shell. Replace each
documentation IP below with the current address before running it.

For MMWAVE1, run the status command and verify `node_id` is `MMWAVE1`:

```sh
MMWAVE1_IP=192.0.2.60
python3 tools/ota_update.py "$MMWAVE1_IP" --status
```

Only then upload the MMWAVE1 app:

```sh
python3 tools/ota_update.py "$MMWAVE1_IP" --token "$MMWAVE_OTA_PSK" \
  --firmware build/mmwave1/esp32-mmwave-node.bin
```

For MMWAVE2, run the status command and verify `node_id` is `MMWAVE2`:

```sh
MMWAVE2_IP=192.0.2.61
python3 tools/ota_update.py "$MMWAVE2_IP" --status
```

Only then upload the MMWAVE2 app:

```sh
python3 tools/ota_update.py "$MMWAVE2_IP" --token "$MMWAVE_OTA_PSK" \
  --firmware build/mmwave2/esp32-mmwave-node.bin
```

The token must match the selected node. For server-managed discovery and
automatic target correction, both nodes must use the token configured as
`MMWAVE_NODE_TOKEN` on the server. After each update, read `/ota/status` again
and verify the node ID, target, and increasing UART/radar/UDP counters. An HTTP
response alone does not prove that the LD2450 is sending frames.

To change a node's measurement mode without uploading firmware:

```sh
python3 tools/ota_update.py "$MMWAVE1_IP" --token "$MMWAVE_OTA_PSK" --mode calibration
python3 tools/ota_update.py "$MMWAVE1_IP" --token "$MMWAVE_OTA_PSK" --mode reference
```

Use `"$MMWAVE2_IP"` for MMWAVE2 instead.

Endpoints on port 8032:

- `GET /ota/status` (read-only)
- `PUT /mode` with body `calibration` or `reference` (Bearer token)
- `PUT /transform` with JSON `origin_x_mm`, `origin_z_mm`, `yaw_mdeg`, and
  `raw_x_inverted` (Bearer token)
- `PUT /transport` with JSON `target_host` and `target_port` (Bearer token)
- `POST /ota` with the app binary (Bearer token)

Mode, transform, and OTA writes fail closed when no token was configured. The
transform is committed to NVS, so moving the node does not require rebuilding
the firmware. The transport is plain HTTP on
the isolated experiment LAN; do not expose port 8032 to the internet.

The read-only status includes cumulative `uart_bytes_received`,
`radar_frames_valid`, `udp_packets_sent`, `udp_send_failures`,
`udp_packets_acked`, `udp_retransmissions`, and `udp_ack_timeouts` counters.
They separate an idle or incorrectly wired UART, local `sendto()` failures, and
loss after local queueing without changing the measurement packet schema.

The default stream interval is 50 ms. The LD2450 itself normally reports at
10 Hz, so this interval is only an upper bound and cannot create additional
radar measurements. WiFi modem sleep is disabled by default for the
mains-powered node so DTIM wake-ups do not add receive latency. Each packet is
acknowledged by the collector; an unacknowledged packet is retried with the
same boot ID and sequence up to eight times. The end-to-end retransmission
timeout uses 150 ms as both its initial value and the minimum for this WLAN
path, adapts upward from ACK round trips, and preserves bounded backoff after
a successful retry as a learned floor for the current boot session. Fast
median ACKs therefore cannot erase the measured WLAN tail. The server
deduplicates a retransmission if only the ACK was lost. The
attempt limit and initial/minimum timeout are configurable in **RuView mmWave
node**. The learned timeout is capped at a configurable 2 s so a collector
outage remains bounded.
The server can use the authenticated `/transport` endpoint to repair a changed
collector IP/port and persists that target in NVS.

ACKs must come from the configured collector IP and port. On a host with
multiple addresses, use the address selected for replies towards the node:
a wildcard UDP listener may receive through an IP alias but reply through the
interface's primary address. That mismatch causes ACK timeouts despite server
reception; correct the target through `/transport` and retain ACK validation.

The node also keeps its WiFi interface in the normal DHCP-client mode. While
radar frames are streaming it broadcasts an authenticated collector-discovery
request on UDP port `5011` every two seconds. A RuView sensing server answers
with the current collector address and UDP port, authenticated with the same
OTA token. The node accepts the answer only for its own node ID and outstanding
nonce, then persists the new target in NVS. This means the Mac can stay on
DHCP: its address may change without manually editing the node target. The
discovery listener is enabled when the server has `MMWAVE_NODE_TOKEN`
configured; it is deliberately disabled without the shared secret.

The ESP32-C3 Super Mini user LED identifies the configured node without using
its IP address: `MMWAVE1` emits one short pulse every three seconds and
`MMWAVE2` emits two. An unknown node ID uses a fast warning blink and is not
silently assigned. GPIO 8 and active-low drive are configurable for boards
whose onboard LED is wired differently.

Disabling modem sleep increases power consumption. If it is re-enabled for a
battery deployment, expect the access point's DTIM/listen interval to become a
possible lower bound on receive latency; compare the server's transport delay
and sequence-loss counters before choosing that trade-off.

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

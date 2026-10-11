# MMWAVE2 SoftAP long-run probe

ESP-IDF v5.4, ESP32-C3. This isolated app advertises `MMWAVE2-RF-TEST`
with WPA2, required PMF, channel 1 and one client. It does not start radar,
STA association, Bluetooth, OTA HTTP endpoints or the measurement stream.
The initial RF failure and power-limit experiment are documented in
[`experiment-reports/mmwave2-wifi-20260926.md`](../../../../../../experiment-reports/mmwave2-wifi-20260926.md).

## Build privately

Keep the sdkconfig, build, credentials, serial logs and device backups under the
repository's ignored `private/` directory. Confirm the ignore rule and absence
of tracked files first. `CONFIG_AP_PROBE_PASSWORD` must contain 8-63 characters;
an empty/invalid password does not start an open AP. Never publish sdkconfig
or images containing the test credential.

From the repository root, after exporting the ESP-IDF environment:

```sh
idf.py -C software/ruview/firmware/esp32-mmwave-node/tools/ap_probe \
  -B "$PWD/private/ap-probe/build" \
  -D SDKCONFIG="$PWD/private/ap-probe/sdkconfig" menuconfig
idf.py -C software/ruview/firmware/esp32-mmwave-node/tools/ap_probe \
  -B "$PWD/private/ap-probe/build" \
  -D SDKCONFIG="$PWD/private/ap-probe/sdkconfig" build
```

Set the private password, `AP_PROBE_CHANNEL` and `AP_PROBE_TX_POWER` in
menuconfig. TX power uses the API's quarter-dBm units: 40 requests 10 dBm;
78 requests 19.5 dBm. Read back the actual configured limit from `STATE`.
This is a limit, not a measurement of radiated power.

Before a separately authorized device write, identify the chip and running
partition, preserve the current image and confirm that this partition table
matches the device. On the diagnosed board, ota_0 is at `0x20000`; only that
app image was written. Do not use a blanket `flash` command on another layout,
erase NVS, replace bootloader/OTA metadata, or modify security settings.
The probe preserves rollback handling for an image pending verification.

## Evidence

- `BOOT`: reset reason, exact IDF version, chip/revision, image SHA and slot.
  USB serial opening/reset can itself create `ESP_RST_USB` (11); distinguish
  this controlled reset from spontaneous reboots.
- `AP_START`/`AP_STOP`, channel changes, client association/disassociation
  (including reason), and DHCP lease events.
- `HEARTBEAT` every 5 seconds: cumulative event counters, probe-request RX count
  and last RSSI/age, plus acknowledgement of a test event through the event loop.
- `STATE`: checked mode/channel/power/client-list/DHCP API results, interface
  state and each connected client's RSSI. `MEMORY`: heap/minimum/largest block,
  task stack low watermark and heap integrity.
- No automatic WiFi restart or NVS erase masks the original fault. Probe RX
  events are counted without a per-packet log flood. Driver logs remain enabled.

Run the native checks and correlate their host timestamps with directed scans,
a real WPA2 association, DHCP and interface-bound ping traffic:

```sh
python3 software/ruview/firmware/esp32-mmwave-node/tools/ap_probe/check_log.py \
  private/ap-probe/serial.log --minimum-seconds 300 --require-client
```

The checker requires an observed boot and continuously advancing heartbeats.
It detects reported faults, reboots, stopped AP state and a stalled event loop.
A passing log cannot prove transmitted beacons or RF health. In particular,
`esp_wifi_start()`, `AP_START`, get-mode/get-channel and received probe requests
can all succeed while other devices cannot receive the AP.

Change one variable at a time. Compare high/low/high TX limits with the same
channel, security, supply and placement before attributing the result to any
specific antenna, regulator or silicon failure. Keep the host's LAN default
route and bind AP test traffic to WiFi. The AP shares the default 192.168.4.0/24
subnet with `csi-test`; verify the associated peer, not only a ping to `.1`.

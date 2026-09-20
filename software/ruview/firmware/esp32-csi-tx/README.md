# ESP32-S3 CSI transmitter (`TX1`)

This is the maintained source counterpart for the separate Arduino SoftAP
transmitter. It intentionally remains separate from `esp32-csi-node`, which is
receiver firmware.

The firmware knows only its stable identity `TX1`. It does not contain a room
position. RuView's active sealed setup maps `TX1` to a position and binds the
RX stream to the transmitter through the existing TX-filter identity.

At runtime it:

- shows the canonical TX1 amber (`#ff9f0a`) on the onboard RGB LED at limited
  brightness;
- starts the configured SoftAP on channel 6 with hostname `TX1`;
- emits the existing 32-byte `CSI_TX` UDP sounding packet every 20 ms on port
  4210; bytes 16-18 additionally carry the informational label `TX1`.

Copy `tx_config.example.h` to `tx_config.h`, enter the private experiment SSID
and password, select the ESP32-S3 board matching the installed hardware, and
compile with a current Espressif Arduino core. Do not flash over the inventoried
TX before taking a private full-flash backup and validating this replacement on
spare hardware: the currently deployed `43a8f6d` binary was read back, but its
original source tree was not present in this repository.

The host-side packet contract can be checked without Arduino tooling:

```sh
sh test/run_tests.sh
```

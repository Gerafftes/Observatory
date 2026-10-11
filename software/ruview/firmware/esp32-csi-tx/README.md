# ESP32-S3 CSI transmitter (`TX1`)

`main/` is the maintained ESP-IDF 5.4 implementation. The existing Arduino
reference (`esp32-csi-tx.ino`, `tx_packet.h`, `tx_config.example.h`) is retained
unchanged. Continue this firmware; do not replace the local source with an older
remote checkout. The internal TX1 evidence/migration plan and the
[local operator runbook](tools/README.md) define the hardware gates. The
internal plan is intentionally not part of the public source export.

## Offline sampling correction (2026-10-03)

The boot caller now waits for the remaining monotonic interval until the last
completed sample plus five seconds. Early tick wakeups cannot restart a full
five-second wait. Network getters (including their logs), heap and sender status
are collected in explicit order before the completion timestamp is read.
Only a due, executed probe advances the sample anchor. A real gap above six
seconds still fails; no catch-up samples, limit changes or priority changes.
The full local observation remains at least 60 seconds within the unchanged
120-second budget. Probe cost can extend observation; it cannot shorten it.
One timing record per poll includes due/wait/probe times and diagnostic ticks.
Tick wrap does not control the deadline. Regressing observed times fail closed.
The caller/bootcheck time regressions are simulations, not hardware validation.
This offline candidate has no write authorization; the hardware result below
and its independent radar/setup/server acceptance gaps remain unchanged.

## Latest recorded hardware run (2026-10-03)

The power-correction candidate, SHA-256
`7c93c98ea015b772263ae326ee4ed4cae0e212b774a087652e6d10571171073d`,
passed the recorded Country20/Power44 checks on TX1. It then stopped before
`BOOT_OK`: the first sampling block was skipped just before its five-second
threshold; the next poll reported 10,198,769 microseconds against the unchanged
6,000,000-microsecond limit. Why the first time was early remains unmeasured.
These power getters do not measure radiated power.

This run used one migration, three range writes and one candidate start.
One full restore passed an immediate independent 16-MiB byte/hash comparison
before one historical start. The required shared operating-return window
failed after 16 samples when MMWAVE2 reported two `room_bounds` rejections
with fresh transport. A separate 62-sample ingress capture completed with
zero kernel capture drops. The last individual observation showed fresh RX4
and both radars valid; both serial captures ended and their ports were free.
That observation does not replace the failed continuous return window.

LED, Mixed-V2 and seven-client acceptance were not reached. There is no
overall hardware PASS, pending restore, firmware change or new build in
this run. Exact frozen BIN/ELF/source bindings, actions and remaining evidence
are in the [TX-7B report](../../../../docs/project/tx1-7b-power-20261003.md).
The previous run below is retained as history with its own restore count.

## Previous semantic-check hardware run (2026-10-03)

The semantic-check candidate (`ab0610fd…2ae4a`) failed before `BOOT_OK`:
the country power field returned 20 dBm against the required 11 dBm, while
the separate power API correctly returned 44 quarter-dBm. Country bytes
`44 45 20` passed the semantic check. Both getters succeeded; `0x103` came
from the firmware comparison. The cause remains open; these are software
readings, not a measurement of radiated power. LED, Mixed-V2 and seven-client
acceptance of this candidate were not performed.

Recovery is now verified. The first full restore's readback was paused;
after a reported reconnection, a complete read differed in 25 NVS bytes.
One additional full restore of the same bound fresh backup was explicitly
authorized, bringing the total to two restore writes without an automatic
retry. Its direct independent 16-MiB byte/hash comparison passed before one
controlled historical UART0 boot. RX4 and both radars then passed a shared
62-sample, 61.063-second operating-return window with stable identities,
boot evidence and continuing data flow. The historical firmware is running;
capture ports are released. Recovery does not accept the new candidate.

No firmware repair or new build was part of recovery. Frozen source/build
exports retain their original hashes; this maintained documentation is a
later edit and does not redefine their receipts. Replacement-board initial
provisioning and hardware acceptance remain separate commissioning work.

## Offline power-contract correction (2026-10-03)

The frozen failing ELF explains the 44/20 pair: its ESP32-S3 `libnet80211`
country setter replaces the supplied power field with the DE table value 20,
while the later power setter changes separate PHY state. This contradicts
the pinned header's claimed power-to-country mapping. The current check now
requires that exact DE/1–13/MANUAL country contract and independently requires
44 quarter-dBm from the power API. Other country contracts fail closed.
Neither setter input, profile, channel rules nor SDK version was changed.

Two additional read-only snapshots record return codes, values and sequential
read times immediately after the country setter and after the power setter.
The pre-start snapshot never calls the post-start-only power getter. Getter
errors stop startup. Independent test observations cover 44/20, wrong power,
wrong country values, errors and state changes before DHCP readiness.

This demonstrated software comparison correction was subsequently exercised
by the exact candidate in the latest run above: its recorded power checks
passed, while its independent boot-timing gate failed. It has no overall
hardware PASS and provides no radiated-power measurement. The offline evidence
and original build bindings remain in
[the offline power investigation](../../../../docs/project/tx1-power-diagnostic-20261003.md).

## Hardware profile

The recorded TX1 board is an ESP32-S3 (QFN56, revision 0.2) with a 40 MHz
crystal, 16 MiB physical flash and 8 MiB PSRAM. The migration deliberately
uses only the retained logical 4 MiB flash layout below. Firmware configuration
targets `esp32s3`; it does not assume that unused physical flash is available.

The identity LED is one addressable WS2812 using GRB byte order on GPIO 48.
The fixed confirmation colour is amber `#ff9f0a` at 32/255 brightness. A
schema-1 profile may set the LED GPIO to `-1` only when a replacement board has
no compatible LED. The TX1 migration profile keeps GPIO 48 and does not invert
the signal.

## Existing TX1 migration versus a new board

These are separate deployment paths even though they share the maintained
source:

| Concern | Existing TX1 migration | New hardware rebuild |
|---|---|---|
| Source identity | Use the frozen candidate archive and its receipt; do not rebuild it from a later checkout. | Export and build the reviewed current checkout with `tools/build_release.py`. |
| Flash layout | Preserve the saved bootloader, partition table, app0 and data partitions; write only the plan's authorized ranges. | Establish and review a board-specific initial layout before first commissioning. |
| Identity and network | Bind the fresh double backup, factory-base MAC, actual SoftAP MAC and private schema-1 profile to the migration plan. | Generate new board and network values; never copy TX1's private profile or hardware identity. |
| NVS | Add the new namespaces to the freshly read original NVS with `provision.py`; never synthesize the old namespaces. | `provision.py` is not a blank-board factory provisioner. A separate reviewed initial-NVS procedure is still required. |
| Recovery | Restore the complete verified pre-migration image through the plan-bound USB workflow. | Define recovery from that board's own initial image and boot layout. |
| Acceptance | Use the frozen Mixed-V2 baseline and the TX1-specific live checklist. | Repeat board bring-up and radio/CSI acceptance as new hardware evidence. |

The public source/build is therefore reproducible without TX1 secrets. A
turnkey replacement-board deployment is not yet claimed: initial NVS creation,
board-specific pin/power review and new-board hardware acceptance remain a
separate commissioning task.

## Existing TX1 migration profile

The compiled table matches the **saved** old layout: NVS `0x9000/0x5000`, OTA
data `0xe000/0x2000`, app0 `0x10000/0x140000`, app1 `0x150000/0x140000`, SPIFFS
`0x290000/0x160000`, coredump `0x3f0000/0x10000`. Physical flash is 16 MiB; this
profile retains the old logical 4 MiB layout. The candidate targets app1.
**The built bootloader and partition-table files are not migration write inputs.**
Do not use generic `idf.py flash`, merged-image flashing or erase-flash here.

Configuration is required, versioned and private. There is no fallback SSID or
open diagnostics AP. `tx_cfg/schema_v = 1` requires separate factory-base and
actual SoftAP MACs, credentials and explicit PHY settings; `security/ota_psk`
is a 64-character lowercase hex bearer secret. Invalid/missing configuration
stops without AP or sounding. The public `main/tx_config.h` contains types and
constants, not device values; only that exact path is exempted from ignoring
private `tx_config.h` files.

NVS is mounted with a persistent **read-only partition descriptor**, including
SDK initialization/recovery paths. There is no automatic NVS erase. WiFi NVS,
saved PHY calibration, partition PHY initialization data, flash coredumps,
encryption, Secure Boot and anti-rollback are disabled and guarded at compile
time. WiFi uses RAM storage before configuration. This changes no eFuses.
The offline importer adds new namespaces only, preserves all old written
entries byte-for-byte and keeps an erased reserve page. It refuses existing
target namespaces, insufficient capacity, corruption and ambiguous recovery;
it is not a general-purpose NVS editor.

## Network, wire and boot contracts

The productive migration profile requires privately supplied `csi-test`
credentials and the already bound actual SoftAP MAC. A factory-base MAC check
prevents provisioning the profile onto another board. Rebuilders supply their
own network and device identities. RX filters, server identity checks, setup
metadata and the host alias are not changed by this TX firmware.

The AP uses `192.168.4.1/24`, WPA2 with optional PMF and at most eight stations
(host, four RX, two independent mmWave nodes plus one spare). Its DHCP pool is
explicitly `.2` through `.49`, installed **before** radio start. `.50` is
reserved for the existing host alias; the Mac retains DHCP. No routing, NAT or
Internet uplink is added. The private migration profile explicitly selects
Germany, channel 6, HT40 with the secondary channel below, b/g/n protocol mask,
100-TU beacons and 50-Hz sounding. The configured TX ceiling is 11 dBm (SDK
value 44), retained as the reference target; exact old runtime power and
radiated power are not inferred from that setting. Both the pre-start PHY limit
and the post-start getter are checked locally.

The sounding contract remains 32 bytes with explicit little-endian encoding:

| Bytes | Meaning |
|---|---|
| 0–5 | `CSI_TX` |
| 6 / 7 | version 2 / transmitter role 2 |
| 8–11 / 12–15 | sequence / boot milliseconds, uint32 LE |
| 16–19 | `TX1` and terminating zero |
| 20–27 | nonzero random boot ID, uint64 LE |
| 28 / 29 | channel / rate Hz |
| 30–31 | reserved zero |

The existing timer/sender implementation sends one current packet per wakeup,
counts skipped deadlines and avoids catch-up bursts. The WS2812/NVS LED function
is retained: amber `#ff9f0a`, 32/255 brightness, WS2812/GRB, explicit GPIO 48
on TX1 or `-1` for a replacement board without a compatible LED. Room position
remains in the setup model.

The diagnostic candidate logs `TX_BOOT APP_MAIN_ENTER` with reset reason,
the manifest source identity, `BEGIN` operation markers, `PROFILE_OK`,
`NETIF_OK`, `WIFI_INIT_OK`, `AP_CONFIG_OK`, `AP_STARTED`, `DHCP_OK`,
`STATUS_OK`, `SOUNDING_STARTED`, `UDP_OK`, `LED_OK` and `BOOT_OK`.
Errors include a constant stage name and numeric `code=0x...`. Network and
boot comparison errors also identify the operation, field, expected/actual
numeric values and sample time. Credentials remain redacted. `UDP_OK` means the first complete datagram was queued locally; it is
not an RX or RF acceptance claim. A failed network getter identifies its
contract field before boot cleanup. Netif setup uses the same default SDK
steps with explicit error returns instead of the convenience helper's aborts.

Country validation keeps the schema-1 two-letter C string separate from the
SDK's third environment octet: ESP-IDF 5.4 preserves `I`, `O`, `X` and treats
other third octets as space. The check applies that exact mapping, compares
both ISO bytes, range, manual policy and the 11-dBm limit independently, and
logs all three raw bytes in hex. The power API's 44 quarter-dBm maps to the
country getter's 11 dBm. No radio setting changes to obtain a pass.
SSID comparison accepts the SDK's explicit length or valid terminated
`ssid_len=0` representation; unused bytes are not part of the name. Invalid
lengths, missing terminators and credential mismatches still stop the boot.
All safe network getters are sampled before returning the first error;
failed getter values are marked unavailable. Status identity getter failures
return HTTP 500 instead of an invented OTA state or zero image digest; a
successfully read `UNDEFINED` state remains valid. Sequential samples and current
channel negotiation remain distinct from the configured interface profile.

The LED now runs once in its own low-priority worker after AP, status and
sounding start. The main task waits at most 1000 ms. A blocked RMT refresh
reports `LED_TIMEOUT`; it cannot hold the main boot path. The worker is not
forcibly deleted while it might hold driver resources. A late `LED_OK` can
still follow a timeout. LED failure does not disable networking or bypass the
unchanged 60-second boot check. Colour, GPIO, inversion and disable semantics
are preserved; one stuck worker can retain its stack/driver resources until
reset. `LED_DISABLED` is distinct from a successful LED refresh.

UART0 at 115200 is the primary diagnostic console, including early
application/panic output, with a three-second printed-panic reboot delay.
Native USB Serial/JTAG receives secondary application output.
Brownout detection, the watchdog and all persistence/security guards remain
enabled as before. The existing on-device bootloader is not replaced by this
app-only build, so its console routing is unchanged. For the next separately
authorized **instrumented boot diagnosis**, arrange continuous capture before
the single start, including UART0 for the retained bootloader and app, and
native USB as the second record. A missing `APP_MAIN_ENTER` requires the ROM/bootloader
trace; application checkpoints cannot diagnose code that never executes.
Use the matching ELF/receipt for panic addresses. Do not equate a captured
checkpoint, `BOOT_OK`, or LED colour with Mixed-V2 hardware acceptance.

The RX-side acceptance contract is **Mixed-V2**. Valid 64- and 128-subcarrier
CSI frames are counted separately and neither path may be silently reclassified
or relaxed. The TX wire packet does not encode a subcarrier count; RX/server
evidence, packet capture and counter deltas establish the two receive paths.
The fixed acceptance sequence is 60 seconds of warm-up followed by three
consecutive 120-second windows against the frozen baseline named in the
operator plan. Exact criteria, hashes and failure-preservation rules live in
the [operator runbook](tools/README.md); a local build is no substitute for
that live evidence.

`GET /status` on port 8032 exposes source/image identity, local boot state and
sender counters, never passwords, OTA tokens or NVS contents. A local boot
check observes sender progress, network getters and heap for 60 seconds within
a 120-second boot budget; it requires neither cloud nor any RX. Failure stops
the sender/radio. In this migration profile it does **not** mark OTA state,
change boot selection or reboot into a presumed rollback image.

`POST /ota` checks the bearer token before any write, bounds image size and
upload time (120 seconds), and only selects a fully validated image. app0 is
protected: on a migrated app1, an upload targeting original app0 is rejected.
Thus first migration and its verified recovery use the separately authorized
USB workflow. The optional `TX_VERIFIED_ROLLBACK` path is disabled; enabling it
requires evidence about the retained bootloader, both apps and shared data.
A build or RX4's past `VALID` state is no TX rollback proof.

## Reproducible rebuild from a clean checkout

The public TX source set uses ESP-IDF commit
`67c1de1eebe095d554d281952fde63c16ee2dca0` (tag `v5.4`) and the committed
Component Manager lock for `espressif/led_strip` 3.0.3. The generated lock is
part of the source contract; do not edit it by hand. Build from a Git clone
with its `.git` history intact; the helper binds that exact commit and every
exported file into `source_sha256`, then embeds the matching 12-character
commit in the runtime `git_hash` field. A source-only archive is refused. A new
checkout needs a network connection once to install the IDF tools and resolve
the locked component. The resolver verifies the component against the lock;
subsequent builds can use the local component cache. ESP-IDF documents its S3 tool setup
[here](https://docs.espressif.com/projects/esp-idf/en/v5.4/esp32s3/get-started/linux-macos-setup.html),
and the lock-file contract
[here](https://docs.espressif.com/projects/idf-component-manager/en/latest/reference/dependencies_lock.html).

An exact rebuild of an already frozen candidate uses that candidate's source
archive and `sources.json`. The export helper hashes documentation and operator
tools as well as firmware sources, so a later checkout can produce a different
embedded source identity even when the firmware logic has not changed. Keep
the frozen archive with the firmware artifact when preparing a release.
Building the current checkout creates a new source identity and is suitable for
publication review or a future new-board build; it must not silently replace
the candidate already bound to the existing TX1 migration plan.

These steps assume a normal Git clone of this BLL repository. Start the
commands below at that checkout's repository root; do not use a source archive.

From a clean machine, first clone and install the exact toolchain. Choose a
new path if `~/esp/esp-idf` already exists:

```sh
mkdir -p "$HOME/esp"
git clone --branch v5.4 --recursive https://github.com/espressif/esp-idf.git \
  "$HOME/esp/esp-idf"
git -C "$HOME/esp/esp-idf" checkout --detach \
  67c1de1eebe095d554d281952fde63c16ee2dca0
git -C "$HOME/esp/esp-idf" submodule update --init --recursive
cd "$HOME/esp/esp-idf"
./install.sh esp32s3
. ./export.sh
```

Then, from the repository root, resolve the committed TX lock and build into a
new artifact directory:

```sh
(
  cd software/ruview/firmware/esp32-csi-tx
  idf.py reconfigure
)
python software/ruview/firmware/esp32-csi-tx/tools/build_release.py \
  --output artifacts/tx1-candidate-UNIQUE --build
```

`idf.py reconfigure` populates the ignored `managed_components/` cache from
`dependencies.lock`; `build_release.py` then exports and hashes the resolved
component together with the public TX sources. The helper refuses a missing
lock/cache, an unreviewed component version, or an ESP-IDF commit other than
the pinned v5.4 revision. It runs
`idf.py build` only: it never scans serial ports, opens a device, flashes,
provisions or changes NVS. Choose a fresh output name for every build.

## Channel diagnostics and the next boot

The channel gate distinguishes the AP configuration from the current radio
state. It requires AP-only mode, the configured and actual primary channel,
and the requested interface bandwidth. An active HT40 secondary channel must
match the requested direction. With HT40 still configured, `NONE` is reported
as `HT20_OBSERVED_HT40_UNPROVEN`: IDF 5.4 negotiates the AP's actual bandwidth
with connected stations. This is not proof of HT40 operation or CSI acceptance.
There is no change to the private channel-6/HT40/below profile, thresholds or
startup wait. The setter remains after `esp_wifi_start`; a station-list check
rejects already associated clients before it. That snapshot does not prevent
a later association race, which remains observable through event timestamps.

Every radio check logs getter operation/return codes, expected primary and
secondary channels, current channels, interface bandwidth, AP configuration,
mode, station count, and microseconds since the start call. `-1` means the
getter did not provide that value. Getter reads are sequential, not an atomic
radio snapshot. Credentials and station MAC addresses are never logged.

UART0 at 115200 is now the primary application/panic console; native USB
Serial/JTAG is the nonblocking secondary application output. Capture both
before reset. UART0 preserves the early record while native USB reenumerates;
USB may omit early/secondary output. The old bootloader is not changed. The
release configuration check enforces this routing. A local boot pass still
requires separate visual LED, Mixed-V2 and seven-client hardware acceptance.

## Offline tests

For an already installed, clean ESP-IDF v5.4 environment, activate the SDK and
export an explicit public source set into a **new** artifact directory:

```sh
export IDF_TOOLS_PATH="$PWD/.toolchains/espressif-v5.4"
. .toolchains/esp-idf-v5.4/export.sh
python software/ruview/firmware/esp32-csi-tx/tools/build_release.py \
  --output artifacts/tx1-candidate-UNIQUE --build
sh artifacts/tx1-candidate-UNIQUE/source/test/run_tests.sh
```

The helper builds only; it never opens a device. It includes the public header,
excludes private Arduino configuration and original build directories, pins
`led_strip` 3.0.3 and exports the dependency lock and resolved component bytes.
`sources.json` hashes the exported source and dependency files and binds their
hashes together with the repository commit into `source_sha256`. The generated
`source_identity.cmake` is not itself listed or directly hashed there: it is
derived from that manifest's commit and `source_sha256` and embeds these values
in the app. When verifying an exported snapshot, check both the listed file
hashes and the exact generated identity file.
`build-receipt.json` records IDF commit, compiler/Python/SDK tools, resolved
components, effective configuration, partition contract, full BIN SHA-256 and
image digest. The migration validator requires `reproducible_build: true` in
the receipt and `CONFIG_APP_REPRODUCIBLE_BUILD=y` in the effective config.
ESP-IDF 5.4 then removes build date, time and source/build path metadata.
Independent builds from the same source and pinned toolchain should have
byte-identical app BINs; compare the `candidate.sha256` value inside each
receipt. `runtime_image_sha256` is the ESP image digest returned by `/status`,
not the full BIN hash. A local clean-source build can verify the intended
publication snapshot before it is committed. An independent build from a public
clone remains a separate check after that snapshot has actually been published.

### Rebuild a frozen migration candidate

The existing TX1 migration candidate is a separate release companion: its
source archive, `sources.json`, generated `source_identity.cmake`, build receipt
and expected app digest must be distributed and verified together. It contains
the pinned LED component but no device profile or flash backup. With the exact
SDK and compiler recorded by that candidate activated, build it in fresh
directories:

```sh
shasum -a 256 /path/to/frozen-candidate-source.tar.gz
mkdir tx1-exact-source-UNIQUE
tar -xzf /path/to/frozen-candidate-source.tar.gz \
  -C tx1-exact-source-UNIQUE
(
  cd tx1-exact-source-UNIQUE
  python "$IDF_PATH/tools/idf.py" -B ../tx1-exact-build-UNIQUE build
)
shasum -a 256 tx1-exact-build-UNIQUE/esp32-csi-tx.bin
```

Check the archive checksum against its release manifest before extraction.
Preserve the recorded identity files during this rebuild, then compare the app
size, full BIN SHA-256 and runtime image digest against the same manifest.
This is build evidence, not TX1 hardware acceptance. Use the current reviewed
operator tools for migration: tools inside a frozen build archive record their
earlier revision.

Tests reuse the Arduino packet/scheduler checks and add strict LE golden bytes,
NVS and WiFi boundary fakes, boot/OTA failure cases, additive NVS fixtures,
PCAP parsing, fixed measurement windows and bytearray-backed migration/restore.
No test enumerates USB, opens serial devices, starts a server or contacts
production endpoints. Local tests/builds establish software evidence only;
WLAN, RX, LED, OTA and restore hardware acceptance remain open.

# Observatory
[Deutsch](README.md) · [**English**](README.en.md)

[![Hack Club Stardance](https://img.shields.io/badge/Hack%20Club-Stardance-ec3750?style=flat-square&logo=hackclub&logoColor=white)](https://stardance.hackclub.com/projects/25673)
[![ESP32](https://img.shields.io/badge/ESP32-E7352C?style=flat-square&logo=espressif&logoColor=white)](https://www.espressif.com/en/products/socs/esp32)
[![Status](https://img.shields.io/badge/status-experimental-orange)](#current-validation-status)
[![License: PolyForm Noncommercial 1.0.0](https://img.shields.io/badge/License-PolyForm%20Noncommercial%201.0.0-blue.svg)](LICENSE.md)


An experimental 1TX/4RX WiFi CSI system investigating presence, movement, and
nine fixed room positions without cameras, while using mmWave only as an
independent reference.

<img src="images/esp32-s3-boards.jpeg" alt="The five labeled ESP32-S3 boards in the Observatory setup: RX1 through RX4 and TX" width="420">

## View the project

**[View Observatory on Stardance](https://stardance.hackclub.com/projects/25673)**

There is currently no public live demo: real measurements require the fixed
local room setup with TX, RX1 through RX4, and the reference sensor. The
[current technical handoff](08-aktueller-arbeitsstand-d6-und-position.md)
documents what has been verified and which hardware gate comes next.

> **You are being sensed.**
>
> This room has a secret system, a machine that watches the Wi-Fi every second
> it is running. I know because I built it.
>
> I designed the machine to detect movement, but it sees every disturbance.
> Reflections caused by ordinary people—people like you. Signals the old
> algorithms considered “irrelevant.”
>
> They couldn’t understand them, so I decided I would. But I needed a partner,
> another sensor with the precision to reveal the truth.
>
> Bound by physics, we work without cameras. You’ll never see us. But moving or
> motionless, if you change the signal… we’ll find *you*.

*Inspired by the opening monologue of the TV series
[*Person of Interest*](https://warnertv.de/serie/sendungen/person-of-interest).*

## Quick start

This repository contains the documentation, hardware files, and the complete
Observatory software source:

```bash
git clone https://github.com/Gerafftes/Observatory.git
cd Observatory
```

The documentation can be read directly. The UI and backend are stored together
under [`software/ruview/`](software/README.md). A hardware-free software run can
be started with:

```bash
cd software/ruview/v2
cargo run -p wifi-densepose-sensing-server --no-default-features -- \
  --source simulate --http-port 3002 --ws-port 3001
```

Then open `http://127.0.0.1:3002/ui/index.html#sensing`. Simulation remains
`SOFTWARE-ONLY / UNVALIDATED` and does not replace any hardware gate.

The main entry points are:

1. [UI, backend, and firmware](software/README.md)
2. [Current D6/mmWave status](08-aktueller-arbeitsstand-d6-und-position.md)
3. [Result reports](results/)
4. [PCB-01 manufacturing files](hardware/pcb-01/)
5. [PCB-02 manufacturing files and KiCad sources](hardware/pcb-02/)

The software is based on [ruvnet/RuView](https://github.com/ruvnet/RuView), but
the Observatory changes and required subprojects are included directly in this
repository. Provenance and pinned source revisions are listed in
[`software/README.md`](software/README.md); project-specific changes are
documented in [`06-ruview-anpassungen.md`](06-ruview-anpassungen.md).

## What Observatory can do

- Capture Raw CSI from four ESP32-S3 receivers without loss and bind each
  capture to a specific physical setup.
- Check packet source, RX identity, subcarrier grid, data rate, and drops
  before a measurement.
- Treat movement, still presence, and position as separate evidence stages.
- Output only P01 through P09, `unknown`, or `ambiguous` for position instead
  of inventing a falsely precise continuous heatmap.
- Keep training, blind prediction, and ground truth separate through distinct
  files and cryptographic hashes.
- Use an HLK-LD2450 as a calibration and reference sensor without feeding its
  values into the later WiFi CSI predictor.

## Local checks

The included software can be checked directly from the repository:

```bash
sh scripts/verify_observatory_source.sh
node --test software/ruview/ui/tests/*.test.mjs
cargo check --manifest-path software/ruview/v2/Cargo.toml \
  -p wifi-densepose-sensing-server --no-default-features
python3 -m unittest scripts/tests/test_evaluate_d5_replay.py
```

The check processes no new sensor signals and does not prove live hardware
quality.

## Research question

How reliably can an ESP32-based WiFi CSI system detect movement and breathing
patterns in a room, and which physical limitations affect the results?

## Current validation status

**As of August 14, 2026**

| Area | Status | What this status proves |
|---|---|---|
| 1 TX / 4 RX | Transport verified | All four receivers delivered source-bound Raw CSI using the same subcarrier grid. |
| D4 movement | Experimental | Coarse movement false alarms were reduced; still-presence detection remains unreliable. |
| D5 still presence | Live test failed | With a motionless person present, 350 out of 350 samples remained `ABSENT`. |
| D6 position | Software prepared | Captures, nine positions, and blind tests are implemented; a real index passing blind evaluation is still missing. |
| mmWave reference | Partially operational | ESP32-C3, CSI WiFi, and the status service are verified; the real LD2450 data path has not yet been fully validated. |
| Overall system | Not validated | There is no joint real-world PASS for classification, position, and mmWave. |

Software tests, a successful flash operation, or lossless transport are
explicitly not treated as proof of real sensor or positioning accuracy.

## User interface

### mmWave calibration assistant

The assistant guides the operator through connection, alignment, coverage,
zones, training, blind testing, and results. The screenshot shows the tested
`Server unreachable` error state with HTTP 502, not a connected radar capture.

<img src="images/ui/mmwave-calibration-server-unreachable.png" alt="Seven-step mmWave calibration assistant showing an unreachable server and HTTP 502 status" width="900">

### Experiment cockpit

The new cockpit keeps the setup profile, WiFi workflow, recordings, and the
separate mmWave reference visible in one place. The screenshots below come
from a simulated run with no sensors connected.

<img src="images/ui/experiment-cockpit-setup.png" alt="Experiment cockpit with setup profile, status overview, and simulated hardware state" width="900">

<img src="images/ui/experiment-cockpit-guide.png" alt="Experiment cockpit showing room, TX, and RX positions while the mmWave reference waits" width="900">

<img src="images/ui/experiment-cockpit-workflow-guide.png" alt="Experiment cockpit workflow guide with ten locked or ready phases" width="900">

#### Short guide

1. **Open the setup profile:** Enter room dimensions (length/height/width),
   the TX position, and RX1–RX4 positions, then click **Save new profile
   version**.
2. **Create an experiment run:** Enter a run name, select the saved profile,
   and click **Create WiFi experiment**.
3. **Seal the setup:** The guide stores the profile and hash for this run.
   This is a software-only step.
4. **Empty WiFi baseline:** Keep the room empty, start and finish the empty
   calibration. Without CSI nodes, the system stops at **too few RX
   fingerprints**.
5. **mmWave calibration:** The radar supplies separate position packets,
   coverage, and CSI time alignment. Without the sensor, the status remains
   **Waiting for mmWave**.
6. **Blind test:** Generate a reproducible order and collect new CSI captures
   without ground truth. Without RX nodes there are no valid captures; the
   software demo can only show the flow.
7. **Keep prediction and truth separate:** Register only the WiFi prediction
   first, then reveal the separate radar/position truth.
8. **Evaluate and report:** Compare accuracy, coverage, error distance,
   confusion matrix, and quality gates, then write the report. Without
   hardware it remains explicitly **SOFTWARE-ONLY / UNVALIDATED** and proves
   no real measurement quality.

## How it works

WiFi signals change through reflection, attenuation, and multipath. One TX
board generates controlled radio traffic; RX1 through RX4 measure complex CSI
values from different positions in the room. Observatory compares these
measurements with an empty-room reference bound to the same setup.

Positioning is deliberately discrete. Instead of interpolating between
unmeasured coordinates, D6 learns fingerprints for nine marked floor points.
If the evidence is insufficient or multiple points match similarly well, the
system must return `unknown` or `ambiguous`.

The mmWave sensor is used only as an independent calibration and blind-test
reference. This prevents the WiFi CSI predictor from indirectly receiving the
correct answer during evaluation.

```text
physical setup
→ setup seal
→ 25-second preflight
→ 65-second empty-room calibration
→ P01–P09 training
→ position index
→ blind tests
→ joint quality gates
→ live display
```

## Verified results

- The technical discovery on August 9 captured `2,612` frames from RX1
  through RX4 with `0` drops. This verifies transport, binding, and grid
  stability, not detection or positioning quality.
- Two historical sealed D6 preflights passed with `2,545` and `2,701` frames,
  respectively, with `0` drops in both runs.
- A 65-second empty-room calibration recorded `6,102` frames with `0` drops
  and passed strict offline inspection.
- The first real D5 still-presence live test achieved `0%` still recall. D5
  therefore remains disabled and experimental.
- Adding the ESP32-C3, PCB, and mmWave hardware later changed the physical
  setup; the current setup has not yet been sealed as setup v2.

Key evidence:

- [D5: offline replay and experimental presence calibration](results/2026-07-26_D5_offline-replay-und-experimentelle-praesenzkalibrierung.md)
- [D5: real still-presence live test](results/2026-07-26_D5_realer-still-livetest.md)
- [D6: setup capture and TX firmware identity](results/2026-08-09_D6_setupaufnahme-und-TX-firmwareidentitaet.md)
- [D6: setup seal and preflight](results/2026-08-09_D6_setup-siegel-und-preflight.md)
- [D6: sidecar fix, resealing, and empty-room calibration](results/2026-08-09_D6_sidecar-fix-neusiegelung-und-preflight.md)

## Hardware

The WiFi CSI setup consists of five ESP32-S3 boards. A separate ESP32-C3 board
will connect the HLK-LD2450 as an independent reference sensor.

<img src="images/hlk-ld2450-mmwave-sensor.jpeg" alt="HLK-LD2450 24 GHz mmWave reference sensor" width="460">

PCB-01 connects the ESP32-C3 to the mmWave reference path. The following
manufacturing preview shows the board revision used by the project:

<img src="images/pcb-01-preview.webp" alt="PCB-01 manufacturing preview with ESP32-C3 footprint, C1, C2, and connector U2" width="460">

The [PCB-01 Gerber and drill files](hardware/pcb-01/) are available in this
repository with a SHA-256 checksum and manufacturing note.

The revised [PCB-02 with KiCad sources, validation reports, and ordering archive](hardware/pcb-02/)
restores the SMD capacitors, uses standard pin-header pads for the ESP32-C3,
and keeps the PCB-01 outer dimensions and mounting-hole positions.

The external MakerWorld model
[*ESP32 S3 Wroom Case*](https://makerworld.com/de/models/1456361-esp32-s3-wroom-case#profileId-1517915)
is intended as an enclosure for the ESP32-S3 boards. Its MakerWorld Standard
Digital File License does not permit redistributing the STL through this
repository. See [`hardware/esp32-s3-case/`](hardware/esp32-s3-case/) for the
source and usage note.

## Documentation

The detailed project documentation linked below is currently written in
German.

| File | Contents |
|---|---|
| [`00-status-und-annahmen.md`](00-status-und-annahmen.md) | Setup, assumptions, coordinates, and open work |
| [`01-projektjournal.md`](01-projektjournal.md) | Chronological development journal |
| [`02-versuchslog.md`](02-versuchslog.md) | Experiments performed |
| [`03-messprotokoll.md`](03-messprotokoll.md) | Measurement procedures and quality rules |
| [`04-auswertung-bis-problemfrage.md`](04-auswertung-bis-problemfrage.md) | Analysis organized around the research question |
| [`05-erfolge-niederlagen-und-aenderungen.md`](05-erfolge-niederlagen-und-aenderungen.md) | Successes, failures, and changes in direction |
| [`06-ruview-anpassungen.md`](06-ruview-anpassungen.md) | Local changes to RuView |
| [`07-screenshot-nachweise.md`](07-screenshot-nachweise.md) | Visual evidence and failure screenshots |
| [`08-aktueller-arbeitsstand-d6-und-position.md`](08-aktueller-arbeitsstand-d6-und-position.md) | Authoritative D6/mmWave handoff |
| [`software/`](software/README.md) | Complete UI, backend, and firmware source with provenance |
| [`results/`](results/) | Detailed result reports |
| [`templates/messblatt.md`](templates/messblatt.md) | Measurement-sheet template |

## License

Observatory-owned material is licensed under the
[PolyForm Noncommercial License 1.0.0](LICENSE.md). The embedded RuView source
and its vendored components retain their respective license and notice files
under [`software/ruview/`](software/ruview/).

## Credits

- [ruvnet/RuView](https://github.com/ruvnet/RuView) is the documented software
  base for the embedded UI, firmware, and sensing-server source.
- [Espressif](https://www.espressif.com/en/products/socs/esp32) develops the
  ESP32 platforms used by this project.
- The ESP32-S3-WROOM enclosure was created by MakerWorld user
  [`aiekick`](https://makerworld.com/de/models/1456361-esp32-s3-wroom-case#profileId-1517915)
  and is offered under the MakerWorld Standard Digital File License.
- The cinematic prologue is inspired by the opening monologue of the TV series
  [*Person of Interest*](https://warnertv.de/serie/sendungen/person-of-interest).

## Documentation rules

- Failed experiments remain documented because they reveal technical and
  physical limitations.
- Raw data is published only after its scope, privacy, and reproducibility have
  been reviewed.
- Secrets, WiFi credentials, and private device identifiers are not included
  in published material.

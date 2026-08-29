<h1 align="center">Observatory</h1>

<p align="center">
  An experimental 1TX/4RX WiFi CSI system investigating presence, movement,
  experimental breathing and heart-rate signals, and camera-free position
  estimation, with mmWave as an independent reference.
</p>

<p align="center">
  <a href="https://stardance.hackclub.com/projects/25673">Stardance</a> ·
  <a href="#quick-start">Quick start</a> ·
  <a href="#what-observatory-can-do">Features</a> ·
  <a href="results/README.en.md">Results</a> ·
  <a href="hardware/README.en.md">Hardware</a> ·
  <a href="README.md">Deutsch</a>
</p>

<p align="center">
  <a href="https://stardance.hackclub.com/projects/25673"><img src="https://img.shields.io/badge/Hack%20Club-Stardance-ec3750?style=flat-square&amp;logo=hackclub&amp;logoColor=white" alt="Hack Club Stardance"></a>
  <a href="https://www.espressif.com/en/products/socs/esp32"><img src="https://img.shields.io/badge/ESP32-E7352C?style=flat-square&amp;logo=espressif&amp;logoColor=white" alt="ESP32"></a>
  <a href="#current-validation-status"><img src="https://img.shields.io/badge/status-experimental-orange" alt="Experimental status"></a>
  <a href="LICENSE.md"><img src="https://img.shields.io/badge/License-PolyForm%20Noncommercial%201.0.0-blue.svg" alt="PolyForm Noncommercial 1.0.0 license"></a>
  <a href="https://github.com/Gerafftes/Observatory"><img src="https://img.shields.io/github/repo-size/Gerafftes/Observatory?style=flat-square&amp;label=project%20size" alt="Project size"></a>
  <a href="https://octocounts.com/github/Gerafftes/Observatory/tree/main"><img src="https://api.octocounts.com/badge/Gerafftes/Observatory/branch/main?type=lines&amp;v=3" alt="Lines"></a>
</p>

<table>
  <tr>
    <td align="center" width="50%">
      <a href="images/esp32-s3-boards.jpeg"><img src="images/esp32-s3-boards.jpeg" alt="The five labeled ESP32-S3 boards in the Observatory setup: RX1 through RX4 and TX" width="100%"></a><br>
      <sub>ESP32-S3 boards: RX1 through RX4 and TX</sub>
    </td>
    <td align="center" width="50%">
      <a href="images/mmwave-breadboard-setup.jpeg"><img src="images/mmwave-breadboard-setup-hero.jpeg" alt="Provisional breadboard setup with HLK-LD2450 and ESP32-C3" width="100%"></a><br>
      <sub>Provisional mmWave breadboard setup</sub>
    </td>
  </tr>
</table>

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

## Table of contents

- [View the project](#view-the-project)
- [Quick start](#quick-start)
- [What Observatory can do](#what-observatory-can-do)
- [Local checks](#local-checks)
- [Research question](#research-question)
- [Current validation status](#current-validation-status)
- [User interface](software/experiment-cockpit.en.md)
- [How it works](architecture.en.md)
- [Verified results](results/README.en.md)
- [Hardware](hardware/README.en.md)
- [Documentation](#documentation)
- [License](#license)
- [Credits](#credits)
- [Documentation rules](#documentation-rules)

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

Then open `http://127.0.0.1:3002/ui/index.html#sensing`.

> [!NOTE]
> The `--source simulate` run checks the UI and workflow without hardware. It remains explicitly `SOFTWARE-ONLY / UNVALIDATED` and does not replace any hardware gate.

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
- Output a discrete, setup-bound position estimate with `unknown` or `ambiguous`
  instead of inventing a falsely precise continuous heatmap.
- Explore breathing and heart-rate signals as experimental CSI indicators;
  without an independent reference measurement they are not validated
  physiological measurements.
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

> [!WARNING]
> These checks process no new sensor signals. A passing software test, flash, or transport check does not prove live hardware or positioning accuracy.

## Research question

How reliably can an ESP32-based WiFi CSI system detect movement, breathing, and
heart-rate signals in a room, and which physical limitations affect the
results?

## Current validation status

**As of August 14, 2026**

| Area | Status | What this status proves |
|---|---|---|
| 1 TX / 4 RX | Transport verified | All four receivers delivered source-bound Raw CSI using the same subcarrier grid. |
| D4 movement | Experimental | Coarse movement false alarms were reduced; still-presence detection remains unreliable. |
| D5 still presence | Live test failed | With a motionless person present, 350 out of 350 samples remained `ABSENT`. |
| D6 position | Software prepared | Captures, discrete position estimation, and blind tests are implemented; a real position index passing blind evaluation is still missing. |
| Breathing/heart rate | Experimental | CSI-based signal indicators are explored, but are not physiologically validated without an independent reference. |
| mmWave reference | Partially operational | ESP32-C3, CSI WiFi, and the status service are verified; the real LD2450 data path has not yet been fully validated. |
| Overall system | Not validated | There is no joint real-world PASS for classification, position, and mmWave. |

Software tests, a successful flash operation, or lossless transport are
explicitly not treated as proof of real sensor or positioning accuracy.

## User interface

The Observatory UI contains the mmWave calibration assistant and the
experiment cockpit for setup profiles, the WiFi workflow, blind captures, and
evaluation. Screenshots and the complete short guide are on the [separate
cockpit page](software/experiment-cockpit.en.md).

<a href="software/experiment-cockpit.en.md"><img src="images/ui/experiment-cockpit-setup.png" alt="Experiment cockpit with setup profile, status overview, and simulated hardware state" width="760"></a>

> [!NOTE]
> The UI screenshots and demo flow show software states without connected sensors. They do not prove real CSI, radar, or positioning data.

## How it works

WiFi CSI is evaluated against an empty-room reference and discrete position
fingerprints; mmWave remains an independent reference. The [architecture and
data-flow page](architecture.en.md) explains the flow and evidence separation
in detail.

## Verified results

The [results overview](results/README.en.md) contains the four checked figures,
short explanations, evidence files, and the complete evaluation.

> [!IMPORTANT]
> D5-abs lowers D4's global empty-room false presence from `75.2%` to `0%`, but also lowers still recall from `88.4%` to `0%`, so it **failed overall**. D6 is technically complete and setup-bound; this does not establish detection or positioning accuracy.

## Hardware

The setup consists of five ESP32-S3 boards, one ESP32-C3, and the HLK-LD2450
as an independent mmWave reference. The complete [hardware documentation](hardware/README.en.md)
collects the PCBs, breadboard CAD, fastening and mmWave components, images, and
enclosure notes.

> [!IMPORTANT]
> **PCB-02 is the required revision for the current setup.** PCB-01 remains documented as the earlier manufacturing preview.

- [PCB-01 Gerber and drill files](hardware/pcb-01/)
- [PCB-02 KiCad sources, validation reports, and ordering archive](hardware/pcb-02/)
- [Breadboard CAD, fastening hardware, and mmWave BOM](hardware/breadboard/README.en.md)

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
| [`architecture.en.md`](architecture.en.md) | Architecture, data flow, and evidence separation |
| [`hardware/README.en.md`](hardware/README.en.md) | Hardware overview, PCBs, breadboard CAD, and fastening parts |
| [`software/experiment-cockpit.en.md`](software/experiment-cockpit.en.md) | UI screenshots and experiment workflow |
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

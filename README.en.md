<h1 align="center">Observatory</h1>

<p align="center">
  Explore what WiFi reveals about movement in a room – without cameras.
  Observatory helps you record and examine wireless signals with ESP32 boards.
  The goal is continuous person localization.
</p>

<p align="center">
  <a href="https://stardance.hackclub.com/projects/25673">Stardance</a> ·
  <a href="#quick-start">Quick start</a> ·
  <a href="#what-observatory-can-do">Features</a> ·
  <a href="experiment-reports/README.en.md">Results</a> ·
  <a href="hardware/README.en.md">Hardware</a> ·
  <a href="docs/research/rf-sensing-sources.md">Sources</a> ·
  <a href="README.md">Deutsch</a>
</p>

<p align="center">
  <a href="https://stardance.hackclub.com/projects/25673"><img src="https://img.shields.io/badge/Hack%20Club-Stardance-ec3750?style=flat-square&amp;logo=hackclub&amp;logoColor=white" alt="Hack Club Stardance"></a>
  <a href="#current-validation-status"><img src="https://img.shields.io/badge/status-experimental-orange" alt="Experimental status"></a>
  <a href="LICENSE.md"><img src="https://img.shields.io/badge/License-PolyForm%20Noncommercial%201.0.0-blue.svg" alt="PolyForm Noncommercial 1.0.0 license"></a>
</p>

<table>
  <tr>
    <td align="center" width="50%">
      <a href="project-media/esp32-s3-boards.jpeg"><img src="project-media/esp32-s3-boards.jpeg" alt="The five labeled ESP32-S3 boards in the Observatory setup: RX1 through RX4 and TX" width="100%"></a><br>
      <sub>ESP32-S3 boards: RX1 through RX4 and TX</sub>
    </td>
    <td align="center" width="50%">
      <a href="project-media/mmwave-breadboard-setup.jpeg"><img src="project-media/mmwave-breadboard-setup-hero.jpeg" alt="Provisional breadboard setup with HLK-LD2450 and ESP32-C3" width="48%"></a>
      <a href="hardware/pcb-03/"><img src="hardware/pcb-03/preview/PCB-03-top.png" alt="PCB-03 board preview for the ESP32-C3/HLK-LD2450 reference node" width="48%"></a><br>
      <sub>mmWave: breadboard setup and PCB-03 board preview</sub>
    </td>
  </tr>
</table>

## View the project

**[View the project on Stardance](https://stardance.hackclub.com/projects/25673)** · **[Try the interface without sensors](#quick-start)**

Want to understand how WiFi signals could reveal someone's presence?
Observatory brings together Raw CSI capture, an experiment interface, and
documented trials. It helps you follow what the data actually shows and
where the method reaches its limits.

**Already demonstrated:** four-receiver capture, an RX1↔RX4 radio comparison,
and 60 minutes of stable TX-only capture. Reliable continuous WiFi localization
is still in development. There is currently no public live demo.

> **You are being sensed.**
>
> This room has a secret system, a machine that watches the Wi-Fi every second
> it is running. I know because I built it.
>
> I built it to study movement: the reflections, the interruptions, the
> small changes ordinary people leave in a wireless signal. People like you.
>
> But a changing signal is not yet a position. To learn what it means,
> I needed a reference—another way to observe the room.
>
> Bound by physics, we work without cameras. Moving or motionless,
> you may change the signal. The question is what we can prove from it.

*Inspired by the opening monologue of the TV series
[*Person of Interest*](https://en.wikipedia.org/wiki/Person_of_Interest_(TV_series)).*

## Table of contents

- [Quick start](#quick-start)
- [What Observatory can do](#what-observatory-can-do)
- [Current validation status](#current-validation-status)
- [User interface](#user-interface)
- [How it works](#how-it-works)
- [Hardware](#hardware)
- [Documentation](#documentation)
- [License and credits](#license-and-credits)

## Quick start

Start with simulation to explore the interface without sensors. You need Git
and a Rust/Cargo environment; the [software guide](software/README.md)
provides source details.

```bash
git clone https://github.com/Gerafftes/Observatory.git
cd Observatory/software/ruview/v2
cargo run -p wifi-densepose-sensing-server --no-default-features -- \
  --source simulate --http-port 3002 --ws-port 3001
```

Then open `http://127.0.0.1:3002/ui/index.html#sensing`.

1. **Explore:** follow the [cockpit guide](software/experiment-cockpit.en.md) through setup and measurement workflows.
2. **Assess:** read the [experiment results and limits](experiment-reports/README.en.md).
3. **Prepare:** check [hardware and revisions](hardware/README.en.md) before planning your own measurements.

> [!NOTE]
> Simulation shows the UI and workflow with artificial data (`SOFTWARE-ONLY / UNVALIDATED`). Real measurements require the local hardware setup and a separate calibration.

## What Observatory can do

- **Examine signals:** capture Raw CSI from four ESP32-S3 receivers; check
  sources, rates, time windows, and gaps.
- **Trace experiments:** bind setups and recordings to versions; keep
  training, blind prediction, and ground truth separate.
- **Compare radio links:** capture RX1↔RX4 alongside the four TX→RX links.
  The technical pair test passed; its positioning benefit remains open.

**The next goal:** a person moves freely through a calibrated room while the
system estimates their position from WiFi alone. Calibration is intended to use
slow walks, with two mmWave sensors supplying position labels and an independent
comparison reference.

The earlier D6 approach returns discrete points or zones, with `unknown` or
`ambiguous` for uncertain evidence. Continuous localization, multiple-person
tracking, and transfer to a second setup remain to be evaluated.

## Current validation status

**As of October 29, 2026.** These statements summarize saved evidence.

| Area | Result | Limit |
|---|---|---|
| Four-RX capture | 180 s baseline passed: 173/173 shared valid windows | Does not establish positioning accuracy |
| RX1↔RX4 pair test | 300 s passed: 2,581/2,578 qualified peer frames; both directions 293/293 valid windows | Positioning benefit and a four-RX mesh remain open |
| 60 minutes TX-only | Passed after offline reassessment: 533,429 frames, 3,589/3,593 shared valid windows (99.89%; 90% required) | Original automatic closeout failed; no global firmware freeze follows from this result |
| Two mmWave references | Joint qualification pending | Spatial/time alignment and reference accuracy still need testing |
| Continuous WiFi localization | In development | Walking calibration and independent blind evaluation have not been demonstrated as a complete system |

The 60-minute run was reassessed twice with identical results; timing was
reconstructed from original evidence. The original online closeout failure
remains documented. Detailed endurance evidence is private, outside this
repository; newer development reports have not yet been published.

Earlier D5-abs trials eliminated empty-room false presence but lost all still
recall, failing overall. Breathing/heart-rate indicators remain experimental
and are not physiologically validated.

> [!IMPORTANT]
> Passing capture and software checks proves neither lossless radio transmission nor reliable localization. Sequence gaps remain documented; radar positions and simulated displays are not WiFi localization evidence.

## User interface

The experiment cockpit lets you manage setup profiles, calibration, blind
captures, and evaluation. The [cockpit guide](software/experiment-cockpit.en.md)
walks through the workflow and mmWave calibration assistant.

<a href="software/experiment-cockpit.en.md"><img src="project-media/ui/experiment-cockpit-setup.png" alt="Experiment cockpit with setup profile, status overview, and simulated hardware state" width="760"></a>

*The image shows a simulated software state without connected sensors.*

## How it works

One ESP32-S3 transmits WiFi packets; four receivers measure Channel State
Information (CSI), the room's effect on the wireless channel. Movement,
obstruction, and reflections change these signals. The host collects and checks
the raw data.

The planned walking calibration starts with an empty-room capture, followed
by slow walks with mmWave as a reference. Separate training and test sessions
are intended to establish whether the WiFi model can localize new movements
without radar as an input. The [architecture page](experiment-reports/architecture/architecture.en.md)
explains the existing data paths and earlier D6 approach.

<p align="center">
  <a href="experiment-reports/architecture/architecture.en.md"><img src="experiment-reports/architecture/figures/mmwave-connection/01-ack-verbindungsfluss.png" alt="BKLIT diagram of the mmWave ACK connection flow from the HLK-LD2450 through the ESP32-C3 and WiFi to the Sensing Server" width="760"></a>
</p>
<p align="center"><sub>This diagram shows one mmWave reference path. WiFi CSI and joint two-radar calibration are not shown here.</sub></p>

## Hardware

The WiFi setup requires five ESP32-S3 boards: TX1 and RX1 through RX4.
Two HLK-LD2450 sensors with ESP32-C3 boards are intended as calibration and
reference nodes. The images above show the earlier breadboard setup and PCB-03 preview.

- [PCB-03: current revision, KiCad sources, and manufacturing files](hardware/pcb-03/)
- [Breadboard: CAD, fastening hardware, and components](hardware/breadboard/README.en.md)
- [MMWAVE1/2: firmware and integration](software/ruview/firmware/esp32-mmwave-node/README.md)

PCB-01 and PCB-02 remain documented as earlier revisions in the
[hardware overview](hardware/README.en.md); use **PCB-03** for the current setup.

## Documentation

| Your next step | Start here |
|---|---|
| Examine trials and results | [Historical experiment reports](experiment-reports/README.en.md) |
| Build the software and check provenance | [Software guide](software/README.md) · [RuView changes](docs/project/06-ruview-anpassungen.md) |
| Prepare your own measurements | [Measurement protocol](docs/project/03-messprotokoll.md) · [Measurement sheet](measurement-templates/messblatt.md) |
| Follow development and sources | [Project journal](docs/project/01-projektjournal.md) · [Research sources](docs/research/rf-sensing-sources.md) |

Technical development notes and individual experiment reports are mostly in German.

## License and credits

Observatory-owned material uses [PolyForm Noncommercial 1.0.0](LICENSE.md).
The software is based on [ruvnet/RuView](https://github.com/ruvnet/RuView);
third-party components retain their own license and notice files.

- [Espressif](https://www.espressif.com/en/products/socs/esp32): ESP32 platforms.
- [aiekick on MakerWorld](https://makerworld.com/de/models/1456361-esp32-s3-wroom-case#profileId-1517915): ESP32-S3-WROOM enclosure, under the MakerWorld Standard Digital File License.
- [*Person of Interest*](https://warnertv.de/serie/sendungen/person-of-interest): inspiration for the prologue.

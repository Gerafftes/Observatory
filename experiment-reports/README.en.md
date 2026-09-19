# Results overview

[Deutsch](README.md)

This directory is the canonical, date-ordered index of measured BLL results.
Each entry is a self-contained package: its `README.md` sits next to the
associated figures, tables, and rendering notes.

## Source of truth

`experiment-reports/` is the only current source for result evidence.
`software/ruview/results/` remains a historical software snapshot. It is not
synchronized automatically and is not interpreted as a newer measurement.

Packages use `YYYY-MM-DD_<series>_<result-type>`. Experiment identifiers such
as `D4`, `E0`, and `RX` retain uppercase spelling, while descriptive parts use
lowercase ASCII slugs. Inside a package:

- `README.md` records the result, limitations, and next decision;
- `figures/` contains directly embedded charts or screenshots;
- `data/` contains derived tables, not raw recordings;
- `chart-map.md` and `rendering.md` preserve mapping, provenance, and render QA.

The catalog is explicitly sorted **newest first**. Directory listings usually
sort the dated names in ascending order and therefore do not replace this
index.

## Results catalog

| Date | Series | Result type | Evidence class | Assessment | Report | Primary figure |
|---|---|---|---|---|---|---|
| 2026-09-13 | mmWave | transport comparison | three controlled before/after runs plus redundancy test | **better, but not loss-free** | [Report](2026-09-13_mmwave_transport-vergleich/README.md) | [Arrival and loss](2026-09-13_mmwave_transport-vergleich/figures/01-ankunft-und-verlust.png) |
| 2026-08-30 | mmWave | runtime audit | recorded accepted packets | **failed**; sequence gap | [Report](2026-08-30_mmwave_runtime-audit/README.md) | — |
| 2026-08-23 | D4/D5/D6 | technical synthesis | 25 recordings, replay, and setup-bound technical evidence | D5-abs **failed**; D6 is technical only | [Report](2026-08-23_D4-D5-D6_technischer-bericht/README.md) | [Global comparison](2026-08-23_D4-D5-D6_technischer-bericht/figures/01-globaler-vergleich.png) |
| 2026-08-09 | D6 | sidecar fix, reseal, and preflight | live runner plus strict offline inspection | recorder preflight and empty-room calibration **passed** | [Report](2026-08-09_D6_sidecar-fix-neusiegelung-und-preflight/README.md) | — |
| 2026-08-09 | D6 | sealed preflight | setup-bound 25-second preflight | recorder preflight **passed**; live visualization not proven | [Report](2026-08-09_D6_setup-siegel-und-preflight/README.md) | — |
| 2026-08-09 | D6 | setup capture and TX identity | read-only hardware and configuration evidence | preparatory; setup was not yet sealed | [Report](2026-08-09_D6_setupaufnahme-und-tx-firmwareidentitaet/README.md) | — |
| 2026-07-26 | E0d/E1b | independent confirmation | second empty-room/still-person pair | RX4 hypothesis rejected; RX3 remains preliminary | [Report](2026-07-26_E0d-E1b_unabhaengige-bestaetigung/README.md) | — |
| 2026-07-26 | E0c/E1 | still-person separation | one empty-room/still-person pair | measurable, but not independently confirmed | [Report](2026-07-26_E0c-E1_still-person-separation/README.md) | — |
| 2026-07-26 | E0b/E0c | Mac-position A/B test | one spatial A/B change | RX4 effect supported; no general distance function | [Report](2026-07-26_E0b-E0c_mac-position-ab-test/README.md) | — |
| 2026-07-26 | D5 | real still-person live test | physical live test | **failed** | [Report](2026-07-26_D5_still-livetest/README.md) | — |
| 2026-07-26 | D5 | offline replay and presence calibration | software tests and replay | software path passed; real calibration was still open | [Report](2026-07-26_D5_offline-replay-und-praesenzkalibrierung/README.md) | — |
| 2026-07-26 | D4/E0b | clean empty-room test | controlled empty-room run | **failed** | [Report](2026-07-26_D4-E0b_sauberer-leerraum/README.md) | — |
| 2026-07-26 | D4/E0 | mixed empty-room run | two unmarked room entries | **unclear**; no valid FPR | [Report](2026-07-26_D4-E0_leerraum/README.md) | — |
| 2026-07-18 | fixed room | live-visualization diagnosis | browser and run observation | data path visible; no position evidence | [Report](2026-07-18_fester-raum_live-visualisierung-diagnose/README.md) | [Live visualization](2026-07-18_fester-raum_live-visualisierung-diagnose/figures/01-live-visualisierung.png) |
| 2026-06-28 | G2 | RX-distribution quality check | logger, server log, and screenshot | capture good; visualization unreliable | [Report](2026-06-28_G2_rx-verteilung-qualitaetscheck/README.md) | [Live visualization](2026-06-28_G2_rx-verteilung-qualitaetscheck/figures/01-live-visualisierung.png) |
| 2026-06-28 | G1 | 500-ms guard quality check | API samples and short server log | workaround supported; physical synchronization unproven | [Report](2026-06-28_G1_guard500ms-qualitaetscheck/README.md) | — |
| 2026-06-28 | A0–A3 | quality check | four early measurement series | technically usable; reliability still open | [Report](2026-06-28_A0-A3_qualitaetscheck/README.md) | — |

## Current primary figures

### mmWave transport

[![Arrival and loss in the mmWave before/after comparison](2026-09-13_mmwave_transport-vergleich/figures/01-ankunft-und-verlust.png)](2026-09-13_mmwave_transport-vergleich/README.md)

Transport improved overall but remains loss-prone, with a measured `13.1%`
sequence-loss ratio. The other three figures and render QA are colocated in the
[result package](2026-09-13_mmwave_transport-vergleich/README.md).

### D4/D5/D6

[![Global comparison of D4, D5 replay, and D5-abs](2026-08-23_D4-D5-D6_technischer-bericht/figures/01-globaler-vergleich.png)](2026-08-23_D4-D5-D6_technischer-bericht/README.md)

D5-abs removes empty-room false presence in the evaluated runs, but also loses
still recall and fails overall. All four figures, derived CSVs, and provenance
notes are colocated in the
[result package](2026-08-23_D4-D5-D6_technischer-bericht/README.md).

Every claim remains bound to its setup series. A passing transport or recorder
test does not establish detection or positioning accuracy.

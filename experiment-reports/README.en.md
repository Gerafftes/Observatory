# Results overview

[Deutsch](README.md)

Public historical catalog, newest first. Every original catalog row is retained below. A report is linked only when included in this public excerpt. Local source tables, screenshots, raw data, generators and render documentation remain with the originals. This documentation derivation does not repeat measurements or tests.

| Date | Series | Result type | Evidence class | Assessment | Report |
|---|---|---|---|---|---|
| 2026-09-20 | mmWave | sequence-loss comparison | two supplied 60-second windows | `0.1.6` **passed**; 0 lost sequences | [Report](2026-09-20_mmwave-sequenzverlustvergleich/README.md) |
| 2026-09-13 | mmWave | transport comparison | three controlled before/after runs plus redundancy test | **better, but not loss-free** | historical report; omitted from this excerpt |
| 2026-08-30 | mmWave | runtime audit | recorded accepted packets | **failed**; sequence gap | historical report; omitted from this excerpt |
| 2026-08-23 | D4/D5/D6 | technical synthesis | 25 recordings, replay, and setup-bound technical evidence | D5-abs **failed**; D6 is technical only | [Report](2026-08-23_D4-D5-D6_technischer-bericht/README.md) |
| 2026-08-09 | D6 | sidecar fix, reseal, and preflight | live runner plus strict offline inspection | recorder preflight and empty-room calibration **passed** | historical report; omitted from this excerpt |
| 2026-08-09 | D6 | sealed preflight | setup-bound 25-second preflight | recorder preflight **passed**; live visualization not proven | historical report; omitted from this excerpt |
| 2026-08-09 | D6 | setup capture and TX identity | read-only hardware and configuration evidence | preparatory; setup was not yet sealed | [Report](2026-08-09_D6_setupaufnahme-und-tx-firmwareidentitaet/README.md) |
| 2026-07-26 | E0d/E1b | independent confirmation | second empty-room/still-person pair | RX4 hypothesis rejected; RX3 remains preliminary | historical report; omitted from this excerpt |
| 2026-07-26 | E0c/E1 | still-person separation | one empty-room/still-person pair | measurable, but not independently confirmed | historical report; omitted from this excerpt |
| 2026-07-26 | E0b/E0c | Mac-position A/B test | one spatial A/B change | RX4 effect supported; no general distance function | historical report; omitted from this excerpt |
| 2026-07-26 | D5 | real still-person live test | physical live test | **failed** | historical report; omitted from this excerpt |
| 2026-07-26 | D5 | offline replay and presence calibration | software tests and replay | software path passed; real calibration was still open | historical report; omitted from this excerpt |
| 2026-07-26 | D4/E0b | clean empty-room test | controlled empty-room run | **failed** | [Report](2026-07-26_D4-E0b_sauberer-leerraum/README.md) |
| 2026-07-26 | D4/E0 | mixed empty-room run | two unmarked room entries | **unclear**; no valid FPR | [Report](2026-07-26_D4-E0_leerraum/README.md) |
| 2026-07-18 | fixed room | live-visualization diagnosis | browser and run observation | data path visible; no position evidence | historical report; omitted from this excerpt |
| 2026-06-28 | G2 | RX-distribution quality check | logger, server log, and screenshot | capture good; visualization unreliable | historical report; omitted from this excerpt |
| 2026-06-28 | G1 | 500-ms guard quality check | API samples and short server log | workaround supported; physical synchronization unproven | [Report](2026-06-28_G1_guard500ms-qualitaetscheck/README.md) |
| 2026-06-28 | A0–A3 | quality check | four early measurement series | technically usable; reliability still open | historical report; omitted from this excerpt |

All claims remain bound to their recorded setup and time window. Passing transport or recorder checks does not establish person detection, physiological validity, positioning accuracy or qualified ground truth. For the September 20 comparison, the source is a supplied two-window table; it contains no raw snapshots, exact start times or repeated runs. D4/D5/D6 includes no independently validated mmWave ground truth. Historical plots are intentionally omitted; the included reports retain their numeric results and limits.

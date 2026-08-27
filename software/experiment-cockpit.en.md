# Experiment cockpit and mmWave calibration

[Deutsch](experiment-cockpit.md)

This page describes the UI views and reproducible flow for the setup profile,
WiFi baseline, mmWave reference, blind captures, and evaluation.

## mmWave calibration assistant

The assistant guides the operator through connection, alignment, coverage,
zones, training, blind testing, and results. The screenshot shows the tested
`Server unreachable` error state with HTTP 502, not a connected radar capture.

<img src="../images/ui/mmwave-calibration-server-unreachable.png" alt="Seven-step mmWave calibration assistant showing an unreachable server and HTTP 502 status" width="900">

## Experiment cockpit

The cockpit keeps the setup profile, WiFi workflow, recordings, and the
separate mmWave reference visible in one place. The screenshots come from a
simulated run with no sensors connected.

<img src="../images/ui/experiment-cockpit-setup.png" alt="Experiment cockpit with setup profile, status overview, and simulated hardware state" width="900">

<img src="../images/ui/experiment-cockpit-guide.png" alt="Experiment cockpit showing room, TX, and RX positions while the mmWave reference waits" width="900">

<img src="../images/ui/experiment-cockpit-workflow-guide.png" alt="Experiment cockpit workflow guide with ten locked or ready phases" width="900">

## Short guide

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

> [!NOTE]
> The screenshots and demo flow document software states. They do not prove a connected radar, valid CSI captures, or real positioning accuracy.

# How Observatory works

[Deutsch](architecture.md)

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

Implementation details are in the [software overview](software/README.md); the
reproducible UI steps are documented in the
[experiment cockpit guide](software/experiment-cockpit.en.md).

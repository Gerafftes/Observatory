# mmWave runtime audit — 2026-08-30

Status: **offline evidence only — previous collector fails the runtime gate**

## Audited artifacts

- `v2/data/mmwave/mmwave-calibration-1788038593581496000.manifest.json`
- `v2/data/mmwave/mmwave-calibration-1788038593581496000.mmwave.jsonl`
- sealed setup `setup-7290a2a4feed1306` (Yaw `0`)

## Result

- 179 accepted mmWave packets
- one boot ID: `3592002182`; no reboot in the accepted session
- one transform throughout: origin `[2050,3300]`, Yaw `0`, raw X not inverted
- all recorded room coordinates reproduce exactly from raw X/Y and the packet
  transform
- eight accepted targets, all inside the sealed `[4020,3440]` mm floor bounds
- one transport interruption: expected sequence `1158`, received `1224`, 66
  packets missing, host-time gap `25.406747958` s
- no matching before/after `/api/v1/mmwave/status` snapshots exist

Verdict: **FAIL**. The sequence interruption alone invalidates the radar
runtime window.

## Evidence boundary

The session JSONL contains only packets accepted by the server. A packet
rejected as `room_bounds`, including the previously observed target
`[6162,3665]`, is therefore absent from this file. The saved session cannot
prove that the server's `room_bounds` counter remained zero.

The eight accepted targets also do not determine the physical yaw: both the
recorded Yaw `0` and the unsealed Yaw `-90000` candidate map this small sample
inside the room. The sensor's physical forward direction or a known-position
target is still required before sealing a changed transform.

## Required proof for the next run

Capture `/api/v1/mmwave/status` immediately before and after the same 25-second
preflight/session, then run:

```bash
python3 project_tools/audit_mmwave_runtime.py \
  --recording data/mmwave/<SESSION>.mmwave.jsonl \
  --setup /absolute/path/to/sealed-setup.json \
  --status-before /tmp/mmwave-status-before.json \
  --status-after /tmp/mmwave-status-after.json \
  --output /tmp/mmwave-runtime-audit.json
```

A passing report requires the sealed node/transform, exact coordinate
recomputation, no accepted out-of-room target, one stable boot, no sequence
gap or clock change, and zero server-side rejection, `room_bounds`, packet-loss
and reboot increments in the measured window.

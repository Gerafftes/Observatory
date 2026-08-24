# Observatory live data contract

This document records what Observatory may present as a real ESP32
measurement. It is intentionally stricter than accepting any data that arrives
through an open WebSocket.

## Source and freshness states

| State | Evidence | Rendering |
| --- | --- | --- |
| `CONNECTING` | Live WebSocket selected, but no sensing frame received yet | No hardware person or field |
| `LIVE ESP32` | Fresh browser-received frame with exact source `esp32` and no simulation marker | Validated hardware geometry; marker only after exact position validation |
| `SIMULATED` | Demo selected or an explicitly synthetic frame is received | Procedural demo scene, clearly labelled |
| `STALE` | Closed connection, expired frame, unknown/spoofed source | Hardware person and field cleared |

Freshness uses the local browser receipt time. Remote device timestamps are not
trusted for this gate. A live frame expires after 3 seconds; an open connection
that supplies no first frame also becomes stale after 3 seconds.

## Hardware geometry gate

`room_dimensions`, `tx_position`, and every receiver `nodes[].position` must
each contain exactly three finite numbers. Room dimensions must be positive;
TX and RX coordinates must lie inside that room. The fixed experiment requires
exactly the unique receiver IDs RX1 through RX4. Missing, partial, extra, or
invalid hardware geometry fails closed: the fixed demo room is never presented
as measured geometry.

## Hardware position gate

A neutral hardware position marker appears only when all of these conditions
are true:

1. The frame is a fresh explicit `esp32` frame.
2. `classification.presence === true`.
3. Hardware geometry is valid.
4. `position_estimate.state === "position"`.
5. `point_id` is exactly one of `P01` through `P09`.
6. `coordinates_m` contains exactly three finite in-room numbers.

Legacy `persons`, `localization.position`, a coarse heatmap, and a procedural
standing skeleton are not accepted as measured hardware positions or poses.
When no measured pose exists, Observatory shows only the neutral position
marker and explicitly says it is not a body pose.

## Signal field

The hardware field is diagnostic CSI field data, not a person-position
measurement. Its grid must be finite, bounded, exactly match `grid_size`, and
is scaled to the validated live room dimensions. It is cleared whenever the
exact hardware position gate fails.

## Regression test

Run:

```sh
node ui/observatory/tests/live-sensing-contract.test.mjs
```

The test covers open-without-frame, spoofed sources, timeout, disconnect,
simulation, invalid geometry, coarse-only persons/localization, presence
false, an exact `P01`-`P09` position, and signal-field validation.

## Implementation map

| File | Responsibility |
| --- | --- |
| `js/live-sensing-contract.js` | Pure source, freshness, geometry, position, and field validation |
| `js/main.js` | Separates demo scene from validated hardware geometry and neutral marker |
| `js/hud-controller.js` | Shows evidence state and clears stale metrics |
| `js/scenario-props.js` | Removes procedural demo props outside simulation mode |
| `tests/live-sensing-contract.test.mjs` | Trust-boundary and fail-closed regression cases |
| `tests/hud-live-state.test.mjs` | Visible badge, evidence text, and stale-state regression cases |

The related Sensing HUD in `../components/SensingTab.js` also requires
`classification.presence === true` before it shows discrete coordinates. Its
regression coverage lives in `../tests/sensing-localization.test.mjs`.

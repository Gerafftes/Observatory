# wifi-densepose-sensing-server – public documentation

The Sensing server receives CSI, keeps experiment/profile/recording state and serves UI/API data. The public [architecture overview](../../../../../docs/software/architecture.md) separates this software surface from positioning evidence. The discrete position classifier uses configured points (legacy P01–P09); its existence does not establish continuous positioning.

The contracts below derive from the bound local source description. The linked [published main source](https://github.com/Gerafftes/Observatory/blob/028f9d3f2703e1f3610e856f48fa0a709647818b/software/ruview/v2/crates/wifi-densepose-sensing-server/src/main.rs) has a verified immutable public file-object binding. No server, build, device or API call was executed for this documentation.

## RX reception diagnostics

Both `/api/v1/nodes` and `/api/v1/control-center/status` expose the additive
`nodes[].csi_reception` object for RX1–RX4. Its counters observe only new,
identity-admitted, source-validated CSI frames at the existing grid gate,
before the grid decision. Duplicate, out-of-order, invalid and unattested
frames receive no credit. Existing admission and grid rules are unchanged.

- `valid_frames` counts all such frames, including off-grid frames.
- `format_exclusions.count` counts valid frames rejected by the grid gate.
  Its `denominator` is `valid_frames`.
- `unexplained_sequence_gaps.count` sums missing positions in forward
  wrapping-u32 transitions between those valid arrivals. Observed off-grid
  frames close sequence positions. Its `denominator` is `forward_transitions`
  plus missing positions; the first frame establishes an anchor only.
- Each metric exposes `count`, `denominator`, `percent` and `valid`.
  `valid` describes the percentage basis: a zero denominator yields
  `valid: false` and `percent: null`. A measured zero with a positive
  denominator yields `valid: true` and `percent: 0`.

`scope: "cumulative_rx_boot"` means cumulative since the current RX/server
state began, with a reset on an admitted CSI boot change. Sync regressions
reset mesh timing and the legacy sequence anchor but preserve these counters.
A server restart starts new state. There is no rolling window. The two
percentage denominators differ; these metrics are not complementary shares
and neither establishes WLAN or TX loss.

The legacy `packet_loss_percent` retains its original calculation:
`100 * inferred_lost_frames / (sequence_observations + inferred_lost_frames)`
between **accepted grid frames**. It may include valid received format
exclusions and must not be interpreted as transport loss or corrected by
subtracting lifetime `skipped_grid_frames`.

Synthetic example: sequences 100–104 arrive, with only 100 and 104 on-grid:

```json
{
  "scope": "cumulative_rx_boot",
  "valid_frames": 5,
  "latest_sequence": 104,
  "format_exclusions": { "count": 3, "denominator": 5, "valid": true, "percent": 60.0 },
  "unexplained_sequence_gaps": { "count": 0, "forward_transitions": 4, "denominator": 4, "valid": true, "percent": 0.0 }
}
```

Both displays show `Formatausschlüsse 3 · 60.0%` and
`Ungeklärte Sequenzlücken 0 · 0.0%`. If 102 is absent, they show
`2 · 50.0%` (denominator 4 valid arrivals) and `1 · 25.0%`
(denominator 3 transitions + 1 missing position). The legacy percentage
remains 75% in both examples. Older responses without `csi_reception` show
`nicht verfügbar`; a missing percentage basis shows `--`. Stale RX values
are hidden as historical data, including an off-grid-only RX with no recent
accepted grid frame. The existing `frame_rate_hz` remains a sync-sequence
rate estimate. Global UDP outcome counters retain their separate semantics.

The JSON example above is synthetic. Original API/UI fixtures and test programs belong to the complete source project; they are not shipped or executed in this documentation set.

### Separate CAD profiles from RX run data

For the next regular server start, pass both storage roots explicitly:

```text
--data-dir /absolute/runtime-directory
--cad-profile-dir /absolute/catalogue-directory
--ui-path /absolute/source/ui
```

`--cad-profile-dir` must name an absolute directory containing the existing
`observatory-experiments.sqlite3`. CAD reads and saves use this catalogue,
independently of the RX runtime directory, UI copy or launch working directory.
A missing directory/database is rejected; catalogue errors never select an old
RX profile instead. Omitting the flag retains the previous single-store behavior.

Runs, artifacts and calibration files remain under `--data-dir`. Creating a run
or registering a calibration pins only its explicitly bound profile revision in
that runtime database. These immutable snapshots preserve foreign keys and
historical bindings; a later CAD save does not update them. Existing run-local
profiles are retained and are not merged into the CAD catalogue.

`GET /api/v1/experiments/status` reports `database_path` and
`profile_database_path`; the profile catalogue response also reports
`profile_database_path`. Check these paths and the selected revision/hash after
the next regular start. This is startup configuration: do not restart an active
RX run merely to activate it, and do not change its sealed runtime geometry.
These are documented software contracts, not a new format-cause fix, runtime
check or hardware acceptance.


## Isolated demo and acceptance limits

Use the [isolated simulation guide](../../../../../docs/setup/installation.md#simulation-starten), with loopback, a separate runtime directory, `--rx-session-only`, `--server-control-port 0` and `--no-edge-registry`. The browser control-port parameter is part of that guide. A simulated source alone does not disable all hardware-related helpers.

A complete appropriate source checkout is required for running or testing the server. This documentation snapshot contains no executable source package, binaries, private configuration or database. Profile catalog creation, fresh-clone builds, real installation, radar and positioning acceptance remain separate evidence. [Project status](../../../../../docs/project/status.md) and [results](../../../../../docs/research/results.md) preserve those limits.

## Explicitly unrecorded cabling in setup schemas v5 and v6

The recording host's `cable_revision` can be a nonempty revision string or,
from schema v5 onward, `{"status": "not_recorded"}`. The marker records an
explicit omission and provides no evidence of cable placement or comparability.
Schemas v1–v4 still require the recorded revision string.

The [setup validation source](src/position_setup.rs) validates the marker and
the schema-specific hash. Other required setup fields and transform checks
continue to apply. This source description adds no new setup seal or runtime
validation.

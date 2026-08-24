# worldgraph

**A privacy-aware environmental digital twin for ambient / RF sensing, in Rust.**

`worldgraph` models a physical space as a typed, provenance-tracked graph — rooms,
zones, walls, doorways, sensors, RF links, person tracks, object anchors, events, and
semantic-state *beliefs* — geospatially grounded and able to **forecast occupancy**.
It sits between sensor fusion and the semantic/agent layer of the
[RuView / wifi-densepose](https://github.com/ruvnet/wifi-densepose) stack
(ADR-139), storing fused *beliefs* rather than raw frames, with **mandatory provenance
and a privacy decision on every semantic state**.

Pure Rust, `serde`-serializable, schema-versioned, deterministic. Three layered crates:

| crate | role |
|-------|------|
| [`wifi-densepose-geo`](./wifi-densepose-geo) | **Geospatial grounding** — IP geolocation, Sentinel-2 satellite tiles, SRTM elevation, OSM buildings/roads, ENU↔geo transforms, temporal change tracking |
| [`wifi-densepose-worldgraph`](./wifi-densepose-worldgraph) | **The digital twin** — a `petgraph` `StableDiGraph` of typed nodes + relations; provenance-mandatory semantic beliefs; JSON (RVF) persistence |
| [`wifi-densepose-worldmodel`](./wifi-densepose-worldmodel) | **Predictive layer** — bridges person-track history to an OccWorld occupancy world-model and returns trajectory priors for the pose tracker |

## Where it sits

```
sensor fusion (ADR-137)  →  WorldGraph (ADR-139)  →  semantic / agent layer (ADR-140)
   fused beliefs              typed belief graph        queries, reasoning, eval (ADR-145)
                                    │
                                    └─→  worldmodel (ADR-147) → occupancy forecast / trajectory priors
```

WorldGraph stores **what is believed about the space**, not raw sensor frames — and
every belief is auditable back to the evidence that produced it.

## What makes it a *trustworthy* twin

- **Typed graph model** — `WorldNode` (rooms, zones, walls, doorways, sensors, RF links,
  person tracks, anchors, events, semantic states) and `WorldEdge` (`observes`,
  `located_in`, `adjacent_to`, `supports`, `contradicts`, `derived_from`,
  `privacy_limited_by`) over a `petgraph` `StableDiGraph`.
- **Provenance is mandatory** — every `SemanticState` carries `SemanticProvenance`
  (signal evidence + model + calibration + privacy decision). You cannot record a belief
  without recording *why* — enforced structurally, not by convention.
- **Privacy first-class** — a `PrivacyRollup` and `privacy_limited_by` relations make the
  privacy posture of any belief queryable; downstream consumers respect it.
- **Deterministic & versioned** — the serde enum node/edge model yields a deterministic,
  schema-versioned wire layout; `to_json` / `from_json` round-trips the whole graph (the
  RVF payload).
- **Geospatially grounded** — `geo` ties the local ENU scene to real coordinates,
  terrain, and map features for outdoor/structural context.
- **Predictive** — `worldmodel` forecasts occupancy and emits trajectory priors that
  improve downstream tracking.

## Quick start

```bash
cargo build
cargo test
```

```rust
use wifi_densepose_worldgraph::graph::WorldGraph;

// a WorldGraph is anchored to a geo-registration (local ENU ↔ real coordinates)
let mut wg = WorldGraph::new(registration);   // registration: GeoRegistration
// add rooms, sensors, person tracks, and provenance-backed semantic beliefs…
let bytes = wg.to_json()?;            // deterministic, schema-versioned RVF payload
let restored = WorldGraph::from_json(&bytes)?;
```

Forecasting occupancy from person-track history (requires the external OccWorld
inference subprocess — `worldmodel` is a thin client):

```rust
use wifi_densepose_worldmodel::{OccWorldBridge, worldgraph_to_occupancy};

let bridge = OccWorldBridge::new("/tmp/occworld.sock");
// worldgraph_to_occupancy(persons, &bounds, voxel_res) builds occupancy frames;
// send them in an OccupancyWorldModelRequest to get trajectory priors back.
```

## Crate layout

```
wifi-densepose-geo/         — coord, tiles, terrain, osm, locate, temporal, fuse, cache, brain
wifi-densepose-worldgraph/  — model (WorldNode/WorldEdge/SemanticProvenance), graph, error
wifi-densepose-worldmodel/  — occupancy, bridge (OccWorld client), error
```

## Related ADRs

| ADR | Title | Relation |
|-----|-------|----------|
| ADR-139 | WorldGraph environmental digital twin | This repo |
| ADR-137 | Streaming fusion / trust | Upstream — produces the fused beliefs |
| ADR-140 | Semantic / agent layer | Downstream — queries the graph |
| ADR-145 | Evaluation harness | Validates graph state |
| ADR-147 | OccWorld occupancy world model | `worldmodel` bridge |

## License

Dual-licensed under **MIT OR Apache-2.0** — see [`LICENSE-MIT`](./LICENSE-MIT) and
[`LICENSE-APACHE`](./LICENSE-APACHE).

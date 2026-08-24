# 01 — Survey: What ruvector Ships Today

Snapshot taken 2026-04-25 against `main` at commit `2e68f0c9f`.

## Workspace shape

- `crates/` contains ~110 directories. The workspace `Cargo.toml` has 96
  active `members =` entries (rest are `exclude`d for env-specific build
  reasons — `ruvector-postgres` needs `pgrx`, `mcp-brain-server` is private,
  the hyperbolic-hnsw pair is intentionally out of the default workspace).
- Workspace version pin is `2.2.0` for first-party `ruvector-*` crates;
  `rvAgent/*` crates are independently versioned at `0.1.0`.
- The two crates that have actual `[package].description` text indicating a
  consumer-facing v1 are:
  - `ruvector-rabitq` — *"RaBitQ: rotation-based 1-bit quantization for
    ultra-fast approximate nearest-neighbor search with theoretical error
    bounds."* No NAPI/wasm sibling crate. Pure Rust, 9 source files,
    ~3,700 LoC, the trait surface is `AnnIndex` over four index variants
    (`FlatF32Index`, `RabitqIndex`, `RabitqPlusIndex`, `RabitqAsymIndex`).
    Already published on crates.io at `2.2.0` per the workspace version.
  - `ruvector-rulake` — *"ruLake — vector-native federation intermediary
    over heterogeneous backends (ADR-155)."* Depends on `ruvector-rabitq`.
    7 source files, ~3,100 LoC. Public surface is `RuLake`,
    `BackendAdapter`, `LocalBackend`, `FsBackend`, `VectorCache`,
    `RuLakeBundle`. Methods on `RuLake` include `search_one`,
    `search_federated`, `search_batch`, `publish_bundle`,
    `refresh_from_bundle_dir`, `save_cache_to_dir`, `warm_from_dir`. All
    sync (no `async`).

These are the obvious starter targets — they're recent, they're small,
they're the ones the ADR pair (ADR-154 + ADR-155) is shipping behind, and
they're the only crates whose names appear in the workspace member list
ahead of `ruvector-core`.

## Existing FFI surfaces (the templates we copy)

### NAPI-RS bindings (Node.js)

The workspace has 14 `*-node` crates wired through `napi-derive` 2.16. The
cleanest minimal template is `crates/ruvector-diskann-node/src/lib.rs` —
one file, ~250 LoC, wraps `ruvector-diskann` with:

- `#[napi(object)]` config struct (`DiskAnnOptions`).
- `#[napi]` result struct (`DiskAnnSearchResult`).
- `#[napi]` opaque handle holding `Arc<RwLock<CoreIndex>>`.
- Sync methods (`insert`, `insert_batch`, `search`).
- Async methods via `tokio::task::spawn_blocking` + `.await` on the
  JoinHandle (`build_async`).

This shape — opaque handle, `Arc<RwLock<inner>>`, sync + spawn_blocking
async pair — is the existing house style. PyO3 bindings should mirror
it module-for-module so reviewers can diff them against each other and so
behaviour is identical across language clients.

### wasm-bindgen modules (browser / Node)

There are ~30 `*-wasm` crates. They use `wasm-bindgen` 0.2 + `js-sys` 0.3
+ a `getrandom` shim (`features = ["wasm_js"]`) that's the workspace
default. Pattern is identical: opaque handle, sync methods only (WASM
has no real threads in stable browsers without SharedArrayBuffer
gymnastics).

WASM is *relevant* to the SDK strategy as an alternative-not-taken
(see 02-strategy), not as a code-share opportunity.

### Raw cbindgen / FFI

`crates/ruvector-router-ffi` is the only `-ffi` crate. C ABI. We do not
use it. Mentioning here because someone will ask.

## What's published

- `ruvector-rabitq` and `ruvector-rulake` — both at workspace version
  `2.2.0`. These are the v1 consumer-facing crates.
- npm packages: `npm/packages/` has 57 directories. The flagship
  `ruvector` npm package is at `0.2.23` and pulls in `@ruvector/core`
  (0.1.25), `@ruvector/attention` (0.1.3), `@ruvector/gnn` (0.1.22),
  `@ruvector/sona` (0.1.4) — i.e. the JS/TS story is **fragmented**:
  one umbrella package over four core sub-packages, each backed by a
  `*-node` crate. The umbrella also bundles a CLI (`bin/cli.js`),
  WASM artifacts (`wasm/`), and an MCP server (`@modelcontextprotocol/sdk`
  is a runtime dep).

## What the JS/TS SDK actually covers (anchor for parity)

Reading `npm/packages/ruvector/package.json` keywords + dependencies:

- HNSW search, hybrid search, RaBitQ ("turboquant" appears),
  Graph RAG, FlashAttention-3, ColBERT, Mamba, hyperbolic geometry,
  ONNX MiniLM (semantic embeddings), SONA / LoRA / EWC adaptive
  learning, MCP server, Pi-Brain identity ("pi-key").

The Python SDK does **not** need to chase parity. The JS package is
the everything-bagel; the Python package should be narrow and
deliberate (see 02-strategy and 04-milestones).

## Examples that map to Python notebooks

`examples/` has 60+ directories. The ones that translate naturally:

- `examples/refrag-pipeline/` — RAG pipeline using `compress.rs` /
  `expand.rs` / `sense.rs`. Becomes the M1 hello-world notebook
  (`01_rag_in_5_lines.ipynb`).
- `examples/onnx-embeddings/` — MiniLM ONNX embedder. Backs the M3
  embedding tutorial.
- `examples/a2a-swarm/` — multi-peer A2A demo. Backs the M4
  agent tutorial. Lives at the workspace top level, was added with
  ADR-159.
- `crates/ruvector-rulake/examples/sidecar_daemon.rs` and
  `warm_restart.rs` — the "production deployment" patterns. Become
  the M2 ops notebook.

The notebooks are tracked under `04-milestones.md` per milestone, not
checked in here.

## What we are deliberately ignoring

These crates exist, are interesting, and will not be in the Python SDK
roadmap:

- The 30+ `*-wasm` browser crates. Not Python's market.
- `ruvix/` (cognition kernel, bare-metal AArch64). Out of scope for
  any host-language SDK.
- `mcp-*` crates. MCP is a coordination protocol; if a Python user
  wants MCP they use the official MCP SDK.
- `examples/*-consciousness`, `examples/*-boundary-discovery`,
  `examples/seti-*`, `examples/seizure-*`, etc. — research demos,
  not API surfaces.
- `crates/ruQu*`, `crates/ruvix/*`, `crates/cognitum-*`,
  `crates/prime-radiant`, `crates/thermorust`. Internal R&D.

## Net assessment

There is no existing Python work — confirmed by exhaustive search. This
is a clean room. The four crates that matter for v1 of a Python SDK are,
in order: `ruvector-rabitq`, `ruvector-rulake`, the embedder
(`ruvector-cnn` + ONNX glue), and `rvagent-a2a`. The NAPI template at
`crates/ruvector-diskann-node/src/lib.rs` is the structural exemplar to
follow for every PyO3 module we write.

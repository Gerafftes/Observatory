# 04 — Milestones

Same shape as ADR-159's milestone plan (`docs/adr/ADR-159-rvagent-a2a-protocol.md`
§ "Implementation plan"). Each milestone has explicit scope, a file list,
a LoC budget, an acceptance test set, the wheel platforms shipped, and the
docs that must land.

The crate `crates/ruvector-py/` is created in M1 and grows by one source
module per milestone.

---

## M1 — RaBitQ-only Python wheel

**Scope.**

- Create the new workspace crate `crates/ruvector-py/` with
  `crate-type = ["cdylib"]`, `pyo3 = "0.22"`, `numpy = "0.22"`,
  `pyo3-asyncio = "0.22"` (or successor `pyo3-async-runtimes` if pinned),
  `maturin` as the build backend in `pyproject.toml`.
- Bind exactly the four index types from `crates/ruvector-rabitq/src/index.rs`:
  `FlatF32Index`, `RabitqIndex`, `RabitqPlusIndex`, `RabitqAsymIndex`.
  All four implement the `AnnIndex` trait there.
- Bind `BinaryCode` accessors for advanced users (`ids`, `norms`, `packed`)
  even though most users will never touch them — they're cheap to expose
  and the test suite uses them.
- Bind `RandomRotation` only as an opaque handle returned by
  `idx.rotation()` — no public constructor in v1.
- Persistence: bind `crates/ruvector-rabitq/src/persist.rs` so
  `idx.save(path)` and `Index.load(path)` work for `RabitqIndex` and
  `RabitqPlusIndex` (the `.rbpx` format from `lake.rs` `PERSISTED_INDEX_FILENAME`).
- Hand-write `python/ruvector/__init__.pyi` with full stubs for the M1
  surface.
- Set up `pyproject.toml` with cibuildwheel configured for the five
  platform wheels listed in `02-strategy.md` § "Wheel distribution
  matrix", abi3-py39.
- CI: GitHub Actions workflow `python-wheels.yml` that builds + tests
  + uploads to TestPyPI on every PR, PyPI on tag.
- Pure-Python helpers in `python/ruvector/`: `_version.py` (mirrors Cargo
  version), `_typing.py` (the `AnyIndex` runtime-checkable Protocol).

**File list.**

```
crates/ruvector-py/
  Cargo.toml                  # ~30 LoC
  pyproject.toml              # ~80 LoC (cibuildwheel matrix, project meta)
  README.md                   # ~50 LoC, links docs/sdk
  src/
    lib.rs                    # ~80 LoC — PyModule init, re-exports
    rabitq.rs                 # ~450 LoC — four index types
    error.rs                  # ~80 LoC — exception hierarchy root + IndexError tree
    numpy_util.rs             # ~60 LoC — view & dtype enforcement helpers
  python/ruvector/
    __init__.py               # ~40 LoC — re-exports
    __init__.pyi              # ~200 LoC — typed stubs
    py.typed                  # 0 LoC marker
    _version.py               # ~5 LoC
    _typing.py                # ~30 LoC — AnyIndex Protocol
  tests/
    test_rabitq_basic.py      # ~120 LoC
    test_rabitq_persist.py    # ~80 LoC
    test_numpy_interop.py     # ~80 LoC
    test_errors.py            # ~60 LoC
.github/workflows/
  python-wheels.yml           # ~120 LoC
docs/sdk/notebooks/
  01_rag_in_5_lines.ipynb     # uses M1 index over toy data
```

**LoC budget.** ~700 Rust + ~340 Python tests + ~200 stubs + ~120 CI YAML.

**Acceptance tests.**

1. `pip install ruvector` from TestPyPI on each of the five platforms
   in `02-strategy.md`'s matrix succeeds in ≤ 10 s on a stock
   GitHub-hosted runner with a warm pip cache.
2. `test_rabitq_basic.py::test_100k_search_under_10ms`: build a
   `RabitqPlusIndex` over 100,000 random D=128 vectors with
   `rerank_factor=20`, run 100 single-query searches, assert
   p99 latency < 10 ms (mirrors `ruvector-rabitq/BENCHMARK.md` baseline
   of 1.05 ms/query for `RabitqPlus rerank×20` with headroom for
   Python overhead).
3. `test_numpy_interop.py::test_zero_copy_search`: build an index,
   pass a contiguous `np.ndarray[np.float32]` query, assert the call
   produces no copy via a memory-tracker fixture.
4. `test_rabitq_persist.py::test_roundtrip`: save → load → search,
   assert bit-identical results to a search before save.
5. `test_errors.py::test_dim_mismatch`: query of wrong dim raises
   `ruvector.DimensionMismatch` and the message names both expected
   and got dim.
6. `mypy --strict` on `tests/` finds no errors.
7. Wheel size ≤ 8 MB on linux x86_64, ≤ 12 MB on macOS arm64.

**Wheels shipped.** All five platforms in `02-strategy.md` § "Wheel
distribution matrix". M1 ships nothing else.

**Docs.**

- `docs/sdk/notebooks/01_rag_in_5_lines.ipynb` — derived from
  `examples/refrag-pipeline/`. Uses the M1 surface only (no embedder yet
  — uses pre-computed vectors from a fixture).
- Sphinx rST scaffold under `docs/sdk/api/` is created but only the
  RaBitQ section is filled.
- Top-level `crates/ruvector-py/README.md`.

---

## M2 — ruLake bindings

**Scope.**

- Add `crates/ruvector-py/src/rulake.rs`. Bind:
  - `RuLake` with builder (`RuLake::new` + chained `with_*` mapped to a
    Python builder pattern).
  - `BackendAdapter` exposed as a Python ABC for users to implement;
    bridges into Rust via a `PyBackendAdapter` impl that calls back into
    the Python class. (This is the only place we need GIL re-acquisition
    in M2.)
  - `LocalBackend` and `FsBackend` as concrete classes.
  - `Consistency` as an int-enum.
  - `RuLakeBundle`, `RefreshResult`, `SearchResult`, `CacheStats`,
    `PerBackendStats`.
  - All `RuLake` methods listed in `crates/ruvector-py-survey` (i.e.
    `register_backend`, `search_one`, `search_federated`,
    `search_batch`, `publish_bundle`, `refresh_from_bundle_dir`,
    `save_cache_to_dir`, `warm_from_dir`, `cache_stats*`,
    `cache_witness_of`, `invalidate_cache`).
- Add `_async` siblings for `search_one` / `search_federated` /
  `search_batch` using `pyo3_asyncio::tokio::future_into_py`. The
  underlying Rust calls are sync today (per `lake.rs`); async siblings
  exist so we don't have to break the surface when the Rust `async`
  refactor lands.
- Initialize the singleton tokio runtime here in M2 (M1 doesn't need it).
- Extend `error.rs` with the `LakeError` subtree.

**File list.**

```
crates/ruvector-py/src/
  rulake.rs                   # ~600 LoC
  runtime.rs                  # ~80 LoC — singleton tokio runtime
crates/ruvector-py/python/ruvector/
  __init__.pyi                # +180 LoC for RuLake surface
crates/ruvector-py/tests/
  test_rulake_local.py        # ~150 LoC
  test_rulake_fs_backend.py   # ~120 LoC
  test_rulake_async.py        # ~100 LoC
  test_rulake_witness.py      # ~80 LoC
docs/sdk/notebooks/
  02_warm_restart_with_witness.ipynb
```

**LoC budget.** ~680 Rust + ~450 Python tests + ~180 stub additions.

**Acceptance tests.**

1. `test_rulake_local.py::test_register_search_local`: register a
   `LocalBackend` with 50,000 D=128 vectors, run `search_one`,
   assert results match a direct `RabitqPlusIndex` search.
2. `test_rulake_async.py::test_search_one_async_in_event_loop`: run
   100 concurrent `await lake.search_one_async(...)` calls inside a
   single asyncio event loop, assert they complete in less than
   10× the sync time (no thread-fight regression).
3. `test_rulake_witness.py::test_publish_refresh_roundtrip`: publish
   bundle, mutate underlying data, re-publish, refresh, assert
   `RefreshResult.INVALIDATED`. Mirrors `lake.rs` `refresh_from_bundle_dir`
   contract.
4. `test_rulake_fs_backend.py::test_warm_restart`: prime cache, save to
   disk, kill process, start a fresh `RuLake`, `warm_from_dir`, assert
   first search after warmup is < 1.5× steady-state latency.
5. `test_rulake_local.py::test_python_backend_adapter`: a user-defined
   Python class subclasses `ruvector.BackendAdapter`, registers, gets
   called back by ruLake on cache miss. (This is the GIL re-acquisition
   round-trip.)

**Wheels shipped.** Same five platforms. Wheel size budget bumps to
≤ 12 MB linux / ≤ 16 MB macOS arm64 (tokio adds ~3 MB).

**Docs.**

- `docs/sdk/notebooks/02_warm_restart_with_witness.ipynb` — derived from
  `crates/ruvector-rulake/examples/warm_restart.rs`.
- Sphinx page for `ruLake` reference complete.

---

## M3 — Embeddings + ML helpers

**Scope.**

- Add `crates/ruvector-py/src/embed.rs`. Bind a single `Embedder` class
  with two factory functions:
  - `Embedder.from_pretrained(name)` for text. Implementation calls
    into `crates/ruvector-cnn/` for image and into a new tiny
    `crates/ruvector-py/src/onnx_embed.rs` helper for text (ONNX
    Runtime via `ort` 2.x). Text models: `all-MiniLM-L6-v2` first;
    `bge-small-en-v1.5` second.
  - `Embedder.from_pretrained` with a `mobilenetv3-*` prefix routes to
    `ruvector-cnn`'s `MobileNetEmbedder` (gated on the `backbone`
    feature in `ruvector-cnn/Cargo.toml`).
- Model weights: download once on first use into the standard
  `~/.cache/ruvector/models/` directory, verify a SHA-256 digest, cache.
  No bundled weights — the wheel stays small.
- Sync `embed`, sync `embed_batch`, async `embed_batch_async`. Async
  exists so a notebook user can interleave embedding with ruLake
  ingestion in the same event loop.
- Extend `error.rs` with `EmbedError`.

**File list.**

```
crates/ruvector-py/src/
  embed.rs                    # ~350 LoC
  onnx_embed.rs               # ~250 LoC — ort wrapper, model registry
crates/ruvector-py/python/ruvector/
  __init__.pyi                # +120 LoC for Embedder
  _models.py                  # ~80 LoC — model registry, download paths
crates/ruvector-py/tests/
  test_embed_text.py          # ~120 LoC
  test_embed_image.py         # ~100 LoC
  test_embed_to_index.py      # ~80 LoC — end-to-end RAG
docs/sdk/notebooks/
  03_text_to_search.ipynb     # full RAG: text → embed → RabitqPlus → search
```

**LoC budget.** ~600 Rust + ~300 Python tests + ~200 helpers/stubs.

**Acceptance tests.**

1. `test_embed_text.py::test_minilm_dim`: embed 100 strings, assert
   shape `(100, 384)` and dtype `float32`.
2. `test_embed_text.py::test_first_use_downloads`: in a fresh cache
   dir, `from_pretrained("all-MiniLM-L6-v2")` downloads, verifies
   SHA-256, caches; second call is no-network.
3. `test_embed_image.py::test_mobilenetv3_small_dim`: embed a
   (224, 224, 3) image, assert shape `(576,)` (matches
   `ruvector-cnn` MobileNetV3-Small dim).
4. `test_embed_to_index.py::test_e2e_rag_under_5_lines`: file is the
   acceptance gate G1 in 06-decision-record. Full pipeline, ≤ 5
   significant lines of user code, completes < 30 s on a stock laptop
   with warm model cache. (Subject to network for the *first* run only.)
5. ONNX Runtime is optional: the wheel ships without `ort` bundled in;
   image-only users `pip install ruvector` and skip the text path.
   Importing `Embedder.from_pretrained("all-MiniLM-...")` without `ort`
   raises `EmbedError("install ruvector[text]")`.

**Wheels shipped.** Same five platforms; the `ruvector` wheel does
**not** bundle `ort`. We ship a `ruvector[text]` extra that adds
`onnxruntime` as a Python-side dep (so wheel size of `ruvector` itself
stays ≤ 14 MB).

**Docs.**

- `docs/sdk/notebooks/03_text_to_search.ipynb`.
- Sphinx page for `Embedder`.
- README example block updated.

---

## M4 — A2A client

**Scope.**

- Add `crates/ruvector-py/src/a2a.rs`. Bind from
  `crates/rvAgent/rvagent-a2a/src/`:
  - `A2aClient` with `connect`, `send_task`, `get_task`, `cancel_task`,
    `stream_task`. All async (the underlying Rust API is async via
    reqwest). Sync siblings via `pyo3_asyncio::tokio::run_until_complete`
    for non-async users.
  - `AgentCard`, `AgentCapabilities`, `AgentSkill`, `AgentProvider`,
    `AuthScheme`, `Task`, `TaskSpec`, `TaskState`, `TaskStatus`,
    `Message`, `Part`, `Role`, `Artifact`,
    `TaskArtifactUpdateEvent`, `TaskStatusUpdateEvent` —
    all from `rvagent-a2a/src/types.rs` and `lib.rs` re-exports.
  - `TaskPolicy` from `rvagent-a2a/src/policy.rs`. Construction-only
    on the Python side; not modifiable post-send.
  - `TaskUpdate` discriminated dataclass returned by `stream_task`.
- Verify-on-discover (ADR-159 r2) enabled by default; `strict_verify=False`
  is exposed but documented as for-test-only.
- We **do not** bind the A2A server. Server-side rvAgent stays Rust-only
  in v1.
- Extend `error.rs` with `A2aError` subtree
  (`CardSignatureInvalid`, `PolicyViolation`, `BudgetExceeded`,
  `TransportError`).

**File list.**

```
crates/ruvector-py/src/
  a2a.rs                      # ~700 LoC
  a2a_types.rs                # ~250 LoC — type conversions for AgentCard, Task, Artifact
crates/ruvector-py/python/ruvector/
  __init__.pyi                # +220 LoC for the A2A surface
crates/ruvector-py/tests/
  test_a2a_card.py            # ~120 LoC
  test_a2a_send_task.py       # ~150 LoC
  test_a2a_stream.py          # ~150 LoC
  test_a2a_policy.py          # ~80 LoC
docs/sdk/notebooks/
  04_dispatch_to_python_peer.ipynb
```

A test fixture stands up an in-process rvAgent A2A server (using
`tokio::test`-equivalent in pytest via a test-only Rust binary
launched in a `subprocess.Popen`). The server lives in
`crates/ruvector-py/tests/a2a_test_server/` and is built once per
test session.

**LoC budget.** ~950 Rust + ~500 Python tests + ~220 stub additions.

**Acceptance tests.**

1. `test_a2a_card.py::test_fetch_signed_card`: connect to the test
   server, fetch the AgentCard, assert signature verifies and
   `agent_id` matches `SHAKE-256(pubkey)`.
2. `test_a2a_card.py::test_tampered_card_rejected`: redirect the
   client to a tampered `/.well-known/agent.json`, assert
   `CardSignatureInvalid`.
3. `test_a2a_send_task.py::test_lifecycle`: send a task, poll until
   `completed`, assert artifacts present.
4. `test_a2a_stream.py::test_stream_no_thread_fight` (acceptance gate
   G2): consume an SSE stream of 1,000 status updates inside a single
   asyncio event loop alongside two other concurrent ruLake
   `search_one_async` calls; assert no event-loop-blocked warnings,
   no thread-stuck warnings, total time < 1.2 × the maximum of the
   three workloads in isolation.
5. `test_a2a_policy.py::test_budget_exceeded`: send a task that
   violates `max_cost_usd`, assert `PolicyViolation` raised before
   any work begins.

**Wheels shipped.** Same five platforms. Wheel size budget tops out at
≤ 22 MB linux / ≤ 28 MB macOS arm64 (reqwest + rustls + axum-deps).
This is the size red line; if we trip it we ship the A2A bits as a
`ruvector[a2a]` extra with a separate wheel `ruvector-a2a`.

**Docs.**

- `docs/sdk/notebooks/04_dispatch_to_python_peer.ipynb` — derived from
  `examples/a2a-swarm/`.
- Sphinx page for `A2aClient`.
- README updated with end-to-end "Python app dispatches to rvAgent"
  walkthrough.

---

## Total sizing

| Milestone | Rust LoC | Python LoC | Tests LoC | Cum. wheel size | Calendar weeks |
|---|---:|---:|---:|---:|---:|
| M1 | ~700 | ~75 | ~340 | ≤ 8 MB | 2 |
| M2 | ~680 | ~30 | ~450 | ≤ 12 MB | 3 |
| M3 | ~600 | ~80 | ~300 | ≤ 14 MB | 2.5 |
| M4 | ~950 | ~30 | ~500 | ≤ 22 MB | 3.5 |
| **Total** | **~2,930** | **~215** | **~1,590** | **≤ 22 MB** | **~11 weeks** |

Calendar weeks assume one engineer with PyO3 experience working full-time;
double if pair-programmed; halve if not done in series (M1 and M3 can
parallelize after M1's CI is green).

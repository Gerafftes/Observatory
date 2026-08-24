# 02 — Binding Strategy

## Decision

**PyO3 + maturin, single extension module, abi3-py39, with `pyo3-asyncio`
for async bridging and a hand-written `.pyi` stub.** Built and distributed
via `cibuildwheel` in CI, published to PyPI as `ruvector`. The crate lives
in-tree at `crates/ruvector-py/`.

The rest of this document defends that choice against the four alternatives
considered, and locks in the supporting decisions (asyncio, GIL, wheels,
stubs).

## The choice space

| Option | Idea | Why we are not picking it |
|---|---|---|
| **A. PyO3 + maturin** *(chosen)* | Native Rust extension exposed as a CPython C-API module via `pyo3`, built with `maturin`. | — |
| B. CFFI over a Rust `cdylib` | Hand-roll a C ABI in `ruvector-py/` (or reuse `ruvector-router-ffi`) and let Python call it via `cffi`. | Loses the rich type story PyO3 gives for free (NumPy buffers, `Vec<T>` <-> `list`, `Result<T,E>` <-> exception, `async fn` <-> awaitable). Forces us to maintain a C header. We already maintain NAPI bindings; CFFI is a strictly worse parallel surface. |
| C. ctypes over cbindgen | Same as B, but using the stdlib `ctypes` module instead of `cffi`. | Same loss; less ergonomic; no installer to declare a build dep on; users hit a `ctypes.CDLL` import error if they pip-install on a platform without a wheel. |
| D. wasmtime-py over the existing `*-wasm` crates | Reuse `ruvector-rabitq` via a new `ruvector-rabitq-wasm` crate, run the WASM in `wasmtime-py`. | Requires writing the missing `*-wasm` crate first (rabitq has none; rulake has none). Loses 5–20× perf vs native (no SIMD escape hatch). Tokio doesn't run inside `wasm32-wasi`. Adds a 6 MB+ wasmtime runtime to every wheel. The whole point of going native is to *match* the Rust numbers, not lose half of them at the boundary. |
| E. gRPC / OpenAPI server with thin Python client | Stand up `ruvector-server` over HTTP/gRPC, ship a Python client that hits localhost. | Two-process architecture is the wrong default for a library — the user gets to deal with port allocation, server lifecycle, and serialization cost on every call. This is the right shape for a Python *service* SDK, but a vector index isn't a service; it's a data structure. |

## Why PyO3 specifically

1. **Surface area parity with NAPI is automatic.** PyO3's `#[pyclass]`
   maps onto an opaque handle the same way `#[napi]` does, and
   `#[pymethods]` maps onto `#[napi]` impl blocks. Anyone who maintains
   `crates/ruvector-diskann-node` can read and review the PyO3 module
   in `crates/ruvector-py` line-for-line.
2. **NumPy zero-copy.** `pyo3` + `numpy` (the `rust-numpy` crate) lets
   us accept `np.ndarray` and read it as `&[f32]` without a copy when
   the array is contiguous and `dtype=float32`. RaBitQ search loops on
   `&[f32]` already; this is a thin wrap.
3. **abi3 wheels.** PyO3 supports the stable ABI (`abi3-py39`), which
   means **one wheel covers Python 3.9 / 3.10 / 3.11 / 3.12 / 3.13 /
   3.14**. We do not need to ship a wheel per Python version.
   This collapses the matrix from ~25 wheels (5 versions × 5 platforms)
   to 5 wheels.
4. **Mature async.** `pyo3-asyncio` (or its successor `pyo3-async-runtimes`,
   which we should track) lets a Rust `async fn` return a Python
   `awaitable` that `asyncio.run` awaits without spawning a thread per
   call. This is the only practical way to bridge tokio without
   double-runtime-fights.
5. **Maturin is the de-facto Rust-Python build tool.** Used by polars,
   pydantic-core, cryptography (in part), tokenizers. We are not
   pioneering anything; we are taking the well-trodden path.

## Async story

**Native asyncio via `pyo3-asyncio`.** Every Rust `async fn` we expose
becomes an `async def` in Python by way of a `pyo3_asyncio::tokio::future_into_py`
wrapper. There is exactly one tokio runtime in the process: a multi-thread
runtime owned by the extension module, lazily initialized on first use,
sized to `min(8, os.cpu_count())` worker threads. We do **not** create a
runtime per call.

We do **not** use `asyncio.to_thread` or `run_in_executor` to wrap a sync
API. That works but breaks cancellation propagation and tracing context.

The main async surfaces are:

- `RuLake.search_async` (M2)
- `A2aClient.send_task` / `stream_task` (M4)
- `Embedder.embed_batch_async` (M3, optional — sync is fine for CPU work)

Sync siblings are kept for every async method (e.g. `search` and
`search_async`). Synchronous calls release the GIL via
`Python::allow_threads`; async calls return immediately and block the
tokio runtime, not the calling Python thread.

Compatibility: tested against CPython's default asyncio + uvloop. We do
not pin uvloop. We do not invent our own loop policy.

## GIL story

Every CPU-bound entry point that takes more than ~50 µs releases the
GIL via `py.allow_threads(|| { ... })` around the inner Rust call. The
list as of M3:

| Surface | Releases GIL? | Why |
|---|---|---|
| `RabitqIndex.build` | yes | dominant cost is rotation + popcount, all Rust |
| `RabitqIndex.search` | yes | scan loop, no Python interaction |
| `RabitqIndex.add` | no | one vector per call, overhead < release cost |
| `RuLake.search_*` | yes | scan + cache lookup, all Rust |
| `Embedder.embed` | yes | tensor ops |
| `A2aClient.send_task` | n/a (async) | tokio runs without holding the GIL |

This is the same calculus polars and tokenizers use. Documenting it
explicitly so the next person who adds a method knows the rule.

## Wheel distribution matrix

We ship five wheels for each release, all `abi3-py39` (works on Python
3.9+):

| Platform | Triple | Built on | Notes |
|---|---|---|---|
| Linux x86_64 | `manylinux_2_28_x86_64` | GitHub Actions ubuntu-latest | AVX2 baseline; runtime detect AVX-512 |
| Linux aarch64 | `manylinux_2_28_aarch64` | GHA ARM runners or QEMU via cibuildwheel | NEON baseline |
| macOS x86_64 | `macosx_10_15_x86_64` | GHA macos-13 | AVX2 baseline; bottlenecking on M-series users is fine, they have an arm64 wheel |
| macOS aarch64 | `macosx_11_0_arm64` | GHA macos-14 | NEON baseline |
| Windows x86_64 | `win_amd64` | GHA windows-latest | AVX2 baseline; runtime detect AVX-512 |

We **drop** musllinux, Windows arm64, and 32-bit anything. cibuildwheel
configures via `[tool.cibuildwheel]` in `pyproject.toml`. A 32-bit user
gets `pip install` falling back to sdist, which fails to build, which
is the correct outcome.

SIMD is **runtime-detected**, not compiled per-platform. ruvector-rabitq
is pure Rust without explicit AVX-512 paths today (the `kernel.rs`
`VectorKernel` trait is the extension point). We ship one binary per
platform; if/when we add an AVX-512 kernel it lives behind a runtime
CPU-feature check.

## Type stubs

**Hand-written `.pyi` stubs**, checked in at
`crates/ruvector-py/python/ruvector/__init__.pyi`. Reasons:

- `pyo3-stub-gen` is real and improving but generates noisy stubs that
  need editing anyway (it overstates `Any`, doesn't infer `Optional[...]`
  from `Option<T>` cleanly).
- The stub surface is small enough (≤ 4 modules × ≤ 40 methods) that
  hand-writing is feasible.
- We control the user-visible API shape, e.g. we want NumPy types in
  signatures (`np.ndarray[np.float32]`), not `list[float]`.

A CI job runs `mypy --strict tests/` and `pyright tests/` against an
`import ruvector` to catch stub regressions.

## Source layout

```
crates/ruvector-py/
  Cargo.toml          # crate-type = ["cdylib"], pyo3 + numpy + pyo3-asyncio
  pyproject.toml      # maturin backend; cibuildwheel config; project metadata
  README.md           # short — links to docs/sdk
  src/
    lib.rs            # PyModule init, re-exports each submodule
    rabitq.rs         # M1
    rulake.rs         # M2
    embed.rs          # M3
    a2a.rs            # M4
    error.rs          # exception hierarchy
    runtime.rs        # the singleton tokio runtime
  python/ruvector/
    __init__.py       # re-exports from the compiled module + small pure-Py helpers
    __init__.pyi      # hand-written stubs
    py.typed          # marker so mypy/pyright recognize stubs
  tests/              # pytest, runs against the installed wheel
  benches/            # asv (airspeed-velocity) over identical workloads to Rust criterion
```

The `python/ruvector/__init__.py` re-export pattern lets us add pure-Python
helpers (e.g. dataclasses for config) without forcing them through the
extension boundary.

## What this strategy explicitly does NOT do

- Does not wrap every workspace crate. We pick four crates over four
  milestones; everything else stays Rust-only.
- Does not try to be a Pythonic vector DB framework (chromadb, weaviate,
  qdrant). We are a thin, fast, typed binding to a specific Rust stack.
- Does not vendor models. The embedder downloads weights from
  HuggingFace at first use, the same way `ruvector-cnn` does in Rust.
- Does not provide an asyncio-only API. Sync siblings always exist
  for non-network calls.

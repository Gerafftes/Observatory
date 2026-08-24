# 05 — Risks and Tradeoffs

The honest reservations. Each item lists the risk, the mitigation we
plan to apply, and the unmitigated remainder we accept.

## R1 — Tokio runtime in a PyO3 extension

**Risk.** PyO3 extensions are loaded into the host CPython process. If
each method call spins up a tokio runtime, we leak threads and contend
for cores; if we share a runtime with the user's `asyncio` loop, the two
runtimes deadlock on each other. uvloop adds a third loop into the mix.

**Mitigation.** One singleton tokio multi-thread runtime per process,
lazily initialized on first async call (in `crates/ruvector-py/src/runtime.rs`
landing in M2). Sized `min(8, os.cpu_count())`. Bridged to asyncio via
`pyo3_asyncio::tokio::future_into_py`, which schedules the future onto
tokio and resolves a Python `Future` on the asyncio loop — no busy
waiting, no second loop in the same event-driven context.

For the rare interleave-heavy workload (acceptance gate G2 in M4), we
test with both default asyncio and uvloop. We do not pin uvloop and we
do not invent our own loop policy.

**Unmitigated.** A user who launches multiple `ruvector.A2aClient`
operations in a thread that does *not* have an asyncio event loop will
see a clean `RuntimeError`, not magical behaviour. We take that as
correct and document it.

## R2 — GIL releases for batched ops

**Risk.** A `RabitqPlusIndex.build` over 1M vectors in the GIL-held
state freezes the whole CPython process. Same for batched search.

**Mitigation.** Every CPU-bound entry point that takes more than ~50 µs
calls `py.allow_threads(|| inner)`. The list is enumerated in
`02-strategy.md` § "GIL story" and is part of the M1 review checklist.
A regression test in `tests/test_concurrency.py` runs an
in-asyncio search alongside a CPU-bound `numpy` op on the main thread
and asserts wall time matches `max(t_search, t_numpy)`, not their sum.

**Unmitigated.** Single-vector `add` is not GIL-released because the
release/reacquire cost dominates the work. Users hot-looping `add` in
Python instead of `add_batch` will not parallelize. Documented.

## R3 — Wheel size

**Risk.** PyO3 + reqwest + rustls + tokio + axum-deps + ort can push
wheels past 50 MB.

**Mitigation (in priority order).**

1. abi3-py39 collapses ~25 wheels to 5.
2. `strip = true` and `lto = "fat"` already set in workspace
   `[profile.release]` (`Cargo.toml` lines 286–289).
3. `ort` is **not bundled**; users opt in via `ruvector[text]` which
   pulls `onnxruntime` as a Python wheel (Microsoft already ships those).
4. M1 budget ≤ 8 MB; M4 budget ≤ 22 MB (per `04-milestones.md`).
5. **Hard line:** if M4 trips 22 MB on any platform, A2A bindings
   ship as a separate wheel `ruvector-a2a` with `ruvector` as a dep.
6. We do not vendor model weights.

**Unmitigated.** macOS arm64 wheels are systematically larger
because the Mach-O format compresses worse than ELF. We accept ~30%
overhead there.

## R4 — SIMD: NEON, AVX2, AVX-512 cross-platform

**Risk.** Building one wheel that performs on a Ryzen, an Ice Lake
Xeon, an M3 Pro, and a Graviton2 means choosing what SIMD to compile.
Compile to AVX-512 baseline → users on AVX2-only crash on `SIGILL`.
Compile to SSE2 baseline → we leave 5–20× perf on AVX-512 hardware.

**Mitigation.** **Runtime CPU feature detection.** ruvector-rabitq's
`kernel.rs` already exposes a `VectorKernel` trait + `CpuKernel` impl
+ `KernelCaps` capability struct (`crates/ruvector-rabitq/src/kernel.rs`).
The Python wheel ships *one* binary per platform, compiled with AVX2
+ NEON baseline (the `manylinux_2_28` and `macosx_11_0_arm64` floors).
At init time we probe via `is_x86_feature_detected!` / `cpufeatures`
crate and pick the best kernel.

A future AVX-512 kernel slots in via the same trait. No wheel-matrix
explosion. ARM SVE is similarly handled when/if added.

**Unmitigated.** Users on pre-Haswell x86 (no AVX2) will get
`Illegal instruction` on Linux x86_64. We document
`manylinux_2_28_x86_64` requires AVX2 and let `pip install` fall back
to sdist (which fails to build on a sufficiently old machine — correct).

## R5 — abi3 stability

**Risk.** abi3-py39 covers the stable ABI but excludes private CPython
APIs. If we ever need one (rare for vector code), we drop abi3 and the
wheel matrix re-explodes.

**Mitigation.** PyO3's macros emit only stable-ABI code under the
`abi3-py39` feature. Code review explicitly bans direct CPython
private-API calls. We have not identified any need today.

**Unmitigated.** abi3 is a one-way door. If we ever want
free-threaded Python (PEP 703) optimizations that require post-3.13
APIs, we'll need a 3.13+ specific wheel alongside abi3. Cross that
bridge in 2027.

## R6 — Tokio multi-runtime risk

**Risk.** A user imports `ruvector` in an application that already
embeds tokio via a different extension (e.g. another Rust-Python lib),
and the two each call `Runtime::new()`. Result: two independent
runtimes competing for the same cores, neither reachable from the
other's `tokio::spawn`.

**Mitigation.** Each runtime is owned by its respective extension
module. We do not hand out runtime references. Async work submitted
through `ruvector` always lands on `ruvector`'s runtime; nothing is
shared. For inter-library async coordination, users go through
asyncio (the lingua franca).

**Unmitigated.** Total OS-thread count goes up linearly in number of
loaded Rust extensions. On a low-core box this matters. We size the
runtime conservatively (`min(8, cpu)`) to avoid being the worst
offender.

## R7 — Symbol clash on PyPI

**Risk.** `ruvector` on PyPI may already be squatted or claimed by
another project. As of plan-write the author has not checked.

**Mitigation.** Before M1 starts, register `ruvector` (and
`ruvector-rabitq`, `ruvector-rulake`, `ruvector-a2a` for safety even
if we don't ship them as separate distributions today) on PyPI under
the org account. Park empty 0.0.0 placeholder packages with a single
README pointing at this repo. Cost: ~10 minutes. If any name is
already taken, we negotiate or fall back to `ruvector-py` and rename
the import in the docs accordingly.

**Unmitigated.** A determined squatter who refuses transfer would
force a rename. Open question O1 in `06-decision-record.md`.

## R8 — Repo location: monorepo vs separate

**Risk.** The Python SDK could live in `crates/ruvector-py/` (in this
monorepo) or in a separate `ruvnet/ruvector-py` repo.

**Decision.** Monorepo. `crates/ruvector-py/`.

**Why.** Every binding crate already lives here (`*-node`, `*-wasm`,
`router-ffi`). Following the precedent means:

- Reviewers diff Rust changes against their bindings in one PR.
- Workspace `Cargo.toml` pins ensure the Python wheel's ruvector-rabitq
  is bit-identical to the Rust crate's; with two repos we'd have to
  cut a tagged release on every change.
- `cibuildwheel` triggers off `crates/ruvector-py/**` path filter on
  PRs; doesn't run when only `crates/ruvix/` changes.
- Single CHANGELOG.

**Cost we accept.** Python contributors who never touch Rust have to
clone a 2-GB repo to push a 5-line stub fix. We accept that.

## R9 — CI maintenance

**Risk.** `python-wheels.yml` is the largest non-CLI workflow in the
repo (~120 LoC YAML, 5 platforms × build × test × upload). It will
break.

**Mitigation.** Use `cibuildwheel` (which is opinionated and
maintained) rather than rolling our own per-platform setup. Pin
`cibuildwheel` to a major version, dependabot-bump weekly. The
workflow is not in the critical path of Rust development — Rust CI
runs without Python.

**Unmitigated.** When PyO3 releases a major (e.g. 0.22 → 0.23), we
re-do the bindings. Major PyO3 bumps come ~2/year; the breaking
changes are mechanical. Expect ~1 person-day per bump.

## R10 — User confusion: which index?

**Risk.** Four index types (`FlatF32Index`, `RabitqIndex`,
`RabitqPlusIndex`, `RabitqAsymIndex`) are exposed in M1. A first-time
Python user picks the wrong one and concludes ruvector is slow / has
bad recall.

**Mitigation.** The README hello-world uses `RabitqPlusIndex`
(rerank=20) — the one with 100% recall@10 in the benchmark table. The
docstring on each class names the tradeoff in one sentence. We add a
top-level `ruvector.recommend(n, dim, recall_target)` helper in M1
that returns the right class for the workload, modeled on
`crates/ruvector-rabitq/BENCHMARK.md`'s recommendations.

**Unmitigated.** `FlatF32Index` users on n=10M will be sad. The
docstring tells them why.

## What we are NOT worried about

- **PyO3 itself.** Mature, used by polars + pydantic-core +
  cryptography. Not a risk.
- **maturin.** Same.
- **NumPy compat.** `rust-numpy` 0.22 covers what we need. The pin
  moves with PyO3 in lockstep.
- **Build determinism.** Workspace already pins everything; the only
  Python-side variable is the wheel build platform, which CI
  controls.

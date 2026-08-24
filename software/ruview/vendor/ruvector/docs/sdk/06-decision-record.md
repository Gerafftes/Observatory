# 06 — Decision Record (one-page summary)

## The chosen strategy

**A new in-tree workspace crate `crates/ruvector-py/` exposes the
Rust SDK through PyO3, built and distributed as a single abi3-py39
wheel via maturin + cibuildwheel.** Async surfaces use `pyo3-asyncio`
over a singleton tokio runtime; vector inputs are accepted as
zero-copy `np.ndarray[np.float32]`; type stubs are hand-written and
shipped with `py.typed`.

## Roadmap

| M | Scope | Rust LoC | Wheel cap | Calendar |
|---|---|---:|---:|---:|
| **M1** | RaBitQ index (`FlatF32`, `Rabitq`, `RabitqPlus`, `RabitqAsym`); persistence; CI publishing pipeline. | ~700 | 8 MB | 2 wk |
| **M2** | ruLake (`RuLake` builder, `LocalBackend` / `FsBackend` / Python `BackendAdapter` ABC); witness operations; sync + async search; tokio runtime singleton. | ~680 | 12 MB | 3 wk |
| **M3** | Embeddings (`Embedder.from_pretrained` for MiniLM-text and MobileNetV3-image); HF model cache + SHA-256 verification. | ~600 | 14 MB | 2.5 wk |
| **M4** | A2A client (`A2aClient.connect/send_task/stream_task/cancel_task`); typed AgentCard / Task / Artifact; signed card verify-on-discover. | ~950 | 22 MB | 3.5 wk |
| **Total** | — | **~2,930** | **22 MB** | **~11 wk** |

(One full-time engineer with PyO3 experience. Sequenceable; M3 may
parallelize after M1 ships.)

## Three acceptance gates that gate the whole effort

**G1 — RAG in 5 lines.** A user types ≤ 5 significant lines of Python
to embed a corpus, build an index, and query it with sub-10-ms p99
latency on 100k D=128 vectors. Concretely:

```python
import ruvector, numpy as np
emb = ruvector.Embedder.from_pretrained("all-MiniLM-L6-v2")
idx = ruvector.RabitqPlusIndex.build(emb.embed_batch(corpus), seed=42, rerank_factor=20)
hits = idx.search(emb.embed("my query"), k=10)
print([(h.id, h.score) for h in hits])
```

This gate clears at the end of M3.

**G2 — asyncio without thread fights.** A user awaits an A2A SSE
stream of 1,000 status updates concurrently with two ruLake
`search_one_async` calls inside a single asyncio event loop, with no
event-loop-blocked warnings, no thread-stuck warnings, and total
wall time within 1.2× of the maximum of the three workloads in
isolation.

This gate clears at the end of M4 and is enforced by
`tests/test_a2a_stream.py::test_stream_no_thread_fight`.

**G3 — `pip install ruvector` is instant.** On a stock Linux x86_64
GitHub Actions runner with a warm pip cache, `pip install ruvector`
from PyPI completes in ≤ 10 s. This is the "we ship a binary wheel,
not a sdist" gate. Enforced as a CI step that fails the release if
the timing regresses.

This gate clears at the end of M1 and stays clear forever.

## Open questions for stakeholders before M1

**O1 — PyPI name.** Is `ruvector` available on PyPI? If not, do we
negotiate transfer, register `ruvector-py`, or pick something else?
Owner: project lead. Resolution required before M1 PR is opened.

**O2 — Python version floor.** abi3-py39 covers Python 3.9–3.14+.
Are we comfortable dropping support for 3.8 (which is EOL but still
deployed)? This document assumes yes. Owner: product.

**O3 — Tokio runtime sizing default.** This document picks
`min(8, os.cpu_count())`. Is that right for the typical ruvector user?
A serving deployment on a 96-core box might want more. Decision can
slide post-M2 (env var override is cheap to add) but the default
needs to be picked once. Owner: performance engineer.

**O4 — `ort` (ONNX Runtime) coupling for M3.** The plan is to **not**
bundle `ort` and instead expose `ruvector[text]` as a Python extra
that pulls `onnxruntime` from PyPI. Confirm this is acceptable from a
"works out of the box" UX perspective. Owner: product.

**O5 — Where does the Python A2A *server* live?** Plan deliberately
ships only the client in M4. If/when a Python user wants to host an
A2A peer from inside their Python process, do they (a) embed the
Rust server via PyO3, (b) run an external rvAgent binary, or (c)
re-implement the server in Python? This document says (b). Owner:
rvAgent maintainer.

**O6 — Stable-ABI commitment.** abi3-py39 is a forward commitment:
once published, downgrading to "version-specific" wheels is a
breaking change for users on niche Python builds. Confirm we're
willing to make that commitment. Owner: maintainer.

## What "done" looks like

When M4 ships:

- `pip install ruvector` works on Linux x86_64/arm64, macOS
  x86_64/arm64, Windows x86_64.
- `import ruvector` exposes vector indexes, ruLake, embedders, and
  the A2A client.
- 100% of the public surface has hand-written type stubs.
- CI gates all three acceptance gates G1, G2, G3 on every PR.
- Four notebooks (`docs/sdk/notebooks/01..04`) walk a new user from
  hello-world to multi-agent dispatch.
- A single PyO3 crate at `crates/ruvector-py/` is the only place
  Python-related Rust code lives.

## Rejected alternatives (one-liners)

- **CFFI** — strictly worse than PyO3 for this code.
- **wasmtime-py** — loses native perf, requires writing missing
  WASM crates first, drags 6 MB runtime.
- **gRPC service + thin client** — wrong architectural shape for a
  vector index.
- **One-wheel-per-Python-version** — abi3 collapses the matrix.
- **Separate `ruvnet/ruvector-py` repo** — breaks the single-PR
  cross-binding diff workflow that NAPI bindings already enjoy.

## Source pointers

- This plan: `docs/sdk/INDEX.md` and siblings 01–06.
- Survey of existing ruvector code: `docs/sdk/01-survey.md`.
- Strategy defense: `docs/sdk/02-strategy.md`.
- API sketch: `docs/sdk/03-api-surface.md`.
- Milestone breakdown: `docs/sdk/04-milestones.md`.
- Risks: `docs/sdk/05-risks-and-tradeoffs.md`.
- Reference Rust APIs: `crates/ruvector-rabitq/src/lib.rs`,
  `crates/ruvector-rulake/src/lib.rs`, `crates/rvAgent/rvagent-a2a/src/lib.rs`.
- NAPI binding template (mirror this style in PyO3):
  `crates/ruvector-diskann-node/src/lib.rs`.
- Anchor ADRs: ADR-154 (RaBitQ), ADR-155 (ruLake), ADR-159 (A2A).

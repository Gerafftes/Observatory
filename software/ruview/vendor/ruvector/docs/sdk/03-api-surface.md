# 03 — Python API Surface

The user-visible Python API across all four milestones. Everything in this
document is what gets typed at a REPL or in a notebook. Implementation
details (PyO3 attributes, GIL handling) are in 02-strategy.

## Top-level layout

```python
import ruvector

# Vector indexes (M1) — backed by ruvector-rabitq
ruvector.FlatF32Index
ruvector.RabitqIndex
ruvector.RabitqPlusIndex
ruvector.RabitqAsymIndex

# Cache-first execution fabric (M2) — backed by ruvector-rulake
ruvector.RuLake
ruvector.LocalBackend
ruvector.FsBackend
ruvector.Consistency  # enum: FRESH | EVENTUAL | STALE
ruvector.RuLakeBundle

# Embedding (M3) — backed by ruvector-cnn + ONNX glue
ruvector.Embedder

# Agent peer protocol (M4) — backed by rvagent-a2a
ruvector.A2aClient
ruvector.AgentCard
ruvector.TaskSpec
ruvector.Task

# Cross-cutting
ruvector.RuVectorError      # base exception
ruvector.__version__         # mirrors Cargo workspace version
ruvector.cpu_features()      # runtime SIMD probe
```

Every public name above is exported from the compiled extension and
re-exported by `python/ruvector/__init__.py`.

## M1 — RaBitQ vector index

```python
import numpy as np
import ruvector

# Build from an (n, d) float32 array. Dtype is enforced; mismatch raises.
vectors = np.random.randn(100_000, 768).astype(np.float32)
idx = ruvector.RabitqPlusIndex.build(
    vectors,
    seed=42,
    rerank_factor=20,    # ADR-154 recommended for 100% recall@10 at D=128
)

# Search a single query — returns a list of (id, score) named tuples.
query = np.random.randn(768).astype(np.float32)
hits = idx.search(query, k=10)
for h in hits:
    print(h.id, h.score)

# Pythonic conveniences
len(idx)            # n vectors
idx.dim             # 768
idx.memory_bytes    # honest accounting (matches AnnIndex::memory_bytes)
idx.save("index.rbpx")
idx2 = ruvector.RabitqPlusIndex.load("index.rbpx")

# Add after build (mirrors AnnIndex::add — appends, must match dim).
idx.add(id=100_001, vector=np.random.randn(768).astype(np.float32))
```

`build()` is a classmethod, takes `np.ndarray` directly (no list copy),
releases the GIL, runs in parallel via rayon. Ergonomic but not magic:
non-contiguous, non-`float32` arrays raise immediately with a clear
message rather than silently copying.

The four index types share an `AnnIndex`-shaped Python protocol but we
do **not** expose a Python ABC; the four classes are concrete.
`isinstance(idx, ruvector.AnyIndex)` works via a runtime-checkable
`Protocol` in the stub.

## M2 — RuLake (cache-first vector fabric)

```python
import ruvector
import asyncio

# Builder pattern mirrors RuLake::new + with_*.
lake = (
    ruvector.RuLake.builder()
    .rerank_factor(20)
    .rotation_seed(42)
    .max_cache_entries(1_000_000)
    .consistency(ruvector.Consistency.FRESH)
    .build()
)

# Backends are first-class Python objects.
backend = ruvector.LocalBackend(name="hot-shard")
backend.upsert("docs", ids=[1, 2, 3], vectors=np.random.randn(3, 768).astype(np.float32))
lake.register_backend(backend)

# Sync search.
hits = lake.search_one(collection="docs", query=query, k=10)

# Async search — no thread fight; runs on the extension's tokio runtime.
async def main():
    hits = await lake.search_one_async(collection="docs", query=query, k=10)
    print([(h.backend, h.id, h.score) for h in hits])

asyncio.run(main())

# Federated search across all backends — fanout + merge by score.
hits = lake.search_federated(collection="docs", query=query, k=10)

# Bundle witness operations — surfaces the SHA3 witness from RuLake::publish_bundle.
witness = lake.publish_bundle("docs", out_dir="/tmp/bundle/")
result = lake.refresh_from_bundle_dir(key=("local", "docs"), dir="/tmp/bundle/")
assert result == ruvector.RefreshResult.UP_TO_DATE  # or INVALIDATED, BUNDLE_MISSING
```

The `(backend_id, collection)` tuple that Rust uses as a `CacheKey` is
exposed as a Python tuple — no custom class, no surprise.

`Consistency` is `enum.Enum`-like (actually `pyo3` int enum) with values
`FRESH`, `EVENTUAL`, `STALE`. We do **not** accept string consistency
levels; the type system catches typos.

## M3 — Embeddings

```python
import ruvector

emb = ruvector.Embedder.from_pretrained("all-MiniLM-L6-v2")  # downloads once, caches
vec = emb.embed("hello world")                                # np.ndarray, shape (384,)
batch = emb.embed_batch(["hello", "world", "foo bar"])        # shape (3, 384)
emb.dim   # 384

# CNN-image embedder (ADR-013). Same shape; takes (H, W, 3) uint8.
img_emb = ruvector.Embedder.from_pretrained("mobilenetv3-small")
v = img_emb.embed_image(np.zeros((224, 224, 3), dtype=np.uint8))  # (576,)
```

One `Embedder` class, two factory paths (`from_pretrained` for text,
same name for image — distinguished by model identifier prefix). All
results are `np.ndarray[np.float32]` ready to feed into a `RabitqIndex`
or `RuLake`. This is the contract that makes "RAG in 5 lines" possible
(see acceptance gate G1 in 06-decision-record).

## M4 — A2A client

```python
import ruvector
import asyncio

# Discover a peer (verifies signature per ADR-159 r2 identity).
async def main():
    client = await ruvector.A2aClient.connect("https://peer.example.com")
    print(client.card.skills)            # list[AgentSkill]
    print(client.card.agent_id)          # SHAKE-256(pubkey) per ADR-159

    # Send a task.
    spec = ruvector.TaskSpec(
        skill="rag.query",
        input="What is RaBitQ?",
        policy=ruvector.TaskPolicy(
            max_tokens=4_000,
            max_cost_usd=0.10,
            max_duration_ms=30_000,
        ),
    )
    task = await client.send_task(spec)
    print(task.status, task.id)

    # Stream task updates (SSE under the hood).
    async for update in client.stream_task(task.id):
        if update.kind == "artifact":
            print("artifact:", update.artifact)
        elif update.kind == "status":
            print("status:", update.status)

    # Cancel.
    await client.cancel_task(task.id)

asyncio.run(main())
```

`stream_task` returns an `AsyncIterator[TaskUpdate]`. `TaskUpdate` is a
tagged union exposed as a discriminated dataclass-like Python type
(`kind` field).

We do **not** expose the A2A *server* in v1 — Python users embed an
rvAgent server via the Rust binary; the Python SDK is client-only. This
keeps the wheel small and avoids dragging axum + tower into Python's
process.

## Error hierarchy

A single root, with subclasses that map onto the Rust error variants:

```
ruvector.RuVectorError                   # root, Exception subclass
├── ruvector.IndexError                   # ruvector_rabitq::RabitqError
│   ├── ruvector.DimensionMismatch        # vector dim != index dim
│   ├── ruvector.EmptyIndex               # search on n=0
│   └── ruvector.PersistError             # save/load IO + format errors
├── ruvector.LakeError                    # ruvector_rulake::RuLakeError
│   ├── ruvector.BackendError             # adapter failure, bubbles backend id
│   ├── ruvector.CacheMissError           # consistency=STRICT and miss happened
│   └── ruvector.WitnessMismatch          # bundle witness != cache witness
├── ruvector.A2aError                     # rvagent_a2a::A2aError
│   ├── ruvector.CardSignatureInvalid     # ADR-159 r2 verify-on-discover failure
│   ├── ruvector.PolicyViolation          # TaskPolicy guard fired
│   ├── ruvector.BudgetExceeded           # GlobalBudget gate fired
│   └── ruvector.TransportError           # HTTP / SSE plumbing
└── ruvector.EmbedError                    # model download / inference failures
```

Names are stable across milestones. `RuVectorError` is what users put
in their `except` blocks if they don't care which subsystem failed.

## Pythonic conveniences

| Operation | Behavior |
|---|---|
| `len(idx)` | n vectors |
| `idx[id]` | returns the original f32 vector if `RabitqPlusIndex` (which keeps originals); raises `LookupError` for `RabitqIndex` (which doesn't) |
| `for v in idx` | iterates `(id, vector)` pairs, only on indexes that retain originals |
| `idx in lake` | `__contains__` checks if a `RabitqPlusIndex` is currently primed in a `RuLake` cache (used for "did my warmup work?") |
| `np.asarray(idx)` | only on indexes that retain originals; returns the (n, d) float32 matrix without a copy |
| `with lake.session() as s` | optional context manager for batched ops; commits caches on exit |
| `repr(idx)` | shows variant, n, d, memory_bytes — diagnostic-friendly |
| `idx == idx2` | structural equality if both come from same data + seed (matches the determinism guarantee in `ruvector-rabitq/src/lib.rs` §Guarantees) |

## NumPy interop is a first-class contract

- Every vector input accepts `np.ndarray[np.float32]` directly.
- `list[float]` / `tuple[float, ...]` / Python sequences are accepted
  for ergonomic one-shot calls but copy through a NumPy buffer
  internally (documented as slower).
- Outputs are `np.ndarray[np.float32]` for vectors and Python `int` /
  `float` scalars for ids and scores.
- We do not invent a `Vector` class. NumPy is the lingua franca of
  Python ML.

## Versioning

`ruvector.__version__` mirrors the Cargo workspace version; the PyPI
release is cut at the same time as the Rust 2.x.y release. We use
trailing `.postN` for Python-only fixes (e.g. stub corrections) without
a Rust source change.

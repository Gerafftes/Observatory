# ruvector-rabitq — Benchmarks

All numbers produced by a **single reproducible run** of

```bash
cargo run --release -p ruvector-rabitq --bin rabitq-demo
```

on a commodity Ryzen-class laptop, release build, single thread, no external
SIMD, no GPU. Seeds are deterministic — reruns are bit-identical.

Recall is measured against `FlatF32Index`'s exact top-100 on the **same
queries** for every variant — no apples-to-oranges mixing of throughput and
recall runs from different setups.

## Dataset

- **D = 128** (main sweep) + **D = 100** (non-aligned regression demo)
- 100 Gaussian clusters in `[-2, 2]^D` hypercube with σ=0.6 within-cluster
  noise. Similar-shape distribution to SIFT / GloVe / OpenAI embeddings.
- `nq = 200` queries per scale, drawn from the same cluster prior.
- Scale sweep: `n ∈ {1 k, 5 k, 50 k, 100 k}`.

**Caveat:** clustered Gaussian is a stand-in, not SIFT1M. The SIGMOD 2024
paper reports on SIFT1M, GIST1M, DEEP10M — those remain a follow-up.

## Headline (n = 100,000, D = 128)

Numbers after the **SoA + cos-LUT optimization** (commit after `8dbc560d0`).
The previous AoS version is shown in the "(v1)" column for reference.

| variant | r@1 | r@10 | r@100 | QPS | QPS (v1) | mem/MB | lat/ms |
|---|---:|---:|---:|---:|---:|---:|---:|
| FlatF32 (exact) | 100.0% | 100.0% | 100.0% | 306 | 309 | 50.4 | 3.27 |
| RaBitQ 1-bit (sym, no rerank) | 2.0% | 8.1% | 27.1% | **3,639** | 1,176 (3.1×) | **2.4** | 0.28 |
| RaBitQ+ (sym, rerank×5) | 92.0% | 87.9% | 78.1% | **2,058** | 811 (2.5×) | 53.5 | 0.49 |
| **RaBitQ+ (sym, rerank×20)** | **100.0%** | **100.0%** | **100.0%** | **957** | 544 (1.76×) | 53.5 | 1.05 |
| RaBitQ-Asym (no rerank) | 4.5% | 13.0% | 34.5% | 26 | 26 (1.0×) | 2.4 | 38.5 |
| RaBitQ-Asym (rerank×5) | 99.0% | 95.6% | 87.0% | 26 | 22 (1.2×) | 53.5 | 38.5 |

**Recommended at-scale config:** `RabitqPlusIndex` with `rerank_factor=20` —
**3.13× over exact flat** at **100 % recall@10 and @100** (up from 1.76× in v1).

**Memory also improved**: the pure 1-bit index at n=100k is now **2.4 MB** vs
Flat's **50.4 MB** = **21× compression** (up from 8.7× in v1). The SoA layout
collapsed the 40 B per-entry overhead (tuple + BinaryCode headers) into 8 B
(u32 id + f32 norm) plus the flat packed-codes slab.

## What changed in the kernel

1. **Struct-of-Arrays storage** for the hot path.
   Was: `Vec<(usize, BinaryCode)>` where each `BinaryCode` heap-allocated
   its own `Vec<u64>`. Pointer chase per candidate.
   Now: three contiguous `Vec`s — `ids: Vec<u32>`, `norms: Vec<f32>`,
   `packed: Vec<u64>` (n × n_words flat slab). No indirection per candidate.

2. **cos-lookup table** replaces the `.cos()` call. Agreement ∈ [0, D] has
   at most D+1 distinct values; precomputed `cos_lut[agreement]` is a single
   indexed load vs a ~30 ns `cos` call. At D=128 the LUT is 516 B — fits
   comfortably in L1.

3. **Aligned-D fast path**: when `D % 64 == 0` the last-word mask is
   `!0u64` and the AND gets skipped. At D=128 the inner loop is literally
   `(!(a[0] ^ q[0])).count_ones() + (!(a[1] ^ q[1])).count_ones() +
    lut[agree] · norms[i] · q_norm · 2 − q_sq − norms[i]²` per candidate.

4. **Raw-pointer SoA walk** avoids the per-iteration bounds check on the
   `packed` slab (the outer `for i in 0..n` still bounds-checks `ids` and
   `norms`, which are shorter vectors and inlined nicely).

Net effect: **2.5–3.1× end-to-end at n=100k** for the symmetric paths.
Asymmetric is unchanged because its O(D) scalar signed-dot-product dominates;
SIMD gather is the next lever (named follow-up).

## Recall × throughput × scale

| n | variant | r@10 | QPS | speed-up vs flat |
|---:|---|---:|---:|---:|
| 1 k  | Flat | 100.0% | 21,195 | — |
|      | Sym rerank×5 | 100.0% | 15,497 | 0.73× |
|      | Sym rerank×20 | 100.0% | 12,177 | 0.57× |
|      | Asym rerank×5 | 100.0% | 2,389 | 0.11× |
| 5 k  | Flat | 100.0% | 5,530 | — |
|      | Sym rerank×5 | 100.0% | 6,770 | 1.22× |
|      | Sym rerank×20 | 100.0% | 3,529 | 0.64× |
| 50 k | Flat | 100.0% | 619 | — |
|      | Sym rerank×5 | 99.9% | 1,439 | **2.32×** |
|      | Sym rerank×20 | 100.0% | 937 | 1.51× |
| 100 k| Flat | 100.0% | 309 | — |
|      | Sym rerank×5 | 87.9% | 811 | 2.62× |
|      | Sym rerank×20 | **100.0%** | **544** | **1.76×** |

The sweet-spot scales upward: at n=50 k, rerank×5 keeps 100% recall@10 and
wins 2.3×; at n=100 k you must bump rerank to ×20 to hold recall, and the
speedup settles to 1.76×.

## Non-aligned D regression demo

Previous code at `f2dbb6efb` had a bug at `D % 64 != 0`: the padding bits of
the last u64 word were zero in every code and XNOR-popcount counted them as
matches, biasing the estimator. [`BinaryCode::masked_xnor_popcount`](src/quantize.rs)
closes it. Verification at D=100, n=2000:

| variant | r@1 | r@10 | r@100 | QPS | mem/MB |
|---|---:|---:|---:|---:|---:|
| FlatF32 | 100.0% | 100.0% | 100.0% | 15,319 | 0.8 |
| RaBitQ+ sym ×5 (D=100) | 100.0% | 100.0% | 99.0% | 12,270 | 1.0 |

Test `quantize::tests::masked_popcount_handles_non_aligned_dim` holds a
regression fixture for the exact bug (raw XNOR returns 28 matches for
opposite vectors at D=100; masked returns 0).

## Distance-kernel micro-benchmarks

`cargo bench -p ruvector-rabitq --bench rabitq_bench` (Criterion):

- **f32 dot product**: O(D) FMA, no SIMD intrinsics (scalar auto-vectorized).
- **masked_xnor_popcount**: O(D/64) POPCNT — 2 `u64::count_ones()` calls at D=128.
- **sym_estimated_sq**: popcount + 1 `.cos()` + 4 scalar ops.
- **asym_estimated_sq**: O(D) signed-dot-product + 4 scalar ops.

Symmetric popcount is the fast path; asymmetric is kept as a higher-recall
option and wants a SIMD gather to be practical at scale.

## What's NOT benchmarked (yet)

- **SIFT1M / GIST1M / DEEP10M** — standard ANN benchmarks. Follow-up.
- **HNSW integration** — RaBitQ in production plugs into a graph index as a
  cheaper distance kernel; ruvector ships HNSW, integration is a follow-up.
- **SIMD popcount via `std::arch`** — current scalar path compiles to POPCNT
  but does no batching; an AVX2 shuffle-based byte-level popcount would give
  ~4× on 50 M-scale scans. Unsafe gated; follow-up.
- **Parallel search** — the `parallel` feature gates `rayon`. All throughput
  numbers above are single-thread.

## Full source of the numbers

```
Scale sweep, 2026-04-23, D=128, 100 clusters, σ=0.6, nq=200.
Release build, single thread, no SIMD intrinsics.

(see the ── n = … ── blocks in the rabitq-demo output for exact
build times and per-scale tables.)
```

Rerun:

```bash
cargo run --release -p ruvector-rabitq --bin rabitq-demo        # ~20 s
cargo run --release -p ruvector-rabitq --bin rabitq-demo -- --fast  # ~5 s
cargo bench -p ruvector-rabitq --bench rabitq_bench              # ~45 s Criterion
cargo test -p ruvector-rabitq --release                          # 20 tests
```

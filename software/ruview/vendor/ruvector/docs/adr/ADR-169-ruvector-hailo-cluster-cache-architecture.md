---
id: ADR-169
title: ruvector-hailo-cluster cache architecture (LRU + TTL + fingerprint isolation + auto-invalidate)
status: Accepted
date: 2026-05-02
author: ruv
branch: hailo-backend
tags: [ruvector, hailo, cache, performance, correctness, lru]
related: [ADR-167, ADR-168]
---

# ADR-169 — Cluster cache architecture

## Context

A Pi 5 + Hailo-8 worker serves embeddings at ~333 req/s for short text
(MiniLM-L6, 384-d). RAG workloads see significant query repetition:

- **Stable corpus, churning queries** — same 100K documents re-embedded
  on every refresh: 100% miss
- **Streaming search** — each character mutates the query: 30-60% miss
- **Idempotent re-runs** — retry-after-failure or downstream cache miss
  storms: ~10% miss

Without a coordinator-side cache, every dispatch hits the worker. With
a cache, repeat queries return in nanoseconds rather than microseconds
(loopback) or milliseconds (Pi NPU).

Three correctness concerns ruled out a naive LRU:

1. **Model swap mid-fleet.** A cached vector tagged with model A served
   to a query that should now resolve against model B = silently wrong.
2. **Long-lived coordinators.** A 24-hour cache that never evicts
   accumulates stale entries unrelated to current world state.
3. **Operator-driven flush.** During a model rollover the operator
   needs an explicit "purge everything" lever.

## Decision

In-process LRU cache with three layered eviction triggers and one
client-controlled isolation key.

### Key composition

```
key = expected_model_fingerprint || \x00 || text
```

Null separator prevents `("fp:a", "bc")` colliding with `("fp:", "abc")`.
The fingerprint prefix makes a model swap (manifested as a fingerprint
change) **automatically** invalidate every entry — no explicit clear
needed in the common case.

### Eviction triggers

| Trigger | Path | Iter |
|---|---|---|
| **Capacity overflow** | `with_cache(N)` — front-of-deque eviction at insert | iter 37 |
| **TTL expiry** | `with_cache_ttl(N, ttl)` — checked on get; counts as miss + eviction | iter 43 |
| **Manual** | `cluster.invalidate_cache()` — drops all, increments evictions | iter 62 |
| **Fingerprint mismatch (auto)** | health-checker callback fires `cache.clear()` on detected drift | iter 64 |

The fourth path — auto-invalidate on health-checker mismatch — closes
the stale-vector window between a worker swap and operator response.
Pre-iter-64, ejected workers' cached results stayed in memory until LRU
or TTL kicked them out (could be hours).

### Storage

```rust
struct Inner {
    map: HashMap<String, (Vec<f32>, Instant)>,  // key → (vector, inserted-at)
    order: VecDeque<String>,                     // LRU access list
    hits: u64,
    misses: u64,
    evictions: u64,
}
```

`Mutex<Inner>` for thread-safety. The whole cache is `Arc<EmbeddingCache>`
in the cluster so the background health-checker can share a reference
for the auto-invalidate callback without unsafe lifetimes.

### Counters

`CacheStats { capacity, size, hits, misses, evictions }` — all preserved
across `clear()` so the run-lifetime "what did the cache see" picture
stays accurate. `evictions` increments on all 3 of (overflow, TTL drop,
explicit clear) — a single observable signal for "things being kicked out".

### Hit-rate empirics (iter 45 bench)

```
$ ruvector-hailo-cluster-bench --concurrency 4 --duration-secs 2 \
                               --cache 2000 --cache-keyspace 100

NO CACHE  (every key unique):  9,486 req/s,  p99 = 649µs
WITH CACHE                  : 1,094,300 req/s, p99 = 8µs, hit_rate = 99.98%
```

115× throughput speedup. 80× p99 latency drop. The cache turns a
network-bound RPC into a memory-bound `HashMap::get`.

## Consequences

**+** Repeat-query workloads see 100×+ throughput speedups
**+** Fingerprint-keyed isolation makes model rollovers automatic
**+** Auto-invalidate on health-check mismatch eliminates a stale-vector
       failure mode that's effectively undetectable from the consumer
**+** Single eviction counter — no need to disambiguate causes
**−** Cache is per-coordinator-process, not fleet-shared (Redis would be)
**−** TTL check uses `Instant` not `SystemTime` — clock-step doesn't
       affect freshness, but also means TTL is monotonic-clock relative
**−** Mutex contention possible at very high parallelism (>32 threads on
       same coordinator); not observed in current bench (4-8 threads)

## Implementation

| Iter | Addition |
|---|---|
| 37 | `EmbeddingCache::new(cap)` — capacity-only LRU, fingerprint key |
| 38 | `embed_batch_blocking` cache-aware (overlap detection, sparse RPC) |
| 39 | `--cache N` flag on embed CLI |
| 43 | `with_ttl(cap, ttl)` + entry timestamps |
| 44 | `--cache-ttl secs` flag |
| 45 | `--cache-keyspace N` on bench (drives controlled hit-rate scenarios) |
| 62 | `cluster.invalidate_cache()` |
| 64 | `health-checker on_fingerprint_mismatch` callback fires `cache.clear()` |

Tests (lib + integration): 9 cache-specific + 4 lib-level integration
+ 2 batch-specific + 2 health-checker callback = **17 cache-correctness tests**.

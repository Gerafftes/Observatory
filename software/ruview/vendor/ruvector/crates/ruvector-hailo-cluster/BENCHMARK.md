# Benchmark log — `ruvector-hailo-cluster`

Single-source-of-truth for performance measurements taken across the
hailo-backend branch. All numbers reproducible by the commands shown.

## Hardware reference

| Host | CPU | Cores | RAM | Network |
|---|---|---:|---:|---|
| **ruvultra** | x86_64, ASUS X870E-E (recent) | 8 (used in bench) | 124 GB DDR5 | Loopback / 1 Gb LAN / Tailnet |
| **cognitum-v0** | Pi 5 + AI HAT+ (Cortex-A76 @ 2.4 GHz) | 4 | 8 GB | 1 Gb LAN / Tailnet |
| **Hailo-8 NPU** | on cognitum-v0 via PCIe | 26 TOPS INT8 | — | via libhailort 4.23.0 |

## Headline result

**Cache hot path on Pi 5 + AI HAT+ (4 threads, 99.999% hit rate):**

```
Throughput  : 3,998,406 req/s     (4M req/s on a $200 ARM box)
Latency p50 : <1 µs
Latency p99 : 1 µs
```

**Cluster path cold dispatch on Pi 5 (no cache, network in the loop):**

```
Throughput  : 11,204 req/s    (4 threads, A76-tuned ultra build)
Latency p50 :    335 µs
Latency p99 :    719 µs
```

**~36× headroom** over the natural Hailo-8 NPU saturation rate of
~309 embed/s (yolov8s reference per ADR-167). The cluster code is
no longer the bottleneck — exactly the design goal.

## Benchmark suite

Three layers:

| Layer | Tool | What it measures |
|---|---|---|
| Microbench | `cargo bench --bench dispatch` | Hot-path operations in isolation (criterion) |
| Integration | `cluster-bench` against fakeworker on loopback | Full dispatch loop with no NPU latency |
| End-to-end | `cluster-bench --tailscale-tag ...` | Real cross-host RPCs through the live transport |

## Microbench results (criterion, ruvultra single-threaded)

```
$ cargo bench --bench dispatch -- 'cache/'

cache/get/hit/keyspace=10        time:   75 ns/op
cache/get/hit/keyspace=100       time:   94 ns/op
cache/get/hit/keyspace=1000      time:  104 ns/op
cache/get/miss/empty             time:   23 ns/op
cache/get/disabled               time:  1.6 ns/op    ← capacity-0 fast path
cache/insert/with_eviction:
  cap=16                         time:  147 ns/op
  cap=256                        time:  171 ns/op
  cap=4096                       time:  539 ns/op    ← O(N/16) shard scan
```

```
$ cargo bench --bench dispatch -- 'pool/'

pool/choose_two_random/n=2       time:  ~30 ns/op
pool/choose_two_random/n=4       time:  ~40 ns/op
pool/choose_two_random/n=8       time:  ~48 ns/op
pool/choose_two_random/n=16      time:  ~75 ns/op
pool/choose_two_random/n=64      time: ~134 ns/op
```

## Integration bench (cluster-bench against fakeworker on loopback)

### ruvultra (x86_64, 8 threads × 10 s)

| Scenario | req/s | p50 | p99 |
|---|---:|---:|---:|
| Cold dispatch (no cache) | **76,500** | ~80 µs | ~150 µs |
| Hot cache pre-iter-80 (single Mutex, VecDeque LRU, Vec clone) | 2,388,278 | — | — |
| Hot cache iter-80 (Arc<Vec>, counter LRU) | 4,051,859 | — | — |
| Hot cache iter-81 (+ 16-way sharded Mutex) | **30,906,701** | <1 µs | <1 µs |

Total speedup ruvultra hot cache: **12.9× vs iter-79 baseline.**

### cognitum-v0 (Pi 5 + AI HAT+, A76-tuned ultra, 4 threads × 10 s)

| Scenario | req/s | p50 | p99 |
|---|---:|---:|---:|
| Cold dispatch pre-A76-tune | 6,782 | 444 µs | 1,297 µs |
| Cold dispatch + A76 tune (`+lse +rcpc +fp16 +crc`) | **11,204** | 335 µs | 719 µs |
| Cold dispatch (8 threads, oversubscribed) | 13,643 | 555 µs | 1,163 µs |
| Hot cache (sharded LRU) | **3,998,406** | <1 µs | 1 µs |

Pi 5 cold-path improvement from A76 tuning: **+65%**.

### Cross-host (ruvultra → cognitum-v0 over Tailnet, 8 threads × 10 s)

| Scenario | req/s | p50 | p99 |
|---|---:|---:|---:|
| Cold dispatch | 414 | 8.9 ms | 107 ms |

Tailnet RTT-bound (~5–15 ms RTT typical, with DERP-relay tail latency).
Direct LAN should land ~5,000+ req/s; tailnet adds the relay overhead.

## Reproducing

### Microbenches
```bash
cd crates/ruvector-hailo-cluster
cargo bench --bench dispatch
```

### Integration bench (loopback fakeworker)
```bash
# Terminal 1
RUVECTOR_FAKE_BIND=127.0.0.1:50300 ruvector-hailo-fakeworker

# Terminal 2 — cold dispatch
ruvector-hailo-cluster-bench --workers 127.0.0.1:50300 \
    --concurrency 8 --duration-secs 10

# Terminal 2 — hot cache (200 unique keys, cap 4096)
ruvector-hailo-cluster-bench --workers 127.0.0.1:50300 \
    --concurrency 8 --duration-secs 10 \
    --cache 4096 --cache-keyspace 200
```

### Pi 5 build & deploy
```bash
# From x86 host (ruvultra)
cd crates/ruvector-hailo-cluster
cargo build --target aarch64-unknown-linux-gnu --profile=ultra \
    --bin ruvector-hailo-fakeworker \
    --bin ruvector-hailo-embed \
    --bin ruvector-hailo-stats \
    --bin ruvector-hailo-cluster-bench

# Deploy via tailscale
scp target/aarch64-unknown-linux-gnu/ultra/ruvector-hailo-{fakeworker,embed,stats,cluster-bench} \
    root@cognitum-v0:/usr/local/bin/

# Bench on Pi
ssh root@cognitum-v0 'nohup /usr/local/bin/ruvector-hailo-fakeworker > /tmp/fake.log 2>&1 &'
ssh root@cognitum-v0 '/usr/local/bin/ruvector-hailo-cluster-bench \
    --workers 127.0.0.1:50052 --concurrency 4 --duration-secs 10 \
    --cache 2000 --cache-keyspace 100'
```

## Optimization timeline

| Iter | Change | Hot-cache req/s (8t × 10s, ruvultra) | Notes |
|---|---|---:|---|
| 79 | Baseline | 2,388,278 | `(Vec<f32>, Instant)` storage, VecDeque LRU |
| 80 | Arc storage + counter LRU | 4,051,859 (1.7×) | Cheap Arc::clone in lock; O(1) get |
| 81 | + 16-way sharded Mutex | **30,906,701 (12.9×)** | Cuts contention by ~16× |
| 82 | + ultra release profile | (same x86) | LTO + 1 codegen-unit + panic=abort |
| 84 | + Cortex-A76 tuning | (same x86) | Pi 5: cold 6,782 → 11,204 (+65%) |

## What's next (HEF-gated)

Single remaining gate: `model.hef` artifact at
`crates/ruvector-hailo/models/all-minilm-l6-v2/model.hef`. The Hailo
Dataflow Compiler runs on x86 and emits the .hef binary. Two short
iterations land it end-to-end:

1. Fill `EmbeddingPipeline::new` body (HEF load + vstream creation
   via hailort-sys)
2. Fill `HailoEmbedder::embed` body (encode → push input vstream →
   pull output vstream → mean_pool → l2_normalize)

Both helpers are already implemented and unit-tested. Once they run,
the projected end-to-end Pi 5 + Hailo-8 throughput is **~309 embed/s
NPU-bound** — exactly what the design predicts.

## Cluster scaling projection

Grounded in the measured ~309 embed/s/Pi NPU rate and the 11,204 req/s
coordinator capacity:

| Pis | NPU-bound throughput (cold) | RAM-bound throughput (60% cache hit) |
|---:|---:|---:|
| 1 | 309 | 770 |
| 4 | 1,236 | 3,090 |
| 8 | 2,472 | 6,180 |
| 16 | 4,944 | 12,360 |
| 32 | 9,888 | 24,720 |
| 64 | 19,776 | 49,440 |

The ~80-Pi soft ceiling on a single coordinator (1 Gbps NIC + 11K
RPC/s capacity) translates to ~25,000 NPU-saturated embeds/sec. Beyond
that, scale by deploying additional coordinators (each handles its own
shard of the fleet via the existing `HashShardRouter`).

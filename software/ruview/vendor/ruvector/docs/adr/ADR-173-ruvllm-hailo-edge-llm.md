---
id: ADR-173
title: ruvllm on Pi 5 + Hailo-8 — edge LLM serving with NPU-accelerated prefill
status: Proposed
date: 2026-05-02
author: ruv
branch: hailo-backend
tags: [ruvector, ruvllm, hailo, llm, kv-cache, paged-attention, edge-ai, sona]
related: [ADR-167, ADR-169, ADR-171, ADR-172]
---

# ADR-173 — ruvllm + Hailo on Pi 5

## Status

**Iter 163 (2026-05-03): embedding upstream now NPU-accelerated end-
to-end via ADR-176 P1-P5. ruvllm-bridge gets 9.6× faster vectors for
free.** LLM-on-NPU (Llama prefill) still requires its own model
surgery; the BERT-6 unblock pattern is documented and reusable.

| Surface | Status |
|---|---|
| `ruvllm-bridge` JSONL stdin/stdout adapter | ✅ Iter 124-125, 8 CLI integration tests |
| Upstream embedding cluster (cpu-fallback) | ✅ Iter 134/137: --features cpu-fallback path stayed as the failover; ~7 embeds/sec on Pi 5. |
| Upstream embedding cluster (NPU) | ✅ Iter 156b-163: HEF compiled + integrated end-to-end. **67.3 embeds/sec/worker on Pi 5**, 9.6× over cpu-fallback. ruvllm-bridge sees this transparently — same gRPC contract, just faster vectors. See ADR-176 for the integration EPIC. |
| Llama-class HEF compile | ❌ Same Hailo-8 limits but the iter-153 Keras monkey-patch + iter-156 single-input pattern from ADR-176 P1 is reusable for any transformer encoder. Llama prefill needs the same surgery (host-side embedding lookup + encoder-only HEF + post-NPU pool). |
| GenAI HEFs from Hailo Model Zoo | ❌ All target hailo10h, none ship for hailo8 (verified iter 124) |

**Practical interpretation:** ruvllm bridges into the embedding cluster
today and gets real vectors for RAG retrieval / context embedding via
cpu-fallback. The NPU acceleration story for the LLM prefill itself is
the same multi-day model-graph surgery as ADR-167's BERT-6 path —
documented but not scheduled, because cpu-fallback covers the embedding
seam (which is what `ruvllm-bridge` actually consumes).

---

**Earlier (iter 125) snapshot** preserved below for context.

**Host-side seam implemented** as of iter 125 (2026-05-02). Real LLM
inference still requires HEF compile of the Llama-class prefill heads
(vendor x86 host tooling — outside this repo's scope).

| Iter | What landed |
|---:|---|
| 124 | New `ruvllm-bridge` bin under `crates/ruvector-hailo-cluster/src/bin/`. JSONL stdin/stdout adapter — any ruvllm process spawns it as a subprocess, sends `{"text":"..."}` lines, gets `{"dim":N,"latency_us":X,"vector":[...]}` lines back. Carries the request_id (ULID) through unchanged. Same TLS/mTLS/§2a flag set as iter-115's mmwave-bridge. ~260 LOC, hand-rolled JSON parser to keep the bin link surface small. |
| 125 | 8 committed CLI integration tests in `tests/ruvllm_bridge_cli.rs`: single + multi-line + request_id propagation, blank-line skip, malformed-request error-line + continue, no-workers gate, §2a fp+cache gate, --help, --version. |

**Why this seam exists today, before HEFs:** ruvllm processes that need
RAG retrieval / context embedding don't want to link tonic. A thin
local subprocess that takes JSON in and gives JSON out is the
universal escape hatch — works from any language, drops cleanly into
existing process trees, surfaces cluster errors as JSON lines without
killing the bin. When the HEF compile pipeline lands and the cluster's
`HailoEmbedder` serves real semantic vectors, this bridge's input/
output contract doesn't change — same JSONL, just real embeddings.

**Still unimplemented on this branch:**
- LLM serving on the NPU itself (the Llama prefill heads). Requires:
  1. Hailo Dataflow Compiler runs against Llama-class ONNX
     (vendor tooling, x86 host, blocked)
  2. New `crates/ruvllm-hailo/` (~1500 LOC) loading the HEFs,
     driving the Cortex-A76 decode loop, exposing a streaming
     gRPC API
  3. Tokio integration for KV-cache management
- MicroLoRA adapter swap mechanics (planned iter; needs HEF too).

**Hailo Model Zoo GenAI reality-check (verified 2026-05-02 via
[hailo-ai/hailo_model_zoo_genai](https://github.com/hailo-ai/hailo_model_zoo_genai)):**

Hailo distributes pre-compiled GenAI HEFs for:
- `deepseek_r1`
- `llama3.2/1b` (Q4_0)
- `qwen2`, `qwen2.5`, `qwen2.5-coder`, `qwen3`

**All target `hailo10h`** — the field `hef_h10h` is the only artifact
in their manifest.json files. No `hef_h8h` / `hef_hailo8` field is
present anywhere in the GenAI zoo. The Pi 5 + AI HAT+ runs Hailo-8,
so Path B (download a pre-compiled GenAI HEF) is a **non-starter for
this hardware today.**

The realistic ruvllm-on-Pi-5 path is the same Path A as ADR-167:
operator installs Hailo Dataflow Compiler, runs it against a
Llama-class ONNX, produces a hailo8-targeted HEF locally. The
deploy/compile-hef.sh from iter 131 + deploy/setup-hailo-compiler.sh
from iter 132 are reusable for that compile (the Llama prefill ONNX
gets substituted for all-MiniLM in the optimum-cli export step).

Companion to ADR-171 (brain + ruview + LoRa). Together
171/173 define the four workloads sharing each Pi 5 + AI HAT+ edge node:

```
                    Pi 5 + AI HAT+ (cognitum-v0)
        ┌───────────────────────┼───────────────────────┐
        │                       │                       │
ruvector-hailo-worker      mcp-brain                ruview              ruvllm-worker
(embed via Hailo NPU)   (MCP client → pi.ruv.io)  (CSI→pose, NPU)    (LLM serve, NPU prefill)
        └─────────────── /dev/hailo0 (vdevice time-sliced) ──────────────┘
```

## Context

The cluster crate (`ruvector-hailo-cluster`) ships with one workload
class — text embeddings — bound to one tonic service per Pi. The same
Hailo-8 NPU is capable of running other transformer-class workloads:

* **Pose / vitals** (ADR-171, ruview)
* **LLM prefill matmul layers** (this ADR)

`crates/ruvllm` already exists in the workspace — a Rust runtime for
LLM serving with paged-attention, KV cache, and SONA learning. It
currently runs CPU-only. This ADR scopes adding a Hailo backend so the
prefill/decode bottleneck moves from the Pi 5's 4 Cortex-A76 cores to
the Hailo-8's 26 TOPS.

## Decision

### 1. Hailo NPU as the prefill accelerator

LLM inference splits into two phases with very different characteristics:

| Phase | Workload | Hailo fit |
|---|---|---|
| **Prefill** | One forward pass over the prompt → KV cache populated | **Excellent** — pure matmul, fits the Hailo dataflow model |
| **Decode** | Token-by-token autoregressive forward pass | **Poor** — KV-cache lookups + per-token control flow + small matmul; GPU/CPU better |

ruvllm-hailo offloads prefill to the NPU and keeps decode on CPU. The
resulting latency profile (estimated for a 7B Q4 model with 512-token
prompts):

| Phase | CPU-only (Pi 5) | CPU + Hailo prefill |
|---|---:|---:|
| Prefill (512 tokens) | ~12 s | **~0.4 s** (30× from NPU) |
| Decode (per token) | ~250 ms | ~250 ms (unchanged) |
| TTFT (time-to-first-token) | ~12 s | **~0.4 s** |
| Tokens/sec sustained decode | ~4 t/s | ~4 t/s |

The prefill-only acceleration unlocks **interactive use** on the Pi —
a 30× TTFT improvement is the difference between "demo" and "useful".

### 2. HEF compilation strategy

A 7B model has ~30 transformer blocks. Two compilation choices:

**Option A — One HEF per layer** (32 HEFs total)
- Pros: easy to compile per-block; reload one block at a time on swap
- Cons: 32 vstream context-switches per prefill = significant overhead

**Option B — Fused multi-layer HEF** (1-4 HEFs covering all blocks)
- Pros: minimal vstream switches; matches Hailo Dataflow Compiler's idiom
- Cons: large HEF binary (50-200 MB); slow to load on cold start

**Decision: B with N=4** (8 blocks per HEF). Balances load latency
(~5 s cold start, acceptable for daemon mode) vs prefill throughput
(~5 vstream switches per prompt, well within budget).

### 3. Quantization for Pi RAM budget

Pi 5 has 8 GB RAM. Workload budget on a fully-loaded edge node:

| Workload | RAM budget |
|---|---:|
| Linux + journald + sshd | ~500 MB |
| ruvector-hailo-worker (with cache cap=4096) | ~250 MB |
| mcp-brain (HTTPS client + small in-memory cache) | ~50 MB |
| ruview (CSI ring buffer + pose model state) | ~500 MB |
| **ruvllm budget** | **~6.5 GB** |

A 7B model in:
- FP16: ~14 GB → **doesn't fit**
- Q8: ~7 GB → tight, no headroom for KV cache
- **Q4 (recommended)**: ~3.5 GB model + ~2.5 GB KV cache (4K context) = ~6 GB → fits

**Decision: Q4 quantization mandatory for 7B on Pi 5.** Smaller models
(3B, 1.5B) can run higher precision and leave more headroom.

### 4. Vdevice time-slicing

`/dev/hailo0` is shared between four workloads. Hailo's vdevice
abstraction natively supports multi-process scheduling — each process
holds its own vdevice handle, the firmware schedules.

Latency budget per workload (target, 1 NPU):

| Workload | Time slice | Frequency |
|---|---:|---|
| Embedding (ruvector-hailo-worker) | 3 ms | on-demand |
| Pose (ruview) | 3 ms | 30 Hz steady |
| LLM prefill (ruvllm) | 400 ms | per-prompt |
| Reserved scheduler overhead | 5% | continuous |

Steady-state worst case: ruview 3 ms × 30 Hz = 90 ms/s (9% NPU). Leaves
~91% for embedding + LLM. Bursty embedding fits in idle gaps; LLM
prefill blocks ruview/embed for ~400 ms when active. Acceptable for
edge-AI use cases (no real-time guarantees promised).

### 5. ruvllm transport extension

Mirror the existing `EmbeddingTransport` trait (ADR-167 §8.2):

```rust
pub trait LlmTransport {
    fn generate(
        &self,
        worker: &WorkerEndpoint,
        prompt: &str,
        max_tokens: u32,
        request_id: &str,
    ) -> Result<TokenStream, ClusterError>;

    fn health(&self, worker: &WorkerEndpoint)
        -> Result<HealthReport, ClusterError>;
}
```

`HailoLlmCluster` parallels `HailoClusterEmbedder` — same P2C+EWMA
dispatch, same fingerprint enforcement, same auto-cache-invalidate on
drift. The cache is conceptually different though: KV-cache lookups
are per-prompt-prefix, not per-input-text. New cache type:

```rust
pub struct PrefixCache {
    /// (prompt_prefix_hash) → KV-cache shards from prefill
    /// Reuses the 16-shard Mutex pattern from EmbeddingCache (ADR-169)
}
```

### 6. SONA learning loop

ruvllm has SONA integration baked in. SONA pulls (query, output,
verdict) triples and feeds them into the learning model. On-edge SONA:

1. Each ruvllm-worker logs (prompt, response) to a local trajectory file
2. mcp-brain.service uploads trajectories to pi.ruv.io periodically
   (with PII stripping per ADR-172 §7a)
3. Cloud Run brain aggregates across the fleet, distills patterns
4. Patterns flow back as routing hints (which prompts to pre-prefill,
   which embeddings to pre-warm in cache)

This is the **federated learning loop**: every Pi contributes to the
shared brain; brain-derived patterns flow back as cache + dispatch hints.

## CLI surface (planned)

Mirror of the embedding tools, sharing flag conventions per ADR-168:

```bash
# Single Pi
ruvllm-worker --model /var/lib/ruvllm/models/llama3-7b-q4 \
              --bind 0.0.0.0:50058

# Cluster client
ruvllm-cluster-prompt --workers-file deploy/llm-fleet.manifest \
                      --auto-fingerprint --validate-fleet \
                      --max-tokens 512 --temperature 0.7 \
                      --request-id "${TRACE_ID}" \
                      --prompt "What is the capital of France?"

# Stats / health
ruvllm-stats --tailscale-tag tag:ruvllm-worker --watch 30 \
             --prom-file /var/lib/node_exporter/textfile_collector/ruvllm.prom
```

## Implementation roadmap (post-merge)

| Iter | Item | Notes |
|---|---|---|
| 91 | `LlmTransport` trait + `RuvllmHailoTransport` impl | Mirrors EmbeddingTransport |
| 92 | HEF compilation pipeline for fused 8-layer transformer block | Hailo Dataflow Compiler input → 4 HEFs per model |
| 93 | `ruvllm-hailo-worker` binary + systemd unit | Sibling to ruvector-hailo-worker |
| 94 | `PrefixCache` (KV-cache-aware) + ADR-169 §extension | Same 16-shard Mutex idiom |
| 95 | SONA trajectory logging + brain upload via mcp-brain | Closes the federated learning loop |
| 96 | `ruvllm-cluster-prompt` + `ruvllm-stats` CLIs | Mirrors embed/stats parity |
| 97 | Quantization tooling: Q4 export from a HuggingFace model → HEF input | Final piece for self-serve model deploy |

## Out of scope

* Multi-GPU LLM serving — the Hailo-8 has fixed NPU, no expansion path
* Speculative decoding — too complex for v1; revisit if decode becomes
  the dominant bottleneck
* RLHF / fine-tuning on edge — ruvllm provides inference only; training
  remains a Cloud Run / x86 host concern

## Combined edge-node deployment (ADR-167 + 171 + 173)

```bash
# One install.sh deploys all four workloads on a Pi 5 + AI HAT+
sudo ./deploy/install.sh --workloads embed,brain,pose,llm

# Systemd state after install:
$ systemctl status ruvector-hailo-worker.service     # embed RPC
$ systemctl status mcp-brain.service                 # brain MCP daemon
$ systemctl status ruview.service                    # CSI → pose
$ systemctl status ruvllm-worker.service             # LLM prefill via NPU

# All four sharing /dev/hailo0 via vdevice scheduler.
# Aggregate fleet observability via:
$ ruvector-hailo-stats --tailscale-tag tag:ruvector-hailo-worker --json | jq
$ ruvllm-stats          --tailscale-tag tag:ruvllm-worker          --json | jq
```

A 4-Pi cluster ($800 capex, ~30W power, no GPU) running this stack
provides:
- Embedding inference: ~1,200 embed/s NPU-bound
- LLM serving: 4 prompts/sec prefill + 16 tokens/sec aggregate decode
- Pose / vitals via WiFi CSI: 4 simultaneous person-tracks
- Brain-aware federated learning across all 4 nodes

That's a real edge-AI cluster competitive with a single mid-range GPU
host on cost+watts, with hardware redundancy + zero-cloud baseline
operation.

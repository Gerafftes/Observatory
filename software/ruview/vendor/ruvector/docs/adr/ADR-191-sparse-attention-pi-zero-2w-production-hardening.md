---
adr: 191
title: "Pi Zero 2W production hardening for ruvllm_sparse_attention: decode-deadline API, warm-up hook, config preset, stochastic Q4 pass-through"
status: proposed
date: 2026-05-07
authors: [ruvnet, claude-flow]
related: [ADR-181, ADR-183, ADR-184, ADR-189, ADR-190]
tags: [sparse-attention, pi-zero-2w, cognitum, production, deadline, warm-up, stochastic-dequant]
---

# ADR-191 — Pi Zero 2W production hardening for ruvllm_sparse_attention

## Status

**Proposed.**

## Context

`ruvllm_sparse_attention` was integrated into the cognitum-agent
(`cognitum-one/seed` PR #133) as the inference kernel for SmolLM2-135M
running on a Raspberry Pi Zero 2W (Cortex-A53 @ 1 GHz, 512 MB RAM,
no NPU). The deployment is now active on cognitum-v0
(`100.77.59.83`) and has surfaced four pieces of production data the
crate's current API does not directly accommodate.

### 1 — Cold-vs-warm inference latency gap

Measured on cognitum-v0 with the current `forward_gqa_flash` +
`KvCacheF16` configuration, SmolLM2-135M Q4_0 GGUF, prompt ≤ 96 tokens,
32 tokens generated:

| Run | Wall-clock | Per-token mean |
|---|---|---|
| Cold (first inference after process start) | **99 s** | 3.10 s |
| Warm (subsequent inferences) | **56 s** | 1.75 s |
| Ratio | **1.77×** | 1.77× |

The 1.77× cold→warm penalty is consistent across 60+ production
inferences. Profiling (perf counter sampling) attributes the gap to
L1d/L2 cache misses on the first KV-cache fill and on the first sparse
candidate selection. The current crate has no API to surface or
mitigate this.

### 2 — Decode wall-clock unboundedness

`SubquadraticSparseAttention::decode_step` returns when the work is
done; there is no deadline parameter. In the cognitum-agent pipeline a
sensor-tick triggers an LLM summary on anomaly. If the very first
trigger lands on a cold cache, the 99 s decode blocks the sensor loop
for the entire window. Cognitum worked around this by:

1. Wrapping the entire `generate_with_fallback` call in a 90 s
   wall-clock deadline checked between decode steps (`sparse_llm_loader.rs`
   `decode_deadline` in PR #133).
2. Spawning the inference into a `std::thread` guarded by an
   `INFERENCE_BUSY: AtomicBool` so the sensor loop continues.

The deadline check is integration-level and only granular at the
**per-token** boundary. A single decode step that itself exceeds the
deadline (cold attention candidate-set construction has been seen at
8 s in the worst case) can still overshoot. The crate has the
information needed to enforce a sub-step deadline (it owns the inner
candidate-selection / attention loops); the integration does not.

### 3 — Empirical good config not yet codified

The cognitum-agent pin in `Cargo.toml` records the Pi Zero 2W profile
as a comment, not an API:

```toml
ruvllm_sparse_attention = {
    git = "https://github.com/ruvnet/RuVector",
    optional = true,
    default-features = false,
    features = ["fp16"],
}
# Sparse attention kernel — Pi Zero 2 W profile (window=64, tile=16, FP16 KV)
```

The values `window=64`, `tile=16`, `fp16` were validated empirically
across a 256-tick A/B run on cognitum-v0 (lower window starves
attention; larger window doubles RSS). Every downstream consumer
targeting Pi Zero 2W must re-discover these defaults from a comment.

### 4 — Stochastic dequant decoupling

PR #133 added stochastic Q4_0 / Q4_K dequant in cognitum-agent's
`sparse_llm_weights.rs` (commit `1675c20`, "feat(sparse-llm):
stochastic Q4 dequantization"). The motivation — uniform `[-0.5, 0.5)`
dither in nibble units gives an unbiased reconstruction
`E[y'] = original_value`, breaking up systematic grid-snap artifacts —
applies equally to the FP16 KV cache reload path inside this crate
(`KvCacheF16::decode_step_f16`). Today the dequant flag is a
process-global atomic in cognitum-agent and the crate's KV reload
ignores it.

## Decision

We will land four changes in `ruvllm_sparse_attention`:

### Decision 1 — `decode_step_with_deadline` family

Add a deadline-aware variant to every decode entry point:

```rust
impl SubquadraticSparseAttention {
    /// Same as `decode_step` but returns `Err(AttentionError::DeadlineExceeded)`
    /// if `wall_clock_now()` passes `deadline` at any of the sub-step
    /// checkpoints (candidate selection, dot-product loop, softmax,
    /// output projection). The deadline is checked at boundaries that do
    /// not require unwinding partial work.
    pub fn decode_step_with_deadline(
        &self,
        q: &Tensor3,
        kv: &mut KvCache,
        deadline: std::time::Instant,
    ) -> Result<Tensor3, AttentionError> { ... }

    /// FP16-cache variant.
    pub fn decode_step_f16_with_deadline(
        &self,
        q: &Tensor3,
        kv: &mut KvCacheF16,
        deadline: std::time::Instant,
    ) -> Result<Tensor3, AttentionError> { ... }

    /// Batch variant.
    pub fn decode_batch_with_deadline(
        &self,
        q: &Tensor3,
        kv: &mut KvCache,
        deadline: std::time::Instant,
    ) -> Result<Tensor3, AttentionError> { ... }
}

pub enum AttentionError {
    // ... existing variants
    DeadlineExceeded { elapsed_ms: u64, checkpoint: &'static str },
}
```

`checkpoint` reports which inner phase tripped the deadline so callers
can profile (e.g. `"candidate_selection"`, `"dot_product"`,
`"softmax"`, `"o_proj"`).

### Decision 2 — `SparseAttentionConfig::pi_zero_2w()` preset

Codify the empirically validated Pi Zero 2W config as a typed preset:

```rust
impl SparseAttentionConfig {
    /// Defaults validated on cognitum-v0 (Pi Zero 2W, SmolLM2-135M Q4_0).
    /// window=64, tile=16, FP16 KV cache, decode-only landmark refresh.
    /// Cold-warm latency 99→56 s for 32 generated tokens at ~1.8 s/tok.
    pub fn pi_zero_2w() -> Self {
        Self {
            window: 64,
            tile: 16,
            backend: AttentionBackend::FlashSparse,
            kv_cache_dtype: KvDtype::F16,
            landmark_refresh: LandmarkRefresh::DecodeOnly,
            ..Self::default()
        }
    }
}
```

The preset is **additive** — `default()` does not change. Other
production presets follow the same shape (`pi_5_hailo10h()`,
`x86_dev()`) in subsequent ADRs.

### Decision 3 — `warm_up()` hook

Add a side-effect-free priming method that runs a synthetic 1-token
decode against a zero-filled KV cache to warm L1d/L2 caches and the
branch predictor before the first user inference:

```rust
impl SubquadraticSparseAttention {
    /// Run a one-token synthetic decode with zero-filled Q/K/V so the
    /// next user-facing decode lands warm. Intended to be called once
    /// after model load. ~3.1 s on Pi Zero 2W; reduces first-real-decode
    /// latency from ~3.1 s/tok to ~1.8 s/tok.
    pub fn warm_up(&self) {
        let dim = self.config.head_dim;
        let kv_heads = self.config.num_kv_heads;
        let q = Tensor3::zeros(1, self.config.num_heads, dim);
        let k = Tensor3::zeros(1, kv_heads, dim);
        let v = Tensor3::zeros(1, kv_heads, dim);
        let _ = self.forward(&q, &k, &v); // discard output
    }
}
```

This is opportunistic — callers can choose to spend ~3 s of cold-start
once at model-load to amortize the cold→warm gap across all subsequent
decodes. Cognitum-agent will call this from its lazy-load `OnceCell`
initializer.

### Decision 4 — Stochastic Q4 dequant pass-through for KV reload

Add an optional flag and feature:

```rust
[features]
stochastic-dequant = []  # off by default

impl SparseAttentionConfig {
    /// When `true` and feature `stochastic-dequant` is enabled, KV
    /// cache reload from FP16 → FP32 adds U(-0.5/(2^15), 0.5/(2^15))
    /// dither per element so reconstruction is unbiased. No effect on
    /// FP32 cache.
    pub stochastic_kv_dequant: bool,
}
```

When the feature is off (the default) the field exists but the
reload path is bit-exact deterministic. When on, the splitmix64
seeded xorshift pattern from cognitum-agent's `sparse_llm_weights.rs`
is reused (the seeding is critical — naïve `seed | 1` collapses
adjacent seeds 42 and 43 to the same xorshift state, see PR #133
discussion).

## Consequences

### Positive

- **Bounded sensor-tick latency.** With `decode_step_with_deadline`,
  cognitum-agent can replace its outer 90 s wall-clock check with a
  per-step deadline and get sub-token granularity. Other latency-
  sensitive consumers (e.g. real-time speech) gain the same primitive.

- **One-line Pi Zero 2W setup.** Consumers no longer have to copy a
  comment block of magic numbers; `SparseAttentionConfig::pi_zero_2w()`
  is a typed contract this ADR commits to.

- **Predictable first inference.** `warm_up()` makes the cold→warm gap
  an explicit, opt-in cost paid at model load rather than an implicit
  spike in the first user response.

- **KV-quantization correctness on long contexts.** Stochastic dequant
  on FP16 KV reload removes a systematic bias that compounds across
  long context windows.

### Negative

- **API surface growth.** Three new methods on
  `SubquadraticSparseAttention` and one on
  `SparseAttentionConfig`. Mitigated by keeping the deadline variants
  as additions (existing methods unchanged) and the preset/warm-up as
  pure additions.

- **`AttentionError::DeadlineExceeded` is a new public variant.**
  Anything `match`-ing exhaustively against the existing enum will
  break at compile time. Acceptable: the enum is `#[non_exhaustive]`
  in the current crate per ADR-187.

- **Stochastic dequant is non-deterministic by default once enabled.**
  Tests that rely on bit-exact reconstruction must opt out of the
  feature flag or pass a fixed seed (the design forwards seed control
  through `SparseAttentionConfig::stochastic_seed: Option<u64>`).

### Neutral

- The 1.77× cold→warm gap is **not** removed — `warm_up()` shifts the
  cost rather than eliminating it. Eliminating it requires changes
  outside this ADR's scope (e.g., AOT page-faulting the model file
  via `mlock`, or pre-computing landmark blocks at model-load time).

## Alternatives considered

### A) Accept the integration-level workarounds (status quo)

Cognitum-agent's outer 90 s deadline + `INFERENCE_BUSY` thread guard
already work in production. Rejected because:

1. The granularity is per-token, not per-step — a single cold attention
   step can overshoot.
2. Every consumer must re-implement the same scaffolding.
3. The crate, not the integration, owns the data needed for accurate
   accounting (per-checkpoint elapsed time).

### B) Expose internal phase timings instead of a deadline

Instead of a `decode_step_with_deadline`, return a richer
`DecodeStepStats` from `decode_step` and let callers decide. Rejected
because callers cannot **abort** a step in flight without crate-side
cooperation; observation is necessary but not sufficient.

### C) Make `pi_zero_2w()` the new default

The default `SparseAttentionConfig` would become Pi-Zero-friendly
(window=64, tile=16, FP16) and other targets opt out via
`x86_dev()`/`pi_5_hailo10h()`. Rejected because the existing default
is already documented across ADR-181/183/189/190 and downstream
consumers (Hailo cluster) depend on it. Adding a preset is
additive; changing the default is breaking.

### D) Build dequant into this crate (full Q4 pipeline)

Move the `dequant_q4_0_stochastic` / `dequant_q4_k_stochastic` from
cognitum-agent's `sparse_llm_weights.rs` into this crate so the full
Q4→attention path is one-stop. Rejected for now because:

1. Q4 dequant is a per-weight concern (Q/K/V projection, FFN, lm_head)
   that affects more than just the attention kernel.
2. The natural home is `ruvllm` (the parent crate) or a new
   `ruvllm_quant` crate. A separate ADR (probably 192) will propose
   that move.

This ADR limits the dequant change to the **KV cache reload path**
which is unambiguously inside this crate's scope.

## Test plan

- [ ] Unit test: `decode_step_with_deadline` returns
      `AttentionError::DeadlineExceeded { checkpoint: "candidate_selection" }`
      when called with `Instant::now() - Duration::from_secs(1)` (already
      expired).

- [ ] Unit test: `decode_step_with_deadline` with `deadline =
      Instant::now() + Duration::from_secs(60)` produces the same
      `Tensor3` output as `decode_step` (deadline does not perturb
      results).

- [ ] Unit test: `SparseAttentionConfig::pi_zero_2w()` returns
      `window == 64`, `tile == 16`, `kv_cache_dtype == KvDtype::F16`,
      `backend == AttentionBackend::FlashSparse`.

- [ ] Unit test: `warm_up()` does not modify any cache state visible
      via the public API (smoke run with two calls to `forward` before
      and after; outputs identical bit-exact).

- [ ] Property test: stochastic KV dequant is unbiased — mean over
      256 trials of `decode_step_f16` with random seeds is within
      `0.06` of the deterministic decode (same bound used in
      cognitum-agent's `test_q4_0_stochastic_unbiased_mean`).

- [ ] Property test: stochastic KV dequant with `stochastic_seed =
      Some(42)` is bit-exact reproducible across two calls.

- [ ] Cluster bench: cognitum-v0 latency comparison
      pre-ADR vs post-ADR with `warm_up()` enabled. Target: cold
      first-decode ≤ 60 s (down from 99 s) at unchanged steady-state
      throughput.

## Migration

- Cognitum-agent (`cognitum-one/seed`) PR follow-up will:
  1. Replace its outer `decode_deadline` check with
     `decode_step_with_deadline` calls.
  2. Replace the `Cargo.toml` magic-number comment with
     `SparseAttentionConfig::pi_zero_2w()`.
  3. Call `warm_up()` from the `SPARSE_CACHE` `OnceCell` initializer.
  4. Remove the local `dequant_q4_0_stochastic` /
     `dequant_q4_k_stochastic` once the parent `ruvllm` crate exports
     them per ADR-192 (separate); keep the cognitum-local
     implementation for now since this ADR does not move dequant.

- Hailo-10H cluster (ADR-190) consumers are unaffected because every
  change is additive and gated behind opt-in flags or new methods.

## References

- Cognitum-agent integration PR: <https://github.com/cognitum-one/seed/pull/133>
- Cognitum-agent stochastic Q4 commit: `1675c20` on `feature/cognitum-sparse-llm-openai-compat`
- Production device: cognitum-v0 (Tailscale `100.77.59.83`)
- ADR-181 — ruvllm Pi quant integration
- ADR-183 — sparse-attention rand dev-only dependency
- ADR-184 — sparse-attention online softmax
- ADR-189 — sparse-attention KV cache incremental decode
- ADR-190 — sparse-attention GQA/MQA support

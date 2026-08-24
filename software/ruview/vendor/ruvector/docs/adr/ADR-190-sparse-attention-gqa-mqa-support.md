---
adr: 190
title: "Add GQA/MQA support to ruvllm sparse attention for Hailo-10H production models"
status: accepted
date: 2026-05-06
authors: [ruvnet, claude-flow]
related: [ADR-182, ADR-184, ADR-189, ADR-181]
tags: [sparse-attention, hailo, gqa, mqa, mistral, llama3, production-models]
---

# ADR-190 — Grouped-Query / Multi-Query Attention for Hailo-10H production models

## Status

**Accepted.**

## Context

All production models targeted for the Hailo-10H cluster use either
**GQA (Grouped-Query Attention)** or **MQA (Multi-Query Attention)**
rather than Multi-Head Attention (MHA):

| Model | Attention type | Q heads | KV heads | KV head ratio |
|---|---|---|---|---|
| Mistral-7B | GQA | 32 | 8 | 4:1 |
| Llama-3-8B | GQA | 32 | 8 | 4:1 |
| Llama-3.2-1B | GQA | 32 | 8 | 4:1 |
| TinyLlama-1.1B | MQA | 32 | 4 | 8:1 |
| Phi-2 | MHA | 32 | 32 | 1:1 |

The current `SubquadraticSparseAttention::forward` signature requires
`q`, `k`, `v` to have **identical shapes**:

```rust
fn validate_qkv(q: &Tensor3, k: &Tensor3, v: &Tensor3) -> Result<(), AttentionError> {
    if q.shape() != k.shape() || q.shape() != v.shape() {
        return Err(AttentionError::ShapeMismatch { ... });
    }
```

This blocks integration with all GQA and MQA models — which is all
production models on the cluster except Phi-2. Without this ADR,
ruvllm on the Hailo-10H cluster can only serve Phi-2 at full
correctness; all Mistral and Llama variants require expanding K/V heads
to match Q heads (a `4–8×` memory and bandwidth amplification that
defeats the purpose of GQA/MQA).

### KV head budget on Hailo-10H DDR4

For Mistral-7B at `seq=8192`, GQA reduces KV cache size from:

```
# MHA: 32 KV heads
8192 × 32 × 128 × 2 × 2 bytes (f16) = 268 MB per layer × 32 layers = 8.6 GB
```

to:

```
# GQA: 8 KV heads
8192 × 8 × 128 × 2 × 2 bytes (f16) = 67 MB per layer × 32 layers = 2.1 GB
```

This is the difference between fitting in Hailo-10H's 8 GB and not.
GQA support is not optional for Mistral-7B at long context on this
hardware.

## Decision

Generalise `Tensor3` and the attention forward to accept separate
`kv_heads` and `q_heads`, where `kv_heads` divides `q_heads`:

### 1. Extend `Tensor3` to support non-uniform head counts

Keep `Tensor3` shape as `[seq, heads, dim]`. Add a separate `kv_heads`
parameter to the forward signature rather than changing the tensor type
(avoids breaking all existing callsites):

```rust
pub fn forward_gqa(
    &self,
    q: &Tensor3,           // [seq, q_heads, dim]
    k: &Tensor3,           // [seq, kv_heads, dim]
    v: &Tensor3,           // [seq, kv_heads, dim]
) -> Result<Tensor3, AttentionError> {
```

Validation:

```rust
fn validate_gqa(q: &Tensor3, k: &Tensor3, v: &Tensor3) -> Result<(), AttentionError> {
    if q.seq != k.seq || k.seq != v.seq {
        return Err(AttentionError::ShapeMismatch { q: q.shape(), k: k.shape(), v: v.shape() });
    }
    if q.dim != k.dim || k.dim != v.dim {
        return Err(AttentionError::InvalidConfig(
            format!("head dim mismatch: q.dim={}, k.dim={}", q.dim, k.dim)
        ));
    }
    if k.heads == 0 || q.heads % k.heads != 0 {
        return Err(AttentionError::InvalidConfig(
            format!("q_heads={} must be divisible by kv_heads={}", q.heads, k.heads)
        ));
    }
    if k.heads != v.heads {
        return Err(AttentionError::InvalidConfig(
            format!("k.heads={} != v.heads={}", k.heads, v.heads)
        ));
    }
    Ok(())
}
```

### 2. Head group mapping in the forward loop

```rust
let group_size = q.heads / k.heads; // e.g. 4 for Mistral-7B

for h in 0..q.heads {
    let kv_h = h / group_size;      // map Q head → KV head group

    for i in 0..seq {
        let q_row = q.row(i, h);
        // Use kv_h for all K and V lookups:
        let k_row = k.row(j, kv_h);
        let v_row = v.row(j, kv_h);
        ...
    }
}
```

MQA is the special case where `kv_heads = 1` → `kv_h = 0` always.

### 3. KV cache alignment with GQA (ADR-189)

`KvCache` stores keys at `[capacity, kv_heads, dim]` rather than
`[capacity, q_heads, dim]`, providing the full 4–8× memory reduction
in Hailo-10H DDR4.

### 4. Backward compatibility

- `forward()` remains unchanged (MHA path, `q.heads == k.heads`).
- `forward_gqa()` is the new entry point for GQA/MQA.
- A convenience wrapper `forward_auto` dispatches based on head count:

```rust
pub fn forward_auto(&self, q: &Tensor3, k: &Tensor3, v: &Tensor3)
    -> Result<Tensor3, AttentionError>
{
    if q.heads == k.heads {
        self.forward(q, k, v)
    } else {
        self.forward_gqa(q, k, v)
    }
}
```

## Consequences

### Positive

- Unlocks Mistral-7B, Llama-3-8B, Llama-3.2-1B, TinyLlama on the
  Hailo-10H cluster without KV head expansion.
- KV cache fits in Hailo DDR4 at `seq=8192` for Mistral-7B (2.1 GB
  vs 8.6 GB for MHA).
- `group_size=1` (MHA) produces identical output to current `forward()`.

### Negative

- One additional parameter (`kv_h`) in the inner loop. Modern branch
  predictors handle the constant `group_size` division well; it becomes
  a bit-shift in the common `group_size = 4` or `8` case.
- New test surface: `forward_gqa` must be tested with `group_size ∈
  {1, 2, 4, 8}`.
- `decode_step` (ADR-189) must also accept `kv_heads` — a coordinated
  change across both ADRs.

## Memory Comparison (FP32 vs FP16 KV Cache at seq=8192)

With 8 KV heads (GQA), `dim=128`, 32 layers:

| Mode | Formula | Total |
|---|---|---|
| FP32 GQA | 8192 × 8 × 128 × 2 × 4 B × 32 layers | 2,147 MB |
| FP16 GQA (`feature = "fp16"`) | 8192 × 8 × 128 × 2 × 2 B × 32 layers | 1,074 MB |
| FP16 GQA (working KV per layer) | 8192 × 8 × 128 × 2 tensors × 2 B | **536 MB** |

The 536 MB working KV at `seq=8192` leaves ~7.5 GB of Hailo-10H DDR4
for model weights (Q4K Mistral-7B ≈ 4.1 GB), OS, ruvector cluster
coordinator, and NPU driver state.

## Flash-Sparse GQA Extension

`forward_gqa_flash` implements the same 3-phase IO-optimal tiling as
`forward_flash` (see ADR-184), but with the GQA head mapping
(`kv_h = h / group_size`). `forward_auto` dispatches to
`forward_gqa_flash` automatically when `q.heads != k.heads`:

```rust
pub fn forward_auto(&self, q: &Tensor3, k: &Tensor3, v: &Tensor3)
    -> Result<Tensor3, AttentionError>
{
    if q.heads == k.heads { self.forward(q, k, v) }
    else { self.forward_gqa_flash(q, k, v) }
}
```

## Cluster Validation

25/25 tests pass on all 4 cognitum cluster nodes (Pi 5 + Hailo-10H).
New tests added for this ADR:

- `forward_gqa_flash_matches_forward_gqa` — tiled GQA matches reference
- `forward_gqa_group1_equals_forward` — group_size=1 degenerates to MHA

Full cluster table in ADR-186.

## Validation

```bash
# GQA equivalence: group_size=4, verify against reference MHA expanded manually
cargo test -p ruvllm_sparse_attention -- gqa

# Mistral-7B smoke: generate 32 tokens on Hailo-10H with GQA path
ruvllm_bench --model mistral-7b --hailo --seq 512 --gqa
# Target: non-NaN outputs, KV cache size = 67 MB/layer (not 268 MB)
```

## Sequence of delivery

1. ADR-183: `rand` → dev-dep (**done in parallel with this ADR**)
2. ADR-184: one-pass softmax (**prerequisite for any perf work**)
3. ADR-185: non-causal landmark fix (**correctness prerequisite**)
4. ADR-186: edge-case tests (**CI gate**)
5. ADR-187: `zeros` overflow check (**safety**)
6. ADR-188: stamp comment (**docs, no block**)
7. ADR-189: KV cache incremental decode (**required before ADR-190**)
8. **ADR-190: GQA/MQA** ← this ADR (**final integration unlock**)

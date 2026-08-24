---
adr: 187
title: "Add overflow-checked shape multiplication to Tensor3::zeros"
status: accepted
date: 2026-05-06
authors: [ruvnet, claude-flow]
related: [ADR-186, ADR-184, ADR-182]
tags: [sparse-attention, hailo, safety, tensor, memory]
---

# ADR-187 — Overflow-checked shape multiplication in `Tensor3::zeros`

## Status

**Accepted.**

## Context

`Tensor3::from_vec` correctly uses checked multiplication to validate
the caller-supplied shape:

```rust
let expected = seq
    .checked_mul(heads)
    .and_then(|v| v.checked_mul(dim))
    .ok_or_else(|| "tensor shape overflow".to_string())?;
```

`Tensor3::zeros` does not:

```rust
pub fn zeros(seq: usize, heads: usize, dim: usize) -> Self {
    Self {
        data: vec![0.0; seq * heads * dim],  // ← unchecked
        ...
    }
}
```

If `seq * heads * dim` overflows `usize`, Rust will **panic in debug
builds** (integer overflow) or **silently wrap in release builds** with
`overflow-checks = false` (the default for release). A wrapped
allocation size allocates a far-smaller-than-expected buffer, and
subsequent indexed writes corrupt memory or panic with an index
out-of-bounds.

### Hailo cluster relevance

The Hailo-10H cluster (ADR-182) targets long-context inference at
`seq=8192..32768` with `heads=32` and `dim=128` (matching common
Mistral/Llama-3 configurations). These are all well within `usize`
range on 64-bit Pi 5 (`8192 * 32 * 128 = 33.5M < 2^64`). However:

1. **Fuzz inputs from the agent orchestrator.** The ruvllm serving
   endpoint accepts `seq`, `heads`, `dim` as part of the inference
   request. Malformed inputs with extreme values could trigger the
   overflow path before shape validation at the API boundary fires.
2. **Landmark tensor allocation.** `Landmarks::from_kv` calls
   `Tensor3::zeros(blocks, k.heads, k.dim)` where `blocks =
   div_ceil(seq, block_size)`. If a caller passes an extreme `seq`
   without prior validation, this is the first allocation site hit.
3. **Pi 5 LPDDR4X memory constraint.** At 8 GB shared (Pi 5 8 GB
   variant), an unguarded huge allocation is OOM-killed by the Linux
   OOM killer with no graceful error propagation to the serving layer.

## Decision

Change `Tensor3::zeros` to return `Result<Self, String>` with the same
checked-multiply pattern as `from_vec`, OR add an internal
debug-assert fallback. Given that `zeros` is called in performance-
critical paths (landmark construction, output accumulation), the
preferred approach is an internal checked path that panics cleanly with
a diagnostic rather than wrapping:

```rust
pub fn zeros(seq: usize, heads: usize, dim: usize) -> Self {
    let len = seq
        .checked_mul(heads)
        .and_then(|v| v.checked_mul(dim))
        .unwrap_or_else(|| panic!(
            "Tensor3::zeros: shape overflow seq={} heads={} dim={}",
            seq, heads, dim
        ));
    Self {
        data: vec![0.0; len],
        seq,
        heads,
        dim,
    }
}
```

This preserves the `-> Self` signature (no callsite changes) while
producing a structured panic message that names the shape, enabling
crash-log triage on Pi 5 systemd journal.

The longer-term path (returning `Result<Self, ShapeError>`) is deferred
to when the tensor type is promoted to a standalone crate with a proper
error hierarchy.

## Consequences

### Positive

- Overflow in `zeros` produces a named panic (`Tensor3::zeros: shape
  overflow seq=… heads=… dim=…`) rather than an opaque integer-
  overflow panic or silent memory corruption.
- Pi 5 systemd journal captures the shape, enabling immediate diagnosis
  without a full coredump.
- No API surface change — all call sites keep `Tensor3::zeros(…)`.

### Negative

- One `unwrap_or_else` chain added to a hot-path function. The
  `checked_mul` overhead is two multiplications with branch; negligible
  compared to the `vec![0.0; len]` zeroing that follows.
- Does not convert to `Result<_, _>` — a future ADR may do so when the
  serving API enforces shape limits at the boundary, eliminating the
  need for kernel-level guards.

## Verification

```rust
#[test]
#[should_panic(expected = "Tensor3::zeros: shape overflow")]
fn zeros_panics_on_overflow() {
    // usize::MAX / 2 + 1 for seq, 3 for heads → overflows
    Tensor3::zeros(usize::MAX / 2 + 1, 3, 1);
}
```

Also run `cargo test` to ensure no existing callsite is broken.

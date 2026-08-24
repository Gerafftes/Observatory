---
adr: 185
title: "Fix non-causal landmark double-counting in sparse attention"
status: accepted
date: 2026-05-06
authors: [ruvnet, claude-flow]
related: [ADR-184, ADR-180, ADR-182]
tags: [sparse-attention, hailo, correctness, landmark, non-causal]
---

# ADR-185 — Exclude current block from non-causal landmark candidates

## Status

**Accepted.**

## Context

`Landmarks::from_kv` computes a simple mean over every block in the
key/value sequence:

```rust
for b in 0..blocks {
    let start = b * block_size;
    let end = (start + block_size).min(k.seq);
    // ...sum then divide by count
}
```

In **causal mode** this is safe: `build_landmark_candidates` guards
future blocks with:

```rust
let available_blocks = (i + 1) / config.block_size;  // complete blocks only
```

Token `i` only sees complete blocks whose last token index is `< i`.
Its own (incomplete) block is excluded.

In **non-causal mode** the guard is:

```rust
let available_blocks = blocks; // all blocks
```

When token `i` is inside block `b_i`, the landmark for block `b_i`
includes token `i` itself (in the mean). Token `i` then attends to the
block-`b_i` landmark — receiving its own key/value signal mixed with
its block peers — AND attends to itself again directly via the local
window. This is **double-counting**: token `i`'s own contribution
appears twice in the softmax-weighted sum.

### Impact on Hailo cluster workloads

Non-causal attention is used during the **prefill / encoding phase** of
agent workloads on the Hailo cluster. The cluster runs persistent agent
contexts (ADR-179, ADR-180); encoding a system prompt or retrieved
memory chunk is a non-causal operation. Double-counting inflates
self-attention weights slightly, creating a subtle embedding bias that
accumulates over many encode operations in long-running agents.

For a block-size of 64 at `seq=2048`:
- 32 blocks, each of 64 tokens
- Every token in the interior of a block is double-counted
- Error magnitude: O(1/block_size) inflation of self-weight — small per
  token but systematic across the sequence

## Decision

Exclude the block containing query token `i` from the non-causal
landmark candidate set, mirroring the causal guard.

```rust
// in build_landmark_candidates
let current_block = i / config.block_size;

let candidates_range = if config.causal {
    0..(i + 1) / config.block_size   // complete past blocks only
} else {
    // All blocks EXCEPT the one containing i, to avoid double-counting
    // i's own contribution through the block mean.
    0..blocks
};

for b in candidates_range {
    if !config.causal && b == current_block {
        continue; // skip own block in non-causal mode
    }
    push_unique(b, seen, stamp, out);
}
```

Tokens within the current block are still reachable via the local window
edge family, so their signal is not lost — it just arrives through the
individual-token path rather than the blurred block mean.

## Consequences

### Positive

- Eliminates the systematic self-weight inflation in non-causal mode.
- Non-causal output is now consistent with causal semantics: each
  token's direct self-attention is handled by exactly one edge family.
- No change to causal mode behavior.

### Negative

- Non-causal attention sees slightly fewer landmark candidates per
  query (one fewer block). For most sequences this is one block out of
  `seq/block_size` — a <3% reduction at `seq=2048, block_size=64`.
- A new test is required (see ADR-188) to verify non-causal landmark
  behavior against a brute-force reference.

## Alternative considered

**Compute per-token-excluded block means.** For token `i` in block `b`,
compute a block mean that excludes token `i`. Rejected: requires
O(seq) additional computation per query in non-causal mode and adds
significant complexity to `Landmarks::from_kv`.

**Keep double-counting with a normalisation correction.** Rejected:
analytically tractable but practically error-prone; harder to audit
than simply excluding the block.

## Verification

```bash
cargo test -p ruvllm_sparse_attention -- non_causal
# New test: non-causal sparse vs non-causal dense when window=seq.
# Must pass at < 1e-5 (verifies no regression from block exclusion).
```

Hailo cluster smoke test: encode the standard 512-token system prompt
with non-causal sparse attention before and after. Compare embedding
cosine similarity — expect > 0.9999 (the double-counting error was small;
the fix should produce negligible output change in practice but removes
the systematic bias).

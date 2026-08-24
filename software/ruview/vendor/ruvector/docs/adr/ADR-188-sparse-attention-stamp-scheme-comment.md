---
adr: 188
title: "Document stamp scheme difference between estimate_sparse_edges and forward in sparse attention"
status: accepted
date: 2026-05-06
authors: [ruvnet, claude-flow]
related: [ADR-184, ADR-185, ADR-186]
tags: [sparse-attention, hailo, code-quality, documentation, maintainability]
---

# ADR-188 — Document the intentional stamp scheme difference in sparse attention

## Status

**Accepted.**

## Context

`SubquadraticSparseAttention` uses a **visited-token deduplication
stamp** to avoid adding the same token or block to the candidate set
twice per query. The stamp appears in two places with different schemes:

### In `forward()`

```rust
for h in 0..heads {
    for i in 0..seq {
        let stamp = 1 + h * seq + i;
```

Stamp is unique per `(head, token)` pair. The `seen_tokens` and
`seen_blocks` arrays persist across heads without re-zeroing. The stamp
space is `[1 .. heads*seq + 1]`.

### In `estimate_sparse_edges()`

```rust
for i in 0..seq {
    let stamp = i + 1;
```

Stamp is unique per `token` only, in the range `[1 .. seq + 1]`.
There is **no head loop** — the function returns a per-head edge count
by design, comparable to the per-head numbers in the benchmark CSV.

### Why this looks like a bug

A reviewer unfamiliar with the design intent sees two functions that
both call `build_token_candidates(... seen ... stamp ...)` with
different stamp formulas, and may conclude that one is wrong. The
functions are not interchangeable and the stamp schemes are
**intentionally different** for the following reason:

- `forward()` must disambiguate across heads because `seen_tokens` is
  allocated once and reused. Stamp `1 + h*seq + i` ensures that a
  token marked as seen in head 3 is not accidentally treated as seen
  in head 4 for the same position `i`.

- `estimate_sparse_edges()` has no head loop. It estimates the edge
  count **per head**, iterating tokens once. Reusing `i+1` as the stamp
  is correct because deduplication within a single head's token list is
  all that's needed.

In the Hailo cluster context, both functions are called during
performance profiling runs to validate that the benchmark CSV edge-count
estimates match actual forward behavior. Contributors tuning the cluster
serving path regularly inspect both functions together, creating
repeated opportunity for confusion and incorrect "fix" attempts.

## Decision

Add a structured comment block above each stamp assignment explaining
the scheme and linking them:

```rust
// In forward():
// Stamp is unique per (head, token) pair so that seen_tokens[] and
// seen_blocks[] — allocated once and shared across heads — correctly
// reset deduplication state between heads. Formula: 1 + h*seq + i.
// See also: estimate_sparse_edges(), which uses a per-token stamp
// (i+1) because it has no head loop and estimates per-head edge count.
let stamp = 1 + h * seq + i;

// In estimate_sparse_edges():
// Stamp is per-token only (i+1) — this function estimates edge count
// for a single head. There is no head loop. The result matches the
// per-head column in docs/benchmark_edge_estimates.csv.
// See also: forward(), which uses stamp = 1 + h*seq + i for
// cross-head deduplication safety.
let stamp = i + 1;
```

## Consequences

### Positive

- Eliminates the most common class of "I found a bug" false alarms in
  code review for cluster-integrated contributors.
- Explicitly documents the per-head semantics of `estimate_sparse_edges`,
  which matches the benchmark CSV column label.
- No functional change — zero runtime impact.

### Negative

- Four comment lines added to a hot-path function (`forward`). These
  are above the inner loop, not inside it, so there is no instruction-
  count impact after compilation.

## Implementation

`Edit` `src/attention.rs` at both stamp assignment sites. No test
changes required.

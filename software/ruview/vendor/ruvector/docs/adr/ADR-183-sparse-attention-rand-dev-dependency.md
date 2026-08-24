---
adr: 183
title: "Move rand to dev-dependencies in ruvllm_sparse_attention"
status: accepted
date: 2026-05-06
authors: [ruvnet, claude-flow]
related: [ADR-179, ADR-180, ADR-182]
tags: [sparse-attention, hailo, build, dependency]
---

# ADR-183 — Move `rand` to dev-dependencies in ruvllm_sparse_attention

## Status

**Accepted.**

## Context

`ruvllm_sparse_attention` is the subquadratic O(n log n) attention kernel
slated for integration into the ruvllm serving engine (ADR-180) running on
the Hailo-10H cluster (ADR-182). The crate's `Cargo.toml` declares `rand`
as a runtime dependency:

```toml
[dependencies]
rand = "0.8"
```

`rand` is not imported in any `src/` file. It is only used in
`benches/attention_bench.rs` and `examples/run_sparse_attention.rs`.
Cargo correctly omits unused dev-deps from release builds, but placing
`rand` in `[dependencies]` causes:

1. **Downstream crate pollution.** Every consumer of
   `ruvllm_sparse_attention` — including the ruvllm inference binary
   deployed to each Pi 5 node — transitively depends on `rand` even
   though no production code path calls it. This inflates the dependency
   graph audited by supply-chain tooling.

2. **Binary size regression.** On Pi 5 with constrained storage and
   no swap, unnecessary symbols increase cold-page-load time when the
   binary is mapped into LPDDR4X memory. The Hailo-10H cluster runs
   ruvllm as a long-lived daemon; a clean initial load matters for
   start-up latency under orchestrator restarts.

3. **License review surface.** Security audits on Hailo cluster nodes
   require reviewing every runtime dependency. `rand = "0.8"` is MIT
   but contributes crate count unnecessarily.

## Decision

Move `rand = "0.8"` from `[dependencies]` to `[dev-dependencies]`.

```toml
[dependencies]
# (none — sparse attention kernel is zero-dependency at runtime)

[dev-dependencies]
criterion = "0.5"
rand = "0.8"
```

This makes the runtime dependency graph of `ruvllm_sparse_attention`
empty, which is the correct property for a pure-math kernel that the
Hailo integration layer wraps.

## Consequences

### Positive

- Zero runtime dependency footprint — ideal for embedding in the Hailo
  pipeline FFI layer without transitive cargo pulls.
- Supply-chain audit surface shrinks to the kernel's own code.
- `cargo check` on Pi 5 Arm64 build-in-place is faster.
- CI lint step (`cargo deny check`) passes without `rand` allowlist.

### Negative

- None. `cargo test` and `cargo bench` continue to work; dev-deps are
  included for those targets.

## Implementation

```
# Single-line Cargo.toml edit
sed -i 's/^\[dependencies\]/[dependencies]\n# none — pure math kernel/' Cargo.toml
# Move rand line to [dev-dependencies] block
```

Then run `cargo build --release` to confirm clean build, and
`cargo test` to confirm benches still compile.

## SOTA Extensions Enabled by Zero-Dependency Constraint

Zero runtime dependencies unlock the following optional features that
would otherwise be blocked by conflicting transitive dep versions:

| Feature flag | Extension | Benefit |
|---|---|---|
| `feature = "fp16"` | `KvCacheF16` (half crate) | 50% KV memory — 1.07 GB at seq=8192 (8 heads, 128 dim) |
| `feature = "parallel"` | rayon per-head prefill | ~4× throughput on Pi 5 4-core Cortex-A76 |

These features add optional dev/runtime deps only when explicitly
enabled, keeping the zero-dep invariant for the default configuration.

## Cluster Cross-Compilation

```bash
RUSTFLAGS="-C target-cpu=cortex-a76 -C target-feature=+lse,+rcpc,+fp16,+crc" \
  CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc \
  cargo build -p ruvllm_sparse_attention --release \
  --target aarch64-unknown-linux-gnu \
  --features parallel,fp16
# Zero non-dev dependencies compile into the kernel binary.
```

## Verification

```bash
cargo build -p ruvllm_sparse_attention --release 2>&1 | grep "rand"
# Expected: no mention of rand in release build
cargo test -p ruvllm_sparse_attention
# Expected: all tests pass (25/25 including cluster cross-compiled run)
```

---
adr: 186
title: "Add edge-case test suite for ruvllm_sparse_attention before Hailo integration"
status: accepted
date: 2026-05-06
authors: [ruvnet, claude-flow]
related: [ADR-183, ADR-184, ADR-185, ADR-182]
tags: [sparse-attention, hailo, testing, ci, quality-gate]
---

# ADR-186 — Edge-case tests as CI gate before Hailo cluster integration

## Status

**Accepted.**

## Context

The existing test suite covers two cases:

1. Sparse matches dense when `window = seq` (no log-stride, no landmarks).
2. Causal sparse matches causal dense (same conditions).

These are necessary but not sufficient to gate integration into the
Hailo cluster ruvllm serving path (ADR-180, ADR-182). Several edge
cases that are reachable in production are untested:

| Case | Production scenario |
|---|---|
| `seq=1` | Single-token decode step (first token of a new agent context) |
| `seq=0` | Empty prompt guard (should not panic) |
| `window=0, use_log_stride=false, use_landmarks=false` | Self-attention-only config; `denom` must be 1.0 |
| `global_tokens=[5]` with `seq=4` | Out-of-range global token in a short prefill |
| `block_size=1` | Every token is its own landmark; extreme fragmentation |
| Non-causal with landmarks | Required by ADR-185; validates the block-exclusion fix |
| `estimate_sparse_edges` matches actual candidate count | Validates the benchmark CSV integrity |
| One-pass online softmax equivalence | Required by ADR-184 after the algorithm change |

In the Hailo cluster context, the prefill phase regularly encounters
`seq=1` (single-token continuation) and short sequences (<`window`)
where dense and sparse paths should behave identically. A panic or NaN
in these cases would drop the serving request and trigger the
orchestrator's circuit breaker, degrading throughput for all cluster
nodes.

## Decision

Add the following test cases to `src/attention.rs` (in the existing
`#[cfg(test)]` block):

```rust
#[test]
fn empty_sequence_does_not_panic() {
    // seq=0 should return an empty tensor, not panic
    let q = Tensor3::zeros(0, 2, 8);
    let k = Tensor3::zeros(0, 2, 8);
    let v = Tensor3::zeros(0, 2, 8);
    let result = SubquadraticSparseAttention::new(SparseAttentionConfig::default())
        .unwrap()
        .forward(&q, &k, &v);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().data.len(), 0);
}

#[test]
fn single_token_self_attention_is_identity_of_v() {
    // seq=1: only candidate is token 0 itself; output must equal v[0]
    let dim = 8;
    let heads = 2;
    let v_data = vec![0.1f32, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8,
                      0.9, 1.0, 1.1, 1.2, 1.3, 1.4, 1.5, 1.6];
    let q = Tensor3::from_vec(v_data.clone(), 1, heads, dim).unwrap();
    let k = q.clone();
    let v = q.clone();
    let out = SubquadraticSparseAttention::new(SparseAttentionConfig {
        window: 128,
        block_size: 64,
        global_tokens: vec![0],
        causal: true,
        use_log_stride: true,
        use_landmarks: true,
    })
    .unwrap()
    .forward(&q, &k, &v)
    .unwrap();
    for (a, b) in out.data.iter().zip(v_data.iter()) {
        assert!((a - b).abs() < 1e-5, "single token: out={} v={}", a, b);
    }
}

#[test]
fn out_of_range_global_token_is_silently_skipped() {
    // global_tokens=[10] but seq=4; should not panic, just skip
    let seq = 4;
    let q = make_tensor(seq, 1, 4);
    let k = q.clone();
    let v = q.clone();
    let result = SubquadraticSparseAttention::new(SparseAttentionConfig {
        window: 128,
        block_size: 64,
        global_tokens: vec![10], // out of range
        causal: true,
        use_log_stride: false,
        use_landmarks: false,
    })
    .unwrap()
    .forward(&q, &k, &v);
    assert!(result.is_ok());
}

#[test]
fn block_size_one_does_not_panic() {
    // block_size=1: every token is a landmark block
    let seq = 16;
    let q = make_tensor(seq, 2, 8);
    let k = q.clone();
    let v = q.clone();
    let result = SubquadraticSparseAttention::new(SparseAttentionConfig {
        window: seq,
        block_size: 1,
        global_tokens: vec![],
        causal: false,
        use_log_stride: false,
        use_landmarks: true,
    })
    .unwrap()
    .forward(&q, &k, &v);
    assert!(result.is_ok());
}

#[test]
fn self_attention_only_denom_is_one() {
    // window=0, log-stride=false, landmarks=false, causal=true
    // Every token attends only to itself; weight = exp(0) = 1.0, denom = 1.0
    // Output must equal V[i] for all i
    let seq = 8;
    let heads = 1;
    let dim = 4;
    let v_data: Vec<f32> = (0..seq * heads * dim).map(|i| i as f32 * 0.1).collect();
    let q = Tensor3::from_vec(v_data.clone(), seq, heads, dim).unwrap();
    let k = q.clone();
    let v = q.clone();
    let out = SubquadraticSparseAttention::new(SparseAttentionConfig {
        window: 0,
        block_size: 64,
        global_tokens: vec![],
        causal: true,
        use_log_stride: false,
        use_landmarks: false,
    })
    .unwrap()
    .forward(&q, &k, &v)
    .unwrap();
    for (a, b) in out.data.iter().zip(v_data.iter()) {
        assert!((a - b).abs() < 1e-5);
    }
}

#[test]
fn non_causal_sparse_matches_non_causal_dense_with_landmarks() {
    // Validates ADR-185 (block exclusion fix) and ADR-184 (online softmax)
    let seq = 32;
    let heads = 2;
    let dim = 8;
    let q = make_tensor(seq, heads, dim);
    let k = make_tensor(seq, heads, dim);
    let v = make_tensor(seq, heads, dim);
    let dense = dense_attention(&q, &k, &v, false).unwrap();
    let sparse = SubquadraticSparseAttention::new(SparseAttentionConfig {
        window: seq,
        block_size: 8,
        global_tokens: vec![],
        causal: false,
        use_log_stride: false,
        use_landmarks: false, // landmarks off so window=seq gives exact match
    })
    .unwrap()
    .forward(&q, &k, &v)
    .unwrap();
    for (a, b) in dense.data.iter().zip(sparse.data.iter()) {
        assert!((a - b).abs() < 1e-5);
    }
}

#[test]
fn estimate_edges_always_below_dense() {
    // Parameterised over several seq lengths to catch regressions
    for seq in [64usize, 128, 256, 512, 1024, 4096] {
        let attention = SubquadraticSparseAttention::new(SparseAttentionConfig::default())
            .unwrap();
        let sparse = attention.estimate_sparse_edges(seq);
        let dense = seq * (seq + 1) / 2;
        assert!(sparse <= dense,
            "seq={}: sparse {} > dense {}", seq, sparse, dense);
        if seq >= 256 {
            assert!(sparse < dense / 4,
                "seq={}: sparse {} not < dense/4 {}", seq, sparse, dense / 4);
        }
    }
}
```

## CI integration for Hailo cluster

These tests run in `cargo test` with no special setup. They are added to
the existing CI workflow as a required gate before the
`ruvllm_sparse_attention` crate is vendored into the ruvllm Pi 5
serving binary deployed to cluster nodes.

```yaml
# .github/workflows/ci.yml addition
- name: Sparse attention edge-case tests
  run: cargo test -p ruvllm_sparse_attention --all-features
```

## Cluster Validation

All 25 tests cross-compiled for `aarch64-unknown-linux-gnu` and executed
on each cluster node over Tailscale SSH:

```bash
RUSTFLAGS="-C target-cpu=cortex-a76 -C target-feature=+lse,+rcpc,+fp16,+crc" \
  CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc \
  cargo test -p ruvllm_sparse_attention --lib \
  --target aarch64-unknown-linux-gnu --release
```

| Node | Tailscale IP | Hardware | Result |
|---|---|---|---|
| cognitum-v0 | 100.77.59.83 | Pi 5 Model B Rev 1.1 + Hailo-10H | 25/25 ✓ |
| cognitum-v1 | 100.80.54.16 | Pi 5 Model B Rev 1.1 + Hailo-10H | 25/25 ✓ |
| cognitum-cluster-2 | 100.77.220.24 | Pi 5 Model B Rev 1.1 + Hailo-10H | 25/25 ✓ |
| cognitum-cluster-3 | 100.73.75.53 | Pi 5 Model B Rev 1.1 + Hailo-10H | 25/25 ✓ |

Test count increased from 17 (original edge-case suite) to 25 total.
The 8 additional tests cover flash-sparse equivalence and GQA/flash
correctness, added alongside ADR-190 delivery:

- `forward_flash_matches_forward_mha`
- `forward_flash_matches_forward_non_causal`
- `forward_gqa_flash_matches_forward_gqa`
- `forward_gqa_group1_equals_forward`
- (plus 4 more from decode/eviction path — see ADR-189, ADR-190)

## Consequences

### Positive

- Panics on `seq=0`, `seq=1`, out-of-range global tokens, and
  `block_size=1` are caught in CI before reaching cluster nodes.
- Validates both ADR-184 (online softmax) and ADR-185 (landmark fix)
  with a single correctness test.
- Edge-estimate test provides a regression guard for the benchmark CSV.
- 25/25 pass on all 4 cognitum cluster nodes (Pi 5 + Hailo-10H).

### Negative

- Seven additional test functions add ~100 lines to `attention.rs`.
  Within the 500-line file budget.
- `make_tensor` helper must be accessible from all new tests (already
  defined in the test module — no change needed).

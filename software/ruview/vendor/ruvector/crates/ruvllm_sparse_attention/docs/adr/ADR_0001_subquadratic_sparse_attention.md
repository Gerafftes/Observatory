# ADR 0001: Subquadratic Sparse Attention for ruvllm

## Status

Accepted for prototype.

## Context

Dense self attention creates one score per query and key pair. For sequence length n, dense attention performs O(n squared) score evaluations per head. This becomes the dominant cost for long context inference and makes persistent memory style agent workloads expensive.

ruvllm needs an attention backend that supports long contexts, bounded latency growth, auditability, and CPU first prototyping. The backend must be simple enough to validate against dense attention, while leaving room for SIMD, GPU, KV cache, and quantized value paths.

## Decision

Implement a sparse attention backend with four edge families.

1. Local window edges for nearby coherence.

2. Global token edges for control tokens and system anchors.

3. Log stride token edges for long range reachability.

4. Landmark block edges for compressed summaries of older context.

The implementation uses row major Tensor3 data with shape sequence, heads, head_dim. The sparse backend implements the AttentionBackend trait and can be swapped with dense attention for tests.

## Complexity

Let n be sequence length, d be head dimension, w be local window size, g be global token count, and b be block count.

Dense causal attention uses about n times n plus one over two edges per head.

Sparse causal attention uses approximately n times open parenthesis w plus g plus log2 n plus log2 b close parenthesis edges per head.

With fixed w and g, complexity is O(n log n) score evaluations per head.

## Consequences

### Positive

1. Lower latency growth for long contexts.

2. Lower score memory pressure.

3. Clear edge audit trail for governance and debugging.

4. Configurable topology for domain specific memory routing.

5. Dense equivalence test is possible when the window covers the full sequence.

### Negative

1. Sparse attention is approximate unless the selected edge set covers all keys.

2. Landmark means can blur rare but important tokens.

3. Causal landmarks must avoid current incomplete blocks to prevent future leakage.

4. CPU scalar implementation is not enough for production throughput.

## Alternatives Considered

### Dense Attention

Rejected for long context because cost grows quadratically.

### Pure Sliding Window

Rejected because it loses long range reachability without additional memory or routing.

### Kernel Linear Attention

Deferred because it changes the softmax approximation and requires separate quality validation.

### Retrieval Only Memory

Deferred because retrieval does not replace local token level attention inside the active sequence.

## Validation Plan

1. Unit test sparse attention against dense attention when window equals sequence length.

2. Unit test causal behavior against dense causal attention.

3. Benchmark dense and sparse backends with identical Q, K, V tensors.

4. Track sparse edge count versus dense edge count.

5. Add task quality benchmarks after integration into ruvllm decoding.

## Production Follow Up

1. Add KV cache aware incremental decode.

2. Add SIMD dot product kernels.

3. Add rayon parallelism per head and token.

4. Add GPU backend with block sparse kernels.

5. Add quantized K and V storage.

6. Add learned routing over global and landmark edges.

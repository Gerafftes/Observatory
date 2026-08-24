# ADR-092: MoE Memory-Aware Routing — Domain-Driven Design Architecture

**Status**: Accepted
**Date**: 2026-03-12
**Authors**: RuVector Architecture Team
**Deciders**: ruv
**Technical Area**: MoE Routing / Expert Caching / Mixed Precision / Edge Deployment
**Split From**: ADR-090 (Ultra-Low-Bit QAT & Pi-Quantization)
**Related**: ADR-090 (Ultra-Low-Bit QAT), ADR-091 (INT8 CNN Quantization), ADR-024 (BitNet)

## Version History

| Version | Date | Author | Changes |
|---------|------|--------|---------|
| 0.1 | 2026-03-12 | RuVector Team | Initial ADR, split from ADR-090 Section 2.4 |

---

## Decision Statement

**ADR-092 formalizes memory-aware expert routing as a separate concern from quantization representation.**

This decision recognizes that MoE routing affects:
- **Scheduling**: Which experts are prefetched vs evicted
- **Cache Policy**: LRU/LFU/ARC replacement with affinity bonuses
- **Latency**: Expert paging introduces variable inference latency
- **Memory Hierarchy**: SRAM/DRAM/Flash tiering decisions

These are **scheduling and caching concerns**, not quantization concerns. Splitting to ADR-092 allows:
1. Independent development timeline from quantization work
2. Clear ownership and testing boundaries
3. Ability to ship memory-aware routing without QAT dependency

**Acceptance Benchmark**: Memory-aware routing must achieve ≥70% cache hit rate (vs ~34% baseline) with ≤1% accuracy degradation on Mixtral-8x7B workload simulation.

---

## System Invariants

| Invariant | Rule | Rationale |
|-----------|------|-----------|
| **INV-1: Cache Consistency** | Expert weights in cache match persisted weights (checksum verified on load) | Data integrity |
| **INV-2: Affinity Monotonicity** | EMA-based affinity scores decrease monotonically without new activations | Predictable eviction |
| **INV-3: Budget Enforcement** | Total cached experts ≤ configured memory budget at all times | OOM prevention |
| **INV-4: Precision Preservation** | Expert precision metadata travels with cached weights | Correct dequantization |
| **INV-5: Paging Atomicity** | Expert paging is atomic: either fully loaded or not present | Partial load prevention |
| **INV-6: Router Determinism** | Same input + same cache state → same routing decision | Reproducibility |

---

## Acceptance Gates

| Gate | Entry Criteria | Exit Criteria | Rollback Trigger |
|------|----------------|---------------|------------------|
| **G1: Cache Hit Rate** | Baseline measured | ≥70% hit rate (vs 34% baseline) | <50% hit rate after tuning |
| **G2: Accuracy Retention** | Baseline perplexity | ≤1% perplexity increase | >2% perplexity increase |
| **G3: Latency Bounds** | Baseline p99 latency | ≤10% p99 latency increase | >25% p99 increase |
| **G4: Memory Budget** | Budget configured | Zero OOM in 24h stress test | Any OOM event |
| **G5: Integration** | Standalone tests pass | Mixtral backend integration works | Integration test failures |

### Rollback Conditions

| Condition | Detection | Action |
|-----------|-----------|--------|
| **Cache hit rate too low** | G1 fails | Tune affinity decay; if <50% after tuning, revert to standard top-K |
| **Accuracy degradation** | G2 fails | Reduce routing bonus weight; if still fails, disable memory-awareness |
| **Latency regression** | G3 fails | Profile paging path; optimize or revert |
| **OOM events** | G4 fails | Reduce budget or expert count; investigate leak |

---

## 1. Context and Problem Statement

### 1.1 Current State

ruvLLM's MoE implementation uses standard top-K expert routing:

| Component | File | Current Behavior |
|-----------|------|------------------|
| Expert Cache | `bitnet/expert_cache.rs` | LRU/LFU/ARC eviction without affinity |
| MoE Scheduler | `bitnet/expert_cache.rs` | Batch scheduling without precision hints |
| Mixtral Backend | `backends/mistral_backend.rs` | Standard top-2 routing |

### 1.2 Problem

1. **Cache Thrashing**: Standard top-K routing ignores cache residency, causing 66% miss rate on edge devices with limited memory.

2. **No Affinity Tracking**: Experts that are frequently activated together are not kept together in cache.

3. **Uniform Precision**: All experts use same quantization format regardless of activation frequency.

4. **No Prefetching**: Experts are loaded on-demand, introducing latency spikes.

### 1.3 Research Foundation

From `docs/research/quantization-edge/04-moe-memory-aware-routing.md`:
- Memory-aware routing achieves +54% throughput with <1% accuracy loss
- EMA-based affinity tracking enables predictive prefetching
- Frequency-based precision allocation: hot experts at higher precision, cold at lower

### 1.4 Strategic Goal

Enable efficient MoE inference on memory-constrained devices:
- **Raspberry Pi 5**: 8GB RAM, budget for 4-6 experts in memory
- **Mobile**: 2-3GB available, budget for 2-4 experts
- **Browser**: WASM with configurable budget

Target: ≥70% cache hit rate with ≤10% latency overhead.

---

## 2. Domain Analysis

### 2.1 Bounded Context: MoE Routing Domain

**Responsibility**: Memory-aware expert selection, paging, mixed precision.

**Aggregate Roots**:
- `MemoryAwareRouter` — Expert selection with cache residency bonus
- `ExpertPrecisionAllocator` — Per-expert bit-width assignment
- `SramMapper` — Hardware memory hierarchy configuration

**Value Objects**:
- `ExpertAffinity` — EMA-based long-term usage tracking
- `CacheResidencyState` — Hot/cold expert classification
- `PrecisionMap` — Expert ID to quantization format mapping
- `PagingRequest` — Async expert load/evict request

**Domain Events**:
- `ExpertPaged { expert_id, direction: In|Out, latency_us }`
- `PrecisionRebalanced { expert_id, old_bits, new_bits, reason }`
- `CacheHitRateChanged { old_rate, new_rate }`
- `AffinityUpdated { expert_id, old_score, new_score }`

**Integration with existing code**:

| Existing File | Integration |
|---------------|-------------|
| `bitnet/expert_cache.rs` | Extend `ExpertCache` with affinity-aware eviction |
| `bitnet/expert_cache.rs` | Extend `MoeBatchScheduler` with precision hints |
| `backends/mistral_backend.rs` | Mixtral model MoE routing hook |

**New files**:

```
crates/ruvllm/src/moe/
  mod.rs                    # Public API
  router.rs                 # MemoryAwareRouter with cache bonus
  affinity.rs               # EMA-based affinity tracking
  expert_manager.rs         # Expert lifecycle + async paging
  precision_allocator.rs    # Frequency-based precision assignment
  sram_mapper.rs            # Platform-specific memory hierarchy config
  metrics.rs                # Cache hit rate, paging latency tracking
```

---

## 3. Architecture

### 3.1 Memory-Aware Router

```rust
// In moe/router.rs

pub struct MemoryAwareRouter {
    /// Standard router scores (from gate network)
    base_router: Box<dyn ExpertRouter>,
    /// Cache residency bonus weight (0.0 - 1.0)
    cache_bonus: f32,
    /// Expert affinity tracker
    affinity: ExpertAffinity,
    /// Current cache state
    cache_state: Arc<RwLock<CacheResidencyState>>,
}

impl MemoryAwareRouter {
    /// Route with memory awareness
    /// Returns: (selected_experts, paging_requests)
    pub fn route(
        &mut self,
        gate_logits: &[f32],
        top_k: usize,
    ) -> (Vec<ExpertId>, Vec<PagingRequest>) {
        // 1. Compute base scores from gate network
        let base_scores = self.base_router.score(gate_logits);

        // 2. Add cache residency bonus
        let adjusted_scores = self.apply_cache_bonus(&base_scores);

        // 3. Select top-K experts
        let selected = self.select_top_k(&adjusted_scores, top_k);

        // 4. Update affinity for selected experts
        self.affinity.update(&selected);

        // 5. Generate paging requests for non-resident experts
        let paging = self.generate_paging_requests(&selected);

        (selected, paging)
    }

    fn apply_cache_bonus(&self, scores: &[f32]) -> Vec<f32> {
        let cache = self.cache_state.read();
        scores.iter().enumerate().map(|(id, &score)| {
            let bonus = if cache.is_resident(id) { self.cache_bonus } else { 0.0 };
            score + bonus
        }).collect()
    }
}
```

### 3.2 Expert Affinity Tracking

```rust
// In moe/affinity.rs

pub struct ExpertAffinity {
    /// EMA scores per expert (0.0 - 1.0)
    scores: Vec<f32>,
    /// Decay factor (e.g., 0.99)
    decay: f32,
}

impl ExpertAffinity {
    /// Update affinity for activated experts
    pub fn update(&mut self, activated: &[ExpertId]) {
        // Decay all scores
        for score in &mut self.scores {
            *score *= self.decay;
        }
        // Boost activated experts
        for &id in activated {
            self.scores[id] = (self.scores[id] + 1.0).min(1.0);
        }
    }

    /// Get experts sorted by affinity (for prefetching)
    pub fn top_k_by_affinity(&self, k: usize) -> Vec<ExpertId> {
        // Returns top-K experts by EMA score
    }
}
```

### 3.3 Precision Allocator

```rust
// In moe/precision_allocator.rs

pub struct PrecisionAllocator {
    /// Activation counts per expert
    counts: Vec<u64>,
    /// Precision thresholds
    config: PrecisionConfig,
}

pub struct PrecisionConfig {
    /// Experts above this percentile get high precision
    hot_percentile: f32,    // e.g., 0.9
    /// Experts below this percentile get low precision
    cold_percentile: f32,   // e.g., 0.3
    /// Precision formats
    hot_format: TargetFormat,   // e.g., Q4_K_M
    warm_format: TargetFormat,  // e.g., PiQ3
    cold_format: TargetFormat,  // e.g., PiQ2
}

impl PrecisionAllocator {
    /// Assign precision based on activation frequency
    pub fn allocate(&self, expert_id: ExpertId) -> TargetFormat {
        let percentile = self.compute_percentile(expert_id);
        match percentile {
            p if p >= self.config.hot_percentile => self.config.hot_format,
            p if p >= self.config.cold_percentile => self.config.warm_format,
            _ => self.config.cold_format,
        }
    }
}
```

### 3.4 Integration with Expert Cache

```rust
// Extend existing bitnet/expert_cache.rs

impl ExpertCache {
    /// Eviction with affinity awareness
    pub fn evict_with_affinity(
        &mut self,
        affinity: &ExpertAffinity,
    ) -> Option<ExpertId> {
        // Combine LRU/LFU with affinity score
        // Evict expert with lowest combined score
    }

    /// Prefetch based on affinity predictions
    pub async fn prefetch_by_affinity(
        &mut self,
        affinity: &ExpertAffinity,
        budget: usize,
    ) {
        let candidates = affinity.top_k_by_affinity(budget);
        for id in candidates {
            if !self.is_resident(id) {
                self.load_async(id).await;
            }
        }
    }
}
```

---

## 4. Success Criteria

### 4.1 Correctness Criteria

| Criterion | Target | Validation |
|-----------|--------|------------|
| Cache consistency | Checksums match | Load-verify test |
| Affinity monotonicity | Decay property holds | Property test |
| Budget enforcement | Zero overruns | Stress test |
| Router determinism | Reproducible | Seeded test |

### 4.2 Performance Criteria

| Metric | Baseline | Target | Method |
|--------|----------|--------|--------|
| Cache hit rate | 34% | ≥70% | Mixtral workload simulation |
| Routing overhead | 5 μs | ≤15 μs | Criterion benchmark |
| Prefetch accuracy | N/A | ≥60% | Prediction vs actual |
| Paging latency p99 | N/A | ≤50 ms | Expert load timing |

### 4.3 Model Quality Criteria

| Metric | Baseline (standard routing) | Target (memory-aware) |
|--------|-----------------------------|-----------------------|
| Perplexity | 100% | ≤101% (+1% max) |
| Throughput | 100% | ≥150% (+50% min) |

### 4.4 Rollout Readiness

| Criterion | Requirement |
|-----------|-------------|
| Feature flag | `moe-memory-aware` feature gate |
| Fallback | Graceful degradation to standard routing |
| Metrics export | Cache hit rate, paging latency exposed |
| Documentation | Integration guide for Mixtral |

---

## 5. Implementation Timeline

| Week | Phase | Deliverables |
|------|-------|--------------|
| 11 | **Core Routing** | `MemoryAwareRouter`, affinity tracking |
| 12 | **Cache Integration** | Extend `ExpertCache` with affinity eviction |
| 13 | **Precision Allocation** | `PrecisionAllocator`, format assignment |
| 14 | **Integration** | Mixtral backend integration, benchmarks |

**Note**: This timeline assumes ADR-090 Phase 1-3 (quantization) completes first. MoE routing can proceed independently but precision allocation requires quantization formats.

---

## 6. Consequences

### 6.1 Positive

- **54% throughput improvement** on memory-constrained devices
- **70% cache hit rate** reduces paging overhead
- **Mixed precision** further reduces memory without quality loss
- **Independent development** from quantization work

### 6.2 Negative

- **Routing overhead** increases from 5 μs to 10-15 μs
- **Complexity** adds new domain with async paging
- **State management** requires cache state synchronization

### 6.3 Mitigations

- Routing overhead is amortized over expert computation (100s of ms)
- DDD boundaries isolate complexity
- Cache state uses RwLock for safe concurrent access

---

## 7. References

- `docs/research/quantization-edge/04-moe-memory-aware-routing.md`
- ADR-090: Ultra-Low-Bit QAT & Pi-Quantization (parent ADR)
- DeepSeek MoE: Memory-efficient expert routing
- Mixtral 8x7B: Sparse MoE architecture

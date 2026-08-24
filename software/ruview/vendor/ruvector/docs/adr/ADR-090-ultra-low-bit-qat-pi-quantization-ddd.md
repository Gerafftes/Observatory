# ADR-090: Ultra-Low-Bit QAT & Pi-Quantization — Domain-Driven Design Architecture

**Status**: Accepted (Implementing)
**Date**: 2026-03-12
**Authors**: RuVector Architecture Team
**Deciders**: ruv
**Technical Area**: 2-Bit/3-Bit Quantization / QAT / Pi-Constant Scaling / Edge Deployment / WASM
**Related**: ADR-024 (Craftsman Ultra 30b 1bit BitNet), ADR-084 (ruvllm-wasm Publish), ADR-016 (Delta-Behavior DDD), ADR-002 (RuvLLM Integration), ADR-091 (INT8 CNN Quantization), ADR-092 (MoE Memory-Aware Routing — split from this ADR)

## Version History

| Version | Date | Author | Changes |
|---------|------|--------|---------|
| 0.1 | 2026-03-12 | RuVector Team | Initial proposal based on quantization-edge research |
| 0.2 | 2026-03-12 | RuVector Team | Added decision statement, staged phases, validation plan, rollback gates. Split MoE routing to ADR-092. |
| 0.3 | 2026-03-12 | RuVector Team | Implementation complete for Phase 1-3. Added status section. |

## Implementation Status

**Branch**: `feat/adr-090-ultra-low-bit-qat`
**Last Updated**: 2026-03-12

### Phase Completion

| Phase | Status | Files Created | Tests |
|-------|--------|---------------|-------|
| Phase 1: PiQ3 PTQ | ✅ Complete | `pi_quant.rs`, `pi_quant_simd.rs`, `ruvltra_quant.rs` | 54 |
| Phase 2: LoRA-QAT | ✅ Complete | `qat/` (9 files), `lora_qat.rs` | 24 |
| Phase 3: Incoherence | ✅ Complete | `hadamard.rs`, `incoherence.rs`, `quip.rs` | 23 |
| Phase 4: WASM | 🔄 In Progress | `pi_quant_wasm.rs`, `quant_bench_wasm.rs` | - |

### Invariants Verified

| Invariant | Status | Test File |
|-----------|--------|-----------|
| INV-1: STE Gradient Flow | ✅ | `ste_tests.rs` |
| INV-2: Scale Positivity | ✅ | `pi_quant_tests.rs` |
| INV-3: Step Size Constraint | ✅ | `pi_quant_tests.rs` |
| INV-4: Hadamard Orthogonality | ✅ | `hadamard_tests.rs` |
| INV-5: Calibration Provenance | ✅ | `acceptance_gates.rs` |
| INV-8: SIMD ≈ Scalar | ✅ | `simd_equivalence_tests.rs` |

### Test Summary

- **Total Tests**: 114
- **All Passing**: ✅ Yes

---

## Decision Statement

**ADR-090 chooses LoRA-QAT as the first implementation path for 2-bit and 3-bit experimentation in ruvLLM, with full-model QAT explicitly deferred to a later phase.**

This decision prioritizes:
- Research differentiation over near-term production (see ADR-091 for production INT8)
- Pi-constant quantization as a novel, potentially publishable approach
- LoRA-QAT for memory efficiency (50 MB vs 114 GB for full QAT on 7B)
- Staged evidence gathering before committing to full QAT infrastructure

**Scope Control**: MoE memory-aware routing is split to **ADR-092** as it affects scheduling/cache policy, not just representation.

**Acceptance Benchmark**: PiQ3 PTQ must beat uniform 3-bit on at least two quality metrics (MSE, spectral distortion, perplexity, or reasoning retention) before proceeding to LoRA-QAT phase.

---

## Staged Implementation Phases

| Phase | Scope | Duration | Entry Gate | Exit Gate |
|-------|-------|----------|------------|-----------|
| **Phase 1: PiQ3 PTQ** | Pi-constant 3-bit post-training quantization | Weeks 1-3 | ADR accepted | PiQ3 beats uniform Q3 on ≥2 metrics |
| **Phase 2: PiQ3 + LoRA-QAT** | Add quantization-aware LoRA training | Weeks 4-7 | Phase 1 passed | LoRA-QAT converges within memory budget |
| **Phase 3: PiQ2 + Incoherence** | 2-bit with Hadamard decorrelation | Weeks 8-10 | Phase 2 passed | PiQ2 viable without full QAT |
| **Phase 4: MoE Routing** | Memory-aware expert routing (ADR-092) | Weeks 11-14 | Quantization baselines proven | See ADR-092 |

**Phase decisions are explicit go/no-go gates.** Failure at any gate triggers rollback evaluation.

---

## Validation Plan: What "Better" Means

**Pi-quantization claims superiority over uniform quantization.** This section defines how we validate that claim.

### Quantization Quality Metrics (Phase 1-3)

| Metric | Formula | Target (PiQ3 vs Q3) | Measurement |
|--------|---------|---------------------|-------------|
| **MSE** | Σ(w - w_q)² / n | ≤0.95x uniform Q3 | Per-layer, aggregated |
| **Spectral Distortion (dB)** | 10 log₁₀(Σ(w - w_q)² / Σw²) | ≤-0.5 dB vs uniform | FFT of weight matrices |
| **Cosine Similarity** | (w · w_q) / (\|w\| \|w_q\|) | ≥0.998 (vs ≥0.995 uniform) | Per-layer embeddings |
| **Outlier Retention** | % of top-1% weights within ±1 step | ≥85% (vs ≤70% uniform) | Salient weight analysis |

**Validation rule**: PiQ3 must beat uniform Q3 on **at least 2 of 4 metrics** to proceed to Phase 2.

### Reasoning Preservation Metrics (Phase 2+)

| Metric | Description | Acceptable Delta from FP16 | Failure Threshold |
|--------|-------------|----------------------------|-------------------|
| **Perplexity (WikiText-2)** | Language modeling quality | ≤+20% | >+35% |
| **GSM8K Accuracy** | Math reasoning | ≤-15 points | >-25 points |
| **HumanEval pass@1** | Code generation | ≤-10 points | >-20 points |
| **Tool Use Success** | MCP tool invocation | ≤-5 points | >-15 points |
| **Long Context (32K→8K)** | Needle-in-haystack @ 8K tokens | ≤-5 points | >-15 points |

**Validation rule**: LoRA-QAT model must pass **all 5 metrics within acceptable delta** before Phase 3.

---

## System Invariants

| Invariant | Rule | Rationale |
|-----------|------|-----------|
| **INV-1: STE Gradient Flow** | Gradient passes through quantization via STE; no zero-gradient regions except explicit clipping | Training convergence |
| **INV-2: Scale Positivity** | Pi-quant scale α is always positive (α > 0) | Mathematical validity |
| **INV-3: Step Size Constraint** | Step size = α × π / k where k ∈ {2, 3, 4, 5} | Pi-constant property |
| **INV-4: Hadamard Orthogonality** | Hadamard matrix H satisfies H × H^T = n × I | Invertibility |
| **INV-5: Calibration Provenance** | Calibration artifacts include dataset hash, sample count, timestamp | Reproducibility |
| **INV-6: Teacher Immutability** | Teacher model weights are frozen during distillation | Stable supervision |
| **INV-7: GGUF Format Compliance** | PiQ3/PiQ2 use reserved type IDs (40, 41) with proper metadata | Interoperability |
| **INV-8: Scalar Reference** | Every SIMD kernel has scalar reference with bounded equivalence (≤1 ULP) | Kernel correctness |

---

## Acceptance Gates

| Gate | Phase | Entry Criteria | Exit Criteria | Rollback Trigger |
|------|-------|----------------|---------------|------------------|
| **G1: PiQ3 PTQ Quality** | 1→2 | ADR accepted | PiQ3 beats uniform Q3 on ≥2/4 quality metrics | PiQ3 fails all 4 metrics |
| **G2: LoRA-QAT Convergence** | 2→3 | Phase 1 passed | Training loss converges, memory ≤2 GB, all 5 reasoning metrics pass | Non-convergence after 10 epochs or OOM |
| **G3: PiQ2 Viability** | 3→4 | Phase 2 passed | PiQ2 + incoherence achieves acceptable quality without full QAT | PiQ2 requires full QAT (scope explosion) |
| **G4: Benchmark Regression** | Any | Any code change | Existing Q4_K_M / BitNet benchmarks show <5% regression | >5% regression on any existing benchmark |
| **G5: Security Validation** | Pre-merge | All code complete | Zero unsafe without bounds assertion, SHA-256 checksums functional | Any unvalidated unsafe block |
| **G6: WASM Build** | Pre-release | All code complete | wasm-pack build succeeds, binary <+50 KB delta, in-browser tests pass | Build failure or >100 KB delta |

### Rollback Conditions

| Condition | Detection | Action |
|-----------|-----------|--------|
| **PiQ3 ≤ Uniform** | Gate G1 fails | Investigate step size tuning; if still fails, de-scope to research paper only |
| **LoRA-QAT OOM** | Training crashes | Reduce rank from 16→8→4; if still OOM, defer to GPU cluster |
| **Reasoning collapse** | >25 point GSM8K drop | Increase distillation weight λ_KD; if still fails, revert to PTQ-only |
| **SIMD kernel divergence** | >1 ULP vs scalar | Fix kernel bug; block merge until resolved |
| **Benchmark regression** | CI benchmark alerts | Bisect and fix; block merge until resolved |

---

## 1. Context and Problem Statement

### 1.1 Current State

ruvLLM implements multiple quantization approaches for post-training quantization (PTQ):

| Module | Path | Capability | Bits |
|--------|------|-----------|------|
| K-quants | `crates/ruvllm/src/quantize/ruvltra_quant.rs` | Q4_K_M, Q5_K_M, Q8_0 | 4.5-8.5 |
| BitNet b1.58 | `crates/ruvllm/src/bitnet/` (17 files) | Ternary {-1,0,+1} | 2.06 |
| GGUF types | `crates/ruvllm/src/gguf/quantization.rs` | 30+ formats incl. IQ1_S-IQ4_NL | 1.56-8.5 |
| KV cache | `crates/ruvllm/src/kv_cache.rs` | Two-tier FP16 tail + Q4 store | 4-16 |
| MoE cache | `crates/ruvllm/src/bitnet/expert_cache.rs` | LRU/LFU/ARC expert eviction | N/A |

### 1.2 Problem

1. **No quantization-aware training (QAT)**: All quantization is post-training. ICLR'26 shows QAT preserves ~90% reasoning at 2-bit vs ~40% for PTQ — a 30-point gap on GSM8K.

2. **Uniform quantization grids only**: Current K-quant and BitNet use evenly-spaced levels. Non-uniform grids (pi-constant, NormalFloat) can gain ~0.5 effective bits of precision.

3. **No incoherence processing**: QuIP-style Hadamard rotations decorrelate weights before quantization, making 2-bit viable without QAT. Not implemented.

4. **WASM quantization kernels incomplete**: `tl1_wasm.rs` handles ternary SIMD but no 2-bit/3-bit dequantization kernels exist for browser deployment.

5. **MoE routing ignores memory**: Standard top-K routing causes cache thrashing on edge devices. Memory-aware routing could improve throughput 54% with <1% accuracy loss.

### 1.3 Research Foundation

This ADR implements findings from `docs/research/quantization-edge/`:

| Document | Key Finding |
|----------|-------------|
| 01 - Survey | 2-bit QAT retains 90% reasoning; 6 viable methods exist |
| 02 - QAT | Two-stage calibration + teacher distillation is state-of-art |
| 03 - QuIP | Hadamard incoherence makes 2-bit PTQ viable (O(n log n)) |
| 04 - MoE | Memory-aware routing: +54% throughput, -0.5% accuracy |
| 05 - Architecture | Gap analysis: QAT loop + STE + differentiable quant missing |
| 06 - Implementation | 14-week Rust implementation plan with success criteria |
| 07 - Pi-Quant | Pi-constant scaling: ~0.5 bit gain, reduced spectral distortion |

### 1.4 Strategic Goal

Deliver 2-bit and 3-bit quantized inference with reasoning preservation for:
- **Edge devices**: ESP32-P4 (8 MB PSRAM), Raspberry Pi 5 (4 GB)
- **Mobile**: 2-3 GB available RAM
- **Browser**: WASM with SIMD128 acceleration
- **Server**: Reduced memory for higher batch sizes

Target: 0.5B model in <130 MB with >85% of FP16 reasoning quality.

---

## 2. Domain Analysis — Bounded Contexts

### 2.1 Strategic Domain Design

```
+====================================================================+
|          ULTRA-LOW-BIT QUANTIZATION SYSTEM (ADR-090)                |
+====================================================================+
|                                                                      |
|  +------------------+    +-------------------+    +----------------+ |
|  | Quantization     |    | Training          |    | MoE Routing    | |
|  | Core Domain      |--->| Domain            |    | Domain         | |
|  |                  |    |                   |    |                | |
|  | - Pi-Quant       |    | - QAT Loop        |    | - Memory-Aware | |
|  | - Incoherence    |    | - Calibration     |    | - Expert Page  | |
|  | - STE Ops        |    | - Distillation    |    | - Mixed Prec.  | |
|  | - Block Codecs   |    | - LoRA-QAT        |    | - SRAM Map     | |
|  +--------+---------+    +--------+----------+    +-------+--------+ |
|           |                       |                        |         |
|           v                       v                        v         |
|  +------------------+    +-------------------+                       |
|  | WASM Runtime     |    | Observability     |                       |
|  | Domain           |    | Domain            |                       |
|  |                  |    |                   |                       |
|  | - SIMD Kernels   |    | - Benchmarks      |                       |
|  | - Memory Mgmt    |    | - Security Valid. |                       |
|  | - Browser API    |    | - Quality Metrics |                       |
|  | - Web Workers    |    | - Profiling       |                       |
|  +------------------+    +-------------------+                       |
|                                                                      |
+====================================================================+
```

### 2.2 Bounded Context: Quantization Core Domain

**Responsibility**: Quantization primitives, format codecs, mathematical transforms.

**Aggregate Roots**:
- `PiQuantizer` — Pi-constant quantization with learnable scale
- `IncoherenceTransform` — Hadamard rotation for weight decorrelation
- `DifferentiableQuantOp` — Forward/backward through quantization with STE

**Value Objects**:
- `QuantGrid` — Quantization level set (uniform, pi-scaled, NormalFloat, learned)
- `BlockCodec` — Packed storage for 2-bit/3-bit blocks
- `QuantStats` — Per-layer quantization error statistics

**Domain Events**:
- `WeightsQuantized { layer, format, mse, spectral_distortion }`
- `IncoherenceApplied { layer, transform_type, mu_before, mu_after }`
- `GridOptimized { layer, centroids, improvement_pct }`

**Integration with existing code**:

| Existing File | Integration |
|---------------|-------------|
| `quantize/ruvltra_quant.rs` | Extend `TargetFormat` enum with `PiQ3`, `PiQ2`, `Q2_QuIP` |
| `quantize/mod.rs` | Re-export new modules |
| `bitnet/ternary_tensor.rs` | Reuse 2-bit packing for Pi-Q2 |
| `gguf/quantization.rs` | Register `PiQ3 = 40`, `PiQ2 = 41` types |

**New files**:

```
crates/ruvllm/src/quantize/
  pi_quant.rs              # Pi-constant quantization core
  pi_quant_simd.rs         # NEON/AVX2 kernels for pi-quant
  incoherence.rs           # Hadamard rotation transforms
  hadamard.rs              # Fast Walsh-Hadamard O(n log n)
  importance.rs            # Fisher information / sensitivity
  iq_quant.rs              # I-quant lattice quantization
```

### 2.3 Bounded Context: Training Domain

**Responsibility**: QAT training loop, calibration, distillation, LoRA-QAT.

**Aggregate Roots**:
- `QatTrainer` — Orchestrates the full QAT pipeline
- `CalibrationEngine` — Mixed-domain calibration for grid initialization
- `DistillationLoss` — Teacher-student composite loss computation

**Value Objects**:
- `QatConfig` — Training hyperparameters (bits, STE variant, loss weights)
- `SteGradient` — Gradient through quantization (Standard, Clipped, LSQ, EWGS)
- `CalibrationResult` — Per-layer scales, centroids, Fisher information

**Domain Events**:
- `CalibrationComplete { layers, domains, total_samples }`
- `QatEpochComplete { epoch, loss, ppl, reasoning_score }`
- `LoraQatConverged { adapter_rank, delta_quality }`

**Integration with existing code**:

| Existing File | Integration |
|---------------|-------------|
| `training/real_trainer.rs` | Add `QatMode` to training loop |
| `training/contrastive.rs` | Reuse hard negative mining for calibration |
| `training/grpo.rs` | Support quantized policy model |
| `lora/micro_lora.rs` | Add `AdapterMode::Qat` variant |
| `lora/training.rs` | LoRA-QAT gradient computation |
| `sona/integration.rs` | Tier 2: quantization scale adaptation |
| `sona/ruvltra_pretrain.rs` | QAT pretraining config for RuvLTRA-Small |
| `training/tool_dataset.rs` | Calibration data source (tool use domain) |
| `training/claude_dataset.rs` | Calibration data source (reasoning domain) |

**New files**:

```
crates/ruvllm/src/qat/
  mod.rs                   # Public API
  config.rs                # QatConfig, SteVariant, QuantGranularity
  ste.rs                   # Straight-through estimator implementations
  differentiable_quant.rs  # DifferentiableQuantizer trait + impls
  calibration.rs           # Mixed-domain calibration pipeline
  distillation.rs          # Teacher-student loss (L_task + L_KD + L_reasoning)
  reasoning_loss.rs        # Chain-of-thought fidelity loss
  training_loop.rs         # Main QAT training orchestrator
  lora_qat.rs              # LoRA-QAT lightweight variant
```

### 2.4 Bounded Context: MoE Routing Domain

**Responsibility**: Memory-aware expert selection, paging, mixed precision.

**Aggregate Roots**:
- `MemoryAwareRouter` — Expert selection with cache residency bonus
- `ExpertPrecisionAllocator` — Per-expert bit-width assignment
- `SramMapper` — Hardware memory hierarchy configuration

**Value Objects**:
- `ExpertPreference` — EMA-based long-term usage tracking
- `CacheResidencyState` — Hot/cold expert classification
- `PrecisionMap` — Expert ID to quantization format mapping

**Domain Events**:
- `ExpertPaged { expert_id, direction: In|Out, latency_us }`
- `PrecisionRebalanced { expert_id, old_bits, new_bits, reason }`
- `CacheHitRateChanged { old_rate, new_rate }`

**Integration with existing code**:

| Existing File | Integration |
|---------------|-------------|
| `bitnet/expert_cache.rs` | Extend `ExpertCache` with memory-aware routing |
| `bitnet/expert_cache.rs` | Extend `MoeBatchScheduler` with precision hints |
| `backends/mistral_backend.rs` | Mixtral model MoE routing hook |

**New files**:

```
crates/ruvllm/src/moe/
  mod.rs                    # Public API
  router.rs                 # MemoryAwareRouter with cache bonus
  expert_manager.rs         # Expert lifecycle + async paging
  precision_allocator.rs    # Frequency-based precision assignment
  sram_mapper.rs            # Platform-specific memory hierarchy config
```

### 2.5 Bounded Context: WASM Runtime Domain

**Responsibility**: Browser-compatible quantization, SIMD kernels, JS API.

**Aggregate Roots**:
- `PiQuantWasm` — Browser-side pi-quantization with WASM SIMD
- `QuantBenchWasm` — In-browser benchmarking for quantized models
- `QatConfigWasm` — Configuration for browser-based QAT (LoRA only)

**Value Objects**:
- `WasmSimdKernel` — Abstraction over WASM SIMD128 operations
- `QuantizedTensorWasm` — JS-accessible quantized weight storage
- `MemoryBudgetWasm` — Browser memory constraint configuration

**Integration with existing code**:

| Existing File | Integration |
|---------------|-------------|
| `crates/ruvllm-wasm/src/bindings.rs` | Add PiQuantWasm, QatConfigWasm exports |
| `crates/ruvllm-wasm/src/hnsw_router.rs` | Route quantization config by embedding similarity |
| `crates/ruvllm-wasm/src/micro_lora.rs` | LoRA-QAT mode for browser adaptation |
| `crates/ruvllm-wasm/src/sona_instant.rs` | Quality signal for dynamic precision |
| `crates/ruvllm/src/bitnet/tl1_wasm.rs` | Reuse LUT-based SIMD128 pattern |

**New files**:

```
crates/ruvllm-wasm/src/
  pi_quant_wasm.rs          # Pi-quantization WASM bindings
  quant_bench_wasm.rs       # In-browser quantization benchmarks

crates/ruvllm/src/quantize/
  pi_quant_wasm_simd.rs     # WASM SIMD128 kernels for pi-quant
```

**WASM binding pattern** (from existing codebase):

```rust
#[wasm_bindgen]
pub struct PiQuantWasm {
    #[wasm_bindgen(skip)]
    inner: PiQuantConfig,
}

#[wasm_bindgen]
impl PiQuantWasm {
    #[wasm_bindgen(constructor)]
    pub fn new(bits: u8, k: u8) -> Self { ... }

    #[wasm_bindgen(getter, js_name = bitsPerWeight)]
    pub fn bits_per_weight(&self) -> f32 { ... }

    pub fn quantize(&self, weights: &[f32]) -> Vec<u8> { ... }
    pub fn dequantize(&self, packed: &[u8]) -> Vec<f32> { ... }

    #[wasm_bindgen(js_name = toJson)]
    pub fn to_json(&self) -> Result<String, JsValue> { ... }

    #[wasm_bindgen(js_name = fromJson)]
    pub fn from_json(json: &str) -> Result<PiQuantWasm, JsValue> { ... }
}
```

### 2.6 Bounded Context: Observability Domain

**Responsibility**: Benchmarking, security validation, quality metrics, profiling.

**Aggregate Roots**:
- `QuantBenchSuite` — Criterion-based benchmark collection
- `SecurityValidator` — Weight integrity and bounds verification
- `QualityMonitor` — Runtime quality tracking with SONA integration

**Value Objects**:
- `BenchmarkResult` — Throughput, latency, memory, quality metrics
- `SecurityReport` — Validation results with severity levels
- `QualitySnapshot` — Per-layer quality scores at current precision

---

## 3. Decision: Architecture

### 3.1 Core Design Principles

| Principle | Rationale |
|-----------|-----------|
| **Extend, don't replace** | Add to `TargetFormat` enum, don't create parallel types |
| **Reuse SIMD patterns** | `tl1_wasm.rs` LUT approach works for multi-bit formats |
| **LoRA-QAT first** | 50 MB memory vs 114 GB for full QAT on 7B model |
| **Pi-quant as preprocessor** | Can layer on top of existing K-quant pipeline |
| **Domain events for cross-cutting** | Decouple domains via event bus |

### 3.2 Module Dependency Graph

```
quantize/pi_quant.rs ----+
                         |
quantize/incoherence.rs  +---> qat/training_loop.rs ---> sona/integration.rs
                         |           |
quantize/hadamard.rs ----+     qat/distillation.rs
                                    |
bitnet/ ---> qat/lora_qat.rs ------+
                                    |
                              qat/calibration.rs ---> training/tool_dataset.rs
                                                  \-> training/claude_dataset.rs

moe/router.rs ---> bitnet/expert_cache.rs
moe/precision_allocator.rs ---> quantize/pi_quant.rs

ruvllm-wasm/pi_quant_wasm.rs ---> quantize/pi_quant.rs
ruvllm-wasm/bindings.rs ---> (all WASM exports)
```

### 3.3 TargetFormat Extension

```rust
// In quantize/ruvltra_quant.rs — extend existing enum
pub enum TargetFormat {
    // Existing
    Q4_K_M,
    Q5_K_M,
    Q8_0,
    F16,
    // NEW: Pi-constant formats
    PiQ3,      // 3-bit pi-scaled (pi/4 step, 3.06 bits/weight with scale)
    PiQ2,      // 2-bit pi-scaled (pi/3 step, 2.06 bits/weight with scale)
    // NEW: QuIP-enhanced
    Q2_QuIP,   // 2-bit K-quant with Hadamard incoherence
}

impl TargetFormat {
    pub fn bits_per_weight(&self) -> f32 {
        match self {
            // ... existing ...
            TargetFormat::PiQ3 => 3.0625,   // 3 bits + scale overhead
            TargetFormat::PiQ2 => 2.0625,   // 2 bits + scale overhead
            TargetFormat::Q2_QuIP => 2.5625, // Q2_K + Hadamard metadata
        }
    }
}
```

### 3.4 QAT Training Loop Architecture

```rust
// In qat/training_loop.rs

pub struct QatTrainer {
    /// Quantization format for forward pass
    quant_op: Box<dyn DifferentiableQuantizer>,
    /// Optional teacher for distillation
    teacher: Option<Box<dyn TeacherModel>>,
    /// Calibration result (per-layer quant params)
    calibration: CalibrationResult,
    /// Training config
    config: QatConfig,
}

impl QatTrainer {
    /// Full QAT pipeline: calibrate -> train -> export
    pub async fn run(
        &mut self,
        model: &mut RuvLLMModel,
        dataset: &dyn QatDataset,
    ) -> Result<QatResult> {
        // Phase 1: Mixed-domain calibration
        let cal = self.calibrate(model, dataset)?;

        // Phase 2: Initialize quantization grids
        model.apply_quant_params(&cal)?;

        // Phase 3: Training epochs with STE
        for epoch in 0..self.config.epochs {
            let metrics = self.train_epoch(model, dataset, epoch)?;
            // Domain event: QatEpochComplete
        }

        // Phase 4: Export quantized model
        self.export(model)
    }
}
```

### 3.5 Pi-Quantization Core

```rust
// In quantize/pi_quant.rs
use std::f32::consts::PI;

/// Pi-constant quantization: w_q = round(w / (alpha * pi / k)) * (alpha * pi / k)
pub struct PiQuantizer {
    pub bits: u8,
    pub k: u8,
    pub alpha: Vec<f32>,  // per-channel learnable scale
}

impl PiQuantizer {
    #[inline(always)]
    pub fn quantize_scalar(&self, w: f32, channel: usize) -> (i8, f32) {
        let step = self.alpha[channel] * PI / (self.k as f32);
        let half = (1i8 << self.bits) / 2;
        let q = (w / step).round() as i8;
        let q_clamped = q.clamp(-half, half - 1);
        (q_clamped, q_clamped as f32 * step)
    }

    pub fn quantize_block(&self, weights: &[f32], channel: usize) -> Pi3BitBlock {
        // Pack 8 weights into 3 bytes (for 3-bit) or 4 weights into 1 byte (for 2-bit)
        // Uses pi/k as step size instead of uniform grid
        // ...
    }
}
```

### 3.6 Hadamard Incoherence Transform

```rust
// In quantize/hadamard.rs

/// Fast Walsh-Hadamard transform: O(n log n)
/// Decorrelates weight matrices before quantization
pub struct HadamardTransform {
    log_dim: u32,
    signs: Vec<i8>,  // random +/-1 for randomized variant
}

impl HadamardTransform {
    /// In-place butterfly: x[j], x[j+h] = x[j]+x[j+h], x[j]-x[j+h]
    pub fn forward_inplace(&self, data: &mut [f32]) {
        let n = 1usize << self.log_dim;
        // Apply random sign flips
        for (x, &s) in data.iter_mut().zip(self.signs.iter()) {
            *x *= s as f32;
        }
        // Walsh-Hadamard butterfly
        let mut h = 1;
        while h < n {
            for i in (0..n).step_by(h * 2) {
                for j in i..i + h {
                    let a = data[j];
                    let b = data[j + h];
                    data[j] = a + b;
                    data[j + h] = a - b;
                }
            }
            h *= 2;
        }
        let norm = (n as f32).sqrt();
        data.iter_mut().for_each(|x| *x /= norm);
    }
}
```

### 3.7 Straight-Through Estimator

```rust
// In qat/ste.rs

pub enum SteVariant {
    /// dw = dq (identity, gradient passes through)
    Standard,
    /// dw = dq * 1{|w| <= clip}, zero outside range
    Clipped { clip_val: f32 },
    /// Scale is learned: ds/dalpha computed alongside dw
    LearnedStepSize,
    /// dw = dq * (1 + lambda * |w - q|), stronger push toward stable points
    Ewgs { lambda: f32 },
}

impl SteVariant {
    pub fn backward(
        &self,
        w: f32,       // latent weight
        q: f32,       // quantized weight
        grad_out: f32, // upstream gradient
    ) -> f32 {
        match self {
            Self::Standard => grad_out,
            Self::Clipped { clip_val } => {
                if w.abs() <= *clip_val { grad_out } else { 0.0 }
            }
            Self::Ewgs { lambda } => {
                grad_out * (1.0 + lambda * (w - q).abs())
            }
            Self::LearnedStepSize => grad_out, // scale grad computed separately
        }
    }
}
```

---

## 4. Security

### 4.1 Threat Model

| Threat | Vector | Severity | Mitigation |
|--------|--------|----------|------------|
| Weight tampering | Modified GGUF on disk | Critical | SHA-256 checksum in GGUF metadata |
| Quantization overflow | Crafted input weights | High | Clamp + assert (existing pattern from `kv_cache.rs`) |
| WASM memory escape | Malicious WASM module | High | Linear memory sandbox (wasm-bindgen default) |
| Adversarial calibration | Poisoned calibration data | Medium | Distribution validation + outlier detection |
| Model extraction | WASM binary inspection | Low | Acceptable risk for edge deployment |

### 4.2 Weight Integrity Validation

```rust
// In new: quantize/security.rs

pub struct WeightIntegrity {
    /// SHA-256 of original FP32 weights
    pub original_hash: [u8; 32],
    /// SHA-256 of quantized weights
    pub quantized_hash: [u8; 32],
    /// Maximum per-layer quantization error
    pub max_layer_mse: f32,
    /// Quantization config used
    pub config_hash: [u8; 32],
}

/// Validate quantized model integrity
pub fn validate_quantized_model(
    path: &Path,
    expected: &WeightIntegrity,
) -> Result<ValidationReport> {
    // 1. Verify GGUF magic bytes and version
    // 2. Compute SHA-256 of weight data
    // 3. Compare against expected hash
    // 4. Verify per-layer MSE within bounds
    // 5. Check quantization config matches
}
```

### 4.3 Quantization Bounds Enforcement

Reuse existing validation patterns from the codebase:

```rust
// Pattern from kv_cache.rs and ruvltra_quant.rs:
assert!(new_len <= self.capacity, "bounds check: {} > {}", new_len, self.capacity);

// Applied to pi-quantization:
let q = (w / step).round() as i8;
let q_clamped = q.clamp(-half_range, half_range - 1);  // ALWAYS clamp
debug_assert!(
    q_clamped >= -half_range && q_clamped < half_range,
    "quantization overflow: q={}, range=[{}, {})", q, -half_range, half_range - 1
);
```

### 4.4 GGUF Format Validation

```rust
/// Validate GGUF file before loading quantized weights
pub fn validate_gguf_security(path: &Path) -> Result<GgufSecurityReport> {
    // 1. Magic bytes: must be 0x46475547 ("GGUF")
    // 2. Version: must be 2 or 3
    // 3. Tensor count: sanity check (< 10000)
    // 4. Metadata size: sanity check (< 100 MB)
    // 5. Quantization types: only known types allowed
    // 6. Tensor dimensions: within reasonable bounds
    // 7. File size: matches sum of tensor sizes
}
```

### 4.5 WASM Sandbox Security

- WASM linear memory is isolated by default (wasm-bindgen)
- No filesystem access from browser context
- Memory budget enforced via `MemoryBudgetWasm` (configurable cap)
- Serialization uses `serde_json` with size limits

---

## 5. Benchmarking

### 5.1 Benchmark Suite Extension

Extend existing `crates/ruvllm/benches/` with new benchmark groups:

```rust
// New: benches/pi_quant_bench.rs
use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};

fn bench_pi_quantize(c: &mut Criterion) {
    let mut group = c.benchmark_group("pi-quantization");

    for &size in &[256, 4096, 4096 * 11008] {
        let weights: Vec<f32> = (0..size).map(|i| (i as f32 * 0.001).sin()).collect();
        let config = PiQuantConfig { bits: 3, k: 4, alpha: vec![1.0], .. };

        group.bench_with_input(
            BenchmarkId::new("pi-q3", size),
            &weights,
            |b, w| b.iter(|| pi_quantize_tensor(black_box(w), &config)),
        );
    }
    group.finish();
}

fn bench_pi_dequantize_simd(c: &mut Criterion) {
    // NEON vs scalar dequantization
}

fn bench_hadamard_transform(c: &mut Criterion) {
    // Fast Walsh-Hadamard at various dimensions
}

fn bench_qat_forward_backward(c: &mut Criterion) {
    // Single QAT step: forward (quantized) + backward (STE)
}

criterion_group!(
    pi_quant_benches,
    bench_pi_quantize,
    bench_pi_dequantize_simd,
    bench_hadamard_transform,
    bench_qat_forward_backward,
);
criterion_main!(pi_quant_benches);
```

### 5.2 Benchmark Targets

| Benchmark | Metric | Target | Baseline |
|-----------|--------|--------|----------|
| Pi-Q3 quantize (4096 weights) | Throughput | >1 GB/s | N/A (new) |
| Pi-Q3 dequantize NEON (4096) | Throughput | >10 GB/s | Q4_K_M: ~16 GB/s |
| Pi-Q3 dequantize WASM (4096) | Throughput | >2 GB/s | BitNet WASM: ~3 GB/s |
| Hadamard 4096-dim | Latency | <50 us | N/A (new) |
| QAT forward+backward (0.5B) | Step time | <500 ms | Standard train: ~200 ms |
| KV cache Q2 dequant | Throughput | >8 GB/s | Q4: ~16 GB/s |
| MoE routing (16 experts) | Latency | <10 us | Standard: ~5 us |
| Memory-aware MoE cache hit | Rate | >70% | Standard: ~34% |

### 5.3 Quality Benchmarks

```
Evaluation harness (extend evaluation/real_harness.rs):

Model: RuvLTRA-Small 0.5B
Datasets: WikiText-2, GSM8K, HumanEval, MCP Tool Use

Configs to benchmark:
  FP16             (baseline)
  Q4_K_M           (current best)
  Q2_K             (current 2-bit)
  BitNet 1.58      (current ternary)
  Pi-Q3 (PTQ)      (new, no training)
  Pi-Q3 + QAT      (new, with training)
  Pi-Q2 (PTQ)      (new, no training)
  Pi-Q2 + QAT      (new, with training)
  Q2_QuIP          (new, incoherence)
  Q2_QuIP + QAT    (new, combined)
```

### 5.4 Regression Detection

```toml
# In benches/Cargo.toml or criterion config
[profile.bench]
# Alert if throughput drops >5% from baseline
[[bench]]
name = "pi_quant_bench"
harness = false

# Stored baselines in benches/baselines/
# cargo bench --save-baseline v0.1
# cargo bench --baseline v0.1  (compare against saved)
```

---

## 6. Optimization

### 6.1 SIMD Kernel Strategy

| Platform | ISA | Width | Key Ops | Priority |
|----------|-----|-------|---------|----------|
| Apple M4 | NEON | 128-bit (4xf32) | vaddq_f32, vmulq_f32, vcvtq_f32_s32 | P0 |
| x86_64 | AVX2 | 256-bit (8xf32) | _mm256_add_ps, _mm256_mul_ps | P1 |
| WASM | SIMD128 | 128-bit (4xf32) | f32x4_add, f32x4_mul, i8x16_swizzle | P0 |
| Fallback | Scalar | 1xf32 | Standard Rust | P0 |

### 6.2 NEON Kernel: Pi-Quant Dequantize

```rust
#[cfg(target_arch = "aarch64")]
pub unsafe fn pi_dequantize_neon(
    packed: &[u8],     // 3-bit packed data
    scale: f32,        // alpha * pi / k
    output: &mut [f32],
) {
    use std::arch::aarch64::*;
    let scale_v = vdupq_n_f32(scale);

    // Process 8 values at a time (two NEON registers)
    for chunk_idx in 0..output.len() / 8 {
        let values = unpack_3bit_8(&packed[chunk_idx * 3..]);
        // Convert i8 -> i32 -> f32 -> scaled f32
        let lo = vcvtq_f32_s32(vmovl_s16(vget_low_s16(vmovl_s8(vld1_s8(values.as_ptr())))));
        let hi = vcvtq_f32_s32(vmovl_s16(vget_high_s16(vmovl_s8(vld1_s8(values.as_ptr())))));
        vst1q_f32(output.as_mut_ptr().add(chunk_idx * 8), vmulq_f32(lo, scale_v));
        vst1q_f32(output.as_mut_ptr().add(chunk_idx * 8 + 4), vmulq_f32(hi, scale_v));
    }
}
```

### 6.3 WASM SIMD128 Kernel: Reuse TL1 Pattern

From `bitnet/tl1_wasm.rs`, the LUT-based approach generalizes to multi-bit:

```rust
#[cfg(target_arch = "wasm32")]
pub fn pi_dequant_wasm_simd(
    packed: &[u8],
    scale: f32,
    output: &mut [f32],
) {
    use core::arch::wasm32::*;

    let scale_v = f32x4_splat(scale);

    // For 3-bit: use LUT with 8 entries instead of BitNet's 4
    // Swizzle decodes packed bits to signed integers
    // Same i8x16_swizzle pattern as tl1_wasm.rs
    for i in (0..output.len()).step_by(4) {
        let indices = unpack_3bit_4(&packed[..]);
        let ints = decode_3bit_lut(indices);
        let floats = f32x4_convert_i32x4(ints);
        let scaled = f32x4_mul(floats, scale_v);
        v128_store(output.as_mut_ptr().add(i) as *mut v128, scaled);
    }
}
```

### 6.4 Memory Optimization

**Arena allocator integration** (reuse `memory_pool.rs`):

```rust
// Quantization temporary buffers from InferenceArena
let arena = InferenceArena::new(64 * 1024 * 1024); // 64 MB

// Calibration activations (temporary, freed after calibration)
let activations = arena.alloc::<f32>(batch_size * hidden_dim)?;

// Quantized weight blocks (permanent, moved to model)
let blocks = arena.alloc::<Pi3BitBlock>(num_blocks)?;
```

**Buffer pool for quantized KV cache**:

```rust
// Extend BufferPool size classes for Q2 blocks
let pool = BufferPool::new();
pool.add_size_class(66);    // BitNet block: 64 bytes ternary + 2 bytes scale
pool.add_size_class(5);     // Pi3BitBlock: 3 bytes data + 2 bytes scale
pool.prewarm_all(1024);     // Pre-allocate 1024 blocks per class
```

### 6.5 Hadamard Optimization

```
Hadamard complexity analysis:

Full orthogonal rotation:  O(d^2)
  d=4096:  16.7M multiply-add ops per layer
  d=11008: 121M multiply-add ops per layer

Fast Walsh-Hadamard:       O(d * log2(d))
  d=4096:  49K multiply-add ops per layer  (340x faster)
  d=11008: 143K multiply-add ops per layer (846x faster)

NEON-vectorized WHT:       O(d * log2(d) / 4)
  d=4096:  12K vector ops per layer
  Estimated time: ~3 us per layer, ~100 us total (32 layers)
```

---

## 7. WASM Implementation

### 7.1 New WASM Exports

```rust
// In crates/ruvllm-wasm/src/pi_quant_wasm.rs

#[wasm_bindgen]
pub struct PiQuantWasm {
    #[wasm_bindgen(skip)]
    config: PiQuantConfig,
}

#[wasm_bindgen]
impl PiQuantWasm {
    #[wasm_bindgen(constructor)]
    pub fn new(bits: u8, k: u8) -> Self;

    #[wasm_bindgen(getter, js_name = bitsPerWeight)]
    pub fn bits_per_weight(&self) -> f32;

    #[wasm_bindgen(getter, js_name = stepSize)]
    pub fn step_size(&self) -> f32;

    /// Quantize FP32 weights to pi-scaled packed format
    pub fn quantize(&self, weights: &[f32]) -> Vec<u8>;

    /// Dequantize packed format back to FP32
    pub fn dequantize(&self, packed: &[u8]) -> Vec<f32>;

    /// Compute MSE between original and quantized
    pub fn compute_mse(&self, original: &[f32], quantized: &[u8]) -> f32;

    /// Compute spectral distortion (dB)
    #[wasm_bindgen(js_name = spectralDistortion)]
    pub fn spectral_distortion(&self, original: &[f32], quantized: &[u8]) -> f32;

    /// Serialize to JSON for localStorage persistence
    #[wasm_bindgen(js_name = toJson)]
    pub fn to_json(&self) -> Result<String, JsValue>;

    /// Deserialize from JSON
    #[wasm_bindgen(js_name = fromJson)]
    pub fn from_json(json: &str) -> Result<PiQuantWasm, JsValue>;
}

#[wasm_bindgen]
pub struct QuantBenchWasm { ... }

#[wasm_bindgen]
impl QuantBenchWasm {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self;

    /// Run quantization benchmark, returns JSON result
    pub fn run_bench(&self, weights: &[f32], bits: u8, iterations: u32) -> String;

    /// Compare multiple formats
    #[wasm_bindgen(js_name = compareFormats)]
    pub fn compare_formats(&self, weights: &[f32]) -> String;
}
```

### 7.2 Cargo Feature Gating

```toml
# In crates/ruvllm-wasm/Cargo.toml
[features]
default = ["simd"]
simd = []
pi-quant = []       # Pi-constant quantization
qat = ["pi-quant"]  # QAT requires pi-quant
webgpu = [...]      # Existing GPU feature
```

### 7.3 WASM Build Profile

Reuse existing optimized profile from `ruvector-wasm`:

```toml
[profile.release]
opt-level = "z"       # Minimize binary size
lto = true            # Link-time optimization (unless Rust codegen bug triggers)
panic = "abort"       # No unwinding in WASM
codegen-units = 1     # Maximum optimization

[package.metadata.wasm-pack.profile.release]
wasm-opt = false      # Disable wasm-opt per ADR-084 workaround
```

### 7.4 Browser Integration Example

```javascript
import init, { PiQuantWasm, QuantBenchWasm } from '@ruvector/ruvllm-wasm';

await init();

// Create pi-quantizer (3-bit, k=4)
const quant = new PiQuantWasm(3, 4);
console.log(`Step size: ${quant.stepSize}`);      // 0.7854 (pi/4)
console.log(`Bits/weight: ${quant.bitsPerWeight}`); // 3.0625

// Quantize weights
const weights = new Float32Array([0.5, -0.3, 0.8, -1.2, ...]);
const packed = quant.quantize(weights);
const restored = quant.dequantize(packed);
const mse = quant.computeMse(weights, packed);

// Run benchmark
const bench = new QuantBenchWasm();
const results = JSON.parse(bench.compareFormats(weights));
// { "pi-q3": { "mse": 0.051, "throughput_mbs": 2400 }, ... }
```

---

## 8. Integration with Existing Crates

### 8.1 Integration Map

| Existing Component | File | Integration Point | Change Type |
|-------------------|------|-------------------|-------------|
| TargetFormat enum | `quantize/ruvltra_quant.rs` | Add PiQ3, PiQ2, Q2_QuIP variants | Extend |
| GgufQuantType enum | `gguf/quantization.rs` | Register PiQ3=40, PiQ2=41 | Extend |
| QuantConfig struct | `quantize/ruvltra_quant.rs` | Add pi_k, use_incoherence fields | Extend |
| Training loop | `training/real_trainer.rs` | Add QatMode variant | Extend |
| Contrastive sampler | `training/contrastive.rs` | Reuse for calibration data | Reuse |
| Tool dataset | `training/tool_dataset.rs` | Calibration source: tool use domain | Reuse |
| Claude dataset | `training/claude_dataset.rs` | Calibration source: reasoning domain | Reuse |
| MicroLoRA | `lora/micro_lora.rs` | Add AdapterMode::Qat | Extend |
| LoRA training | `lora/training.rs` | LoRA-QAT gradient path | Extend |
| SONA engine | `sona/integration.rs` | Tier 2: scale adaptation | Extend |
| SONA pretraining | `sona/ruvltra_pretrain.rs` | QAT-aware pretraining config | Extend |
| KV cache | `kv_cache.rs` | Add Q2 cold store precision | Extend |
| Expert cache | `bitnet/expert_cache.rs` | Add memory-aware routing | Extend |
| MoE scheduler | `bitnet/expert_cache.rs` | Add precision hints | Extend |
| WASM SIMD | `bitnet/tl1_wasm.rs` | Reuse LUT pattern for multi-bit | Pattern reuse |
| WASM bindings | `ruvllm-wasm/bindings.rs` | Export PiQuantWasm, QuantBenchWasm | Extend |
| HNSW router | `ruvllm-wasm/hnsw_router.rs` | Route quantization config by similarity | Reuse |
| SONA instant | `ruvllm-wasm/sona_instant.rs` | Quality signal for dynamic precision | Reuse |
| Arena allocator | `memory_pool.rs` | Temp buffers for calibration/quant | Reuse |
| Buffer pool | `memory_pool.rs` | Size classes for Q2/Pi3Bit blocks | Extend |
| Evaluation harness | `evaluation/real_harness.rs` | Add quantized model evaluation | Extend |
| Benchmarks | `benches/ruvltra_benchmark.rs` | Add pi-quant throughput benchmarks | Extend |

### 8.2 Dependency Flow

```
External (no changes):
  ruvector-core          # Vector storage (used by SONA, witness log)
  ruvector-attention     # Multi-head attention (unaffected by weight quant)
  ruvector-gnn           # Graph neural networks (separate concern)

Internal (extend):
  ruvllm                 # Main crate: new modules in quantize/, qat/, moe/
  ruvllm-wasm            # WASM: new pi_quant_wasm.rs, quant_bench_wasm.rs
  sona                   # Learning: quantization-aware tier updates

No changes needed:
  ruvllm-cli             # CLI: picks up new TargetFormat variants automatically
  ruvector-postgres      # SQL quantization is independent concern
```

---

## 9. Implementation Timeline

| Week | Phase | Deliverables | Key Files |
|------|-------|-------------|-----------|
| 1-2 | **Foundation** | STE implementations, Pi-Quant core | `qat/ste.rs`, `quantize/pi_quant.rs` |
| 3 | **Incoherence** | Hadamard transform, incoherence processing | `quantize/hadamard.rs`, `quantize/incoherence.rs` |
| 4-5 | **Training** | Calibration, distillation, reasoning loss | `qat/calibration.rs`, `qat/distillation.rs` |
| 6 | **QAT Loop** | Main training orchestrator | `qat/training_loop.rs`, `qat/config.rs` |
| 7-8 | **LoRA-QAT** | Lightweight QAT via LoRA adapters | `qat/lora_qat.rs`, `lora/micro_lora.rs` update |
| 9-10 | **MoE** | Memory-aware routing, mixed precision | `moe/router.rs`, `moe/precision_allocator.rs` |
| 11 | **WASM** | Browser quantization, SIMD kernels | `pi_quant_wasm.rs`, `pi_quant_wasm_simd.rs` |
| 12 | **Security** | Weight validation, GGUF security | `quantize/security.rs` |
| 13 | **Benchmarks** | Criterion suite, quality evaluation | `benches/pi_quant_bench.rs` |
| 14 | **Integration** | SONA integration, CLI, documentation | All modules, integration tests |

---

## 10. Success Criteria

### 10.1 Correctness Criteria

| Criterion | Target | Validation Method |
|-----------|--------|-------------------|
| Scalar-SIMD equivalence | ≤1 ULP difference | Automated fuzz testing |
| STE gradient correctness | Matches PyTorch reference | Reference implementation comparison |
| Hadamard invertibility | H × H^T = n × I to machine precision | Property test |
| Calibration determinism | Same input → identical output | Seeded RNG + hash comparison |
| GGUF format compliance | Type IDs 40/41 parse correctly | llama.cpp interop test |

**Gate**: All correctness criteria must pass before any performance testing.

### 10.2 Performance Criteria

| Metric | Target | Method | Regression Threshold |
|--------|--------|--------|---------------------|
| Pi-quant NEON throughput | >10 GB/s dequantize | criterion benchmark | -5% |
| Pi-quant WASM throughput | >2 GB/s dequantize | in-browser benchmark | -10% |
| Hadamard 4096-dim latency | <50 μs | criterion benchmark | +10% |
| QAT step time (0.5B) | <500 ms/step | training loop | +20% |
| LoRA-QAT memory (0.5B) | <2 GB total | profiling | +50% |
| WASM binary size delta | <+50 KB | wasm-pack build | +100 KB |
| Existing Q4_K_M throughput | No regression | criterion comparison | -5% |
| Existing BitNet throughput | No regression | criterion comparison | -5% |

**Gate**: No existing benchmark may regress beyond threshold.

### 10.3 Model Quality Criteria

| Metric | FP16 Baseline | PiQ3 PTQ Target | PiQ3 + QAT Target | PiQ2 + QAT Target |
|--------|---------------|-----------------|-------------------|-------------------|
| WikiText-2 PPL | 12.3 | <14.5 (+18%) | <13.5 (+10%) | <16.0 (+30%) |
| GSM8K Accuracy | 45% | >30% (-15pt) | >38% (-7pt) | >32% (-13pt) |
| HumanEval pass@1 | 28% | >18% (-10pt) | >24% (-4pt) | >20% (-8pt) |
| Tool Use Success | 92% | >82% (-10pt) | >88% (-4pt) | >85% (-7pt) |
| Model Size (0.5B) | 1 GB | 190 MB | 195 MB | 130 MB |

**Gate**: PTQ targets are Phase 1 exit criteria; QAT targets are Phase 2+ exit criteria.

### 10.4 Rollout Readiness Criteria

| Criterion | Requirement | Validation |
|-----------|-------------|------------|
| Documentation | All public APIs documented | cargo doc --no-deps passes |
| Examples | One example per WASM export | examples/ directory check |
| Error messages | User-actionable error strings | Manual review |
| CI integration | All phases in CI pipeline | GitHub Actions check |
| Security review | Weight integrity validation functional | Security test suite |
| Benchmark baselines | Criterion baselines saved | benches/baselines/ populated |
| CHANGELOG | Release notes prepared | CHANGELOG.md entry |

**Gate**: All rollout criteria must pass before merge to main.

---

## 11. Consequences

### 11.1 Positive

- **8x memory reduction**: 7B model at 2-bit = 1.75 GB (from 14 GB FP16)
- **Edge viability**: 0.5B model in 130 MB fits ESP32-P4 PSRAM
- **Reasoning preservation**: QAT retains ~90% vs ~40% for PTQ at 2-bit
- **Browser deployment**: WASM SIMD kernels enable client-side inference
- **Novel contribution**: Pi-constant quantization is a publishable approach
- **Reuses 80%+ of existing code**: Minimal new infrastructure needed

### 11.2 Negative

- **Training cost**: QAT requires GPU hours for fine-tuning
- **Complexity**: 5 bounded contexts add architectural surface area
- **Maintenance**: New quantization formats need ongoing kernel support
- **WASM binary size**: Additional kernels increase download size (~50 KB)

### 11.3 Mitigations

- **LoRA-QAT** reduces training cost to 1 epoch on single GPU
- **DDD boundaries** keep complexity isolated per domain
- **Shared SIMD patterns** (tl1_wasm.rs LUT approach) reduce kernel duplication
- **Feature gating** (`pi-quant`, `qat` features) keeps base binary lean

---

## 12. Related Decisions

- **ADR-024**: Craftsman Ultra 30b 1bit — BitNet integration (ternary quantization)
- **ADR-084**: ruvllm-wasm — First functional npm publish (WASM build patterns)
- **ADR-016**: Delta-Behavior DDD Architecture (DDD pattern reference)
- **ADR-002**: RuvLLM Integration (core ruvllm architecture)
- **ADR-005**: WASM Runtime Integration (WASM infrastructure)
- **ADR-074**: RuvLLM Neural Embeddings (embedding system)

---

## 13. References

- `docs/research/quantization-edge/01-ultra-low-bit-quantization-survey.md`
- `docs/research/quantization-edge/02-quantization-aware-training-qat.md`
- `docs/research/quantization-edge/03-quip-2bit-framework.md`
- `docs/research/quantization-edge/04-moe-memory-aware-routing.md`
- `docs/research/quantization-edge/05-ruvllm-quantization-architecture.md`
- `docs/research/quantization-edge/06-implementation-plan-rust-ruvllm.md`
- `docs/research/quantization-edge/07-3int-pi-constant-quantization.md`
- ICLR 2026: "Reasoning-Oriented QAT for 2-Bit LLMs"
- QuIP (Cornell/RelaxML): Incoherence processing for 2-bit LLM quantization
- LLM-QAT (Meta): Reusable QAT training loop with KV-cache quantization
- ParetoQ: Multi-objective ultra-low-bit quantization
- BitNet b1.58 (Microsoft Research): Ternary weight quantization

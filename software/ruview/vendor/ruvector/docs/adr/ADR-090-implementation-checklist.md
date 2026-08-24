# ADR-090 Implementation Checklist: Ultra-Low-Bit QAT & Pi-Quantization

**ADR**: ADR-090-ultra-low-bit-qat-pi-quantization-ddd.md
**Status**: Ready for Implementation (Staged)
**Target Crate**: `ruvllm`

---

## Phase 1: PiQ3 PTQ (Weeks 1-3)

### 1.1 Pi-Quantization Core

- [ ] **File**: `crates/ruvllm/src/quantize/pi_quant.rs`
  - [ ] `PiQuantizer` struct (bits, k, alpha per-channel)
  - [ ] `quantize_scalar()` method with pi/k step size
  - [ ] `quantize_block()` method for packed storage
  - [ ] `Pi3BitBlock` packed format (3 bytes → 8 values)
  - [ ] `Pi2BitBlock` packed format (1 byte → 4 values)
  - [ ] Unit tests for quantize/dequantize round-trip
  - [ ] **Invariant Check**: INV-2 (scale positivity), INV-3 (step size constraint)

### 1.2 Pi-Quant SIMD Kernels

- [ ] **File**: `crates/ruvllm/src/quantize/pi_quant_simd.rs`
  - [ ] `pi_dequantize_neon()` - ARM NEON kernel
  - [ ] `pi_dequantize_avx2()` - x86_64 AVX2 kernel
  - [ ] Scalar reference implementation
  - [ ] Kernel equivalence tests (≤1 ULP, INV-8)

### 1.3 TargetFormat Extension

- [ ] **File**: `crates/ruvllm/src/quantize/ruvltra_quant.rs`
  - [ ] Add `PiQ3` variant to `TargetFormat` enum
  - [ ] Add `PiQ2` variant to `TargetFormat` enum
  - [ ] Implement `bits_per_weight()` for new variants
  - [ ] Update `quantize_tensor()` dispatch

### 1.4 GGUF Type Registration

- [ ] **File**: `crates/ruvllm/src/gguf/quantization.rs`
  - [ ] Register `PiQ3 = 40` type ID (INV-7)
  - [ ] Register `PiQ2 = 41` type ID
  - [ ] Implement serialization/deserialization

### 1.5 Phase 1 Validation

- [ ] **Metrics collection**: MSE, spectral distortion, cosine similarity, outlier retention
- [ ] **Gate G1**: PiQ3 beats uniform Q3 on ≥2/4 quality metrics
- [ ] **Benchmark**: `benches/pi_quant_bench.rs` created

---

## Phase 2: PiQ3 + LoRA-QAT (Weeks 4-7)

### 2.1 Straight-Through Estimator

- [ ] **File**: `crates/ruvllm/src/qat/ste.rs`
  - [ ] `SteVariant` enum (Standard, Clipped, LearnedStepSize, Ewgs)
  - [ ] `backward()` method for each variant
  - [ ] Gradient correctness tests vs PyTorch reference (INV-1)

### 2.2 Differentiable Quantization

- [ ] **File**: `crates/ruvllm/src/qat/differentiable_quant.rs`
  - [ ] `DifferentiableQuantizer` trait
  - [ ] `PiQuantDifferentiable` impl
  - [ ] Forward/backward pass with STE
  - [ ] Scale gradient computation for LSQ variant

### 2.3 Calibration Pipeline

- [ ] **File**: `crates/ruvllm/src/qat/calibration.rs`
  - [ ] `CalibrationEngine` aggregate root
  - [ ] Mixed-domain calibration (tool use + reasoning)
  - [ ] Per-layer scale/zero-point initialization
  - [ ] Calibration artifact serialization (INV-5)
  - [ ] Integration with `training/tool_dataset.rs`
  - [ ] Integration with `training/claude_dataset.rs`

### 2.4 Distillation Loss

- [ ] **File**: `crates/ruvllm/src/qat/distillation.rs`
  - [ ] `DistillationLoss` struct
  - [ ] `L_task` component (task loss)
  - [ ] `L_KD` component (KL divergence from teacher)
  - [ ] `L_reasoning` component (CoT fidelity)
  - [ ] Composite loss with configurable weights

### 2.5 Reasoning Loss

- [ ] **File**: `crates/ruvllm/src/qat/reasoning_loss.rs`
  - [ ] Chain-of-thought fidelity loss
  - [ ] Step-wise reasoning preservation
  - [ ] Integration with evaluation harness

### 2.6 LoRA-QAT Integration

- [ ] **File**: `crates/ruvllm/src/qat/lora_qat.rs`
  - [ ] `LoraQatTrainer` struct
  - [ ] Quantization-aware LoRA forward pass
  - [ ] Memory-efficient gradient checkpointing
  - [ ] Integration with `lora/micro_lora.rs`
  - [ ] Integration with `lora/training.rs`

- [ ] **File**: `crates/ruvllm/src/lora/micro_lora.rs` (extend)
  - [ ] Add `AdapterMode::Qat` variant
  - [ ] Support quantized base + FP32 adapter

### 2.7 QAT Training Loop

- [ ] **File**: `crates/ruvllm/src/qat/training_loop.rs`
  - [ ] `QatTrainer` orchestrator
  - [ ] `run()` method: calibrate → train → export
  - [ ] Epoch metrics: loss, PPL, reasoning score
  - [ ] Domain event emission: `QatEpochComplete`

### 2.8 QAT Config

- [ ] **File**: `crates/ruvllm/src/qat/config.rs`
  - [ ] `QatConfig` struct (bits, STE variant, loss weights, epochs)
  - [ ] `QuantGranularity` enum (PerTensor, PerChannel, PerToken)
  - [ ] Serialization for config persistence

### 2.9 Phase 2 Validation

- [ ] **Reasoning metrics**: PPL delta, GSM8K delta, HumanEval delta, tool use delta, long context
- [ ] **Gate G2**: All 5 reasoning metrics within acceptable delta
- [ ] **Memory check**: LoRA-QAT ≤2 GB for 0.5B model

---

## Phase 3: PiQ2 + Incoherence (Weeks 8-10)

### 3.1 Hadamard Transform

- [ ] **File**: `crates/ruvllm/src/quantize/hadamard.rs`
  - [ ] `HadamardTransform` struct
  - [ ] `forward_inplace()` - O(n log n) Walsh-Hadamard
  - [ ] `inverse_inplace()` - inverse transform
  - [ ] Random sign flip support
  - [ ] Property test: H × H^T = n × I (INV-4)

### 3.2 Incoherence Processing

- [ ] **File**: `crates/ruvllm/src/quantize/incoherence.rs`
  - [ ] `IncoherenceTransform` aggregate root
  - [ ] Apply Hadamard before quantization
  - [ ] Store transform metadata
  - [ ] Domain event: `IncoherenceApplied`

### 3.3 QuIP-Enhanced Quantization

- [ ] **File**: `crates/ruvllm/src/quantize/quip.rs`
  - [ ] `Q2_QuIP` variant in TargetFormat
  - [ ] Combine incoherence + 2-bit K-quant
  - [ ] Metadata for inverse transform

### 3.4 PiQ2 Implementation

- [ ] **File**: `crates/ruvllm/src/quantize/pi_quant.rs` (extend)
  - [ ] `quantize_2bit()` method
  - [ ] 2-bit packing (reuse `bitnet/ternary_tensor.rs` pattern)
  - [ ] Integration with incoherence pipeline

### 3.5 Phase 3 Validation

- [ ] **Gate G3**: PiQ2 + incoherence achieves acceptable quality without full QAT
- [ ] **Benchmark**: 2-bit throughput targets

---

## Phase 4: WASM Integration (Week 11)

### 4.1 WASM SIMD Kernels

- [ ] **File**: `crates/ruvllm/src/quantize/pi_quant_wasm_simd.rs`
  - [ ] `pi_dequant_wasm_simd()` using SIMD128
  - [ ] Reuse LUT pattern from `bitnet/tl1_wasm.rs`
  - [ ] In-browser kernel tests

### 4.2 WASM Bindings

- [ ] **File**: `crates/ruvllm-wasm/src/pi_quant_wasm.rs`
  - [ ] `PiQuantWasm` wasm_bindgen struct
  - [ ] `quantize()` method
  - [ ] `dequantize()` method
  - [ ] `computeMse()` method
  - [ ] `spectralDistortion()` method
  - [ ] JSON serialization

### 4.3 WASM Benchmarks

- [ ] **File**: `crates/ruvllm-wasm/src/quant_bench_wasm.rs`
  - [ ] `QuantBenchWasm` struct
  - [ ] `runBench()` method
  - [ ] `compareFormats()` method

### 4.4 WASM Feature Gating

- [ ] **File**: `crates/ruvllm-wasm/Cargo.toml`
  - [ ] Add `pi-quant` feature
  - [ ] Add `qat` feature (depends on `pi-quant`)
  - [ ] Feature flag tests

---

## Phase 5: Security & Observability (Week 12)

### 5.1 Weight Integrity

- [ ] **File**: `crates/ruvllm/src/quantize/security.rs`
  - [ ] `WeightIntegrity` struct (hashes, config)
  - [ ] `validate_quantized_model()` function
  - [ ] SHA-256 checksum computation
  - [ ] GGUF security validation

### 5.2 Observability

- [ ] **File**: `crates/ruvllm/src/qat/metrics.rs`
  - [ ] Per-epoch loss tracking
  - [ ] Quality metric export
  - [ ] Training duration metrics

---

## Phase 6: Integration & Benchmarks (Weeks 13-14)

### 6.1 SONA Integration

- [ ] **File**: `crates/ruvllm/src/sona/integration.rs` (extend)
  - [ ] Tier 2: Quantization scale adaptation
  - [ ] Quality signal for dynamic precision

### 6.2 Evaluation Harness Extension

- [ ] **File**: `crates/ruvllm/src/evaluation/real_harness.rs` (extend)
  - [ ] Add quantized model evaluation
  - [ ] GSM8K, HumanEval, WikiText-2 benchmarks
  - [ ] Tool use evaluation

### 6.3 Criterion Benchmarks

- [ ] **File**: `crates/ruvllm/benches/pi_quant_bench.rs`
  - [ ] `bench_pi_quantize` (target: >1 GB/s)
  - [ ] `bench_pi_dequantize_simd` (target: >10 GB/s NEON, >2 GB/s WASM)
  - [ ] `bench_hadamard_transform` (target: <50 μs for 4096-dim)
  - [ ] `bench_qat_forward_backward` (target: <500 ms/step)

### 6.4 Integration Tests

- [ ] **File**: `crates/ruvllm/tests/qat_integration.rs`
  - [ ] Full QAT pipeline test (calibrate → train → export)
  - [ ] Quantized model inference test
  - [ ] WASM export test

---

## Acceptance Gates Verification

| Gate | Phase | Test Command |
|------|-------|--------------|
| G1 | 1→2 | `cargo test -p ruvllm gate_piq3_quality` |
| G2 | 2→3 | `cargo test -p ruvllm gate_lora_qat_convergence` |
| G3 | 3→4 | `cargo test -p ruvllm gate_piq2_viability` |
| G4 | Any | `cargo bench -p ruvllm -- --baseline main` |
| G5 | Pre-merge | `cargo clippy -p ruvllm -- -D clippy::undocumented_unsafe_blocks` |
| G6 | Pre-release | `cd crates/ruvllm-wasm && wasm-pack build --target web` |

---

## Quality Metrics Collection

### Phase 1 Metrics (PiQ3 PTQ)

```bash
# Run quality comparison
cargo test -p ruvllm --release pi_quant_quality -- --nocapture

# Expected output:
# MSE (PiQ3 vs Q3): 0.042 vs 0.051 (✓ PiQ3 better)
# Spectral (PiQ3 vs Q3): -18.2 dB vs -17.5 dB (✓ PiQ3 better)
# Cosine (PiQ3 vs Q3): 0.9981 vs 0.9975 (✓ PiQ3 better)
# Outlier Retention: 87% vs 68% (✓ PiQ3 better)
```

### Phase 2 Metrics (LoRA-QAT)

```bash
# Run reasoning evaluation
cargo run -p ruvllm --release -- evaluate \
  --model quantized-0.5b-piq3-qat.gguf \
  --benchmarks wikitext2,gsm8k,humaneval,tool_use,long_context

# Expected output:
# WikiText-2 PPL: 13.2 (+7% from FP16 12.3) ✓
# GSM8K: 40% (-5pt from 45%) ✓
# HumanEval: 25% (-3pt from 28%) ✓
# Tool Use: 89% (-3pt from 92%) ✓
# Long Context 8K: 88% (-4pt from 92%) ✓
```

---

## Rollback Triggers

| Trigger | Detection | Response |
|---------|-----------|----------|
| PiQ3 ≤ Uniform | G1 fails (0-1 metrics better) | Investigate step size; if fails, de-scope to research |
| LoRA-QAT OOM | Training crashes | Reduce rank 16→8→4; if OOM persists, defer to GPU cluster |
| Reasoning collapse | >25 point GSM8K drop | Increase λ_KD; if fails, revert to PTQ-only |
| SIMD divergence | >1 ULP vs scalar | Fix kernel; block merge |
| Benchmark regression | >5% slower than baseline | Bisect and fix; block merge |

---

## File Summary

### New Files (16 files)

```
crates/ruvllm/src/quantize/
  pi_quant.rs              # Pi-constant quantization core
  pi_quant_simd.rs         # NEON/AVX2 kernels
  pi_quant_wasm_simd.rs    # WASM SIMD128 kernels
  incoherence.rs           # Hadamard rotation transforms
  hadamard.rs              # Fast Walsh-Hadamard O(n log n)
  quip.rs                  # QuIP-enhanced 2-bit
  security.rs              # Weight integrity validation

crates/ruvllm/src/qat/
  mod.rs                   # Public API
  config.rs                # QatConfig, SteVariant
  ste.rs                   # Straight-through estimator
  differentiable_quant.rs  # DifferentiableQuantizer trait
  calibration.rs           # Mixed-domain calibration
  distillation.rs          # Teacher-student loss
  reasoning_loss.rs        # Chain-of-thought fidelity
  training_loop.rs         # Main QAT orchestrator
  lora_qat.rs              # LoRA-QAT lightweight variant

crates/ruvllm-wasm/src/
  pi_quant_wasm.rs         # Pi-quantization WASM bindings
  quant_bench_wasm.rs      # In-browser benchmarks

crates/ruvllm/benches/
  pi_quant_bench.rs        # Criterion benchmarks
```

### Extended Files (12 files)

```
crates/ruvllm/src/quantize/ruvltra_quant.rs  # TargetFormat enum
crates/ruvllm/src/gguf/quantization.rs       # GGUF type IDs
crates/ruvllm/src/training/real_trainer.rs   # QatMode
crates/ruvllm/src/lora/micro_lora.rs         # AdapterMode::Qat
crates/ruvllm/src/lora/training.rs           # LoRA-QAT gradients
crates/ruvllm/src/sona/integration.rs        # Tier 2 adaptation
crates/ruvllm/src/evaluation/real_harness.rs # Quantized eval
crates/ruvllm-wasm/src/bindings.rs           # WASM exports
crates/ruvllm-wasm/Cargo.toml                # Feature flags
```

---

## Definition of Done

- [ ] All checkboxes above completed
- [ ] Gates G1-G6 passing
- [ ] All 5 reasoning metrics within acceptable delta
- [ ] Benchmarks meet performance targets
- [ ] Documentation in `crates/ruvllm/docs/qat/`
- [ ] CHANGELOG entry added
- [ ] PR reviewed and approved
- [ ] Merged to main

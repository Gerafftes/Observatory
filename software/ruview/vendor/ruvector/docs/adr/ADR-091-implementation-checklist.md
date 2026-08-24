# ADR-091 Implementation Checklist: INT8 CNN Quantization

**ADR**: ADR-091-int8-cnn-quantization-ddd.md
**Status**: Ready for Implementation
**Target Crate**: `ruvector-cnn`

---

## Phase 1: Core Quantization Infrastructure (Weeks 1-2)

### 1.1 Quantization Parameter Module

- [ ] **File**: `crates/ruvector-cnn/src/quantize/params.rs`
  - [ ] `QuantizationParams` struct (scale, zero_point, qmin, qmax)
  - [ ] `QuantizationScheme` enum (PerTensor, PerChannel)
  - [ ] `QuantizationMode` enum (Symmetric, Asymmetric)
  - [ ] `from_minmax()` constructor
  - [ ] `from_percentile()` constructor (calibration-based)
  - [ ] Unit tests for parameter computation

### 1.2 INT8 Tensor Types

- [ ] **File**: `crates/ruvector-cnn/src/quantize/tensor.rs`
  - [ ] `QuantizedTensor<i8>` struct with metadata
  - [ ] `QuantizationMetadata` (scale, zero_point, shape)
  - [ ] `quantize()` method (f32 → i8)
  - [ ] `dequantize()` method (i8 → f32)
  - [ ] Bounds checking (INV-1, INV-2, INV-3)
  - [ ] Unit tests for round-trip accuracy

### 1.3 Calibration Engine

- [ ] **File**: `crates/ruvector-cnn/src/quantize/calibration.rs`
  - [ ] `CalibrationMethod` enum (MinMax, Percentile, MSE, Entropy)
  - [ ] `CalibrationCollector` for activation statistics
  - [ ] `CalibrationResult` with per-layer params
  - [ ] `calibrate_model()` entry point
  - [ ] Calibration artifact serialization (INV-7)
  - [ ] Unit tests for each calibration method

---

## Phase 2: INT8 Kernels (Weeks 3-4)

### 2.1 Scalar Reference Kernels

- [ ] **File**: `crates/ruvector-cnn/src/kernels/int8_scalar.rs`
  - [ ] `conv2d_int8_scalar()` - reference implementation
  - [ ] `depthwise_conv2d_int8_scalar()`
  - [ ] `matmul_int8_scalar()` for FC layers
  - [ ] `requantize_scalar()` (i32 → i8 with scale adjustment)
  - [ ] Property tests: output bounds, accumulator overflow checks

### 2.2 AVX2 Kernels

- [ ] **File**: `crates/ruvector-cnn/src/kernels/int8_avx2.rs`
  - [ ] `conv2d_int8_avx2()` using `_mm256_maddubs_epi16`
  - [ ] `depthwise_conv2d_int8_avx2()`
  - [ ] `matmul_int8_avx2()` using VNNI intrinsics (if available)
  - [ ] Kernel equivalence tests vs scalar (≤1 ULP, INV-6)

### 2.3 NEON Kernels

- [ ] **File**: `crates/ruvector-cnn/src/kernels/int8_neon.rs`
  - [ ] `conv2d_int8_neon()` using `vmlal_s8`
  - [ ] `depthwise_conv2d_int8_neon()`
  - [ ] `matmul_int8_neon()` using dot product instructions
  - [ ] Kernel equivalence tests vs scalar

### 2.4 WASM SIMD128 Kernels

- [ ] **File**: `crates/ruvector-cnn/src/kernels/int8_wasm.rs`
  - [ ] `conv2d_int8_wasm()` using `i8x16` operations
  - [ ] `depthwise_conv2d_int8_wasm()`
  - [ ] `matmul_int8_wasm()`
  - [ ] In-browser kernel equivalence tests

---

## Phase 3: Graph Rewrite Passes (Weeks 5-6)

### 3.1 BatchNorm Fusion

- [ ] **File**: `crates/ruvector-cnn/src/quantize/graph_rewrite.rs`
  - [ ] `fuse_batchnorm_to_conv()` pass (GR-1)
  - [ ] Fused weight/bias computation
  - [ ] Integration with model loader
  - [ ] Unit test: verify BN params absorbed correctly

### 3.2 Zero-Point Correction

- [ ] **File**: `crates/ruvector-cnn/src/quantize/graph_rewrite.rs` (continued)
  - [ ] `fuse_zp_to_bias()` pass (GR-2)
  - [ ] Pre-compute: `bias_q = bias - zp_input × Σweights`
  - [ ] Unit test: verify runtime subtraction eliminated

### 3.3 Quantize/Dequantize Insertion

- [ ] **File**: `crates/ruvector-cnn/src/quantize/graph_rewrite.rs` (continued)
  - [ ] `insert_qdq_nodes()` pass (GR-3)
  - [ ] `QuantizeNode` and `DequantizeNode` types
  - [ ] Boundary detection for INT8 subgraph
  - [ ] Integration test: full model quantization

### 3.4 Activation Fusion

- [ ] **File**: `crates/ruvector-cnn/src/quantize/graph_rewrite.rs` (continued)
  - [ ] `fuse_relu()` pass (GR-4)
  - [ ] `fuse_hardswish()` pass (GR-4)
  - [ ] LUT-based HardSwish (256-entry table)
  - [ ] Unit tests for activation correctness

---

## Phase 4: Quantized Layer Implementations (Weeks 7-8)

### 4.1 QuantizedConv2d

- [ ] **File**: `crates/ruvector-cnn/src/layers/quantized_conv2d.rs`
  - [ ] `QuantizedConv2d` struct
  - [ ] `forward()` method with SIMD dispatch
  - [ ] Weight packing for SIMD efficiency
  - [ ] Unit tests with known inputs/outputs

### 4.2 QuantizedDepthwiseConv2d

- [ ] **File**: `crates/ruvector-cnn/src/layers/quantized_depthwise.rs`
  - [ ] `QuantizedDepthwiseConv2d` struct
  - [ ] Separate kernel for channel-wise ops
  - [ ] Unit tests

### 4.3 QuantizedLinear

- [ ] **File**: `crates/ruvector-cnn/src/layers/quantized_linear.rs`
  - [ ] `QuantizedLinear` struct
  - [ ] GEMM-based forward pass
  - [ ] Unit tests

### 4.4 Quantized Pooling

- [ ] **File**: `crates/ruvector-cnn/src/layers/quantized_pooling.rs`
  - [ ] `QuantizedMaxPool2d` - operates in INT8 domain
  - [ ] `QuantizedAvgPool2d` - requires intermediate precision
  - [ ] Unit tests for pooling correctness

### 4.5 Quantized Residual Add

- [ ] **File**: `crates/ruvector-cnn/src/layers/quantized_residual.rs`
  - [ ] `QuantizedResidualAdd` with requantization
  - [ ] Handle mismatched scales between branches
  - [ ] Unit tests

---

## Phase 5: Model Export & Integration (Weeks 9-10)

### 5.1 Model Exporter

- [ ] **File**: `crates/ruvector-cnn/src/quantize/export.rs`
  - [ ] Export quantized model to binary format
  - [ ] Include quantization config (INV-8)
  - [ ] Checksum generation (INV-7)
  - [ ] Version stamping

### 5.2 Model Loader

- [ ] **File**: `crates/ruvector-cnn/src/quantize/loader.rs`
  - [ ] Load quantized model from binary
  - [ ] Checksum verification
  - [ ] Config validation

### 5.3 MobileNetV3 Integration

- [ ] **File**: `crates/ruvector-cnn/src/models/mobilenetv3_int8.rs`
  - [ ] `MobileNetV3Int8` model definition
  - [ ] `from_float_model()` conversion
  - [ ] End-to-end inference test

---

## Phase 6: Benchmarks & Validation (Weeks 11-12)

### 6.1 Criterion Benchmarks

- [ ] **File**: `crates/ruvector-cnn/benches/int8_bench.rs`
  - [ ] Conv2d INT8 throughput (target: 2x FP32)
  - [ ] MobileNetV3 INT8 latency (target: 2.5x speedup)
  - [ ] Memory usage comparison

### 6.2 Quality Validation

- [ ] **File**: `crates/ruvector-cnn/tests/quality_validation.rs`
  - [ ] Cosine similarity ≥0.995 vs FP32 (GATE-1)
  - [ ] Per-layer MSE tracking
  - [ ] Embedding validation on test set (GATE-2)

### 6.3 Acceptance Gate Tests

- [ ] **File**: `crates/ruvector-cnn/tests/acceptance_gates.rs`
  - [ ] GATE-1: Calibration produces valid params
  - [ ] GATE-2: Cosine similarity ≥0.995
  - [ ] GATE-3: Latency improvement ≥2.5x
  - [ ] GATE-4: Memory reduction ≥3x
  - [ ] GATE-5: Zero unsafe without assertion
  - [ ] GATE-6: WASM build succeeds
  - [ ] GATE-7: CI pipeline passes

---

## Acceptance Criteria Verification

| Gate | Test File | Command |
|------|-----------|---------|
| GATE-1 | `tests/acceptance_gates.rs` | `cargo test gate_calibration` |
| GATE-2 | `tests/quality_validation.rs` | `cargo test gate_cosine_similarity` |
| GATE-3 | `benches/int8_bench.rs` | `cargo bench --bench int8_bench -- latency` |
| GATE-4 | `tests/acceptance_gates.rs` | `cargo test gate_memory_reduction` |
| GATE-5 | CI | `cargo clippy -- -D clippy::undocumented_unsafe_blocks` |
| GATE-6 | CI | `wasm-pack build --target web` |
| GATE-7 | CI | GitHub Actions workflow |

---

## Rollback Triggers

| Trigger | Detection | Response |
|---------|-----------|----------|
| Cosine <0.99 | GATE-2 fails | Review calibration method |
| Latency <2x | GATE-3 fails | Profile kernel hot paths |
| SIMD ≠ Scalar | Kernel tests fail | Fix kernel bug, block merge |
| CI failure | GATE-7 fails | Fix or revert |

---

## Definition of Done

- [ ] All checkboxes above completed
- [ ] All 7 acceptance gates passing
- [ ] Documentation in `crates/ruvector-cnn/docs/`
- [ ] CHANGELOG entry added
- [ ] PR reviewed and approved
- [ ] Merged to main

# INT8 Quantization Design for ruvector-cnn

## Executive Summary

This document outlines the design for INT8 quantization support in `ruvector-cnn`, targeting **2-4x inference speedup** over FP32 with minimal accuracy loss (<1% top-1 degradation). The design leverages AVX2 integer SIMD instructions (`_mm256_maddubs_epi16`, `_mm256_madd_epi16`) for efficient INT8 matrix multiplication.

## 1. Quantization Scheme Selection

### 1.1 Symmetric vs Asymmetric Quantization

| Aspect | Symmetric | Asymmetric |
|--------|-----------|------------|
| **Formula** | `x_q = round(x / scale)` | `x_q = round(x / scale) + zero_point` |
| **Zero Point** | Always 0 | Computed per tensor |
| **Range Utilization** | May waste range if data is not centered | Full [-128, 127] utilization |
| **Computation** | Simpler (no zero_point in GEMM) | Requires zero_point subtraction |
| **Best For** | Weights (often centered) | Activations (often ReLU, asymmetric) |

**Recommendation: Hybrid Approach**
- **Weights**: Symmetric quantization (weights are typically centered around 0)
- **Activations**: Asymmetric quantization (ReLU outputs are non-negative)

### 1.2 Per-Tensor vs Per-Channel Quantization

| Aspect | Per-Tensor | Per-Channel |
|--------|------------|-------------|
| **Scale Factors** | 1 per tensor | 1 per output channel |
| **Accuracy** | Lower (coarse) | Higher (fine-grained) |
| **Memory** | Minimal overhead | O(out_channels) scales |
| **Compute** | Simpler dequantization | Per-channel scale application |

**Recommendation: Per-Channel for Weights, Per-Tensor for Activations**

Per-channel quantization for weights is critical for CNNs because:
1. Weight distributions vary significantly across output channels
2. Some channels have large weights, others have small weights
3. Per-tensor would clip large weights OR lose precision on small weights

### 1.3 Quantization Parameters

```rust
/// Quantization parameters for a tensor
#[derive(Debug, Clone)]
pub struct QuantParams {
    /// Scale factor(s) - per-tensor or per-channel
    pub scale: Vec<f32>,

    /// Zero point(s) - 0 for symmetric quantization
    pub zero_point: Vec<i8>,

    /// Quantization mode
    pub mode: QuantMode,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum QuantMode {
    /// Symmetric: x_q = round(x / scale)
    Symmetric,
    /// Asymmetric: x_q = round(x / scale) + zero_point
    Asymmetric,
}

/// Granularity of quantization
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum QuantGranularity {
    /// Single scale for entire tensor
    PerTensor,
    /// Scale per output channel (for Conv2d weights)
    PerChannel,
}
```

## 2. AVX2 INT8 Operations

### 2.1 Key AVX2 Intrinsics for INT8 GEMM

The core computation for INT8 convolution uses two key instructions:

#### `_mm256_maddubs_epi16`
- **Input**: Two 256-bit vectors (32 x i8 each)
- **Operation**: Multiply unsigned i8 by signed i8, pairwise add to i16
- **Output**: 16 x i16 values
- **Formula**: `result[i] = a[2i]*b[2i] + a[2i+1]*b[2i+1]` (i16)

#### `_mm256_madd_epi16`
- **Input**: Two 256-bit vectors (16 x i16 each)
- **Operation**: Multiply i16 pairs, pairwise add to i32
- **Output**: 8 x i32 values
- **Formula**: `result[i] = a[2i]*b[2i] + a[2i+1]*b[2i+1]` (i32)

### 2.2 INT8 Dot Product Implementation

```rust
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

/// AVX2 INT8 dot product
///
/// Computes: sum(a[i] * b[i]) where a is unsigned i8, b is signed i8
/// Uses _mm256_maddubs_epi16 + _mm256_madd_epi16 cascade
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
pub unsafe fn dot_product_int8_avx2(
    a: &[u8],      // Unsigned activations (after zero-point shift)
    b: &[i8],      // Signed weights
) -> i32 {
    debug_assert_eq!(a.len(), b.len());

    let len = a.len();
    let chunks = len / 32;  // Process 32 elements per iteration

    // Accumulator for partial sums (8 x i32)
    let mut acc = _mm256_setzero_si256();
    let ones = _mm256_set1_epi16(1);  // For horizontal sum in madd

    for i in 0..chunks {
        // Load 32 unsigned i8 activations
        let va = _mm256_loadu_si256(a.as_ptr().add(i * 32) as *const __m256i);

        // Load 32 signed i8 weights
        let vb = _mm256_loadu_si256(b.as_ptr().add(i * 32) as *const __m256i);

        // Multiply u8 * i8 -> i16, pairwise add: 32 products -> 16 sums
        let prod16 = _mm256_maddubs_epi16(va, vb);

        // Sum pairs of i16 to i32: 16 values -> 8 values
        let prod32 = _mm256_madd_epi16(prod16, ones);

        // Accumulate
        acc = _mm256_add_epi32(acc, prod32);
    }

    // Horizontal sum of 8 x i32
    let sum128 = _mm_add_epi32(
        _mm256_extracti128_si256(acc, 0),
        _mm256_extracti128_si256(acc, 1)
    );
    let sum64 = _mm_add_epi32(sum128, _mm_srli_si128(sum128, 8));
    let sum32 = _mm_add_epi32(sum64, _mm_srli_si128(sum64, 4));

    let mut result = _mm_cvtsi128_si32(sum32);

    // Handle remainder
    for i in (chunks * 32)..len {
        result += (a[i] as i32) * (b[i] as i32);
    }

    result
}
```

### 2.3 INT8 3x3 Convolution

```rust
/// INT8 quantized 3x3 convolution with AVX2
///
/// Processes 8 output channels simultaneously.
/// Input activations are quantized to u8 (after zero-point shift).
/// Weights are quantized to i8 (symmetric).
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
pub unsafe fn conv_3x3_int8_avx2(
    input: &[u8],           // Quantized activations (u8)
    input_zero_point: i32,  // Zero point for input
    kernel: &[i8],          // Quantized weights (i8, symmetric)
    bias_i32: &[i32],       // Pre-computed bias in int32 accumulator space
    output: &mut [i32],     // Output accumulators (will be dequantized later)
    in_h: usize,
    in_w: usize,
    in_c: usize,
    out_c: usize,
    stride: usize,
    padding: usize,
) {
    let out_h = (in_h + 2 * padding - 3) / stride + 1;
    let out_w = (in_w + 2 * padding - 3) / stride + 1;

    let out_c_chunks = out_c / 8;
    let kernel_size = 3;

    // Pre-compute zero-point correction term
    // For each output, we need to subtract: zp_a * sum(weights)
    let mut weight_sums = vec![0i32; out_c];
    for oc in 0..out_c {
        let mut sum = 0i32;
        for ic in 0..in_c {
            for kh in 0..3 {
                for kw in 0..3 {
                    let idx = (oc * in_c + ic) * 9 + kh * 3 + kw;
                    sum += kernel[idx] as i32;
                }
            }
        }
        weight_sums[oc] = sum;
    }

    for oh in 0..out_h {
        for ow in 0..out_w {
            let out_spatial_idx = oh * out_w + ow;

            // Process 8 output channels at once
            for oc_chunk in 0..out_c_chunks {
                let oc_base = oc_chunk * 8;

                // Initialize accumulators with bias and zero-point correction
                let mut acc = [0i32; 8];
                for i in 0..8 {
                    let oc = oc_base + i;
                    // Bias - zp_a * sum(weights) for this output channel
                    acc[i] = bias_i32[oc] - input_zero_point * weight_sums[oc];
                }

                // Convolve over 3x3 kernel
                for kh in 0..kernel_size {
                    for kw in 0..kernel_size {
                        let ih = (oh * stride + kh) as isize - padding as isize;
                        let iw = (ow * stride + kw) as isize - padding as isize;

                        if ih >= 0 && ih < in_h as isize &&
                           iw >= 0 && iw < in_w as isize {
                            let ih = ih as usize;
                            let iw = iw as usize;

                            // Process input channels in groups of 32 for AVX2
                            let ic_chunks = in_c / 32;

                            for ic_chunk in 0..ic_chunks {
                                let ic_base = ic_chunk * 32;

                                // Load 32 input activations
                                let input_base = (ih * in_w + iw) * in_c + ic_base;
                                let va = _mm256_loadu_si256(
                                    input.as_ptr().add(input_base) as *const __m256i
                                );

                                // For each output channel in this chunk
                                for i in 0..8 {
                                    let oc = oc_base + i;

                                    // Load 32 weights for this output channel
                                    let mut w_buf = [0i8; 32];
                                    for j in 0..32 {
                                        let k_idx = (oc * in_c + ic_base + j) * 9 +
                                                   kh * 3 + kw;
                                        w_buf[j] = kernel[k_idx];
                                    }
                                    let vw = _mm256_loadu_si256(
                                        w_buf.as_ptr() as *const __m256i
                                    );

                                    // u8 * i8 -> i16, pairwise add
                                    let prod16 = _mm256_maddubs_epi16(va, vw);

                                    // i16 -> i32, pairwise add
                                    let ones = _mm256_set1_epi16(1);
                                    let prod32 = _mm256_madd_epi16(prod16, ones);

                                    // Horizontal sum to single i32
                                    let sum = horizontal_sum_epi32(prod32);
                                    acc[i] += sum;
                                }
                            }

                            // Handle remainder input channels
                            for ic in (ic_chunks * 32)..in_c {
                                let input_idx = (ih * in_w + iw) * in_c + ic;
                                let input_val = input[input_idx] as i32;

                                for i in 0..8 {
                                    let oc = oc_base + i;
                                    let k_idx = (oc * in_c + ic) * 9 + kh * 3 + kw;
                                    let w_val = kernel[k_idx] as i32;
                                    acc[i] += input_val * w_val;
                                }
                            }
                        }
                    }
                }

                // Store accumulated results
                for i in 0..8 {
                    output[out_spatial_idx * out_c + oc_base + i] = acc[i];
                }
            }

            // Handle remainder output channels
            for oc in (out_c_chunks * 8)..out_c {
                let mut acc = bias_i32[oc] - input_zero_point * weight_sums[oc];

                for kh in 0..kernel_size {
                    for kw in 0..kernel_size {
                        let ih = (oh * stride + kh) as isize - padding as isize;
                        let iw = (ow * stride + kw) as isize - padding as isize;

                        if ih >= 0 && ih < in_h as isize &&
                           iw >= 0 && iw < in_w as isize {
                            let ih = ih as usize;
                            let iw = iw as usize;

                            for ic in 0..in_c {
                                let input_idx = (ih * in_w + iw) * in_c + ic;
                                let k_idx = (oc * in_c + ic) * 9 + kh * 3 + kw;
                                acc += (input[input_idx] as i32) *
                                       (kernel[k_idx] as i32);
                            }
                        }
                    }
                }

                output[out_spatial_idx * out_c + oc] = acc;
            }
        }
    }
}

/// Horizontal sum of 8 x i32 in __m256i
#[inline(always)]
unsafe fn horizontal_sum_epi32(v: __m256i) -> i32 {
    let sum128 = _mm_add_epi32(
        _mm256_extracti128_si256(v, 0),
        _mm256_extracti128_si256(v, 1)
    );
    let sum64 = _mm_add_epi32(sum128, _mm_srli_si128(sum128, 8));
    let sum32 = _mm_add_epi32(sum64, _mm_srli_si128(sum64, 4));
    _mm_cvtsi128_si32(sum32)
}
```

## 3. Quantization-Aware Layers

### 3.1 Quantized Conv2d

```rust
/// Quantized 2D Convolution Layer
///
/// Stores weights in INT8 format with per-channel scales.
/// Performs computation in INT32, then dequantizes to FP32 or requantizes to INT8.
#[derive(Debug, Clone)]
pub struct QuantizedConv2d {
    /// Quantized weights: [out_c, kh, kw, in_c] in i8
    weights_q: Vec<i8>,

    /// Per-channel weight scales
    weight_scales: Vec<f32>,

    /// Bias pre-computed in i32 accumulator space
    /// bias_q[oc] = round(bias[oc] / (input_scale * weight_scale[oc]))
    bias_q: Vec<i32>,

    /// Original FP32 bias (for dequantization)
    bias_f32: Vec<f32>,

    /// Layer configuration
    in_channels: usize,
    out_channels: usize,
    kernel_size: usize,
    stride: usize,
    padding: usize,
    groups: usize,
}

impl QuantizedConv2d {
    /// Create from FP32 Conv2d with per-channel weight quantization
    pub fn from_fp32(
        conv: &Conv2d,
        input_scale: f32,
        input_zero_point: i32,
    ) -> Self {
        let out_c = conv.out_channels();
        let in_c = conv.in_channels();
        let ks = conv.kernel_size();

        // Compute per-channel weight scales
        let mut weight_scales = vec![0.0f32; out_c];
        let weights = conv.weights();

        for oc in 0..out_c {
            let mut max_abs = 0.0f32;
            for ic in 0..in_c {
                for kh in 0..ks {
                    for kw in 0..ks {
                        let idx = oc * ks * ks * in_c + kh * ks * in_c + kw * in_c + ic;
                        max_abs = max_abs.max(weights[idx].abs());
                    }
                }
            }
            // Symmetric quantization scale: [-max_abs, max_abs] -> [-127, 127]
            weight_scales[oc] = max_abs / 127.0;
        }

        // Quantize weights
        let mut weights_q = vec![0i8; weights.len()];
        for oc in 0..out_c {
            let scale = weight_scales[oc];
            if scale > 0.0 {
                for ic in 0..in_c {
                    for kh in 0..ks {
                        for kw in 0..ks {
                            let idx = oc * ks * ks * in_c + kh * ks * in_c + kw * in_c + ic;
                            let w_f32 = weights[idx];
                            let w_q = (w_f32 / scale).round().clamp(-127.0, 127.0) as i8;
                            weights_q[idx] = w_q;
                        }
                    }
                }
            }
        }

        // Pre-compute bias in i32 accumulator space
        let bias_f32 = conv.bias().map(|b| b.to_vec()).unwrap_or_else(|| vec![0.0; out_c]);
        let mut bias_q = vec![0i32; out_c];
        for oc in 0..out_c {
            // bias_q = bias / (input_scale * weight_scale)
            let combined_scale = input_scale * weight_scales[oc];
            if combined_scale > 0.0 {
                bias_q[oc] = (bias_f32[oc] / combined_scale).round() as i32;
            }
        }

        Self {
            weights_q,
            weight_scales,
            bias_q,
            bias_f32,
            in_channels: in_c,
            out_channels: out_c,
            kernel_size: ks,
            stride: conv.stride(),
            padding: conv.padding(),
            groups: conv.groups(),
        }
    }

    /// Forward pass with INT8 computation
    ///
    /// Input: quantized u8 tensor + scale/zero_point
    /// Output: FP32 tensor (dequantized) or INT8 (requantized)
    pub fn forward_int8(
        &self,
        input: &QuantizedTensor<u8>,
    ) -> QuantizedTensor<i32> {
        let shape = input.shape();
        let batch = shape[0];
        let in_h = shape[1];
        let in_w = shape[2];

        let out_h = (in_h + 2 * self.padding - self.kernel_size) / self.stride + 1;
        let out_w = (in_w + 2 * self.padding - self.kernel_size) / self.stride + 1;

        let mut output = vec![0i32; batch * out_h * out_w * self.out_channels];

        for b in 0..batch {
            let batch_in_size = in_h * in_w * self.in_channels;
            let batch_out_size = out_h * out_w * self.out_channels;

            let input_slice = &input.data()[b * batch_in_size..(b + 1) * batch_in_size];
            let output_slice = &mut output[b * batch_out_size..(b + 1) * batch_out_size];

            unsafe {
                conv_3x3_int8_avx2(
                    input_slice,
                    input.zero_point() as i32,
                    &self.weights_q,
                    &self.bias_q,
                    output_slice,
                    in_h, in_w, self.in_channels, self.out_channels,
                    self.stride, self.padding,
                );
            }
        }

        // Compute output scales (per-channel)
        let output_scales: Vec<f32> = self.weight_scales.iter()
            .map(|ws| input.scale() * ws)
            .collect();

        QuantizedTensor::from_i32(
            output,
            vec![batch, out_h, out_w, self.out_channels],
            output_scales,
        )
    }

    /// Dequantize i32 accumulator to f32
    pub fn dequantize(
        &self,
        acc: &QuantizedTensor<i32>,
    ) -> Tensor {
        let data = acc.data();
        let scales = acc.scales();
        let out_c = self.out_channels;

        let mut output = vec![0.0f32; data.len()];

        for (i, &val) in data.iter().enumerate() {
            let oc = i % out_c;
            output[i] = val as f32 * scales[oc];
        }

        Tensor::from_data(output, acc.shape()).unwrap()
    }
}

/// Quantized tensor with metadata
#[derive(Debug, Clone)]
pub struct QuantizedTensor<T> {
    data: Vec<T>,
    shape: Vec<usize>,
    scale: f32,           // For per-tensor (activations)
    scales: Vec<f32>,     // For per-channel (weights/accumulators)
    zero_point: T,
}

impl QuantizedTensor<u8> {
    /// Quantize FP32 tensor to u8 (asymmetric)
    pub fn from_f32(tensor: &Tensor) -> Self {
        let data = tensor.data();

        // Find min/max
        let (min_val, max_val) = data.iter()
            .fold((f32::MAX, f32::MIN), |(min, max), &v| {
                (min.min(v), max.max(v))
            });

        // Compute scale and zero_point
        let scale = (max_val - min_val) / 255.0;
        let zero_point = if scale > 0.0 {
            ((-min_val / scale).round().clamp(0.0, 255.0)) as u8
        } else {
            0u8
        };

        // Quantize
        let quantized: Vec<u8> = data.iter()
            .map(|&v| {
                if scale > 0.0 {
                    ((v / scale).round() + zero_point as f32).clamp(0.0, 255.0) as u8
                } else {
                    zero_point
                }
            })
            .collect();

        Self {
            data: quantized,
            shape: tensor.shape().to_vec(),
            scale,
            scales: vec![scale],
            zero_point,
        }
    }

    pub fn data(&self) -> &[u8] { &self.data }
    pub fn shape(&self) -> &[usize] { &self.shape }
    pub fn scale(&self) -> f32 { self.scale }
    pub fn zero_point(&self) -> u8 { self.zero_point }
}
```

### 3.2 Quantized BatchNorm (Fused with Conv)

For inference, BatchNorm is fused into the preceding Conv2d layer:

```rust
/// Fuse BatchNorm into Conv2d weights and bias
///
/// Conv: y = W * x + b
/// BN:   y' = gamma * (y - mean) / sqrt(var + eps) + beta
///
/// Fused: y' = (gamma / sqrt(var + eps)) * (W * x + b - mean) + beta
///           = W' * x + b'
///
/// Where: W' = W * (gamma / sqrt(var + eps))
///        b' = (b - mean) * (gamma / sqrt(var + eps)) + beta
pub fn fuse_conv_bn(
    conv: &Conv2d,
    bn: &BatchNorm,
) -> Conv2d {
    let out_c = conv.out_channels();
    let in_c = conv.in_channels();
    let ks = conv.kernel_size();

    let gamma = bn.gamma();
    let beta = bn.beta();
    let mean = bn.running_mean();
    let var = bn.running_var();
    let eps = 1e-5f32;  // BatchNorm epsilon

    // Compute scale factors: gamma / sqrt(var + eps)
    let bn_scales: Vec<f32> = (0..out_c)
        .map(|c| gamma[c] / (var[c] + eps).sqrt())
        .collect();

    // Fuse weights
    let weights = conv.weights();
    let mut fused_weights = vec![0.0f32; weights.len()];

    for oc in 0..out_c {
        for ic in 0..in_c {
            for kh in 0..ks {
                for kw in 0..ks {
                    let idx = oc * ks * ks * in_c + kh * ks * in_c + kw * in_c + ic;
                    fused_weights[idx] = weights[idx] * bn_scales[oc];
                }
            }
        }
    }

    // Fuse bias
    let orig_bias = conv.bias().map(|b| b.to_vec()).unwrap_or_else(|| vec![0.0; out_c]);
    let fused_bias: Vec<f32> = (0..out_c)
        .map(|c| (orig_bias[c] - mean[c]) * bn_scales[c] + beta[c])
        .collect();

    let mut fused_conv = Conv2d::new(in_c, out_c, ks, conv.stride(), conv.padding());
    fused_conv.set_weights(fused_weights).unwrap();
    fused_conv.set_bias(fused_bias).unwrap();

    fused_conv
}
```

### 3.3 Quantized Activation (ReLU, ReLU6)

For activations after INT8 convolution, we implement quantized versions:

```rust
/// Quantized ReLU: max(x, zero_point)
///
/// Since zero in quantized space is `zero_point`, ReLU clamps to that value.
pub fn relu_int8(input: &[u8], output: &mut [u8], zero_point: u8) {
    for (out, &inp) in output.iter_mut().zip(input.iter()) {
        *out = inp.max(zero_point);
    }
}

/// Quantized ReLU (AVX2)
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
pub unsafe fn relu_int8_avx2(input: &[u8], output: &mut [u8], zero_point: u8) {
    let zp_vec = _mm256_set1_epi8(zero_point as i8);
    let chunks = input.len() / 32;

    for i in 0..chunks {
        let v = _mm256_loadu_si256(input.as_ptr().add(i * 32) as *const __m256i);
        let result = _mm256_max_epu8(v, zp_vec);
        _mm256_storeu_si256(output.as_mut_ptr().add(i * 32) as *mut __m256i, result);
    }

    for i in (chunks * 32)..input.len() {
        output[i] = input[i].max(zero_point);
    }
}

/// Quantized ReLU6
///
/// Clamps to [zero_point, zero_point + 6/scale]
pub fn relu6_int8(
    input: &[u8],
    output: &mut [u8],
    zero_point: u8,
    scale: f32,
) {
    let six_q = ((6.0 / scale) + zero_point as f32).round().clamp(0.0, 255.0) as u8;

    for (out, &inp) in output.iter_mut().zip(input.iter()) {
        *out = inp.max(zero_point).min(six_q);
    }
}
```

## 4. Calibration Process

### 4.1 Overview

Calibration determines the optimal quantization parameters (scale, zero_point) by analyzing the activation distributions on a representative dataset.

```rust
/// Calibration statistics collector
#[derive(Debug, Clone)]
pub struct CalibrationStats {
    /// Min values observed per channel
    min_vals: Vec<f32>,
    /// Max values observed per channel
    max_vals: Vec<f32>,
    /// Number of samples processed
    num_samples: usize,
    /// Running histogram for percentile-based calibration (optional)
    histograms: Option<Vec<Histogram>>,
}

/// Calibration method
#[derive(Debug, Clone, Copy)]
pub enum CalibrationMethod {
    /// Use min/max observed values
    MinMax,
    /// Use percentiles to exclude outliers
    Percentile { lower: f32, upper: f32 },
    /// Minimize KL divergence between FP32 and INT8 distributions
    Entropy,
    /// Mean Squared Error minimization
    MSE,
}

impl CalibrationStats {
    pub fn new(channels: usize, use_histograms: bool) -> Self {
        Self {
            min_vals: vec![f32::MAX; channels],
            max_vals: vec![f32::MIN; channels],
            num_samples: 0,
            histograms: if use_histograms {
                Some((0..channels).map(|_| Histogram::new(2048)).collect())
            } else {
                None
            },
        }
    }

    /// Update statistics with a batch of activations
    pub fn update(&mut self, activations: &Tensor) {
        let data = activations.data();
        let shape = activations.shape();
        let channels = shape[shape.len() - 1];
        let spatial = data.len() / channels;

        for s in 0..spatial {
            for c in 0..channels {
                let val = data[s * channels + c];
                self.min_vals[c] = self.min_vals[c].min(val);
                self.max_vals[c] = self.max_vals[c].max(val);

                if let Some(ref mut hists) = self.histograms {
                    hists[c].add(val);
                }
            }
        }

        self.num_samples += 1;
    }

    /// Compute quantization parameters
    pub fn compute_params(
        &self,
        method: CalibrationMethod,
        mode: QuantMode,
    ) -> QuantParams {
        match method {
            CalibrationMethod::MinMax => {
                self.compute_minmax_params(mode)
            }
            CalibrationMethod::Percentile { lower, upper } => {
                self.compute_percentile_params(lower, upper, mode)
            }
            CalibrationMethod::Entropy => {
                self.compute_entropy_params(mode)
            }
            CalibrationMethod::MSE => {
                self.compute_mse_params(mode)
            }
        }
    }

    fn compute_minmax_params(&self, mode: QuantMode) -> QuantParams {
        let channels = self.min_vals.len();
        let mut scales = vec![0.0f32; channels];
        let mut zero_points = vec![0i8; channels];

        for c in 0..channels {
            let min_val = self.min_vals[c];
            let max_val = self.max_vals[c];

            match mode {
                QuantMode::Symmetric => {
                    let max_abs = min_val.abs().max(max_val.abs());
                    scales[c] = max_abs / 127.0;
                    zero_points[c] = 0;
                }
                QuantMode::Asymmetric => {
                    scales[c] = (max_val - min_val) / 255.0;
                    zero_points[c] = if scales[c] > 0.0 {
                        ((-min_val / scales[c]).round().clamp(-128.0, 127.0)) as i8
                    } else {
                        0
                    };
                }
            }
        }

        QuantParams {
            scale: scales,
            zero_point: zero_points,
            mode,
        }
    }

    fn compute_percentile_params(
        &self,
        lower: f32,
        upper: f32,
        mode: QuantMode,
    ) -> QuantParams {
        let histograms = self.histograms.as_ref()
            .expect("Percentile calibration requires histograms");

        let channels = histograms.len();
        let mut scales = vec![0.0f32; channels];
        let mut zero_points = vec![0i8; channels];

        for c in 0..channels {
            let min_val = histograms[c].percentile(lower);
            let max_val = histograms[c].percentile(upper);

            match mode {
                QuantMode::Symmetric => {
                    let max_abs = min_val.abs().max(max_val.abs());
                    scales[c] = max_abs / 127.0;
                    zero_points[c] = 0;
                }
                QuantMode::Asymmetric => {
                    scales[c] = (max_val - min_val) / 255.0;
                    zero_points[c] = if scales[c] > 0.0 {
                        ((-min_val / scales[c]).round().clamp(-128.0, 127.0)) as i8
                    } else {
                        0
                    };
                }
            }
        }

        QuantParams {
            scale: scales,
            zero_point: zero_points,
            mode,
        }
    }

    fn compute_entropy_params(&self, _mode: QuantMode) -> QuantParams {
        // KL divergence minimization (TensorRT-style)
        // Iteratively search for optimal threshold that minimizes
        // KL(fp32_distribution || quantized_distribution)
        todo!("Implement entropy calibration")
    }

    fn compute_mse_params(&self, _mode: QuantMode) -> QuantParams {
        // Grid search for scale that minimizes MSE
        // MSE = E[(x - dequantize(quantize(x)))^2]
        todo!("Implement MSE calibration")
    }
}

/// Simple histogram for percentile computation
#[derive(Debug, Clone)]
pub struct Histogram {
    bins: Vec<usize>,
    min_val: f32,
    max_val: f32,
    num_bins: usize,
    count: usize,
}

impl Histogram {
    pub fn new(num_bins: usize) -> Self {
        Self {
            bins: vec![0; num_bins],
            min_val: f32::MAX,
            max_val: f32::MIN,
            num_bins,
            count: 0,
        }
    }

    pub fn add(&mut self, val: f32) {
        // Update range on first few samples, then use fixed bins
        if self.count < 1000 {
            self.min_val = self.min_val.min(val);
            self.max_val = self.max_val.max(val);
        }

        if self.max_val > self.min_val {
            let normalized = (val - self.min_val) / (self.max_val - self.min_val);
            let bin = ((normalized * self.num_bins as f32) as usize)
                .min(self.num_bins - 1);
            self.bins[bin] += 1;
        }

        self.count += 1;
    }

    pub fn percentile(&self, p: f32) -> f32 {
        let target = (self.count as f32 * p) as usize;
        let mut cumsum = 0;

        for (i, &count) in self.bins.iter().enumerate() {
            cumsum += count;
            if cumsum >= target {
                let bin_start = self.min_val +
                    (i as f32 / self.num_bins as f32) * (self.max_val - self.min_val);
                return bin_start;
            }
        }

        self.max_val
    }
}
```

### 4.2 Calibration Workflow

```rust
/// Calibrate a model using a representative dataset
pub fn calibrate_model(
    model: &MobileNetV3,
    calibration_data: &[Tensor],
    method: CalibrationMethod,
) -> QuantizedModel {
    // Collect statistics for each layer
    let mut layer_stats: HashMap<String, CalibrationStats> = HashMap::new();

    // Forward pass through calibration data
    for input in calibration_data {
        let activations = model.forward_with_intermediates(input);

        for (name, tensor) in activations {
            let channels = tensor.shape().last().copied().unwrap_or(1);
            let stats = layer_stats.entry(name.clone())
                .or_insert_with(|| CalibrationStats::new(
                    channels,
                    matches!(method, CalibrationMethod::Percentile { .. })
                ));
            stats.update(&tensor);
        }
    }

    // Compute quantization parameters for each layer
    let mut quant_params: HashMap<String, QuantParams> = HashMap::new();

    for (name, stats) in &layer_stats {
        let mode = if name.contains("activation") || name.contains("relu") {
            QuantMode::Asymmetric  // Activations after ReLU are non-negative
        } else {
            QuantMode::Symmetric   // Weights are typically centered
        };

        quant_params.insert(name.clone(), stats.compute_params(method, mode));
    }

    // Create quantized model
    QuantizedModel::from_fp32(model, quant_params)
}
```

## 5. Performance Expectations

### 5.1 Theoretical Speedup

| Operation | FP32 (AVX2) | INT8 (AVX2) | Speedup |
|-----------|-------------|-------------|---------|
| **Throughput** | 8 floats/cycle | 32 bytes/cycle | 4x |
| **Memory BW** | 4 bytes/value | 1 byte/value | 4x |
| **Cache Efficiency** | 4x larger footprint | 1x footprint | 4x better |

### 5.2 Practical Speedup (Expected)

| Layer Type | Expected Speedup | Notes |
|------------|------------------|-------|
| **Conv2d (3x3)** | 2-3x | Compute-bound, good SIMD utilization |
| **Conv2d (1x1)** | 3-4x | Memory-bound, benefits most from INT8 |
| **Depthwise Conv** | 2-2.5x | Lower arithmetic intensity |
| **BatchNorm** | Fused (free) | Fused into preceding conv |
| **Linear** | 2.5-3.5x | Matrix multiplication |
| **Overall MobileNet** | **2-4x** | Depends on layer mix |

### 5.3 Accuracy Impact

| Quantization Config | Expected Accuracy Drop |
|---------------------|------------------------|
| Weights: per-tensor symmetric | 2-5% |
| Weights: per-channel symmetric | 0.5-1% |
| Activations: per-tensor asymmetric | 0.3-0.5% |
| With calibration (percentile) | <0.5% |
| With calibration (entropy) | <0.3% |

## 6. Implementation Plan

### Phase 1: Core Infrastructure (Week 1-2)
1. Define `QuantParams`, `QuantizedTensor<T>` types
2. Implement basic quantize/dequantize functions
3. Add INT8 dot product with AVX2

### Phase 2: Quantized Layers (Week 3-4)
1. Implement `QuantizedConv2d` with per-channel weights
2. Fuse BatchNorm into Conv2d
3. Implement quantized activations (ReLU, ReLU6)

### Phase 3: Calibration (Week 5-6)
1. Implement `CalibrationStats` and histogram collection
2. Add MinMax and Percentile calibration methods
3. Create calibration workflow for full model

### Phase 4: Integration & Testing (Week 7-8)
1. Create `QuantizedMobileNetV3` wrapper
2. Add comprehensive accuracy tests
3. Benchmark against FP32 baseline
4. Document API and usage

## 7. API Design

```rust
// Example usage
use ruvector_cnn::{
    MobileNetV3, MobileNetConfig,
    quantization::{
        QuantizedModel, CalibrationMethod, QuantConfig,
        calibrate_model, quantize_model,
    },
};

// Load FP32 model
let model = MobileNetV3::new(MobileNetConfig::small())?;

// Prepare calibration data (100-1000 representative samples)
let calibration_data: Vec<Tensor> = load_calibration_images("./calib/")?;

// Calibrate with percentile method (recommended)
let quant_config = QuantConfig {
    weight_mode: QuantMode::Symmetric,
    weight_granularity: QuantGranularity::PerChannel,
    activation_mode: QuantMode::Asymmetric,
    activation_granularity: QuantGranularity::PerTensor,
    calibration_method: CalibrationMethod::Percentile {
        lower: 0.001,
        upper: 0.999,
    },
};

let quantized_model = quantize_model(&model, &calibration_data, quant_config)?;

// Inference with INT8 (2-4x faster)
let embedding = quantized_model.embed(&image)?;

// Save/load quantized model
quantized_model.save("model_int8.bin")?;
let loaded = QuantizedModel::load("model_int8.bin")?;
```

## 8. References

1. "Quantization and Training of Neural Networks for Efficient Integer-Arithmetic-Only Inference" - Google, 2018
2. "A Survey of Quantization Methods for Efficient Neural Network Inference" - Wu et al., 2021
3. Intel Intrinsics Guide: https://www.intel.com/content/www/us/en/docs/intrinsics-guide/
4. TensorRT Quantization: https://docs.nvidia.com/deeplearning/tensorrt/developer-guide/
5. PyTorch Quantization: https://pytorch.org/docs/stable/quantization.html

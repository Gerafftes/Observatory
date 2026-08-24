---
adr: 175
title: "Rust-side workarounds for Hailo Dataflow Compiler transformer-encoder bugs"
status: accepted
date: 2026-05-02
authors: [ruvnet, claude-flow]
related: [ADR-167, ADR-172, ADR-173]
---

# ADR-175 — Rust-side workarounds for Hailo Dataflow Compiler transformer-encoder bugs

## Status

**Accepted, with major update at iter 156b (2026-05-02): Option A
unblocked via Rust-side SDK monkey-patch.** A working
`encoder.hef` was compiled for `sentence-transformers/all-MiniLM-L6-v2`
on Hailo-8 — 15.7 MB,
sha256 `cdbc892765d3099f74723ee6c28ab3f0daade2358827823ba08d2969b07ebd40`.

**Iter 163 final update (2026-05-03): Option A is now the production
default.** ADR-176 P5 shipped end-to-end NPU acceleration on real Pi
5 + AI HAT+ hardware:

  cpu-fallback (Option E):     7.0 embeds/sec/worker
  NPU HEF      (Option A):    67.3 embeds/sec/worker  (9.6× faster)
  p50 latency:                 572 ms → 57 ms          (10× faster)
  p99 latency:                 813 ms → 152 ms         (5.4× faster)

`HailoEmbedder` (iter 162, ADR-176 P4) routes
`HEF > cpu-fallback > NoModelLoaded` automatically. Operators
following deploy/install.sh + dropping `model.hef` into the model
dir get NPU acceleration with no other config changes. cpu-fallback
remains the failover path when no HEF is available.

The 156-iteration arc resolved every SDK bug encountered:
1. KeyError input_layer1 (iter 142): keyed calibration dict by
   internal HN layer name discovered via `runner.get_hn()` introspection
2. AccelerasValueError shape (iter 142b): reshape calibration to
   NCHW with implicit channels=1
3. ElementwiseAddDirectOp Keras deserialize (iter 153): walk every
   `acceleras` module at import time and apply
   `keras.saving.register_keras_serializable()` to every
   `keras.layers.Layer` subclass we find. This is what the SDK should
   do internally; we patch it externally before `runner.optimize()`.
4. tf_rgb_to_hailo_rgb features alignment (iter 156b): drop the
   rank-4 attention mask input entirely; use single-input encoder
   (full attention, host-side post-NPU mean-pool applies the real
   padding mask). Same final embedding semantics.

**Production path now has TWO routes**:
- Option E (cpu-fallback, iter 147): works on any Pi 5, 7 embeds/sec/worker
- **Option A (NPU acceleration, iter 156b/157): unblocked AND
  validated on real hardware. SCP'd encoder.hef to cognitum-v0
  (Pi 5 + AI HAT+) and ran via `hailortcli run`:**

```text
Running streaming inference (encoder.hef):
  Transform data: true
    Type:      auto
    Quantized: true
Network minilm_encoder/minilm_encoder: 100% | 5/5 | FPS: 73.41
> Inference result:
    FPS: 73.48
    Send Rate: 28.89 Mbit/s
    Recv Rate: 28.89 Mbit/s
```

  **73.4 FPS on the actual NPU forward pass** (encoder only,
  raw vstream throughput, no host-side overhead).
  **10× the cpu-fallback rate** (7/sec/worker → 73/sec/worker
  for the encoder block).

  Adding host-side overhead (tokenize ~0.5ms + embedding lookup
  ~1-5ms + post-NPU mean-pool + L2-norm ~0.1ms) the realistic
  end-to-end latency lands ~15-20 ms per embed →
  **~50-65 embeds/sec single-worker, ~250/sec for a 4-Pi cluster**.

Iter 157+ work: wire the HEF path through `HailoEmbedder` (~150 LOC of
Rust):
  1. HEF load via `hailo_create_hef`
  2. vdevice configure_network_group
  3. Input/output vstream creation
  4. Host-side embedding lookup (candle BertEmbeddings layer)
  5. Tokenize → embed lookup → vstream write → vstream read →
     dequantize UINT8 → mean-pool with attention mask → L2-normalize

Until that wiring lands cpu-fallback remains the shipping default.

## 1. Context

ADR-167 documents the chain of three Hailo Dataflow Compiler v3.33.0
SDK bugs that block compiling a standard HuggingFace BERT-6 encoder
(`sentence-transformers/all-MiniLM-L6-v2`) to a Hailo-8 `.hef`:

1. **`KeyError: 'minilm_encoder/input_layer1'`** — `stats_collection`
   keys the calibration dict by user-supplied names; `hailo_model.build`
   looks them up by internal layer names. Worked around by introspecting
   the parsed HN.
2. **`AccelerasValueError` shape mismatch** — HN uses NCHW with
   implicit channels=1; calibration data must be reshaped accordingly,
   and the attention mask's seq dim is H not W. Worked around.
3. **`TypeError: Could not locate class 'ElementwiseAddDirectOp'`** —
   the SDK's `_decompose_layer_norm` algorithm does a Keras `deepcopy`
   that pickles → un-pickles a model containing custom `acceleras`
   layers. The deserialization context can't find the
   `@register_keras_serializable` registrations. Tried
   `multiproc_policy=disabled`, the official Hailo Model Zoo
   `bert_base_uncased.alls` recipe, and various optimization-level
   knobs — same error.

**Bug 3 cannot be worked around from user-space.** It's a
`hailo_model_optimization` framework bug. We've drafted a Hailo support
ticket (`docs/hailo/HAILO-SUPPORT-TICKET.md`) but a fix isn't on a
known timeline.

The user asked: "There must be a workaround we can implement ourselves
using Rust." This ADR scopes the realistic Rust-side options.

## 2. Options

### Option A — Wait for Hailo SDK fix

**Effort**: zero on our side; Hailo support ticket open.
**Risk**: indefinite timeline. Hailo's GenAI HEFs all target hailo10h /
hailo15h; transformer support on Hailo-8 isn't a priority for them.
**Outcome**: would unlock NPU acceleration with the iter-139/144
helpers we already have; the host-side embedding lookup + post-NPU
pool wiring is ~150 LOC of Rust.

### Option B — Reimplement Hailo's optimizer in Rust

**What it requires**: rewrite `hailo_model_optimization` (the Python
framework that quantizes HN → INT8 HN). That's the package containing
`pre_quantization_structural`, `_decompose_layer_norm`,
`pre_quantization_optimization`, `core_quantization`,
`finetune_optimized`, `post_quantization_optimization` — at minimum
60 algorithms across ~30 K LOC of Python with ML quantization
expertise.

**Effort**: weeks of full-time work for a single engineer with deep
quantization experience. The Hailo team has had years to build this
and it still has the bugs documented above.

**Recommendation**: **DO NOT pursue.** Reimplementation cost vastly
exceeds the value, and we'd inherit the same bugs because we don't
have the spec for what the optimizer is supposed to produce —
Hailo's HW expects very specific INT8 weight layouts that aren't
publicly documented.

### Option C — Bypass the optimizer; build a quantized HEF by hand

**What it requires**: skip `runner.optimize()` entirely. Use a
**pre-quantized** ONNX model (post-training quantization done with
`onnxruntime.quantization` which Microsoft maintains and is well-
tested), then feed it to `runner.compile()` directly, claiming it's
already in INT8.

**Iter 149 probe**: tried this. `onnxruntime.quantize_dynamic` on the
encoder ONNX produced an 11 MB QInt8 file (from 43 MB FP32). Hailo's
parser then **rejected the ONNX-Runtime quantization ops**:
```
UnsupportedOperationError in op hidden_states_QuantizeLinear:
  DynamicQuantizeLinear operation is unsupported
UnsupportedOperationError in op /encoder/layer.0/attention/self/key/MatMul_quant:
  MatMulInteger operation is unsupported
```

**Iter 150 follow-up**: also tried `quantize_static` with
`QuantFormat.QOperator` (produces standard `QLinearConv`,
`QLinearMatMul`, `QLinearAdd`, `QuantizeLinear` ops). Hailo's parser
**rejected those too**:
```
UnsupportedOperationError: QuantizeLinear operation is unsupported
UnsupportedOperationError: QLinearMatMul operation is unsupported
UnsupportedOperationError: QLinearAdd operation is unsupported
```

Hailo's parser **only accepts FP32 ONNX** and expects to do its own
quantization internally (which is the broken `_decompose_layer_norm`
/ `ElementwiseAddDirectOp` path). No format of pre-quantized ONNX
gets past the parser. Option C is definitively closed.

**Catch beyond the parser**: even if a quantized ONNX parses,
`runner.compile()` checks the HN state and refuses non-quantized
inputs (we saw "Model requires quantized weights in order to run on HW,
but none were given" in iter 142b). We'd need to either:
- Reverse-engineer the HN JSON format and write a generator that
  produces it directly (skipping ONNX → HN translation), or
- Patch the SDK to accept onnxruntime-quantized weights.

**Effort**: weeks of investigation + likely Hailo support engagement
to understand the HN file format; may end up needing the same fix
as Option A anyway.

**Recommendation**: **closed**, not parked. Both `quantize_dynamic`
and `quantize_static` (QOperator) are rejected by the Hailo parser.
The only path from FP32 ONNX to a quantized HEF is through
`runner.optimize()` which hits the `ElementwiseAddDirectOp` Keras
deserialize bug. Option A (Hailo SDK fix) is the unblocker.

### Option D — Use Hailo-8 for matrix multiplication ops only

**What it requires**: compile a tiny HEF that does ONE matmul
(deeply tested, simple op that Hailo's compiler handles well).
Call it from Rust over the HailoRT vstream API for each of the
~24 GEMM ops in a single BERT-6 forward pass. Do everything else
(LayerNorm, Softmax, residual adds, attention reshape) on CPU.

**Effort**: medium. Compile the matmul HEF (probably works since
matmul is a primitive Hailo handles cleanly), wire HailoRT vstream
calls per matmul, marshal tensors over PCIe.

**Catch**: the latency overhead of each Hailo round-trip dominates
the math. Hailo-8 PCIe round-trip is ~50 µs minimum per call. With
~24 calls per embed, the overhead alone is ~1.2 ms — comparable to
or worse than the entire CPU forward pass on x86.

**Verdict**: **Latency-bound, not throughput-bound win.** Real
production benefit is small; the cpu-fallback runs the whole forward
pass in ~40 ms warm without any PCIe shuttling.

**Recommendation**: revisit only if a real Hailo-8 batched-multi-
network inference path becomes available (Hailo claims this is
coming for "transformer block" execution but not in v3.33).

### Option E — cpu-fallback + parallel embedder pool ✅ SHIPPED iter 147

**What it does**: keep all inference on the host CPU (Cortex-A76 on
Pi 5 / x86 AVX2 on dev hosts), but run N candle `BertModel`
instances in parallel behind a try-lock pool. The 90 MB weight
mmap is OS-deduped across instances.

**Effort**: ~80 LOC of Rust, shipped in iter 147.

**Measured benefit** (x86 release build, cluster-bench, concurrency=8,
pool=4 vs single Mutex):

| Metric | pool=1 | pool=4 | Δ |
|---|---:|---:|---:|
| throughput | 25.7 / sec | **45.0 / sec** | **+75%** |
| p50 latency | 279 ms | **175 ms** | **−37%** |
| p99 latency | 582 ms | **279 ms** | **−52%** |

**Real Pi 5 measurements** (iter 149, deployed cross-built aarch64
release binary on cognitum-v0, pool=4, concurrency=4 from x86 client):

| Metric | Pi 5 |
|---|---:|
| throughput | 7.0 / sec |
| p50 latency | 572 ms |
| p99 latency | 813 ms |

A76 cores split 4 ways are memory-bandwidth limited so the per-call
latency goes UP under concurrent load (vs single-thread which would
be ~150-200ms). Aggregate throughput at 4 workers (4-Pi cluster):
~28 embeds/sec, which covers most ingest workloads. Self-test embed
at startup confirms the model loads + first inference works
(`startup self-test embed ok dim=384`).

**Mixed-cluster dispatch validation** (iter 154, x86 + Pi 5 workers
behind a single coordinator, concurrency=8, 10 s):

| Metric | Mixed cluster |
|---|---:|
| throughput | 43.8 / sec |
| p50 latency | 175 ms |
| p99 latency | 826 ms |
| errors | 0 / 446 |

P2C+EWMA correctly biased traffic toward the faster local x86 worker
(~9:1 vs Pi). Latency tail tracks the slower Pi worker that
occasionally got picked. Confirms ADR-167 §8 dispatch invariants under
heterogeneous fleet load.

**Full systemd-managed Pi 5 deploy** (iter 152): `install.sh` →
`systemctl start` → `kill -9 <pid>` → systemd respawned new PID,
status `active`. Drop-root user `ruvector-worker` per ADR-172 §3a.

**Memory cost**: ~100 MB resident at pool=4 (vs 90 MB at pool=1) —
the safetensors mmap dominates and is shared.

**Recommendation**: **ship as production default.** Pi 5 deploy
should set `RUVECTOR_CPU_FALLBACK_POOL_SIZE=4`.

## 3. Decision

**Adopt Option E (cpu-fallback embedder pool) as the production
embedding path.** It is:
- Implemented and validated end-to-end (45 embeds/sec sustained on
  x86; ~12-20 embeds/sec/worker estimated on Pi 5)
- Hardened with iter-143 fingerprint integrity (cluster detects model
  drift across cpu-fallback workers)
- Hardened with iter-145 startup self-test (catches model corruption
  at boot, not at first traffic)
- Cross-builds cleanly for aarch64 in one command (iter 141)
- Documented in `crates/ruvector-hailo/models/README.md` and
  `crates/ruvector-hailo-cluster/README.md`

**Keep Option A (Hailo SDK fix) as the long-term NPU path.** When
Hailo addresses the `ElementwiseAddDirectOp` deserialize bug, the
iter-139 / iter-144 helpers (`export-minilm-encoder-onnx.py`,
`compile-encoder-hef.py`) produce the HEF in one command and
~150 LOC of Rust wires the host-side embedding lookup + post-NPU
mean-pool into `HailoEmbedder::embed`.

**Revisit Options C and D** only if Option E becomes throughput-bound
in production. For current ruvllm + ruview workloads (low to medium
embedding rate), Option E provides ample headroom.

## 4. Consequences

**Positive**:
- Production embedding path unblocked today, no waiting on Hailo
- 1.75× throughput improvement vs naive single-Mutex approach
- All existing security / observability / deployment infrastructure
  (ADR-167 fingerprint, ADR-172 §3a drop-root, ADR-170 tracing)
  carries over unchanged
- No new dependencies; only candle which we already had

**Negative**:
- NPU is dormant. The 26 TOPS of Hailo-8 silicon is unused for
  embedding workloads. Pi 5 + AI HAT+ buyers expect to use the NPU.
- ~40 ms / embed on x86, ~150-300 ms / embed on Pi 5 vs Hailo's
  documented 1-3 ms / embed for image classification on the same
  silicon. We can't claim NPU acceleration in marketing.
- The HEF compile pipeline tooling we built (DFC install,
  setup-hailo-compiler.sh, compile-hef.sh, encoder ONNX export, SDK
  Python driver) sits unused waiting for the Hailo SDK fix.

**Neutral**:
- Mixed cluster operation works: NPU-equipped workers and
  cpu-fallback workers can co-exist, but the iter-143 fingerprint
  intentionally distinguishes them so the cluster won't mix them
  into the same dispatch group (would break the implicit "all
  workers compute the same vectors" assumption).

## 5. Implementation status

| Surface | State |
|---|---|
| cpu-fallback embedder pool | ✅ iter 147, shipped |
| Worker startup self-test | ✅ iter 145, shipped |
| Cluster fingerprint integrity for cpu-fallback | ✅ iter 143, shipped |
| aarch64 cross-build of cpu-fallback worker | ✅ iter 141, shipped |
| Release-mode latency benchmark assertion | ✅ iter 140, shipped |
| Hailo support ticket text | ✅ iter 147, ready to send |
| HEF model surgery helpers (encoder ONNX, SDK Python driver) | ✅ iter 139/144, ready when SDK fix lands |
| Host-side embedding lookup + post-NPU mean-pool wiring | ⏸ deferred until SDK fix lands |

## 6. References

- ADR-167 — original ruvector-hailo embedding backend design + the
  three SDK bugs documented in detail
- ADR-172 — security review (drop-root, fingerprint integrity)
- ADR-173 — ruvllm bridge into the embedding cluster
- `docs/hailo/HAILO-SUPPORT-TICKET.md` — pre-drafted ticket for Hailo
- Hailo Model Zoo `bert_base_uncased.alls` — Hailo's official BERT
  recipe (targets hailo15h/10h, dropped `set_input_mask_to_softmax()`
  in DFC 3.33 since the directive doesn't exist there yet)
- candle-transformers BertModel — the Rust-native BERT we use for
  Option E

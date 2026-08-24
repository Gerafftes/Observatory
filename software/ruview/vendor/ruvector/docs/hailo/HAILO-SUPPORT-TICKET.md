# Hailo Support Ticket — DFC 3.33.0 cannot quantize all-MiniLM-L6-v2 encoder

**Severity**: blocks production NPU acceleration of standard
HuggingFace transformer encoders on Hailo-8.

**Affected versions**: HailoRT 4.23.0 + Hailo Dataflow Compiler v3.33.0
on Ubuntu 24.04 (Python 3.10 venv with TF 2.18 + protobuf 3.20.3 +
torch 2.4.1 + transformers 4.49 — all pinned per Hailo Model Zoo's
official `requirements.txt`).

**Hardware target**: Hailo-8 on Pi 5 + AI HAT+.

**Model**: `sentence-transformers/all-MiniLM-L6-v2` (BERT-6, 22 M params).

## What we want

Compile the BERT-6 encoder to a Hailo-8 `.hef` so we can offload the
forward pass from the Cortex-A76 to the NPU. Pre-computing token
embeddings + attention mask host-side is fine — we only need the
encoder block (12 layers, ~21 M params, ~120 MB FP32).

## Repro

```bash
# 1. Install
curl -LsSf https://astral.sh/uv/install.sh | sh
bash <ruvector-clone>/crates/ruvector-hailo-cluster/deploy/setup-hailo-compiler.sh \
    ~/Downloads/hailo  # contains hailort_*.deb + dataflow_compiler*.whl
                       # (or extract them from hailo8_ai_sw_suite_2025-10.run)

# 2. Export the encoder ONNX (single torch.onnx.export, ~43 MB)
bash <ruvector-clone>/crates/ruvector-hailo-cluster/deploy/export-minilm-encoder-onnx.py \
    /tmp/encoder-onnx
# Two inputs:  hidden_states           [1, 128, 384] float
#              attention_softmax_mask  [1, 1, 1, 128] float
# One output:  last_hidden_state       [1, 128, 384] float
# Verified zero Gather / Where / Expand ops via onnx introspection.

# 3. Drive the SDK directly via Python (not the `hailo` CLI, which
#    auto-accepts a bad end-node recommendation under -y).
~/.cache/ruvector-hailo-compiler/active/bin/python \
    <ruvector-clone>/crates/ruvector-hailo-cluster/deploy/compile-encoder-hef.py \
    /tmp/encoder-onnx/encoder.onnx /tmp/encoder-onnx/encoder.hef
```

## What works

- **Parse** stage: clean. Maps both inputs cleanly:
  ```
  [info] Start nodes mapped from original model:
      'hidden_states':           'minilm_encoder/input_layer1'
      'attention_softmax_mask':  'minilm_encoder/input_layer2'
  [info] End nodes mapped from original model:
      '/encoder/layer.5/output/LayerNorm/Add_1'
  [info] Translation completed (1.50 s)
  ```
- **Full-precision optimize**: clean (86 MB optimized HAR produced).
- The compile script adopts Hailo Model Zoo's official BERT recipe
  from `cfg/alls/generic/bert_base_uncased.alls` (minus
  `set_input_mask_to_softmax()` which doesn't exist in DFC 3.33).

## Where it fails

INT8 optimize, deep inside `hailo_model_optimization`. We hit a chain
of three bugs:

### Bug 1 — KeyError on internal layer name lookup

```
File ".../hailo_model_optimization/.../stats_collection.py", line 87, in _setup
    self._model.build(self._get_build_inputs())
File ".../hailo_model_optimization/acceleras/model/hailo_model/hailo_model.py", line 1227, in build
    input_shape = [
File ".../hailo_model.py", line 1228, in <listcomp>
    input_shape[node]
KeyError: 'minilm_encoder/input_layer1'
```

**Root cause** (read from SDK source):
- `stats_collection._get_build_inputs()` returns dict keyed by the
  user-supplied calibration dataset's keys (operator-side names like
  `hidden_states`).
- `hailo_model.build()` iterates over `self.flow.input_nodes` (the
  parser-assigned internal layer names like
  `minilm_encoder/input_layer1`) and looks them up in the dict.
- The two name spaces never match → KeyError.

**Workaround**: introspect the parsed HN, key the calibration dict by
the actual `input_layer` name. Implemented in our
`compile-encoder-hef.py:48-65`. **This should be unnecessary — the
SDK should bridge the user names to internal names internally.**

### Bug 2 — AccelerasValueError on layout mismatch

After working around Bug 1:

```
AccelerasValueError: Inference input shapes [[-1, 128, 384, 384], [-1, 128, 384]]
for layer minilm_encoder/conv4 does not match HN shapes
[[-1, 1, 128, 384], [-1, 1, 128, 384]]
```

**Root cause**: HN treats inputs as 4D NCHW with implicit channels=1,
so a 3D `[batch, seq, hidden]` input must be reshaped to
`[batch, 1, seq, hidden]` for the calibration dataset. For the mask
input, `[batch, 1, 1, seq]` is wrong — the HN expects
`[batch, 1, seq, 1]` (seq dim is H, broadcast dim is W).

**Workaround**: reshape calibration tensors to NCHW with the right
axis assignments. Implemented in `compile-encoder-hef.py:75-95`.
**This should be discoverable from the `runner.translate_onnx_model`
call's `net_input_shapes` — those shapes are what we passed.**

### Bug 3 — Keras deserialize failure on SDK's own custom layer

After working around Bugs 1 + 2:

```
File ".../keras/src/saving/serialization_lib.py", line 834, in _retrieve_class_or_fn
    raise TypeError(
TypeError: Could not locate class 'ElementwiseAddDirectOp'.
Make sure custom classes and functions are decorated with
`@keras.saving.register_keras_serializable()`. If they are already
decorated, make sure they are all imported so that the decorator is
run before trying to load them.

Full object config: {
    'module': 'hailo_model_optimization.acceleras.atomic_ops.element_wise_add_op',
    'class_name': 'ElementwiseAddDirectOp',
    'config': {
        'name': 'minilm_encoder/ew_sub_softmax1/elementwise_add_op',
        ...
    },
    'registered_name': 'ElementwiseAddDirectOp'
}
```

**Root cause** (best guess from stack trace):
`hailo_model_optimization` does a Keras model `deepcopy()` at the
start of `_decompose_layer_norm`. The deepcopy serializes the model
to a pickle then loads it back. The class
`hailo_model_optimization.acceleras.atomic_ops.element_wise_add_op.ElementwiseAddDirectOp`
needs to be importable in the deserialization context. The
`@register_keras_serializable` decorator that registers it isn't
running in whatever context Keras's `_load_model_from_fileobj` looks
in.

This fires regardless of the Hailo Model Zoo BERT recipe alls
directives. We even tried
`model_optimization_config(globals, multiproc_policy=disabled)`
(cribbed from `cfg/alls/generic/tinyclip_vit_8m_16_text_3m_yfcc15m_text_encoder.alls`)
to keep the optimizer in-process — same error.

**Cannot work around from user-space.**

## Why this matters

`sentence-transformers/all-MiniLM-L6-v2` is the most-downloaded
sentence embedding model on HuggingFace (>50 M monthly downloads).
The closest thing in Hailo Model Zoo is `bert_base_uncased.alls`
which targets `hailo15h` / `hailo10h` only — there is no `hailo8`
HEF for any sentence/text encoder anywhere in the Model Zoo
(verified via `gh api` traversal of both `hailo_model_zoo` and
`hailo_model_zoo_genai`).

Pi 5 + AI HAT+ ships with a Hailo-8. Operators following the
official Raspberry Pi blog post on "AI HAT+" cannot use the NPU for
text embedding without solving these three bugs.

## What we'd love

Either:
1. Confirm DFC > 3.33 fixes Bug 3, and let us know the timeline. We're
   willing to be on a beta build.
2. Tell us the right way to register `ElementwiseAddDirectOp` so the
   deserialize succeeds — perhaps we can `import
   hailo_model_optimization.acceleras.atomic_ops.element_wise_add_op`
   eagerly somewhere?
3. Publish a `hailo8` HEF for any sentence-encoder model in the Model
   Zoo (we can adopt to that — `all-MiniLM-L6-v2` isn't load-bearing).

The encoder ONNX, the parsed HAR, the optimized HAR, and full stack
traces from each bug are all available for upload — say the word.

## Reference

- ruvector branch (full code): https://github.com/ruvnet/RuVector/tree/hailo-backend
- ADR with all the iteration history:
  https://github.com/ruvnet/RuVector/blob/hailo-backend/docs/adr/ADR-167-ruvector-hailo-npu-embedding-backend.md
- The two helpers driving the repro:
  - `crates/ruvector-hailo-cluster/deploy/export-minilm-encoder-onnx.py`
  - `crates/ruvector-hailo-cluster/deploy/compile-encoder-hef.py`

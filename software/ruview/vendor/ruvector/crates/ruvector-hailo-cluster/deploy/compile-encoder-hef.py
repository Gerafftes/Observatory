#!/usr/bin/env python3
"""Compile the encoder-only ONNX (iter 139) to Hailo-8 .hef.

Companion to compile-hef.py. Uses the encoder-only export from
export-minilm-encoder-onnx.py — no Gather/Where/Expand ops, just clean
MatMul/Softmax/Add/Mul/Reshape encoder primitives that Hailo can fuse.

If this compile succeeds, the HEF surgery in ADR-167 is unblocked.
The host-side embedding lookup + mask construction will be wired in
HailoEmbedder in a follow-up iter.

Usage: python3 compile-encoder-hef.py <encoder_onnx> <out_hef>
"""

import os
import sys
from pathlib import Path

os.environ.setdefault("TRANSFORMERS_NO_TF", "1")

# Iter 153 monkey-patch: the SDK's `ElementwiseAddDirectOp`
# (and a handful of sibling acceleras op classes) inherit from
# `keras.layers.Layer` but aren't decorated with
# `@keras.saving.register_keras_serializable()`. Inside the optimizer,
# `_decompose_layer_norm` calls `keras.deepcopy(model)` which serializes
# to JSON then deserializes — and the deserialize step looks the class
# up by name in Keras's registry. KeyError follows for any unregistered
# class. Workaround: walk every module under
# `hailo_model_optimization.acceleras` and register every Layer subclass
# we find. This is what the SDK should do internally.
import importlib
import inspect
import pkgutil
try:
    import keras
    import hailo_model_optimization.acceleras as _acceleras_pkg
    registered = 0
    for _finder, _name, _ in pkgutil.walk_packages(
        _acceleras_pkg.__path__, prefix="hailo_model_optimization.acceleras."
    ):
        try:
            _mod = importlib.import_module(_name)
        except Exception:
            continue
        for _attr_name, _attr in inspect.getmembers(_mod, inspect.isclass):
            if (
                _attr.__module__ == _name
                and issubclass(_attr, keras.layers.Layer)
                and getattr(_attr, "_keras_api_names", None) is None
            ):
                keras.saving.register_keras_serializable()(_attr)
                registered += 1
    print(f"==> registered {registered} acceleras Layer classes for Keras serialize")
except Exception as e:
    print(f"==> warn: keras-register monkey-patch failed: {type(e).__name__}: {e}")

from hailo_sdk_client import ClientRunner
import numpy as np

HW_ARCH = "hailo8"
NET_NAME = "minilm_encoder"
SEQ_LEN = 128
HIDDEN = 384


def main(onnx_path: str, out_hef: str) -> None:
    onnx_path = Path(onnx_path).resolve()
    out_hef = Path(out_hef).resolve()
    work = out_hef.parent

    print(f"==> [parse] {onnx_path}", flush=True)
    runner = ClientRunner(hw_arch=HW_ARCH)
    # Iter 156 — single-input form to avoid the iter-154 RGB conversion
    # blocker on the rank-4 mask. Encoder runs full attention; host-side
    # mean-pool applies the real attention mask post-NPU.
    runner.translate_onnx_model(
        str(onnx_path),
        net_name=NET_NAME,
        start_node_names=["hidden_states"],
        end_node_names=["last_hidden_state"],
        net_input_shapes={
            "hidden_states": [1, SEQ_LEN, HIDDEN],
        },
    )

    parsed_har = work / f"{NET_NAME}_parsed.har"
    runner.save_har(str(parsed_har))
    print(f"    parsed HAR → {parsed_har}", flush=True)

    print("==> [optimize] Hailo Model Zoo BERT recipe (iter 144)", flush=True)
    # Iter 144 — adopt Hailo's official BERT alls recipe from
    # `hailo_model_zoo/cfg/alls/generic/bert_base_uncased.alls`. The
    # `set_input_mask_to_softmax()` directive is the missing piece —
    # it tells the SDK that the second input is the additive mask for
    # softmax, which routes through the SDK's well-tested transformer
    # codepath instead of the generic optimizer that hits the
    # iter-139/142 SDK bug chain.
    # Iter 144b: drop `set_input_mask_to_softmax()` — that command was
    # added in DFC > 3.33 (verified via grep of installed SDK
    # site-packages: zero matches anywhere). Keep the rest of the BERT
    # recipe alls directives that ARE supported in 3.33: equalization,
    # disable ew_add fusing, optimization_level=0, matmul correction,
    # negative_exponent rank=0, ew_add 16-bit precision.
    # Iter 146: add `multiproc_policy=disabled` (cribbed from
    # tinyclip_vit_8m_16_text_3m_yfcc15m_text_encoder.alls). The
    # iter-142b/144 ElementwiseAddDirectOp Keras deserialize bug fires
    # inside a spawned subprocess that doesn't carry the SDK's custom
    # layer registry. Disabling multiproc keeps the optimizer in-process
    # so the @register_keras_serializable decorations stay loaded.
    # Iter 156 — single-input form. Drop iter-155 mask input_conversion
    # (no longer needed, no mask input). Keep the rest of Hailo's BERT
    # alls recipe + iter-153 multiproc disable.
    bert_alls = """\
model_optimization_config(calibration, batch_size=8, calibset_size=64)
model_optimization_config(globals, multiproc_policy=disabled)
pre_quantization_optimization(equalization, policy=enabled)
pre_quantization_optimization(ew_add_fusing, policy=disabled)
model_optimization_flavor(optimization_level=0, compression_level=0)
pre_quantization_optimization(matmul_correction, layers={matmul*}, correction_type=zp_comp_block)
model_optimization_config(negative_exponent, layers={*}, rank=0)
quantization_param({ew_add*}, precision_mode=a16_w16)
"""
    runner.load_model_script(bert_alls)

    rng = np.random.default_rng(seed=42)
    # Discover internal input layer names from the parsed HN, in
    # declaration order, so we can pair them with our calibration data.
    input_layer_names = []
    try:
        hn = runner.get_hn()
        import json as _json
        hn_d = _json.loads(hn) if isinstance(hn, str) else hn
        for lname, layer in hn_d.get("layers", {}).items():
            if layer.get("type") == "input_layer":
                input_layer_names.append(lname)
    except Exception as e:
        print(f"    warn: couldn't introspect HN ({e})", flush=True)

    print(f"    parsed input layers: {input_layer_names}", flush=True)

    # Hailo's HN treats inputs as 4D NCHW with implicit channels=1, so
    # [batch, seq, hidden] reshapes to [batch, 1, seq, hidden].
    # The mask is already 4D [batch, 1, 1, seq].
    # Iter 144c: mask is [batch, 1, seq, 1] in HN order (W=1) — Hailo
    # treats the seq dim as H and the broadcast dim as W. Verified by
    # AccelerasValueError on iter 144b: HN shape [-1, 1, 128, 1] vs
    # our incorrect [-1, 1, 1, 128].
    # Iter 156 — single-input form. Use the introspected internal name
    # so stats_collection finds the dict key (iter-142 fix).
    calib_key = input_layer_names[0] if input_layer_names else "hidden_states"
    print(f"    calibration dict key: {calib_key}", flush=True)
    calib = {
        calib_key: rng.standard_normal(
            (64, 1, SEQ_LEN, HIDDEN), dtype=np.float32
        ),
    }
    runner.optimize(calib)
    opt_har = work / f"{NET_NAME}_optimized.har"
    runner.save_har(str(opt_har))
    print(f"    optimized HAR → {opt_har}", flush=True)

    print("==> [compile] hailo8 placement + scheduling (slow — minutes)", flush=True)
    hef = runner.compile()
    out_hef.write_bytes(hef)
    size = out_hef.stat().st_size
    print(f"    {size} bytes → {out_hef}", flush=True)


if __name__ == "__main__":
    if len(sys.argv) != 3:
        print(f"usage: {sys.argv[0]} <encoder_onnx> <out_hef>", file=sys.stderr)
        sys.exit(1)
    main(sys.argv[1], sys.argv[2])

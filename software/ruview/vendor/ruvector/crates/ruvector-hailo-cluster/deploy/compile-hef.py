#!/usr/bin/env python3
"""Compile an ONNX model to a Hailo-8 .hef using the SDK Python API directly.

Replaces the iter-131 `hailo parser/optimize/compiler` CLI invocations
because the CLI's `-y` auto-accepts the parser's end-node recommendation
which (for BERT-6) wrongly suggests `/Where` (an attention-mask broadcast
node that the HN graph can't represent). The Python API lets us pin
end_node_names explicitly without the recommendation override.

Usage: python3 compile-hef.py <onnx_path> <out_hef_path>
"""

import os
import sys
from pathlib import Path

# Hailo SDK reaches for tensorflow / keras at import time. The Hailo-pinned
# venv has TF 2.18 + Keras 3.12 + protobuf 3.20.3 — this triplet works.
# Set TRANSFORMERS_NO_TF=1 in case anything in transformers gets pulled in.
os.environ.setdefault("TRANSFORMERS_NO_TF", "1")

from hailo_sdk_client import ClientRunner

HW_ARCH = "hailo8"
NET_NAME = "minilm"
END_NODES = ["last_hidden_state"]
START_NODES = ["input_ids", "attention_mask", "token_type_ids"]
SEQ_LEN = 128


def main(onnx_path: str, out_hef: str) -> None:
    onnx_path = Path(onnx_path).resolve()
    out_hef = Path(out_hef).resolve()
    work = out_hef.parent

    print(f"==> [parse] {onnx_path}", flush=True)
    runner = ClientRunner(hw_arch=HW_ARCH)
    runner.translate_onnx_model(
        str(onnx_path),
        net_name=NET_NAME,
        start_node_names=START_NODES,
        end_node_names=END_NODES,
        net_input_shapes={
            "input_ids": [1, SEQ_LEN],
            "attention_mask": [1, SEQ_LEN],
            "token_type_ids": [1, SEQ_LEN],
        },
    )

    parsed_har = work / f"{NET_NAME}_parsed.har"
    runner.save_har(str(parsed_har))
    print(f"    parsed HAR → {parsed_har}", flush=True)

    print("==> [optimize] random calibration set (FP→INT8)", flush=True)
    # Random calibration trades ~3-5% accuracy vs a real corpus. Fine
    # for the first ship; revisit with a sentence corpus once the
    # NPU/host pipeline is end-to-end stable.
    import numpy as np

    rng = np.random.default_rng(seed=42)
    calib = {
        "input_ids": rng.integers(low=100, high=10000, size=(64, SEQ_LEN), dtype=np.int32),
        "attention_mask": np.ones((64, SEQ_LEN), dtype=np.int32),
        "token_type_ids": np.zeros((64, SEQ_LEN), dtype=np.int32),
    }
    runner.optimize(calib)
    opt_har = work / f"{NET_NAME}_optimized.har"
    runner.save_har(str(opt_har))
    print(f"    optimized HAR → {opt_har}", flush=True)

    print("==> [compile] hailo8 placement + scheduling", flush=True)
    hef = runner.compile()
    out_hef.write_bytes(hef)
    size = out_hef.stat().st_size
    print(f"    {size} bytes → {out_hef}", flush=True)


if __name__ == "__main__":
    if len(sys.argv) != 3:
        print(f"usage: {sys.argv[0]} <onnx_path> <out_hef_path>", file=sys.stderr)
        sys.exit(1)
    main(sys.argv[1], sys.argv[2])

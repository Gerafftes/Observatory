#!/usr/bin/env python3
"""Export sentence-transformers/all-MiniLM-L6-v2 to ONNX (opset 14).

Companion to compile-hef.sh. Replaces the optimum-cli step that caused
TF/keras/protobuf dependency hell with a 30-line torch.onnx.export call
that only needs torch + transformers.

The resulting model.onnx has two inputs (input_ids, attention_mask) and
one output (last_hidden_state, shape [batch, seq, 384]). The Hailo
Dataflow Compiler's parser handles this BERT-6 graph natively.

Usage: python3 export-minilm-onnx.py <output_dir>
       (writes <output_dir>/model.onnx)
"""

import os
import sys
from pathlib import Path

# transformers will try to import TF/Keras at module load and fail if
# the venv has a Keras 3 / tf-keras / TF version mix that doesn't line
# up. We don't need TF — only the torch path. These env vars tell
# transformers to skip the TF backend entirely.
os.environ.setdefault("TRANSFORMERS_NO_TF", "1")
os.environ.setdefault("USE_TF", "0")
os.environ.setdefault("TRANSFORMERS_NO_FLAX", "1")

import torch
from transformers import AutoTokenizer, AutoModel

MODEL_NAME = "sentence-transformers/all-MiniLM-L6-v2"
OPSET = 14
SEQ_LEN = 128


def main(out_dir: str) -> None:
    out = Path(out_dir)
    out.mkdir(parents=True, exist_ok=True)
    onnx_path = out / "model.onnx"

    print(f"==> loading {MODEL_NAME}", flush=True)
    tok = AutoTokenizer.from_pretrained(MODEL_NAME)
    model = AutoModel.from_pretrained(MODEL_NAME).eval()

    print("==> dummy inputs (batch=1, seq=128)", flush=True)
    encoded = tok(
        "the quick brown fox jumps over the lazy dog",
        padding="max_length",
        truncation=True,
        max_length=SEQ_LEN,
        return_tensors="pt",
    )
    input_ids = encoded["input_ids"]
    attention_mask = encoded["attention_mask"]
    token_type_ids = torch.zeros_like(input_ids)

    print(f"==> torch.onnx.export → {onnx_path}", flush=True)
    torch.onnx.export(
        model,
        (input_ids, attention_mask, token_type_ids),
        str(onnx_path),
        input_names=["input_ids", "attention_mask", "token_type_ids"],
        output_names=["last_hidden_state"],
        opset_version=OPSET,
        do_constant_folding=True,
        dynamic_axes={
            "input_ids": {0: "batch"},
            "attention_mask": {0: "batch"},
            "token_type_ids": {0: "batch"},
            "last_hidden_state": {0: "batch"},
        },
    )

    size = onnx_path.stat().st_size
    print(f"    {size} bytes → {onnx_path}", flush=True)


if __name__ == "__main__":
    if len(sys.argv) != 2:
        print(f"usage: {sys.argv[0]} <output_dir>", file=sys.stderr)
        sys.exit(1)
    main(sys.argv[1])

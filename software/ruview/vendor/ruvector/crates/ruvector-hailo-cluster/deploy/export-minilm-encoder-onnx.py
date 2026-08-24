#!/usr/bin/env python3
"""Export only the BERT encoder layers of all-MiniLM-L6-v2 to ONNX.

Iter 139 follow-up to ADR-167's HEF model surgery scope. Skips:
  * `word_embeddings.Gather` (host-side embedding lookup will replace)
  * `Where`/`Expand` attention mask broadcast (host pre-computes the
    additive bias and passes it as a fully-expanded 4D tensor)

Inputs:
  hidden_states          [batch, seq, 384]   float32  — host pre-computed embeddings
  extended_attention_mask [batch, 1, 1, seq]  float32 — host pre-computed mask (0 or -10000)

Output:
  last_hidden_state      [batch, seq, 384]   float32

Iter 139: probe whether the Hailo Dataflow Compiler can fuse this
slimmed-down graph. If yes, the HEF model surgery in ADR-167 is unblocked
and we proceed to wire the host-side embedding lookup + mask construction
in HailoEmbedder. If no (Hailo still rejects the encoder's internal
ops), we know more about what surgery is actually required.
"""

import os
import sys
from pathlib import Path

os.environ.setdefault("TRANSFORMERS_NO_TF", "1")
os.environ.setdefault("USE_TF", "0")
os.environ.setdefault("TRANSFORMERS_NO_FLAX", "1")

import torch
from transformers import AutoModel

MODEL_NAME = "sentence-transformers/all-MiniLM-L6-v2"
OPSET = 14
SEQ_LEN = 128
HIDDEN = 384


class EncoderOnly(torch.nn.Module):
    """Wraps BertEncoder taking only hidden_states (no mask input).

    Iter 156 — single-input form. The dual-input form from iter 144
    fails at the allocator stage with
    `tf_rgb_to_hailo_rgb format conversion ... features not aligned to 8`
    on the rank-4 mask input (C=1, can't be aligned to 8). Hailo's
    `input_conversion` script command only accepts image-color
    conversions, not feature passthrough. Iter 153 fixed the original
    blocker (Keras deserialize on ElementwiseAddDirectOp) so the
    single-input form — which iter 144b tried before iter 153 —
    should now compile cleanly.

    Trade-off: encoder runs full attention with no padding mask. The
    worker pads input to seq=128 with [PAD] tokens, so shorter inputs
    just produce meaningful values at PAD positions; the post-NPU
    host-side mean-pool applies the real attention mask, zeroing out
    those PAD-position contributions. Same final embedding semantics."""

    def __init__(self, model):
        super().__init__()
        self.encoder = model.encoder

    def forward(self, hidden_states):
        out = self.encoder(
            hidden_states=hidden_states,
            attention_mask=None,
            return_dict=True,
        )
        return out.last_hidden_state


def main(out_dir: str) -> None:
    out = Path(out_dir)
    out.mkdir(parents=True, exist_ok=True)
    onnx_path = out / "encoder.onnx"

    print(f"==> loading {MODEL_NAME}", flush=True)
    model = AutoModel.from_pretrained(MODEL_NAME).eval()
    encoder_only = EncoderOnly(model).eval()

    print(f"==> dummy inputs (batch=1, seq={SEQ_LEN}, hidden={HIDDEN})", flush=True)
    hidden_states = torch.randn(1, SEQ_LEN, HIDDEN)

    print(f"==> torch.onnx.export → {onnx_path}", flush=True)
    torch.onnx.export(
        encoder_only,
        (hidden_states,),
        str(onnx_path),
        input_names=["hidden_states"],
        output_names=["last_hidden_state"],
        opset_version=OPSET,
        do_constant_folding=True,
        # Fixed batch=1 — Hailo HEFs are compiled with concrete shapes
        # anyway, so dynamic batching gains us nothing on the export side.
    )

    size = onnx_path.stat().st_size
    print(f"    {size} bytes → {onnx_path}", flush=True)


if __name__ == "__main__":
    if len(sys.argv) != 2:
        print(f"usage: {sys.argv[0]} <output_dir>", file=sys.stderr)
        sys.exit(1)
    main(sys.argv[1])

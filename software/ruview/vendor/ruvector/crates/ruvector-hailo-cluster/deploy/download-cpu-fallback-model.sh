#!/usr/bin/env bash
# Download the sentence-transformers/all-MiniLM-L6-v2 model artifacts
# needed by the iter-133 cpu-fallback path (ADR-167 path C).
#
# When the worker is built with `--features cpu-fallback` and the model
# directory contains the three files listed below but no model.hef, the
# cluster runs real BERT-6 inference on the host CPU instead of erroring
# with NoModelLoaded. Slow (50-150ms/embed on Pi 5 vs 1-3ms on Hailo-8)
# but produces real semantic vectors today.
#
# Once the operator has a compiled model.hef, drop it into the same dir
# and restart the worker — the existing HailoEmbedder::open path picks
# up the HEF and the CPU fallback is bypassed automatically.
#
# What this script downloads (from HuggingFace, ~100 MB total):
#   model.safetensors    (~90 MB) — BERT-6 weights
#   tokenizer.json       (~700 KB) — fast tokenizer
#   config.json          (~600 B)  — hidden_size / layers / heads
#
# No HF auth token required; the model is publicly licensed (Apache 2.0).
#
# Usage:
#   bash download-cpu-fallback-model.sh [model_dir]
#
#   model_dir defaults to /var/lib/ruvector-hailo/model
#
# Re-run idempotently — skips files that exist with the right size + sha256.

set -euo pipefail

MODEL_DIR="${1:-/var/lib/ruvector-hailo/model}"
HF_BASE="https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main"

# (filename, expected_sha256, approx_size) from the HF model card. Pin
# the hashes so a tampered mirror or a silent model update can't change
# what we ship.
declare -a FILES=(
  "model.safetensors|53aa51172d142c89d9012cce15ae4d6cc0ca6895895114379cacb4fab128d9db|90.9MB"
  "tokenizer.json|be50c3628f2bf5bb5e3a7f17b1f74611b2561a3a27eeab05e5aa30f411572037|466KB"
  "config.json|953f9c0d463486b10a6871cc2fd59f223b2c70184f49815e7efbcab5d8908b41|612B"
)

if ! command -v curl >/dev/null 2>&1; then
  echo "curl not found — install with apt/yum/pacman" >&2
  exit 1
fi
if ! command -v sha256sum >/dev/null 2>&1; then
  echo "sha256sum not found — install coreutils" >&2
  exit 1
fi

echo "==> [1/3] prepare model dir"
mkdir -p "$MODEL_DIR"
echo "    target: $MODEL_DIR"

echo "==> [2/3] fetch artifacts (skip if hash already matches)"
for entry in "${FILES[@]}"; do
  name="${entry%%|*}"
  rest="${entry#*|}"
  want_sha="${rest%%|*}"
  approx_size="${rest##*|}"
  dest="$MODEL_DIR/$name"

  if [[ -f "$dest" ]]; then
    have_sha="$(sha256sum "$dest" | awk '{print $1}')"
    if [[ "$have_sha" == "$want_sha" ]]; then
      echo "    ✓ $name already present ($approx_size, sha256 OK)"
      continue
    fi
    echo "    ! $name present but sha256 mismatch — re-downloading"
  fi

  echo "    ↓ $name ($approx_size)"
  tmp="$dest.partial"
  curl -fSL --progress-bar -o "$tmp" "$HF_BASE/$name"
  got_sha="$(sha256sum "$tmp" | awk '{print $1}')"
  if [[ "$got_sha" != "$want_sha" ]]; then
    rm -f "$tmp"
    echo "    ✗ $name sha256 mismatch after download" >&2
    echo "      expected: $want_sha" >&2
    echo "      got:      $got_sha" >&2
    echo "      not writing — re-run or check network for tampering" >&2
    exit 2
  fi
  mv -f "$tmp" "$dest"
done

echo "==> [3/3] summary"
ls -la "$MODEL_DIR" 2>&1 | grep -E "model.safetensors|tokenizer.json|config.json" || true

cat <<EOF

Downloaded the all-MiniLM-L6-v2 artifacts to $MODEL_DIR.

Next steps:
  1. Build the worker with cpu-fallback enabled:
       cargo build --release --features hailo,cpu-fallback \\
           --bin ruvector-hailo-worker \\
           --manifest-path crates/ruvector-hailo-cluster/Cargo.toml

  2. Point the worker at this dir on startup:
       export RUVECTOR_MODEL_DIR=$MODEL_DIR
       /usr/local/bin/ruvector-hailo-worker --bind 0.0.0.0:7050

  3. Confirm health probe reports ready=true even without a model.hef:
       grpcurl -plaintext localhost:7050 ruvector.hailo.v1.Worker/Health

  4. When you have a compiled model.hef (see compile-hef.sh), drop it
     into $MODEL_DIR and restart — the HEF takes priority over the
     CPU fallback. No code change required.
EOF

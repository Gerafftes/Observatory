#!/usr/bin/env bash
# Download the iter-156b compiled encoder.hef for Hailo-8.
#
# ADR-176 P5b distribution (iter 169). Companion to
# `download-cpu-fallback-model.sh` — this fetches the pre-compiled
# HEF artifact so operators don't have to install the proprietary
# Hailo Dataflow Compiler (~6 GB Python wheel + DFC license + the
# four-bug iter-153 monkey-patch dance) just to use the NPU path.
#
# Idempotent: re-runs skip the download when the file is present
# with the right sha256.
#
# Usage:
#   bash download-encoder-hef.sh [model_dir]
#
#   model_dir defaults to /var/lib/ruvector-hailo/models/all-minilm-l6-v2
#
# After this lands the HEF, build a worker with both features:
#   cargo build --release --features hailo,cpu-fallback \
#       --bin ruvector-hailo-worker \
#       --manifest-path crates/ruvector-hailo-cluster/Cargo.toml
#
# The model dir also needs the safetensors trio (HefEmbedder uses
# host-side BertEmbeddings to compute the embedding lookup before
# pushing to the NPU). If you haven't run it yet:
#   bash download-cpu-fallback-model.sh <same model_dir>

set -euo pipefail

MODEL_DIR="${1:-/var/lib/ruvector-hailo/models/all-minilm-l6-v2}"
RELEASE_TAG="hailo-encoder-v0.1.0-iter156b"
HEF_NAME="encoder.hef"
HEF_SHA256="cdbc892765d3099f74723ee6c28ab3f0daade2358827823ba08d2969b07ebd40"
HEF_URL="https://github.com/ruvnet/ruvector/releases/download/${RELEASE_TAG}/${HEF_NAME}"

if ! command -v curl >/dev/null 2>&1; then
  echo "curl not found — install with apt/yum/pacman" >&2
  exit 1
fi
if ! command -v sha256sum >/dev/null 2>&1; then
  echo "sha256sum not found — install coreutils" >&2
  exit 1
fi

echo "==> [1/3] prepare model dir"
# install.sh creates this with 0750 owned by ruvector-worker; if we
# arrive before install.sh ran we still want a useful default.
if ! mkdir -p "$MODEL_DIR" 2>/dev/null; then
  echo "    can't create $MODEL_DIR (need sudo? try: sudo bash $0)" >&2
  exit 2
fi
echo "    target: $MODEL_DIR"

echo "==> [2/3] fetch ${HEF_NAME} (sha256-pinned)"
DEST="$MODEL_DIR/model.hef"
if [[ -f "$DEST" ]]; then
  HAVE_SHA="$(sha256sum "$DEST" | awk '{print $1}')"
  if [[ "$HAVE_SHA" == "$HEF_SHA256" ]]; then
    echo "    ✓ already present (sha256 OK), skipping"
  else
    echo "    ! sha256 mismatch — re-downloading (have $HAVE_SHA)"
    rm -f "$DEST"
  fi
fi
if [[ ! -f "$DEST" ]]; then
  TMP="$DEST.partial"
  echo "    ↓ $HEF_URL → $DEST"
  curl -fSL --progress-bar -o "$TMP" "$HEF_URL"
  GOT_SHA="$(sha256sum "$TMP" | awk '{print $1}')"
  if [[ "$GOT_SHA" != "$HEF_SHA256" ]]; then
    rm -f "$TMP"
    echo "    ✗ sha256 mismatch after download" >&2
    echo "      expected: $HEF_SHA256" >&2
    echo "      got:      $GOT_SHA" >&2
    echo "      not writing — re-run or check network for tampering" >&2
    exit 3
  fi
  mv -f "$TMP" "$DEST"
fi

echo "==> [3/3] summary"
ls -la "$DEST"
sha256sum "$DEST"

cat <<EOF

The Hailo-8 encoder HEF is now at $DEST.

Required next steps for end-to-end NPU inference:

  1. Make sure the safetensors trio is also in the same dir
     (HefEmbedder needs all four files):
       bash download-cpu-fallback-model.sh $MODEL_DIR

  2. Build worker with both features:
       cargo build --release --features hailo,cpu-fallback \\
           --bin ruvector-hailo-worker \\
           --manifest-path crates/ruvector-hailo-cluster/Cargo.toml

  3. Install + start systemd unit:
       sudo bash deploy/install.sh \\
           target/release/ruvector-hailo-worker \\
           $MODEL_DIR
       sudo systemctl start ruvector-hailo-worker

The iter-145/167 startup self-test will print a sim_close/sim_far
ranking check in journald — that's how you confirm the NPU path
loaded correctly.

To re-compile the HEF from source (e.g. against a different
calibration corpus or a tighter mask-aware encoder), see
deploy/setup-hailo-compiler.sh + deploy/compile-encoder-hef.py.
EOF

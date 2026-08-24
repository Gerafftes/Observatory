#!/usr/bin/env bash
# Compile sentence-transformers/all-MiniLM-L6-v2 to a Hailo-8 .hef artifact.
#
# Run on an x86_64 Linux box with the Hailo Dataflow Compiler installed.
# This is the *only* missing piece between the worker's `NoModelLoaded`
# error (iter 130) and real semantic embeddings on the Hailo-8 NPU
# (iter 167's whole point).
#
# Why a script: the operator-side recipe was previously documented only
# in iter-86 + ADR-167 prose ("run the Hailo Dataflow Compiler against
# all-MiniLM-L6-v2.onnx"). One shell script makes this reproducible
# instead of operator-tribal-knowledge.
#
# Prereqs:
#   * Hailo Dataflow Compiler (proprietary, vendor-licensed):
#       https://hailo.ai/developer-zone/sw-downloads/
#       (ships as a .deb; installs as `hailomz`, `hailo`, and friends)
#   * Python 3.10+ with optimum-cli for the ONNX export
#   * ~5 GB free disk for intermediate artifacts
#
# Usage:
#   bash compile-hef.sh [--out <path>]
#
#   --out PATH   Final .hef destination. Defaults to ./model.hef.
#                Drop the result into the worker's model dir:
#                  /var/lib/ruvector-hailo/models/all-minilm-l6-v2/
#                and restart `ruvector-hailo-worker`. The next health
#                probe reports ready=true; embed RPCs return real
#                semantic vectors instead of NoModelLoaded.

set -euo pipefail

# Iter 132/134 — pick up the Hailo Dataflow Compiler venv automatically.
# setup-hailo-compiler.sh leaves a symlink at ~/.cache/ruvector-hailo-compiler/active
# pointing at the Python 3.10 venv that owns `hailo` and `optimum-cli`.
# Prepending it to PATH means a fresh shell can run this script without
# any manual env wrangling. Operator override: set HAILO_VENV.
HAILO_VENV="${HAILO_VENV:-$HOME/.cache/ruvector-hailo-compiler/active}"
if [[ -x "$HAILO_VENV/bin/hailo" ]]; then
  export PATH="$HAILO_VENV/bin:$PATH"
fi

OUT="model.hef"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --out)  OUT="${2:-}"; [[ -z "$OUT" ]] && { echo "--out needs a path" >&2; exit 1; }; shift 2 ;;
    -h|--help) sed -n '2,30p' "$0" | sed 's/^# \?//'; exit 0 ;;
    *) echo "unknown arg: $1" >&2; exit 1 ;;
  esac
done

WORK="$(mktemp -d -t hef-build-XXXXXX)"
trap 'rm -rf "$WORK"' EXIT

echo "==> [1/5] verify Hailo Dataflow Compiler is installed"
if ! command -v hailo >/dev/null 2>&1 && ! command -v hailomz >/dev/null 2>&1; then
  cat <<EOF >&2
Hailo Dataflow Compiler not found on PATH.

Install from:
  https://hailo.ai/developer-zone/sw-downloads/

Typical Ubuntu 22.04 install (as root):
  sudo apt install ./hailort_*.deb
  sudo apt install ./hailo-dataflow-compiler_*.deb
  hailo --version

Then re-run this script.
EOF
  exit 2
fi
HAILO_TOOL="$(command -v hailo || command -v hailomz)"
echo "    using: $HAILO_TOOL"

echo "==> [2/5] verify python + transformers/torch in venv"
PY="${HAILO_VENV:-$HOME/.cache/ruvector-hailo-compiler/active}/bin/python"
if [[ ! -x "$PY" ]]; then
  PY="$(command -v python3 || true)"
fi
if [[ -z "$PY" ]] || ! "$PY" -c "import sys; sys.exit(0 if sys.version_info >= (3, 10) else 1)" 2>/dev/null; then
  echo "    Python 3.10+ required (looked at $PY)" >&2; exit 2
fi
if ! "$PY" -c "import torch, transformers" 2>/dev/null; then
  echo "    installing torch + transformers into venv"
  uv pip install --python "$PY" 'torch==2.4.*' 'transformers>=4.40,<4.50' 2>&1 | tail -3
fi

echo "==> [3/5] export sentence-transformers/all-MiniLM-L6-v2 → ONNX"
ONNX_DIR="$WORK/onnx"
mkdir -p "$ONNX_DIR"
EXPORT_PY="$(dirname "${BASH_SOURCE[0]}")/export-minilm-onnx.py"
"$PY" "$EXPORT_PY" "$ONNX_DIR"
ONNX="$ONNX_DIR/model.onnx"
[[ -s "$ONNX" ]] || { echo "    ONNX export missing $ONNX" >&2; exit 3; }
echo "    $(stat --format='%s' "$ONNX") bytes → $ONNX"

echo "==> [4/5] hailo parse → optimize → compile (Python SDK API)"
# We drive the SDK via Python rather than `hailo` CLI because the CLI's
# `-y` auto-accepts the parser's end-node recommendation, which for BERT-6
# wrongly picks `/Where` (an attention-mask broadcast that the HN graph
# can't represent). The Python API lets us pin start/end node names
# explicitly. See compile-hef.py for the full sequence.
COMPILE_PY="$(dirname "${BASH_SOURCE[0]}")/compile-hef.py"
COMPILED="$WORK/minilm.hef"
"$PY" "$COMPILE_PY" "$ONNX" "$COMPILED"
[[ -s "$COMPILED" ]] || { echo "    no .hef produced at $COMPILED" >&2; exit 4; }

echo "==> [5/5] move to $OUT and report"
install -m 0644 "$COMPILED" "$OUT"
SHA="$(sha256sum "$OUT" | awk '{print $1}')"
echo
echo "  ✓ $OUT  ($(stat --format='%s' "$OUT") bytes)"
echo "  sha256: $SHA"
echo
echo "Deploy the artifact to the Pi 5 worker:"
echo "    scp $OUT root@cognitum-v0:/var/lib/ruvector-hailo/models/all-minilm-l6-v2/model.hef"
echo "    ssh root@cognitum-v0 'systemctl restart ruvector-hailo-worker'"
echo
echo "Verify the worker picked it up:"
echo "    ruvector-hailo-stats --workers cognitum-v0:50057 --json | jq '.stats, .ready'"
echo
echo "Once ready=true, ruvector-hailo-embed returns real semantic vectors;"
echo "iter-130's NoModelLoaded gate flips closed."

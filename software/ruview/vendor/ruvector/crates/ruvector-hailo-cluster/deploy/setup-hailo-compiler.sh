#!/usr/bin/env bash
# Set up the Hailo Dataflow Compiler in a Python 3.10 venv (iter 132).
#
# Companion to compile-hef.sh (iter 131). The compiler ships as a Python
# 3.10 wheel; modern host distros (Ubuntu 24.04, Fedora 40+) ship 3.12
# which breaks the wheel's imports. This script uses `uv` to materialise
# a 3.10-only venv, installs the vendor wheel into it, then prints the
# exact compile-hef.sh invocation that uses it.
#
# Prereqs (operator-side, one-time):
#
#   1. Create a free Hailo developer account:
#        https://hailo.ai/developer-zone/sw-downloads/
#
#   2. Download these two files into the same directory:
#        hailort_X.Y.Z_amd64.deb              (HailoRT C library)
#        hailo_dataflow_compiler-X.Y.Z-py3-none-linux_x86_64.whl
#
#   3. Run this script and pass the directory:
#        bash setup-hailo-compiler.sh /path/to/downloaded-files
#
# What the script does:
#
#   [1/5] verify `uv` (Python toolchain manager) is on PATH
#   [2/5] verify the two downloaded files are present + readable
#   [3/5] sudo apt install ./hailort_*.deb  (HailoRT runtime + libs)
#   [4/5] uv venv --python 3.10 ~/.cache/ruvector-hailo-compiler/venv
#         uv pip install --python ~/…/venv/bin/python ./hailo_dataflow_compiler-*.whl
#         uv pip install --python ~/…/venv/bin/python 'optimum[exporters]>=1.20'
#   [5/5] verify `hailo --version` runs from the venv
#
# Once finished, run:
#
#   ~/.cache/ruvector-hailo-compiler/venv/bin/hailo --version
#   bash compile-hef.sh   # picks up the venv automatically (iter-131 update)
#
# Note: the Dataflow Compiler is x86_64-linux-only (proprietary).
# Mac / Windows operators must install on a Linux box (VM or container).

set -euo pipefail

if [[ $# -lt 1 ]]; then
  echo "usage: $0 <path/to/downloaded-files>" >&2
  echo "  expects hailort_*.deb + hailo_dataflow_compiler-*.whl in that dir" >&2
  exit 1
fi

DOWNLOAD_DIR="$(realpath "$1")"
VENV_DIR="${HAILO_VENV:-$HOME/.cache/ruvector-hailo-compiler/venv}"

echo "==> [1/5] verify uv"
if ! command -v uv >/dev/null 2>&1; then
  echo "    uv not found on PATH. Install with one of:" >&2
  echo "      curl -LsSf https://astral.sh/uv/install.sh | sh" >&2
  echo "      pip install --user uv" >&2
  exit 2
fi
echo "    using: $(which uv) — $(uv --version)"

echo "==> [2/5] verify downloaded artifacts in $DOWNLOAD_DIR"
DEB_FILE="$(ls -1 "$DOWNLOAD_DIR"/hailort_*.deb 2>/dev/null | head -n 1)"
WHL_FILE="$(ls -1 "$DOWNLOAD_DIR"/hailo_dataflow_compiler-*.whl 2>/dev/null | head -n 1)"
if [[ -z "$DEB_FILE" ]]; then
  echo "    missing hailort_*.deb in $DOWNLOAD_DIR" >&2
  echo "    download from https://hailo.ai/developer-zone/sw-downloads/" >&2
  exit 3
fi
if [[ -z "$WHL_FILE" ]]; then
  echo "    missing hailo_dataflow_compiler-*.whl in $DOWNLOAD_DIR" >&2
  echo "    download from https://hailo.ai/developer-zone/sw-downloads/" >&2
  exit 3
fi
echo "    runtime:   $DEB_FILE  ($(stat --format='%s' "$DEB_FILE") bytes)"
echo "    compiler:  $WHL_FILE  ($(stat --format='%s' "$WHL_FILE") bytes)"

echo "==> [3/5] install HailoRT runtime via apt (requires sudo)"
if dpkg -l hailort 2>/dev/null | grep -q '^ii  hailort'; then
  echo "    hailort already installed: $(dpkg -l hailort | awk '/^ii/ {print $3}')"
else
  sudo apt install -y "$DEB_FILE"
fi
# Smoke-test the runtime hook
if ldconfig -p | grep -q libhailort.so; then
  echo "    libhailort.so visible to the linker"
else
  echo "    warn: libhailort.so not on ldconfig path; run \`sudo ldconfig\`" >&2
fi

echo "==> [4/5] create Python 3.10 venv + install compiler wheel"
mkdir -p "$(dirname "$VENV_DIR")"
if [[ ! -x "$VENV_DIR/bin/python" ]]; then
  echo "    creating venv at $VENV_DIR"
  uv venv --python 3.10 "$VENV_DIR"
else
  echo "    venv already exists at $VENV_DIR"
fi

VENV_PY="$VENV_DIR/bin/python"
echo "    installing wheel + Hailo's pinned deps + ONNX export deps into venv"
# Iter 134 — install in three phases so we get a working set:
#   (a) the dataflow compiler wheel (which has loose deps)
#   (b) Hailo's official requirements.txt if it's alongside the wheel —
#       this pins TF 2.18 + protobuf 3.20.3 + onnx 1.16, which is the
#       exact combo their SDK was tested against
#   (c) torch + transformers (no-deps so we don't clobber Hailo's pins)
#       for the ONNX export step driven by export-minilm-onnx.py.
#       The export script sets TRANSFORMERS_NO_TF=1 so we don't need
#       tf-keras (which would pull in TF 2.21 + proto 4 + break Hailo).
uv pip install --python "$VENV_PY" "$WHL_FILE"

REQ_FILE="$DOWNLOAD_DIR/requirements.txt"
if [[ ! -f "$REQ_FILE" ]]; then
  # Fall back to the suite's requirements.txt if the operator extracted
  # the AI SW Suite .run installer to a sibling dir.
  REQ_FILE="$(ls -1 "$DOWNLOAD_DIR"/../*hailo*suite*/requirements.txt 2>/dev/null | head -n 1)"
fi
if [[ -f "$REQ_FILE" ]]; then
  echo "    installing Hailo official requirements.txt: $REQ_FILE"
  uv pip install --python "$VENV_PY" -r "$REQ_FILE"
else
  echo "    no Hailo requirements.txt found — installing minimum pin set"
  uv pip install --python "$VENV_PY" 'tensorflow==2.18.*' 'protobuf==3.20.3' 'onnx==1.16.0' 'numpy<2'
fi

echo "    installing torch + transformers (--no-deps to preserve Hailo pins)"
uv pip install --python "$VENV_PY" --index-url https://download.pytorch.org/whl/cpu 'torch==2.4.*'
uv pip install --python "$VENV_PY" --no-deps 'transformers>=4.40,<4.50'
# transformers needs a few runtime deps that aren't in Hailo's req set
uv pip install --python "$VENV_PY" --no-deps 'tokenizers>=0.19' 'safetensors' 'huggingface-hub'

# Persist the venv path so compile-hef.sh's iter-131 invocation finds it.
# Symlink rather than env-var so it survives shell-context loss.
ln -sf "$VENV_DIR" "$HOME/.cache/ruvector-hailo-compiler/active"

echo "==> [5/5] verify hailo --version runs from the venv"
if "$VENV_DIR/bin/hailo" --version >/dev/null 2>&1; then
  VER="$("$VENV_DIR/bin/hailo" --version 2>&1 | head -1)"
  echo "    ✓ $VER"
else
  echo "    ✗ hailo --version failed" >&2
  echo "    Inspect the venv: $VENV_DIR/bin/hailo --help" >&2
  exit 4
fi

cat <<EOF

Setup complete. The Hailo Dataflow Compiler venv lives at:
  $VENV_DIR

Next step — produce the .hef artifact:
  bash compile-hef.sh

If compile-hef.sh doesn't pick up the venv automatically, prepend the
venv to your PATH manually:
  PATH="$VENV_DIR/bin:\$PATH" bash compile-hef.sh

Or set HAILO_VENV explicitly:
  HAILO_VENV=$VENV_DIR bash compile-hef.sh
EOF

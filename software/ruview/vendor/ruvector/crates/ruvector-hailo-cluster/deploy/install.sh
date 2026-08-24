#!/usr/bin/env bash
# Install ruvector-hailo-worker on a Pi 5 (with or without AI HAT+).
#
# Build on the Pi (or cross-compile for aarch64) before running this:
#
#   # CPU fallback path (works on any Pi 5; ~40-150 ms / embed):
#   cargo build --release --features cpu-fallback \
#       --bin ruvector-hailo-worker \
#       --manifest-path crates/ruvector-hailo-cluster/Cargo.toml
#
#   # Production path (Pi 5 + AI HAT+, NPU acceleration when HEF lands):
#   cargo build --release --features hailo,cpu-fallback \
#       --bin ruvector-hailo-worker \
#       --manifest-path crates/ruvector-hailo-cluster/Cargo.toml
#
# Idempotent — re-run after upgrading the binary.
#
# What this drops on the Pi (ADR-172 §3a iter-106 drop-root):
#   /usr/local/bin/ruvector-hailo-worker         (binary)
#   /var/lib/ruvector-hailo/                     (state dir, owned by
#                                                 ruvector-worker:ruvector-worker)
#   /etc/ruvector-hailo.env                      (config; preserved if
#                                                 it already exists)
#   /etc/systemd/system/ruvector-hailo-worker.service
#   /etc/udev/rules.d/99-hailo-ruvector.rules    (gives the
#                                                 ruvector-worker
#                                                 group rw on /dev/hailo*)
#   system user: ruvector-worker (no home, no shell)
#
# Usage:
#   sudo bash install.sh /path/to/ruvector-hailo-worker /path/to/models-dir

set -euo pipefail

if [[ $EUID -ne 0 ]]; then
  echo "must run as root (use sudo)" >&2; exit 1
fi
if [[ $# -lt 2 ]]; then
  echo "usage: $0 <path/to/ruvector-hailo-worker> <path/to/models-dir>" >&2
  echo "  models-dir must contain at least one of:" >&2
  echo "    model.hef + vocab.txt + special_tokens.json (NPU path, iter 167)" >&2
  echo "    model.safetensors + tokenizer.json + config.json (cpu-fallback, iter 134)" >&2
  echo "  Run deploy/download-cpu-fallback-model.sh to fetch the latter." >&2
  exit 1
fi

WORKER_BIN="$1"
MODELS_SRC="$2"

if [[ ! -x "$WORKER_BIN" ]]; then
  echo "binary not executable: $WORKER_BIN" >&2; exit 1
fi
if [[ ! -d "$MODELS_SRC" ]]; then
  echo "models dir not found: $MODELS_SRC" >&2; exit 1
fi
# Iter 166 (ADR-176 P5b polish): the iter-162 dispatch needs the
# safetensors trio EVEN on the NPU path — HefEmbedder uses
# HostEmbeddings (loads model.safetensors + tokenizer.json + config.json
# at boot) to compute the host-side embedding lookup before pushing to
# the NPU. So model.hef alone isn't enough; we need both layouts merged.
HAS_HEF=0
HAS_SAFETENSORS=0
HAS_TOKENIZER=0
HAS_CONFIG=0
[[ -f "$MODELS_SRC/model.hef" ]]          && HAS_HEF=1
[[ -f "$MODELS_SRC/model.safetensors" ]]  && HAS_SAFETENSORS=1
[[ -f "$MODELS_SRC/tokenizer.json" ]]     && HAS_TOKENIZER=1
[[ -f "$MODELS_SRC/config.json" ]]        && HAS_CONFIG=1

if (( HAS_HEF == 1 )); then
  if (( HAS_SAFETENSORS == 1 && HAS_TOKENIZER == 1 && HAS_CONFIG == 1 )); then
    echo "==> NPU path detected: model.hef + safetensors + tokenizer + config all present"
    echo "    HefEmbedder dispatch will route through the Hailo-8 NPU"
  else
    echo "warning: model.hef present but safetensors trio incomplete" >&2
    echo "         HefEmbedder needs all of:" >&2
    (( HAS_SAFETENSORS == 0 )) && echo "           model.safetensors  (missing)" >&2
    (( HAS_TOKENIZER   == 0 )) && echo "           tokenizer.json    (missing)" >&2
    (( HAS_CONFIG      == 0 )) && echo "           config.json       (missing)" >&2
    echo "         worker will fall through to NoModelLoaded — fix by running" >&2
    echo "         deploy/download-cpu-fallback-model.sh into $MODELS_SRC first" >&2
  fi
elif (( HAS_SAFETENSORS == 1 )); then
  echo "==> CPU fallback path detected: safetensors present, model.hef missing"
  if (( HAS_TOKENIZER == 0 || HAS_CONFIG == 0 )); then
    echo "warning: cpu-fallback also needs tokenizer.json + config.json" >&2
    echo "         re-run deploy/download-cpu-fallback-model.sh to fetch all three" >&2
  fi
else
  echo "warning: neither model.hef nor model.safetensors found in $MODELS_SRC" >&2
  echo "         worker will start but embed RPCs will return NoModelLoaded" >&2
fi

DEPLOY_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RUVECTOR_USER="ruvector-worker"
RUVECTOR_GROUP="ruvector-worker"

echo "==> ensure system user $RUVECTOR_USER exists"
# `useradd --system` returns 9 if the user already exists; treat as ok.
# Idempotent re-runs are the common case (binary upgrades).
if ! getent passwd "$RUVECTOR_USER" >/dev/null; then
  useradd \
    --system \
    --no-create-home \
    --home-dir /var/lib/ruvector-hailo \
    --shell /usr/sbin/nologin \
    --comment "ruvector Hailo worker (ADR-172 §3a)" \
    "$RUVECTOR_USER"
  echo "    -> created"
else
  echo "    -> already exists"
fi

echo "==> install binary"
install -o root -g root -m 0755 "$WORKER_BIN" /usr/local/bin/ruvector-hailo-worker

echo "==> install models -> /var/lib/ruvector-hailo/models/all-minilm-l6-v2"
install -d -o "$RUVECTOR_USER" -g "$RUVECTOR_GROUP" -m 0750 \
  /var/lib/ruvector-hailo \
  /var/lib/ruvector-hailo/models \
  /var/lib/ruvector-hailo/models/all-minilm-l6-v2
cp -a "$MODELS_SRC/." /var/lib/ruvector-hailo/models/all-minilm-l6-v2/
chown -R "$RUVECTOR_USER":"$RUVECTOR_GROUP" /var/lib/ruvector-hailo

echo "==> install /etc/ruvector-hailo.env (skipped if exists)"
if [[ ! -f /etc/ruvector-hailo.env ]]; then
  install -o root -g root -m 0644 "$DEPLOY_DIR/ruvector-hailo.env.example" /etc/ruvector-hailo.env
  echo "    -> wrote default; edit if non-default bind/model dir wanted"
else
  echo "    -> existing /etc/ruvector-hailo.env preserved"
fi

echo "==> install udev rule (gives $RUVECTOR_GROUP group rw on /dev/hailo*)"
install -o root -g root -m 0644 \
  "$DEPLOY_DIR/99-hailo-ruvector.rules" \
  /etc/udev/rules.d/99-hailo-ruvector.rules
udevadm control --reload-rules
# Trigger every hailo device the kernel currently sees so existing
# nodes pick up the new ownership without a reboot.
for dev in /dev/hailo*; do
  if [[ -e "$dev" ]]; then
    udevadm trigger "$dev" || true
  fi
done

echo "==> install systemd unit"
install -o root -g root -m 0644 \
  "$DEPLOY_DIR/ruvector-hailo-worker.service" \
  /etc/systemd/system/ruvector-hailo-worker.service

echo "==> daemon-reload + enable"
systemctl daemon-reload
systemctl enable ruvector-hailo-worker.service

echo
echo "Installed (running as $RUVECTOR_USER, no root)."
echo "To start now:"
echo "    sudo systemctl start ruvector-hailo-worker"
echo "Tail logs:"
echo "    journalctl -u ruvector-hailo-worker -f"
echo "Verify drop-root:"
echo "    ps -o user,pid,cmd -C ruvector-hailo-worker"
echo "    ls -l /dev/hailo0   # expect group ${RUVECTOR_GROUP}"

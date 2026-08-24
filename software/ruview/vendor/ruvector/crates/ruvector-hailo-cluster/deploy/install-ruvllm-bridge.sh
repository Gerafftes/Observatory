#!/usr/bin/env bash
# Install ruvllm-bridge — JSONL stdin/stdout adapter from ruvllm-shaped
# requests to the hailo-backend cluster (ADR-173, iter 124).
#
# Closes ADR-178 Gap A (HIGH): the other two bridges
# (mmwave, ruview-csi) shipped with install scripts since iter 106/123
# but ruvllm-bridge had no deploy automation, leaving operators to
# hand-build the user, drop the binary, and write the env file
# themselves.
#
# UNLIKE install-mmwave-bridge.sh and install-ruview-csi-bridge.sh, this
# installer does NOT drop a systemd unit. ruvllm-bridge is a
# subprocess-style adapter that reads JSON from stdin, writes JSON to
# stdout, and exits on EOF — it's spawned by the parent ruvllm
# process, not run as a long-lived daemon. systemd's lifecycle model
# (start/stop/restart-on-failure) doesn't fit; the parent process
# owns the bridge's lifecycle.
#
# Drops:
#   /usr/local/bin/ruvllm-bridge
#   /etc/ruvllm-bridge.env                  (config; preserved if exists)
#   /var/lib/ruvector-ruvllm/               (state dir, mostly reserved)
#   system user: ruvector-ruvllm (no home, no shell)
#
# Usage:
#   sudo bash install-ruvllm-bridge.sh /path/to/ruvllm-bridge

set -euo pipefail

if [[ $EUID -ne 0 ]]; then
  echo "must run as root (use sudo)" >&2; exit 1
fi
if [[ $# -lt 1 ]]; then
  echo "usage: $0 <path/to/ruvllm-bridge>" >&2
  exit 1
fi

BRIDGE_BIN="$1"
if [[ ! -x "$BRIDGE_BIN" ]]; then
  echo "binary not executable: $BRIDGE_BIN" >&2; exit 1
fi

DEPLOY_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RUVECTOR_USER="ruvector-ruvllm"
RUVECTOR_GROUP="ruvector-ruvllm"

echo "==> ensure system user $RUVECTOR_USER exists"
if ! getent passwd "$RUVECTOR_USER" >/dev/null; then
  useradd \
    --system \
    --no-create-home \
    --home-dir /var/lib/ruvector-ruvllm \
    --shell /usr/sbin/nologin \
    --comment "ruvector ruvllm bridge (ADR-173 + ADR-172 §3a)" \
    "$RUVECTOR_USER"
  echo "    -> created"
else
  echo "    -> already exists"
fi

echo "==> install binary"
install -o root -g root -m 0755 "$BRIDGE_BIN" /usr/local/bin/ruvllm-bridge

echo "==> install state dir /var/lib/ruvector-ruvllm"
install -d -o "$RUVECTOR_USER" -g "$RUVECTOR_GROUP" -m 0750 \
  /var/lib/ruvector-ruvllm

echo "==> install /etc/ruvllm-bridge.env (skipped if exists)"
if [[ ! -f /etc/ruvllm-bridge.env ]]; then
  install -o root -g root -m 0644 \
    "$DEPLOY_DIR/ruvllm-bridge.env.example" \
    /etc/ruvllm-bridge.env
  echo "    -> wrote default; edit BEFORE invoking the bridge"
  echo "       (RUVECTOR_RUVLLM_WORKERS / FINGERPRINT both need real values)"
else
  echo "    -> existing /etc/ruvllm-bridge.env preserved"
fi

echo
echo "Installed. Note: NO systemd unit — ruvllm-bridge is a subprocess"
echo "of the parent ruvllm process, not a daemon."
echo
echo "To invoke from a parent process / shell test:"
echo "    set -a; . /etc/ruvllm-bridge.env; set +a"
echo "    /usr/local/bin/ruvllm-bridge \\"
echo "        --workers \"\$RUVECTOR_RUVLLM_WORKERS\" \\"
echo "        --fingerprint \"\$RUVECTOR_RUVLLM_FINGERPRINT\" \\"
echo "        --dim \"\$RUVECTOR_RUVLLM_DIM\" \\"
echo "        \$RUVECTOR_RUVLLM_EXTRA_ARGS"
echo
echo "Test request line (pipe to stdin):"
echo "    echo '{\"text\":\"hello world\"}' | /usr/local/bin/ruvllm-bridge ..."

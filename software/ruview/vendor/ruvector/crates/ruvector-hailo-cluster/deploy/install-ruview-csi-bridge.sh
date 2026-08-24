#!/usr/bin/env bash
# Install ruview-csi-bridge on a host that receives RuView ADR-018
# CSI UDP frames (typically a Pi 5 on the same LAN as the ESP32 CSI
# nodes, or any Linux host that can route to them).
#
# Companion to install-mmwave-bridge.sh (mmwave) and install.sh (worker).
# Same idempotent shape as the iter-106 worker installer.
#
# Drops:
#   /usr/local/bin/ruview-csi-bridge
#   /var/lib/ruvector-csi/                        (state dir)
#   /etc/ruvector-csi-bridge.env                  (config; preserved if exists)
#   /etc/systemd/system/ruview-csi-bridge.service
#   system user: ruvector-csi (no home, no shell)
#
# Usage:
#   sudo bash install-ruview-csi-bridge.sh /path/to/ruview-csi-bridge

set -euo pipefail

if [[ $EUID -ne 0 ]]; then
  echo "must run as root (use sudo)" >&2; exit 1
fi
if [[ $# -lt 1 ]]; then
  echo "usage: $0 <path/to/ruview-csi-bridge>" >&2
  exit 1
fi

BRIDGE_BIN="$1"
if [[ ! -x "$BRIDGE_BIN" ]]; then
  echo "binary not executable: $BRIDGE_BIN" >&2; exit 1
fi

DEPLOY_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RUVECTOR_USER="ruvector-csi"
RUVECTOR_GROUP="ruvector-csi"

echo "==> ensure system user $RUVECTOR_USER exists"
if ! getent passwd "$RUVECTOR_USER" >/dev/null; then
  useradd \
    --system \
    --no-create-home \
    --home-dir /var/lib/ruvector-csi \
    --shell /usr/sbin/nologin \
    --comment "ruvector RuView CSI bridge (ADR-171 + ADR-172 §3a)" \
    "$RUVECTOR_USER"
  echo "    -> created"
else
  echo "    -> already exists"
fi

echo "==> install binary"
install -o root -g root -m 0755 "$BRIDGE_BIN" /usr/local/bin/ruview-csi-bridge

echo "==> install state dir /var/lib/ruvector-csi"
install -d -o "$RUVECTOR_USER" -g "$RUVECTOR_GROUP" -m 0750 \
  /var/lib/ruvector-csi

echo "==> install /etc/ruvector-csi-bridge.env (skipped if exists)"
if [[ ! -f /etc/ruvector-csi-bridge.env ]]; then
  install -o root -g root -m 0644 \
    "$DEPLOY_DIR/ruview-csi-bridge.env.example" \
    /etc/ruvector-csi-bridge.env
  echo "    -> wrote default; edit BEFORE starting the service"
  echo "       (RUVECTOR_CSI_LISTEN / WORKERS / FINGERPRINT all need real values)"
else
  echo "    -> existing /etc/ruvector-csi-bridge.env preserved"
fi

echo "==> install systemd unit"
install -o root -g root -m 0644 \
  "$DEPLOY_DIR/ruview-csi-bridge.service" \
  /etc/systemd/system/ruview-csi-bridge.service

echo "==> daemon-reload + enable (NOT started — env file needs editing first)"
systemctl daemon-reload
systemctl enable ruview-csi-bridge.service

echo
echo "Installed. Before starting:"
echo "    sudo \$EDITOR /etc/ruvector-csi-bridge.env"
echo "  set RUVECTOR_CSI_LISTEN / RUVECTOR_CSI_WORKERS / RUVECTOR_CSI_FINGERPRINT"
echo
echo "Then:"
echo "    sudo systemctl start ruview-csi-bridge"
echo "    journalctl -u ruview-csi-bridge -f"
echo "    ss -ulnp | grep 5005    # verify the UDP listener is bound"

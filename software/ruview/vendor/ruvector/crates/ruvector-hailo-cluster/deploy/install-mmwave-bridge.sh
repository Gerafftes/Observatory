#!/usr/bin/env bash
# Install ruvector-mmwave-bridge on a host with a 60 GHz mmWave radar
# attached over USB-serial (CP210x / CH340 / FTDI / native USB-CDC).
#
# Run on the radar-attached host (not the Pi 5 that runs the worker)
# after building the binary on the same arch with:
#
#   cargo build --release --bin ruvector-mmwave-bridge \
#       --manifest-path crates/ruvector-hailo-cluster/Cargo.toml
#
# Idempotent — re-run after upgrading the binary.
#
# Drops on the host:
#   /usr/local/bin/ruvector-mmwave-bridge      (binary)
#   /var/lib/ruvector-bridge/                  (state dir, owned by user)
#   /etc/ruvector-mmwave-bridge.env            (config; preserved if exists)
#   /etc/systemd/system/ruvector-mmwave-bridge.service
#   /etc/udev/rules.d/99-radar-ruvector.rules  (group rw on radar tty)
#   system user: ruvector-bridge (no home, no shell)
#
# Usage:
#   sudo bash install-mmwave-bridge.sh /path/to/ruvector-mmwave-bridge

set -euo pipefail

if [[ $EUID -ne 0 ]]; then
  echo "must run as root (use sudo)" >&2; exit 1
fi
if [[ $# -lt 1 ]]; then
  echo "usage: $0 <path/to/ruvector-mmwave-bridge>" >&2
  exit 1
fi

BRIDGE_BIN="$1"
if [[ ! -x "$BRIDGE_BIN" ]]; then
  echo "binary not executable: $BRIDGE_BIN" >&2; exit 1
fi

DEPLOY_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RUVECTOR_USER="ruvector-bridge"
RUVECTOR_GROUP="ruvector-bridge"

echo "==> ensure system user $RUVECTOR_USER exists"
if ! getent passwd "$RUVECTOR_USER" >/dev/null; then
  useradd \
    --system \
    --no-create-home \
    --home-dir /var/lib/ruvector-bridge \
    --shell /usr/sbin/nologin \
    --comment "ruvector mmWave bridge (ADR-063 + ADR-172 §3a)" \
    "$RUVECTOR_USER"
  echo "    -> created"
else
  echo "    -> already exists"
fi

echo "==> install binary"
install -o root -g root -m 0755 "$BRIDGE_BIN" /usr/local/bin/ruvector-mmwave-bridge

echo "==> install state dir /var/lib/ruvector-bridge"
install -d -o "$RUVECTOR_USER" -g "$RUVECTOR_GROUP" -m 0750 \
  /var/lib/ruvector-bridge

echo "==> install /etc/ruvector-mmwave-bridge.env (skipped if exists)"
if [[ ! -f /etc/ruvector-mmwave-bridge.env ]]; then
  install -o root -g root -m 0644 \
    "$DEPLOY_DIR/ruvector-mmwave-bridge.env.example" \
    /etc/ruvector-mmwave-bridge.env
  echo "    -> wrote default; edit BEFORE starting the service"
  echo "       (RUVECTOR_BRIDGE_DEVICE / WORKERS / FINGERPRINT all need real values)"
else
  echo "    -> existing /etc/ruvector-mmwave-bridge.env preserved"
fi

echo "==> install udev rule (gives $RUVECTOR_GROUP group rw on radar tty)"
install -o root -g root -m 0644 \
  "$DEPLOY_DIR/99-radar-ruvector.rules" \
  /etc/udev/rules.d/99-radar-ruvector.rules
udevadm control --reload-rules
# Re-trigger any tty nodes that match the rule's vendor IDs so an
# already-attached radar picks up the new ownership without replug.
for dev in /dev/ttyUSB* /dev/ttyACM*; do
  if [[ -e "$dev" ]]; then
    udevadm trigger "$dev" || true
  fi
done

echo "==> install systemd unit"
install -o root -g root -m 0644 \
  "$DEPLOY_DIR/ruvector-mmwave-bridge.service" \
  /etc/systemd/system/ruvector-mmwave-bridge.service

echo "==> daemon-reload + enable (NOT started — env file needs editing first)"
systemctl daemon-reload
systemctl enable ruvector-mmwave-bridge.service

echo
echo "Installed. Before starting:"
echo "    sudo \$EDITOR /etc/ruvector-mmwave-bridge.env"
echo "  set RUVECTOR_BRIDGE_DEVICE / RUVECTOR_BRIDGE_WORKERS / RUVECTOR_BRIDGE_FINGERPRINT"
echo
echo "Then:"
echo "    sudo systemctl start ruvector-mmwave-bridge"
echo "    journalctl -u ruvector-mmwave-bridge -f"
echo "    ls -l /dev/ttyUSB* /dev/ttyACM*    # group should be ruvector-bridge"

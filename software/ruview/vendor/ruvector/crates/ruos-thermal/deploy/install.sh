#!/usr/bin/env bash
# Install ruos-thermal on a Pi 5 (or any aarch64 Linux box that exposes
# the standard /sys/class/thermal + /sys/devices/system/cpu/cpufreq tree).
#
# Companion to ruvector-hailo-worker (same Pi). Drops a 30s timer that
# atomically writes a Prometheus textfile-collector file to:
#   /var/lib/node_exporter/textfile_collector/ruos-thermal.prom
#
# Run on the Pi (not on a dev host) after building / scp'ing the binary:
#   cargo build --release --target aarch64-unknown-linux-gnu \
#       --manifest-path crates/ruos-thermal/Cargo.toml
#   scp target/aarch64-unknown-linux-gnu/release/ruos-thermal \
#       root@cognitum-v0:/usr/local/bin/
#
# Idempotent — re-run after upgrading the binary.
#
# Usage:
#   sudo bash install.sh [/path/to/ruos-thermal]
#
# If the binary path is omitted, the script assumes /usr/local/bin/ruos-thermal
# already exists (e.g. from an scp drop).

set -euo pipefail

if [[ $EUID -ne 0 ]]; then
  echo "must run as root (use sudo)" >&2; exit 1
fi

THERMAL_BIN="${1:-/usr/local/bin/ruos-thermal}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# 1) Stage binary if a source path was given (skips if already in place).
if [[ "$THERMAL_BIN" != "/usr/local/bin/ruos-thermal" ]]; then
  install -m 0755 -o root -g root "$THERMAL_BIN" /usr/local/bin/ruos-thermal
  echo "[install] /usr/local/bin/ruos-thermal updated"
fi
if [[ ! -x /usr/local/bin/ruos-thermal ]]; then
  echo "ruos-thermal binary not found at /usr/local/bin/ruos-thermal" >&2
  echo "Build it first:" >&2
  echo "  cargo build --release --target aarch64-unknown-linux-gnu \\" >&2
  echo "      --manifest-path crates/ruos-thermal/Cargo.toml" >&2
  exit 1
fi

# 2) Make sure the textfile-collector dir exists. node_exporter typically
#    creates this; we belt-and-suspender it for headless Pi installs.
install -d -m 0755 -o root -g root /var/lib/node_exporter/textfile_collector

# 3) Drop the systemd unit + timer.
install -m 0644 -o root -g root \
    "$SCRIPT_DIR/ruos-thermal.service" \
    /etc/systemd/system/ruos-thermal.service
install -m 0644 -o root -g root \
    "$SCRIPT_DIR/ruos-thermal.timer" \
    /etc/systemd/system/ruos-thermal.timer

# 4) Enable + start the timer (which fires the service).
systemctl daemon-reload
systemctl enable --now ruos-thermal.timer

echo
echo "[install] ruos-thermal.timer enabled — first snapshot in 5s, then every 30s"
echo "[install] Output: /var/lib/node_exporter/textfile_collector/ruos-thermal.prom"
echo "[install] Inspect with:"
echo "    systemctl status ruos-thermal.timer ruos-thermal.service"
echo "    cat /var/lib/node_exporter/textfile_collector/ruos-thermal.prom"
echo "    journalctl -u ruos-thermal.service --since '1 min ago'"

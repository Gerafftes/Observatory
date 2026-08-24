#!/usr/bin/env bash
# Cross-compile ruvector-hailo cluster CLIs from x86_64 → aarch64.
#
# Builds four binaries that DON'T need libhailort (so they cross-compile
# without a Hailo aarch64 sysroot):
#
#   ruvector-hailo-embed
#   ruvector-hailo-stats
#   ruvector-hailo-fakeworker
#   ruvector-hailo-cluster-bench
#
# The fifth binary, ruvector-hailo-worker, links libhailort and must
# build natively on the Pi (or in a Pi-like aarch64 sysroot with HailoRT
# 4.23 installed). Not handled here.
#
# Usage:
#   bash cross-build.sh [--deploy <pi-tailnet-or-local-name>]
#
#   --deploy NAME   rsync the four binaries to NAME:/usr/local/bin/
#                   (uses tailscale ssh if NAME is on the tailnet, plain
#                    ssh otherwise; expects passwordless ssh).
#
# Re-run idempotently. cargo's incremental cache makes re-runs fast.

set -euo pipefail

CRATE_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")"/.. && pwd)"
TARGET="aarch64-unknown-linux-gnu"

echo "==> [1/5] verify rustup target"
if ! rustup target list --installed | grep -q "^$TARGET\$"; then
  echo "    installing rustup target $TARGET"
  rustup target add "$TARGET"
else
  echo "    $TARGET already installed"
fi

echo "==> [2/5] verify aarch64 C linker"
if ! command -v aarch64-linux-gnu-gcc >/dev/null 2>&1; then
  echo "    aarch64-linux-gnu-gcc not found." >&2
  echo "    On Ubuntu/Debian: sudo apt install gcc-aarch64-linux-gnu" >&2
  echo "    On Fedora:        sudo dnf install gcc-aarch64-linux-gnu" >&2
  echo "    On Arch:          sudo pacman -S aarch64-linux-gnu-gcc" >&2
  exit 1
fi
echo "    $(aarch64-linux-gnu-gcc --version | head -1)"

echo "==> [3/5] write per-target cargo config (idempotent)"
mkdir -p "$CRATE_DIR/.cargo"
CFG="$CRATE_DIR/.cargo/config.toml"
if ! grep -q "^\[target\.$TARGET\]" "$CFG" 2>/dev/null; then
  cat >> "$CFG" <<EOF
[target.$TARGET]
linker = "aarch64-linux-gnu-gcc"
EOF
  echo "    appended [target.$TARGET] linker setting to $CFG"
else
  echo "    $CFG already has $TARGET section"
fi

echo "==> [4/5] cross-compile (release, no hailo feature)"
BINS=(
  ruvector-hailo-embed
  ruvector-hailo-stats
  ruvector-hailo-fakeworker
  ruvector-hailo-cluster-bench
)
cd "$CRATE_DIR"
env -u RUSTFLAGS \
  cargo build --release --target "$TARGET" \
    "${BINS[@]/#/--bin=}" 2>&1 | tail -20

OUTDIR="$CRATE_DIR/target/$TARGET/release"
echo "    artifacts in $OUTDIR:"
for b in "${BINS[@]}"; do
  if [[ -x "$OUTDIR/$b" ]]; then
    sz=$(stat -c '%s' "$OUTDIR/$b")
    echo "      $b   $(numfmt --to=iec-i --suffix=B "$sz")"
  else
    echo "      $b   MISSING"
  fi
done

echo "==> [5/5] optional deploy"
DEPLOY=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --deploy) DEPLOY="$2"; shift 2;;
    *) echo "unknown arg: $1" >&2; exit 1;;
  esac
done
if [[ -n "$DEPLOY" ]]; then
  echo "    rsyncing to $DEPLOY:/usr/local/bin/  (sudo via ssh)"
  for b in "${BINS[@]}"; do
    rsync -aH --rsync-path="sudo rsync" "$OUTDIR/$b" "$DEPLOY:/usr/local/bin/$b"
  done
  echo "    done — verify:  ssh $DEPLOY '/usr/local/bin/ruvector-hailo-stats --help'"
else
  echo "    (no --deploy <host> given — skipping)"
  echo "    next: copy with"
  echo "      scp $OUTDIR/ruvector-hailo-{embed,stats,fakeworker,cluster-bench} HOST:/usr/local/bin/"
fi

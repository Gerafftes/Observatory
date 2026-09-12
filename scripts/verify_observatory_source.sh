#!/bin/sh
set -eu

repo_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
source_dir="$repo_dir/software/ruview"
expected_source_entries=15889

required_paths='README.md
LICENSE
ui/index.html
ui/components/ObservatoryControlCenter.js
ui/components/RoomGeometryEditor.js
ui/components/MmwaveCalibrationAssistant.js
v2/Cargo.toml
v2/crates/wifi-densepose-sensing-server/Cargo.toml
v2/crates/wifi-densepose-sensing-server/src/d5_presence.rs
v2/crates/wifi-densepose-sensing-server/src/d6_fingerprint.rs
v2/crates/wifi-densepose-sensing-server/src/mmwave_calibration.rs
v2/crates/wifi-densepose-sensing-server/src/calibration_dataset.rs
v2/crates/wifi-densepose-sensing-server/src/experiment.rs
firmware/esp32-csi-node/CMakeLists.txt
firmware/esp32-mmwave-node/CMakeLists.txt
archive/v1/src/sensing/ws_server.py
vendor/midstream/Cargo.toml
vendor/ruvector/Cargo.toml
vendor/sublinear-time-solver/Cargo.toml
vendor/rvcsi/Cargo.toml
vendor/rufield/Cargo.toml
v2/crates/ruv-neural/Cargo.toml
v2/crates/ruview-swarm/Cargo.toml
v2/crates/worldgraph/Cargo.toml'

for relative_path in $required_paths; do
  if [ ! -e "$source_dir/$relative_path" ]; then
    printf 'MISSING: software/ruview/%s\n' "$relative_path" >&2
    exit 1
  fi
done

required_reports='results/2026-07-26_D4-E0_leerraum.md
results/2026-07-26_D5_realer-still-livetest.md
results/2026-08-09_D6_setup-siegel-und-preflight.md
results/2026-08-23_D4-D5-D6_technischer-ergebnisbericht.md'

for relative_path in $required_reports; do
  if [ ! -e "$repo_dir/$relative_path" ]; then
    printf 'MISSING: %s\n' "$relative_path" >&2
    exit 1
  fi
done

source_entries=$(
  git -C "$repo_dir" ls-files --cached --others --exclude-standard -- 'software/ruview/**' |
    while IFS= read -r relative_path; do
      if [ -e "$repo_dir/$relative_path" ]; then
        printf '%s\n' "$relative_path"
      fi
    done |
    sort -u |
    wc -l |
    tr -d ' '
)
if [ "$source_entries" -ne "$expected_source_entries" ]; then
  printf 'SOURCE COUNT: expected %s, found %s\n' \
    "$expected_source_entries" "$source_entries" >&2
  exit 1
fi

unexpected_state=$(find "$source_dir" \
  \( -path '*/target' -o -path '*/node_modules' \) -prune -o \
  \( -name .git -o -name .DS_Store -o -path '*/data/recordings/*' \) \
  -print -quit)
if [ -n "$unexpected_state" ]; then
  printf 'UNEXPECTED LOCAL STATE: %s\n' "$unexpected_state" >&2
  exit 1
fi

oversized_file=$(find "$source_dir" \
  \( -path '*/target' -o -path '*/node_modules' \) -prune -o \
  -type f -size +95M -print -quit)
if [ -n "$oversized_file" ]; then
  printf 'GITHUB SIZE GATE: %s exceeds 95 MiB\n' "$oversized_file" >&2
  exit 1
fi

git -C "$repo_dir" check-ignore -q data/raw/private-probe.jsonl
git -C "$repo_dir" check-ignore -q logs/private-probe.log
git -C "$repo_dir" check-ignore -q software/ruview/data/recordings/private-probe.jsonl

printf 'Observatory source verification: PASS (%s source entries)\n' "$source_entries"

#!/bin/sh
set -eu

repo_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
source_dir="$repo_dir/software/ruview"
expected_source_entries=15814

required_paths='README.md
README.en.md
08-aktueller-arbeitsstand-d6-und-position.md
CHANGELOG.md
PROOF.md
LICENSE
ui/index.html
ui/components/ObservatoryControlCenter.js
ui/components/RoomGeometryEditor.js
ui/components/MmwaveCalibrationAssistant.js
ui/components/MmwaveDebugView.js
ui/tests/mmwave-debug-view.test.mjs
v2/Cargo.toml
v2/crates/wifi-densepose-sensing-server/Cargo.toml
v2/crates/wifi-densepose-sensing-server/src/calibration_persistence.rs
v2/crates/wifi-densepose-sensing-server/src/d5_presence.rs
v2/crates/wifi-densepose-sensing-server/src/d6_fingerprint.rs
v2/crates/wifi-densepose-sensing-server/src/mmwave_calibration.rs
v2/crates/wifi-densepose-sensing-server/src/calibration_dataset.rs
v2/crates/wifi-densepose-sensing-server/src/experiment.rs
firmware/esp32-csi-node/CMakeLists.txt
firmware/esp32-csi-node/tests/test_security_boundaries.py
firmware/esp32-mmwave-node/CMakeLists.txt
scripts/audit_csi_timestamp_fusion.py
scripts/audit_mmwave_runtime.py
scripts/build_d4_d5_d6_results.py
scripts/tests/test_audit_csi_timestamp_fusion.py
scripts/tests/test_audit_mmwave_runtime.py
scripts/tests/test_ruview_sensing_server_auth.py
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

source_entries=$(find "$source_dir" \( -type f -o -type l \) | wc -l | tr -d ' ')
if [ "$source_entries" -ne "$expected_source_entries" ]; then
  printf 'SOURCE COUNT: expected %s, found %s\n' \
    "$expected_source_entries" "$source_entries" >&2
  exit 1
fi

unexpected_state=$(find "$source_dir" \
  \( -name .git -o -name .DS_Store -o -path '*/target/*' \
  -o -path '*/node_modules/*' -o -path '*/data/recordings/*' \
  -o -path '*/.vite/*' -o -name '*.db' -o -name '*.sqlite' -o -name '*.sqlite3' \) \
  -print -quit)
if [ -n "$unexpected_state" ]; then
  printf 'UNEXPECTED LOCAL STATE: %s\n' "$unexpected_state" >&2
  exit 1
fi

oversized_file=$(find "$source_dir" -type f -size +95M -print -quit)
if [ -n "$oversized_file" ]; then
  printf 'GITHUB SIZE GATE: %s exceeds 95 MiB\n' "$oversized_file" >&2
  exit 1
fi

privacy_hits=$(mktemp)
trap 'rm -f "$privacy_hits"' EXIT HUP INT TERM
private_key_prefix='-----BEGIN'
private_key_suffix='PRIVATE KEY-----'
privacy_pattern="${private_key_prefix} (RSA |EC |OPENSSH )?${private_key_suffix}|AKIA[0-9A-Z]{16}|gh[pousr]_[A-Za-z0-9_]{20,}|github_pat_[A-Za-z0-9_]{20,}|xox[baprs]-[A-Za-z0-9-]{20,}|/Users/Johann|/private/var|/var/folders"
rg --no-config -n -H --hidden -g '!.git' -g '!.git/**' -- "$privacy_pattern" "$repo_dir" > "$privacy_hits" || true
unexpected_privacy_hits=''
while IFS= read -r privacy_hit; do
  privacy_path=${privacy_hit%%:*}
  privacy_relative=${privacy_path#"$repo_dir"/}
  case "$privacy_relative" in
    scripts/verify_observatory_source.sh|\
    software/ruview/vendor/midstream/AIMDS/reports/SECURITY_AUDIT_REPORT.md|\
    software/ruview/vendor/ruvector/crates/mcp-brain-server/src/tests.rs|\
    software/ruview/vendor/ruvector/crates/mcp-brain-server/src/verify.rs|\
    software/ruview/vendor/ruvector/crates/mcp-brain/src/pipeline.rs|\
    software/ruview/vendor/ruvector/crates/ruvector-kalshi/src/auth.rs|\
    software/ruview/vendor/ruvector/crates/ruvector-kalshi/src/secrets.rs|\
    software/ruview/vendor/ruvector/crates/rvf/rvf-federation/src/pii_strip.rs|\
    software/ruview/vendor/ruvector/docs/research/claude-code-rvsource/extracted/source/config/config.js|\
    software/ruview/vendor/ruvector/docs/research/claude-code-rvsource/extracted/source/uncategorized/uncategorized.js|\
    software/ruview/vendor/ruvector/examples/apify/llm/README.md|\
    software/ruview/vendor/ruvector/npm/packages/ruvbot/.env.example|\
    software/ruview/vendor/sublinear-time-solver/validation/security_validation.rs)\
      ;;
    *) unexpected_privacy_hits="$unexpected_privacy_hits$privacy_hit\n" ;;
  esac
done < "$privacy_hits"
if [ -n "$unexpected_privacy_hits" ]; then
  printf 'PRIVACY GATE: unreviewed credential or local-path pattern found\n%s' "$unexpected_privacy_hits" >&2
  exit 1
fi

if git -C "$repo_dir" ls-files | awk '/(^|\/)\.env($|\.)/ && $0 !~ /\.env\.example$/ {print; found=1} END {exit found ? 0 : 1}' | grep -q .; then
  printf 'PRIVACY GATE: non-example .env file is tracked\n' >&2
  exit 1
fi

if [ ! -f "$repo_dir/SHA256SUMS.txt" ]; then
  printf 'INTEGRITY GATE: SHA256SUMS.txt is missing\n' >&2
  exit 1
fi

tracked_without_manifest=$(git -C "$repo_dir" ls-files | awk '$0 != "SHA256SUMS.txt"' | wc -l | tr -d ' ')
manifest_entries=$(wc -l < "$repo_dir/SHA256SUMS.txt" | tr -d ' ')
if [ "$tracked_without_manifest" -ne "$manifest_entries" ]; then
  printf 'INTEGRITY GATE: manifest has %s entries for %s tracked files\n' \
    "$manifest_entries" "$tracked_without_manifest" >&2
  exit 1
fi

if ! (cd "$repo_dir" && shasum -a 256 -c SHA256SUMS.txt >/dev/null); then
  printf 'INTEGRITY GATE: SHA-256 verification failed\n' >&2
  exit 1
fi

git -C "$repo_dir" check-ignore -q data/raw/private-probe.jsonl
git -C "$repo_dir" check-ignore -q logs/private-probe.log
git -C "$repo_dir" check-ignore -q software/ruview/data/recordings/private-probe.jsonl

printf 'Observatory source verification: PASS (%s source entries)\n' "$source_entries"

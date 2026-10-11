#!/bin/sh
set -eu

repo_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
source_dir="$repo_dir/software/ruview"

required_paths='README.md
LICENSE
ui/index.html
ui/components/ObservatoryControlCenter.js
ui/components/RoomGeometryEditor.js
ui/components/MmwaveCalibrationAssistant.js
ui/tests/api-contract-catalog.test.mjs
v2/Cargo.toml
v2/crates/wifi-densepose-sensing-server/Cargo.toml
v2/crates/wifi-densepose-sensing-server/src/d5_presence.rs
v2/crates/wifi-densepose-sensing-server/src/d6_fingerprint.rs
v2/crates/wifi-densepose-sensing-server/src/mmwave_calibration.rs
v2/crates/wifi-densepose-sensing-server/src/calibration_dataset.rs
v2/crates/wifi-densepose-sensing-server/src/experiment.rs
v2/crates/wifi-densepose-sensing-server/src/model_routes.rs
v2/crates/wifi-densepose-sensing-server/src/recording_routes.rs
v2/crates/wifi-densepose-sensing-server/src/training_routes.rs
v2/crates/wifi-densepose-sensing-server/src/observatory_routes.rs
v2/crates/wifi-densepose-sensing-server/src/mmwave_routes.rs
v2/crates/wifi-densepose-sensing-server/src/calibration_routes.rs
v2/crates/wifi-densepose-sensing-server/src/sensing_routes.rs
v2/crates/wifi-densepose-sensing-server/src/system_routes.rs
v2/crates/wifi-densepose-sensing-server/src/route_support.rs
v2/crates/wifi-densepose-sensing-server/src/state.rs
v2/crates/wifi-densepose-sensing-server/src/protocol.rs
v2/crates/wifi-densepose-sensing-server/src/runtime_tasks.rs
firmware/esp32-csi-node/CMakeLists.txt
firmware/esp32-csi-tx/main/tx_config.h
firmware/esp32-csi-tx/main/tx_safety.h
firmware/esp32-csi-tx/main/tx_network.c
firmware/esp32-csi-tx/main/tx_boot.c
firmware/esp32-csi-tx/main/tx_ota.c
firmware/esp32-csi-tx/tools/tx1_usb.py
firmware/esp32-csi-tx/tools/tx1_measure.py
firmware/esp32-csi-tx/tools/build_release.py
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

required_reports='experiment-reports/2026-07-26_D4-E0_leerraum/README.md
experiment-reports/2026-07-26_D5_still-livetest/README.md
experiment-reports/2026-08-09_D6_setup-siegel-und-preflight/README.md
experiment-reports/2026-08-23_D4-D5-D6_technischer-bericht/README.md'

for relative_path in $required_reports; do
  if [ ! -e "$repo_dir/$relative_path" ]; then
    printf 'MISSING: %s\n' "$relative_path" >&2
    exit 1
  fi
done

# SHA256SUMS defines the reviewed source inventory, not a historical file count.
# Existing untracked build products remain untouched and are not publishable sources.
source_entries=$(python3 - "$repo_dir" <<'PY_VERIFY'
import hashlib
from pathlib import Path
import subprocess
import sys
root = Path(sys.argv[1])
manifest = {}
for line in (root / "SHA256SUMS.txt").read_text().splitlines():
    checksum, name = line.split("  ", 1)
    if name in manifest or Path(name).is_absolute() or ".." in Path(name).parts:
        raise SystemExit("Invalid/duplicate source manifest entry")
    manifest[name] = checksum
    if name.startswith(("private/", "artifacts/", ".toolchains/")) or "/managed_components/" in name:
        raise SystemExit("Private/generated entry in public manifest")
    path = root / name
    if not path.is_file() or hashlib.sha256(path.read_bytes()).hexdigest() != checksum:
        raise SystemExit("Source hash/missing file: " + name)
tracked = set(subprocess.check_output(["git", "-C", str(root), "ls-files", "--", "software/ruview/"], text=True).splitlines())
visible = set(subprocess.check_output(["git", "-C", str(root), "ls-files", "--cached", "--others", "--exclude-standard", "--", "software/ruview/"], text=True).splitlines())
generated_tests = {"test_adr110", "test_csi_source_binding", "test_mmwave_detect", "test_node_identity", "test_vitals"}
for cache in (root/"software/ruview/firmware").glob("*/managed_components"):
    relative = cache.relative_to(root).as_posix()
    ignored = subprocess.run(["git", "-C", str(root), "check-ignore", "-q", relative+"/source-audit-probe"], check=False)
    if ignored.returncode != 0 or any(name.startswith(relative+"/") for name in tracked):
        raise SystemExit("SDK cache is not wholly private/generated: " + relative)
for name in visible:
    if name not in tracked:
        if name.startswith("software/ruview/firmware/esp32-csi-node/build-rx-led/"):
            continue
        if Path(name).parent.as_posix() == "software/ruview/firmware/esp32-csi-node/test" and Path(name).name in generated_tests:
            # Exclude only the existing Mach-O executables, never C/Python sources.
            if (root/name).read_bytes()[:4] in (b"\xcf\xfa\xed\xfe", b"\xfe\xed\xfa\xcf", b"\xca\xfe\xba\xbe"):
                continue
    if name not in manifest:
        raise SystemExit("Source missing from SHA256SUMS: " + name)
private_tracked = subprocess.check_output(["git", "-C", str(root), "ls-files", "--", "private/", "software/ruview/firmware/esp32-csi-tx/tx_config.h"], text=True)
if private_tracked.strip():
    raise SystemExit("Private backup/config is tracked")
print(sum(name.startswith("software/ruview/") for name in manifest))
PY_VERIFY
)

unexpected_state=$(find "$source_dir" \
  \( -path '*/target' -o -path '*/node_modules' -o -path "$source_dir/firmware/*/managed_components" \) -prune -o \
  \( -name .git -o -name .DS_Store -o -path '*/data/recordings/*' \) \
  -print -quit)
if [ -n "$unexpected_state" ]; then
  printf 'UNEXPECTED LOCAL STATE: %s\n' "$unexpected_state" >&2
  exit 1
fi

oversized_file=$(find "$source_dir" \
  \( -path '*/target' -o -path '*/node_modules' -o -path "$source_dir/firmware/*/managed_components" \) -prune -o \
  -type f -size +95M -print -quit)
if [ -n "$oversized_file" ]; then
  printf 'GITHUB SIZE GATE: %s exceeds 95 MiB\n' "$oversized_file" >&2
  exit 1
fi

git -C "$repo_dir" check-ignore -q data/raw/private-probe.jsonl
git -C "$repo_dir" check-ignore -q logs/private-probe.log
git -C "$repo_dir" check-ignore -q software/ruview/data/recordings/private-probe.jsonl

git -C "$repo_dir" check-ignore -q private/tx1-private-probe.json
git -C "$repo_dir" check-ignore -q software/ruview/firmware/esp32-csi-tx/tx_config.h
if git -C "$repo_dir" check-ignore -q software/ruview/firmware/esp32-csi-tx/main/tx_config.h; then
  printf 'REQUIRED TX HEADER IS IGNORED\n' >&2
  exit 1
fi

printf 'Observatory source verification: PASS (%s source entries)\n' "$source_entries"

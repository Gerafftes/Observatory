#!/usr/bin/env python3
"""Snapshot and build the common RX app twice. Never opens a device or flashes."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

SDK_COMMIT = "67c1de1eebe095d554d281952fde63c16ee2dca0"
APP_SLOT_BYTES = 2 * 1024 * 1024


def sha(data):
    return hashlib.sha256(data).hexdigest()


def write_json(path, value):
    with path.open("x", encoding="utf-8") as output:
        json.dump(value, output, sort_keys=True, indent=2)
        output.write("\n")


def source_files(root):
    names = ["CMakeLists.txt", "version.txt", "partitions_display.csv", "sdkconfig.defaults",
             "sdkconfig.defaults.rx-unified", "dependencies.rx-release.lock", "build_rx_release.py"]
    names += [str(path.relative_to(root)) for path in sorted((root / "main").iterdir())
              if path.suffix in (".c", ".h", ".cmake") or path.name in ("Kconfig.projbuild", "CMakeLists.txt")]
    names += [str(path.relative_to(root)) for path in sorted((root / "rx_release").iterdir()) if path.is_file()]
    return sorted(names)


def snapshot(root, output):
    destination = output / "source"
    destination.mkdir()
    hashes = {}
    for name in source_files(root):
        source = root / name
        if source.is_symlink():
            raise RuntimeError(f"Source symlink requires explicit review: {name}")
        before = source.stat()
        first = source.read_bytes()
        second = source.read_bytes()
        after = source.stat()
        identity = lambda s: (s.st_dev, s.st_ino, s.st_size, s.st_mtime_ns, s.st_ctime_ns)
        if first != second or identity(before) != identity(after):
            (output / "source-read-first.bin").write_bytes(first)
            (output / "source-read-second.bin").write_bytes(second)
            write_json(output / "SOURCE-READ-FAILURE.json", {"path": name, "first_sha256": sha(first),
                       "second_sha256": sha(second), "before": identity(before), "after": identity(after)})
            raise RuntimeError("Source changed or read buffers differ; stopped without retry")
        target = destination / name
        target.parent.mkdir(parents=True, exist_ok=True)
        with target.open("xb") as file:
            file.write(first)
        observed_copy = target.read_bytes()
        if observed_copy != first:
            (output / "copy-expected.bin").write_bytes(first)
            (output / "copy-observed.bin").write_bytes(observed_copy)
            write_json(output / "COPY-FAILURE.json", {"path": name,
                       "expected_sha256": sha(first), "observed_sha256": sha(observed_copy)})
            raise RuntimeError(f"Snapshot copy differs; stopped without retry: {name}")
        hashes[name] = sha(first)
    manifest_bytes = json.dumps(hashes, sort_keys=True, separators=(",", ":")).encode()
    write_json(output / "SOURCE-MANIFEST.json", {"schema": "observatory.rx-sources.v1",
               "files": hashes, "source_sha256": sha(manifest_bytes)})
    return hashes, sha(manifest_bytes)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path, help="New external directory; existing paths are refused")
    args = parser.parse_args()
    root = Path(__file__).resolve().parent
    output = args.output.expanduser().resolve()
    if output == root or root in output.parents:
        parser.error("Use a new directory outside the firmware source tree")
    sdk_path = os.environ.get("IDF_PATH")
    if not sdk_path:
        parser.error("Source the pinned ESP-IDF 5.4 export.sh first")
    sdk = Path(sdk_path).resolve()
    commit = subprocess.check_output(["git", "-C", str(sdk), "rev-parse", "HEAD"], text=True).strip()
    dirty = subprocess.check_output(["git", "-C", str(sdk), "status", "--porcelain", "--untracked-files=no"], text=True)
    if commit != SDK_COMMIT or dirty:
        parser.error("SDK must be the clean pinned ESP-IDF 5.4 commit")
    output.mkdir(mode=0o700)  # No overwrite, resume, reset or automatic retry.
    hashes, source_sha = snapshot(root, output)
    license_source = root.parents[1] / "LICENSE"
    (output / "LICENSE").write_bytes(license_source.read_bytes())
    env = dict(os.environ, IDF_COMPONENT_CACHE_PATH=str(output / "component-cache"),
               TMPDIR=str(output / "tmp"), SOURCE_DATE_EPOCH="0")
    (output / "tmp").mkdir()
    app_hashes = []
    for label in ("build-a", "build-b"):
        command = [sys.executable, str(sdk / "tools" / "idf.py"), "-C", str(output / "source"),
                   "-B", str(output / label), "-D", f"SDKCONFIG={output / 'sdkconfig'}",
                   "-D", "SDKCONFIG_DEFAULTS=sdkconfig.defaults;sdkconfig.defaults.rx-unified",
                   "-D", "OBSERVATORY_RX_RELEASE=ON", "-D", f"RX_SOURCE_SHA256={source_sha}", "build"]
        with (output / f"{label}.log").open("xb") as log:
            subprocess.run(command, env=env, stdout=log, stderr=subprocess.STDOUT, check=True)
        for name, expected in hashes.items():
            if sha((output / "source" / name).read_bytes()) != expected:
                raise RuntimeError(f"Build changed a bound source; stopped: {name}")
        app = (output / label / "esp32-csi-node.bin").read_bytes()
        if len(app) < 24 or len(app) > APP_SLOT_BYTES or app[0] != 0xE9 or app[3] >> 4 != 3:
            raise RuntimeError("App size/image header does not match the 2-MiB slot / 8-MiB layout")
        app_hashes.append(sha(app))
    if app_hashes[0] != app_hashes[1]:
        raise RuntimeError("Independent builds differ; both results preserved, no retry")
    with (output / "RESOURCE-REPORT.log").open("xb") as log:
        subprocess.run([sys.executable, str(sdk / "tools" / "idf.py"), "-C", str(output / "source"),
                        "-B", str(output / "build-a"), "size"], env=env, stdout=log,
                       stderr=subprocess.STDOUT, check=True)
    build_description = json.loads((output / "build-a" / "project_description.json").read_text())
    compiler = Path(build_description["c_compiler"])
    if build_description.get("target") != "esp32s3":
        raise RuntimeError("Release target must be ESP32-S3")
    from esptool.bin_image import LoadFirmwareImage
    parsed_image = LoadFirmwareImage("esp32s3", str(output / "build-a" / "esp32-csi-node.bin"))
    if not parsed_image.append_digest or parsed_image.stored_digest != parsed_image.calc_digest:
        raise RuntimeError("SDK image digest verification failed")
    (output / "rx-unified.bin").write_bytes((output / "build-a" / "esp32-csi-node.bin").read_bytes())
    config = (output / "sdkconfig").read_text()
    for setting in ("CONFIG_RX_UNIFIED=y", "CONFIG_HEAP_POISONING_COMPREHENSIVE=y",
                    "CONFIG_APP_REPRODUCIBLE_BUILD=y", "CONFIG_FREERTOS_CHECK_STACKOVERFLOW_CANARY=y",
                    "CONFIG_COMPILER_STACK_CHECK_MODE_STRONG=y", "CONFIG_CSI_SELF_PING_ENABLE=y",
                    "CONFIG_CSI_SELF_PING_INTERVAL_MS=20"):
        if setting not in config.splitlines():
            raise RuntimeError(f"Required release setting missing: {setting}")
    write_json(output / "RELEASE.json", {"schema": "observatory.rx-offline-build.v1",
               "source_sha256": source_sha, "app_sha256": app_hashes[0], "sdk_commit": commit,
               "image_sha256": parsed_image.stored_digest.hex(),
               "target": "esp32s3", "app_version": "rx-unified-1", "installation_scope": "app-only",
               "partition_table_sha256": sha((output / "build-a" / "partition_table" / "partition-table.bin").read_bytes()),
               "compiler_sha256": sha(compiler.read_bytes()),
               "compiler_version": subprocess.check_output([str(compiler), "--version"], text=True).splitlines()[0],
               "sdkconfig_sha256": sha(config.encode()), "app_bytes": len(app), "app_slot_bytes": APP_SLOT_BYTES,
               "profiles": [{"node_id": node, "app_sha256": app_hashes[0]} for node in range(1, 5)],
               "reproduced_builds": 2, "wire_version": 4, "hardware_validated": False,
               "installation_authorized": False})
    print(f"Offline app reproduced: {output / 'rx-unified.bin'} ({len(app)} bytes)")


if __name__ == "__main__":
    main()

#!/usr/bin/env python3.15
"""Assemble explicit public TX sources and build locally; never flash or use devices."""
from __future__ import annotations
lazy import argparse
lazy import hashlib
lazy import json
lazy import os
lazy from pathlib import Path
lazy import shutil
lazy import subprocess
lazy import sys
lazy from importlib import metadata

TX = Path(__file__).resolve().parents[1]
EXPECTED_IDF_COMMIT = "67c1de1eebe095d554d281952fde63c16ee2dca0"


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def validate_boot_diagnostics_config(config):
    """Keep early/panic output observable; never mask brownout or enable PSRAM."""
    required = {"ESP_CONSOLE_UART_DEFAULT": True,
                "ESP_CONSOLE_UART_NUM": 0,
                "ESP_CONSOLE_UART_BAUDRATE": 115200,
                "ESP_CONSOLE_SECONDARY_USB_SERIAL_JTAG": True,
                "ESP_SYSTEM_PANIC_PRINT_REBOOT": True,
                "ESP_SYSTEM_PANIC_REBOOT_DELAY_SECONDS": 3,
                "ESP_MAIN_TASK_STACK_SIZE": 8192,
                "ESP_BROWNOUT_DET": True,
                "ESP_TASK_WDT_EN": True, "ESP_TASK_WDT_INIT": True,
                "ESP_TASK_WDT_TIMEOUT_S": 10}
    if any(config.get(key) != value for key, value in required.items()):
        raise ValueError("boot diagnostic console/panic/watchdog configuration differs")
    if config.get("SPIRAM") or config.get("ESP_SYSTEM_PANIC_SILENT_REBOOT"):
        raise ValueError("boot diagnostic build must not require PSRAM or reboot silently")


def public_sources(root=TX):
    names = ["CMakeLists.txt", "sdkconfig.defaults", "partitions.csv", "version.txt", "README.md",
             "provision.py", "esp32-csi-tx.ino", "tx_packet.h", "tx_config.example.h"]
    for directory in ("main", "test", "tools"):
        for p in sorted((root/directory).rglob("*")):
            if p.is_file() and not p.is_symlink() and p.suffix in (".c", ".h", ".cpp", ".py", ".sh", ".yml", ".md", ".json"):
                names.append(str(p.relative_to(root)))
    names.extend(["main/CMakeLists.txt", "main/Kconfig.projbuild"])
    return sorted(set(names))


def assemble(output: Path):
    output.mkdir(parents=True, exist_ok=False)
    source = output/"source";source.mkdir()
    for name in public_sources():
        destination=source/name;destination.parent.mkdir(parents=True,exist_ok=True)
        shutil.copyfile(TX/name,destination)
    if not (source/"main/tx_config.h").is_file() or (source/"tx_config.h").exists():
        raise ValueError("public header contract violated")
    # Use the already installed, pinned component. No broad upstream update.
    lock=TX/"dependencies.lock"
    if not lock.is_file(): raise ValueError("resolved component lock missing")
    import yaml
    resolved=yaml.safe_load(lock.read_text())
    if (resolved["dependencies"]["espressif/led_strip"]["version"]!="3.0.3"
            or resolved["dependencies"]["idf"]["version"]!="5.4.0"):
        raise ValueError("unreviewed dependency resolution")
    shutil.copyfile(lock,source/"dependencies.lock")
    component=TX/"managed_components/espressif__led_strip"
    if not component.is_dir(): raise ValueError("pinned led_strip is not cached locally")
    shutil.copytree(component,source/"managed_components/espressif__led_strip")
    hashes={str(p.relative_to(source)):sha(p) for p in sorted(source.rglob("*")) if p.is_file()}
    try:
        git_commit=subprocess.check_output(["git","-C",str(TX),"rev-parse","HEAD"],text=True).strip()
    except subprocess.CalledProcessError as error:
        raise ValueError("a Git checkout with commit history is required for a release build") from error
    git_hash=git_commit[:12]
    identity_input={"git_commit":git_commit,"files":hashes}
    identity=hashlib.sha256(json.dumps(identity_input,sort_keys=True,separators=(",",":")).encode()).hexdigest()
    (source/"source_identity.cmake").write_text(
        f'set(TX_GIT_HASH "{git_hash}")\nset(TX_SOURCE_SHA "{identity}")\n')
    (output/"sources.json").write_text(
        json.dumps({"source_sha256":identity,"git_commit":git_commit,"files":hashes},indent=2)+"\n")
    return source


def main(argv=None):
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output",type=Path,required=True,help="new artifact directory, never an existing checkout")
    parser.add_argument("--build",action="store_true",help="run idf.py build only, using activated IDF environment")
    args=parser.parse_args(argv)
    try:
        source=assemble(args.output.resolve())
        if not args.build:
            print("Public source export prepared; no build/device operation.")
            return 0
        idf=Path(os.environ["IDF_PATH"])
        if subprocess.check_output(["git","-C",str(idf),"describe","--tags","--always"],text=True).strip()!="v5.4":
            raise ValueError("this profile requires the reviewed ESP-IDF v5.4")
        idf_commit=subprocess.check_output(["git","-C",str(idf),"rev-parse","HEAD"],text=True).strip()
        if idf_commit!=EXPECTED_IDF_COMMIT:
            raise ValueError("this profile requires the documented ESP-IDF v5.4 commit")
        if subprocess.check_output(["git","-C",str(idf),"status","--porcelain"],text=True).strip():
            raise ValueError("SDK source differs from its documented commit")
        # SDK dependencies use their installed interpreter; the exporter uses 3.15.
        idf_python=Path(os.environ["IDF_PYTHON_ENV_PATH"])/"bin/python"
        command=[str(idf_python),str(idf/"tools/idf.py"),"-B",str(args.output.resolve()/"build"),
                 "-D",f"EXTRA_COMPONENT_DIRS={source/'managed_components/espressif__led_strip'}","build"]
        with (args.output/"build.log").open("w") as log:
            result=subprocess.run(command,cwd=source,stdout=log,stderr=subprocess.STDOUT,timeout=600)
        if result.returncode: return result.returncode
        build=args.output/"build"
        exported=json.loads((args.output/"sources.json").read_text())
        if any(sha(source/name)!=checksum for name,checksum in exported["files"].items()):
            raise ValueError("exported build input changed during build")
        config=json.loads((build/"config/sdkconfig.json").read_text())
        from tx1_usb import app_image, partitions, LAYOUT, validate_build_config
        validate_build_config(config)
        validate_boot_diagnostics_config(config)
        app=build/"esp32-csi-tx.bin"
        if app.stat().st_size>0x140000: raise ValueError("candidate exceeds old app1")
        image=app_image(app.read_bytes())
        table=partitions((build/"partition_table/partition-table.bin").read_bytes().ljust(4096,b"\xff"))
        if {k:tuple(v[x] for x in ("type","subtype","offset","size")) for k,v in table.items()}!=LAYOUT:
            raise ValueError("compiled partition table differs from migration profile")
        receipt={"idf":"v5.4","idf_commit":idf_commit,
                 "source_sha256":exported["source_sha256"],"idf_dirty":False,
                 "reproducible_build":config.get("APP_REPRODUCIBLE_BUILD") is True,
                 "sdk_tools":json.loads(subprocess.check_output([
                     str(idf_python),"-c",
                     'import json; from importlib.metadata import version; '
                     'print(json.dumps({name:version(name) for name in '
                     '("esptool","esp-idf-nvs-partition-gen","idf-component-manager")}))'],text=True)),
                 "component_lock_sha256":sha(source/"dependencies.lock"),"led_strip":"3.0.3",
                 "runtime_image_sha256":image.calc_digest.hex(),
                 "image_header":{"chip_id":image.chip_id,"flash_mode":image.flash_mode,"flash_size_freq":image.flash_size_freq},
                 "partition_contract":table,
                 "python":sys.version,
                 "sdk_python":subprocess.check_output([str(idf_python),"--version"],text=True).strip(),
                 "compiler":subprocess.check_output(["xtensa-esp32s3-elf-gcc","--version"],text=True).splitlines()[0],
                 "candidate":{"file":"build/esp32-csi-tx.bin","sha256":sha(app),"size":app.stat().st_size,
                              "intended_offset":0x150000,"slot_size":0x140000},
                 "artifacts":{name:sha(build/name) for name in ["esp32-csi-tx.bin","esp32-csi-tx.elf","partition_table/partition-table.bin","config/sdkconfig.json"]},
                 "hardware_validated":False,"write_authorized":False,
                 "next_hardware_run":"instrumented boot diagnosis; not hardware acceptance",
                 "boot_diagnostics":{"primary_console":"uart0",
                                     "secondary_console":"usb_serial_jtag",
                                     "panic_reboot_delay_seconds":3,
                                     "main_task_stack_bytes":config["ESP_MAIN_TASK_STACK_SIZE"],
                                     "led_main_wait_budget_ms":1000},
                 "bootloader_artifact":"build output only; NOT part of migration write set"}
        (args.output/"build-receipt.json").write_text(json.dumps(receipt,indent=2)+"\n")
        print(f"Built app-only candidate: {app.stat().st_size} bytes; no device access.")
        return 0
    except (OSError,ValueError,KeyError,subprocess.SubprocessError) as error:
        print(f"Build refused/failed: {error}",file=sys.stderr);return 2


if __name__=="__main__":
    raise SystemExit(main())

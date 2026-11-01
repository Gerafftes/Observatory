#!/usr/bin/env python3
"""Compatibility entry point; authoritative provisioning lives with the firmware.

The legacy entry used floating thresholds in signal units. Preserve that CLI
conversion while sharing storage, authentication and write guards.
"""
import importlib.util
from pathlib import Path
import sys

_path = Path(__file__).resolve().parents[1] / "firmware" / "esp32-csi-node" / "provision.py"
_spec = importlib.util.spec_from_file_location("ruview_firmware_provision", _path)
_impl = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_impl)
globals().update({name: getattr(_impl, name) for name in dir(_impl) if not name.startswith("__")})


def main():
    for index, value in enumerate(sys.argv[1:], 1):
        for option in ("--pres-thresh", "--fall-thresh"):
            if value == option and index + 1 < len(sys.argv):
                sys.argv[index + 1] = str(round(float(sys.argv[index + 1]) * 1000))
            elif value.startswith(option + "="):
                sys.argv[index] = option + "=" + str(round(float(value.split("=", 1)[1]) * 1000))
    _impl.main()


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Inspect or update an ESP32 mmWave node over WiFi."""

import argparse
import json
import urllib.request


def request(url: str, method: str = "GET", data: bytes | None = None,
            token: str | None = None,
            content_type: str = "application/octet-stream") -> bytes:
    headers = {}
    if token:
        headers["Authorization"] = f"Bearer {token}"
    if data is not None:
        headers["Content-Type"] = content_type
    req = urllib.request.Request(url, data=data, headers=headers, method=method)
    with urllib.request.urlopen(req, timeout=60) as response:
        return response.read()


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("host", help="Node IP address")
    parser.add_argument("--token")
    action = parser.add_mutually_exclusive_group(required=True)
    action.add_argument("--status", action="store_true")
    action.add_argument("--mode", choices=("calibration", "reference"))
    action.add_argument(
        "--transform",
        nargs=4,
        metavar=("ORIGIN_X_MM", "ORIGIN_Z_MM", "YAW_MDEG", "INVERT_RAW_X"),
        help="persist the room transform; INVERT_RAW_X is true or false",
    )
    action.add_argument("--firmware")
    args = parser.parse_args()
    base_url = f"http://{args.host}:8032"
    if args.status:
        result = request(f"{base_url}/ota/status")
    elif args.mode:
        if not args.token:
            parser.error("--token is required for a mode change")
        result = request(f"{base_url}/mode", "PUT", args.mode.encode(), args.token)
    elif args.transform:
        if not args.token:
            parser.error("--token is required for a transform change")
        origin_x, origin_z, yaw, invert = args.transform
        if invert not in ("true", "false"):
            parser.error("INVERT_RAW_X must be true or false")
        payload = json.dumps({
            "origin_x_mm": int(origin_x),
            "origin_z_mm": int(origin_z),
            "yaw_mdeg": int(yaw),
            "raw_x_inverted": invert == "true",
        }).encode()
        result = request(
            f"{base_url}/transform",
            "PUT",
            payload,
            args.token,
            "application/json",
        )
    else:
        if not args.token:
            parser.error("--token is required for OTA")
        with open(args.firmware, "rb") as firmware:
            result = request(f"{base_url}/ota", "POST", firmware.read(), args.token)
    print(json.dumps(json.loads(result), indent=2))


if __name__ == "__main__":
    main()

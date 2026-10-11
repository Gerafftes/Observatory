#!/usr/bin/env python3.15
"""Offline-only, additive NVS import. No serial/network code and no secret CLI args."""
from __future__ import annotations
lazy import argparse
lazy import io
lazy import json
lazy import os
lazy from pathlib import Path
lazy import re
lazy import struct
lazy import sys


def parse_mac(value: str) -> bytes:
    if not isinstance(value, str) or not re.fullmatch(r"(?:[0-9a-fA-F]{2}:){5}[0-9a-fA-F]{2}", value):
        raise ValueError("MAC must contain six colon-separated octets")
    result = bytes.fromhex(value.replace(":", ""))
    if not any(result) or result[0] & 1:
        raise ValueError("MAC must be nonzero unicast")
    return result


def validate(p: dict) -> None:
    if not isinstance(p, dict): raise ValueError("profile must be a JSON object")
    required = {"schema_v", "node_id", "ssid", "password", "base_mac", "ap_mac", "channel",
                "rate_hz", "bw_mhz", "protocol", "secondary", "beacon_tu", "country",
                "country_nch", "led_gpio", "led_active_low", "ota_psk"}
    if set(p) != required:
        raise ValueError("profile must contain exactly the documented schema-1 fields")
    for key in ("node_id", "ssid", "password", "country", "ota_psk"):
        if not isinstance(p[key], str) or "\0" in p[key]:
            raise ValueError("invalid text field")
    for key in required - {"node_id", "ssid", "password", "base_mac", "ap_mac", "country", "ota_psk"}:
        if type(p[key]) is not int:
            raise ValueError("numeric profile fields must be integers")
    parse_mac(p["base_mac"]); parse_mac(p["ap_mac"])
    if p["schema_v"] != 1 or p["node_id"] != "TX1":
        raise ValueError("unsupported schema or node")
    if not 1 <= len(p["ssid"].encode()) <= 32 or not 8 <= len(p["password"].encode()) <= 63:
        raise ValueError("SSID/password byte length invalid; open AP is not supported")
    if not 1 <= p["channel"] <= p["country_nch"] <= 13 or not 1 <= p["rate_hz"] <= 100:
        raise ValueError("channel/rate/country range invalid")
    if not re.fullmatch("[A-Z]{2}", p["country"]) or not re.fullmatch("[0-9a-f]{64}", p["ota_psk"]):
        raise ValueError("country or OTA secret format invalid")
    if p["protocol"] not in range(1, 8) or p["bw_mhz"] not in (20, 40):
        raise ValueError("unsupported PHY profile")
    if (p["bw_mhz"] == 20 and p["secondary"] != 0) or (p["bw_mhz"] == 40 and (
            not p["protocol"] & 4 or p["secondary"] not in (1, 2))):
        raise ValueError("bandwidth/secondary channel mismatch")
    if ((p["secondary"] == 1 and p["channel"] + 4 > p["country_nch"])
            or (p["secondary"] == 2 and p["channel"] < 5)):
        raise ValueError("secondary channel outside country range")
    if not 100 <= p["beacon_tu"] <= 60000 or p["beacon_tu"] % 100:
        raise ValueError("invalid beacon interval")
    if not -1 <= p["led_gpio"] <= 48 or p["led_active_low"] not in (0, 1):
        raise ValueError("invalid LED configuration")


def rows(p: dict) -> list[tuple]:
    validate(p)
    result = [("tx_cfg", "namespace", "", "")]
    for key in ("node_id", "ssid", "password", "base_mac", "ap_mac", "channel", "rate_hz",
                "bw_mhz", "protocol", "secondary", "beacon_tu", "country", "country_nch",
                "led_gpio", "led_active_low"):
        value = p[key]
        encoding = "u8"
        if key in ("node_id", "ssid", "password", "country"):
            encoding = "string"
        elif key in ("base_mac", "ap_mac"):
            encoding, value = "hex2bin", parse_mac(value).hex()
        elif key in ("rate_hz", "beacon_tu"):
            encoding = "u16"
        elif key == "led_gpio" and value == -1:
            value = 255
        result.append((key, "data", encoding, str(value)))
    result.extend([("security", "namespace", "", ""),
                   ("ota_psk", "data", "string", p["ota_psk"])])
    # schema_v is emitted separately into tx_cfg LAST, after all required data.
    return result


def sdk_modules(idf: Path):
    from esp_idf_nvs_partition_gen import nvs_partition_gen as gen
    parser_path = idf / "components/nvs_flash/nvs_partition_tool"
    if not (parser_path / "nvs_parser.py").is_file():
        raise ValueError("IDF NVS parser unavailable")
    import importlib.util
    spec = importlib.util.spec_from_file_location("tx_nvs_parser", parser_path / "nvs_parser.py")
    parser = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(parser)
    return gen, parser


def inspect_nvs(raw: bytes, parser):
    if len(raw) != 0x5000:
        raise ValueError("migration NVS must be exactly 0x5000 bytes")
    partition = parser.NVS_Partition("nvs", bytearray(raw))
    namespaces, live = {}, []
    for page in partition.pages:
        if page.is_empty:
            if raw[page.start_address:page.start_address + 4096] != b"\xff" * 4096:
                raise ValueError("non-erased empty page needs recovery")
            continue
        header = page.header
        if header["status"] not in ("Active", "Full") or header["version"] != 2:
            raise ValueError("unsupported NVS page state/version")
        if header["crc"]["original"] != header["crc"]["computed"]:
            raise ValueError("NVS page CRC mismatch")
        for entry in page.entries:
            if entry.state not in ("Written", "Erased", "Empty"):
                raise ValueError("invalid entry bitmap")
            if entry.state != "Written":
                continue
            crc = entry.metadata["crc"]
            if entry.key is None or crc["original"] != crc["computed"]:
                raise ValueError("NVS entry CRC/key invalid")
            if entry.metadata["type"] in ("string", "blob", "blob_data") and (
                    crc["data_original"] != crc["data_computed"]):
                raise ValueError("NVS variable data CRC mismatch")
            if any(child.state != "Written" for child in entry.children):
                raise ValueError("incomplete NVS entry")
            if entry.metadata["namespace"] == 0:
                if entry.metadata["type"] != "uint8_t" or entry.key in namespaces:
                    raise ValueError("ambiguous namespace")
                namespaces[entry.key] = entry.data["value"]
            live.append(bytes(entry.raw) + b"".join(bytes(x.raw) for x in entry.children))
    return partition, namespaces, sorted(live)


def merge_nvs(raw: bytes, profile: dict, idf: Path) -> bytes:
    """Recycle only a fully obsolete page; never move or rewrite live entries.

    ponytail: updates/compaction are intentionally refused. Add them only with a
    separately specified update protocol and SDK-level interruption tests.
    """
    validate(profile)
    gen, parser = sdk_modules(idf)
    partition, namespaces, live_before = inspect_nvs(raw, parser)
    if "tx_cfg" in namespaces or "security" in namespaces:
        raise ValueError("target namespaces already exist; no implicit overwrite/recovery")
    if len(set(namespaces.values())) != len(namespaces) or max(namespaces.values(), default=0) > 251:
        raise ValueError("namespace indices invalid/exhausted")
    spare = [p for p in partition.pages if p.is_empty]
    if not spare:
        raise ValueError("no erased reserve page; preserve input and stop")
    retired = [p for p in partition.pages if not p.is_empty
               and all(e.state != "Written" for e in p.entries)]
    if retired:
        target = retired[0]
    elif len(spare) >= 2:
        target = spare[0]
    else:
        raise ValueError("no reusable page without moving live data")
    sequence = max((p.header["page_index"] for p in partition.pages if not p.is_empty), default=0) + 1
    if sequence >= 0xffffffff:
        raise ValueError("NVS page sequence wrap requires separate review")
    nvs = gen.NVS(io.BytesIO(), 4096, gen.Page.VERSION2, read_only=True)
    nvs.namespace_count = max(namespaces.values(), default=0)
    tx_namespace = nvs.namespace_count + 1
    for key, kind, encoding, value in rows(profile):
        if kind == "namespace": nvs.write_namespace(key)
        else: nvs.write_entry(key, value, encoding)
    nvs.write_entry("schema_v", "1", "u8", namespace_idx=tx_namespace)
    if len(nvs.pages) != 1:
        raise ValueError("profile exceeds single-page import budget")
    nvs.cur_page.set_header(sequence, gen.Page.VERSION2)
    result = bytearray(raw)
    for page in partition.pages:
        if not page.is_empty and page.header["status"] == "Active":
            struct.pack_into("<I", result, page.start_address, gen.Page.FULL)
    result[target.start_address:target.start_address + 4096] = nvs.cur_page.page_buf
    _, after_namespaces, live_after = inspect_nvs(bytes(result), parser)
    if any(entry not in live_after for entry in live_before) or any(
            after_namespaces.get(key) != value for key, value in namespaces.items()):
        raise ValueError("existing logical data changed")
    return bytes(result)


def read_private(path: Path) -> bytes:
    if path.is_symlink() or not path.is_file() or path.stat().st_mode & 0o077:
        raise ValueError("private input must be a regular file with mode 0600 or stricter")
    return path.read_bytes()


def write_private(path: Path, content: bytes):
    if not path.parent.is_dir() or path.parent.stat().st_mode & 0o077:
        raise ValueError("output directory must already exist with mode 0700")
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(fd, "wb") as output:
        output.write(content)
        output.flush()
        os.fsync(output.fileno())


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--profile", type=Path, required=True, help="private explicit schema-1 JSON")
    parser.add_argument("--existing-nvs", type=Path, required=True, help="private 0x5000-byte copy")
    parser.add_argument("--output", type=Path, required=True, help="new private merged image; never flashed here")
    parser.add_argument("--idf-path", type=Path, default=Path(os.environ.get("IDF_PATH", "/nonexistent")))
    args = parser.parse_args(argv)
    try:
        profile = json.loads(read_private(args.profile))
        result = merge_nvs(read_private(args.existing_nvs), profile, args.idf_path)
        write_private(args.output, result)
    except (OSError, ValueError, ImportError, RuntimeError) as error:
        # SDK exceptions may contain data; do not print their payload.
        print("Import refused; input unchanged. Check private permissions, schema, SDK and NVS capacity.", file=sys.stderr)
        return 2
    print("Created private merged NVS image; no device access.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

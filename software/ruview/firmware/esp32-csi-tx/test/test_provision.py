lazy import importlib.util
lazy import io
lazy import json
lazy import os
lazy from pathlib import Path
lazy import struct
lazy import tempfile
lazy import unittest
lazy from unittest.mock import patch

MODULE_PATH = Path(__file__).resolve().parents[1] / "provision.py"
SPEC = importlib.util.spec_from_file_location("tx_provision", MODULE_PATH)
tx_provision = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(tx_provision)
IDF = Path(os.environ.get("IDF_PATH", "/nonexistent"))


def profile(**changes):
    p = dict(schema_v=1, node_id="TX1", ssid="fixture-network", password="12345678",
             base_mac="02:11:22:33:44:55", ap_mac="02:11:22:33:44:56", channel=6,
             rate_hz=50, bw_mhz=20, protocol=7, secondary=0, beacon_tu=100,
             country="DE", country_nch=13, led_gpio=48, led_active_low=0, ota_psk="a"*64)
    p.update(changes)
    return p


def old_nvs():
    gen, _ = tx_provision.sdk_modules(IDF)
    out = io.BytesIO()
    with gen.NVS(out, 0x4000, gen.Page.VERSION2) as nvs:
        nvs.write_namespace("old_app")
        nvs.write_entry("private_text", "preserve this exactly", "string")
        nvs.write_entry("old_blob", "001122334455", "hex2bin")
    return out.getvalue()


class ProvisionTests(unittest.TestCase):
    def test_emits_explicit_identities_and_led(self):
        rows = tx_provision.rows(profile())
        self.assertIn(("base_mac", "data", "hex2bin", "021122334455"), rows)
        self.assertIn(("ap_mac", "data", "hex2bin", "021122334456"), rows)
        self.assertIn(("led_gpio", "data", "u8", "255"), tx_provision.rows(profile(led_gpio=-1)))

    def test_invalid_profiles(self):
        for change in [dict(schema_v=0), dict(node_id="TX2"), dict(ota_psk="ABC"),
                       dict(base_mac="00:00:00:00:00:00"), dict(ap_mac="01:11:22:33:44:55"),
                       dict(password=""), dict(ssid="ü"*17), dict(channel=0), dict(rate_hz=0),
                       dict(secondary=1), dict(bw_mhz=40), dict(protocol=8), dict(country=""),
                       dict(beacon_tu=99), dict(led_gpio=49), dict(rate_hz=True), dict(password="x\0yyyyyyy")]:
            with self.subTest(change=list(change)), self.assertRaises(ValueError):
                tx_provision.validate(profile(**change))
        p = profile(); del p["ssid"]
        with self.assertRaises(ValueError): tx_provision.validate(p)

    def test_merge_preserves_all_old_entries_and_reserve(self):
        with patch("socket.socket", side_effect=AssertionError("no network")):
            before = old_nvs()
            merged = tx_provision.merge_nvs(before, profile(), IDF)
        _, parser = tx_provision.sdk_modules(IDF)
        _, namespaces, old = tx_provision.inspect_nvs(before, parser)
        partition, after_namespaces, after = tx_provision.inspect_nvs(merged, parser)
        self.assertTrue(all(item in after for item in old))
        self.assertEqual(namespaces["old_app"], after_namespaces["old_app"])
        self.assertTrue(any(p.is_empty for p in partition.pages))
        self.assertEqual(len(merged), 0x5000)
        tx_ns = after_namespaces["tx_cfg"]
        markers = [e for p in partition.pages for e in p.entries if e.state == "Written"
                   and e.metadata["namespace"] == tx_ns and e.key == "schema_v"]
        self.assertEqual(len(markers), 1)
        self.assertEqual(markers[0].data["value"], 1)
        with self.assertRaises(ValueError): tx_provision.merge_nvs(merged, profile(), IDF)

    def test_crc_and_insufficient_capacity_refuse_without_mutation(self):
        before = old_nvs(); bad = bytearray(before); bad[28] ^= 1
        with self.assertRaises(ValueError): tx_provision.merge_nvs(bytes(bad), profile(), IDF)
        gen, _ = tx_provision.sdk_modules(IDF)
        one_page = before[:4096]
        # All five pages contain live entries: no reserve, no safe recycling.
        full = one_page * 5
        with self.assertRaises(ValueError): tx_provision.merge_nvs(full, profile(), IDF)
        self.assertEqual(before, old_nvs())

    def test_cli_permissions_no_clobber_and_no_io_on_help(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);root.chmod(0o700)
            p=root/"profile.json";p.write_text(json.dumps(profile()));p.chmod(0o600)
            n=root/"old.bin";n.write_bytes(old_nvs());n.chmod(0o600)
            out=root/"new.bin"
            args=["--profile",str(p),"--existing-nvs",str(n),"--output",str(out),"--idf-path",str(IDF)]
            with patch("socket.socket",side_effect=AssertionError("no network")):
                self.assertEqual(tx_provision.main(args),0)
                saved=out.read_bytes();self.assertEqual(tx_provision.main(args),2)
                self.assertEqual(saved,out.read_bytes())
                with self.assertRaises(SystemExit) as result: tx_provision.main(["--help"])
                self.assertEqual(result.exception.code,0)
            self.assertEqual(out.stat().st_mode&0o777,0o600)


if __name__ == "__main__":
    unittest.main()

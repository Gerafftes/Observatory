"""Tests for provision.py's additive-by-default merge behaviour (#391, #574)."""

from __future__ import annotations

import argparse
import json
import os
import sys
import tempfile
import unittest

# Allow `python -m unittest` from anywhere in the repo.
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))

import provision  # noqa: E402  — sibling import after sys.path tweak


def _mk_args(**overrides) -> argparse.Namespace:
    """Build a Namespace with every mergeable attr set to None unless overridden."""
    base = {name: None for name in provision.MERGEABLE_ATTRS}
    base.update(overrides)
    return argparse.Namespace(**base)


class TestStateFile(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.mkdtemp(prefix="provision-state-")

    def tearDown(self):
        import shutil
        shutil.rmtree(self.dir, ignore_errors=True)

    def test_load_state_empty_when_missing(self):
        self.assertEqual(provision.load_state("COM7", self.dir), {})

    def test_save_then_load_roundtrip(self):
        provision.save_state("COM7", self.dir, {"ssid": "x", "password": "y"})
        self.assertEqual(
            provision.load_state("COM7", self.dir),
            {"ssid": "x", "password": "y"},
        )
        if os.name != "nt":
            mode = os.stat(provision._state_path_for("COM7", self.dir)).st_mode & 0o777
            self.assertEqual(mode, 0o600)

    def test_save_never_persists_raw_ota_psk(self):
        provision.save_state(
            "COM7",
            self.dir,
            {
                "ssid": "x",
                "ota_psk": "ab" * 32,
                provision.OTA_PSK_STATE_MARKER: True,
            },
        )

        loaded = provision.load_state("COM7", self.dir)
        self.assertNotIn("ota_psk", loaded)
        self.assertTrue(loaded[provision.OTA_PSK_STATE_MARKER])

    def test_save_creates_per_port_files(self):
        provision.save_state("COM7", self.dir, {"ssid": "a"})
        provision.save_state("/dev/ttyUSB0", self.dir, {"ssid": "b"})
        self.assertEqual(provision.load_state("COM7", self.dir), {"ssid": "a"})
        self.assertEqual(provision.load_state("/dev/ttyUSB0", self.dir), {"ssid": "b"})

    def test_load_state_handles_corrupt_json(self):
        path = provision._state_path_for("COM7", self.dir)
        os.makedirs(self.dir, exist_ok=True)
        with open(path, "w", encoding="utf-8") as f:
            f.write("{not valid json")
        # Should warn but not raise.
        self.assertEqual(provision.load_state("COM7", self.dir), {})


class TestMerge(unittest.TestCase):
    def test_cli_wins_over_prior(self):
        args = _mk_args(ssid="new-ssid")
        prior = {"ssid": "old-ssid", "password": "abc"}
        merged = provision.merge_state_into_args(args, prior)
        self.assertEqual(args.ssid, "new-ssid")  # CLI value preserved
        self.assertEqual(args.password, "abc")    # filled from prior
        self.assertEqual(merged["ssid"], "new-ssid")
        self.assertEqual(merged["password"], "abc")

    def test_prior_fills_missing_cli(self):
        args = _mk_args()  # all None
        prior = {
            "ssid": "MyWiFi",
            "password": "secret",
            "target_ip": "192.168.1.20",
            "node_id": 3,
        }
        merged = provision.merge_state_into_args(args, prior)
        self.assertEqual(args.ssid, "MyWiFi")
        self.assertEqual(args.password, "secret")
        self.assertEqual(args.target_ip, "192.168.1.20")
        self.assertEqual(args.node_id, 3)
        for key, val in prior.items():
            self.assertEqual(merged[key], val)

    def test_partial_invocation_does_not_drop_unrelated_keys(self):
        # The exact #391 scenario: user previously provisioned WiFi, now adds
        # only --seed-url. Old behaviour wiped SSID. New behaviour keeps it.
        args = _mk_args(seed_url="http://10.1.10.236")
        prior = {
            "ssid": "ruv.net",
            "password": "<secret>",
            "target_ip": "192.168.1.20",
        }
        merged = provision.merge_state_into_args(args, prior)
        self.assertEqual(args.ssid, "ruv.net")
        self.assertEqual(args.password, "<secret>")
        self.assertEqual(args.target_ip, "192.168.1.20")
        self.assertEqual(args.seed_url, "http://10.1.10.236")
        # And the on-disk merged dict carries all four keys.
        self.assertEqual(set(merged.keys()),
                         {"ssid", "password", "target_ip", "seed_url"})

    def test_empty_prior_is_noop(self):
        args = _mk_args(ssid="x")
        merged = provision.merge_state_into_args(args, {})
        self.assertEqual(merged, {"ssid": "x"})

    def test_falsy_but_not_none_cli_value_overrides_prior(self):
        # node_id=0 is a legal value; must NOT be replaced by prior["node_id"]=5.
        args = _mk_args(node_id=0)
        prior = {"node_id": 5}
        merged = provision.merge_state_into_args(args, prior)
        self.assertEqual(args.node_id, 0)
        self.assertEqual(merged["node_id"], 0)


class TestOtaProvisioning(unittest.TestCase):
    def test_ota_psk_is_written_to_security_namespace(self):
        ota_psk = "ab" * 32
        args = _mk_args(ota_psk=ota_psk)

        csv_content = provision.build_nvs_csv(args)

        self.assertIn("security,namespace,,", csv_content)
        self.assertIn(f"ota_psk,data,string,{ota_psk}", csv_content)

    def test_ota_psk_counts_as_configuration(self):
        self.assertTrue(provision.has_config_value(_mk_args(ota_psk="ab" * 32)))

    def test_ota_psk_is_not_persisted_in_port_state(self):
        self.assertNotIn("ota_psk", provision.MERGEABLE_ATTRS)

    def test_known_ota_psk_cannot_be_silently_dropped(self):
        args = _mk_args()
        args.clear_ota_psk = False
        args.confirm_no_ota_psk = False
        self.assertIsNotNone(
            provision.ota_preservation_error(
                args, {provision.OTA_PSK_STATE_MARKER: True}
            )
        )
        args.ota_psk = "ab" * 32
        self.assertIsNone(
            provision.ota_preservation_error(
                args, {provision.OTA_PSK_STATE_MARKER: True}
            )
        )

    def test_old_unknown_state_requires_explicit_ota_answer(self):
        args = _mk_args()
        args.clear_ota_psk = False
        args.confirm_no_ota_psk = False
        prior = {"ssid": "existing"}
        self.assertIsNotNone(provision.ota_preservation_error(args, prior))
        args.confirm_no_ota_psk = True
        self.assertIsNone(provision.ota_preservation_error(args, prior))

    def test_missing_state_requires_explicit_ota_answer(self):
        args = _mk_args()
        args.clear_ota_psk = False
        args.confirm_no_ota_psk = False

        error = provision.ota_preservation_error(args, {})

        self.assertIsNotNone(error)
        self.assertIn("no trustworthy port state", error)

    def test_corrupt_state_cannot_silently_authorize_rewrite(self):
        args = _mk_args()
        args.clear_ota_psk = False
        args.confirm_no_ota_psk = False
        with tempfile.TemporaryDirectory(prefix="provision-corrupt-") as directory:
            state_path = provision._state_path_for("COM7", directory)
            with open(state_path, "w", encoding="utf-8") as state_file:
                state_file.write("{not valid json")

            prior = provision.load_state("COM7", directory)

        self.assertEqual(prior, {})
        self.assertIsNotNone(provision.ota_preservation_error(args, prior))

    def test_only_trustworthy_or_explicit_safe_cases_allow_rewrite(self):
        args = _mk_args()
        args.clear_ota_psk = False
        args.confirm_no_ota_psk = False

        self.assertIsNone(
            provision.ota_preservation_error(
                args, {provision.OTA_PSK_STATE_MARKER: False}
            )
        )
        self.assertIsNotNone(
            provision.ota_preservation_error(
                args, {provision.OTA_PSK_STATE_MARKER: "false"}
            )
        )

        args.confirm_no_ota_psk = True
        self.assertIsNone(provision.ota_preservation_error(args, {}))

        args.confirm_no_ota_psk = False
        args.clear_ota_psk = True
        self.assertIsNone(provision.ota_preservation_error(args, {}))

    def test_state_diagnostics_redact_reusable_secrets_and_raw_filter(self):
        state = {
            "password": "wifi-secret",
            "seed_token": "seed-secret",
            "ota_psk": "ota-secret",
            "filter_mac": "00:11:22:33:44:55",
        }
        diagnostic = provision.redacted_state(state)
        self.assertEqual(diagnostic["password"], "(set)")
        self.assertEqual(diagnostic["seed_token"], "(set)")
        self.assertEqual(diagnostic["ota_psk"], "(set)")
        self.assertEqual(diagnostic["filter_mac"], "(set)")
        self.assertNotIn("00:11:22:33:44:55", json.dumps(diagnostic))
        self.assertNotIn("ota-secret", json.dumps(diagnostic))
        self.assertEqual(
            diagnostic["filter_identity_sha256"],
            provision.filter_identity_sha256("00:11:22:33:44:55"),
        )

    def test_state_diagnostics_redact_invalid_legacy_filter_without_crashing(self):
        for invalid in ("not-a-mac", [0, 17, 34, 51, 68, 85]):
            with self.subTest(invalid=invalid):
                diagnostic = provision.redacted_state({"filter_mac": invalid})

                self.assertEqual(diagnostic["filter_mac"], "(set)")
                self.assertEqual(
                    diagnostic["filter_identity_sha256"],
                    "(invalid stored value)",
                )
                self.assertNotIn(str(invalid), json.dumps(diagnostic))


class TestFilterIdentity(unittest.TestCase):
    def test_identity_hashes_exact_nvs_bytes(self):
        self.assertEqual(
            provision.filter_identity_sha256("00:11:22:33:44:55"),
            "48f4634d1002f9f3c7570cb43e00dd869b22c79538e9b4adc7e402de1189cfe1",
        )

    def test_case_does_not_change_binary_identity(self):
        self.assertEqual(
            provision.filter_identity_sha256("AA:BB:CC:DD:EE:FF"),
            provision.filter_identity_sha256("aa:bb:cc:dd:ee:ff"),
        )

    def test_noncanonical_text_is_rejected(self):
        for invalid in ["0:11:22:33:44:55", "00-11-22-33-44-55", "001122334455"]:
            with self.subTest(invalid=invalid):
                with self.assertRaises(ValueError):
                    provision.filter_identity_sha256(invalid)


class TestPrivateArtifacts(unittest.TestCase):
    def test_generated_secret_artifact_is_private_and_no_clobber(self):
        with tempfile.TemporaryDirectory(prefix="provision-private-") as directory:
            path = os.path.join(directory, "artifact.bin")
            provision.write_private_no_clobber(path, b"secret", binary=True)
            with open(path, "rb") as artifact:
                self.assertEqual(artifact.read(), b"secret")
            if os.name != "nt":
                self.assertEqual(os.stat(path).st_mode & 0o777, 0o600)
            with self.assertRaises(FileExistsError):
                provision.write_private_no_clobber(path, b"replacement", binary=True)

    def test_sensitive_config_detection_covers_ota_and_wifi(self):
        self.assertTrue(provision.has_sensitive_config(_mk_args(password="secret")))
        self.assertTrue(provision.has_sensitive_config(_mk_args(seed_token="token")))
        self.assertTrue(provision.has_sensitive_config(_mk_args(ota_psk="ab" * 32)))
        self.assertFalse(provision.has_sensitive_config(_mk_args(ssid="not-secret")))


class TestStatePathSanitization(unittest.TestCase):
    def test_slashes_in_port_are_safe(self):
        path = provision._state_path_for("/dev/ttyUSB0", "/tmp/x")
        # Must not contain a raw slash in the basename
        self.assertNotIn("/", os.path.basename(path))

    def test_windows_com_port_is_safe(self):
        path = provision._state_path_for("COM7", "/tmp/x")
        self.assertTrue(path.endswith("COM7.json"))


if __name__ == "__main__":
    unittest.main()

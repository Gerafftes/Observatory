"""Device-bound configuration, no serial access and no implicit POST retry."""
import io
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import provision

DEVICE_ID = "a" * 64
CAPABILITIES = {"schema": "observatory.rx-capabilities.v1", "wire_version": 4,
                "device_id": DEVICE_ID, "measurement_active": False}
STATE = {"schema": "observatory.rx-config.v1", "active": {"generation": 1, "node_id": 1},
         "stored": {"generation": 1, "node_id": 1}}
RESULT = dict(STATE, stored={"generation": 2, "node_id": 1})


class DeviceConfigTests(unittest.TestCase):
    def responses(self, values):
        self.requests = []

        def open_request(request, timeout):
            self.assertEqual(timeout, 10)
            self.assertEqual(request.get_header("Authorization"), "Bearer test-token")
            self.assertEqual(request.get_header("X-rx-device-id"), DEVICE_ID)
            self.requests.append(request)
            value = values[len(self.requests) - 1]
            if isinstance(value, Exception):
                raise value
            return io.BytesIO(json.dumps(value).encode())
        return patch.object(provision.urllib.request.OpenerDirector, "open", side_effect=open_request)

    def configure(self, **kwargs):
        return provision.device_configure("http://192.0.2.10:8032", DEVICE_ID,
                                          {"target_port": 6000}, "test-token", **kwargs)

    def test_uses_actual_generation_and_returns_stored_active_separately(self):
        with self.responses([CAPABILITIES, STATE, RESULT]):
            self.assertEqual(self.configure(), RESULT)
        self.assertEqual(len(self.requests), 3)
        self.assertEqual(json.loads(self.requests[-1].data),
                         {"expected_generation": 1, "target_port": 6000})

    def test_dry_run_never_posts(self):
        with self.responses([CAPABILITIES, STATE]):
            self.assertFalse(self.configure(dry_run=True)["written"])
        self.assertTrue(all(request.data is None for request in self.requests))

    def test_wrong_physical_device_never_reads_config_or_posts(self):
        with self.responses([dict(CAPABILITIES, device_id="b" * 64)]):
            with self.assertRaisesRegex(ValueError, "identity"):
                self.configure()
        self.assertEqual(len(self.requests), 1)

    def test_active_measurement_never_posts(self):
        with self.responses([dict(CAPABILITIES, measurement_active=True), STATE]):
            with self.assertRaisesRegex(ValueError, "lease"):
                self.configure()
        self.assertEqual(len(self.requests), 2)

    def test_ambiguous_post_is_not_retried(self):
        for response in (TimeoutError("test timeout"), [], {"schema": "wrong"}):
            with self.subTest(response=response):
                with self.responses([CAPABILITIES, STATE, response]):
                    with self.assertRaisesRegex(RuntimeError, "[Nn]o (retry|repeat)"):
                        self.configure()
                self.assertEqual(len(self.requests), 3)

    def test_full_nvs_write_requires_explicit_initial_selection_before_any_work(self):
        with tempfile.TemporaryDirectory() as directory:
            result = subprocess.run([sys.executable, provision.__file__, "--port", "/dev/not-a-device",
                                     "--state-dir", directory, "--ssid", "test"], capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("--initial-provision", result.stderr)
            self.assertEqual(list(Path(directory).iterdir()), [])

    def test_legacy_entry_preserves_threshold_units(self):
        legacy = Path(provision.__file__).resolve().parents[2] / "scripts" / "provision.py"
        with tempfile.TemporaryDirectory() as directory:
            result = subprocess.run([sys.executable, str(legacy), "--port", "COM7", "--state-dir", directory,
                                     "--state", "--fall-thresh", "2.0"], capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(json.loads(result.stdout)["fall_thresh"], 2000)


if __name__ == "__main__":
    unittest.main()

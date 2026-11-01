import hashlib
import os
import subprocess
import importlib.util
from pathlib import Path
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location("publication", Path(__file__).parents[1] / "check_publication.py")
publication = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(publication)


class PublicationBoundaryTests(unittest.TestCase):
    def test_private_state_is_blocked_in_nested_and_tracked_locations(self):
        for name in ["private/evidence.json", "software/vendor/x/.claude/memory.json",
                     "config.env/.env", "backup/result.private.json", "capture.pcapng",
                     "firmware/sdkconfig", "keys/service.pem", "../outside.txt",
                     "software/ruview/data/config.json", "historical-screenshots/private-desktop.png"]:
            with self.subTest(name=name):
                self.assertIsNotNone(publication.path_problem(name))

    def test_runtime_sources_safe_templates_and_notices_remain_publishable(self):
        for name in ["software/ruview/firmware/main/tx_config.h", "config/.env.example",
                     "vendor/crate/src/lib.rs", "vendor/crate/LICENSE", "hardware/pcb-03/PCB-03.kicad_pro",
                     "software/ruview/v2/crates/cog-person-count/cog/artifacts/count_v1.onnx"]:
            self.assertIsNone(publication.path_problem(name))

    def test_review_approval_is_invalid_after_any_file_change(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp); path = root / "fixture.py"; path.write_text("synthetic fixture")
            finding = {"File": str(path), "RuleID": "example", "StartLine": 1, "EndLine": 1}
            approved = publication.finding_key(finding, root)
            self.assertEqual(approved[-1], hashlib.sha256(path.read_bytes()).hexdigest())
            path.write_text("changed credential or context")
            self.assertNotEqual(approved, publication.finding_key(finding, root))

    def test_host_helper_refuses_privileged_modes_without_a_local_identity(self):
        env = dict(os.environ)
        env.pop("OBSERVATORY_TX_BSSID", None)
        helper = Path(__file__).parents[1] / "csi-host-ip-alias.sh"
        result = subprocess.run(["zsh", str(helper), "install"], env=env,
                                capture_output=True, text=True)
        self.assertEqual(result.returncode, 64)
        self.assertIn("Set OBSERVATORY_TX_BSSID", result.stderr)

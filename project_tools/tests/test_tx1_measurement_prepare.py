lazy import copy
lazy import json
lazy from pathlib import Path
lazy import unittest

lazy from project_tools import tx1_comparison_capture as capture
lazy from project_tools import tx1_measurement_prepare as preparation
lazy from project_tools.tests.test_tx1_comparison_capture import config_fixture
lazy from tempfile import TemporaryDirectory


class PreparationTests(unittest.TestCase):
    def test_known_mapping_rejects_boolean_and_integer_gauges_before_readiness(self):
        counters = [{"endpoint_key": key, "path": list(path)} for key, path in preparation.counter_paths()]
        preparation.validate_counter_checks(counters)
        self.assertEqual(len(counters), 99)
        for name in ("csi_balance_closed", "udp_queue_depth", "processing_duration_last_us",
                     "last_server_receive_unix_ns"):
            with self.assertRaises(ValueError):
                preparation.validate_counter_checks(counters + [{"endpoint_key": "metrics",
                                                                 "path": ["csi_ingress", "counters", name]}])
        with self.assertRaises(ValueError):
            preparation.validate_counter_checks(counters[:-1])

    def test_true_and_one_are_different_identity_types_even_nested(self):
        checks = [{"endpoint_key": "ready", "path": ["binding"], "expected": {"enabled": True}}]
        row = {"endpoint_key": "ready", "http_status": 200, "body_complete": True,
               "value": {"binding": {"enabled": 1}}}
        with self.assertRaises(capture.SafetyAbort):
            capture.check_identity(row, checks)

    def test_preparation_must_be_bound_and_changed_static_contract_is_rejected(self):
        with TemporaryDirectory() as directory, config_fixture(directory) as config:
            capture.validate_config(config)
            for change in ("receipt", "mode", "path", "scope start"):
                bad = copy.deepcopy(config)
                if change == "receipt":
                    bad["preparation"]["sha256"] = "0" * 64
                elif change == "mode":
                    bad["mode"] = "seven_clients"
                elif change == "path":
                    bad["counter_checks"][0]["path"] = ["stations"]
                else:
                    bad["preflight"]["begin_mono_ns"] -= 1000
                with self.assertRaises(ValueError, msg=change):
                    capture.validate_config(bad)

    def test_synthetic_v5_definition_requires_its_own_canonical_identity(self):
        with TemporaryDirectory() as directory, config_fixture(directory) as config:
            preparation.validate_setup(config)
            original_setup = copy.deepcopy(config["setup"])
            path = Path(config["artifacts"]["setup"]["file"])
            sealed = json.loads(path.read_text())
            sealed["definition"]["test_identity"] += "-candidate"
            raw = (json.dumps(sealed["definition"], sort_keys=True, indent=2, ensure_ascii=False) + "\n").encode()
            digest = capture.hashlib.sha256(b"ruview.position-setup.v5\0" + raw).hexdigest()
            sealed.update(setup_sha256=digest, setup_id="setup-" + digest[:16])
            path.write_text(json.dumps(sealed))
            with self.assertRaises(ValueError):
                preparation.validate_setup(config)
            config["setup"] = {"id": sealed["setup_id"], "sha256": digest}
            preparation.validate_setup(config)
            self.assertNotEqual(config["setup"], original_setup)

    def test_old_seal_cannot_cover_a_new_runtime_or_different_candidate(self):
        with TemporaryDirectory() as directory, config_fixture(directory) as config:
            preparation.validate_setup(config)
            new_runtime = copy.deepcopy(config)
            for check in new_runtime["identity_checks"]:
                if check["endpoint_key"] == "TX1" and check["path"] == ["image_sha256"]:
                    check["expected"] = "c" * 64
            with self.assertRaisesRegex(ValueError, "expected TX runtime"):
                preparation.validate_setup(new_runtime)
            Path(config["artifacts"]["tx_candidate"]["file"]).write_bytes(b"different local candidate")
            with self.assertRaisesRegex(ValueError, "actual candidate"):
                preparation.validate_setup(config)


if __name__ == "__main__":
    unittest.main()

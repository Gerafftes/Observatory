import importlib.util
import json
import stat
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).parents[1] / "build_classification_truth_template.py"
SPEC = importlib.util.spec_from_file_location("classification_truth_template", SCRIPT)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(MODULE)


def hash_value(seed: int) -> str:
    return f"{seed:064x}"


def capture(recording_id: str, seed: int) -> dict:
    return {
        "recording_id": recording_id,
        "label": None,
        "ground_truth": None,
        "raw_sha256": hash_value(seed),
        "metadata_sha256": hash_value(seed + 10),
        "signal_sha256": hash_value(seed + 20),
        "setup_id": "setup-0123456789abcdef",
        "setup_sha256": hash_value(99),
    }


class ClassificationTruthTemplateTests(unittest.TestCase):
    def prediction_bytes(self) -> bytes:
        return json.dumps(
            {
                "schema_version": 2,
                "kind": "ruview.classification-predictions",
                "calibration": {"capture": capture("calibration", 1)},
                "measurements": [
                    {"capture": capture("neutral-a", 2)},
                    {"capture": capture("neutral-b", 3)},
                ],
            },
            separators=(",", ":"),
        ).encode()

    def test_template_binds_predictions_and_keeps_truth_as_placeholders(self):
        template = MODULE.build_template(self.prediction_bytes())
        self.assertEqual(template["calibration"]["expected_occupied"], False)
        self.assertIsNone(template["calibration"]["expected_point_id"])
        self.assertEqual(
            template["measurements"][0]["expected_occupied"],
            "__SET_TRUE_OR_FALSE__",
        )
        self.assertEqual(
            template["measurements"][0]["expected_point_id"],
            "__SET_NULL_OR_P01_TO_P09__",
        )

    def test_output_is_private_and_never_clobbered(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "truth-template.json"
            MODULE.write_private_no_clobber(output, {"ok": True})
            self.assertEqual(stat.S_IMODE(output.stat().st_mode), 0o600)
            with self.assertRaises(FileExistsError):
                MODULE.write_private_no_clobber(output, {"ok": False})
            self.assertEqual(json.loads(output.read_text()), {"ok": True})

    def test_embedded_truth_is_rejected(self):
        value = json.loads(self.prediction_bytes())
        value["measurements"][0]["capture"]["ground_truth"] = {"occupied": True}
        with self.assertRaisesRegex(ValueError, "embedded label or truth"):
            MODULE.build_template(json.dumps(value).encode())


if __name__ == "__main__":
    unittest.main()

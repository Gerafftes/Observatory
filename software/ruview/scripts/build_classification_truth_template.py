#!/usr/bin/env python3
"""Build a private, no-clobber truth template from classification predictions."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
from typing import Any

PREDICTION_SCHEMA_VERSION = 2
PREDICTION_KIND = "ruview.classification-predictions"
TRUTH_SCHEMA_VERSION = 1
TRUTH_KIND = "ruview.classification-truth"
SHA256_LENGTH = 64


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Create a mode-0600 classification truth template from an unlabelled "
            "replay artifact. The output must be completed before evaluation."
        )
    )
    parser.add_argument("predictions", type=Path)
    parser.add_argument("output", type=Path)
    return parser.parse_args()


def require_sha256(field: str, value: Any) -> str:
    if not isinstance(value, str) or len(value) != SHA256_LENGTH:
        raise ValueError(f"{field} must be a lowercase SHA-256")
    if any(character not in "0123456789abcdef" for character in value):
        raise ValueError(f"{field} must be a lowercase SHA-256")
    return value


def capture_identity(capture: Any, field: str) -> dict[str, Any]:
    if not isinstance(capture, dict):
        raise ValueError(f"{field} must be an object")
    if capture.get("label") is not None or capture.get("ground_truth") is not None:
        raise ValueError(f"{field} contains embedded label or truth")
    recording_id = capture.get("recording_id")
    if not isinstance(recording_id, str) or not recording_id.strip():
        raise ValueError(f"{field}.recording_id is empty")
    return {
        "recording_id": recording_id,
        "raw_sha256": require_sha256(f"{field}.raw_sha256", capture.get("raw_sha256")),
        "metadata_sha256": require_sha256(
            f"{field}.metadata_sha256", capture.get("metadata_sha256")
        ),
        "signal_sha256": require_sha256(
            f"{field}.signal_sha256", capture.get("signal_sha256")
        ),
    }


def build_template(prediction_bytes: bytes) -> dict[str, Any]:
    try:
        predictions = json.loads(prediction_bytes)
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ValueError(f"invalid prediction JSON: {error}") from error
    if predictions.get("schema_version") != PREDICTION_SCHEMA_VERSION:
        raise ValueError("unsupported classification prediction schema")
    if predictions.get("kind") != PREDICTION_KIND:
        raise ValueError("unsupported classification prediction kind")

    calibration = predictions.get("calibration")
    if not isinstance(calibration, dict):
        raise ValueError("prediction calibration is missing")
    calibration_capture = calibration.get("capture")
    calibration_identity = capture_identity(calibration_capture, "calibration.capture")
    setup_id = calibration_capture.get("setup_id")
    setup_sha256 = calibration_capture.get("setup_sha256")
    if not isinstance(setup_id, str) or not setup_id.strip():
        raise ValueError("calibration.capture.setup_id is empty")
    require_sha256("calibration.capture.setup_sha256", setup_sha256)

    measurements = predictions.get("measurements")
    if not isinstance(measurements, list) or not measurements:
        raise ValueError("prediction measurements are empty")
    truth_measurements = []
    seen_ids: set[str] = set()
    for index, measurement in enumerate(measurements):
        if not isinstance(measurement, dict):
            raise ValueError(f"measurements[{index}] must be an object")
        capture = measurement.get("capture")
        identity = capture_identity(capture, f"measurements[{index}].capture")
        if identity["recording_id"] in seen_ids:
            raise ValueError("prediction measurements contain duplicate recording IDs")
        seen_ids.add(identity["recording_id"])
        if capture.get("setup_id") != setup_id or capture.get("setup_sha256") != setup_sha256:
            raise ValueError("prediction measurements do not share the calibration setup")
        truth_measurements.append(
            {
                **identity,
                "expected_occupied": "__SET_TRUE_OR_FALSE__",
                "expected_point_id": "__SET_NULL_OR_P01_TO_P09__",
            }
        )

    return {
        "schema_version": TRUTH_SCHEMA_VERSION,
        "kind": TRUTH_KIND,
        "predictions_sha256": hashlib.sha256(prediction_bytes).hexdigest(),
        "setup_id": setup_id,
        "setup_sha256": setup_sha256,
        "calibration": {
            **calibration_identity,
            "expected_occupied": False,
            "expected_point_id": None,
        },
        "measurements": truth_measurements,
    }


def write_private_no_clobber(path: Path, payload: dict[str, Any]) -> None:
    encoded = (json.dumps(payload, indent=2, sort_keys=True) + "\n").encode()
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    try:
        with os.fdopen(descriptor, "wb") as output:
            output.write(encoded)
            output.flush()
            os.fsync(output.fileno())
    except Exception:
        path.unlink(missing_ok=True)
        raise


def main() -> int:
    args = parse_args()
    prediction_bytes = args.predictions.read_bytes()
    template = build_template(prediction_bytes)
    write_private_no_clobber(args.output, template)
    print(f"Private truth template written to {args.output}")
    print("Replace every __SET_*__ placeholder before classification evaluation.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

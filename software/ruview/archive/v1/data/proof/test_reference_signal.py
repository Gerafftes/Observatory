"""Synthetic, private-file-independent contracts for on-demand proof input."""

import contextlib
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock

import generate_reference_signal as generator


class ReferenceSignalTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name)
        self.data = self.directory / "sample_csi_data.json"

    def ensure(self):
        with contextlib.redirect_stdout(io.StringIO()):
            return generator.ensure_reference_signal(self.directory)

    def test_missing_input_is_generated_and_matches_original_normalized_hash(self):
        self.assertEqual(self.ensure(), str(self.data))
        self.assertEqual(generator.reference_digest(self.data),
                         generator.REFERENCE_HASH_PATH.read_text().strip())
        self.assertEqual(len(json.loads(self.data.read_text())["frames"]), 1000)
        self.assertEqual(list(self.directory.iterdir()), [self.data])

    def test_valid_cache_is_reused_without_writing_or_generating(self):
        self.ensure()
        before = self.data.stat().st_mtime_ns
        with mock.patch.object(generator, "generate_csi_frames", side_effect=AssertionError):
            self.ensure()
        self.assertEqual(self.data.stat().st_mtime_ns, before)

    def test_corrupted_cache_is_rejected_and_preserved(self):
        self.ensure()
        changed = json.loads(self.data.read_text())
        changed["frames"][-1]["amplitude"][0][0] += 0.01
        self.data.write_text(json.dumps(changed))
        before = self.data.read_bytes()
        with self.assertRaisesRegex(ValueError, "hash mismatch"):
            self.ensure()
        self.assertEqual(self.data.read_bytes(), before)

    def test_malformed_cache_is_not_overwritten(self):
        self.data.write_text("not JSON")
        with self.assertRaises(ValueError):
            self.ensure()
        self.assertEqual(self.data.read_text(), "not JSON")

    def test_wrong_hash_never_publishes_generated_data(self):
        expected = self.directory / "wrong.sha256"
        expected.write_text("0" * 64)
        with mock.patch.object(generator, "REFERENCE_HASH_PATH", expected):
            with self.assertRaisesRegex(ValueError, "hash mismatch"):
                self.ensure()
        self.assertEqual(list(self.directory.iterdir()), [expected])

    def test_missing_hash_does_not_accept_cached_data(self):
        self.ensure()
        with mock.patch.object(generator, "REFERENCE_HASH_PATH", self.directory / "missing"):
            with self.assertRaises(FileNotFoundError):
                self.ensure()

    def test_generation_failure_leaves_no_partial_cache(self):
        with mock.patch.object(generator, "generate_csi_frames", side_effect=RuntimeError("failed")):
            with self.assertRaises(RuntimeError):
                self.ensure()
        self.assertEqual(list(self.directory.iterdir()), [])

    def test_existing_metadata_is_not_modified(self):
        metadata = self.directory / "sample_csi_meta.json"
        metadata.write_text("preserve existing metadata")
        self.ensure()
        self.assertEqual(metadata.read_text(), "preserve existing metadata")

    def test_concurrent_cache_is_not_overwritten(self):
        def another_writer(source, destination):
            Path(destination).write_text("preserve concurrent data")
            raise FileExistsError

        with mock.patch.object(generator.os, "link", side_effect=another_writer):
            with self.assertRaises(ValueError):
                self.ensure()
        self.assertEqual(self.data.read_text(), "preserve concurrent data")


if __name__ == "__main__":
    unittest.main()

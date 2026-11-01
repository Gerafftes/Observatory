"""Release export fault paths; the actual SDK build is validated separately."""
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
import types
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("rx_release_builder", Path(__file__).resolve().parents[1] / "build_rx_release.py")
builder = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(builder)


class ReleaseBuilderTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.base = Path(self.directory.name)
        self.root = self.base / "project" / "firmware" / "node"
        self.root.mkdir(parents=True)
        (self.root / "main").mkdir()
        (self.root / "rx_release").mkdir()
        names = ["CMakeLists.txt", "version.txt", "partitions_display.csv", "sdkconfig.defaults",
                 "sdkconfig.defaults.rx-unified", "dependencies.rx-release.lock", "build_rx_release.py",
                 "main/a.c", "rx_release/CMakeLists.txt"]
        for name in names:
            (self.root / name).write_text("synthetic " + name)
        (self.root.parents[1] / "LICENSE").write_text("synthetic license")

    def snapshot(self):
        output = self.base / "snapshot"
        output.mkdir()
        return output

    def test_snapshot_hashes_and_freezes_exact_source_bytes(self):
        output = self.snapshot()
        hashes, identity = builder.snapshot(self.root, output)
        self.assertEqual(hashes["main/a.c"], builder.sha((self.root / "main/a.c").read_bytes()))
        self.assertEqual(json.loads((output / "SOURCE-MANIFEST.json").read_text())["source_sha256"], identity)
        (self.root / "main/a.c").write_text("changed after snapshot")
        self.assertEqual(builder.sha((output / "source/main/a.c").read_bytes()), hashes["main/a.c"])

    def test_read_disagreement_preserves_operands_and_never_reads_until_a_match(self):
        output = self.snapshot()
        original = Path.read_bytes
        calls = 0

        def read(path):
            nonlocal calls
            if path == self.root / "main/a.c":
                calls += 1
                return b"first" if calls == 1 else b"second"
            return original(path)

        with patch.object(Path, "read_bytes", read):
            with self.assertRaisesRegex(RuntimeError, "without retry"):
                builder.snapshot(self.root, output)
        self.assertEqual(calls, 2)
        self.assertEqual((output / "source-read-first.bin").read_bytes(), b"first")
        self.assertEqual((output / "source-read-second.bin").read_bytes(), b"second")
        self.assertTrue((output / "SOURCE-READ-FAILURE.json").exists())
        self.assertFalse((output / "RELEASE.json").exists())

    def test_copy_disagreement_preserves_both_comparison_buffers(self):
        output = self.snapshot()
        original = Path.read_bytes

        def read(path):
            return b"changed copy" if path == output / "source/main/a.c" else original(path)

        with patch.object(Path, "read_bytes", read):
            with self.assertRaisesRegex(RuntimeError, "stopped without retry"):
                builder.snapshot(self.root, output)
        self.assertEqual((output / "copy-observed.bin").read_bytes(), b"changed copy")
        self.assertEqual((output / "copy-expected.bin").read_bytes(), b"synthetic main/a.c")

    def invoke(self, output, run):
        with patch.object(builder, "__file__", str(self.root / "build_rx_release.py")), \
             patch.dict(builder.os.environ, {"IDF_PATH": str(self.base / "sdk")}), \
             patch.object(builder.subprocess, "check_output", side_effect=[builder.SDK_COMMIT + "\n", "", "synthetic compiler"]), \
             patch.object(builder.subprocess, "run", side_effect=run) as calls, \
             patch.object(builder.sys, "argv", ["build_rx_release.py", "--output", str(output)]):
            builder.main()
        return calls

    def test_existing_output_is_never_reused_or_modified(self):
        output = self.snapshot()
        marker = output / "keep.bin"
        marker.write_bytes(b"existing evidence")
        with self.assertRaises(FileExistsError):
            self.invoke(output, AssertionError("must not run compiler"))
        self.assertEqual(marker.read_bytes(), b"existing evidence")
        self.assertEqual(list(output.iterdir()), [marker])

    def test_failed_compiler_stops_once_and_preserves_snapshot(self):
        output = self.base / "failed-build"
        count = 0

        def run(command, **kwargs):
            nonlocal count
            count += 1
            raise subprocess.CalledProcessError(2, command)

        with self.assertRaises(subprocess.CalledProcessError):
            self.invoke(output, run)
        self.assertEqual(count, 1)
        self.assertTrue((output / "SOURCE-MANIFEST.json").exists())
        self.assertFalse((output / "RELEASE.json").exists())

    def test_different_build_bytes_do_not_produce_a_release_or_third_attempt(self):
        output = self.base / "different-builds"
        count = 0

        def run(command, **kwargs):
            nonlocal count
            count += 1
            build = Path(command[command.index("-B") + 1])
            build.mkdir()
            (build / "esp32-csi-node.bin").write_bytes(b"\xe9\x01\x00\x30" + bytes([count]) * 28)

        with self.assertRaisesRegex(RuntimeError, "Independent builds differ"):
            self.invoke(output, run)
        self.assertEqual(count, 2)
        self.assertFalse((output / "RELEASE.json").exists())

    def test_success_checks_an_existing_build_image_before_publishing(self):
        output = self.base / "successful-build"
        compiler=self.base / "compiler"; compiler.write_bytes(b"compiler")
        image=b"\xe9\x01\x00\x30"+b"x"*28
        def run(command, **kwargs):
            if command[-1] == "size": return
            build=Path(command[command.index("-B")+1]); build.mkdir()
            (build / "esp32-csi-node.bin").write_bytes(image)
            (build / "partition_table").mkdir()
            (build / "partition_table/partition-table.bin").write_bytes(b"partition")
            (build / "project_description.json").write_text(json.dumps({"c_compiler":str(compiler),"target":"esp32s3"}))
            (output / "sdkconfig").write_text("\n".join([
                "CONFIG_RX_UNIFIED=y", "CONFIG_HEAP_POISONING_COMPREHENSIVE=y",
                "CONFIG_APP_REPRODUCIBLE_BUILD=y", "CONFIG_FREERTOS_CHECK_STACKOVERFLOW_CANARY=y",
                "CONFIG_COMPILER_STACK_CHECK_MODE_STRONG=y", "CONFIG_CSI_SELF_PING_ENABLE=y",
                "CONFIG_CSI_SELF_PING_INTERVAL_MS=20"]))
        def load(chip,path):
            self.assertEqual(chip,"esp32s3")
            self.assertEqual(Path(path).read_bytes(),image)
            return types.SimpleNamespace(append_digest=True,stored_digest=b"x"*32,calc_digest=b"x"*32)
        module=types.SimpleNamespace(LoadFirmwareImage=load)
        with patch.dict(builder.sys.modules,{"esptool":types.ModuleType("esptool"),"esptool.bin_image":module}):
            self.invoke(output,run)
        receipt=json.loads((output / "RELEASE.json").read_text())
        self.assertEqual((output / "rx-unified.bin").read_bytes(),image)
        self.assertEqual(receipt["reproduced_builds"],2)
        self.assertFalse(receipt["hardware_validated"])

    def test_unified_profile_preserves_qualified_20ms_traffic_floor(self):
        config = Path(__file__).resolve().parents[1] / "sdkconfig.defaults.rx-unified"
        lines = config.read_text().splitlines()
        self.assertIn("CONFIG_CSI_SELF_PING_ENABLE=y", lines)
        self.assertIn("CONFIG_CSI_SELF_PING_INTERVAL_MS=20", lines)


if __name__ == "__main__":
    unittest.main()

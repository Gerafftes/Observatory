"""No device I/O: positive ROM handoff and strict negative boundary cases."""
lazy import hashlib
lazy from pathlib import Path
lazy import re
lazy import sys
lazy import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "tools"))
lazy import tx1_rom_guard as guard


def prefix(raw):
    return {"size": len(raw), "sha256": hashlib.sha256(raw).hexdigest()}


def failures(raw):
    return re.search(rb"TX_BOOT ERROR|Guru Meditation|Brownout", raw)


def strict(raw):
    if failures(raw) or re.search(rb"\brst:0x|TX_BOOT (?:APP_MAIN_ENTER|IDENTITY|BOOT_OK)", raw):
        raise ValueError("pre-ROM failure")


class ROMGuardTests(unittest.TestCase):
    snapshot = b"stable\n"
    pre = snapshot + b"still stable\n"
    reset = (b"ESP-ROM:esp32s3\r\n"
             b"rst:0x15 (USB_UART_CHIP_RESET),boot:0x0 (DOWNLOAD(USB/UART0))\r\n"
             b"waiting for download\r\n")

    def check(self, raw, rom=None, controlled=None):
        guard.inspect_after_window(raw, self.snapshot, rom, controlled, strict, failures)

    def test_requested_rom_and_no_marker_yet(self):
        self.check(self.pre, prefix(self.pre))
        self.check(self.pre + self.reset, prefix(self.pre))

    def test_no_handoff_remains_strict(self):
        self.check(self.pre)
        with self.assertRaises(ValueError):
            self.check(self.pre + self.reset)

    def test_pre_rom_failures(self):
        for marker in (b"TX_BOOT ERROR", b"rst:0x1", b"TX_BOOT APP_MAIN_ENTER",
                       b"TX_BOOT IDENTITY", b"TX_BOOT BOOT_OK"):
            raw = self.pre + marker + b"\n"
            with self.subTest(marker=marker), self.assertRaises(ValueError):
                self.check(raw + self.reset, prefix(raw))

    def test_held_rom_failures(self):
        for marker in (b"TX_BOOT ERROR", b"Guru Meditation", b"TX_BOOT APP_MAIN_ENTER",
                       b"TX_BOOT IDENTITY", b"TX_BOOT BOOT_OK", self.reset, b"rst:0x1"):
            with self.subTest(marker=marker), self.assertRaises(ValueError):
                self.check(self.pre + self.reset + marker, prefix(self.pre))

    def test_wrong_reset_and_missing_confirmation(self):
        for reset in (self.reset.replace(b"0x15", b"0x1"),
                      self.reset.replace(b"USB_UART_CHIP_RESET", b"WDT"),
                      self.reset.replace(b"boot:0x0", b"boot:0x8"),
                      self.reset.replace(b"waiting for download", b"")):
            with self.subTest(reset=reset), self.assertRaises(ValueError):
                self.check(self.pre + reset, prefix(self.pre))

    def test_prefix_changes_and_order(self):
        raw = self.pre + self.reset
        for rom in (prefix(b"other"), {"size": True, "sha256": "0" * 64},
                    prefix(raw + b"beyond"), prefix(self.snapshot[:-1])):
            with self.subTest(rom=rom), self.assertRaises(ValueError):
                self.check(raw, rom)
        with self.assertRaises(ValueError):
            self.check(raw, prefix(self.pre), prefix(self.snapshot))
        with self.assertRaises(ValueError):
            self.check(b"X" + raw[1:], prefix(self.pre))

    def test_controlled_boot_is_separate_and_prefix_checked(self):
        held = self.pre + self.reset
        self.check(held + b"TX_BOOT APP_MAIN_ENTER\n", prefix(self.pre), prefix(held))
        with self.assertRaises(ValueError):
            self.check(held + b"TX_BOOT APP_MAIN_ENTER\n", prefix(self.pre), prefix(held + b"X"))

    def test_split_reset_at_handoff_is_rejected(self):
        raw = self.pre + self.reset
        start = raw.index(b"rst:0x")
        for split in range(1, len(b"rst:0x")):
            with self.subTest(split=split), self.assertRaises(ValueError):
                self.check(raw, prefix(raw[:start + split]))

    def test_required_reset_cannot_be_missing(self):
        with self.assertRaises(ValueError):
            guard.inspect_held_rom(b"", failures, require_reset=True)
        guard.inspect_held_rom(self.reset, failures, require_reset=True)

    def test_failure_split_at_handoff_is_rejected(self):
        raw = self.pre + b"TX_BOOT ERROR injected\n" + self.reset
        with self.assertRaises(ValueError):
            self.check(raw, prefix(self.pre + b"TX_BOOT ER"))

    def test_reset_split_at_stable_snapshot_is_rejected(self):
        snapshot = self.pre + b"rst:"
        raw = snapshot + b"0x1 (POWERON)\n"
        with self.assertRaises(ValueError):
            guard.inspect_after_window(raw, snapshot, None, None, strict, failures)


if __name__ == "__main__":
    unittest.main()

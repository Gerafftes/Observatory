"""Actual collector/curl/connection/cleanup integration on isolated loopback.

All 121 one-second slots and the production clock/margin limits remain intact.
Only the protected production preflight, host reads and tcpdump are replaced:
the capture fixture is a real, signal-cleaned child writing a complete PCAP.
"""
lazy import asyncio
lazy from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
lazy import json
lazy from pathlib import Path
lazy import signal
lazy import sys
lazy import tempfile
lazy import threading
lazy import time
lazy import unittest
lazy from unittest.mock import patch

lazy from project_tools import tx1_comparison_capture as capture


PCAP_CHILD = """
import signal, struct, sys, time
running = True
def stop(*args):
    global running
    running = False
signal.signal(signal.SIGINT, stop)
with open(sys.argv[1], 'xb', buffering=0) as stream:
    stream.write(b'\\x4d\\x3c\\xb2\\xa1' + struct.pack('<HHIIII', 2, 4, 0, 0, 65535, 1))
    print('listening on offline-loopback-fixture', file=sys.stderr, flush=True)
    while running:
        sec, frac = divmod(time.time_ns(), 1000000000)
        stream.write(struct.pack('<IIII', sec, frac, 4, 4) + b'test')
        time.sleep(0.01)
print('0 packets dropped by kernel', file=sys.stderr, flush=True)
"""


class LoopbackCollectorTests(unittest.IsolatedAsyncioTestCase):
    async def test_twelve_real_http_endpoints_full_window_timeout_and_cleanup(self):
        counts, received, lock = {}, [], threading.Lock()

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *_):
                pass

            def do_GET(self):
                key = self.path.removeprefix("/")
                with lock:
                    index = counts.get(key, 0)
                    counts[key] = index + 1
                    received.append((key, index, time.monotonic_ns()))
                # A real delayed HTTP body forces curl's unchanged 2-s limit.
                # All other replies vary independently and remain inexpensive.
                time.sleep(2.25 if key == "RX1" and index == 1 else 0.001 + index % 4 * 0.002)
                body = json.dumps({"endpoint": key, "response_number": index, "complete": True}).encode()
                try:
                    self.send_response(200)
                    self.send_header("Content-Length", str(len(body)))
                    self.end_headers()
                    self.wfile.write(body)
                except (BrokenPipeError, ConnectionResetError):
                    pass  # The collector retains the timed-out request's evidence.

        server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        server.daemon_threads = True
        thread = threading.Thread(target=server.serve_forever, kwargs={"poll_interval": 0.05}, daemon=True)
        thread.start()
        original_spawn = asyncio.create_subprocess_exec
        children, commands = [], []

        async def spawn(*args, **kwargs):
            commands.append(args)
            if args[0] == "/usr/sbin/tcpdump":
                args = (sys.executable, "-u", "-c", PCAP_CHILD, args[args.index("-w") + 1])
            child = await original_spawn(*args, **kwargs)
            children.append(child)
            return child

        async def offline_guards(*args):
            # No project/host reads are permitted by this integration fixture.
            return

        try:
            with tempfile.TemporaryDirectory() as directory:
                root = Path(directory).resolve()
                (root / "private").mkdir()
                out = root / "private" / "one-full-window"
                config = {"mode": "seven_clients", "interface": "en0",
                          "apis": {key: f"http://127.0.0.1:{server.server_port}/{key}"
                                   for key in sorted(capture.API_KEYS)},
                          "identity_checks": [], "counter_checks": [],
                          "preflight": {"begin_mono_ns": time.monotonic_ns()},
                          "authorization": {"expires_wall_ns": time.time_ns() + 180_000_000_000}}
                with patch.object(capture, "PROJECT", root), \
                     patch.object(capture, "validate_config"), \
                     patch.object(capture.preparation, "field_contract", return_value=[]), \
                     patch.object(capture, "bound_file_state", return_value={}), \
                     patch.object(capture, "host_guard_loop", side_effect=offline_guards), \
                     patch.object(capture.asyncio, "create_subprocess_exec", side_effect=spawn):
                    result = await capture.collect(config, out)
                run = json.loads((out / "run.private.json").read_text())
                self.assertEqual(result["planned"], 121 * 12)
                self.assertEqual(len(run["windows"]), 1)
                self.assertEqual(run["windows"][0]["end_mono_ns"] - run["windows"][0]["begin_mono_ns"],
                                 120_000_000_000)
                self.assertTrue(result["scope_within_budget"])
                self.assertTrue(result["cleanup_within_budget"])
                self.assertEqual(result["status"], "CAPTURE_EVIDENCE_FAIL")
                self.assertTrue(all(child.returncode is not None for child in children))
                curl_commands = [args for args in commands if args[0] == "/usr/bin/curl"]
                self.assertEqual(len(curl_commands), result["attempted"])
                self.assertTrue(all(args[args.index("--connect-timeout") + 1] == "1" for args in curl_commands))
                self.assertTrue(all(args[args.index("--max-time") + 1] == "2" for args in curl_commands))
                timeout = [row for row in run["rows"] if row["timed_out"]]
                self.assertEqual([(row["endpoint_key"], row["slot"]) for row in timeout], [("RX1", 1)])
                self.assertEqual(timeout[0]["curl_exit_code"], 28)
                self.assertFalse(timeout[0]["body_complete"])
                self.assertIsNone(timeout[0]["value"])
                self.assertIsNotNone(timeout[0]["curl_timings"])
                skipped = run["proof"]["skipped_slots"]
                self.assertEqual({row["slot_id"] for row in skipped}, {"2:RX1", "3:RX1"})
                for key in capture.API_KEYS - {"RX1"}:
                    rows = [row for row in run["rows"] if row["endpoint_key"] == key]
                    self.assertEqual({row["slot"] for row in rows}, set(range(121)))
                    self.assertTrue(all(row["http_status"] == 200 and row["error"] is None
                                        and row["body_complete"] for row in rows))
                for key in capture.API_KEYS:
                    rows = [row for row in run["rows"] if row["endpoint_key"] == key]
                    for earlier, later in zip(rows, rows[1:]):
                        self.assertLessEqual(earlier["end"]["mono_after_ns"], later["begin"]["mono_before_ns"])
                self.assertEqual(run["proof"]["kernel_drops"], 0)
                self.assertTrue(run["proof"]["clean_exit"])
                self.assertEqual(result["coverage"]["missing_request_ids"], ["2:RX1", "3:RX1"])
        finally:
            server.shutdown()
            server.server_close()
            thread.join(timeout=1)


if __name__ == "__main__":
    unittest.main()

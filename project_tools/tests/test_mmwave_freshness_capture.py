lazy import asyncio
lazy import copy
lazy import io
lazy import json
lazy from pathlib import Path
lazy import signal
lazy import sys
lazy import tempfile
lazy import time
lazy import unittest
lazy from unittest.mock import patch

lazy from project_tools import mmwave_freshness_capture as capture
lazy from project_tools import tx7b_coverage as coverage
lazy from project_tools.tests.test_tx7b_coverage import fixture, pair


class ConcurrentCoverageTests(unittest.TestCase):
    def fixture(self):
        rows, span, proof = fixture()
        slow = rows[0]
        slow.update(request_id="0:slow", endpoint="slow")
        fast = copy.deepcopy(slow)
        fast.update(request_id="0:fast", endpoint="fast", begin=pair(4_000_000_000),
                    end=pair(4_200_000_000))
        rows = [fast, slow]  # Actual completion order differs from start order.
        proof["schema"] = coverage.CONCURRENT_SCHEMA
        proof["clock_events"] = [
            {"ordinal": index, "request_id": row["request_id"], "boundary": edge,
             "stamp": copy.deepcopy(row[edge])}
            for index, (row, edge) in enumerate([(slow, "begin"), (fast, "begin"),
                                                (fast, "end"), (slow, "end")])]
        return rows, span, proof

    def assess(self, rows, span, proof):
        return coverage.assess(rows, span, proof, [r["request_id"] for r in rows])

    def test_overlapping_endpoints_keep_real_clock_order_and_margins(self):
        rows, span, proof = self.fixture()
        result = self.assess(rows, span, proof)
        self.assertEqual(result["status"], "COVERAGE_PASS")
        self.assertEqual(result["schema"], coverage.CONCURRENT_SCHEMA)
        self.assertGreater(result["packet_pre_roll_lower_bound_ns"], 2_000_000_000)
        span["last_ns"] -= 2_000_000_000
        self.assertEqual(self.assess(rows, span, proof)["status"], "COVERAGE_FAIL")

    def test_missing_tampered_reordered_or_duplicate_clock_events_fail(self):
        for kind in ("missing", "tampered", "reordered", "duplicate"):
            rows, span, proof = self.fixture()
            events = proof["clock_events"]
            if kind == "missing":
                events.pop()
            elif kind == "tampered":
                events[0]["stamp"]["wall_ns"] += 1
            elif kind == "reordered":
                events[1], events[2] = events[2], events[1]
                for i, event in enumerate(events):
                    event["ordinal"] = i  # Correct ordinals cannot mask time regression.
            else:
                events.append(copy.deepcopy(events[0]))
            self.assertEqual(self.assess(rows, span, proof)["status"], "COVERAGE_FAIL", kind)

    def test_one_endpoint_overlap_is_rejected(self):
        rows, span, proof = self.fixture()
        rows[0]["endpoint"] = rows[1]["endpoint"]
        self.assertEqual(self.assess(rows, span, proof)["status"], "COVERAGE_FAIL")

    def test_timeout_keeps_clock_coverage_separate_from_query_success(self):
        rows, span, proof = self.fixture()
        rows[1].update(http_status=None, error="timeout")
        result = self.assess(rows, span, proof)
        self.assertEqual(result["status"], "COVERAGE_PASS")
        self.assertFalse(result["all_queries_succeeded"])
        proof["clock_pairs"][-1]["wall_ns"] += 1_185_375
        self.assertEqual(self.assess(rows, span, proof)["status"], "COVERAGE_FAIL")


class ScheduleTests(unittest.IsolatedAsyncioTestCase):
    async def test_actual_getter_uses_private_header_file_and_preserves_curl_phase_times(self):
        original = asyncio.create_subprocess_exec
        arguments = []

        async def fake_curl(*args, **kwargs):
            arguments.append(args)
            Path(args[args.index("--output") + 1]).write_text('{"complete":true}')
            return await original(sys.executable, "-c",
                                  "print('200\\n0.000001\\n0.000010\\n0.001000\\n0.001500')", **kwargs)

        with tempfile.TemporaryDirectory() as directory:
            header = Path(directory) / "auth.private.header"
            header.write_text("Authorization: Bearer LOCAL_TEST_OPAQUE_TOKEN\n")
            header.chmod(0o600)
            recorder = capture.Requests(Path(directory), io.StringIO(), headers={"TX1": {"file": str(header)}})
            with patch.object(capture.asyncio, "create_subprocess_exec", side_effect=fake_curl):
                row = await recorder.get("TX1", "http://127.0.0.1/unused", 0, time.monotonic_ns())
            self.assertEqual(arguments[0][arguments[0].index("--header") + 1], "@" + str(header))
            self.assertNotIn("LOCAL_TEST_OPAQUE_TOKEN", " ".join(arguments[0]))
            self.assertNotIn("LOCAL_TEST_OPAQUE_TOKEN", json.dumps(row))
            self.assertEqual(row["curl_timings"]["connect_seconds"], "0.000010")
            self.assertLessEqual(row["process_start_begin_mono_ns"], row["process_started_mono_ns"])
            self.assertFalse(recorder.children)

    async def test_collector_deadline_stops_capture_without_restarting_it(self):
        children, cancelled = [], []

        class CaptureChild:
            returncode = None
            def send_signal(self, sig):
                self.returncode = 0
            async def wait(self):
                return self.returncode

        async def fake_capture(*args, **kwargs):
            Path(args[args.index("-w") + 1]).write_bytes(b"offline PCAP fixture")
            kwargs["stderr"].write("listening on offline-test\n")
            kwargs["stderr"].flush()
            child = CaptureChild()
            children.append(child)
            return child

        async def blocked_schedules(*args):
            try:
                await asyncio.Event().wait()
            finally:
                cancelled.append(True)

        with tempfile.TemporaryDirectory() as temporary:
            config = {"interface": "en0", "apis": {k: f"http://192.168.4.{i + 2}:8032/ota/status"
                      for i, k in enumerate(capture.API_KEYS)}}
            with patch.object(capture, "validate_config"), \
                 patch.object(capture.asyncio, "create_subprocess_exec", side_effect=fake_capture), \
                 patch.object(capture, "run_schedules", side_effect=blocked_schedules), \
                 patch.object(capture, "SCOPE_SECONDS", 2.12), \
                 patch.object(capture, "ROLL_SECONDS", 0.001), \
                 patch.object(coverage, "pcap_span", return_value={"first_ns": 1}), \
                 patch.object(coverage, "assess", return_value={"status": "COVERAGE_FAIL"}):
                result = await capture.collect(config, Path(temporary) / "one-window")
            self.assertEqual(len(children), 1)
            self.assertEqual(children[0].returncode, 0)
            self.assertTrue(cancelled)
            self.assertIn("TimeoutError", result["error"])
            self.assertTrue(result["scope_within_budget"])
            self.assertTrue(result["cleanup_within_budget"])

    async def test_slow_endpoint_does_not_delay_other_deadlines_or_catch_up(self):
        active, maximum, rows = {}, {}, []

        class Recorder:
            async def get(self, key, url, index, due):
                begin = time.monotonic_ns()
                active[key] = active.get(key, 0) + 1
                maximum[key] = max(maximum.get(key, 0), active[key])
                await asyncio.sleep(0.13 if key == "slow" else 0.001)
                active[key] -= 1
                row = {"request_id": f"{index}:{key}", "begin": begin,
                       "end": {"mono_after_ns": time.monotonic_ns()}, "due": due}
                rows.append(row)
                return row

        slots, skipped = [], []
        await capture.run_schedules({"slow": "s", "RX4": "r", "server": "h"}, Recorder(),
                                    time.monotonic_ns(), slots, skipped, samples=9, period=0.04)
        for key in ("RX4", "server"):
            selected = [r for r in rows if r["request_id"].endswith(":" + key)]
            self.assertEqual(len(selected), 9)
            self.assertEqual([r["request_id"] for r in selected], [f"{i}:{key}" for i in range(9)])
            slow_end = next(r for r in rows if r["request_id"] == "0:slow")["end"]["mono_after_ns"]
            self.assertLess(selected[1]["begin"], slow_end)
        self.assertEqual(maximum, {"slow": 1, "RX4": 1, "server": 1})
        self.assertTrue(skipped)
        self.assertTrue(all(s["slot_id"].endswith(":slow") for s in skipped))
        self.assertEqual(set(slots) | {s["slot_id"] for s in skipped},
                         {f"{i}:{k}" for i in range(9) for k in maximum})

    async def test_absent_response_global_budget_cancels_all_workers(self):
        active, cancelled, fast = set(), set(), []

        class Recorder:
            async def get(self, key, url, index, due):
                active.add(key)
                try:
                    if key == "absent":
                        await asyncio.Event().wait()
                    else:
                        fast.append(index)
                        await asyncio.sleep(0)
                    return {"request_id": f"{index}:{key}", "end": {"mono_after_ns": time.monotonic_ns()}}
                except asyncio.CancelledError:
                    cancelled.add(key)
                    raise
                finally:
                    active.remove(key)

        started = time.monotonic()
        with self.assertRaises(asyncio.TimeoutError):
            async with asyncio.timeout(0.14):
                await capture.run_schedules({"absent": "a", "RX4": "r"}, Recorder(),
                    time.monotonic_ns(), [], [], samples=100, period=0.02)
        self.assertLess(time.monotonic() - started, 0.6)
        self.assertFalse(active)
        self.assertIn("absent", cancelled)
        self.assertGreaterEqual(len(fast), 5)

    async def test_actual_getter_global_cancellation_reaps_child_and_keeps_boundary(self):
        original = asyncio.create_subprocess_exec
        children = []

        async def fake_curl(*args, **kwargs):
            child = await original(sys.executable, "-c", "import time; time.sleep(30)", **kwargs)
            children.append(child)
            return child

        with tempfile.TemporaryDirectory() as temporary:
            recorder = capture.Requests(Path(temporary), io.StringIO())
            with patch.object(capture.asyncio, "create_subprocess_exec", side_effect=fake_curl):
                with self.assertRaises(asyncio.TimeoutError):
                    async with asyncio.timeout(0.08):
                        await recorder.get("MMWAVE2", "http://127.0.0.1/unused", 0, time.monotonic_ns())
            self.assertTrue(all(c.returncode is not None for c in children))
            self.assertFalse(recorder.children)
            self.assertEqual(len(recorder.rows), 1)
            row = recorder.rows[0]
            self.assertEqual(row["error"], "global deadline/cancellation")
            self.assertIsNone(row["value"])
            self.assertGreater(row["end"]["mono_before_ns"], row["begin"]["mono_after_ns"])

    async def test_complete_failed_and_partial_HTTP_bodies_are_retained(self):
        original = asyncio.create_subprocess_exec
        for status, code, body in [(200, 0, '{"all":[1,2,3]}'), (503, 0, '{"error":"busy"}'),
                                   (200, 28, '{"partial":')]:
            async def fake_curl(*args, **kwargs):
                Path(args[args.index("--output") + 1]).write_text(body)
                return await original(sys.executable, "-c", f"print('{status}', end=''); raise SystemExit({code})", **kwargs)
            with tempfile.TemporaryDirectory() as temporary:
                recorder = capture.Requests(Path(temporary), io.StringIO())
                with patch.object(capture.asyncio, "create_subprocess_exec", side_effect=fake_curl):
                    row = await recorder.get("MMWAVE2", "http://127.0.0.1/unused", 0, time.monotonic_ns())
                self.assertEqual(row["response_text"], body)
                self.assertEqual(row["http_status"], status)
                self.assertEqual(row["body_complete"], code == 0)
                if code:
                    self.assertIsNone(row["value"])
                    self.assertTrue(row["timed_out"])
                elif status == 503:
                    self.assertEqual(row["value"], {"error": "busy"})
                    self.assertEqual(row["error"], "HTTP 503")

    async def test_capture_cleanup_escalation_is_bounded(self):
        class Child:
            returncode = None
            signals = []
            def send_signal(self, sig):
                self.signals.append(sig)
            def kill(self):
                self.returncode = -9
            async def wait(self):
                if self.returncode is None:
                    await asyncio.Event().wait()
                return self.returncode
        child = Child()
        result = await capture.stop_process(child, grace=0.01, kill_timeout=0.05)
        self.assertEqual(result, {"exit_code": -9, "killed": True})
        self.assertEqual(child.signals, [signal.SIGINT])


if __name__ == "__main__":
    unittest.main()

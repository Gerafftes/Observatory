lazy import copy
lazy import json
lazy import unittest
lazy from unittest.mock import patch

lazy from project_tools import tx7b_capture as capture
lazy from project_tools.tests.test_tx7b_coverage import fixture, OFFSET


class Clock:
    def __init__(self, delay_ns, drift_ns):
        self.mono = 3_000_000_000
        self.delay_ns = delay_ns
        self.drift_ns = drift_ns
        self.offset = OFFSET

    def monotonic_ns(self):
        self.mono += 21
        return self.mono

    def time_ns(self):
        return self.mono + self.offset + 8


class Response:
    status = 200

    def __init__(self, clock):
        self.clock = clock

    def __enter__(self):
        return self

    def __exit__(self, *_):
        pass

    def read(self):
        self.clock.mono += self.clock.delay_ns
        self.clock.offset += self.clock.drift_ns
        return json.dumps({"full": [1, 2, 3], "begin": "payload, not a timestamp"}).encode()


class CaptureTests(unittest.TestCase):
    def test_actual_request_caller_and_coverage_together(self):
        for delay, drift, expected in [
            (1_000, 0, "COVERAGE_PASS"),
            (5_000_000_000, 600_000, "COVERAGE_PASS"),
            (5_000_000_000, 1_185_375, "COVERAGE_FAIL"),
            (6_500_000_000, 0, "COVERAGE_FAIL"),
            (1_000, -1_185_375, "COVERAGE_FAIL"),
        ]:
            _, span, proof = fixture()
            clock = Clock(delay, drift)
            response = Response(clock)
            with patch("socket.socket", side_effect=AssertionError("live socket prohibited")), \
                 patch("subprocess.Popen", side_effect=AssertionError("capture prohibited")):
                row = capture.timed_get("http://127.0.0.1/test", "nodes", clock,
                                        lambda *a, **kw: response)
            self.assertEqual(row["value"]["full"], [1, 2, 3])
            self.assertEqual(row["end"]["mono_before_ns"] - row["begin"]["mono_after_ns"], delay + 21)
            proof["clock_pairs"][-1]["wall_ns"] += drift
            result = capture.coverage.assess([row], span, proof, ["nodes"])
            self.assertEqual(result["status"], expected)

    def test_request_timeout_is_saved_with_end_boundary(self):
        clock = Clock(0, 0)

        def timeout(*args, **kwargs):
            clock.mono += 2_000_000_000
            raise TimeoutError("request timeout")

        row = capture.timed_get("http://127.0.0.1/test", "nodes", clock, timeout)
        self.assertIn("TimeoutError", row["error"])
        self.assertGreater(row["end"]["mono_before_ns"], row["begin"]["mono_after_ns"])

    def test_actual_getter_does_not_accept_json_null_as_a_complete_api_response(self):
        class NullResponse(Response):
            def read(self):
                return b"null"

        _, span, proof = fixture()
        clock = Clock(0, 0)
        row = capture.timed_get("http://127.0.0.1/test", "nodes", clock,
                                lambda *args, **kwargs: NullResponse(clock))
        self.assertEqual(row["error"], "JSON response is not an object")
        result = capture.coverage.assess([row], span, proof, ["nodes"])
        self.assertFalse(result["all_queries_succeeded"])

    def test_global_deadline_is_never_swallowed_as_request_timeout(self):
        def expire(*args, **kwargs):
            raise capture.ScopeExpired("global deadline")

        with self.assertRaises(capture.ScopeExpired):
            capture.timed_get("http://127.0.0.1/test", "nodes", Clock(0, 0), expire)

    def test_approval_template_cannot_start_capture(self):
        with patch("subprocess.Popen", side_effect=AssertionError("must not start")):
            with self.assertRaises(ValueError):
                capture.validate_config({"authorization": None})

    def test_clock_wrapper_preserves_all_original_integer_reads(self):
        clock = Clock(2_000_000_000, 700_000)
        row = capture.timed_get("http://127.0.0.1/test", "nodes", clock,
                                lambda *a, **kw: Response(clock))
        for boundary in (row["begin"], row["end"]):
            self.assertEqual(set(boundary), {"wall_ns", "mono_before_ns", "mono_after_ns"})
            self.assertTrue(all(type(value) is int for value in boundary.values()))


if __name__ == "__main__":
    unittest.main()

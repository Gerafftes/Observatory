lazy import copy
lazy import struct
lazy import unittest

lazy from project_tools import tx7b_coverage as gate


OFFSET = 1_791_000_000_000_000_000


def pair(mono, offset=OFFSET, width=0):
    return {"wall_ns": mono + offset, "mono_before_ns": mono,
            "mono_after_ns": mono + width}


def fixture():
    rows = [{"request_id": "nodes", "begin": pair(3_000_000_000),
             "end": pair(8_000_000_000), "error": None, "http_status": 200}]
    span = {"first_ns": OFFSET, "last_ns": OFFSET + 11_000_000_000,
            "resolution_ns": 1, "sha256": "fixture"}
    proof = {"schema": gate.SCHEMA, "pcap_sha256": "fixture", "clean_exit": True,
             "exit_code": 0, "kernel_drops": 0, "ready_mono_ns": 0,
             "stop_requested_mono_ns": 11_000_000_000, "timestamp_type": "host",
             "timestamp_type_confirmed": True,
             "clock_info": {"time": {"resolution_ns": 1},
                            "monotonic": {"resolution_ns": 1, "monotonic": True}},
             "clock_pairs": [pair(0), pair(12_000_000_000)]}
    return rows, span, proof


def check(rows, span, proof, ids=("nodes",)):
    return gate.assess(rows, span, proof, ids)


class CoverageTests(unittest.TestCase):
    def test_actual_read_order_and_integer_precision(self):
        events = []

        class Clocks:
            def monotonic_ns(self):
                events.append("mono")
                return 10 if len(events) == 1 else 47

            def time_ns(self):
                events.append("wall")
                return OFFSET + 123

        self.assertEqual(gate.stamp(Clocks()), pair(10, OFFSET + 113, 37))
        self.assertEqual(events, ["mono", "wall", "mono"])

    def test_independent_clock_drift_and_step_sequences(self):
        for offsets, expected in [
            ([0, 100_000, 300_000, 800_000], "COVERAGE_PASS"),
            ([0, -100_000, -300_000, -800_000], "COVERAGE_PASS"),
            ([0, 200_000, 600_000, 1_185_375], "COVERAGE_FAIL"),
            ([0, 2_000_000, 0, 0], "COVERAGE_FAIL"),
            ([0, -2_000_000, 0, 0], "COVERAGE_FAIL"),
        ]:
            rows, span, proof = fixture()
            stamps = [proof["clock_pairs"][0], rows[0]["begin"],
                      rows[0]["end"], proof["clock_pairs"][1]]
            for stamp, change in zip(stamps, offsets):
                stamp["wall_ns"] += change
            self.assertEqual(check(rows, span, proof)["status"], expected)

    def test_alignment_threshold_is_not_relaxed(self):
        for spread, expected in [(999_999, "COVERAGE_PASS"),
                                 (1_000_000, "COVERAGE_PASS"),
                                 (1_000_001, "COVERAGE_FAIL")]:
            rows, span, proof = fixture()
            # Four ns arise from explicitly accounted clock resolution.
            proof["clock_pairs"][-1]["wall_ns"] += spread - 4
            result = check(rows, span, proof)
            self.assertEqual(result["mapping_spread_ns"], spread)
            self.assertEqual(result["status"], expected)

    def test_exact_packet_margin_boundaries(self):
        for gap, expected in [(2_000_000_003, "COVERAGE_FAIL"),
                              (2_000_000_004, "COVERAGE_PASS"),
                              (2_000_000_005, "COVERAGE_PASS")]:
            rows, span, proof = fixture()
            span["first_ns"] = rows[0]["begin"]["wall_ns"] - gap
            span["last_ns"] = rows[0]["end"]["wall_ns"] + gap
            self.assertEqual(check(rows, span, proof)["status"], expected)

    def test_slow_queries_and_all_nested_request_boundaries(self):
        rows, span, proof = fixture()
        nested = copy.deepcopy(rows[0])
        nested.update(request_id="setup", begin=pair(8_100_000_000), end=pair(9_100_000_000))
        rows.append({"later": {"setup": nested}})
        self.assertEqual(check(rows, span, proof, ("nodes", "setup"))["status"], "COVERAGE_FAIL")
        span["last_ns"] = OFFSET + 11_200_000_000
        proof["stop_requested_mono_ns"] = 11_200_000_000
        self.assertEqual(check(rows, span, proof, ("nodes", "setup"))["status"], "COVERAGE_PASS")

    def test_failed_query_keeps_its_boundary_but_not_query_success(self):
        rows, span, proof = fixture()
        rows[0].update(error="timeout", http_status=None)
        result = check(rows, span, proof)
        self.assertEqual(result["status"], "COVERAGE_PASS")
        self.assertFalse(result["all_queries_succeeded"])
        self.assertEqual(result["failed_request_ids"], ["nodes"])

    def test_incomplete_or_null_body_is_not_a_successful_query(self):
        for change in ({"body_complete": False}, {"value": None}):
            rows, span, proof = fixture()
            rows[0].update(change)
            result = check(rows, span, proof)
            self.assertEqual(result["status"], "COVERAGE_PASS")
            self.assertFalse(result["all_queries_succeeded"])

    def test_missing_sample_does_not_hide_an_observed_clock_violation(self):
        rows, span, proof = fixture()
        rows[0]["begin"]["mono_after_ns"] += 1_982_293
        result = check(rows, span, proof, ("nodes", "missing"))
        self.assertEqual(result["status"], "COVERAGE_FAIL")
        self.assertIn("missing/unexpected/duplicate request boundaries", result["reasons"])
        self.assertIn("clock bracket over 1 ms", result["reasons"])
        self.assertIn("wall/monotonic mapping differs by over 1 ms", result["reasons"])

    def test_response_body_cannot_inject_clock_wrappers(self):
        rows, span, proof = fixture()
        rows[0]["value"] = {"begin": "business field", "end": "business field",
                            "request_id": "not a collector request"}
        self.assertEqual(check(rows, span, proof)["api_intervals"], 1)

    def test_missing_duplicate_or_unexpected_requests_fail(self):
        rows, span, proof = fixture()
        for invalid in ([], rows + rows, [{"unmeasured": True}]):
            self.assertEqual(check(invalid, span, proof)["status"], "COVERAGE_FAIL")
        self.assertEqual(check(rows, span, proof, ("nodes", "setup"))["status"], "COVERAGE_FAIL")

    def test_api_clock_bracket_is_not_discarded(self):
        rows, span, proof = fixture()
        rows[0]["begin"]["mono_after_ns"] += 1_000_001
        self.assertEqual(check(rows, span, proof)["status"], "COVERAGE_FAIL")

    def test_lifecycle_and_metadata_failure_branches(self):
        changes = [{"kernel_drops": 1}, {"clean_exit": False}, {"exit_code": 1},
                   {"pcap_sha256": "wrong"}, {"timestamp_type_confirmed": False},
                   {"ready_mono_ns": 2_000_000_000}, {"stop_requested_mono_ns": 9_000_000_000},
                   {"clock_pairs": []}, {"schema": "legacy"}]
        for change in changes:
            rows, span, proof = fixture()
            proof.update(change)
            self.assertEqual(check(rows, span, proof)["status"], "COVERAGE_FAIL", change)

    def test_reversed_and_noninteger_time_fail(self):
        for change in [{"mono_after_ns": 1}, {"wall_ns": float("nan")},
                       {"wall_ns": OFFSET / 1e9}, {"mono_before_ns": True}]:
            rows, span, proof = fixture()
            rows[0]["begin"].update(change)
            self.assertEqual(check(rows, span, proof)["status"], "COVERAGE_FAIL")

    def test_cross_request_clock_regression_is_not_hidden_by_sorting(self):
        rows, span, proof = fixture()
        later = copy.deepcopy(rows[0])
        later.update(request_id="later", begin=pair(2_000_000_000), end=pair(2_500_000_000))
        rows.append(later)
        result = check(rows, span, proof, ("nodes", "later"))
        self.assertEqual(result["status"], "COVERAGE_FAIL")
        self.assertIn("monotonic timestamp regression or overlapping requests", result["reasons"])

    def test_pcap_precision_order_and_truncation(self):
        for endian, magic, fraction, scale in [
            ("<", b"\xd4\xc3\xb2\xa1", 123456, 1000),
            (">", b"\xa1\xb2\xc3\xd4", 123456, 1000),
            ("<", b"\x4d\x3c\xb2\xa1", 123456789, 1),
            (">", b"\xa1\xb2\x3c\x4d", 123456789, 1),
        ]:
            header = magic + struct.pack(endian + "HHIIII", 2, 4, 0, 0, 65535, 1)
            record = struct.pack(endian + "IIII", 10, fraction, 4, 4) + b"data"
            span = gate.pcap_span(header + record)
            self.assertEqual(span["first_ns"], 10_000_000_000 + fraction * scale)
            self.assertEqual(span["resolution_ns"], scale)
            bad_fraction = struct.pack(endian + "IIII", 10, 1_000_000_000 // scale, 4, 4) + b"data"
            earlier = struct.pack(endian + "IIII", 9, fraction, 4, 4) + b"data"
            for raw in (header, header + record[:-1], header + bad_fraction,
                        header + record + earlier, header + record + b"partial"):
                with self.assertRaises(ValueError):
                    gate.pcap_span(raw)


if __name__ == "__main__":
    unittest.main()

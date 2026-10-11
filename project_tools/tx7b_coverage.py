"""Version 2 coverage evidence. Importing/evaluating this module performs no I/O.

Wall and monotonic clocks remain independent. No fitted offset, interpolation,
or wider tolerance can turn observed clock drift into a coverage pass.
"""
lazy import hashlib
lazy import math
lazy import struct
lazy import time

SCHEMA = "tx7b.capture-coverage.v2"
CONCURRENT_SCHEMA = "tx7b.capture-coverage.v3"
MARGIN_NS = 2_000_000_000
ALIGNMENT_LIMIT_NS = 1_000_000


def integer(value, name):
    if type(value) is not int or value < 0:
        raise ValueError(f"{name} must be a nonnegative integer")
    return value


def stamp(clock=time):
    """Keep the actual three reads, without a float/ISO round trip."""
    before = clock.monotonic_ns()
    wall = clock.time_ns()
    after = clock.monotonic_ns()
    return {"wall_ns": wall, "mono_before_ns": before, "mono_after_ns": after}


def clock_info(clock=time):
    return {
        name: {
            "implementation": clock.get_clock_info(name).implementation,
            "resolution_ns": math.ceil(clock.get_clock_info(name).resolution * 1e9),
            "monotonic": clock.get_clock_info(name).monotonic,
            "adjustable": clock.get_clock_info(name).adjustable,
        }
        for name in ("time", "monotonic")
    }


def requests(value):
    """Walk collector wrappers, never timestamps inside a server response."""
    if isinstance(value, dict):
        if "request_id" in value:
            if not all(key in value for key in ("begin", "end", "error", "http_status")):
                raise ValueError("incomplete timed request wrapper")
            yield value
        else:
            for child in value.values():
                yield from requests(child)
    elif isinstance(value, list):
        for child in value:
            yield from requests(child)


def pcap_span(raw):
    """Validate classic Ethernet PCAP, including fractional range and order."""
    formats = {
        b"\xd4\xc3\xb2\xa1": ("<", 1000), b"\xa1\xb2\xc3\xd4": (">", 1000),
        b"\x4d\x3c\xb2\xa1": ("<", 1), b"\xa1\xb2\x3c\x4d": (">", 1),
    }
    if len(raw) < 24 or raw[:4] not in formats:
        raise ValueError("unsupported/truncated PCAP")
    endian, resolution = formats[raw[:4]]
    major, minor, _, _, snaplen, link = struct.unpack_from(endian + "HHIIII", raw, 4)
    if (major, minor) != (2, 4) or link != 1 or not snaplen:
        raise ValueError("unsupported PCAP header")
    offset, times = 24, []
    while offset < len(raw):
        if offset + 16 > len(raw):
            raise ValueError("truncated PCAP record header")
        sec, fraction, size, original = struct.unpack_from(endian + "IIII", raw, offset)
        offset += 16
        if fraction >= 1_000_000_000 // resolution:
            raise ValueError("PCAP fractional timestamp out of range")
        if not size or size != original or size > snaplen or offset + size > len(raw):
            raise ValueError("truncated/invalid PCAP packet")
        timestamp = sec * 1_000_000_000 + fraction * resolution
        if times and timestamp < times[-1]:
            raise ValueError("PCAP timestamp regression")
        times.append(timestamp)
        offset += size
    if not times:
        raise ValueError("empty PCAP")
    return {"first_ns": times[0], "last_ns": times[-1], "records": len(times),
            "resolution_ns": resolution, "sha256": hashlib.sha256(raw).hexdigest()}


def assess(rows, span, proof, expected_request_ids):
    """Conservative bounds at every API boundary; query success is separate."""
    reasons = []
    result = {"schema": SCHEMA, "status": "COVERAGE_FAIL", "reasons": reasons,
              "alignment_limit_ns": ALIGNMENT_LIMIT_NS, "margin_ns": MARGIN_NS,
              "packet_join_proven": False, "live_measurement_executed_by_evaluator": False}
    try:
        concurrent = proof["schema"] == CONCURRENT_SCHEMA
        if proof["schema"] not in (SCHEMA, CONCURRENT_SCHEMA):
            raise ValueError("clock schema mismatch; legacy float evidence is not v2")
        result["schema"] = proof["schema"]
        wrapped = list(requests(rows))
        ids = [item["request_id"] for item in wrapped]
        expected = list(expected_request_ids)
        if not expected or len(set(expected)) != len(expected):
            raise ValueError("expected request IDs missing/duplicated")
        if len(set(ids)) != len(ids) or set(ids) != set(expected):
            # Keep the failed inventory visible while checking every boundary
            # that exists. A sampling failure must not conceal a clock failure.
            reasons.append("missing/unexpected/duplicate request boundaries")
        result.update(expected_api_intervals=len(expected),
                      missing_request_ids=sorted(set(expected) - set(ids)),
                      unexpected_request_ids=sorted(set(ids) - set(expected)))
        if not wrapped:
            raise ValueError("no actual request boundaries")
        if not proof["clean_exit"] or proof["exit_code"] != 0 or proof["kernel_drops"] != 0:
            reasons.append("capture exit/drop evidence failed")
        if proof["pcap_sha256"] != span["sha256"]:
            reasons.append("PCAP binding mismatch")
        if proof["timestamp_type"] != "host" or not proof["timestamp_type_confirmed"]:
            reasons.append("PCAP host timestamp source unconfirmed")
        info = proof["clock_info"]
        wall_res = integer(info["time"]["resolution_ns"], "wall resolution")
        mono_res = integer(info["monotonic"]["resolution_ns"], "monotonic resolution")
        if not wall_res or not mono_res or not info["monotonic"]["monotonic"]:
            raise ValueError("invalid clock metadata")
        ready = integer(proof["ready_mono_ns"], "capture readiness")
        stop = integer(proof["stop_requested_mono_ns"], "capture stop")
        lifecycle = proof["clock_pairs"]
        if len(lifecycle) != 2:
            raise ValueError("two lifecycle clock pairs required")
        pairs = [lifecycle[0]]
        beginnings, endings, failed = [], [], []
        for item in wrapped:
            begin, end = item["begin"], item["end"]
            if begin["mono_after_ns"] > end["mono_before_ns"] or begin["wall_ns"] > end["wall_ns"]:
                reasons.append("reversed API interval")
            beginnings.append(begin["mono_before_ns"] - mono_res)
            endings.append(end["mono_after_ns"] + mono_res)
            pairs.extend((begin, end))
            if (item["error"] is not None or item["http_status"] != 200
                    or ("body_complete" in item and not item["body_complete"])
                    or ("value" in item and item["value"] is None)):
                failed.append(item["request_id"])
        pairs.append(lifecycle[1])
        if concurrent:
            # The recorder serializes clock reads, not HTTP requests. Preserve
            # this recorded event order; sorting would conceal clock regressions.
            boundaries = {(r["request_id"], edge): r[edge]
                          for r in wrapped for edge in ("begin", "end")}
            events = proof["clock_events"]
            seen = set()
            for index, event in enumerate(events):
                key = (event["request_id"], event["boundary"])
                if (type(event["ordinal"]) is not int or event["ordinal"] != index or key in seen
                        or key not in boundaries or event["stamp"] != boundaries[key]):
                    raise ValueError("clock event binding/order mismatch")
                seen.add(key)
            if seen != set(boundaries):
                raise ValueError("missing clock boundary event")
            pairs = [lifecycle[0]] + [event["stamp"] for event in events] + [lifecycle[1]]
            endpoint_ends = {}
            for item in wrapped:  # Completion order; one request per endpoint.
                endpoint = item["endpoint"]
                if item["begin"]["mono_before_ns"] < endpoint_ends.get(endpoint, 0):
                    reasons.append("overlapping or reordered requests on one endpoint")
                endpoint_ends[endpoint] = item["end"]["mono_after_ns"]
        low, high, brackets = [], [], []
        for pair in pairs:
            wall = integer(pair["wall_ns"], "wall_ns")
            before = integer(pair["mono_before_ns"], "mono_before_ns")
            after = integer(pair["mono_after_ns"], "mono_after_ns")
            if after < before:
                raise ValueError("reversed clock bracket")
            bracket = after - before + 2 * mono_res
            brackets.append(bracket)
            if bracket > ALIGNMENT_LIMIT_NS:
                reasons.append("clock bracket over 1 ms")
            low.append(wall - wall_res - after - mono_res)
            high.append(wall + wall_res - before + mono_res)
        # v2 requests or v3 clock events are recorded in actual read order.
        if any(b["mono_before_ns"] < a["mono_after_ns"] for a, b in zip(pairs, pairs[1:])):
            reasons.append("monotonic timestamp regression or overlapping requests")
        if any(b["wall_ns"] < a["wall_ns"] for a, b in zip(pairs, pairs[1:])):
            reasons.append("wall timestamp regression")
        if lifecycle[0]["mono_after_ns"] > ready or lifecycle[1]["mono_before_ns"] < stop:
            reasons.append("clock pairs do not enclose capture lifecycle")
        if ready > min(beginnings) - MARGIN_NS or stop < max(endings) + MARGIN_NS:
            reasons.append("capture lifecycle margins below 2 seconds")
        spread = max(high) - min(low)
        if spread > ALIGNMENT_LIMIT_NS:
            reasons.append("wall/monotonic mapping differs by over 1 ms")
        # Worst-case conversion across all observed clock brackets. No fit.
        first_packet_latest = span["first_ns"] + span["resolution_ns"] - min(low)
        last_packet_earliest = span["last_ns"] - span["resolution_ns"] - max(high)
        pre = min(beginnings) - first_packet_latest
        post = last_packet_earliest - max(endings)
        if pre < MARGIN_NS or post < MARGIN_NS:
            reasons.append("packet margins below 2 seconds after clock uncertainty")
        result.update(api_intervals=len(wrapped), mapping_spread_ns=spread,
                      clock_bracket_max_ns=max(brackets),
                      packet_pre_roll_lower_bound_ns=pre, packet_post_roll_lower_bound_ns=post,
                      failed_request_ids=failed, all_queries_succeeded=not failed)
    except (KeyError, TypeError, ValueError) as error:
        reasons.append(str(error))
    result["reasons"] = sorted(set(reasons))
    result["status"] = "COVERAGE_FAIL" if reasons else "COVERAGE_PASS"
    return result

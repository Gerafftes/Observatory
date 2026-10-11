"""Offline supplement for the sealed 64/128/192 transport contract.

The original Mixed-V2 evaluator and its frozen limits remain unchanged. Packet
eligibility is evidence about the wire, never an assertion of server acceptance.
Input bindings and request wrappers contain private identities; output is counts.
"""
lazy import ast
lazy import argparse
lazy from collections import Counter
lazy from functools import lru_cache
lazy import hashlib
lazy import json
lazy import math
lazy from pathlib import Path
lazy import statistics
lazy import struct
lazy import zlib

lazy from project_tools import mmwave_freshness_packets as wire
lazy from project_tools import tx1_association as association
lazy from project_tools import tx7b_coverage as coverage

SCHEMA = "tx1.transport-comparison.v1"
REQUEST_ENDPOINTS = ("TX1", "RX1", "RX2", "RX3", "RX4", "MMWAVE1", "MMWAVE2",
                     "nodes", "mmwave", "ready", "profiles", "metrics")
FROZEN_LIMITS_SHA256 = "886c8875f45fc800a3c68eefc3554dc8476ff07adbcbf9e1879b9466ea197777"
PRIMARY_METRICS = ("sounding_rate", "rx4_rate", "sounding_interval_p95",
                   "sounding_max_gap", "rx4_max_gap", "sequence_gap_ratio")
NODE_COUNTERS = ("accepted_csi_frames", "sequence_observations",
                 "inferred_lost_frames", "skipped_grid_frames")
RX_COUNTERS = ("enqueued", "queue_full_drops", "sent", "retries", "errors", "abandoned",
               "socket_rebuilds", "socket_send_failures", "enomem_backoffs")
LIMIT_COUNTERS = RX_COUNTERS[1:2] + RX_COUNTERS[3:]
TX_COUNTERS = ("sent_packets", "send_errors", "missed_deadlines", "socket_rebuilds", "wifi_restarts")
INGRESS_COUNTERS = ("udp_datagrams_from_os", "udp_queue_overflow_datagrams",
                    "udp_queue_stale_datagrams", "udp_receive_errors", "csi_datagrams_received",
                    "csi_frames_accepted_new", "csi_frames_discarded",
                    "csi_queue_overflow_datagrams", "csi_queue_stale_datagrams")
INGRESS_ERROR_COUNTERS = ("udp_queue_overflow_datagrams", "udp_queue_stale_datagrams",
                          "udp_receive_errors", "csi_queue_overflow_datagrams", "csi_queue_stale_datagrams")
RADAR_DIAGNOSTICS = ("uart_bytes_received", "radar_frames_valid", "udp_packets_sent",
                     "udp_packets_acked", "udp_ack_timeouts", "udp_send_failures", "udp_retransmissions")
RADAR_SERVER_COUNTERS = ("raw_udp_packets", "packets_received", "packets_rejected",
                         "packets_lost", "reboot_count")
LEGACY_PATH = (Path(__file__).resolve().parents[1] /
               "software/ruview/firmware/esp32-csi-tx/tools/tx1_measure.py")


@lru_cache(maxsize=1)
def legacy_helpers():
    """Load only existing pure helpers, without importing the live collector.

    Removing the lazy keyword allows their unchanged bodies to run on the host's
    existing Python. Imports, main, serial/provisioning and live functions are
    excluded from the AST that is executed.
    """
    raw = LEGACY_PATH.read_bytes()
    tree = ast.parse(raw.decode().replace("lazy import ", "import ")
                     .replace("lazy from ", "from "))
    names = {"MeasurementError", "reject", "gaps", "deltas", "digest",
             "rx4_packet_kind", "audit_packets"}
    body = [item for item in tree.body
            if isinstance(item, (ast.FunctionDef, ast.ClassDef)) and item.name in names]
    if {item.name for item in body} != names:
        raise ValueError("existing pure measurement helpers are missing")
    namespace = {"hashlib": hashlib, "statistics": statistics, "struct": struct,
                 "zlib": zlib, "parse_mac": lambda value: bytes.fromhex(value.replace(":", "")),
                 "EXPECTED_RX4_RASTER": {"frequency_mhz": 2437, "antennas": 1,
                                          "ppdu": 0, "stable_flags": 0},
                 "VALID_RX4_LENGTHS": (64, 128), "RX4_TRANSIENT_FLAGS": 0x10}
    exec(compile(ast.Module(body=body, type_ignores=[]), str(LEGACY_PATH), "exec"), namespace)
    return namespace


def packet_error(kind, reason, **fields):
    return {"kind": kind, "valid": False, "reasons": [reason], **fields}


def radar_shape(value):
    """Mirror RadarPacket/RadarTarget serde types and validate_packet_shape."""
    required = {"schema", "node_id", "mode", "boot_id", "sequence", "sensor_time_us", "unix_time_ms", "targets"}
    if not isinstance(value, dict) or not required <= value.keys() or set(value) - required - {"coordinate_frame"}:
        return False
    if value["mode"] not in ("calibration", "reference"):
        return False
    if any(type(value[k]) is not int or not -(2 ** 63) <= value[k] < 2 ** 63 for k in ("sensor_time_us", "unix_time_ms")):
        return False
    if value["sensor_time_us"] < 0 or not isinstance(value["targets"], list) or len(value["targets"]) != 3:
        return False
    frame = value.get("coordinate_frame", {"local": "x_right_y_forward_mm", "room": "x_length_z_width_mm",
                                           "origin_x_mm": 0, "origin_z_mm": 0, "yaw_mdeg": 0, "raw_x_inverted": False})
    if (not isinstance(frame, dict) or set(frame) != {"local", "room", "origin_x_mm", "origin_z_mm", "yaw_mdeg", "raw_x_inverted"}
            or frame["local"] != "x_right_y_forward_mm" or frame["room"] != "x_length_z_width_mm"
            or type(frame["raw_x_inverted"]) is not bool):
        return False
    if any(type(frame[k]) is not int or not -(2 ** 31) <= frame[k] < 2 ** 31 for k in ("origin_x_mm", "origin_z_mm", "yaw_mdeg")):
        return False
    if abs(frame["yaw_mdeg"]) > 360_000:
        return False
    target_required = {"slot", "present", "x_mm", "y_mm", "speed_cm_s", "resolution_mm"}
    ranges = {"slot": (0, 255), "x_mm": (-32768, 32767), "y_mm": (-32768, 32767),
              "speed_cm_s": (-32768, 32767), "resolution_mm": (0, 65535),
              "room_x_mm": (-(2 ** 31), 2 ** 31 - 1), "room_z_mm": (-(2 ** 31), 2 ** 31 - 1)}
    for index, target in enumerate(value["targets"]):
        if (not isinstance(target, dict) or not target_required <= target.keys()
                or set(target) - target_required - {"room_x_mm", "room_z_mm"}
                or type(target["present"]) is not bool or target["slot"] != index + 1):
            return False
        if any(type(target.get(k, 0)) is not int or not low <= target.get(k, 0) <= high
               for k, (low, high) in ranges.items()):
            return False
        if not target["present"] and any(target.get(k, 0) != 0 for k in ranges if k != "slot"):
            return False
    return True


def unique_json_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("duplicate JSON field")
        result[key] = value
    return result


def classify_packet(packet, binding):
    """Validate the actual packet's class, protocol, source and own boot binding."""
    data, port = packet["payload"], packet["dst_port"]
    if port == 4210:
        state = {}
        try:
            legacy_helpers()["audit_packets"](
                [{"data": data, "port": port, "src": packet["src_ip"],
                  "src_mac": packet["src_mac"], "t": packet["timestamp_ns"] / 1e9}],
                {"ap_mac": binding["ap_mac"]}, "after", "1", state, binding["tx_boot_id"])
        except ValueError as error:
            return packet_error("damaged_sounding", str(error))
        return {"kind": "sounding", "valid": True, "reasons": [],
                "sequence": int.from_bytes(data[8:12], "little"),
                "boot_ms": int.from_bytes(data[12:16], "little")}
    if port == 5005:
        matches = [(name, device) for name, device in binding["rx"].items()
                   if device["ip"] == packet["src_ip"]]
        if len(matches) != 1:
            return packet_error("unknown_rx_source", "unbound RX source")
        name, device = matches[0]
        fields = {"participant": name}
        if (packet["src_mac"] != device["mac"].replace(":", "").lower()
                or packet["dst_ip"] != binding["rx_target_ip"]
                or ("host_mac" in binding and packet["dst_mac"] != binding["host_mac"].replace(":", "").lower())):
            return packet_error("damaged_rx", "RX link source/destination binding", **fields)
        if len(data) < 4:
            return packet_error("damaged_rx", "RX packet type truncated", **fields)
        magic = int.from_bytes(data[:4], "little")
        if magic == 0xC5110001:
            if len(data) < 20:
                return packet_error("damaged_csi", "CSI header truncated", **fields)
            carriers = int.from_bytes(data[6:8], "little")
            fields["subcarriers"] = carriers
            if carriers not in (64, 128, 192):
                return packet_error("unknown_csi_grid", "unknown subcarrier count", **fields)
            if data[4] != device["node_id"]:
                return packet_error("damaged_csi", "CSI own node binding", **fields)
            # The shared decoder binds RX4. Check the actual node first; changing
            # only that checked byte reuses identical header/trailer validation.
            decoded = wire.csi_header(data[:4] + b"\x04" + data[5:],
                                      device["boot_id"], binding["source_mac_sha256"])
            if not decoded["header_and_trailers_match"]:
                return packet_error("damaged_csi", "CSI structure/source/protocol/boot trailers", **fields)
            # Bandwidth, STBC, LDPC and transient sync are defined layout bits.
            # In particular the observed 192/0x11 class is complete transport.
            if (decoded["antennas"] != 1 or decoded["frequency_mhz"] != 2437
                    or decoded["ppdu"] not in (0, 1, 2, 3) or decoded["flags"] & ~0x1D):
                return packet_error("damaged_csi", "CSI undefined protocol/layout fields", **fields)
            stable_flags = decoded["flags"] & ~0x10
            eligible = (carriers == 128 and decoded["ppdu"] == 0 and stable_flags == 0)
            reason = None if eligible else (f"subcarriers_{carriers}" if carriers != 128
                else f"128_ppdu_{decoded['ppdu']}_stable_flags_{stable_flags}")
            return {"kind": f"csi_{carriers}", "valid": True, "reasons": [], **fields,
                    "sequence": decoded["sequence"], "fixed_grid_eligible": eligible,
                    "transport_grid": {"frequency_mhz": decoded["frequency_mhz"], "antennas": decoded["antennas"],
                        "subcarriers": carriers, "ppdu": decoded["ppdu"], "stable_flags": stable_flags},
                    "bandwidth_mhz": 40 if stable_flags & 1 else 20,
                    "server_accepted": None,
                    "exclusion_reason": reason}
        if magic not in (0xC511A110, 0xC5110006, 0xC5118100):
            return packet_error("unknown_rx_type", "unknown RX protocol type", **fields)
        normalized = data
        if magic in (0xC511A110, 0xC5110006):
            if len(data) < 5 or data[4] != device["node_id"]:
                return packet_error("damaged_rx", "auxiliary packet own node binding", **fields)
            normalized = data[:4] + b"\x04" + data[5:]
        if magic in (0xC5110006, 0xC5118100):
            if len(data) < 8 or zlib.crc32(data[:-4]) != int.from_bytes(data[-4:], "little"):
                return packet_error("damaged_rx", "auxiliary packet original CRC", **fields)
            if magic == 0xC5118100 and data[16:24] != bytes([device["node_id"]]) + bytes(7):
                return packet_error("damaged_rx", "mesh health own node binding", **fields)
            normalized = normalized[:-4] + struct.pack("<I", zlib.crc32(normalized[:-4]))
        try:
            kind = legacy_helpers()["rx4_packet_kind"](
                normalized, device["boot_id"], bytes.fromhex(binding["source_mac_sha256"]))
        except ValueError as error:
            return packet_error("damaged_rx", str(error), **fields)
        return {"kind": kind, "valid": True, "reasons": [], **fields,
                "boot_binding_available_in_packet": False}
    try:
        identity = wire.radar_identity(packet, binding["radar"], binding["host_ip"])
    except (ValueError, TypeError, KeyError):
        return packet_error("damaged_radar", "invalid radar identity structure")
    if identity["kind"] in ("radar", "ack"):
        name = identity["node_id"]
        if not identity["bound"] or identity["boot_id"] != binding["radar"][name]["boot_id"]:
            return packet_error("damaged_radar", "radar own source/protocol/boot binding", participant=name)
        mac_key = "src_mac" if identity["kind"] == "ack" else "dst_mac"
        if "host_mac" in binding and packet[mac_key] != binding["host_mac"].replace(":", "").lower():
            return packet_error("damaged_radar", "radar host link MAC binding", participant=name)
        if identity["kind"] == "radar":
            try:
                value = json.loads(data, object_pairs_hook=unique_json_object)
            except (ValueError, UnicodeDecodeError):
                return packet_error("damaged_radar", "radar duplicate/invalid JSON fields", participant=name)
            if not radar_shape(value):
                return packet_error("damaged_radar", "radar strict packet/target structure", participant=name)
            if "mode" in binding["radar"][name] and value["mode"] != binding["radar"][name]["mode"]:
                return packet_error("damaged_radar", "radar bound measurement mode", participant=name)
        return {"kind": identity["kind"], "valid": True, "reasons": [],
                "participant": name, "sequence": identity["sequence"]}
    return packet_error("unknown_packet", "unbound/unknown radar or packet protocol")


def classify_capture(pcap_raw, binding):
    classified, radar_records, radar_indices, previous = [], [], [], {}
    for packet in wire.udp_packets(pcap_raw):
        result = classify_packet(packet, binding) | {"timestamp_ns": packet["timestamp_ns"]}
        if result["valid"] and (result["kind"] == "sounding" or result["kind"].startswith("csi_")):
            name = result.get("participant", "TX1")
            old = previous.get(name)
            result["sequence_error"] = bool(old and (result["sequence"] <= old["sequence"]
                or (name == "TX1" and result["boot_ms"] < old["boot_ms"])))
            previous[name] = result
        if result["valid"] and result["kind"] in ("radar", "ack"):
            identity = wire.radar_identity(packet, binding["radar"], binding["host_ip"])
            result["_wire"] = packet | identity  # Internal only; never returned in reports.
            radar_indices.append(len(classified))
            radar_records.append(result["_wire"])
            if result["kind"] == "radar":
                result["ack_pair_confirmed"] = False
        classified.append(result)
    for radar_index, _ in wire.radar_ack_attempt_pairs(radar_records):
        classified[radar_indices[radar_index]]["ack_pair_confirmed"] = True
    return classified


def ordered_rows(rows, key, window):
    result = sorted((row for row in rows if row["endpoint_key"] == key
                     and window["first_slot"] <= row["slot"] <= window["last_slot"]),
                    key=lambda row: row["slot"])
    expected = list(range(window["first_slot"], window["last_slot"] + 1))
    if [row["slot"] for row in result] != expected:
        raise ValueError(f"missing/duplicate {key} request slots")
    if any(row["error"] is not None or row["http_status"] != 200
           or not row["body_complete"] or not isinstance(row["value"], dict) for row in result):
        raise ValueError(f"incomplete {key} responses")
    if any(not 0 < b["end"]["mono_after_ns"] - a["end"]["mono_after_ns"] <= 2_000_000_000
           for a, b in zip(result, result[1:])):
        raise ValueError(f"{key} observation gap")
    return result


def nodes_from_row(row):
    nodes = row["value"]["nodes"]
    if sorted(node["node_id"] for node in nodes) != [1, 2, 3, 4]:
        raise ValueError("four unique RX nodes required")
    return {f"RX{node['node_id']}": node for node in nodes}


def grid_accounting(rows, classified, name):
    """Same conservative HTTP boundary brackets; 64 and 192 stay separate."""
    a, b = rows[0], rows[-1]
    excluded = [p for p in classified if p.get("participant") == name
                and p["valid"] and p.get("exclusion_reason")]
    reasons = {}
    kinds = {"subcarriers_64", "subcarriers_192"} | {p["exclusion_reason"] for p in excluded}
    for kind in sorted(kinds):
        times = [p["timestamp_ns"] for p in excluded if p["exclusion_reason"] == kind]
        reasons[kind] = {
            "minimum": sum(a["end"]["wall_ns"] <= t < b["begin"]["wall_ns"] for t in times),
            "maximum": sum(a["begin"]["wall_ns"] <= t < b["end"]["wall_ns"] for t in times)}
    delta = legacy_helpers()["deltas"](
        nodes_from_row(a)[name], nodes_from_row(b)[name], NODE_COUNTERS)["skipped_grid_frames"]
    minimum = sum(r["minimum"] for r in reasons.values())
    maximum = sum(r["maximum"] for r in reasons.values())
    return {"skipped_grid_frames": delta, "exclusion_packet_brackets": reasons,
            "minimum": minimum, "maximum": maximum,
            "status": "PASS" if minimum <= delta <= maximum else "FAIL",
            "individual_packets_validated": True, "radio_loss": False,
            "method": "own nodes request begin/end; no numeric tolerance"}


def sequence_evidence(packets):
    sequences = [p["sequence"] for p in packets]
    differences = [b - a for a, b in zip(sequences, sequences[1:])]
    missing = sum(max(0, delta - 1) for delta in differences)
    observations = len(differences)
    return {"observations": observations, "missing": missing,
            "denominator": observations + missing,
            "nonincreasing": sum(delta <= 0 for delta in differences),
            "scope": "all validated CSI transport classes; excludes auxiliary packets"}


def historical_reference(metrics, limits):
    results = {}
    for key, limit in limits["limits"].items():
        value = metrics.get(key)
        if value is None:
            results[key] = {"status": "NOT_COMPARABLE", "value": None, "limit": limit}
        else:
            passed = value >= limit["value"] if limit["direction"] == "min" else value <= limit["value"]
            results[key] = {"status": "PASS" if passed else "FAIL", "value": value, "limit": limit}
    statuses = {value["status"] for value in results.values()}
    status = "REFERENCE_FAILED" if "FAIL" in statuses else (
        "NOT_COMPARABLE" if "NOT_COMPARABLE" in statuses else "REFERENCE_MET")
    return {"status": status, "targets": results, "original_mixed_v2_pass": False,
            "identical_measurement_conditions_proven": False}


def noncomparable_reference(metrics, limits, reason):
    """Retain measured numbers without assigning formal reference outcomes."""
    result = historical_reference(metrics, limits)
    result["descriptive_numeric_status"] = result["status"]
    result["status"] = "NOT_COMPARABLE"
    result["reason"] = reason
    for target in result["targets"].values():
        target["descriptive_value"] = target["value"]
        target["descriptive_numeric_status"] = target["status"]
        target.update(status="NOT_COMPARABLE", value=None, reason=reason)
    return result


def validate_limits(limits):
    expected = set(PRIMARY_METRICS) | {"rx4_" + key for key in LIMIT_COUNTERS}
    if (limits.get("schema"), limits.get("rule"), limits.get("warmup_seconds"),
            limits.get("window_seconds"), limits.get("window_count")) != (
            1, "old-three-window-envelope", 60, 120, 3):
        raise ValueError("frozen reference contract differs")
    if set(limits["limits"]) != expected:
        raise ValueError("missing/unknown historical reference targets")
    for key, limit in limits["limits"].items():
        direction = "min" if key in ("sounding_rate", "rx4_rate") else "max"
        if (limit["direction"] != direction or isinstance(limit["value"], bool)
                or not math.isfinite(limit["value"]) or limit["value"] < 0):
            raise ValueError("historical reference direction/value differs")


def evaluate_radars(run, binding, classified, window):
    """Per-radar transport, separate from content and physical position quality."""
    result = {}
    delta = legacy_helpers()["deltas"]
    for name, device in binding["radar"].items():
        radar = {"status": "FAIL", "reasons": [], "physical_position_accuracy_proven": False}
        result[name] = radar
        try:
            own = ordered_rows(run["rows"], name, window)
            server = ordered_rows(run["rows"], "mmwave", window)
            values = []
            for row in server:
                matches = [sensor for sensor in row["value"]["sensors"] if sensor["node_id"] == name]
                if len(matches) != 1 or matches[0]["boot_id"] != device["boot_id"]:
                    raise ValueError(f"{name} own server boot identity")
                values.append(matches[0])
            for row in own:
                if row["value"]["node_id"] != name:
                    raise ValueError(f"{name} own device identity")
            for first, last in zip(own, own[1:]):
                delta(first["value"]["diagnostics"], last["value"]["diagnostics"], RADAR_DIAGNOSTICS)
            for first, last in zip(values, values[1:]):
                delta(first, last, RADAR_SERVER_COUNTERS + ("sequence",))
            diagnostics = delta(own[0]["value"]["diagnostics"], own[-1]["value"]["diagnostics"], RADAR_DIAGNOSTICS)
            counters = delta(values[0], values[-1], RADAR_SERVER_COUNTERS + ("sequence",))
            rejection_keys = set(values[0]["reject_reasons"]) | set(values[-1]["reject_reasons"])
            rejections = delta({key: values[0]["reject_reasons"].get(key, 0) for key in rejection_keys},
                               {key: values[-1]["reject_reasons"].get(key, 0) for key in rejection_keys}, rejection_keys)
            a, b = server[0]["end"]["wall_ns"], server[-1]["end"]["wall_ns"]
            selected = [p for p in classified if p.get("participant") == name and p["valid"]
                        and a <= p["timestamp_ns"] < b]
            packet_counts = dict(Counter(p["kind"] for p in selected))
            maximum_age = max(v["packet_age_ms"] for v in values)
            if maximum_age > 1000:
                radar["reasons"].append("packet age exceeds bound 1000 ms")
            if any(diagnostics[key] <= 0 for key in RADAR_DIAGNOSTICS[:4]) or any(
                    counters[key] <= 0 for key in ("raw_udp_packets", "sequence")):
                radar["reasons"].append("required UART/valid/sent/ACK/raw/sequence progress absent")
            if any(diagnostics[key] != 0 for key in RADAR_DIAGNOSTICS[4:]) or any(
                    counters[key] != 0 for key in ("packets_lost", "reboot_count")):
                radar["reasons"].append("new radar transport error/loss/reboot")
            if packet_counts.get("radar", 0) <= 0 or packet_counts.get("ack", 0) <= 0:
                radar["reasons"].append("packet/ACK path absent in capture")
            data_packets = [p for p in selected if p["kind"] == "radar"]
            matched = sum(p.get("ack_pair_confirmed") is True for p in data_packets)
            unmatched = len(data_packets) - matched
            if unmatched:
                radar["reasons"].append("individual radar datagrams lack bound reverse-port boot/sequence ACK")
            protocol_rejections = {key: value for key, value in rejections.items()
                                   if value and key not in ("room_bounds", "no_target", "multi_target")}
            if protocol_rejections:
                radar["reasons"].append("new radar protocol/identity/mode/content errors")
            radar.update(own_diagnostic_deltas=diagnostics, server_deltas=counters,
                         content_rejection_deltas=rejections, packet_age_ms_max=maximum_age,
                         matched_ack_datagrams=matched, unmatched_ack_datagrams=unmatched,
                         protocol_rejection_deltas=protocol_rejections,
                         packet_counts=packet_counts, content_rejections_are_transport_loss=False,
                         no_target_is_position_proof=False,
                         own_counter_duration_seconds=(own[-1]["end"]["mono_after_ns"] - own[0]["end"]["mono_after_ns"]) / 1e9,
                         server_counter_duration_seconds=(server[-1]["end"]["mono_after_ns"] - server[0]["end"]["mono_after_ns"]) / 1e9)
        except (ValueError, KeyError, TypeError, IndexError) as error:
            radar["reasons"].append(str(error))
        radar["status"] = "FAIL" if radar["reasons"] else "PASS"
    return result


def evaluate_window(run, binding, classified, window, limits):
    result = {"index": window["index"], "status": "CURRENT_FUNCTION_FAIL", "reasons": [],
              "metrics": {}, "participants": {}, "packet_errors": {}}
    delta = legacy_helpers()["deltas"]
    gaps = legacy_helpers()["gaps"]
    try:
        rows = ordered_rows(run["rows"], "nodes", window)
        snapshots = [nodes_from_row(row) for row in rows]
        a, b = rows[0], rows[-1]
        duration = (b["end"]["mono_after_ns"] - a["end"]["mono_after_ns"]) / 1e9
        wall_duration = (b["end"]["wall_ns"] - a["end"]["wall_ns"]) / 1e9
        if abs(duration - 120) > 1 or abs(wall_duration - duration) > .05:
            raise ValueError("own nodes window/clock boundary invalid")
        start, end = a["end"]["wall_ns"], b["end"]["wall_ns"]
        selected = [p for p in classified if start <= p["timestamp_ns"] < end]
        result["packet_errors"] = dict(Counter(p["kind"] for p in selected if not p["valid"]))
        result["packet_sequence_errors"] = dict(Counter(p.get("participant", "TX1") for p in selected if p.get("sequence_error")))
        if result["packet_errors"]:
            result["reasons"].append("unknown/damaged/unbound packets")
        if result["packet_sequence_errors"]:
            result["reasons"].append("continuous packet sequence/time reset/duplicate/wrap")
        sounding = [p for p in selected if p["kind"] == "sounding" and p["valid"]]
        tx_gaps = gaps([p["timestamp_ns"] / 1e9 for p in sounding], start / 1e9, end / 1e9)
        tx_sequence = sequence_evidence(sounding)
        result["sounding"] = {"count": len(sounding), "intervals": tx_gaps, "sequence": tx_sequence}
        result["metrics"].update(sounding_rate=tx_gaps["rate"],
                                 sounding_interval_p95=tx_gaps["interval_p95"], sounding_max_gap=tx_gaps["max_gap"])
        if tx_sequence["nonincreasing"] or any(b["boot_ms"] < a["boot_ms"] for a, b in zip(sounding, sounding[1:])):
            result["reasons"].append("TX sequence/time reset/duplicate/wrap")
        for name, device in binding["rx"].items():
            try:
                nodes = [snapshot[name] for snapshot in snapshots]
                if any(node["boot_id"] != device["boot_id"] or node["protocol_version"] != 2
                       or not all(node.get(key) is True for key in (
                           "identity_valid", "identity_matches_setup", "source_binding_attested",
                           "source_matched_filter", "filter_enforced")) for node in nodes):
                    raise ValueError(f"{name} server identity/setup binding")
                counters = delta(nodes[0], nodes[-1], NODE_COUNTERS)
                for previous, current in zip(nodes, nodes[1:]):
                    delta(previous, current, NODE_COUNTERS)
                packets = [p for p in selected if p.get("participant") == name and p["valid"]]
                csi = [p for p in packets if p["kind"].startswith("csi_")]
                eligible = [p for p in csi if p["fixed_grid_eligible"]]
                grid = grid_accounting(rows, classified, name)
                maximum_age = max(node["last_seen_ms"] for node in nodes)
                accepted_gap = 0
                last_progress = rows[0]["end"]["mono_after_ns"]
                for previous, current, row in zip(nodes, nodes[1:], rows[1:]):
                    if current["accepted_csi_frames"] > previous["accepted_csi_frames"]:
                        last_progress = row["end"]["mono_after_ns"]
                    accepted_gap = max(accepted_gap, (row["end"]["mono_after_ns"] - last_progress) / 1e9)
                eligible_gaps = gaps([p["timestamp_ns"] / 1e9 for p in eligible], start / 1e9, end / 1e9)
                sequence = sequence_evidence(csi)
                if (grid["status"] != "PASS" or maximum_age >= 2000
                        or (name == "RX4" and (accepted_gap >= 2 or eligible_gaps["max_gap"] >= 2))
                        or counters["accepted_csi_frames"] <= 0
                        or sequence["nonincreasing"]):
                    result["reasons"].append(f"{name} grid/freshness/progress/sequence")
                participant = {"packet_counts": dict(Counter(p["kind"] for p in packets)),
                               "server_deltas": counters,
                               "grid_accounting": grid, "last_seen_ms_max": maximum_age,
                               "accepted_progress_gap_seconds": accepted_gap,
                               "fixed_128_intervals": eligible_gaps, "transport_sequence": sequence}
                result["participants"][name] = participant
            except (ValueError, KeyError, TypeError, IndexError) as error:
                result["participants"][name] = {"status": "FAIL", "reasons": [str(error)]}
                result["reasons"].append(f"{name} server/wire evidence incomplete: {error}")
            # RX status sampling is an independent gate. Missing own responses
            # must not erase complete server/wire evidence or other endpoints.
            try:
                own = ordered_rows(run["rows"], name, window)
                for row in own:
                    value = row["value"]
                    if (value["node_id"] != name or value["boot_id"] != device["boot_id"]
                            or value["image_sha256"] != device["image_sha256"]):
                        raise ValueError(f"{name} own status identity")
                for first, last in zip(own, own[1:]):
                    delta(first["value"]["transport"], last["value"]["transport"], RX_COUNTERS)
                    delta(first["value"], last["value"], ("sequence",))
                transport = delta(own[0]["value"]["transport"], own[-1]["value"]["transport"], RX_COUNTERS)
                sequence_delta = delta(own[0]["value"], own[-1]["value"], ("sequence",))["sequence"]
                if sequence_delta <= 0 or transport["sent"] <= 0 or transport["enqueued"] <= 0:
                    result["reasons"].append(f"{name} own sequence/sent/enqueued progress absent")
                if any(transport[key] != 0 for key in LIMIT_COUNTERS):
                    result["reasons"].append(f"{name} new device transport errors")
                result["participants"][name].update(
                    transport_deltas=transport, own_sequence_delta=sequence_delta,
                    own_evidence_status="COMPLETE",
                    own_counter_duration_seconds=(
                        own[-1]["end"]["mono_after_ns"] - own[0]["end"]["mono_after_ns"]) / 1e9)
            except (ValueError, KeyError, TypeError, IndexError) as error:
                result["participants"][name].update(own_evidence_status="FAIL", own_evidence_reasons=[str(error)])
                result["reasons"].append(f"{name} independent evidence incomplete: {error}")
        rx4 = result["participants"]["RX4"]
        if "server_deltas" in rx4:
            counters = rx4["server_deltas"]
            denominator = counters["sequence_observations"] + counters["inferred_lost_frames"]
            result.update(duration=duration, sequence_denominator=denominator,
                          sequence_gap_scope="server accepted 128; valid 64/192 exclusions remain in inferred gaps; not radio loss")
            result["metrics"].update(
                rx4_rate=counters["accepted_csi_frames"] / duration,
                rx4_max_gap=rx4["fixed_128_intervals"]["max_gap"],
                sequence_gap_ratio=counters["inferred_lost_frames"] / denominator if denominator else None)
            if denominator <= 0:
                result["reasons"].append("no RX4 sequence denominator")
        if "transport_deltas" in rx4:
            result["metrics"].update({"rx4_" + key: rx4["transport_deltas"][key] for key in LIMIT_COUNTERS})
    except (ValueError, KeyError, TypeError, IndexError) as error:
        result["reasons"].append(str(error))
    for key, counter_keys in (("TX1", TX_COUNTERS), ("metrics", INGRESS_COUNTERS)):
        try:
            own = ordered_rows(run["rows"], key, window)
            values = [row["value"] if key == "TX1" else row["value"]["csi_ingress"]["counters"] for row in own]
            for first, last in zip(values, values[1:]):
                delta(first, last, counter_keys)
            result["tx_deltas" if key == "TX1" else "server_ingress_global_deltas"] = delta(values[0], values[-1], counter_keys)
            result[key + "_counter_duration_seconds"] = (
                own[-1]["end"]["mono_after_ns"] - own[0]["end"]["mono_after_ns"]) / 1e9
            if key == "TX1":
                tx = result["tx_deltas"]
                if tx["sent_packets"] <= 0 or any(tx[k] != 0 for k in ("send_errors", "socket_rebuilds", "wifi_restarts")):
                    result["reasons"].append("TX own sounding progress/error contract")
            elif any(result["server_ingress_global_deltas"][name] != 0 for name in INGRESS_ERROR_COUNTERS):
                result["reasons"].append("new global ingress transport errors")
        except (ValueError, KeyError, TypeError, IndexError) as error:
            result["reasons"].append(f"{key} independent evidence incomplete: {error}")
    result["radars"] = evaluate_radars(run, binding, classified, window)
    result["reasons"].extend(f"{name} radar transport" for name, radar in result["radars"].items() if radar["status"] != "PASS")
    result["status"] = "CURRENT_FUNCTION_FAIL" if result["reasons"] else "CURRENT_FUNCTION_PASS"
    result["historical_reference"] = noncomparable_reference(
        result["metrics"], limits, "complete three-window protocol and run coverage not assessed")
    return result


def assess_scope_coverage(run, pcap_raw, mode):
    """The scope fixes mandatory samples; input declarations cannot shrink them."""
    slots = {"comparison": 421, "seven_clients": 121}[mode]
    expected = [f"{slot}:{endpoint}" for slot in range(slots) for endpoint in REQUEST_ENDPOINTS]
    expected_set = set(expected)
    evidence = run.get("coverage_proof", run.get("proof", {}))
    if not isinstance(evidence, dict):
        evidence = {}
    proof = coverage.assess(run["rows"], coverage.pcap_span(pcap_raw), evidence, expected)
    reasons = []
    rows = run["rows"]
    if any(not isinstance(row, dict) or type(row.get("slot")) is not int or row.get("endpoint_key") not in REQUEST_ENDPOINTS
           or row.get("request_id") != f"{row.get('slot')}:{row.get('endpoint_key')}" for row in rows):
        reasons.append("request IDs do not match mandatory slot/endpoint wrappers")
    declarations = [evidence.get("expected_request_ids")]
    if "expected_request_ids" in run:
        declarations.append(run["expected_request_ids"])
    if "planned_slot_ids" in evidence:
        declarations.append(evidence["planned_slot_ids"])
    for declared in declarations:
        if (not isinstance(declared, list) or len(declared) != len(expected)
                or not all(isinstance(value, str) for value in declared)
                or set(declared) != expected_set):
            reasons.append("declared request inventory missing/duplicated/differs from fixed scope")
            break
    proof["mandatory_api_intervals"] = len(expected)
    if reasons:
        proof["status"] = "COVERAGE_FAIL"
        proof["reasons"] = proof.get("reasons", []) + reasons
    return proof


def evaluate(run, binding, pcap_raw, limits):
    """All three windows survive numeric failures. No network/device operations."""
    validate_limits(limits)
    classified = classify_capture(pcap_raw, binding)
    windows = [evaluate_window(run, binding, classified, window, limits) for window in run["windows"]]
    errors = dict(Counter(p["kind"] for p in classified if not p["valid"]))
    sequence_errors = dict(Counter(p.get("participant", "TX1") for p in classified if p.get("sequence_error")))
    result = {"schema": SCHEMA, "windows": windows, "capture_packet_errors": errors,
              "capture_packet_sequence_errors": sequence_errors,
              "capture_packet_counts": dict(Counter(p["kind"] for p in classified)),
              "legacy_evaluator_sha256": hashlib.sha256(LEGACY_PATH.read_bytes()).hexdigest(),
              "original_mixed_v2_pass": False, "new_baseline": False,
              "live_measurement_executed_by_evaluator": False}
    epoch = run.get("epoch_mono_ns")
    protocol_fields = ("index", "first_slot", "last_slot", "begin_mono_ns", "end_mono_ns")
    protocol = (type(epoch) is int and epoch >= 0 and run.get("warmup_seconds", 60) == 60
                and len(windows) == 3 and all(
                    all(type(window.get(key)) is int for key in protocol_fields)
                    and tuple(window.get(key) for key in protocol_fields) == (
                        index + 1, 60 + index * 120, 180 + index * 120,
                        epoch + (60 + index * 120) * 1_000_000_000,
                        epoch + (180 + index * 120) * 1_000_000_000)
                    for index, window in enumerate(run["windows"])))
    result["protocol_complete"] = protocol
    result["warmup_excluded_from_metrics"] = protocol
    proof = assess_scope_coverage(run, pcap_raw, "comparison")
    result["coverage"] = proof
    captured = run.get("result", {})
    capture_complete = (captured.get("error") is None and captured.get("scope_within_budget", True)
                        and captured.get("cleanup_within_budget", True) and not captured.get("skipped", 0))
    result["status"] = "CURRENT_FUNCTION_PASS" if (protocol and capture_complete and not errors and not sequence_errors
        and proof["status"] == "COVERAGE_PASS" and proof["all_queries_succeeded"]
        and all(w["status"] == "CURRENT_FUNCTION_PASS" for w in windows)) else "CURRENT_FUNCTION_FAIL"
    comparable = (protocol and capture_complete and not errors and not sequence_errors
                  and proof["status"] == "COVERAGE_PASS" and proof["all_queries_succeeded"])
    for window in windows:
        if comparable:
            window["historical_reference"] = historical_reference(window["metrics"], limits)
        else:
            window["historical_reference"] = noncomparable_reference(
                window["metrics"], limits, "incomplete protocol/capture/coverage or invalid bound packets/sequences")
    statuses = {w["historical_reference"]["status"] for w in windows}
    result["historical_reference_status"] = "NOT_COMPARABLE" if not comparable else "REFERENCE_FAILED" if "REFERENCE_FAILED" in statuses else (
        "NOT_COMPARABLE" if not protocol or "NOT_COMPARABLE" in statuses else "REFERENCE_MET")
    return result


def evaluate_seven_clients(run, binding, pcap_raw, limits):
    """A transport pass cannot fill missing identified association evidence."""
    validate_limits(limits)
    classified = classify_capture(pcap_raw, binding)
    windows = run["windows"]
    proof = assess_scope_coverage(run, pcap_raw, "seven_clients")
    transport = [evaluate_window(run, binding, classified, window, limits) for window in windows]
    errors = dict(Counter(p["kind"] for p in classified if not p["valid"]))
    sequence_errors = dict(Counter(p.get("participant", "TX1") for p in classified if p.get("sequence_error")))
    protocol = (len(windows) == 1 and windows[0]["first_slot"] == 0 and windows[0]["last_slot"] == 120
                and windows[0]["end_mono_ns"] - windows[0]["begin_mono_ns"] == 120_000_000_000)
    captured = run.get("result", {})
    complete = captured.get("error") is None and captured.get("scope_within_budget", True) and captured.get("cleanup_within_budget", True)
    passed = (protocol and complete and not errors and not sequence_errors and proof["status"] == "COVERAGE_PASS"
              and proof["all_queries_succeeded"] and all(w["status"] == "CURRENT_FUNCTION_PASS" for w in transport))
    association_proof = association.assess(run, binding, windows[0]) if protocol else {
        "status": "NOT_PROVEN", "reasons": ["single seven-client window contract missing"],
        "participants": {name: {"status": "NOT_PROVEN"} for name in association.CLIENTS}}
    association_complete = association_proof["status"] == "PASS"
    return {"schema": SCHEMA, "mode": "seven_clients", "windows": transport, "coverage": proof,
            "capture_packet_errors": errors, "protocol_complete": protocol,
            "capture_packet_sequence_errors": sequence_errors,
            "function_transport_status": "PASS" if passed else "FAIL",
            "association_evidence": association_proof["participants"], "association_status": association_proof["status"],
            "association_proof": association_proof,
            "status": "PASS" if passed and association_complete else "NOT_PROVEN" if passed else "FAIL",
            "stations_count_alone_used_as_proof": False, "positions_proven": False,
            "live_measurement_executed_by_evaluator": False}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("run", "binding", "pcap", "limits", "output"):
        parser.add_argument("--" + name, type=Path, required=True)
    args = parser.parse_args()
    limits_raw = args.limits.read_bytes()
    if hashlib.sha256(limits_raw).hexdigest() != FROZEN_LIMITS_SHA256:
        raise ValueError("original frozen historical limits file binding differs")
    run = json.loads(args.run.read_text())
    bound = json.loads(args.binding.read_text())
    binding = bound.get("evaluation_binding", bound)
    if "config" in bound:
        binding = bound["config"]["evaluation_binding"]
    evaluator = evaluate_seven_clients if run.get("mode") == "seven_clients" else evaluate
    result = evaluator(run, binding, args.pcap.read_bytes(), json.loads(limits_raw))
    result["historical_limits_sha256"] = FROZEN_LIMITS_SHA256
    with args.output.open("x") as stream:
        json.dump(result, stream, indent=2, sort_keys=True)
    print(json.dumps({"schema": result["schema"], "status": result["status"], "windows": len(result["windows"])}))


if __name__ == "__main__":
    main()

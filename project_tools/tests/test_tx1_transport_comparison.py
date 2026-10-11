lazy import copy
lazy import hashlib
lazy import json
lazy import struct
lazy import unittest
lazy from unittest.mock import patch

lazy from project_tools import tx1_transport_comparison as comparison


def binding():
    ap_mac = "02:00:00:00:00:01"
    return {"host_ip": "192.168.4.20", "host_mac": "02:00:00:00:00:20", "rx_target_ip": "192.168.4.50", "ap_mac": ap_mac, "tx_boot_id": "2a",
            "source_mac_sha256": hashlib.sha256(bytes.fromhex(ap_mac.replace(":", ""))).hexdigest(),
            "rx": {f"RX{i}": {"node_id": i, "ip": f"192.168.4.{i + 1}",
                    "mac": f"02:00:00:00:00:0{i + 1}", "boot_id": f"{i:016x}",
                    "image_sha256": "ab" * 32} for i in range(1, 5)},
            "radar": {f"MMWAVE{i}": {"ip": f"192.168.4.{i + 5}",
                      "mac": f"02:00:00:00:00:0{i + 5}", "boot_id": i + 5} for i in (1, 2)}}


def csi(name, carriers, sequence=1):
    bound = binding()
    device = bound["rx"][name]
    header = bytearray(20)
    struct.pack_into("<IBBHII", header, 0, 0xC5110001, device["node_id"], 1,
                     carriers, 2437, sequence)
    data = bytes(header) + bytes(carriers * 2)
    data += struct.pack("<IBBH", 0xC511A060, 1, 7, 40) + bytes.fromhex(bound["source_mac_sha256"])
    data += struct.pack("<IBBHQ", 0xC511A061, 2, 1, 24, int(device["boot_id"], 16)) + bytes(8)
    return {"payload": data, "dst_port": 5005, "src_port": 40000,
            "src_ip": device["ip"], "src_mac": device["mac"].replace(":", ""),
            "dst_ip": bound["rx_target_ip"], "dst_mac": "020000000020", "timestamp_ns": 1_000_000_000}


def limits():
    names = comparison.PRIMARY_METRICS + tuple("rx4_" + key for key in comparison.LIMIT_COUNTERS)
    return {"schema": 1, "rule": "old-three-window-envelope", "warmup_seconds": 60,
            "window_seconds": 120, "window_count": 3,
            "limits": {name: {"direction": "min" if name.endswith("rate") else "max",
                       "value": 0 if name.endswith("rate") else 1_000} for name in names}}


def radar_packet():
    device = binding()["radar"]["MMWAVE1"]
    value = {"schema": "ruview.mmwave.ld2450.v1", "node_id": "MMWAVE1", "mode": "calibration",
             "boot_id": device["boot_id"], "sequence": 123, "sensor_time_us": 123_000, "unix_time_ms": 0,
             "targets": [{"slot": i, "present": False, "x_mm": 0, "y_mm": 0, "speed_cm_s": 0,
                          "resolution_mm": 0} for i in (1, 2, 3)]}
    return {"payload": json.dumps(value).encode(), "dst_port": 55010, "src_port": 40000,
            "src_ip": device["ip"], "src_mac": device["mac"].replace(":", ""),
            "dst_ip": binding()["host_ip"], "dst_mac": "020000000020", "timestamp_ns": 1_000_000_000}


def row(key, slot, value, mono_offset=0):
    time_ns = (100 + slot) * 1_000_000_000 + mono_offset
    stamp = {"wall_ns": time_ns, "mono_before_ns": time_ns, "mono_after_ns": time_ns}
    return {"endpoint_key": key, "endpoint": key, "slot": slot, "value": value, "error": None,
            "request_id": f"{slot}:{key}",
            "http_status": 200, "body_complete": True, "begin": stamp.copy(), "end": stamp.copy()}


def window_fixture():
    bound, rows, packets = binding(), [], []
    for slot in range(121):
        nodes = []
        for name, device in bound["rx"].items():
            node = {"node_id": device["node_id"], "boot_id": device["boot_id"], "protocol_version": 2,
                    "last_seen_ms": 100, "accepted_csi_frames": slot, "sequence_observations": slot,
                    "inferred_lost_frames": slot * 2, "skipped_grid_frames": slot * 2}
            node.update({key: True for key in ("identity_valid", "identity_matches_setup",
                        "source_binding_attested", "source_matched_filter", "filter_enforced")})
            nodes.append(node)
            own = {"node_id": name, "boot_id": device["boot_id"], "image_sha256": device["image_sha256"],
                   "sequence": slot * 3, "transport": {key: 0 for key in comparison.RX_COUNTERS}}
            own["transport"].update(enqueued=slot * 3, sent=slot * 3)
            rows.append(row(name, slot, own))
            if slot < 120:
                for count, carriers in enumerate((64, 128, 192)):
                    packet = csi(name, carriers, slot * 3 + count)
                    packet["timestamp_ns"] = (100 + slot) * 1_000_000_000 + (count + 1) * 100_000_000
                    packets.append(comparison.classify_packet(packet, bound) | {"timestamp_ns": packet["timestamp_ns"]})
        rows.append(row("nodes", slot, {"nodes": nodes}))
        rows.append(row("TX1", slot, {key: slot if key == "sent_packets" else 0 for key in comparison.TX_COUNTERS}, 300_000_000))
        progress = ("udp_datagrams_from_os", "csi_datagrams_received", "csi_frames_accepted_new", "csi_frames_discarded")
        rows.append(row("metrics", slot, {"csi_ingress": {"counters": {
            key: slot if key in progress else 0 for key in comparison.INGRESS_COUNTERS}}}, 500_000_000))
        rows.append(row("ready", slot, {"status": "ready"}))
        rows.append(row("profiles", slot, {"profiles": []}))
        sensors = []
        for name, device in bound["radar"].items():
            diagnostics = {key: slot if key in comparison.RADAR_DIAGNOSTICS[:4] else 0
                           for key in comparison.RADAR_DIAGNOSTICS}
            rows.append(row(name, slot, {"node_id": name, "diagnostics": diagnostics}))
            sensors.append({"node_id": name, "boot_id": device["boot_id"], "packet_age_ms": 100,
                            "sequence": slot, "raw_udp_packets": slot, "packets_received": slot,
                            "packets_rejected": max(0, slot - 80), "packets_lost": 0, "reboot_count": 0,
                            "reject_reasons": {"room_bounds": max(0, slot - 80)}})
            if slot < 120:
                for kind in ("radar", "ack"):
                    packets.append({"kind": kind, "valid": True, "participant": name, "sequence": slot,
                                    "ack_pair_confirmed": kind == "radar",
                                    "timestamp_ns": (100 + slot) * 1_000_000_000 + 200_000_000})
        rows.append(row("mmwave", slot, {"sensors": sensors}))
        if slot < 120:
            packets.append({"kind": "sounding", "valid": True, "reasons": [], "sequence": slot,
                            "boot_ms": slot * 1000, "timestamp_ns": (100 + slot) * 1_000_000_000 + 100_000_000})
    window = {"index": 1, "first_slot": 0, "last_slot": 120,
              "begin_mono_ns": 100_000_000_000, "end_mono_ns": 220_000_000_000}
    packets.sort(key=lambda value: value["timestamp_ns"])
    return {"rows": rows}, bound, packets, window


def comparison_windows():
    # Explicit contract fixture, anchored at monotonic epoch 40 seconds.
    return [
        {"index": 1, "first_slot": 60, "last_slot": 180, "begin_mono_ns": 100_000_000_000, "end_mono_ns": 220_000_000_000},
        {"index": 2, "first_slot": 180, "last_slot": 300, "begin_mono_ns": 220_000_000_000, "end_mono_ns": 340_000_000_000},
        {"index": 3, "first_slot": 300, "last_slot": 420, "begin_mono_ns": 340_000_000_000, "end_mono_ns": 460_000_000_000},
    ]


def scope_fixture(mode):
    """Complete synthetic scope with real clock/PCAP-span coverage validation."""
    base, bound, base_packets, window = window_fixture()
    count, slot_shift, epoch = (421, -60, 40_000_000_000) if mode == "comparison" else (121, 0, 100_000_000_000)
    rows, packets = [], []
    samples = {slot: [sample for sample in base["rows"] if sample["slot"] == slot] for slot in range(120)}
    packet_slots = {slot: [packet for packet in base_packets
                          if (packet["timestamp_ns"] - 100_000_000_000) // 1_000_000_000 == slot] for slot in range(120)}
    for slot in range(count):
        source_slot = slot % 120
        counter_shift = slot - source_slot
        shift_ns = (counter_shift + slot_shift) * 1_000_000_000
        for original in samples[source_slot]:
            sample = copy.deepcopy(original)
            sample.update(slot=slot, request_id=f"{slot}:{sample['endpoint_key']}")
            for stamp in (sample["begin"], sample["end"]):
                for key in stamp:
                    stamp[key] += shift_ns
            key, value = sample["endpoint_key"], sample["value"]
            if key == "nodes":
                for node in value["nodes"]:
                    for field, factor in (("accepted_csi_frames", 1), ("sequence_observations", 1),
                                          ("inferred_lost_frames", 2), ("skipped_grid_frames", 2)):
                        node[field] += counter_shift * factor
            elif key.startswith("RX"):
                value["sequence"] += counter_shift * 3
                for field in ("enqueued", "sent"):
                    value["transport"][field] += counter_shift * 3
            elif key == "TX1":
                value["sent_packets"] += counter_shift
            elif key == "metrics":
                for field in ("udp_datagrams_from_os", "csi_datagrams_received", "csi_frames_accepted_new", "csi_frames_discarded"):
                    value["csi_ingress"]["counters"][field] += counter_shift
            elif key.startswith("MMWAVE"):
                for field in ("uart_bytes_received", "radar_frames_valid", "udp_packets_sent", "udp_packets_acked"):
                    value["diagnostics"][field] += counter_shift
            elif key == "mmwave":
                for sensor in value["sensors"]:
                    for field in ("sequence", "raw_udp_packets", "packets_received"):
                        sensor[field] += counter_shift
                    sensor.update(packets_rejected=max(0, slot - 80), reject_reasons={"room_bounds": max(0, slot - 80)})
            rows.append(sample)
        if slot < count - 1:
            for original in packet_slots[source_slot]:
                packet = copy.deepcopy(original)
                packet["timestamp_ns"] += shift_ns
                packet["sequence"] += counter_shift * (3 if packet["kind"].startswith("csi_") else 1)
                if packet["kind"] == "sounding":
                    packet["boot_ms"] += counter_shift * 1000
                packets.append(packet)
    rows.sort(key=lambda sample: sample["end"]["mono_after_ns"])
    before, after = epoch - 3_000_000_000, epoch + (count + 3) * 1_000_000_000
    raw = bytearray(struct.pack("<IHHIIII", 0xA1B23C4D, 2, 4, 0, 0, 65535, 1))
    for timestamp in (before, after):
        raw.extend(struct.pack("<IIII", timestamp // 1_000_000_000, timestamp % 1_000_000_000, 1, 1))
        raw.extend(b"x")
    def stamp(ns):
        return {"wall_ns": ns, "mono_before_ns": ns, "mono_after_ns": ns}
    proof = {"schema": comparison.coverage.CONCURRENT_SCHEMA, "pcap_sha256": hashlib.sha256(raw).hexdigest(),
             "clean_exit": True, "exit_code": 0, "kernel_drops": 0, "ready_mono_ns": before,
             "stop_requested_mono_ns": after, "timestamp_type": "host", "timestamp_type_confirmed": True,
             "clock_info": {"time": {"resolution_ns": 1}, "monotonic": {"resolution_ns": 1, "monotonic": True}},
             "clock_pairs": [stamp(before - 1_000_000_000), stamp(after + 1_000_000_000)],
             "clock_events": [], "expected_request_ids": [sample["request_id"] for sample in rows]}
    for sample in rows:
        for edge in ("begin", "end"):
            proof["clock_events"].append({"ordinal": len(proof["clock_events"]), "request_id": sample["request_id"],
                                          "boundary": edge, "stamp": sample[edge]})
    return {"rows": rows, "mode": mode, "windows": comparison_windows() if mode == "comparison" else [window],
            "epoch_mono_ns": epoch, "proof": proof}, bound, packets, bytes(raw)


class ClassificationTests(unittest.TestCase):
    def test_192_bandwidth_sync_bits_and_distinct_rx_radar_destinations(self):
        for name in binding()["rx"]:
            packet = csi(name, 192)
            payload = bytearray(packet["payload"])
            payload[19] = 0x11
            result = comparison.classify_packet(packet | {"payload": bytes(payload)}, binding())
            self.assertTrue(result["valid"])
            self.assertEqual(result["kind"], "csi_192")
            self.assertEqual(result["bandwidth_mhz"], 40)
            self.assertFalse(result["fixed_grid_eligible"])
            self.assertFalse(comparison.classify_packet(packet | {"dst_ip": binding()["host_ip"]}, binding())["valid"])
        packet = csi("RX4", 128)
        payload = bytearray(packet["payload"])
        payload[19] = 0x11
        result = comparison.classify_packet(packet | {"payload": bytes(payload)}, binding())
        self.assertTrue(result["valid"])
        self.assertFalse(result["fixed_grid_eligible"])
        self.assertEqual(result["exclusion_reason"], "128_ppdu_0_stable_flags_1")
        payload[19] = 0x80
        self.assertFalse(comparison.classify_packet(packet | {"payload": bytes(payload)}, binding())["valid"])

    def test_all_rx_192_are_valid_separately_excluded_without_sample_mutation(self):
        for name in binding()["rx"]:
            for carriers in (64, 128, 192):
                packet = csi(name, carriers)
                original = packet["payload"]
                result = comparison.classify_packet(packet, binding())
                self.assertTrue(result["valid"], (name, carriers))
                self.assertEqual(result["kind"], f"csi_{carriers}")
                self.assertEqual(result["fixed_grid_eligible"], carriers == 128)
                self.assertEqual(result["exclusion_reason"], None if carriers == 128 else f"subcarriers_{carriers}")
                self.assertIsNone(result["server_accepted"])
                self.assertEqual(packet["payload"], original)

    def test_unknown_types_and_unknown_grid_are_separate_from_damaged_packets(self):
        packet = csi("RX4", 192)
        self.assertEqual(comparison.classify_packet(packet | {"payload": b"\x88" * 4}, binding())["kind"], "unknown_rx_type")
        self.assertEqual(comparison.classify_packet(csi("RX4", 96), binding())["kind"], "unknown_csi_grid")
        self.assertEqual(comparison.classify_packet(packet | {"payload": packet["payload"][:-1]}, binding())["kind"], "damaged_csi")

    def test_every_grid_requires_actual_source_node_version_boot_and_full_trailers(self):
        for carriers in (64, 128, 192):
            original = csi("RX2", carriers)
            tail = 20 + 2 * carriers
            for offset in (4, tail + 4, tail + 8, tail + 40 + 4, tail + 40 + 8, tail + 63):
                payload = bytearray(original["payload"])
                payload[offset] ^= 1
                self.assertFalse(comparison.classify_packet(original | {"payload": bytes(payload)}, binding())["valid"], (carriers, offset))
            for change in ({"src_ip": "192.168.4.30"}, {"src_mac": "020000000030"}, {"dst_ip": "192.168.4.31"}):
                self.assertFalse(comparison.classify_packet(original | change, binding())["valid"])

    def test_radar_valid_no_target_still_requires_own_boot_and_source(self):
        packet = radar_packet()
        self.assertTrue(comparison.classify_packet(packet, binding())["valid"])
        self.assertFalse(comparison.classify_packet(packet | {"src_mac": "020000000030"}, binding())["valid"])

    def test_radar_missing_unknown_or_invalid_typed_fields_fail_strict_structure(self):
        packet = radar_packet()
        value = json.loads(packet["payload"])
        for key in ("mode", "sensor_time_us", "unix_time_ms", "targets"):
            altered = copy.deepcopy(value)
            del altered[key]
            self.assertFalse(comparison.classify_packet(packet | {"payload": json.dumps(altered).encode()}, binding())["valid"])
        for change in ({"targets": []}, {"sensor_time_us": True}, {"mode": "other"}, {"unknown": 1}):
            self.assertFalse(comparison.classify_packet(packet | {"payload": json.dumps(value | change).encode()}, binding())["valid"])
        altered = copy.deepcopy(value)
        altered["targets"][0]["x_mm"] = 1
        self.assertFalse(comparison.radar_shape(altered))
        self.assertFalse(comparison.classify_packet(packet | {"payload": json.dumps(value | {"node_id": []}).encode()}, binding())["valid"])
        duplicate = packet["payload"][:-1] + b',"sequence":123}'
        self.assertFalse(comparison.classify_packet(packet | {"payload": duplicate}, binding())["valid"])

    def test_individual_ack_join_requires_reverse_ports_own_boot_and_sequence(self):
        data = radar_packet()
        device = binding()["radar"]["MMWAVE1"]
        ack = {"payload": struct.pack("!III", 0x5256414B, device["boot_id"], 123),
               "src_ip": data["dst_ip"], "dst_ip": data["src_ip"], "src_port": 55010,
               "dst_port": 40000, "src_mac": data["dst_mac"], "dst_mac": data["src_mac"],
               "timestamp_ns": data["timestamp_ns"] + 1_000_000}
        with patch.object(comparison.wire, "udp_packets", return_value=[data, ack]):
            result = comparison.classify_capture(b"fixture", binding())
        self.assertTrue(result[0]["ack_pair_confirmed"])
        for change in ({"dst_port": 40001}, {"src_mac": "020000000031"},
                       {"payload": struct.pack("!III", 0x5256414B, device["boot_id"], 124)}):
            with patch.object(comparison.wire, "udp_packets", return_value=[data, ack | change]):
                result = comparison.classify_capture(b"fixture", binding())
            self.assertFalse(result[0]["ack_pair_confirmed"])
        with patch.object(comparison.wire, "udp_packets", return_value=[data, ack | {"timestamp_ns": data["timestamp_ns"] - 1}]):
            with self.assertRaisesRegex(ValueError, "timestamp order"):
                comparison.classify_capture(b"fixture", binding())

    def test_single_ack_after_retransmission_does_not_confirm_two_attempts(self):
        data = radar_packet()
        retry = data | {"timestamp_ns": data["timestamp_ns"] + 2_000_000}
        device = binding()["radar"]["MMWAVE1"]
        ack = {"payload": struct.pack("!III", 0x5256414B, device["boot_id"], 123),
               "src_ip": data["dst_ip"], "dst_ip": data["src_ip"], "src_port": 55010,
               "dst_port": 40000, "src_mac": data["dst_mac"], "dst_mac": data["src_mac"],
               "timestamp_ns": data["timestamp_ns"] + 3_000_000}
        with patch.object(comparison.wire, "udp_packets", return_value=[data, retry, ack]):
            result = comparison.classify_capture(b"fixture", binding())
        self.assertEqual([p["ack_pair_confirmed"] for p in result if p["kind"] == "radar"], [False, True])

    def test_capture_sequence_reset_at_a_window_edge_or_warmup_is_retained(self):
        first, last = csi("RX1", 64, 10), csi("RX1", 192, 9)
        last["timestamp_ns"] += 1_000_000
        with patch.object(comparison.wire, "udp_packets", return_value=[first, last]):
            result = comparison.classify_capture(b"fixture", binding())
        self.assertFalse(result[0]["sequence_error"])
        self.assertTrue(result[1]["sequence_error"])
        self.assertEqual(result[1]["kind"], "csi_192")


class MetricsTests(unittest.TestCase):
    def test_complete_fixed_inventories_pass_real_coverage_for_both_modes(self):
        for mode, expected in (("comparison", 5052), ("seven_clients", 1452)):
            run, bound, packets, raw = scope_fixture(mode)
            evaluate = comparison.evaluate if mode == "comparison" else comparison.evaluate_seven_clients
            with patch.object(comparison, "classify_capture", return_value=packets):
                result = evaluate(run, bound, raw, limits())
            self.assertEqual(result["coverage"]["mandatory_api_intervals"], expected)
            self.assertEqual(result["coverage"]["status"], "COVERAGE_PASS", result["coverage"]["reasons"])
            if mode == "comparison":
                self.assertEqual(result["status"], "CURRENT_FUNCTION_PASS")
                self.assertEqual(result["historical_reference_status"], "REFERENCE_MET")
            else:
                self.assertEqual(result["function_transport_status"], "PASS")
                self.assertEqual(result["status"], "NOT_PROVEN")

    def test_self_declared_inventory_cannot_hide_warmup_or_ready_profiles(self):
        run, bound, packets, raw = scope_fixture("comparison")
        run["rows"] = [sample for sample in run["rows"] if sample["slot"] >= 60
                       and sample["endpoint_key"] not in ("ready", "profiles")]
        run["proof"]["expected_request_ids"] = [sample["request_id"] for sample in run["rows"]]
        remaining = set(run["proof"]["expected_request_ids"])
        run["proof"]["clock_events"] = [event for event in run["proof"]["clock_events"]
                                            if event["request_id"] in remaining]
        for ordinal, event in enumerate(run["proof"]["clock_events"]):
            event["ordinal"] = ordinal
        self_declared = comparison.coverage.assess(
            run["rows"], comparison.coverage.pcap_span(raw), run["proof"], run["proof"]["expected_request_ids"])
        self.assertEqual(self_declared["status"], "COVERAGE_PASS", self_declared["reasons"])
        with patch.object(comparison, "classify_capture", return_value=packets):
            result = comparison.evaluate(run, bound, raw, limits())
        self.assertTrue(result["protocol_complete"])
        self.assertEqual(len(run["rows"]), 3610)
        self.assertEqual(len(result["coverage"]["missing_request_ids"]), 1442)
        self.assertEqual(result["coverage"]["status"], "COVERAGE_FAIL")
        self.assertEqual(result["status"], "CURRENT_FUNCTION_FAIL")
        self.assertEqual(result["historical_reference_status"], "NOT_COMPARABLE")
        self.assertEqual(result["windows"][0]["metrics"]["rx4_rate"], 1)
        self.assertEqual(result["windows"][0]["historical_reference"]["targets"]["rx4_rate"]["descriptive_value"], 1)

    def test_inventory_rejects_missing_duplicate_relabelled_or_reduced_proof(self):
        base, bound, packets, raw = scope_fixture("seven_clients")
        cases = []
        missing = copy.deepcopy(base)
        missing["rows"] = missing["rows"][1:]
        cases.append(missing)
        duplicated = copy.deepcopy(base)
        duplicated["rows"].append(copy.deepcopy(duplicated["rows"][0]))
        cases.append(duplicated)
        relabelled = copy.deepcopy(base)
        relabelled["rows"][0]["endpoint_key"] = "ready"
        cases.append(relabelled)
        for field in ("expected_request_ids", "planned_slot_ids"):
            reduced = copy.deepcopy(base)
            reduced["proof"][field] = reduced["proof"]["expected_request_ids"][1:]
            cases.append(reduced)
        duplicate_proof = copy.deepcopy(base)
        duplicate_proof["proof"]["expected_request_ids"][-1] = duplicate_proof["proof"]["expected_request_ids"][0]
        cases.append(duplicate_proof)
        overridden = copy.deepcopy(base)
        overridden["expected_request_ids"] = []
        cases.append(overridden)
        for malformed in cases:
            with patch.object(comparison, "classify_capture", return_value=packets):
                result = comparison.evaluate_seven_clients(malformed, bound, raw, limits())
            self.assertEqual(result["coverage"]["status"], "COVERAGE_FAIL")
            self.assertEqual(result["function_transport_status"], "FAIL")

    def test_identical_metric_bodies_and_denominators_keep_exclusions_in_inferred_gaps(self):
        run, bound, packets, window = window_fixture()
        result = comparison.evaluate_window(run, bound, packets, window, limits())
        self.assertEqual(result["status"], "CURRENT_FUNCTION_PASS", result["reasons"])
        self.assertEqual(result["sequence_denominator"], 360)
        self.assertEqual(result["metrics"]["sequence_gap_ratio"], 240 / 360)
        self.assertEqual(result["metrics"]["rx4_rate"], 1)
        self.assertEqual(result["participants"]["RX4"]["transport_sequence"]["denominator"], 359)
        tx_times = [p["timestamp_ns"] / 1e9 for p in packets if p["kind"] == "sounding"]
        original = comparison.legacy_helpers()["gaps"](tx_times, 100, 220)
        self.assertEqual(result["metrics"]["sounding_rate"], original["rate"])
        self.assertEqual(result["metrics"]["sounding_interval_p95"], original["interval_p95"])
        self.assertEqual(result["metrics"]["sounding_max_gap"], original["max_gap"])

    def test_exclusions_are_packet_proven_and_never_reclassified_as_radio_loss(self):
        run, bound, packets, window = window_fixture()
        result = comparison.evaluate_window(run, bound, packets, window, limits())
        grid = result["participants"]["RX4"]["grid_accounting"]
        self.assertEqual(grid["minimum"], 240)
        self.assertEqual(grid["maximum"], 240)
        self.assertEqual(grid["exclusion_packet_brackets"]["subcarriers_64"], {"minimum": 120, "maximum": 120})
        self.assertEqual(grid["exclusion_packet_brackets"]["subcarriers_192"], {"minimum": 120, "maximum": 120})
        self.assertFalse(grid["radio_loss"])
        packets = [p for p in packets if p.get("participant") != "RX4" or p["kind"] != "csi_192"]
        result = comparison.evaluate_window(run, bound, packets, window, limits())
        self.assertEqual(result["status"], "CURRENT_FUNCTION_FAIL")
        self.assertEqual(result["participants"]["RX4"]["grid_accounting"]["status"], "FAIL")

    def test_invalid_packets_do_not_disappear_into_grid_exclusion_counts(self):
        run, bound, packets, window = window_fixture()
        packets.append({"kind": "unknown_rx_type", "valid": False, "participant": "RX4", "timestamp_ns": 150_000_000_000})
        result = comparison.evaluate_window(run, bound, packets, window, limits())
        self.assertEqual(result["packet_errors"], {"unknown_rx_type": 1})
        self.assertEqual(result["participants"]["RX4"]["grid_accounting"]["maximum"], 240)
        self.assertEqual(result["status"], "CURRENT_FUNCTION_FAIL")

    def test_own_counter_boundaries_remain_separate_from_nodes_duration(self):
        run, bound, packets, window = window_fixture()
        next(row for row in run["rows"] if row["endpoint_key"] == "TX1" and row["slot"] == 120)["end"]["mono_after_ns"] += 200_000_000
        result = comparison.evaluate_window(run, bound, packets, window, limits())
        self.assertEqual(result["duration"], 120)
        self.assertEqual(result["TX1_counter_duration_seconds"], 120.2)
        self.assertEqual(result["metrics"]["rx4_rate"], 1)

    def test_every_rx_and_tx_transport_error_is_a_current_function_failure(self):
        for key, counter in (("RX1", "errors"), ("RX2", "queue_full_drops"), ("TX1", "send_errors")):
            run, bound, packets, window = window_fixture()
            last = next(row for row in run["rows"] if row["endpoint_key"] == key and row["slot"] == 120)
            if key == "TX1":
                last["value"][counter] += 1
            else:
                last["value"]["transport"][counter] += 1
            result = comparison.evaluate_window(run, bound, packets, window, limits())
            self.assertEqual(result["status"], "CURRENT_FUNCTION_FAIL", (key, result["reasons"]))

    def test_each_global_transport_error_fails_but_raster_discards_are_allowed(self):
        error_names = ("udp_queue_overflow_datagrams", "udp_queue_stale_datagrams", "udp_receive_errors",
                       "csi_queue_overflow_datagrams", "csi_queue_stale_datagrams")
        for error_name in error_names:
            run, bound, packets, window = window_fixture()
            for sample in run["rows"]:
                if sample["endpoint_key"] == "metrics":
                    sample["value"]["csi_ingress"]["counters"][error_name] = sample["slot"]
            result = comparison.evaluate_window(run, bound, packets, window, limits())
            self.assertEqual(result["server_ingress_global_deltas"][error_name], 120)
            self.assertEqual(result["status"], "CURRENT_FUNCTION_FAIL")
            self.assertIn("new global ingress transport errors", result["reasons"])
        run, bound, packets, window = window_fixture()
        result = comparison.evaluate_window(run, bound, packets, window, limits())
        self.assertEqual(result["server_ingress_global_deltas"]["csi_frames_discarded"], 120)
        self.assertEqual(result["status"], "CURRENT_FUNCTION_PASS", result["reasons"])

    def test_rx1_missing_evidence_preserves_independent_rx4_metrics_and_other_paths(self):
        run, bound, packets, window = window_fixture()
        run["rows"] = [row for row in run["rows"] if row["endpoint_key"] != "RX1" or row["slot"] != 60]
        result = comparison.evaluate_window(run, bound, packets, window, limits())
        self.assertEqual(result["status"], "CURRENT_FUNCTION_FAIL")
        self.assertEqual(result["metrics"]["rx4_rate"], 1)
        self.assertEqual(result["metrics"]["sequence_gap_ratio"], 240 / 360)
        self.assertEqual(result["radars"]["MMWAVE1"]["status"], "PASS")

    def test_missing_rx4_own_response_retains_server_rate_and_independent_tx_failure(self):
        run, bound, packets, window = window_fixture()
        run["rows"] = [row for row in run["rows"] if row["endpoint_key"] != "RX4" or row["slot"] != 60]
        last_tx = next(row for row in run["rows"] if row["endpoint_key"] == "TX1" and row["slot"] == 120)
        last_tx["value"]["send_errors"] = 8
        result = comparison.evaluate_window(run, bound, packets, window, limits())
        self.assertEqual(result["status"], "CURRENT_FUNCTION_FAIL")
        self.assertEqual(result["metrics"]["rx4_rate"], 1)
        self.assertEqual(result["metrics"]["sequence_gap_ratio"], 240 / 360)
        self.assertNotIn("rx4_errors", result["metrics"])
        self.assertEqual(result["participants"]["RX4"]["server_deltas"]["accepted_csi_frames"], 120)
        self.assertEqual(result["participants"]["RX4"]["own_evidence_status"], "FAIL")
        self.assertEqual(result["tx_deltas"]["send_errors"], 8)
        self.assertIn("server_ingress_global_deltas", result)
        self.assertIn("TX own sounding progress/error contract", result["reasons"])

    def test_missing_nodes_or_tx_responses_cannot_hide_complete_global_ingress(self):
        for missing in ("nodes", "TX1"):
            run, bound, packets, window = window_fixture()
            run["rows"] = [row for row in run["rows"] if row["endpoint_key"] != missing or row["slot"] != 60]
            result = comparison.evaluate_window(run, bound, packets, window, limits())
            self.assertEqual(result["status"], "CURRENT_FUNCTION_FAIL")
            self.assertEqual(result["server_ingress_global_deltas"]["csi_frames_accepted_new"], 120)
            if missing == "nodes":
                self.assertIn("tx_deltas", result)
                self.assertNotIn("rx4_rate", result["metrics"])
            else:
                self.assertEqual(result["metrics"]["rx4_rate"], 1)
                self.assertNotIn("tx_deltas", result)

    def test_radar_1000ms_binding_and_content_rejections_remain_separate(self):
        run, bound, packets, window = window_fixture()
        result = comparison.evaluate_window(run, bound, packets, window, limits())
        self.assertEqual(result["radars"]["MMWAVE2"]["status"], "PASS")
        self.assertEqual(result["radars"]["MMWAVE2"]["content_rejection_deltas"], {"room_bounds": 40})
        self.assertFalse(result["radars"]["MMWAVE2"]["content_rejections_are_transport_loss"])
        server = next(row for row in run["rows"] if row["endpoint_key"] == "mmwave" and row["slot"] == 60)
        server["value"]["sensors"][1]["packet_age_ms"] = 1001
        result = comparison.evaluate_window(run, bound, packets, window, limits())
        self.assertEqual(result["radars"]["MMWAVE2"]["status"], "FAIL")
        self.assertEqual(result["status"], "CURRENT_FUNCTION_FAIL")
        self.assertEqual(result["metrics"]["rx4_rate"], 1)

    def test_seven_clients_never_infers_identified_associations_from_transport(self):
        run, bound, packets, window = window_fixture()
        run.update(windows=[window], proof={"expected_request_ids": [sample["request_id"] for sample in run["rows"]]}, mode="seven_clients")
        outcome = comparison.evaluate_window(run, bound, packets, window, limits())
        with patch.object(comparison.wire, "udp_packets", return_value=[]), \
             patch.object(comparison.coverage, "pcap_span", return_value={}), \
             patch.object(comparison.coverage, "assess", return_value={"status": "COVERAGE_PASS", "all_queries_succeeded": True}), \
             patch.object(comparison, "evaluate_window", return_value=outcome):
            result = comparison.evaluate_seven_clients(run, bound, b"fixture", limits())
        self.assertEqual(result["function_transport_status"], "PASS")
        self.assertEqual(result["association_status"], "NOT_PROVEN")
        self.assertEqual(result["status"], "NOT_PROVEN")
        self.assertEqual(set(result["association_evidence"]), {"Mac", "RX1", "RX2", "RX3", "RX4", "MMWAVE1", "MMWAVE2"})
        self.assertFalse(result["stations_count_alone_used_as_proof"])

    def test_binding_pass_flags_cannot_replace_current_association_responses(self):
        run, bound, packets, window = window_fixture()
        run.update(windows=[window], proof={"expected_request_ids": [sample["request_id"] for sample in run["rows"]]}, mode="seven_clients")
        bound["association_evidence"] = {name: {"status": "PASS"} for name in comparison.association.CLIENTS}
        with patch.object(comparison, "classify_capture", return_value=packets), \
             patch.object(comparison.coverage, "pcap_span", return_value={}), \
             patch.object(comparison.coverage, "assess", return_value={"status": "COVERAGE_PASS", "all_queries_succeeded": True}):
            result = comparison.evaluate_seven_clients(run, bound, b"fixture", limits())
        self.assertEqual(result["function_transport_status"], "PASS")
        self.assertEqual(result["association_status"], "NOT_PROVEN")
        self.assertEqual(result["status"], "NOT_PROVEN")
        self.assertTrue(result["association_proof"]["reasons"])

    def test_reference_failures_and_missing_values_never_claim_original_mixed_pass(self):
        reference = limits()
        reference["limits"]["rx4_rate"]["value"] = 2
        result = comparison.historical_reference({"rx4_rate": 1}, reference)
        self.assertEqual(result["status"], "REFERENCE_FAILED")
        self.assertEqual(result["targets"]["rx4_rate"]["status"], "FAIL")
        self.assertEqual(result["targets"]["sequence_gap_ratio"]["status"], "NOT_COMPARABLE")
        self.assertFalse(result["original_mixed_v2_pass"])

    def test_empty_or_incomplete_comparison_never_claims_all_reference_targets_met(self):
        empty = {"rows": [], "windows": [], "warmup_seconds": 60,
                 "proof": {"expected_request_ids": []}}
        with patch.object(comparison, "classify_capture", return_value=[]), \
             patch.object(comparison.coverage, "pcap_span", return_value={}), \
             patch.object(comparison.coverage, "assess", return_value={"status": "COVERAGE_FAIL"}):
            result = comparison.evaluate(empty, binding(), b"fixture", limits())
        self.assertEqual(result["status"], "CURRENT_FUNCTION_FAIL")
        self.assertEqual(result["historical_reference_status"], "NOT_COMPARABLE")

    def test_coverage_failure_keeps_numeric_reference_observations_not_comparable(self):
        run, bound, packets, window = window_fixture()
        run.update(windows=comparison_windows(), epoch_mono_ns=40_000_000_000,
                   warmup_seconds=60, proof={"expected_request_ids": []})
        outcome = {"status": "CURRENT_FUNCTION_PASS", "metrics": {name: 1 for name in limits()["limits"]},
                   "historical_reference": {"status": "REFERENCE_MET"}}
        for proof in ({"status": "COVERAGE_FAIL", "all_queries_succeeded": True},
                      {"status": "COVERAGE_PASS", "all_queries_succeeded": False}):
            with patch.object(comparison, "classify_capture", return_value=[]), \
                 patch.object(comparison, "evaluate_window", return_value=outcome), \
                 patch.object(comparison.coverage, "pcap_span", return_value={}), \
             patch.object(comparison, "assess_scope_coverage", return_value=proof):
                result = comparison.evaluate(run, bound, b"fixture", limits())
            self.assertTrue(result["protocol_complete"])
            self.assertEqual(result["status"], "CURRENT_FUNCTION_FAIL")
            self.assertEqual(result["historical_reference_status"], "NOT_COMPARABLE")
            for observed in result["windows"]:
                reference = observed["historical_reference"]
                self.assertEqual(reference["status"], "NOT_COMPARABLE")
                self.assertEqual(len(reference["targets"]), 13)
                self.assertTrue(reference["reason"])
                for target in reference["targets"].values():
                    self.assertEqual(target["status"], "NOT_COMPARABLE")
                    self.assertIsNone(target["value"])
                    self.assertEqual(target["descriptive_value"], 1)

    def test_single_window_numeric_targets_are_explicitly_descriptive(self):
        run, bound, packets, window = window_fixture()
        result = comparison.evaluate_window(run, bound, packets, window, limits())
        self.assertEqual(result["status"], "CURRENT_FUNCTION_PASS")
        reference = result["historical_reference"]
        self.assertEqual(reference["status"], "NOT_COMPARABLE")
        self.assertEqual(reference["descriptive_numeric_status"], "REFERENCE_MET")
        self.assertEqual(reference["targets"]["rx4_rate"]["descriptive_value"], 1)

    def test_declared_warmup_cannot_replace_exact_slot_sequence_and_epoch_binding(self):
        base_run = {"rows": [], "windows": comparison_windows(), "epoch_mono_ns": 40_000_000_000,
                    "warmup_seconds": 60, "proof": {"expected_request_ids": []}}
        invalid_runs = []
        no_warmup = copy.deepcopy(base_run)
        for observed in no_warmup["windows"]:
            observed["first_slot"] -= 60
            observed["last_slot"] -= 60
        invalid_runs.append(no_warmup)
        shifted_slot = copy.deepcopy(base_run)
        shifted_slot["windows"][1]["last_slot"] += 1
        invalid_runs.append(shifted_slot)
        shifted_boundary = copy.deepcopy(base_run)
        for observed in shifted_boundary["windows"]:
            observed["begin_mono_ns"] += 1_000_000_000
            observed["end_mono_ns"] += 1_000_000_000
        invalid_runs.append(shifted_boundary)
        missing_epoch = copy.deepcopy(base_run)
        del missing_epoch["epoch_mono_ns"]
        invalid_runs.append(missing_epoch)
        for invalid_epoch in (None, True, -1, 41_000_000_000):
            invalid_runs.append(copy.deepcopy(base_run) | {"epoch_mono_ns": invalid_epoch})
        outcome = {"status": "CURRENT_FUNCTION_PASS", "metrics": {name: 1 for name in limits()["limits"]}}
        with patch.object(comparison, "classify_capture", return_value=[]), \
             patch.object(comparison, "evaluate_window", side_effect=lambda *args: copy.deepcopy(outcome)), \
             patch.object(comparison.coverage, "pcap_span", return_value={}), \
             patch.object(comparison, "assess_scope_coverage", return_value={"status": "COVERAGE_PASS", "all_queries_succeeded": True}):
            valid = comparison.evaluate(base_run, binding(), b"fixture", limits())
            self.assertTrue(valid["protocol_complete"])
            for invalid in invalid_runs:
                result = comparison.evaluate(invalid, binding(), b"fixture", limits())
                self.assertFalse(result["protocol_complete"])
                self.assertEqual(result["status"], "CURRENT_FUNCTION_FAIL")
                self.assertEqual(result["historical_reference_status"], "NOT_COMPARABLE")
                for observed in result["windows"]:
                    self.assertTrue(all(target["status"] == "NOT_COMPARABLE"
                                        for target in observed["historical_reference"]["targets"].values()))

    def test_complete_comparison_can_fail_numeric_targets_despite_transport_failures(self):
        run, bound, packets, window = window_fixture()
        run.update(windows=comparison_windows(), epoch_mono_ns=40_000_000_000,
                   warmup_seconds=60, proof={"expected_request_ids": []})
        observed = {name: 0 for name in limits()["limits"]}
        observed.update(sounding_rate=1, rx4_rate=1)
        reference = limits()
        reference["limits"]["rx4_rate"]["value"] = 2
        outcome = {"status": "CURRENT_FUNCTION_FAIL", "metrics": observed,
                   "reasons": ["TX own sounding progress/error contract", "MMWAVE1 radar transport"]}
        with patch.object(comparison, "classify_capture", return_value=[]), \
             patch.object(comparison, "evaluate_window", return_value=outcome), \
             patch.object(comparison.coverage, "pcap_span", return_value={}), \
             patch.object(comparison, "assess_scope_coverage", return_value={"status": "COVERAGE_PASS", "all_queries_succeeded": True}):
            result = comparison.evaluate(run, bound, b"fixture", reference)
        self.assertEqual(result["status"], "CURRENT_FUNCTION_FAIL")
        self.assertEqual(result["historical_reference_status"], "REFERENCE_FAILED")
        self.assertEqual(result["windows"][0]["historical_reference"]["targets"]["rx4_rate"]["status"], "FAIL")

    def test_original_helpers_do_not_execute_live_functions(self):
        original = comparison.LEGACY_PATH.read_bytes()
        with patch("subprocess.run", side_effect=AssertionError("live operation")):
            comparison.legacy_helpers.cache_clear()
            helpers = comparison.legacy_helpers()
        self.assertNotIn("live_collect", helpers)
        self.assertEqual(comparison.LEGACY_PATH.read_bytes(), original)
        altered = copy.deepcopy(limits())
        altered["limits"]["sequence_gap_ratio"]["direction"] = "min"
        with self.assertRaises(ValueError):
            comparison.validate_limits(altered)


if __name__ == "__main__":
    unittest.main()

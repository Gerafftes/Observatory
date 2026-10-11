lazy import copy
lazy import hashlib
lazy import json
lazy import unittest

lazy from project_tools import tx1_association as association


def fixture():
    ap = "020000000001"
    identity = {"node_id": "TX1", "boot_id": "0000000000000042",
                "source_sha256": "ab" * 32, "image_sha256": "cd" * 32,
                "source_mac_sha256": hashlib.sha256(bytes.fromhex(ap)).hexdigest()}
    binding = {"tx_identity": identity, "tx_boot_id": identity["boot_id"],
               "ap_mac": ap, "source_mac_sha256": identity["source_mac_sha256"],
               "host_mac": "02:00:00:00:00:20",
               "rx": {f"RX{i}": {"mac": f"02000000000{i + 1}"} for i in range(1, 5)},
               "radar": {f"MMWAVE{i}": {"mac": f"02000000000{i + 5}"} for i in (1, 2)}}
    macs = [binding["host_mac"]] + [x["mac"] for x in binding["rx"].values()] + [x["mac"] for x in binding["radar"].values()]
    run = {"interface": "en0", "rows": [], "host_wifi": {
        "interface": "en0", "begin_mono_ns": 99_000_000_000, "end_mono_ns": 99_100_000_000,
        "hardware_ports_stdout": "Hardware Port: Wi-Fi\nDevice: en0\nEthernet Address: 02:00:00:00:00:99\n",
        "ifconfig_stdout": "en0: flags=8863<UP,BROADCAST,SMART,RUNNING,SIMPLEX,MULTICAST> mtu 1500\n\tether 02:00:00:00:00:20\n"}}
    for slot in range(121):
        start = (100 + slot) * 1_000_000_000
        status = dict(identity, ap_association={
            "schema": 1, "status": "current", "station_list_rc": 0, "ap_mac_rc": 0,
            "capture_begin_us": 10_000_000 + slot * 1_000_000,
            "capture_end_us": 10_000_100 + slot * 1_000_000,
            "ap_mac": ap, "station_count": 7, "stations": [{"mac": value} for value in macs],
            "generation_before": 14, "generation_after": 14})
        run["rows"].append({"endpoint_key": "TX1", "slot": slot, "error": None,
                            "http_status": 200, "body_complete": True,
                            "begin": {"mono_before_ns": start}, "end": {"mono_after_ns": start + 2_000_000},
                            "value": status})
    window = {"first_slot": 0, "last_slot": 120, "begin_mono_ns": 100_000_000_000,
              "end_mono_ns": 220_000_000_000}
    return run, binding, window


class AssociationTests(unittest.TestCase):
    def test_current_full_series_uses_actual_wifi_mac(self):
        run, binding, window = fixture()
        result = association.assess(run, binding, window)
        self.assertEqual(result["status"], "PASS")
        self.assertEqual(result["samples_checked"], 121)
        self.assertEqual(set(result["participants"]), set(association.CLIENTS))
        # Native hardware-port inventory MAC intentionally differs from actual
        # ifconfig MAC. The actual Wi-Fi address is authoritative for this run.
        self.assertNotIn(binding["host_mac"], json.dumps(result))

    def test_predeclared_pass_without_station_answers_never_passes(self):
        run, binding, window = fixture()
        binding["association_evidence"] = {name: {"status": "PASS"} for name in association.CLIENTS}
        for row in run["rows"]:
            del row["value"]["ap_association"]
        self.assertEqual(association.assess(run, binding, window)["status"], "NOT_PROVEN")

    def test_missing_duplicate_and_failed_http_samples(self):
        for case in ("missing", "duplicate", "timeout", "partial"):
            with self.subTest(case=case):
                run, binding, window = fixture()
                if case == "missing": run["rows"].pop(60)
                if case == "duplicate": run["rows"].append(copy.deepcopy(run["rows"][60]))
                if case == "timeout": run["rows"][60]["error"] = "timeout"
                if case == "partial": run["rows"][60]["body_complete"] = False
                self.assertEqual(association.assess(run, binding, window)["status"], "NOT_PROVEN")

    def test_empty_failed_changed_and_replayed_lists(self):
        cases = {
            "empty": {"stations": [], "station_count": 0},
            "getter failed": {"station_list_rc": 1},
            "AP getter failed": {"ap_mac_rc": 1},
            "unauthorized": {"status": "authorization_required", "stations": None},
            "unavailable": {"status": "unavailable"},
            "changed": {"status": "changed"},
            "changed inside": {"generation_after": 15},
            "reconnected": {"generation_before": 16, "generation_after": 16},
            "replayed": {"capture_begin_us": 10_000_000, "capture_end_us": 10_000_100},
            "reversed time": {"capture_end_us": 1},
            "impossible duration": {"capture_end_us": 999_999_999},
            "wrong BSSID": {"ap_mac": "020000000099"},
            "bool rc": {"station_list_rc": False},
            "bool generation": {"generation_before": True, "generation_after": True},
            "count mismatch": {"station_count": 8},
        }
        for name, changes in cases.items():
            with self.subTest(case=name):
                run, binding, window = fixture()
                run["rows"][60]["value"]["ap_association"].update(changes)
                self.assertEqual(association.assess(run, binding, window)["status"], "NOT_PROVEN")

    def test_missing_and_duplicate_station_identity(self):
        for case in ("missing", "duplicate", "multicast"):
            with self.subTest(case=case):
                run, binding, window = fixture()
                current = run["rows"][60]["value"]["ap_association"]
                if case == "missing": current["stations"][4]["mac"] = "020000000099"
                if case == "duplicate": current["stations"][4] = current["stations"][3]
                if case == "multicast": current["stations"][4]["mac"] = "010000000005"
                self.assertEqual(association.assess(run, binding, window)["status"], "NOT_PROVEN")

    def test_identity_changes_and_missing_binding(self):
        for key in association.IDENTITY_FIELDS:
            with self.subTest(key=key):
                run, binding, window = fixture()
                run["rows"][60]["value"][key] = "changed"
                self.assertEqual(association.assess(run, binding, window)["status"], "NOT_PROVEN")
        run, binding, window = fixture()
        del binding["tx_identity"]
        self.assertEqual(association.assess(run, binding, window)["status"], "NOT_PROVEN")

    def test_lan_stale_future_or_other_host_observation_rejected(self):
        for case in ("lan", "stale", "future", "other", "MAC mismatch"):
            with self.subTest(case=case):
                run, binding, window = fixture()
                wifi = run["host_wifi"]
                if case == "lan": wifi["hardware_ports_stdout"] = "Hardware Port: Ethernet\nDevice: en0\n"
                if case == "stale": window["begin_mono_ns"] += 300_000_000_000
                if case == "future": wifi["end_mono_ns"] = window["begin_mono_ns"] + 1
                if case == "other": wifi["ifconfig_stdout"] = wifi["ifconfig_stdout"].replace("en0:", "en5:")
                if case == "MAC mismatch": binding["host_mac"] = "020000000099"
                result = association.assess(run, binding, window)
                self.assertEqual(result["status"], "NOT_PROVEN")
                self.assertNotIn("02:00:", json.dumps(result))


if __name__ == "__main__":
    unittest.main()

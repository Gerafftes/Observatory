lazy import asyncio
lazy import copy
lazy from contextlib import contextmanager
lazy import io
lazy import json
lazy from pathlib import Path
lazy import signal
lazy import sys
lazy import tempfile
lazy import time
lazy import unittest
lazy from unittest.mock import patch

lazy from project_tools import tx1_comparison_capture as capture
lazy from project_tools import tx7b_coverage as coverage


@contextmanager
def config_fixture(directory, mode="comparison"):
    """Bind an inert test helper only for the lifetime of this synthetic config."""
    inspector = Path(directory) / "synthetic-process-inspector.py"
    inspector.write_text("raise SystemExit('synthetic fixture must never execute')\n")
    with patch.object(capture, "PROCESS_INSPECTOR", inspector):
        yield _bound_config_fixture(directory, mode)


def _bound_config_fixture(directory, mode):
    directory = Path(directory)
    user = directory / "authorization.txt"
    user.write_text("explicit comparison and separate seven-client authorization")
    now = time.time_ns()
    plan = capture.PLANS[mode]
    routes = {"nodes": "/api/v1/nodes", "mmwave": "/api/v1/mmwave/status",
              "ready": "/health/ready", "profiles": "/api/v1/experiments/setup-profiles",
              "metrics": "/health/metrics"}
    apis = {key: f"http://127.0.0.1:8080{route}" for key, route in routes.items()}
    apis.update({key: f"http://192.168.4.{index + 1}:8032/" + ("status" if key == "TX1" else "ota/status")
                 for index, key in enumerate(sorted(capture.DEVICE_KEYS))})
    artifacts = {}
    for name in ("limits", "criteria", "setup"):
        path = directory / (name + ".json")
        path.write_text(json.dumps({"bound": name}))
        artifacts[name] = {"file": str(path), "sha256": capture.digest(path)}
    candidate = directory / "candidate.bin"
    candidate.write_bytes(b"synthetic offline unit-test candidate")
    receipt = directory / "build-receipt.json"
    receipt.write_text(json.dumps({"candidate": {"file": candidate.name, "sha256": capture.digest(candidate)},
                                   "runtime_image_sha256": "a" * 64, "source_sha256": "b" * 64}))
    artifacts["tx_build_receipt"] = {"file": str(receipt), "sha256": capture.digest(receipt)}
    artifacts["tx_candidate"] = {"file": str(candidate), "sha256": capture.digest(candidate)}
    definition = {"schema_version": 5, "test_identity": "sealed-test",
                  "transmitter": {"firmware": {"artifact_sha256": capture.digest(candidate)}}}
    encoded = (json.dumps(definition, sort_keys=True, indent=2) + "\n").encode()
    setup_hash = capture.hashlib.sha256(b"ruview.position-setup.v5\0" + encoded).hexdigest()
    setup = {"id": "setup-" + setup_hash[:16], "sha256": setup_hash}
    setup_path = Path(artifacts["setup"]["file"])
    setup_path.write_text(json.dumps({"schema_version": 5, "kind": "ruview.position-setup",
                                    "setup_id": setup["id"], "setup_sha256": setup_hash,
                                    "definition": definition}))
    artifacts["setup"]["sha256"] = capture.digest(setup_path)
    connection = directory / "connection-config.json"
    connection.write_text(json.dumps({"ap_mac": "02:11:22:33:44:56", "internet_interface": "en7",
                                      "internet_hardware_port": "Ethernet"}))
    process = directory / "process.json"
    process.write_text(json.dumps({"pid": 777, "argv": ["server", "--position-setup", "sealed.json"],
                                  "executable": "/server", "cwd": "/project", "sha256": "a" * 64,
                                  "bytes": 512, "process": "777 parent uid gid bound-start server"}))
    host_expected = {"csi_interface": "en0", "dhcp_primary": "192.168.4.2", "alias": "192.168.4.50/32",
                     "gateway_mac": "02:11:22:33:44:56", "default_interface": "en7",
                     "internet_interface": "en7", "internet_hardware_port": "Ethernet"}
    config = {"mode": mode, "interface": "en0", "apis": apis,
              "authorization": {"scope": plan["scope"], "max_runs": 1,
                                "max_windows": plan["windows"], "user_request_file": str(user),
                                "user_request_sha256": capture.digest(user),
                                "issued_wall_ns": now - 1_000_000_000,
                                "expires_wall_ns": now + 900_000_000_000},
              "setup": setup,
              "participants": {key: {"own_identity": key} for key in capture.DEVICE_KEYS | {"Mac"}},
              "identity_checks": [{"endpoint_key": key, "path": ["boot_id"], "expected": "bound-boot"}
                                  for key in sorted(capture.DEVICE_KEYS)],
              "counter_checks": [{"endpoint_key": key, "path": list(path)}
                                 for key, path in capture.preparation.counter_paths()],
              "live_guard": {"pid": 777, "connection_expected": host_expected,
                             "connection_config": {"file": str(connection), "sha256": capture.digest(connection)},
                             "frozen_process": {"file": str(process), "sha256": capture.digest(process)}},
              "artifacts": artifacts,
              "tool_sha256": {str(Path(module.__file__).resolve()): capture.digest(module.__file__)
                              for module in (capture, capture.existing, coverage,
                                             capture.preparation, capture.preparation.evaluator)}}
    config["tool_sha256"].update({str(path): capture.digest(path)
                                  for path in (capture.CONNECTION_SOURCE, capture.PROCESS_INSPECTOR,
                                               capture.ASSOCIATION_SOURCE)})
    config["identity_checks"].extend([{"endpoint_key": "TX1", "path": ["image_sha256"], "expected": "a" * 64},
                                      {"endpoint_key": "TX1", "path": ["source_sha256"], "expected": "b" * 64}])
    preflight = directory / "preflight.json"
    saved = {key: copy.deepcopy(config[key]) for key in
             ("apis", "interface", "participants", "setup", "identity_checks", "counter_checks", "artifacts", "live_guard")}
    started = time.monotonic_ns()
    saved.update(status="PASS", wall_ns=now, begin_mono_ns=started, begin_wall_ns=now)
    if mode == "seven_clients":
        saved["host_wifi"] = {"interface": "en0", "begin_mono_ns": started - 10,
                              "end_mono_ns": started, "hardware_ports_stdout": "Hardware Port: Wi-Fi\nDevice: en0\n",
                              "ifconfig_stdout": "en0: flags=UP\n\tether 02:11:22:33:44:56\n"}
    preflight.write_text(json.dumps(saved))
    config["preflight"] = {"file": str(preflight), "sha256": capture.digest(preflight),
                           "wall_ns": now, "status": "PASS_FRESH_READ_ONLY_PREFLIGHT",
                           "begin_mono_ns": started, "begin_wall_ns": now}
    if mode == "seven_clients":
        config["preflight"]["host_wifi"] = saved["host_wifi"]
    prepared = directory / "prepared.json"
    prepared.write_text(json.dumps({"schema": capture.preparation.SCHEMA,
                                   "status": "PASS_OFFLINE_PREPARATION",
                                   "configuration_sha256": capture.preparation.projection(config),
                                   "field_contract": capture.preparation.field_contract(config)}))
    config["preparation"] = {"file": str(prepared), "sha256": capture.digest(prepared)}
    return config


async def never_guard(*args):
    await asyncio.Event().wait()


class ContractTests(unittest.TestCase):
    def test_three_contiguous_windows_share_boundaries_without_extra_duration(self):
        plan, epoch = capture.PLANS["comparison"], 5_000_000_000
        windows = capture.windows(plan, epoch)
        self.assertEqual(capture.sample_count(plan), 421)
        self.assertEqual([(w["first_slot"], w["last_slot"]) for w in windows],
                         [(60, 180), (180, 300), (300, 420)])
        for left, right in zip(windows, windows[1:]):
            self.assertEqual(left["end_mono_ns"], right["begin_mono_ns"])
        self.assertEqual(windows[-1]["end_mono_ns"] - windows[0]["begin_mono_ns"], 360_000_000_000)
        self.assertEqual(capture.annotate({"slot": 59}, plan, epoch)["window_indices"], [])
        self.assertEqual(capture.annotate({"slot": 180}, plan, epoch)["window_indices"], [1, 2])
        self.assertEqual(capture.annotate({"slot": 300}, plan, epoch)["window_indices"], [2, 3])
        for window in windows:
            self.assertEqual(window["last_slot"] - window["first_slot"] + 1, 121)

    def test_seven_client_window_and_entire_scope_have_separate_bounds(self):
        plan = capture.PLANS["seven_clients"]
        self.assertEqual(capture.sample_count(plan), 121)
        self.assertEqual(plan["scope_seconds"], 180)
        self.assertEqual(plan["readiness_seconds"] + capture.WINDOW_SECONDS + plan["closing_seconds"], 180)
        self.assertEqual(capture.windows(plan, 0)[0]["end_mono_ns"], 120_000_000_000)

    def test_config_binding_freshness_limits_and_identity_are_required(self):
        with tempfile.TemporaryDirectory() as directory, config_fixture(directory) as config:
            capture.validate_config(config)
            for key in ("expired", "stale", "different setup", "tool", "limits", "identity"):
                bad = copy.deepcopy(config)
                if key == "expired":
                    bad["authorization"]["expires_wall_ns"] = 0
                elif key == "stale":
                    bad["preflight"]["wall_ns"] -= 121_000_000_000
                elif key == "different setup":
                    bad["setup"]["id"] = "unbound"
                elif key == "tool":
                    bad["tool_sha256"][str(Path(capture.__file__).resolve())] = "0" * 64
                elif key == "limits":
                    bad["artifacts"]["limits"]["sha256"] = "0" * 64
                else:
                    bad["identity_checks"] = []
                with self.assertRaises(ValueError, msg=key):
                    capture.validate_config(bad)

    def test_synthetic_inspector_requires_an_unchanged_tool_binding(self):
        original = capture.PROCESS_INSPECTOR
        with tempfile.TemporaryDirectory() as directory:
            unavailable = Path(directory) / "absent-private-inspector.py"
            with patch.object(capture, "PROCESS_INSPECTOR", unavailable):
                with config_fixture(directory) as config:
                    inspector = capture.PROCESS_INSPECTOR
                    self.assertFalse(unavailable.exists())
                    self.assertNotEqual(inspector, unavailable)
                    self.assertEqual(config["tool_sha256"][str(inspector)], capture.digest(inspector))
                    capture.validate_config(config)
                    missing = copy.deepcopy(config)
                    del missing["tool_sha256"][str(inspector)]
                    with self.assertRaisesRegex(ValueError, "must be bound"):
                        capture.validate_config(missing)
                    inspector.write_text("raise SystemExit('tampered synthetic helper')\n")
                    with self.assertRaisesRegex(ValueError, "tested tool binding changed"):
                        capture.validate_config(config)
                self.assertEqual(capture.PROCESS_INSPECTOR, unavailable)
        self.assertEqual(capture.PROCESS_INSPECTOR, original)

    def test_synthetic_inspector_patch_is_restored_after_an_exception(self):
        original = capture.PROCESS_INSPECTOR
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaisesRegex(RuntimeError, "fixture exit"):
                with config_fixture(directory):
                    raise RuntimeError("fixture exit")
        self.assertEqual(capture.PROCESS_INSPECTOR, original)

    def test_capture_is_passive_and_restricted_to_bound_project_udp(self):
        with tempfile.TemporaryDirectory() as directory, config_fixture(directory) as config:
            command = capture.capture_command(config, Path(directory) / "capture.pcap")
            self.assertIn("-p", command)
            self.assertEqual(command[command.index("-j") + 1], "host")
            self.assertEqual(command[command.index("-s") + 1], "0")
            self.assertEqual(command[command.index("-i") + 1], "en0")
            for port in (4210, 5005, 55010):
                self.assertIn(str(port), command[-1])
            self.assertNotIn("arp", command[-1])
            self.assertNotIn("src net", command[-1])

    def test_metric_failure_continues_but_identity_change_aborts(self):
        checks = [{"endpoint_key": "RX4", "path": ["boot_id"], "expected": "same"}]
        row = {"endpoint_key": "RX4", "http_status": 200, "body_complete": True,
               "value": {"boot_id": "same", "last_seen_ms": 3000}}
        capture.check_identity(row, checks)
        row["value"]["boot_id"] = "changed"
        with self.assertRaises(capture.SafetyAbort):
            capture.check_identity(row, checks)
        row.update(http_status=None, body_complete=False, value=None)
        capture.check_identity(row, checks)

    def test_counter_regression_aborts_but_http_failure_does_not_reset_state(self):
        checks = [{"endpoint_key": "RX4", "path": ["packets_sent"]}]
        previous = {}
        row = {"endpoint_key": "RX4", "http_status": 200, "body_complete": True,
               "value": {"packets_sent": 400}}
        capture.check_counters(row, checks, previous)
        row["value"]["packets_sent"] = 401
        capture.check_counters(row, checks, previous)
        bad = {**row, "http_status": None, "body_complete": False, "value": None}
        capture.check_counters(bad, checks, previous)
        row["value"]["packets_sent"] = 400
        with self.assertRaises(capture.SafetyAbort):
            capture.check_counters(row, checks, previous)
        for value in (-1, "402", True):
            row["value"]["packets_sent"] = value
            with self.assertRaises(capture.SafetyAbort):
                capture.check_counters(row, checks, previous)


class SchedulingTests(unittest.IsolatedAsyncioTestCase):
    async def test_actual_twelve_endpoint_getter_keeps_bad_clock_reads_and_reaps_children(self):
        from project_tools.tests.test_tx7b_coverage import OFFSET
        original_spawn = asyncio.create_subprocess_exec
        for fault, expected_reason in ((None, None), ("bracket", "clock bracket over 1 ms"),
                                       ("wall", "wall timestamp regression"),
                                       ("mono", "monotonic timestamp regression or overlapping requests")):
            class Clock:
                mono, reads, pairs = 3_000_000_000, 0, 0
                def monotonic_ns(self):
                    self.reads += 1
                    if self.reads % 2 == 1:
                        self.pairs += 1
                        if fault == "mono" and self.pairs == 8:
                            self.mono -= 5_000_000
                    elif fault == "bracket" and self.pairs == 8:
                        self.mono += 1_982_293
                    self.mono += 30
                    return self.mono
                def time_ns(self):
                    return self.mono + OFFSET - (5_000_000 if fault == "wall" and self.pairs == 8 else 0)

            clock, children = Clock(), []

            async def curl(*args, **kwargs):
                Path(args[args.index("--output") + 1]).write_text('{"complete":true}')
                child = await original_spawn(sys.executable, "-c", "print('200', end='')", **kwargs)
                children.append(child)
                return child

            with tempfile.TemporaryDirectory() as directory:
                recorder = capture.Requests(Path(directory), io.StringIO(), [], [])
                # Keep the original stamp implementation, including all three
                # reads; inject only the independent host clock inputs.
                original_stamp = coverage.stamp
                def clock_read():
                    return original_stamp(clock)
                with patch.object(capture.asyncio, "create_subprocess_exec", side_effect=curl), \
                     patch.object(coverage, "stamp", side_effect=clock_read):
                    await asyncio.gather(*(recorder.get(key, "http://127.0.0.1/" + key, 0, 0)
                                           for key in sorted(capture.API_KEYS)))
                self.assertEqual(len(recorder.rows), 12)
                self.assertEqual(len(recorder.events), 24)
                self.assertFalse(recorder.children)
                self.assertTrue(all(child.returncode is not None for child in children))
                proof = {"schema": coverage.CONCURRENT_SCHEMA, "clock_events": recorder.events,
                         "clean_exit": True, "exit_code": 0, "kernel_drops": 0,
                         "pcap_sha256": "clock-fixture", "timestamp_type": "host",
                         "timestamp_type_confirmed": True, "ready_mono_ns": 0,
                         "stop_requested_mono_ns": 10_000_000_000,
                         "clock_info": {"time": {"resolution_ns": 1},
                                        "monotonic": {"resolution_ns": 1, "monotonic": True}},
                         "clock_pairs": [{"wall_ns": OFFSET, "mono_before_ns": 0, "mono_after_ns": 0},
                                         {"wall_ns": OFFSET + 11_000_000_000,
                                          "mono_before_ns": 11_000_000_000, "mono_after_ns": 11_000_000_000}]}
                span = {"first_ns": OFFSET, "last_ns": OFFSET + 10_000_000_000,
                        "resolution_ns": 1, "sha256": "clock-fixture"}
                result = coverage.assess(recorder.rows, span, proof, ["0:" + key for key in sorted(capture.API_KEYS)])
                self.assertEqual(result["status"], "COVERAGE_PASS" if fault is None else "COVERAGE_FAIL")
                if expected_reason:
                    self.assertIn(expected_reason, result["reasons"])

    async def test_original_readiness_start_cannot_be_reset_by_entering_collector(self):
        with tempfile.TemporaryDirectory() as directory, config_fixture(directory, "seven_clients") as config:
            root = Path(directory).resolve()
            (root / "private").mkdir()
            config["preflight"]["begin_mono_ns"] -= 31_000_000_000
            config["preflight"]["begin_wall_ns"] -= 31_000_000_000
            saved_path = Path(config["preflight"]["file"])
            saved = json.loads(saved_path.read_text())
            saved.update({key: config["preflight"][key] for key in ("begin_mono_ns", "begin_wall_ns")})
            saved_path.write_text(json.dumps(saved))
            config["preflight"]["sha256"] = capture.digest(saved_path)
            with patch.object(capture, "PROJECT", root), \
                 patch.object(capture.asyncio, "create_subprocess_exec", side_effect=AssertionError("no capture budget")) as spawn:
                result = await capture.collect(config, root / "private" / "expired-ready")
            self.assertIn("readiness budget exhausted before capture start", result["error"])
            self.assertEqual(result["attempted"], 0)
            spawn.assert_not_called()

    async def test_reused_scheduler_keeps_one_request_per_url_and_skips_catchup(self):
        active, maximum, rows = {}, {}, []

        class Recorder:
            async def get(self, key, url, slot, due):
                active[key] = active.get(key, 0) + 1
                maximum[key] = max(maximum.get(key, 0), active[key])
                await asyncio.sleep(0.025 if key == "slow" else 0.0001)
                active[key] -= 1
                row = {"request_id": f"{slot}:{key}", "end": {"mono_after_ns": time.monotonic_ns()}}
                rows.append(row)
                return row

        slots, skipped = [], []
        await capture.existing.run_schedules({"slow": "a", "RX4": "b"}, Recorder(),
                                             time.monotonic_ns(), slots, skipped, 8, 0.01)
        self.assertEqual(maximum, {"slow": 1, "RX4": 1})
        self.assertEqual(len([row for row in rows if row["request_id"].endswith(":RX4")]), 8)
        self.assertTrue(skipped)
        self.assertTrue(all(item["slot_id"].endswith(":slow") for item in skipped))

    async def test_identity_abort_cancels_only_our_schedules(self):
        cancelled = []

        async def blocked(*args):
            try:
                await asyncio.Event().wait()
            finally:
                cancelled.append(True)

        class OwnCapture:
            returncode = 2

        with tempfile.TemporaryDirectory() as directory, config_fixture(directory) as config:
            with patch.object(capture.existing, "run_schedules", side_effect=blocked), \
                 patch.object(capture, "host_guard_loop", side_effect=never_guard):
                with self.assertRaises(capture.SafetyAbort):
                    await capture.observe(config, OwnCapture(), object(), time.monotonic_ns(), [], [],
                                          capture.PLANS["comparison"])
        self.assertTrue(cancelled)

    async def test_frozen_limits_change_aborts_without_a_second_capture(self):
        cancelled = []

        class OwnCapture:
            returncode = None

        async def change_limits(*args):
            Path(config["artifacts"]["limits"]["file"]).write_text('{"changed":true}')
            try:
                await asyncio.Event().wait()
            finally:
                cancelled.append(True)

        with tempfile.TemporaryDirectory() as directory, config_fixture(directory) as config:
            with patch.object(capture.existing, "run_schedules", side_effect=change_limits), \
                 patch.object(capture, "host_guard_loop", side_effect=never_guard):
                with self.assertRaises(capture.SafetyAbort):
                    await capture.observe(config, OwnCapture(), object(), time.monotonic_ns(), [], [],
                                          capture.PLANS["comparison"])
        self.assertTrue(cancelled)

    async def test_total_budget_cancel_cleanup_and_no_second_attempt(self):
        children, cancelled = [], []

        class OwnCapture:
            returncode = None
            signals = []
            def send_signal(self, sig):
                self.signals.append(sig)
                self.returncode = 0
            async def wait(self):
                return self.returncode

        async def fake_capture(*args, **kwargs):
            Path(args[args.index("-w") + 1]).write_bytes(b"offline fixture")
            kwargs["stderr"].write("listening on offline-test\n0 packets dropped by kernel\n")
            kwargs["stderr"].flush()
            child = OwnCapture()
            children.append(child)
            return child

        async def blocked(*args):
            try:
                await asyncio.Event().wait()
            finally:
                cancelled.append(True)

        plan = {"scope": "offline", "windows": 3, "warmup_seconds": 60,
                "readiness_seconds": 0.08, "closing_seconds": 0.08, "scope_seconds": 0.12}
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "private").mkdir()
            out = root / "private" / "one-shot"
            config = {"mode": "comparison", "apis": {key: f"http://192.168.4.1/{key}"
                      for key in capture.API_KEYS}, "interface": "en0", "identity_checks": [], "counter_checks": [],
                      "preflight": {"begin_mono_ns": time.monotonic_ns()}}
            with patch.object(capture, "PROJECT", root.resolve()), patch.object(capture, "validate_config"), \
                 patch.dict(capture.PLANS, {"comparison": plan}), \
                 patch.object(capture, "ROLL_SECONDS", 0.001), patch.object(capture, "CLEANUP_SECONDS", 0.03), \
                 patch.object(capture.asyncio, "create_subprocess_exec", side_effect=fake_capture), \
                 patch.object(capture, "observe", side_effect=blocked), \
                 patch.object(capture.preparation, "field_contract", return_value=[]), \
                 patch.object(coverage, "pcap_span", return_value={"first_ns": 0}), \
                 patch.object(coverage, "assess", return_value={"status": "COVERAGE_FAIL"}) as assessment:
                result = await capture.collect(config, out)
                self.assertIn("TimeoutError", result["error"])
                self.assertTrue(result["scope_within_budget"])
                self.assertTrue(result["cleanup_within_budget"])
                self.assertEqual(len(assessment.call_args.args[3]), 421 * len(capture.API_KEYS))
                with self.assertRaises(FileExistsError):
                    await capture.collect(config, out)
            self.assertEqual(len(children), 1)
            self.assertEqual(children[0].signals, [signal.SIGINT])
            self.assertTrue(cancelled)
            run = json.loads((out / "run.private.json").read_text())
            self.assertEqual(len(run["windows"]), 3)
            self.assertEqual(len(run["result"]["unattempted_slot_ids"]), 421 * len(capture.API_KEYS))


class HostGuardTests(unittest.IsolatedAsyncioTestCase):
    def replies(self):
        return {
            "/usr/sbin/networksetup -listallhardwareports": "Hardware Port: Wi-Fi\nDevice: en0\nHardware Port: Ethernet\nDevice: en7\n",
            "/usr/sbin/networksetup -getairportnetwork en0": "Current Wi-Fi Network: csi-test\n",
            "/sbin/route -n get 192.168.4.1": "interface: en0\ngateway: link#4\n",
            "/sbin/route -n get default": "interface: en7\ngateway: 172.20.10.1\n",
            "/usr/sbin/ipconfig getpacket en0": "yiaddr = 192.168.4.2\nsubnet_mask (ip): 255.255.255.0\nrouter (ip_mult): {192.168.4.1}\nserver_identifier (ip): 192.168.4.1\n",
            "/usr/sbin/arp -n -i en0 192.168.4.1": "? (192.168.4.1) at 2:11:22:33:44:56 on en0 ifscope\n",
            "/usr/sbin/arp -an -i en0": "",
            "/sbin/ifconfig -a": "en0: flags=UP\n\tether 00:11:22:33:44:55\n\tinet 192.168.4.2 netmask 0xffffff00\n\tinet 192.168.4.50 netmask 0xffffffff\nen7: flags=UP\n\tinet 172.20.10.2 netmask 0xfffffff0\n",
        }

    async def test_host_and_process_reads_preserve_bound_argv_and_listener_ownership(self):
        for kind, expected_error in [("pass", None), ("read failure", capture.GuardEvidenceUnknown),
                                     ("interface", capture.SafetyAbort), ("gateway", capture.SafetyAbort),
                                     ("listener", capture.SafetyAbort), ("argv", capture.SafetyAbort),
                                     ("dhcp", capture.SafetyAbort)]:
            with tempfile.TemporaryDirectory() as directory, config_fixture(directory) as config:
                replies = self.replies()
                if kind == "interface":
                    replies["/sbin/route -n get 192.168.4.1"] = "interface: en9\n"
                if kind == "gateway":
                    replies["/usr/sbin/arp -n -i en0 192.168.4.1"] = "? (192.168.4.1) at 2:11:22:33:44:99 on en0\n"
                if kind == "dhcp":
                    for name in ("/usr/sbin/ipconfig getpacket en0", "/sbin/ifconfig -a"):
                        replies[name] = replies[name].replace("192.168.4.2", "192.168.4.3")

                async def fake(command):
                    if command[0] == "/usr/bin/python3":
                        frozen = json.loads(Path(config["live_guard"]["frozen_process"]["file"]).read_text())
                        frozen["environment"] = {"PRIVATE": "must never be printed"}
                        if kind == "argv":
                            frozen["argv"].append("--changed")
                        Path(command[-1]).write_text(json.dumps(frozen))
                        text = "safe inspector metadata"
                    elif command[0] == "/usr/sbin/lsof":
                        text = "p778\n" if kind == "listener" else "p777\n"
                    else:
                        if kind == "read failure" and command[0] == "/usr/sbin/ipconfig":
                            raise TimeoutError("bounded read")
                        text = replies[" ".join(command)]
                    return {"returncode": 0, "stdout": text, "stderr_present": False}

                with patch.object(capture, "guard_command", side_effect=fake):
                    if expected_error:
                        with self.assertRaises(expected_error, msg=kind):
                            await capture.host_guard_once(config, Path(directory), 0)
                    else:
                        result = await capture.host_guard_once(config, Path(directory), 0)
                        self.assertEqual(result["process_binding"], "MATCH")
                        self.assertNotIn("environment", result)

    async def test_guard_retains_actual_wifi_mac_even_later_in_comparison_scope(self):
        for changed in (False, True):
            with tempfile.TemporaryDirectory() as directory, config_fixture(directory) as config:
                now = time.monotonic_ns()
                config["preflight"]["host_wifi"] = {
                    "interface": "en0", "begin_mono_ns": now - 350_000_000_000,
                    "end_mono_ns": now - 349_000_000_000,
                    "hardware_ports_stdout": "Hardware Port: Wi-Fi\nDevice: en0\n",
                    "ifconfig_stdout": "en0: flags=UP\n\tether 00:11:22:33:44:55\n"}
                replies = self.replies()
                if changed:
                    replies["/sbin/ifconfig -a"] = replies["/sbin/ifconfig -a"].replace("00:11:22:33:44:55", "00:11:22:33:44:99")

                async def fake(command):
                    if command[0] == "/usr/bin/python3":
                        Path(command[-1]).write_text(Path(config["live_guard"]["frozen_process"]["file"]).read_text())
                        output = ""
                    elif command[0] == "/usr/sbin/lsof":
                        output = "p777\n"
                    else:
                        output = replies[" ".join(command)]
                    return {"returncode": 0, "stdout": output, "stderr_present": False}

                with patch.object(capture, "guard_command", side_effect=fake):
                    if changed:
                        with self.assertRaises(capture.SafetyAbort):
                            await capture.host_guard_once(config, Path(directory), 70)
                    else:
                        result = await capture.host_guard_once(config, Path(directory), 0)
                        self.assertEqual(result["process_binding"], "MATCH")
                        second = await capture.host_guard_once(config, Path(directory), 1)
                        self.assertEqual(Path(result["host_wifi_file"]).name, "guard-host-wifi-0.private.json")
                        self.assertEqual(Path(second["host_wifi_file"]).name, "guard-host-wifi-1.private.json")
                        self.assertTrue(Path(result["host_wifi_file"]).is_file())
                        self.assertTrue(Path(second["host_wifi_file"]).is_file())

    async def test_slow_host_guard_does_not_delay_http_and_final_guard_is_awaited(self):
        order = []

        class OwnCapture:
            returncode = None

        async def http(*args):
            order.append("HTTP done")

        async def host(*args):
            await asyncio.sleep(0.03)
            order.append("host done")

        with tempfile.TemporaryDirectory() as directory, config_fixture(directory) as config:
            with patch.object(capture.existing, "run_schedules", side_effect=http), \
                 patch.object(capture, "host_guard_loop", side_effect=host):
                await capture.observe(config, OwnCapture(), object(), time.monotonic_ns(), [], [],
                                      capture.PLANS["comparison"])
        self.assertEqual(order, ["HTTP done", "host done"])

    async def test_read_timeout_reaps_only_its_own_helper_group(self):
        signals = []

        class OwnHelper:
            pid, returncode = 12345, None
            async def communicate(self):
                await asyncio.Event().wait()
            async def wait(self):
                self.returncode = -9
                return self.returncode

        helper = OwnHelper()

        async def spawn(*args, **kwargs):
            self.assertTrue(kwargs["start_new_session"])
            return helper

        with patch.object(capture.asyncio, "create_subprocess_exec", side_effect=spawn), \
             patch.object(capture.os, "killpg", side_effect=lambda pid, sig: signals.append((pid, sig))), \
             patch.object(capture, "GUARD_COMMAND_SECONDS", 0.01):
            with self.assertRaises(TimeoutError):
                await capture.guard_command(["/offline/read"])
        self.assertEqual(signals, [(helper.pid, signal.SIGKILL)])
        self.assertEqual(helper.returncode, -9)


if __name__ == "__main__":
    unittest.main()

"""One authorized comparison or separate seven-client read-only capture.

No I/O on import. Reuses the existing independent HTTP schedules and v3 clock
proof; old collectors and evaluators remain unchanged. No automatic retry.
"""
lazy import argparse
lazy import ast
lazy import asyncio
lazy from functools import lru_cache
lazy import hashlib
lazy import ipaddress
lazy import json
lazy import os
lazy from pathlib import Path
lazy import re
lazy import signal
lazy import subprocess
lazy import time
lazy import urllib.parse

if __package__:
    lazy from . import mmwave_freshness_capture as existing
    lazy from . import tx7b_coverage as coverage
    lazy from . import tx1_measurement_prepare as preparation
else:
    lazy import mmwave_freshness_capture as existing
    lazy import tx7b_coverage as coverage
    lazy import tx1_measurement_prepare as preparation

SCHEMA = "tx1.comparison-capture.v1"
PROJECT = Path(__file__).resolve().parents[1]
DEVICE_KEYS = {"TX1", "RX1", "RX2", "RX3", "RX4", "MMWAVE1", "MMWAVE2"}
API_KEYS = DEVICE_KEYS | {"nodes", "mmwave", "ready", "profiles", "metrics"}
PERIOD_SECONDS = 1.0
WINDOW_SECONDS = 120
ROLL_SECONDS = 2.1
CLEANUP_SECONDS = 6
GUARD_PERIOD_SECONDS = 5
GUARD_COMMAND_SECONDS = 3
CONNECTION_SOURCE = PROJECT / "software/ruview/firmware/esp32-csi-tx/tools/tx1_measure.py"
PROCESS_INSPECTOR = PROJECT / "private/tx1-system-integration-20261003/inspect-process.private.py"
ASSOCIATION_SOURCE = PROJECT / "project_tools/tx1_association.py"
PLANS = {
    "comparison": {"scope": "TX1_COMPARISON_THREE_WINDOWS", "windows": 3,
                   "warmup_seconds": 60, "readiness_seconds": 180,
                   "closing_seconds": 30, "scope_seconds": 630},
    "seven_clients": {"scope": "TX1_SEVEN_CLIENTS_ONE_WINDOW", "windows": 1,
                      "warmup_seconds": 0, "readiness_seconds": 30,
                      "closing_seconds": 30, "scope_seconds": 180},
}


class SafetyAbort(Exception):
    """An observed binding change ends only our measurement processes."""


class GuardEvidenceUnknown(Exception):
    """A required read failed; this does not prove a configuration change."""


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def sample_count(plan):
    return plan["warmup_seconds"] + plan["windows"] * WINDOW_SECONDS + 1


def windows(plan, epoch_ns):
    start = plan["warmup_seconds"]
    return [{"index": index + 1, "first_slot": start + index * WINDOW_SECONDS,
             "last_slot": start + (index + 1) * WINDOW_SECONDS,
             "begin_mono_ns": epoch_ns + (start + index * WINDOW_SECONDS) * 1_000_000_000,
             "end_mono_ns": epoch_ns + (start + (index + 1) * WINDOW_SECONDS) * 1_000_000_000}
            for index in range(plan["windows"])]


def annotate(row, plan, epoch_ns):
    row["phase"] = "warmup" if row["slot"] < plan["warmup_seconds"] else "measurement"
    row["slot_epoch_ns"] = epoch_ns
    row["window_indices"] = [window["index"] for window in windows(plan, epoch_ns)
                             if window["first_slot"] <= row["slot"] <= window["last_slot"]]
    return row


def check_identity(row, checks):
    # Failed/incomplete HTTP is measurement failure, not proof of changed identity.
    if row["http_status"] != 200 or not row["body_complete"] or row["value"] is None:
        return
    for check in checks:
        if check["endpoint_key"] != row["endpoint_key"]:
            continue
        value = row["value"]
        try:
            for component in check["path"]:
                value = value[component]
        except (KeyError, IndexError, TypeError):
            raise SafetyAbort(f"required identity/config field absent: {row['endpoint_key']}") from None
        if not preparation.same_value(value, check["expected"]):
            raise SafetyAbort(f"identity/config binding changed: {row['endpoint_key']}")


def check_counters(row, checks, previous):
    if row["http_status"] != 200 or not row["body_complete"] or row["value"] is None:
        return
    for check in checks:
        if check["endpoint_key"] != row["endpoint_key"]:
            continue
        value = row["value"]
        try:
            for component in check["path"]:
                value = value[component]
        except (KeyError, IndexError, TypeError):
            raise SafetyAbort(f"required counter absent: {row['endpoint_key']}") from None
        key = (check["endpoint_key"], tuple(check["path"]))
        if type(value) is not int or value < 0:
            raise SafetyAbort(f"invalid cumulative counter: {row['endpoint_key']}")
        if value < previous.get(key, value):
            raise SafetyAbort(f"cumulative counter regressed: {row['endpoint_key']}")
        previous[key] = value


class Requests(existing.Requests):
    def __init__(self, out, stream, checks, counter_checks, fields=(), headers=None):
        super().__init__(out, stream, headers=headers)
        self.checks, self.counter_checks, self.previous_counters = checks, counter_checks, {}
        self.fields = fields
        self.guard_events = []

    async def get(self, key, url, index, due_ns):
        row = await super().get(key, url, index, due_ns)
        try:
            preparation.validate_fields(row, self.fields)
        except (KeyError, IndexError, TypeError, ValueError):
            raise SafetyAbort(f"prepared response type/path changed: {key}") from None
        check_identity(row, self.checks)
        check_counters(row, self.counter_checks, self.previous_counters)
        return row


def validate_config(config, live=True):
    """All bindings are checked before creating a directory or live child."""
    plan = PLANS[config["mode"]]
    auth = config["authorization"]
    pre = config.get("preflight")
    now = time.time_ns()
    if (auth["scope"] != plan["scope"] or auth["max_runs"] != 1
            or auth["max_windows"] != plan["windows"]
            or (live and not auth["issued_wall_ns"] <= now <= auth["expires_wall_ns"])
            or digest(auth["user_request_file"]) != auth["user_request_sha256"]):
        raise ValueError("fresh explicit one-shot authorization binding required")
    if live and (pre["status"] != "PASS_FRESH_READ_ONLY_PREFLIGHT"
            or not 0 <= now - pre["wall_ns"] <= 120_000_000_000
            or digest(pre["file"]) != pre["sha256"]):
        raise ValueError("bound read-only preflight within 120 seconds required")
    required = {str(Path(__file__).resolve()), str(Path(existing.__file__).resolve()),
                str(Path(coverage.__file__).resolve()), str(Path(preparation.__file__).resolve()),
                str(Path(preparation.evaluator.__file__).resolve()), str(CONNECTION_SOURCE),
                str(PROCESS_INSPECTOR), str(ASSOCIATION_SOURCE)}
    if not required <= set(config["tool_sha256"]):
        raise ValueError("runner, HTTP scheduler and clock evaluator must be bound")
    for path, expected in config["tool_sha256"].items():
        if digest(path) != expected:
            raise ValueError("tested tool binding changed")
    if not {"limits", "criteria", "setup"} <= set(config["artifacts"]):
        raise ValueError("limits, criteria and sealed setup artifacts required")
    for artifact in config["artifacts"].values():
        if digest(artifact["file"]) != artifact["sha256"]:
            raise ValueError("criteria/limits/setup artifact binding changed")
    preparation.validate_setup(config)
    preparation.validate_counter_checks(config["counter_checks"])
    guard = config["live_guard"]
    for name in ("connection_config", "frozen_process"):
        if digest(guard[name]["file"]) != guard[name]["sha256"]:
            raise ValueError("live host/process guard binding changed")
    if type(guard["pid"]) is not int or guard["pid"] <= 0:
        raise ValueError("current server PID must be bound")
    host_keys = {"csi_interface", "dhcp_primary", "alias", "gateway_mac", "default_interface",
                 "internet_interface", "internet_hardware_port"}
    if not host_keys <= set(guard["connection_expected"]):
        raise ValueError("full host identity and separate LAN route must be bound")
    if guard["connection_expected"]["csi_interface"] != config["interface"]:
        raise ValueError("guard project interface differs")
    if not (DEVICE_KEYS | {"Mac"}) <= set(config["participants"]):
        raise ValueError("all eight participants including TX1 must be bound")
    if any(not value for value in config["participants"].values()):
        raise ValueError("empty participant identity binding")
    checks = config["identity_checks"]
    if not DEVICE_KEYS <= {check["endpoint_key"] for check in checks}:
        raise ValueError("own device identity checks required")
    for check in checks + config["counter_checks"]:
        if (check["endpoint_key"] not in API_KEYS or not check["path"]
                or any(type(part) not in (str, int) for part in check["path"])):
            raise ValueError("invalid identity/config check")
    if (set(config["apis"]) != API_KEYS
            or len(set(config["apis"].values())) != len(API_KEYS)
            or not re.fullmatch(r"en[0-9]+", config["interface"])):
        raise ValueError("unexpected endpoint or verified project interface inventory")
    routes = {"nodes": "/api/v1/nodes", "mmwave": "/api/v1/mmwave/status",
              "ready": "/health/ready", "profiles": "/api/v1/experiments/setup-profiles",
              "metrics": "/health/metrics"}
    for key, url in config["apis"].items():
        parsed = urllib.parse.urlsplit(url)
        address = ipaddress.ip_address(parsed.hostname)
        route = routes.get(key, "/status" if key == "TX1" else "/ota/status")
        if (parsed.scheme != "http" or parsed.username or parsed.password
                or parsed.query or parsed.fragment or parsed.path != route):
            raise ValueError("unexpected numeric, uncredentialed GET endpoint")
        if key in routes:
            if not address.is_loopback or parsed.port != 8080:
                raise ValueError("server endpoint must be existing local instance")
        elif address not in ipaddress.ip_network("192.168.4.0/24") or parsed.port != 8032:
            raise ValueError("device endpoint outside approved project network")
    headers = config.get("request_headers", {})
    if set(headers) - {"TX1"}:
        raise ValueError("only the existing protected TX status request may carry a private header")
    for header in headers.values():
        path = Path(header["file"]).resolve()
        if (not path.is_relative_to(PROJECT / "private")
                or digest(path) != header["sha256"] or path.stat().st_mode & 0o077):
            raise ValueError("bound private owner-only header file required")
        if not re.fullmatch(r"Authorization: Bearer [A-Za-z0-9._~+/=-]{1,1024}\r?\n?", path.read_text()):
            raise ValueError("exactly one existing Bearer header required")
    if not live:
        return
    preparation.verify_preparation(config)
    if (type(pre["begin_mono_ns"]) is not int or pre["begin_mono_ns"] < 0
            or pre["begin_mono_ns"] > time.monotonic_ns()
            or type(pre["begin_wall_ns"]) is not int or pre["begin_wall_ns"] > pre["wall_ns"]):
        raise ValueError("original fresh preflight scope start required")
    saved = json.loads(Path(pre["file"]).read_text())
    if saved["status"] != "PASS" or saved["wall_ns"] != pre["wall_ns"]:
        raise ValueError("preflight status/time binding differs")
    for name in ("apis", "interface", "participants", "setup", "identity_checks", "counter_checks", "artifacts", "live_guard"):
        if saved[name] != config[name]:
            raise ValueError(f"preflight {name} binding differs")
    for name in ("begin_mono_ns", "begin_wall_ns"):
        if pre[name] != saved[name]:
            raise ValueError("preflight scope start binding differs")
    if saved.get("request_headers", {}) != headers:
        raise ValueError("preflight private header binding differs")
    if config["mode"] == "seven_clients":
        host = pre["host_wifi"]
        if host != saved["host_wifi"] or host["interface"] != config["interface"]:
            raise ValueError("fresh actual Wi-Fi interface/MAC binding required")
    for name in ("wire_binding", "counter_starts"):
        if name in pre:
            if pre[name] != saved[name] or digest(pre[name]["file"]) != pre[name]["sha256"]:
                raise ValueError("fresh preflight evidence binding differs")


def capture_command(config, pcap):
    hosts = {key: urllib.parse.urlsplit(config["apis"][key]).hostname for key in DEVICE_KEYS}
    receivers = " or ".join(f"src host {hosts[f'RX{i}']}" for i in range(1, 5))
    expression = (f"udp and ((src host {hosts['TX1']} and dst port 4210) "
                  f"or (({receivers}) and dst port 5005) "
                  f"or (port 55010 and (host {hosts['MMWAVE1']} or host {hosts['MMWAVE2']})))")
    return ["/usr/sbin/tcpdump", "-i", config["interface"], "-j", "host", "-p", "-n",
            "-s", "0", "-U", "-w", str(pcap), expression]


async def ready_capture(process, pcap, stderr, seconds):
    async with asyncio.timeout(seconds):
        while True:
            if process.returncode is not None:
                raise RuntimeError("capture exited before readiness")
            text = stderr.read_text()
            if "warning" in text.lower():
                raise RuntimeError("capture host timestamp warning")
            try:
                coverage.pcap_span(pcap.read_bytes())
                if "listening on" in text:
                    return time.monotonic_ns()
            except (OSError, ValueError):
                pass
            await asyncio.sleep(0.05)


async def post_roll(process, pcap, latest_end_wall_ns):
    while True:
        if process.returncode is not None:
            raise RuntimeError("capture exited before post-roll")
        try:
            if coverage.pcap_span(pcap.read_bytes())["last_ns"] >= latest_end_wall_ns + round(ROLL_SECONDS * 1e9):
                return
        except (OSError, ValueError):
            pass
        await asyncio.sleep(0.05)


def bound_file_state(config):
    files = dict(config["tool_sha256"])
    files.update({item["file"]: item["sha256"] for item in config["artifacts"].values()})
    files[config["authorization"]["user_request_file"]] = config["authorization"]["user_request_sha256"]
    files[config["preflight"]["file"]] = config["preflight"]["sha256"]
    if "preparation" in config:
        files[config["preparation"]["file"]] = config["preparation"]["sha256"]
    files.update({item["file"]: item["sha256"] for item in config.get("request_headers", {}).values()})
    files.update({config["preflight"][name]["file"]: config["preflight"][name]["sha256"]
                  for name in ("wire_binding", "counter_starts") if name in config["preflight"]})
    files.update({config["live_guard"][key]["file"]: config["live_guard"][key]["sha256"]
                  for key in ("connection_config", "frozen_process")})
    return {path: (expected, file_state(path)) for path, expected in files.items()}


def file_state(path):
    stat = Path(path).stat()
    # Other read-only observers may change atime without changing the binding.
    return stat.st_dev, stat.st_ino, stat.st_size, stat.st_mtime_ns, stat.st_ctime_ns


@lru_cache(maxsize=1)
def connection_functions():
    # The legacy module uses Python 3.15 lazy imports. Extract only these two
    # unchanged function bodies so the current runner need not import it.
    lines = CONNECTION_SOURCE.read_text().splitlines(keepends=True)
    selected = []
    for start, line in enumerate(lines):
        if line.startswith(("def connection_check(", "def route_value(")):
            end = next((index for index in range(start + 1, len(lines))
                        if lines[index].startswith("def ")), len(lines))
            selected.extend(ast.parse("".join(lines[start:end])).body)
    if len(selected) != 2:
        raise ValueError("bound connection-check functions absent")
    namespace = {"re": re, "ipaddress": ipaddress, "command": None}
    exec(compile(ast.Module(body=selected, type_ignores=[]), str(CONNECTION_SOURCE), "exec"), namespace)
    return namespace["connection_check"], namespace["route_value"]


async def guard_command(command):
    """Bound only our read-only child/group; never expose raw command output."""
    process = None
    try:
        async with asyncio.timeout(GUARD_COMMAND_SECONDS):
            process = await asyncio.create_subprocess_exec(*command, stdout=asyncio.subprocess.PIPE,
                      stderr=asyncio.subprocess.PIPE, start_new_session=True)
            stdout, stderr = await process.communicate()
            return {"returncode": process.returncode, "stdout": stdout.decode(errors="replace"),
                    "stderr_present": bool(stderr)}
    finally:
        if process is not None and process.returncode is None:
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            await asyncio.wait_for(process.wait(), 1)


async def host_guard_once(config, out, index):
    observed_begin = time.monotonic_ns()
    wifi_evidence = {}
    guard, iface = config["live_guard"], config["interface"]
    process_file = out / f"guard-process-{index}.private.json"
    commands = [
        ["/usr/sbin/networksetup", "-listallhardwareports"],
        ["/sbin/route", "-n", "get", "192.168.4.1"],
        ["/usr/sbin/networksetup", "-getairportnetwork", iface],
        ["/usr/sbin/ipconfig", "getpacket", iface],
        ["/usr/sbin/arp", "-n", "-i", iface, "192.168.4.1"],
        ["/sbin/ifconfig", "-a"], ["/usr/sbin/arp", "-an", "-i", iface],
        ["/sbin/route", "-n", "get", "default"],
        ["/usr/sbin/lsof", "-nP", "-iTCP:8080", "-sTCP:LISTEN", "-Fp"],
        ["/usr/sbin/lsof", "-nP", "-iUDP:5005", "-Fp"],
        ["/usr/sbin/lsof", "-nP", "-iUDP:55010", "-Fp"],
        ["/usr/bin/python3", str(PROCESS_INSPECTOR), str(guard["pid"]), str(process_file)],
    ]
    responses = await asyncio.gather(*(guard_command(command) for command in commands), return_exceptions=True)
    for response_index, response in enumerate(responses):
        # Hidden SSID is permitted by the existing independent DHCP/ARP proof.
        if isinstance(response, Exception) or (response["returncode"] and response_index != 2):
            raise GuardEvidenceUnknown("required host/process read failed")
    cached = {tuple(command): response for command, response in zip(commands, responses)}

    def read(command):
        response = cached[tuple(command)]
        if response["returncode"]:
            raise RuntimeError("read-only command failed")
        return response["stdout"]

    check, route_value = connection_functions()
    observed_interface = route_value(responses[1]["stdout"], "interface")
    if observed_interface and observed_interface != iface:
        raise SafetyAbort("project route/interface changed")
    default_interface = route_value(responses[7]["stdout"], "interface")
    if default_interface and default_interface != guard["connection_expected"]["default_interface"]:
        raise SafetyAbort("separate LAN default route changed")
    arp = re.search(r" at ([0-9a-f:]+) ", responses[4]["stdout"], re.I)
    if arp:
        observed_mac = ":".join(f"{int(part, 16):02x}" for part in arp.group(1).split(":"))
        if observed_mac != guard["connection_expected"]["gateway_mac"].lower():
            raise SafetyAbort("project gateway MAC changed")
    try:
        connection = check(json.loads(Path(guard["connection_config"]["file"]).read_text()), read)
    except (ValueError, RuntimeError, KeyError) as error:
        raise GuardEvidenceUnknown("host connection binding unproven: " + str(error)) from None
    if any(connection.get(key) != value for key, value in guard["connection_expected"].items()):
        raise SafetyAbort("host DHCP/alias/route binding changed")
    if config.get("preflight", {}).get("host_wifi"):
        from project_tools.tx1_association import host_wifi_mac
        before = config["preflight"]["host_wifi"]
        # The existing inventory contains ifconfig -a; isolate only the bound
        # Wi-Fi interface before checking the actual MAC against preflight.
        match = re.search(r"(?ms)^" + re.escape(iface) + r":.*?(?=^\S[^\n]*:|\Z)", responses[5]["stdout"])
        if match is None:
            raise GuardEvidenceUnknown("bound Wi-Fi interface absent from current inventory")
        observed_end = time.monotonic_ns()
        current_host = {"interface": iface, "begin_mono_ns": observed_begin,
                        "end_mono_ns": observed_end,
                        "hardware_ports_stdout": responses[0]["stdout"], "ifconfig_stdout": match[0]}
        wifi_file = out / f"guard-host-wifi-{index}.private.json"
        existing.save(wifi_file, current_host)
        wifi_evidence = {"host_wifi_file": str(wifi_file), "host_wifi_sha256": digest(wifi_file)}
        try:
            boundary = time.monotonic_ns()
            if host_wifi_mac(current_host, iface, boundary) != host_wifi_mac(before, iface, before["end_mono_ns"]):
                raise SafetyAbort("actual project Wi-Fi MAC changed")
        except (KeyError, TypeError, ValueError):
            raise GuardEvidenceUnknown("actual project Wi-Fi MAC binding unproven") from None
    for response in responses[8:11]:
        pids = {int(match) for match in re.findall(r"^p(\d+)$", response["stdout"], re.M)}
        if pids != {guard["pid"]}:
            raise SafetyAbort("server listener ownership changed")
    try:
        frozen = json.loads(Path(guard["frozen_process"]["file"]).read_text())
        current = json.loads(process_file.read_text())
        keys = ("pid", "argv", "executable", "cwd", "sha256", "bytes", "process")
        if any(current[key] != frozen[key] for key in keys) or current["pid"] != guard["pid"]:
            raise SafetyAbort("server executable/argv/start binding changed")
    except (OSError, ValueError, KeyError):
        raise GuardEvidenceUnknown("server process read incomplete") from None
    return {"connection": connection, "process_binding": "MATCH",
            "process_file": str(process_file), "process_sha256": digest(process_file), **wifi_evidence}


async def host_guard_loop(config, recorder, epoch, plan):
    final_second = sample_count(plan) - 1
    for index in range(final_second // GUARD_PERIOD_SECONDS + 1):
        due = epoch + index * GUARD_PERIOD_SECONDS * 1_000_000_000
        await asyncio.sleep(max(0, (due - time.monotonic_ns()) / 1e9))
        if time.monotonic_ns() >= due + GUARD_PERIOD_SECONDS * 1_000_000_000:
            raise GuardEvidenceUnknown("required host guard deadline missed")
        event = {"index": index, "due_mono_ns": due, "begin_mono_ns": time.monotonic_ns(),
                 "status": "RUNNING"}
        recorder.guard_events.append(event)
        try:
            event.update(await host_guard_once(config, recorder.out, index), status="PASS")
        except asyncio.CancelledError:
            event["status"] = "CANCELLED"
            raise
        except Exception as error:
            event.update(status="FAIL", error=type(error).__name__ + ": " + str(error))
            raise
        finally:
            event["end_mono_ns"] = time.monotonic_ns()


async def observe(config, process, recorder, epoch, slots, skipped, plan):
    """Watch our capture and frozen files while independent schedules run."""
    state = bound_file_state(config)

    async def monitor():
        while True:
            if process.returncode is not None:
                raise SafetyAbort("own capture exited during observation")
            if time.time_ns() > config["authorization"]["expires_wall_ns"]:
                raise SafetyAbort("authorization interval exhausted")
            for path, (expected, old_stat) in list(state.items()):
                new_stat = file_state(path)
                if new_stat != old_stat:
                    if digest(path) != expected:
                        raise SafetyAbort("frozen tool/criteria/setup binding changed")
                    state[path] = (expected, new_stat)
            await asyncio.sleep(0.1)

    schedules = asyncio.create_task(existing.run_schedules(config["apis"], recorder, epoch, slots,
                                                          skipped, sample_count(plan), PERIOD_SECONDS))
    watcher = asyncio.create_task(monitor())
    host_guard = asyncio.create_task(host_guard_loop(config, recorder, epoch, plan))
    try:
        while not (schedules.done() and host_guard.done()):
            active = [task for task in (schedules, watcher, host_guard) if not task.done()]
            done, _ = await asyncio.wait(active, return_when=asyncio.FIRST_COMPLETED)
            for task in done:
                await task
    finally:
        for task in (schedules, watcher, host_guard):
            if not task.done():
                task.cancel()
        await asyncio.gather(schedules, watcher, host_guard, return_exceptions=True)


async def collect(config, out):
    validate_config(config)
    if not out.resolve().is_relative_to(PROJECT / "private"):
        raise ValueError("raw evidence must remain beneath project/private")
    plan = PLANS[config["mode"]]
    os.umask(0o077)
    out.mkdir(mode=0o700)  # Exclusive one-shot claim persists on failure/cancellation.
    # Readiness started with the fresh live preflight, not with this function.
    # Older prepared configs cannot silently start a fresh readiness budget.
    started = config["preflight"]["begin_mono_ns"]
    command = capture_command(config, out / "capture.private.pcap")
    existing.save(out / "binding.private.json", {"schema": SCHEMA, "config": config,
                  "plan": plan, "command": command, "period_seconds": PERIOD_SECONDS,
                  "samples_per_endpoint": sample_count(plan), "configuration_writes": 0})
    proof = {"schema": coverage.CONCURRENT_SCHEMA, "clock_info": coverage.clock_info(),
             "clock_pairs": [coverage.stamp()], "timestamp_type": "host",
             "timestamp_type_confirmed": False}
    process, error, cleanup, epoch = None, None, None, None
    slots, skipped = [], []
    pcap, stderr = out / "capture.private.pcap", out / "capture.stderr.private.log"
    with (out / "requests.private.jsonl").open("x") as stream, stderr.open("x") as log:
        recorder = Requests(out, stream, config["identity_checks"], config["counter_checks"],
                            preparation.field_contract(config), config.get("request_headers"))
        try:
            # Cleanup belongs to the total scope, rather than extending it.
            remaining_scope = plan["scope_seconds"] - CLEANUP_SECONDS - (time.monotonic_ns() - started) / 1e9
            if remaining_scope <= 0:
                raise SafetyAbort("scope budget exhausted before capture start")
            async with asyncio.timeout(remaining_scope):
                ready_remaining = (plan["readiness_seconds"] - ROLL_SECONDS
                                   - (time.monotonic_ns() - started) / 1e9)
                if ready_remaining <= 0:
                    raise SafetyAbort("readiness budget exhausted before capture start")
                async with asyncio.timeout(ready_remaining):
                    process = await asyncio.create_subprocess_exec(*command, stdout=subprocess.DEVNULL, stderr=log)
                    proof["ready_mono_ns"] = await ready_capture(process, pcap, stderr, ready_remaining)
                proof["timestamp_type_confirmed"] = True
                await asyncio.sleep(ROLL_SECONDS)
                epoch = time.monotonic_ns()
                proof["readiness_elapsed_ns"] = epoch - started
                if proof["readiness_elapsed_ns"] > plan["readiness_seconds"] * 1_000_000_000:
                    raise SafetyAbort("readiness budget exhausted")
                await observe(config, process, recorder, epoch, slots, skipped, plan)
                # Final HTTP response completion is part of the bounded closing phase.
                closing_deadline_ns = epoch + (sample_count(plan) - 1 + plan["closing_seconds"]
                                               - CLEANUP_SECONDS) * 1_000_000_000
                remaining = (closing_deadline_ns - time.monotonic_ns()) / 1e9
                if remaining <= 0:
                    raise SafetyAbort("closing budget exhausted")
                async with asyncio.timeout(remaining):
                    await post_roll(process, pcap, max(row["end"]["wall_ns"] for row in recorder.rows))
        except (Exception, asyncio.CancelledError) as exc:
            error = type(exc).__name__ + ": " + str(exc)
        finally:
            proof["stop_requested_mono_ns"] = time.monotonic_ns()
            cleanup_start = time.monotonic_ns()
            try:
                async with asyncio.timeout(CLEANUP_SECONDS):
                    if process is not None:
                        cleanup = await existing.stop_process(process)
            except (Exception, asyncio.CancelledError) as exc:
                error = (error or "") + "; cleanup: " + type(exc).__name__
            proof["clock_pairs"].append(coverage.stamp())
            proof["cleanup_elapsed_ns"] = time.monotonic_ns() - cleanup_start
            proof["scope_elapsed_ns"] = time.monotonic_ns() - started
    if epoch is not None:
        for row in recorder.rows:
            annotate(row, plan, epoch)
    expected = [f"{i}:{key}" for i in range(sample_count(plan)) for key in config["apis"]]
    proof.update(clock_events=recorder.events, expected_request_ids=expected,
                 host_guard_events=recorder.guard_events,
                 planned_slot_ids=expected, completed_slot_ids=slots, skipped_slots=skipped,
                 exit_code=process.returncode if process is not None else None,
                 clean_exit=bool(cleanup and cleanup["exit_code"] == 0 and not cleanup["killed"]))
    text = stderr.read_text()
    drops = re.search(r"(\d+) packets dropped by kernel", text)
    proof["kernel_drops"] = int(drops[1]) if drops else None
    proof["timestamp_type_confirmed"] &= "warning" not in text.lower()
    proof["pcap_sha256"] = digest(pcap) if pcap.exists() else None
    existing.save(out / "proof.private.json", proof)
    attempted = {row["request_id"] for row in recorder.rows}
    result = {"schema": SCHEMA, "mode": config["mode"], "status": "CAPTURE_EVIDENCE_FAIL",
              "error": error, "attempted": len(recorder.rows), "planned": len(expected),
              "skipped": len(skipped), "unattempted_slot_ids": sorted(set(expected) - attempted),
              "scope_within_budget": proof["scope_elapsed_ns"] <= plan["scope_seconds"] * 1_000_000_000,
              "cleanup_within_budget": proof["cleanup_elapsed_ns"] <= CLEANUP_SECONDS * 1_000_000_000,
              "configuration_writes": 0, "functional_acceptance": False}
    try:
        result["coverage"] = coverage.assess(recorder.rows, coverage.pcap_span(pcap.read_bytes()), proof, expected)
        if (error is None and result["scope_within_budget"] and result["cleanup_within_budget"]
                and result["coverage"]["status"] == "COVERAGE_PASS"
                and result["coverage"]["all_queries_succeeded"]):
            result["status"] = "CAPTURE_EVIDENCE_PASS_NOT_FUNCTIONAL_ACCEPTANCE"
    except (OSError, ValueError) as exc:
        result["coverage_error"] = str(exc)
    run = {"schema": SCHEMA, "mode": config["mode"], "epoch_mono_ns": epoch,
           "interface": config["interface"], "host_wifi": config["preflight"].get("host_wifi"),
           "preflight_wire_binding": config["preflight"].get("wire_binding"),
           "windows": windows(plan, epoch) if epoch is not None else [],
           "rows": recorder.rows, "proof": proof, "result": result}
    existing.save(out / "run.private.json", run)
    existing.save(out / "result.private.json", result)
    return result


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--config", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--execute", action="store_true")
    args = parser.parse_args()
    if not args.execute:
        parser.error("explicit --execute plus bound one-shot authorization required")
    result = asyncio.run(collect(json.loads(args.config.read_text()), args.output.resolve()))
    print(json.dumps({key: result[key] for key in ("status", "attempted", "planned", "skipped", "error")}))
    raise SystemExit(0 if result["status"].startswith("CAPTURE_EVIDENCE_PASS") else 2)

"""Prepared, bounded GET/passive-capture collector. Never runs on import.

A future explicit approval and fresh preflight are required. The template
deliberately cannot authorize an execution. This tool cannot configure devices.
"""
lazy import argparse
lazy import hashlib
lazy import ipaddress
lazy import json
lazy import os
lazy from pathlib import Path
lazy import re
lazy import signal
lazy import subprocess
lazy import time
lazy import urllib.error
lazy import urllib.parse
lazy import urllib.request

if __package__:
    lazy from . import tx7b_coverage as coverage
else:
    lazy import tx7b_coverage as coverage

SAMPLES = 62
SCOPE_SECONDS = 90
FILTER = "src net 192.168.4.0/24 and udp and (dst port 5005 or dst port 4210 or dst port 55010)"
API_KEYS = {"nodes", "rx4_status", "metrics", "mmwave", "MMWAVE1", "MMWAVE2", "tx", "ready"}


class ScopeExpired(BaseException):
    """The global deadline must escape per-request Exception handling."""


def timed_get(url, request_id, clock=time, opener=None):
    record = {"request_id": request_id, "endpoint": url, "begin": coverage.stamp(clock),
              "http_status": None, "error": None, "response_text": None, "value": None,
              "body_complete": False}
    try:
        if opener is None:
            opener = urllib.request.build_opener(urllib.request.ProxyHandler({})).open
        try:
            response = opener(url, timeout=2)
        except urllib.error.HTTPError as error:
            response = error  # Preserve failed HTTP response bodies and their boundaries too.
        with response:
            record["http_status"] = response.status
            record["response_text"] = response.read().decode("utf-8")
            record["body_complete"] = True
            record["value"] = json.loads(record["response_text"])
            if type(record["value"]) is not dict:
                record["error"] = "JSON response is not an object"
            if response.status != 200:
                record["error"] = f"HTTP {response.status}"
    except Exception as error:
        record["error"] = type(error).__name__ + ": " + str(error)
    finally:
        record["end"] = coverage.stamp(clock)
    return record


def validate_config(config):
    approval = config.get("authorization")
    if not isinstance(approval, dict) or approval.get("scope") != "TX7B_READ_ONLY_CAPTURE":
        raise ValueError("fresh explicit capture authorization required; template is not approval")
    now = time.time_ns()
    if not approval.get("user_request_sha256") or not approval["issued_wall_ns"] <= now <= approval["expires_wall_ns"]:
        raise ValueError("missing/expired approval binding")
    preflight = config["preflight"]
    if preflight["status"] != "PASS_FRESH_READ_ONLY_PREFLIGHT" or not 0 <= now - preflight["wall_ns"] <= 120_000_000_000:
        raise ValueError("fresh preflight within 120 seconds required")
    if not re.fullmatch(r"en[0-9]+", config["interface"]) or set(config["apis"]) != API_KEYS:
        raise ValueError("unexpected interface/API inventory")
    for url in config["apis"].values():
        parsed = urllib.parse.urlsplit(url)
        address = ipaddress.ip_address(parsed.hostname)
        if parsed.scheme != "http" or parsed.username or parsed.password or parsed.fragment:
            raise ValueError("plain authenticated-context-free GET endpoint required")
        if not (address.is_loopback or address in ipaddress.ip_network("192.168.4.0/24")):
            raise ValueError("endpoint outside the approved project network")


def save(path, value):
    with path.open("x") as stream:
        json.dump(value, stream, sort_keys=True, indent=2)


def collect(config, out):
    validate_config(config)  # Must precede capture creation and every live action.
    os.umask(0o077)
    out.mkdir(mode=0o700)
    save(out / "binding.private.json", {"config": config, "schema": coverage.SCHEMA,
        "max_scope_seconds": SCOPE_SECONDS, "samples": SAMPLES, "filter": FILTER,
        "configuration_writes": 0, "live_boot_or_system_acceptance": False})
    pcap = out / "capture.private.pcap"
    stderr = out / "capture.stderr.private.log"
    command = ["/usr/sbin/tcpdump", "-i", config["interface"], "-j", "host",
               "-p", "-n", "-s", "0", "-U", "-w", str(pcap), FILTER]
    expected = [f"{i}:{key}" for i in range(SAMPLES) for key in config["apis"]]
    proof = {"schema": coverage.SCHEMA, "clock_info": coverage.clock_info(),
             "clock_pairs": [coverage.stamp()], "command": command,
             "timestamp_type": "host", "timestamp_type_confirmed": False,
             "expected_request_ids": expected}
    rows, error, process, killed = [], None, None, False

    def expire(*_):
        raise ScopeExpired("90-second scope exhausted")

    old_handler = signal.signal(signal.SIGALRM, expire)
    signal.alarm(SCOPE_SECONDS)
    try:
        with stderr.open("x") as log:
            process = subprocess.Popen(command, stdout=subprocess.DEVNULL, stderr=log)
            deadline = time.monotonic_ns() + 10_000_000_000
            while time.monotonic_ns() < deadline:
                if process.poll() is not None:
                    raise RuntimeError("capture exited before readiness")
                text = stderr.read_text()
                if "warning" in text.lower():
                    raise RuntimeError("capture warning; host timestamp type cannot be assumed")
                try:
                    span = coverage.pcap_span(pcap.read_bytes())
                    if "listening on" in text:
                        break
                except (OSError, ValueError):
                    pass
                time.sleep(0.05)
            else:
                raise TimeoutError("capture has no proven readiness")
            proof["ready_mono_ns"] = time.monotonic_ns()
            proof["timestamp_type_confirmed"] = True
            while time.monotonic_ns() < proof["ready_mono_ns"] + 2_100_000_000:
                time.sleep(0.05)
            start = time.monotonic_ns()
            with (out / "samples.private.jsonl").open("x") as stream:
                for i in range(SAMPLES):
                    remaining = start + i * 1_000_000_000 - time.monotonic_ns()
                    if remaining > 0:
                        time.sleep(remaining / 1e9)
                    if process.poll() is not None:
                        raise RuntimeError("capture exited during APIs")
                    row = {"index": i, "mono_ns": time.monotonic_ns(), "apis": {}}
                    for key, url in config["apis"].items():
                        row["apis"][key] = timed_get(url, f"{i}:{key}")
                    rows.append(row)
                    stream.write(json.dumps(row, sort_keys=True) + "\n")
                    stream.flush()
            last = max(r["end"]["wall_ns"] for r in coverage.requests(rows))
            while True:
                if process.poll() is not None:
                    raise RuntimeError("capture exited before post-roll")
                try:
                    span = coverage.pcap_span(pcap.read_bytes())
                    if span["last_ns"] >= last + 2_100_000_000:
                        break
                except (OSError, ValueError):
                    pass
                time.sleep(0.05)
    except (Exception, ScopeExpired, KeyboardInterrupt) as exc:
        error = type(exc).__name__ + ": " + str(exc)
    finally:
        signal.alarm(0)
        proof["stop_requested_mono_ns"] = time.monotonic_ns()
        if process is not None and process.poll() is None:
            process.send_signal(signal.SIGINT)
            try:
                process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                process.kill()
                killed = True
                process.wait(timeout=2)
        signal.signal(signal.SIGALRM, old_handler)
        proof["clock_pairs"].append(coverage.stamp())
    proof["exit_code"] = process.returncode if process is not None else None
    proof["clean_exit"] = proof["exit_code"] == 0 and not killed
    text = stderr.read_text() if stderr.exists() else ""
    if "warning" in text.lower():
        proof["timestamp_type_confirmed"] = False
    drops = re.search(r"(\d+) packets dropped by kernel", text)
    proof["kernel_drops"] = int(drops[1]) if drops else None
    proof["pcap_sha256"] = hashlib.sha256(pcap.read_bytes()).hexdigest() if pcap.exists() else None
    save(out / "proof.private.json", proof)
    result = {"status": "CAPTURE_EVIDENCE_FAIL", "error": error, "samples": len(rows),
              "system_acceptance": False, "configuration_writes": 0}
    try:
        assessment = coverage.assess(rows, coverage.pcap_span(pcap.read_bytes()), proof, expected)
        result["coverage"] = assessment
        if error is None and assessment["status"] == "COVERAGE_PASS" and assessment["all_queries_succeeded"]:
            result["status"] = "CAPTURE_EVIDENCE_PASS_NOT_FUNCTIONAL_ACCEPTANCE"
    except (OSError, ValueError) as exc:
        result["assessment_error"] = str(exc)
    save(out / "result.private.json", result)
    return result


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--config", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--execute", action="store_true")
    args = parser.parse_args()
    if not args.execute:
        parser.error("no execution without --execute and fresh approval/preflight")
    project = Path(__file__).resolve().parents[1]
    target = args.output.resolve()
    if not target.is_relative_to(project / "private"):
        parser.error("raw capture output must remain beneath project/private")
    result = collect(json.loads(args.config.read_text()), target)
    print(json.dumps({k: result[k] for k in ("status", "samples", "system_acceptance")}))
    raise SystemExit(0 if result["status"].startswith("CAPTURE_EVIDENCE_PASS") else 2)

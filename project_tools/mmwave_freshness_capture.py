"""One bounded, independently scheduled read-only radar/RX4 diagnostic capture.

No I/O on import. Explicit authorization, fresh preflight and source hashes are
required by the CLI. Existing v2 clock/margin limits also apply to v3 concurrency.
"""
lazy import argparse
lazy import asyncio
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
    lazy from . import tx7b_coverage as coverage
else:
    lazy import tx7b_coverage as coverage

SAMPLES = 61
PERIOD = 1.05
SCOPE_SECONDS = 90
CLEANUP_SECONDS = 6
HTTP_SECONDS = 2
ROLL_SECONDS = 2.1
API_KEYS = {"MMWAVE1", "MMWAVE2", "RX4", "nodes", "mmwave"}


def save(path, value):
    with path.open("x") as stream:
        json.dump(value, stream, indent=2, sort_keys=True)


async def stop_process(process, grace=3, kill_timeout=1):
    """Only stop our child; bounded escalation and reaping are part of evidence."""
    killed = False
    if process.returncode is None:
        process.send_signal(signal.SIGINT)
        try:
            await asyncio.wait_for(process.wait(), grace)
        except asyncio.TimeoutError:
            process.kill()
            killed = True
            await asyncio.wait_for(process.wait(), kill_timeout)
    return {"exit_code": process.returncode, "killed": killed}


class Requests:
    def __init__(self, out, stream, headers=None):
        self.out, self.stream = out, stream
        self.rows, self.events, self.children = [], [], set()
        self.headers = headers or {}

    def boundary(self, request_id, edge):
        # No await between the three reads and append: global event order is real.
        stamp = coverage.stamp()
        self.events.append({"ordinal": len(self.events), "request_id": request_id,
                            "boundary": edge, "stamp": stamp})
        return stamp

    async def get(self, key, url, index, due_ns):
        request_id = f"{index}:{key}"
        body = self.out / f"response-{index}-{key}.private.body"
        row = {"request_id": request_id, "endpoint": url, "endpoint_key": key,
               "slot": index, "due_mono_ns": due_ns, "begin": self.boundary(request_id, "begin"),
               "http_status": None, "error": None, "response_text": None, "value": None,
               "body_complete": False, "curl_exit_code": None, "timed_out": False}
        row["process_start_begin_mono_ns"] = time.monotonic_ns()
        row["process_started_mono_ns"] = None
        row["curl_timings"] = None
        process, cancelled = None, False
        try:
            header = self.headers.get(key)
            header_args = ["--header", "@" + header["file"]] if header else []
            process = await asyncio.create_subprocess_exec(
                "/usr/bin/curl", "--noproxy", "*", "--silent", "--show-error",
                "--connect-timeout", "1", "--max-time", str(HTTP_SECONDS),
                "--max-filesize", "1048576", "--request", "GET", "--output", str(body),
                "--write-out", "%{http_code}\n%{time_namelookup}\n%{time_connect}\n%{time_starttransfer}\n%{time_total}\n",
                *header_args, url,
                stdout=asyncio.subprocess.PIPE, stderr=asyncio.subprocess.PIPE)
            row["process_started_mono_ns"] = time.monotonic_ns()
            self.children.add(process)
            stdout, stderr = await asyncio.wait_for(process.communicate(), HTTP_SECONDS + 1)
            row["curl_exit_code"] = process.returncode
            row["timed_out"] = process.returncode == 28
            output = stdout.decode().splitlines()
            code = output[0].strip() if output else ""
            row["http_status"] = int(code) if code.isdigit() and code != "000" else None
            # Preserve every curl-reported duration as its original decimal.
            # These diagnose phases; the host clock boundaries remain the proof.
            if len(output) == 5 and all(re.fullmatch(r"[0-9]+\.[0-9]+", item) for item in output[1:]):
                row["curl_timings"] = dict(zip(("namelookup_seconds", "connect_seconds",
                                                "starttransfer_seconds", "total_seconds"), output[1:]))
            row["body_complete"] = process.returncode == 0
            if process.returncode:
                row["error"] = f"curl exit {process.returncode}: " + stderr.decode(errors="replace")
            elif row["http_status"] != 200:
                row["error"] = f"HTTP {row['http_status']}"
        except asyncio.CancelledError:
            row["error"], cancelled = "global deadline/cancellation", True
        except Exception as error:
            row["error"] = type(error).__name__ + ": " + str(error)
            row["timed_out"] = isinstance(error, asyncio.TimeoutError)
        finally:
            if process is not None:
                if process.returncode is None:
                    process.kill()
                    await asyncio.wait_for(process.wait(), 1)
                self.children.discard(process)
            row["end"] = self.boundary(request_id, "end")
            if body.exists():
                row["response_text"] = body.read_text(errors="replace")
                if row["body_complete"]:
                    try:
                        row["value"] = json.loads(row["response_text"])
                        if type(row["value"]) is not dict:
                            row["error"] = "JSON response is not an object"
                    except ValueError as error:
                        row["error"] = "invalid JSON: " + str(error)
            self.rows.append(row)
            self.stream.write(json.dumps(row, sort_keys=True) + "\n")
            self.stream.flush()
        if cancelled:
            raise asyncio.CancelledError
        return row


async def endpoint_schedule(key, url, epoch_ns, requester, slots, skipped,
                            samples=SAMPLES, period=PERIOD):
    """Fixed deadlines; skip elapsed slots instead of issuing catch-up requests."""
    previous_end = None
    step = round(period * 1e9)
    for index in range(samples):
        due = epoch_ns + index * step
        if previous_end is not None and previous_end > due:
            skipped.append({"slot_id": f"{index}:{key}", "reason": "previous request crossed deadline",
                            "due_mono_ns": due, "observed_mono_ns": time.monotonic_ns()})
            continue
        await asyncio.sleep(max(0, (due - time.monotonic_ns()) / 1e9))
        if time.monotonic_ns() >= due + step:
            skipped.append({"slot_id": f"{index}:{key}", "reason": "scheduler missed complete interval",
                            "due_mono_ns": due, "observed_mono_ns": time.monotonic_ns()})
            continue
        row = await requester(key, url, index, due)
        slots.append(row["request_id"])
        previous_end = row["end"]["mono_after_ns"]


async def run_schedules(apis, recorder, epoch_ns, slots, skipped,
                        samples=SAMPLES, period=PERIOD):
    tasks = [asyncio.create_task(endpoint_schedule(key, url, epoch_ns, recorder.get,
             slots, skipped, samples, period)) for key, url in apis.items()]
    try:
        await asyncio.gather(*tasks)
    finally:
        for task in tasks:
            if not task.done():
                task.cancel()
        await asyncio.gather(*tasks, return_exceptions=True)


def validate_config(config):
    auth, pre = config["authorization"], config["preflight"]
    now = time.time_ns()
    if (auth["scope"] != "MMWAVE2_FRESHNESS_ONE_PASSIVE_WINDOW"
            or auth["max_windows"] != 1 or not auth["issued_wall_ns"] <= now <= auth["expires_wall_ns"]):
        raise ValueError("fresh task-specific single-window authorization required")
    if hashlib.sha256(Path(auth["file"]).read_bytes()).hexdigest() != auth["sha256"]:
        raise ValueError("authorization binding differs")
    if (pre["status"] != "PASS" or not 0 <= now - pre["wall_ns"] <= 120_000_000_000
            or hashlib.sha256(Path(pre["file"]).read_bytes()).hexdigest() != pre["sha256"]):
        raise ValueError("fresh bound preflight required")
    for path, digest in config["tool_sha256"].items():
        if hashlib.sha256(Path(path).read_bytes()).hexdigest() != digest:
            raise ValueError("tested tool binding differs")
    required = {str(Path(__file__).resolve()), str(Path(coverage.__file__).resolve())}
    if not required <= set(config["tool_sha256"]):
        raise ValueError("caller and clock evaluator must be bound")
    if set(config["apis"]) != API_KEYS or not re.fullmatch(r"en[0-9]+", config["interface"]):
        raise ValueError("unexpected endpoint/interface inventory")
    for key, url in config["apis"].items():
        parsed = urllib.parse.urlsplit(url)
        address = ipaddress.ip_address(parsed.hostname)
        route = {"nodes": "/api/v1/nodes", "mmwave": "/api/v1/mmwave/status"}.get(key, "/ota/status")
        if parsed.scheme != "http" or parsed.username or parsed.password or parsed.query or parsed.fragment:
            raise ValueError("numeric, uncredentialed GET endpoint required")
        if parsed.path != route:
            raise ValueError("unexpected diagnostic GET route")
        if key in ("nodes", "mmwave"):
            if not address.is_loopback or parsed.port != 8080:
                raise ValueError("server must be existing local diagnostic instance")
        elif address not in ipaddress.ip_network("192.168.4.0/24") or parsed.port != 8032:
            raise ValueError("device endpoint outside project network")
    if config["interface"] != config["bound_interface"]:
        raise ValueError("interface differs from preflight")
    saved = json.loads(Path(pre["file"]).read_text())
    if (saved["status"] != "PASS" or saved["apis"] != config["apis"]
            or saved["interface"] != config["interface"]):
        raise ValueError("preflight endpoint/interface binding differs")


def capture_command(config, pcap):
    hosts = {k: urllib.parse.urlsplit(config["apis"][k]).hostname for k in ("MMWAVE1", "MMWAVE2", "RX4")}
    expression = (f"udp and ((port 55010 and (host {hosts['MMWAVE1']} or host {hosts['MMWAVE2']})) "
                  f"or (src host {hosts['RX4']} and dst port 5005))")
    return ["/usr/sbin/tcpdump", "-i", config["interface"], "-j", "host", "-p", "-n",
            "-s", "0", "-U", "-w", str(pcap), expression]


async def collect(config, out):
    validate_config(config)
    os.umask(0o077)
    out.mkdir(mode=0o700)  # Exclusive one-shot claim, including failed attempts.
    command = capture_command(config, out / "capture.private.pcap")
    save(out / "binding.private.json", {"config": config, "command": command,
         "samples_per_endpoint": SAMPLES, "period_seconds": PERIOD,
         "scope_seconds": SCOPE_SECONDS, "cleanup_seconds": CLEANUP_SECONDS})
    proof = {"schema": coverage.CONCURRENT_SCHEMA, "clock_info": coverage.clock_info(),
             "clock_pairs": [coverage.stamp()], "timestamp_type": "host",
             "timestamp_type_confirmed": False}
    process, error, cleanup = None, None, None
    slots, skipped = [], []
    pcap, stderr = out / "capture.private.pcap", out / "capture.stderr.private.log"
    with (out / "requests.private.jsonl").open("x") as stream, stderr.open("x") as log:
        recorder = Requests(out, stream)
        started = time.monotonic_ns()
        try:
            # Reserve two seconds inside the 90-second scope for cancelling and
            # reaping HTTP children before the separately bounded capture cleanup.
            async with asyncio.timeout(SCOPE_SECONDS - 2):
                process = await asyncio.create_subprocess_exec(*command, stdout=subprocess.DEVNULL, stderr=log)
                ready_deadline = time.monotonic_ns() + 10_000_000_000
                while time.monotonic_ns() < ready_deadline:
                    if process.returncode is not None:
                        raise RuntimeError("capture exited before readiness")
                    text = stderr.read_text()
                    if "warning" in text.lower():
                        raise RuntimeError("capture timestamp warning")
                    try:
                        span = coverage.pcap_span(pcap.read_bytes())
                        if "listening on" in text:
                            break
                    except (OSError, ValueError):
                        pass
                    await asyncio.sleep(0.05)
                else:
                    raise TimeoutError("capture readiness unproven")
                proof["ready_mono_ns"] = time.monotonic_ns()
                proof["timestamp_type_confirmed"] = True
                await asyncio.sleep(ROLL_SECONDS)
                epoch = time.monotonic_ns()
                await run_schedules(config["apis"], recorder, epoch, slots, skipped)
                # Keep actual packet timestamps, not a sleep/drop-count inference.
                latest_end = max(row["end"]["wall_ns"] for row in recorder.rows)
                while True:
                    if process.returncode is not None:
                        raise RuntimeError("capture exited before post-roll")
                    try:
                        if coverage.pcap_span(pcap.read_bytes())["last_ns"] >= latest_end + round(ROLL_SECONDS * 1e9):
                            break
                    except (OSError, ValueError):
                        pass
                    await asyncio.sleep(0.05)
        except (Exception, asyncio.CancelledError) as exc:
            error = type(exc).__name__ + ": " + str(exc)
        finally:
            proof["stop_requested_mono_ns"] = time.monotonic_ns()
            cleanup_start = time.monotonic_ns()
            try:
                async with asyncio.timeout(CLEANUP_SECONDS):
                    if process is not None:
                        cleanup = await stop_process(process)
            except Exception as exc:
                error = (error or "") + "; cleanup: " + type(exc).__name__
            proof["clock_pairs"].append(coverage.stamp())
            proof["scope_elapsed_ns"] = proof["stop_requested_mono_ns"] - started
            proof["cleanup_elapsed_ns"] = time.monotonic_ns() - cleanup_start
    proof.update(clock_events=recorder.events, expected_request_ids=[r["request_id"] for r in recorder.rows],
                 planned_slot_ids=[f"{i}:{k}" for i in range(SAMPLES) for k in config["apis"]],
                 completed_slot_ids=slots, skipped_slots=skipped,
                 exit_code=process.returncode if process is not None else None,
                 clean_exit=bool(cleanup and cleanup["exit_code"] == 0 and not cleanup["killed"]))
    text = stderr.read_text()
    drops = re.search(r"(\d+) packets dropped by kernel", text)
    proof["kernel_drops"] = int(drops[1]) if drops else None
    proof["timestamp_type_confirmed"] &= "warning" not in text.lower()
    proof["pcap_sha256"] = hashlib.sha256(pcap.read_bytes()).hexdigest() if pcap.exists() else None
    save(out / "proof.private.json", proof)
    result = {"status": "DIAGNOSTIC_RECORDED_NOT_FUNCTIONAL_ACCEPTANCE", "error": error,
              "attempted": len(recorder.rows), "planned": SAMPLES * len(config["apis"]),
              "skipped": len(skipped), "live_windows_started": 1, "configuration_writes": 0,
              "scope_elapsed_ns": proof["scope_elapsed_ns"], "cleanup_elapsed_ns": proof["cleanup_elapsed_ns"]}
    attempted = {r["request_id"] for r in recorder.rows}
    result["unattempted_slot_ids"] = sorted(set(proof["planned_slot_ids"]) - attempted)
    result["scope_within_budget"] = proof["scope_elapsed_ns"] <= SCOPE_SECONDS * 1_000_000_000
    result["cleanup_within_budget"] = proof["cleanup_elapsed_ns"] <= CLEANUP_SECONDS * 1_000_000_000
    try:
        result["coverage"] = coverage.assess(recorder.rows, coverage.pcap_span(pcap.read_bytes()), proof,
                                               proof["expected_request_ids"])
    except (OSError, ValueError) as exc:
        result["coverage_error"] = str(exc)
    save(out / "result.private.json", result)
    return result


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--config", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--execute", action="store_true")
    args = parser.parse_args()
    if not args.execute or not args.output.resolve().is_relative_to(Path(__file__).resolve().parents[1] / "private"):
        parser.error("explicit --execute and a new private output directory required")
    result = asyncio.run(collect(json.loads(args.config.read_text()), args.output.resolve()))
    print(json.dumps({k: result[k] for k in ("status", "attempted", "planned", "skipped", "error")}))

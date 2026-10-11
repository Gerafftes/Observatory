"""Offline preparation of the TX1 measurement contract; never opens a socket.

Known cumulative counters are selected by meaning, not by a JSON value's type.
The prepared paths/types and executable sources are frozen before readiness.
Fresh boot values are bound by preflight, never rebound inside a running scope.
"""
lazy import argparse
lazy import hashlib
lazy import json
lazy from pathlib import Path

lazy from project_tools import tx1_transport_comparison as evaluator

SCHEMA = "tx1.measurement-preparation.v1"


def sha256(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def value_at(value, path):
    for component in path:
        value = value[component]
    return value


def same_value(actual, expected):
    """JSON equality must not confuse true with 1, including nested objects."""
    if type(actual) is not type(expected):
        return False
    if isinstance(actual, dict):
        return actual.keys() == expected.keys() and all(
            same_value(actual[key], expected[key]) for key in actual)
    if isinstance(actual, list):
        return len(actual) == len(expected) and all(
            same_value(a, b) for a, b in zip(actual, expected))
    return actual == expected


def counter_paths():
    paths = [("TX1", (key,)) for key in ("sequence",) + evaluator.TX_COUNTERS]
    for index in range(4):
        paths.extend((f"RX{index + 1}", ("transport", key)) for key in evaluator.RX_COUNTERS)
        paths.append((f"RX{index + 1}", ("sequence",)))
        paths.extend(("nodes", ("nodes", index, key))
                     for key in evaluator.NODE_COUNTERS + ("ip_change_count",))
    for index in range(2):
        paths.extend((f"MMWAVE{index + 1}", ("diagnostics", key))
                     for key in evaluator.RADAR_DIAGNOSTICS)
        paths.extend(("mmwave", ("sensors", index, key))
                     for key in evaluator.RADAR_SERVER_COUNTERS)
    paths.extend(("metrics", ("csi_ingress", "counters", key))
                 for key in evaluator.INGRESS_COUNTERS)
    return paths


def validate_counter_checks(checks, complete=True):
    actual = [(item["endpoint_key"], tuple(item["path"])) for item in checks]
    known = set(counter_paths())
    if len(set(actual)) != len(actual) or not set(actual) <= known:
        raise ValueError("unknown/duplicate cumulative counter mapping")
    if complete and set(actual) != known:
        raise ValueError("complete cumulative counter inventory required")


def field_contract(config):
    """Only explicitly named gauges/booleans can vary outside counter rules."""
    validate_counter_checks(config["counter_checks"])
    fields = [{**item, "kind": "counter", "json_type": "int"}
              for item in config["counter_checks"]]
    for item in config["identity_checks"]:
        kind = "epoch" if item["path"][-1] == "boot_id" else "identity"
        if type(item["expected"]) is bool:
            kind = "boolean"
        fields.append({"endpoint_key": item["endpoint_key"], "path": item["path"],
                       "kind": kind, "json_type": type(item["expected"]).__name__})
    gauges = [("TX1", (key,)) for key in ("stations", "free_heap", "jitter_average_us", "jitter_max_us")]
    gauges += [(f"RX{index + 1}", ("transport", "queue_depth")) for index in range(4)]
    gauges += [("nodes", ("nodes", index, "last_seen_ms")) for index in range(4)]
    gauges += [("mmwave", ("sensors", index, "packet_age_ms")) for index in range(2)]
    # These are durations, queue occupancy, timestamps or balance state, never
    # cumulative packet counts. They remain observations, not reference deltas.
    names = ("csi_processing_current", "csi_processing_duration_avg_us",
             "csi_processing_duration_last_us", "csi_processing_duration_max_us",
             "last_processing_completed_unix_ns", "last_server_receive_monotonic_ns",
             "last_server_receive_unix_ns", "processing_duration_avg_us",
             "processing_duration_last_us", "processing_duration_max_us",
             "udp_queue_age_avg_us", "udp_queue_age_last_us", "udp_queue_age_max_us",
             "udp_queue_depth", "udp_queue_high_watermark", "csi_outcomes_unaccounted")
    gauges += [("metrics", ("csi_ingress", "counters", key)) for key in names]
    fields.extend({"endpoint_key": endpoint, "path": list(path), "kind": "gauge",
                   "json_type": "int"} for endpoint, path in gauges)
    fields.append({"endpoint_key": "metrics", "path": ["csi_ingress", "counters", "csi_balance_closed"],
                   "kind": "boolean", "json_type": "bool"})
    keys = [(item["endpoint_key"], tuple(item["path"])) for item in fields]
    if len(keys) != len(set(keys)):
        raise ValueError("ambiguous field meaning")
    return fields


def validate_fields(row, fields):
    if row["http_status"] != 200 or not row["body_complete"] or row["value"] is None:
        return
    for field in fields:
        if field["endpoint_key"] != row["endpoint_key"]:
            continue
        value = value_at(row["value"], field["path"])
        if type(value).__name__ != field["json_type"]:
            raise ValueError("prepared field type differs: " + field["endpoint_key"])
        if field["kind"] == "counter" and (type(value) is not int or value < 0):
            raise ValueError("invalid prepared cumulative counter")


def validate_setup(config):
    """Bind a newly sealed setup without allowing a stale, hardcoded identity."""
    sealed = json.loads(Path(config["artifacts"]["setup"]["file"]).read_text())
    version = sealed["schema_version"]
    if (type(version) is not int or version not in range(1, 6)
            or sealed["kind"] != "ruview.position-setup"
            or sealed["definition"]["schema_version"] != version):
        raise ValueError("unsupported sealed setup document")
    # Matches position_artifact::deterministic_pretty_json: recursively sorted
    # keys, preserved array order, two spaces and exactly one final newline.
    canonical = (json.dumps(sealed["definition"], sort_keys=True, indent=2,
                            ensure_ascii=False, allow_nan=False) + "\n").encode()
    digest = hashlib.sha256(f"ruview.position-setup.v{version}\0".encode() + canonical).hexdigest()
    expected = {"id": "setup-" + digest[:16], "sha256": digest}
    if (sealed["setup_id"] != expected["id"] or sealed["setup_sha256"] != digest
            or config["setup"] != expected):
        raise ValueError("sealed setup canonical identity/binding differs")
    if not {"tx_build_receipt", "tx_candidate"} <= set(config["artifacts"]):
        raise ValueError("TX candidate and tested build receipt bindings required")
    receipt_binding = config["artifacts"]["tx_build_receipt"]
    receipt_path = Path(receipt_binding["file"]).resolve()
    if sha256(receipt_path) != receipt_binding["sha256"]:
        raise ValueError("TX build receipt binding differs")
    receipt = json.loads(receipt_path.read_text())
    candidate = Path(receipt["candidate"]["file"])
    if not candidate.is_absolute():
        candidate = receipt_path.parent / candidate
    candidate_binding = config["artifacts"]["tx_candidate"]
    firmware = sealed["definition"]["transmitter"]["firmware"]
    if (candidate.resolve() != Path(candidate_binding["file"]).resolve()
            or sha256(candidate) != candidate_binding["sha256"]
            or candidate_binding["sha256"] != receipt["candidate"]["sha256"]
            or firmware["artifact_sha256"] != receipt["candidate"]["sha256"]):
        raise ValueError("sealed TX firmware and actual candidate/build receipt differ")
    runtime = {tuple(check["path"]): check["expected"] for check in config["identity_checks"]
               if check["endpoint_key"] == "TX1"}
    if (runtime.get(("image_sha256",)) != receipt["runtime_image_sha256"]
            or runtime.get(("source_sha256",)) != receipt["source_sha256"]):
        raise ValueError("expected TX runtime image/source and bound build receipt differ")


def projection(config):
    stable = {key: config[key] for key in ("mode", "interface", "apis", "setup",
              "counter_checks", "artifacts", "tool_sha256")}
    stable["identity_checks"] = [{key: value for key, value in item.items()
                                 if key != "expected" or item["path"][-1] != "boot_id"}
                                for item in config["identity_checks"]]
    stable["request_headers"] = config.get("request_headers", {})
    stable["connection_config"] = config["live_guard"]["connection_config"]
    stable["connection_expected"] = config["live_guard"]["connection_expected"]
    return hashlib.sha256(json.dumps(stable, sort_keys=True, separators=(",", ":"),
                                     allow_nan=False).encode()).hexdigest()


def verify_preparation(config):
    bound = config["preparation"]
    if sha256(bound["file"]) != bound["sha256"]:
        raise ValueError("offline preparation binding changed")
    prepared = json.loads(Path(bound["file"]).read_text())
    if (prepared["schema"] != SCHEMA or prepared["status"] != "PASS_OFFLINE_PREPARATION"
            or prepared["configuration_sha256"] != projection(config)
            or prepared["field_contract"] != field_contract(config)):
        raise ValueError("offline prepared contract differs")
    return prepared


def prepare(config, snapshots, sources):
    from project_tools import tx1_comparison_capture as capture
    capture.validate_config(config, live=False)
    fields = field_contract(config)
    previous, seen, replay_identity_checks = {}, set(), []
    replay_identity_keys = set()
    counts = {key: 0 for key in config["apis"]}
    for snapshot in snapshots:
        rows = snapshot.items() if isinstance(snapshot, dict) else [(row["endpoint_key"], row) for row in snapshot]
        for key, original in rows:
            row = dict(original)
            # Original preflight wrappers preceded body_complete/endpoint_key.
            # Establish completeness from the entire retained HTTP body, never
            # from selected fields or invented zero values.
            if "body_complete" not in row:
                row["body_complete"] = (row["error"] is None and row["http_status"] == 200
                                        and row["response_text"] is not None
                                        and same_value(json.loads(row["response_text"]), row["value"]))
            row.setdefault("endpoint_key", key)
            if key not in config["apis"] or row["endpoint"] != config["apis"][key]:
                raise ValueError("saved response endpoint binding differs")
            validate_fields(row, fields)
            if row["http_status"] == 200 and row["body_complete"] and row["value"] is not None:
                # A future candidate's expected firmware/setup differs from its
                # historical shape fixture. Freeze each original replay's own
                # identity once and reject changes inside that replay; only the
                # later fresh preflight proves the new configured identity.
                for check in config["identity_checks"]:
                    identity_key = (check["endpoint_key"], tuple(check["path"]))
                    if check["endpoint_key"] == key and identity_key not in replay_identity_keys:
                        replay_identity_checks.append({**check, "expected": value_at(row["value"], check["path"])})
                        replay_identity_keys.add(identity_key)
            capture.check_identity(row, replay_identity_checks)
            capture.check_counters(row, config["counter_checks"], previous)
            if row["http_status"] == 200 and row["body_complete"] and row["value"] is not None:
                seen.add(row["endpoint_key"])
                counts[row["endpoint_key"]] += 1
    if seen != set(config["apis"]):
        raise ValueError("complete saved responses from all twelve endpoints required")
    evaluator.validate_limits(json.loads(Path(config["artifacts"]["limits"]["file"]).read_text()))
    runtime_paths = {("image_sha256",), ("source_sha256",)}
    replay_runtime = {check["path"][0]: check["expected"] for check in replay_identity_checks
                      if check["endpoint_key"] == "TX1" and tuple(check["path"]) in runtime_paths}
    prospective_runtime = {check["path"][0]: check["expected"] for check in config["identity_checks"]
                           if check["endpoint_key"] == "TX1" and tuple(check["path"]) in runtime_paths}
    return {"schema": SCHEMA, "status": "PASS_OFFLINE_PREPARATION",
            "configuration_sha256": projection(config), "field_contract": fields,
            "replay_identity_scope": "HISTORICAL_FIXTURE_ONLY_NOT_CANDIDATE_LIVE_BINDING",
            "replay_tx_runtime": replay_runtime, "prospective_tx_runtime": prospective_runtime,
            "saved_response_counts": counts, "source_sha256": {str(path): sha256(path) for path in sources},
            "plans": capture.PLANS, "samples_per_endpoint": capture.sample_count(capture.PLANS[config["mode"]]),
            "readiness_started": False, "live_queries": 0, "configuration_writes": 0}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--config", type=Path, required=True)
    parser.add_argument("--responses", type=Path, action="append", required=True,
                        help="saved preflight with snapshots, or saved collector run with rows")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    project = Path(__file__).resolve().parents[1]
    if not args.output.resolve().is_relative_to(project / "private"):
        parser.error("private output required")
    snapshots = []
    for path in args.responses:
        saved = json.loads(path.read_text())
        snapshots.extend(saved["snapshots"] if "snapshots" in saved else [saved["rows"]])
    prepared = prepare(json.loads(args.config.read_text()), snapshots, args.responses)
    with args.output.open("x") as stream:
        json.dump(prepared, stream, indent=2, sort_keys=True)
    print(json.dumps({"status": prepared["status"], "readiness_started": False, "live_queries": 0}))


if __name__ == "__main__":
    main()

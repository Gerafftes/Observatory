"""Verify private current AP station responses; publish only aggregate evidence.

This is one gate of the seven-client scope. Coverage, packet source bindings and
each participant's data progress remain independent mandatory gates.
"""
lazy import hashlib
lazy import re


CLIENTS = ("Mac", "RX1", "RX2", "RX3", "RX4", "MMWAVE1", "MMWAVE2")
IDENTITY_FIELDS = ("node_id", "boot_id", "image_sha256", "source_sha256", "source_mac_sha256")


def require(condition, reason):
    if not condition:
        raise ValueError(reason)


def mac(value):
    require(isinstance(value, str), "MAC field missing")
    value = value.replace(":", "").lower()
    require(re.fullmatch(r"[0-9a-f]{12}", value) is not None, "MAC field malformed")
    require(value != "000000000000" and not int(value[:2], 16) & 1, "invalid station MAC")
    return value


def integer(value):
    return type(value) is int and value >= 0


def host_wifi_mac(evidence, interface, window_begin_ns):
    """Read the actual interface MAC from a bound native host observation.

    Evidence is collected in the fresh preflight. The existing periodic host
    guard separately keeps that same CSI interface and route bound in scope.
    """
    require(isinstance(interface, str) and re.fullmatch(r"en[0-9]+", interface),
            "bound CSI interface missing")
    require(evidence["interface"] == interface, "host Wi-Fi interface differs")
    begin, end = evidence["begin_mono_ns"], evidence["end_mono_ns"]
    require(integer(begin) and integer(end) and
            window_begin_ns - 180_000_000_000 <= begin <= end <= window_begin_ns,
            "host Wi-Fi observation outside fresh preflight")
    ports = evidence["hardware_ports_stdout"]
    pattern = r"(?:^|\n)Hardware Port: (?:Wi-Fi|AirPort)\r?\nDevice: " + re.escape(interface) + r"\r?\n"
    require(re.search(pattern, ports) is not None, "CSI interface is not identified as Wi-Fi")
    output = evidence["ifconfig_stdout"]
    require(re.match(re.escape(interface) + r":", output) is not None,
            "ifconfig observation is for another interface")
    matches = re.findall(r"(?m)^\s*ether ([0-9a-fA-F:]{17})\s*$", output)
    require(len(matches) == 1, "actual Wi-Fi MAC unavailable or ambiguous")
    return mac(matches[0])


def assess(run, binding, window):
    """Require every scheduled TX response to identify all seven current clients.

    Never accept an externally supplied PASS flag as association evidence.
    A changed association generation also rejects reconnects hidden between
    otherwise identical station snapshots. Device clocks are never joined to
    packet clocks here.
    """
    result = {"status": "NOT_PROVEN", "method": "current protected TX AP station getter",
              "samples_checked": 0, "reasons": [],
              "participants": {name: {"status": "NOT_PROVEN"} for name in CLIENTS}}
    try:
        identity = binding["tx_identity"]
        require(set(IDENTITY_FIELDS) <= identity.keys(), "complete TX identity binding required")
        require(identity["node_id"] == "TX1" and identity["boot_id"] == binding["tx_boot_id"],
                "TX identity/boot bindings disagree")
        for key in IDENTITY_FIELDS[2:]:
            require(isinstance(identity[key], str) and re.fullmatch(r"[0-9a-f]{64}", identity[key]),
                    "TX digest binding malformed")
        ap = mac(binding["ap_mac"])
        require(hashlib.sha256(bytes.fromhex(ap)).hexdigest() == identity["source_mac_sha256"]
                == binding["source_mac_sha256"], "TX AP source binding disagrees")
        clients = {"Mac": host_wifi_mac(run["host_wifi"], run["interface"], window["begin_mono_ns"])}
        require(clients["Mac"] == mac(binding["host_mac"]), "packet host MAC is not actual Wi-Fi MAC")
        clients.update({name: mac(binding["rx"][name]["mac"]) for name in CLIENTS[1:5]})
        clients.update({name: mac(binding["radar"][name]["mac"]) for name in CLIENTS[5:]})
        require(len(set(clients.values())) == 7 and ap not in clients.values(),
                "seven distinct client identities required")
        rows = sorted((row for row in run["rows"] if row["endpoint_key"] == "TX1"
                       and window["first_slot"] <= row["slot"] <= window["last_slot"]),
                      key=lambda row: row["slot"])
        require([row["slot"] for row in rows] == list(range(window["first_slot"], window["last_slot"] + 1)),
                "missing or duplicate TX association samples")
        generation = previous_device_end = previous_host_end = None
        for row in rows:
            require(row["error"] is None and row["http_status"] == 200 and row["body_complete"] is True,
                    "incomplete TX association response")
            value = row["value"]
            require(all(value[key] == identity[key] for key in IDENTITY_FIELDS), "TX identity changed")
            host_begin, host_end = row["begin"]["mono_before_ns"], row["end"]["mono_after_ns"]
            require(integer(host_begin) and integer(host_end) and host_begin <= host_end,
                    "invalid host response boundaries")
            require(previous_host_end is None or previous_host_end < host_end,
                    "host response time did not advance")
            current = value["ap_association"]
            require(type(current["schema"]) is int and current["schema"] == 1
                    and current["status"] == "current", "current protected station list unavailable")
            require(type(current["station_list_rc"]) is int and current["station_list_rc"] == 0
                    and type(current["ap_mac_rc"]) is int and current["ap_mac_rc"] == 0,
                    "AP station getter failed")
            require(mac(current["ap_mac"]) == ap, "AP BSSID changed")
            start, end = current["capture_begin_us"], current["capture_end_us"]
            require(integer(start) and integer(end) and start <= end
                    and (previous_device_end is None or start > previous_device_end),
                    "station snapshot time invalid or replayed")
            require((end - start) * 1000 <= host_end - host_begin,
                    "station snapshot exceeds its HTTP boundaries")
            before, after = current["generation_before"], current["generation_after"]
            require(integer(before) and integer(after) and before == after,
                    "station associations changed during snapshot")
            require(generation is None or before == generation, "station associations changed in scope")
            stations = current["stations"]
            require(isinstance(stations, list) and type(current["station_count"]) is int
                    and current["station_count"] == len(stations), "invalid station list size")
            observed = [mac(item["mac"]) for item in stations]
            require(len(observed) == len(set(observed)), "duplicate station list entries")
            require(set(clients.values()) <= set(observed), "one or more direct clients absent")
            generation, previous_device_end, previous_host_end = before, end, host_end
            result["samples_checked"] += 1
        require(bool(rows), "no station snapshots")
        result["status"] = "PASS"
        result["participants"] = {name: {"status": "PASS", "method": result["method"]} for name in CLIENTS}
    except (ValueError, KeyError, TypeError, IndexError) as error:
        # No input values, MACs, body fragments or private paths in this result.
        result["reasons"].append(str(error) if isinstance(error, ValueError) else "required association evidence missing or malformed")
    return result

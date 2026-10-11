"""Integer-time offline packet decoding; no capture, sockets or device changes."""
lazy import hashlib
lazy import ipaddress
lazy import json
lazy import struct

lazy from project_tools import tx7b_coverage as coverage


def udp_packets(raw):
    coverage.pcap_span(raw)  # Same strict completeness, order and clock format gate.
    endian, resolution = {
        b"\xd4\xc3\xb2\xa1": ("<", 1000), b"\xa1\xb2\xc3\xd4": (">", 1000),
        b"\x4d\x3c\xb2\xa1": ("<", 1), b"\xa1\xb2\x3c\x4d": (">", 1),
    }[raw[:4]]
    offset, result = 24, []
    while offset < len(raw):
        sec, fraction, size, _ = struct.unpack_from(endian + "IIII", raw, offset)
        offset += 16
        frame = raw[offset:offset + size]
        offset += size
        if len(frame) < 14 or frame[12:14] != b"\x08\x00":
            raise ValueError("unexpected non-IPv4 frame in bounded UDP capture")
        ip = frame[14:]
        header = (ip[0] & 15) * 4
        if ip[0] >> 4 != 4 or header < 20 or len(ip) < header + 8 or ip[9] != 17:
            raise ValueError("invalid/non-UDP IP packet")
        length, = struct.unpack_from("!H", ip, 2)
        if length > len(ip) or length < header + 8 or struct.unpack_from("!H", ip, 6)[0] & 0x3FFF:
            raise ValueError("truncated/fragmented IP packet")
        source_port, destination_port, udp_length, _ = struct.unpack_from("!HHHH", ip, header)
        if udp_length < 8 or header + udp_length != length:
            raise ValueError("invalid UDP/IP length")
        payload = ip[header + 8:header + udp_length]
        result.append({"timestamp_ns": sec * 1_000_000_000 + fraction * resolution,
            "src_ip": str(ipaddress.ip_address(ip[12:16])),
            "dst_ip": str(ipaddress.ip_address(ip[16:20])),
            "src_mac": frame[6:12].hex(), "dst_mac": frame[:6].hex(),
            "src_port": source_port, "dst_port": destination_port,
            "payload": payload, "payload_sha256": hashlib.sha256(payload).hexdigest()})
    return result


def radar_identity(packet, devices, host_ip):
    """ACK identity uses reverse IP/port plus boot/sequence, never equal totals."""
    payload = packet["payload"]
    if packet["dst_port"] == 55010:
        try:
            value = json.loads(payload)
        except (ValueError, UnicodeDecodeError):
            return {"kind": "unparsed_radar", "bound": False}
        node = value.get("node_id") if isinstance(value, dict) else None
        if node not in devices:
            return {"kind": "unknown_radar", "bound": False}
        device = devices[node]
        bound = (packet["src_ip"] == device["ip"] and packet["dst_ip"] == host_ip
                 and packet["src_mac"] == device["mac"].replace(":", "").lower()
                 and value.get("schema") == "ruview.mmwave.ld2450.v1"
                 and all(type(value.get(k)) is int and 0 <= value[k] <= 0xFFFFFFFF
                         for k in ("boot_id", "sequence")))
        return {"kind": "radar", "node_id": node, "bound": bound,
                "boot_id": value.get("boot_id"), "sequence": value.get("sequence"),
                "application_record": value}
    if packet["src_port"] == 55010 and len(payload) == 12:
        magic, boot, sequence = struct.unpack("!III", payload)
        nodes = [node for node, device in devices.items() if device["ip"] == packet["dst_ip"]]
        if magic == 0x5256414B and len(nodes) == 1:
            node = nodes[0]
            return {"kind": "ack", "node_id": node, "boot_id": boot, "sequence": sequence,
                    "bound": packet["src_ip"] == host_ip
                    and packet["dst_mac"] == devices[node]["mac"].replace(":", "").lower()}
    return {"kind": "other", "bound": False}


def matching_ack(packet, ack):
    return (packet["kind"] == "radar" and ack["kind"] == "ack"
            and packet["bound"] and ack["bound"]
            and all(packet[k] == ack[k] for k in ("node_id", "boot_id", "sequence"))
            and packet["src_ip"] == ack["dst_ip"] and packet["dst_ip"] == ack["src_ip"]
            and packet["src_port"] == ack["dst_port"] and packet["dst_port"] == ack["src_port"]
            and ack["timestamp_ns"] >= packet["timestamp_ns"])


def radar_ack_attempt_pairs(records):
    """Pair each captured ACK once within the latest same-path attempt window.

    A repeated boot/sequence on the same path closes the previous window. An ACK
    observed after that repeat cannot also confirm the original attempt.
    This records capture order, not ACK receipt at the radar or server content
    acceptance; identical retransmissions have no separate wire attempt ID.
    """
    pending, pairs, previous_ns = {}, [], None
    for index, record in enumerate(records):
        timestamp_ns = record["timestamp_ns"]
        if (type(timestamp_ns) is not int or timestamp_ns < 0
                or previous_ns is not None and timestamp_ns < previous_ns):
            raise ValueError("radar/ACK records must retain capture timestamp order")
        previous_ns = timestamp_ns
        if record["kind"] not in ("radar", "ack") or not record["bound"]:
            continue
        identity = tuple(record[k] for k in ("node_id", "boot_id", "sequence"))
        if record["kind"] == "radar":
            path = tuple(record[k] for k in ("src_ip", "dst_ip", "src_port", "dst_port"))
            pending[identity + path] = index
        else:
            path = tuple(record[k] for k in ("dst_ip", "src_ip", "dst_port", "src_port"))
            radar_index = pending.get(identity + path)
            if radar_index is not None and matching_ack(records[radar_index], record):
                pairs.append((radar_index, index))
                del pending[identity + path]
    return pairs


def csi_header(payload, boot_id, source_hash):
    """Header/trailer evidence only; eligible does not mean server-accepted."""
    result = {"kind": "other_rx4", "header_and_trailers_match": False,
              "fixed_grid_eligible": False, "server_accepted": None}
    if len(payload) < 20 or payload[:4] != struct.pack("<I", 0xC5110001):
        return result
    carriers = int.from_bytes(payload[6:8], "little")
    result.update(kind="csi", node_id=payload[4], antennas=payload[5], subcarriers=carriers,
                  frequency_mhz=int.from_bytes(payload[8:12], "little"),
                  sequence=int.from_bytes(payload[12:16], "little"), ppdu=payload[18], flags=payload[19])
    tail = 20 + 2 * payload[5] * carriers
    if len(payload) != tail + 64:
        return result
    source, identity = payload[tail:tail + 40], payload[tail + 40:]
    result["header_and_trailers_match"] = (
        payload[4] == 4 and payload[5] > 0 and carriers > 0
        and source[:8] == struct.pack("<IBBH", 0xC511A060, 1, 7, 40)
        and source[8:] == bytes.fromhex(source_hash)
        and identity[:8] == struct.pack("<IBBH", 0xC511A061, 2, 1, 24)
        and int.from_bytes(identity[8:16], "little") == int(boot_id, 16)
        and identity[16:] == b"\0" * 8)
    result["fixed_grid_eligible"] = (result["header_and_trailers_match"] and payload[5] == 1
        and carriers == 128 and result["frequency_mhz"] == 2437
        and payload[18] == 0 and payload[19] & ~0x10 == 0)
    return result

#!/usr/bin/env python3.15
"""Explicit read-only 7A/7B windows. Import, --help and --fixture never contact a host."""
from __future__ import annotations
lazy import argparse
lazy import hashlib
lazy import ipaddress
lazy import json
lazy import math
lazy import os
lazy from pathlib import Path
lazy import re
lazy import signal
lazy import statistics
lazy import struct
lazy import subprocess
lazy import sys
lazy import time
lazy import zlib

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
lazy from provision import parse_mac, read_private, write_private

WARMUP = 60
WINDOW = 120
WINDOWS = 3
READY_BUDGET = 180
TOTAL_BUDGET = 630
PASS, INVALID, UNAVAILABLE, UNCLEAR, REGRESSION = 0, 2, 3, 4, 5
COUNTERS = ("accepted_csi_frames", "sequence_observations", "inferred_lost_frames", "skipped_grid_frames")
RX_COUNTERS = ("enqueued", "queue_full_drops", "sent", "retries", "errors", "abandoned",
               "socket_rebuilds", "socket_send_failures", "enomem_backoffs")
EXPECTED_RX4_RASTER = {"frequency_mhz":2437,"antennas":1,"ppdu":0,"stable_flags":0}
VALID_RX4_LENGTHS = (64,128)
RX4_TRANSIENT_FLAGS = 0x10
SERVER_INGRESS_COUNTERS = ("udp_datagrams_from_os","udp_queue_overflow_datagrams",
    "udp_queue_stale_datagrams","udp_receive_errors","csi_datagrams_received",
    "csi_frames_accepted_new","csi_frames_discarded","csi_queue_overflow_datagrams",
    "csi_queue_stale_datagrams")


class MeasurementError(ValueError):
    def __init__(self,message,*,expected=None,actual=None):
        super().__init__(message)
        self.expected=expected
        self.actual=actual


def reject(message,expected=None,actual=None):
    raise MeasurementError(message,expected=expected,actual=actual)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":")).encode()


def validate_config(c):
    common = {"ap_mac", "rx4_ip", "rx4_image_sha256", "server_binary_sha256", "server_port",
              "channel", "rate_hz", "old_manifest_sha256", "candidate_image_sha256",
              "candidate_source_sha256", "conditions_sha256"}
    legacy = common | {"hotspot_interface", "hotspot_hardware_port"}
    current = common | {"internet_interface", "internet_hardware_port", "criteria_sha256",
                        "provision_sha256", "start_helper_sha256"}
    if set(c) not in (legacy, current): raise ValueError("measurement config fields incomplete/unknown")
    parse_mac(c["ap_mac"])
    if ipaddress.ip_address(c["rx4_ip"]) not in ipaddress.ip_network("192.168.4.0/24") or c["rx4_ip"] in (
            "192.168.4.0", "192.168.4.1", "192.168.4.50", "192.168.4.255"):
        raise ValueError("RX4 target is outside the direct CSI client range")
    for k in ("rx4_image_sha256", "server_binary_sha256", "old_manifest_sha256",
              "candidate_image_sha256", "candidate_source_sha256", "conditions_sha256"):
        if not re.fullmatch("[0-9a-f]{64}", c[k]): raise ValueError("identity hash missing")
    if c["channel"] != 6 or c["rate_hz"] != 50 or type(c["server_port"]) is not int or not 1 <= c["server_port"] <= 65535:
        raise ValueError("migration contract differs")
    if set(c)==legacy:
        if not re.fullmatch(r"en[0-9]+", c["hotspot_interface"]) or "USB" not in c["hotspot_hardware_port"]:
            raise ValueError("explicit legacy USB hotspot interface/hardware port required")
    else:
        if (not re.fullmatch(r"en[0-9]+", c["internet_interface"])
                or not isinstance(c["internet_hardware_port"],str) or not c["internet_hardware_port"].strip()):
            raise ValueError("explicit LAN Internet interface/hardware port required")
        for key in ("criteria_sha256", "provision_sha256", "start_helper_sha256"):
            if not re.fullmatch("[0-9a-f]{64}", c[key]): raise ValueError("measurement provenance hash missing")


def command(args, timeout=3):
    result = subprocess.run(args, capture_output=True, text=True, timeout=timeout)
    if result.returncode: raise RuntimeError("read-only command failed")
    return result.stdout


def route_value(text, key):
    match = re.search(r"^\s*" + re.escape(key) + r":\s*(\S+)", text, re.M)
    return match.group(1) if match else None


def connection_check(c, run=command):
    """Route/link proof only. Does not claim Internet reachability or send probes."""
    ports = run(["/usr/sbin/networksetup", "-listallhardwareports"])
    hardware = dict(re.findall(r"Hardware Port: ([^\n]+)\nDevice: (\S+)", ports))
    csi = run(["/sbin/route", "-n", "get", "192.168.4.1"])
    iface = route_value(csi, "interface")
    wifi = next((v for k,v in hardware.items() if k in ("Wi-Fi", "AirPort", "WLAN")), None)
    if not iface or iface != wifi: raise ValueError("CSI route is not on the WiFi interface")
    try:
        ssid = run(["/usr/sbin/networksetup", "-getairportnetwork", iface]).strip()
    except RuntimeError:
        # macOS commonly returns nonzero when it does not expose the SSID.
        # Continue only with the independent DHCP, gateway-MAC, and alias checks below.
        ssid = ""
    ssid_match=re.search(r"Current Wi-Fi Network:\s*(.*?)\s*$",ssid,re.M)
    ssid_name=ssid_match.group(1).strip() if ssid_match else None
    if ssid_name and ssid_name!="csi-test":
        raise ValueError("WiFi reports a different SSID; no network switch performed")
    if route_value(csi,"gateway") not in (None,"192.168.4.1") and not str(route_value(csi,"gateway")).startswith("link#"):
        raise ValueError("CSI route uses a foreign gateway")
    packet = run(["/usr/sbin/ipconfig", "getpacket", iface])
    def dhcp(key):
        m=re.search(re.escape(key)+r"\s+\([^)]*\):\s*\{?([^}\n]+)",packet)
        return m.group(1).strip() if m else None
    lease=dhcp("yiaddr")
    if not lease:
        m=re.search(r"yiaddr\s*=\s*(\S+)",packet);lease=m.group(1) if m else None
    if not lease or ipaddress.ip_address(lease) not in ipaddress.ip_network("192.168.4.0/24") or lease in (
            "192.168.4.0","192.168.4.1","192.168.4.50","192.168.4.255"):
        raise ValueError("DHCP primary address absent/conflicting")
    if any(dhcp(k)!=v for k,v in {"subnet_mask":"255.255.255.0","router":"192.168.4.1","server_identifier":"192.168.4.1"}.items()):
        raise ValueError("DHCP identity differs")
    arp=run(["/usr/sbin/arp","-n","-i",iface,"192.168.4.1"])
    m=re.search(r" at ([0-9a-f:]+) ",arp,re.I)
    observed=":".join(f"{int(x,16):02x}" for x in m.group(1).split(":")) if m else ""
    if observed != c["ap_mac"].lower(): raise ValueError("gateway MAC absent/different; no active ARP probe performed")
    interfaces=run(["/sbin/ifconfig","-a"])
    alias=re.findall(r"inet 192\.168\.4\.50 netmask (\S+)",interfaces)
    section_match=re.search(r"(?ms)^"+re.escape(iface)+r":.*?(?=^[^\s]+: flags=|\Z)",interfaces)
    section=section_match.group(0) if section_match else ""
    if alias != ["0xffffffff"] or not re.search(r"inet 192\.168\.4\.50 netmask 0xffffffff",section):
        raise ValueError("exactly one existing .50/32 alias required")
    if not re.search(r"inet "+re.escape(lease)+r" netmask 0xffffff00",section):
        raise ValueError("DHCP address/netmask is not active on CSI WiFi")
    local_mac = re.search(r"ether ([0-9a-f:]+)", section, re.I)
    neighbors = run(["/usr/sbin/arp", "-an", "-i", iface])
    for claimed in re.findall(r"\(192\.168\.4\.50\) at ([0-9a-f:]+)", neighbors, re.I):
        normalized = ":".join(f"{int(x,16):02x}" for x in claimed.split(":"))
        if not local_mac or normalized.lower() != local_mac.group(1).lower():
            raise ValueError("foreign .50 claim; leave alias/network unchanged")
    default=run(["/sbin/route","-n","get","default"])
    default_iface=route_value(default,"interface")
    if "internet_interface" in c:
        internet_iface=c["internet_interface"]
        internet_port=c["internet_hardware_port"]
        if hardware.get(internet_port)!=internet_iface:
            raise ValueError("bound LAN Internet hardware port/interface changed")
        if default_iface!=internet_iface:
            raise ValueError("default route is not on the bound LAN Internet interface")
    else:
        internet_iface=c["hotspot_interface"]
        internet_port=c["hotspot_hardware_port"]
    separate_uplink=bool(default_iface and default_iface!=iface)
    hotspot = ("hotspot_interface" in c and hardware.get(internet_port)==internet_iface
               and default_iface==internet_iface)
    return {"csi_interface":iface,"dhcp_primary":lease,"alias":"192.168.4.50/32",
            "ssid_status":"visible-csi-test" if ssid_name=="csi-test" else "not-exposed",
            "wifi_identity":"expected-gateway-mac","gateway_mac":observed,
            "default_interface":default_iface,"internet_interface":internet_iface,
            "internet_hardware_port":internet_port,"internet_route_is_separate":separate_uplink,
            "usb_hotspot_route":hotspot,"internet_reachability":"not probed",
            "offline_measurement_supported":True}


def get_json(host, port, route):
    # Numeric destinations only; no proxy, redirects, DNS or unbounded body.
    ipaddress.ip_address(host)
    raw=command(["/usr/bin/curl","--noproxy","*","--silent","--show-error","--fail",
                 "--connect-timeout","1","--max-time","2","--max-filesize","1048576",
                 f"http://{host}:{port}{route}"],timeout=3)
    return json.loads(raw)


def sample(c, phase, cached=None, refresh=True):
    observed_start=time.time()
    nodes=get_json("127.0.0.1",c["server_port"],"/api/v1/nodes")
    matches=[n for n in nodes["nodes"] if n.get("node_id")==4]
    if len(matches)!=1: raise ValueError("RX4 is not uniquely present")
    node=matches[0]
    rx_host=node.get("transport_ip") or c["rx4_ip"]
    rx_host=str(ipaddress.ip_address(rx_host))
    if ipaddress.ip_address(rx_host) not in ipaddress.ip_network("192.168.4.0/24") or rx_host in (
            "192.168.4.0","192.168.4.1","192.168.4.50","192.168.4.255"):
        raise ValueError("RX4 live transport IP is outside the direct CSI client range")
    rx=get_json(rx_host,8032,"/ota/status") if refresh or cached is None else cached["rx"]
    ingress=(get_json("127.0.0.1",c["server_port"],"/health/metrics")["csi_ingress"]["counters"]
             if refresh or cached is None or "server_ingress" not in cached else cached["server_ingress"])
    return {"t":time.time(),"mono":time.monotonic(),"observed_start_utc":observed_start,
            "node":node,"rx":rx,"rx_status_target_ip":rx_host,"server_ingress":ingress,
            "aggregate_source_binding_consistent_across_nodes":nodes.get("source_binding_consistent_across_nodes"),
            "tx":(get_json("192.168.4.1",8032,"/status") if refresh or cached is None else cached["tx"]) if phase=="after" else None}


def check_sample(s,c,boot=None):
    n,r=s["node"],s["rx"]
    if (n.get("node_id")!=4 or n.get("last_seen_ms",2000)>=2000 or n.get("status")=="stale"
            or not all(n.get(k) is True for k in ("source_binding_attested","filter_enforced","source_matched_filter","identity_valid"))
            or n.get("protocol_version")!=2 or not n.get("boot_id") or int(n["boot_id"],16)==0
            or r.get("node_id")!="RX4" or r.get("boot_id")!=n["boot_id"]
            or r.get("image_sha256")!=c["rx4_image_sha256"] or r.get("ip")!=c["rx4_ip"]):
        raise ValueError("RX4 freshness/identity/image contract failed")
    if (n.get("transport_ip",c["rx4_ip"])!=c["rx4_ip"]
            or s.get("rx_status_target_ip",c["rx4_ip"])!=c["rx4_ip"]):
        raise ValueError("RX4 transport/status address changed during observation")
    if boot and n["boot_id"]!=boot: raise ValueError("unexpected RX4 reboot")
    if s.get("tx") and (s["tx"].get("boot_check")!=1 or s["tx"].get("channel")!=6
            or s["tx"].get("rate_hz")!=50 or s["tx"].get("source_mac_sha256")!=digest(parse_mac(c["ap_mac"]))
            or s["tx"].get("image_sha256")!=c["candidate_image_sha256"]
            or s["tx"].get("source_sha256")!=c["candidate_source_sha256"]):
        raise ValueError("TX local boot/source contract failed")


def check_progress(current,c,state):
    """The accepted 128 path must stay fresh even while valid 64 transport continues."""
    check_sample(current,c,state.get("boot"))
    previous=state.get("previous")
    if previous is not None:
        gap=current["mono"]-previous["mono"]
        if not 0<=gap<=2:
            reject("observation gap",{"interval_seconds":"0..2"},{"interval_seconds":gap})
        ds=deltas(previous["node"],current["node"],COUNTERS)
        deltas(previous["rx"],current["rx"],("sequence",))
        deltas(previous["rx"]["transport"],current["rx"]["transport"],RX_COUNTERS)
        if ds["accepted_csi_frames"]>0:
            state["last_accepted_progress"]=current["mono"]
    else:
        state["last_accepted_progress"]=current["mono"]
    age=current["mono"]-state["last_accepted_progress"]
    if age>=2:
        reject("accepted 128 CSI progress stale",{"accepted_progress_age_seconds":"<2"},
               {"accepted_progress_age_seconds":age,"accepted_csi_frames":current["node"]["accepted_csi_frames"],
                "skipped_grid_frames":current["node"]["skipped_grid_frames"],
                "last_seen_ms":current["node"]["last_seen_ms"]})
    state["previous"]=current
    state["boot"]=current["node"]["boot_id"]


def check_grid_exclusions(samples,packets,c):
    """Bound asynchronous counter endpoints by their recorded HTTP request intervals."""
    a,b=samples[0],samples[-1]
    binding=bytes.fromhex(digest(parse_mac(c["ap_mac"])))
    times=[]
    for packet in packets:
        if packet["port"]==5005 and packet["src"]==c["rx4_ip"]:
            if rx4_packet_kind(packet["data"],a["node"]["boot_id"],binding)=="csi" and int.from_bytes(packet["data"][6:8],"little")==64:
                times.append(packet["t"])
    earliest=a.get("observed_start_utc",a["t"])
    latest=b.get("observed_start_utc",b["t"])
    minimum=sum(a["t"]<=t<latest for t in times)
    maximum=sum(earliest<=t<b["t"] for t in times)
    skipped=deltas(a["node"],b["node"],COUNTERS)["skipped_grid_frames"]
    result={"skipped_grid_frames":skipped,"captured_64_minimum":minimum,
            "captured_64_maximum":maximum,"boundary_uncertainty":maximum-minimum,
            "method":"HTTP endpoint time brackets; no numeric tolerance"}
    if not minimum<=skipped<=maximum:
        reject("unexplained server grid exclusions",{"skipped_grid_frames_range":[minimum,maximum]},result)
    return result


def pcap_packets(path, allow_partial=False):
    raw=path.read_bytes()
    if allow_partial and len(raw)<24: return []
    magic=raw[:4]
    formats={b"\xd4\xc3\xb2\xa1":("<",1e6),b"\xa1\xb2\xc3\xd4":(">",1e6),b"\x4d\x3c\xb2\xa1":("<",1e9)}
    if magic not in formats or len(raw)<24:
        reject("unsupported/incomplete pcap",{"header_bytes":24,"magic":"supported pcap"},
               {"header_bytes":len(raw),"magic":magic.hex()})
    endian,scale=formats[magic]
    linktype=struct.unpack_from(endian+"I",raw,20)[0]
    if linktype!=1:
        reject("capture needs Ethernet link format; do not change WiFi mode",{"linktype":1},{"linktype":linktype})
    offset=24;result=[]
    while offset<len(raw):
        if offset+16>len(raw):
            if allow_partial: break
            reject("truncated pcap record",{"record_header_bytes":16},{"bytes_remaining":len(raw)-offset})
        sec,fraction,length,original=struct.unpack_from(endian+"IIII",raw,offset);offset+=16
        if allow_partial and offset+length>len(raw): break
        frame=raw[offset:offset+length];offset+=length
        if len(frame)!=length or length<original:
            reject("truncated captured frame",{"captured_bytes":length,"original_bytes":length},
                   {"captured_bytes":len(frame),"original_bytes":original})
        if len(frame)<42 or frame[12:14]!=b"\x08\x00": continue
        ip=frame[14:];header=(ip[0]&15)*4
        if ip[0]>>4!=4 or header<20 or len(ip)<header+8 or ip[9]!=17: continue
        fragment=struct.unpack_from("!H",ip,6)[0]&0x3fff
        if fragment: reject("fragmented measurement UDP",{"fragment_offset_and_flags":0},{"fragment_offset_and_flags":fragment})
        sport,dport,udp_size,_=struct.unpack_from("!HHHH",ip,header)
        if dport not in (4210,5005): continue
        data=ip[header+8:header+udp_size]
        if len(data)!=udp_size-8:
            reject("invalid UDP length",{"payload_bytes":udp_size-8},{"payload_bytes":len(data)})
        result.append({"t":sec+fraction/scale,"port":dport,"src":str(ipaddress.ip_address(ip[12:16])),
                       "src_mac":frame[6:12].hex(),"data":data})
    return result


def audit_packets(packets, c, phase, boot, state, tx_boot=None):
    """Validate every completed measurement datagram, including warm-up datagrams."""
    binding=bytes.fromhex(digest(parse_mac(c["ap_mac"])))
    for packet in packets:
        data=packet["data"]
        if packet["port"]==4210:
            expected_mac=parse_mac(c["ap_mac"]).hex()
            if packet["src"]!="192.168.4.1" or packet["src_mac"]!=expected_mac:
                reject("sounding source differs",{"source_ip":"192.168.4.1","source_mac":expected_mac},
                       {"source_ip":packet["src"],"source_mac":packet["src_mac"],"t":packet["t"]})
            if len(data)!=32 or data[:6]!=b"CSI_TX" or data[30:]!=b"\0\0":
                reject("sounding identity/packet contract failed",{"bytes":32,"magic":"CSI_TX","trailing_zero":True},
                       {"bytes":len(data),"magic":data[:6].decode("ascii","replace"),
                        "trailing_zero":data[30:]==b"\0\0","t":packet["t"]})
            if phase=="before":
                if data[6:8]!=b"\0\0" or data[16:]!=b"\0"*16:
                    reject("old sounding differs from reference",{"version":[0,0],"bytes_16_to_31":"zero"},
                           {"version":list(data[6:8]),"bytes_16_to_31_zero":data[16:]==b"\0"*16,"t":packet["t"]})
            elif (data[6:8]!=b"\2\2" or data[16:20]!=b"TX1\0" or data[28:30]!=bytes([6,50])
                  or int.from_bytes(data[20:28],"little")==0):
                reject("v2 sounding fields invalid",{"version":[2,2],"marker":"TX1\\0","channel":6,"rate_hz":50,"boot_id":"nonzero"},
                       {"version":list(data[6:8]),"marker":data[16:20].decode("ascii","replace"),
                        "channel":data[28],"rate_hz":data[29],"boot_id":int.from_bytes(data[20:28],"little"),"t":packet["t"]})
            if phase=="after" and tx_boot and int.from_bytes(data[20:28],"little")!=int(tx_boot,16):
                reject("TX packet/status boot identity differs",{"boot_id":tx_boot},
                       {"boot_id":f"{int.from_bytes(data[20:28],'little'):016x}","t":packet["t"]})
            sequence=int.from_bytes(data[8:12],"little")
            timestamp=int.from_bytes(data[12:16],"little")
            if ((state.get("last_tx_sequence") is not None and sequence<=state["last_tx_sequence"])
                    or (state.get("last_tx_ms") is not None and timestamp<state["last_tx_ms"])):
                reject("TX sequence/time reset, duplicate or wrap",{"sequence":"> previous","timestamp":">= previous"},
                       {"sequence":sequence,"timestamp":timestamp,"last_sequence":state.get("last_tx_sequence"),
                        "last_timestamp":state.get("last_tx_ms"),"t":packet["t"]})
            state["last_tx_sequence"],state["last_tx_ms"]=sequence,timestamp
            state["sounding"]=state.get("sounding",0)+1
        elif packet["port"]==5005:
            if packet["src"]!=c["rx4_ip"]:
                reject("unexpected source on RX4 CSI port",{"source_ip":c["rx4_ip"]},
                       {"source_ip":packet["src"],"source_mac":packet["src_mac"],"t":packet["t"]})
            try:
                kind=rx4_packet_kind(data,boot,binding)
            except MeasurementError as error:
                error.actual={**error.actual,"packet_time_utc":packet["t"],
                              "source_ip":packet["src"],"payload_bytes":len(data)}
                raise
            state[kind]=state.get(kind,0)+1
            if kind=="csi":
                length=int.from_bytes(data[6:8],"little")
                key=f"csi_{length}"
                state[key]=state.get(key,0)+1
                time_key=f"last_{key}_time"
                if length==128 and time_key in state:
                    interval=packet["t"]-state[time_key]
                    if not 0<=interval<2:
                        reject("RX4 128 capture freshness gap",{"maximum_gap_seconds":"<2"},
                               {"maximum_gap_seconds":interval,"packet_time_utc":packet["t"]})
                state[time_key]=packet["t"]
                sequence=int.from_bytes(data[12:16],"little")
                previous=state.get("last_rx_sequence")
                if previous is not None:
                    if sequence<=previous:
                        reject("RX4 transport sequence reset/duplicate/wrap",{"sequence":f"> {previous}"},
                               {"sequence":sequence,"t":packet["t"]})
                    state["rx_sequence_observations"]=state.get("rx_sequence_observations",0)+1
                    state["rx_sequence_missing"]=state.get("rx_sequence_missing",0)+sequence-previous-1
                state.setdefault("first_rx_sequence",sequence)
                state["last_rx_sequence"]=sequence
        else:
            reject("unexpected packet in filtered capture",{"ports":[4210,5005]},
                   {"port":packet["port"],"source_ip":packet.get("src"),"t":packet["t"]})


def audit_capture(path, c, phase, boot, state, tx_boot=None):
    if not path.is_file(): return
    packets=pcap_packets(path,allow_partial=True)
    processed=state.get("processed",0)
    if len(packets)<processed:
        reject("capture packet count regressed",{"minimum":processed},{"captured":len(packets)})
    audit_packets(packets[processed:],c,phase,boot,state,tx_boot)
    state["processed"]=len(packets)


def observed_tx_boot(current, phase):
    if phase=="before": return None
    status=current.get("tx")
    if not isinstance(status,dict) or not status.get("boot_id"):
        reject("new TX boot status missing",{"tx_boot_id":"present"},
               {"tx_boot_id":status.get("boot_id") if isinstance(status,dict) else None})
    return status["boot_id"]


def gaps(times,start,end):
    if len(times)<2: reject("insufficient captured packets",{"minimum_packets":2},{"packets":len(times)})
    differences=[b-a for a,b in zip(times,times[1:])]
    if any(x<0 for x in differences):
        reject("capture clock regression",{"intervals":"nonnegative"},{"negative_intervals":[x for x in differences if x<0]})
    ordered=sorted(differences)
    return {"rate":len(times)/(end-start),"interval_p50":statistics.median(ordered),
            "interval_p95":ordered[min(len(ordered)-1,int(len(ordered)*.95))],"interval_max":max(ordered),
            "max_gap":max([times[0]-start,end-times[-1],*differences])}


def deltas(first,last,keys):
    result={k:last[k]-first[k] for k in keys}
    if any(type(first[k]) is not int or type(last[k]) is not int or result[k]<0 for k in keys):
        reject("counter reset/wrap/invalid type",{k:"integer; monotone within window" for k in keys},
               {k:{"first":first[k],"last":last[k],"delta":result[k],
                   "first_type":type(first[k]).__name__,"last_type":type(last[k]).__name__} for k in keys})
    return result


def rx4_packet_kind(data, boot, binding):
    if len(data) < 4:
        reject("RX4 packet type missing",{"minimum_bytes":4},{"bytes":len(data)})
    magic = int.from_bytes(data[:4], "little")
    if magic == 0xC5110001:  # ADR-018 raw CSI
        if len(data)<20: reject("RX4 CSI header truncated",{"header_bytes":20},{"bytes":len(data)})
        actual_grid={"frequency_mhz":int.from_bytes(data[8:12],"little"),"antennas":data[5],
                     "subcarriers":int.from_bytes(data[6:8],"little"),"ppdu":data[18],
                     "stable_flags":data[19]&~RX4_TRANSIENT_FLAGS,"flags":data[19]}
        if (data[4]!=4 or actual_grid["subcarriers"] not in VALID_RX4_LENGTHS
                or any(actual_grid[k]!=v for k,v in EXPECTED_RX4_RASTER.items() if k!="subcarriers")
                or data[19]&~RX4_TRANSIENT_FLAGS):
            reject("RX4 CSI raster/header differs",{"node_id":4,**EXPECTED_RX4_RASTER,"subcarriers":list(VALID_RX4_LENGTHS),
                    "allowed_transient_flag_mask":RX4_TRANSIENT_FLAGS},
                   {"node_id":data[4],**actual_grid})
        iq=2*data[5]*int.from_bytes(data[6:8],"little");tail=20+iq
        if len(data)!=tail+64:
            reject("RX4 CSI frame length invalid",{"bytes":tail+64},{"bytes":len(data)})
        if data[tail:tail+8]!=struct.pack("<IBBH",0xC511A060,1,7,40) or data[tail+8:tail+40]!=binding:
            reject("RX4 CSI source trailer invalid",{"magic":"0xC511A060","version":1,"flags":7,
                    "length":40,"binding_sha256":binding.hex()},
                   {"magic":data[tail:tail+4].hex(),"version":data[tail+4] if len(data)>tail+4 else None,
                    "flags":data[tail+5] if len(data)>tail+5 else None,
                    "length":int.from_bytes(data[tail+6:tail+8],"little") if len(data)>=tail+8 else None,
                    "binding_sha256":data[tail+8:tail+40].hex() if len(data)>=tail+40 else None})
        ident=data[tail+40:]
        if ident[:8]!=struct.pack("<IBBH",0xC511A061,2,1,24) or int.from_bytes(ident[8:16],"little")!=int(boot,16) or ident[16:]!=b"\0"*8:
            reject("RX4 CSI boot trailer invalid",{"magic":"0xC511A061","protocol_version":2,"role":1,
                    "length":24,"boot_id":boot,"reserved":"zero"},
                   {"magic":ident[:4].hex(),"protocol_version":ident[4],"role":ident[5],
                    "length":int.from_bytes(ident[6:8],"little"),"boot_id":ident[8:16].hex(),
                    "reserved_zero":ident[16:]==b"\0"*8})
        return "csi"
    if magic == 0xC511A110:  # ADR-110 sync packet
        if (len(data)!=32 or data[4]!=4 or data[5]!=1 or data[6]&~7
                or data[7]!=0 or data[28:32]!=b"\0"*4):
            reject("RX4 sync packet invalid",{"bytes":32,"node_id":4,"version":1,"flags_mask":7,
                    "reserved":"zero"},{"bytes":len(data),"node_id":data[4] if len(data)>4 else None,
                    "version":data[5] if len(data)>5 else None,"flags":data[6] if len(data)>6 else None,
                    "reserved_zero":data[28:32]==b"\0"*4})
        return "sync"
    if magic == 0xC5110006:  # ADR-081 feature state
        if (len(data)!=60 or data[4]!=4 or data[5]>4 or data[54:56]!=b"\0"*2
                or zlib.crc32(data[:-4])!=int.from_bytes(data[-4:],"little")):
            reject("RX4 feature-state packet invalid",{"bytes":60,"node_id":4,"state_max":4,
                    "reserved":"zero","crc32":"matches payload"},
                   {"bytes":len(data),"node_id":data[4] if len(data)>4 else None,
                    "state":data[5] if len(data)>5 else None,"reserved_zero":data[54:56]==b"\0"*2,
                    "crc32_valid":len(data)==60 and zlib.crc32(data[:-4])==int.from_bytes(data[-4:],"little")})
        return "feature_state"
    if magic == 0xC5118100:  # ADR-081 mesh HEALTH
        if (len(data)!=48 or data[4]!=1 or data[5]!=6 or data[6]>4 or data[7]!=0
                or data[12:16]!=struct.pack("<HH",28,0) or data[32]>4
                or data[42:44]!=b"\0"*2
                or zlib.crc32(data[:-4])!=int.from_bytes(data[-4:],"little")):
            reject("RX4 mesh-health packet invalid",{"bytes":48,"protocol_version":1,"packet_type":6,
                    "state_max":4,"payload_bytes":28,"reserved":"zero","crc32":"matches payload"},
                   {"bytes":len(data),"protocol_version":data[4] if len(data)>4 else None,
                    "packet_type":data[5] if len(data)>5 else None,"state":data[6] if len(data)>6 else None,
                    "payload_bytes":int.from_bytes(data[12:14],"little") if len(data)>=14 else None,
                    "reserved_zero":data[42:44]==b"\0"*2,
                    "crc32_valid":len(data)==48 and zlib.crc32(data[:-4])==int.from_bytes(data[-4:],"little")})
        return "mesh_health"
    reject("unknown RX4 packet type",{"known_magic":["0xC5110001","0xC511A110","0xC5110006","0xC5118100"]},
           {"magic":f"0x{magic:08X}","bytes":len(data)})


def evaluate(run,c,packets):
    if run["warmup_seconds"]!=60 or len(run["windows"])!=3 or run.get("capture_drops")!=0:
        reject("measurement protocol/capture completeness invalid",
               {"warmup_seconds":60,"window_count":3,"capture_drops":0},
               {"warmup_seconds":run.get("warmup_seconds"),"window_count":len(run.get("windows",[])),
                "capture_drops":run.get("capture_drops")})
    warmup=run.get("warmup_samples",[])
    progress={}
    for current in warmup:
        check_progress(current,c,progress)
    if warmup:check_grid_exclusions(warmup,packets,c)
    return evaluate_windows(run,c,packets)


def evaluate_windows(run,c,packets):
    results=[];boot=None;tx_boot=None;last_tx_sequence=None;last_tx_ms=None
    first_sample=run["windows"][0]["samples"][0]
    audit_state={}
    audit_packets(packets,c,run["phase"],first_sample["node"]["boot_id"],audit_state,
                  observed_tx_boot(first_sample,run["phase"]))
    binding=bytes.fromhex(digest(parse_mac(c["ap_mac"])))
    progress={}
    for window in run["windows"]:
        samples=window["samples"]
        if len(samples)<121:
            reject("missing one-second samples",{"minimum_samples":121},{"samples":len(samples)})
        for s in samples:
            if run["phase"] == "after" and not s.get("tx"):
                reject("new TX status missing",{"tx_status":"present"},{"tx_status":"missing"})
            check_progress(s,c,progress);boot=s["node"]["boot_id"]
            if s.get("tx"):
                current=s["tx"]["boot_id"]
                if tx_boot and current!=tx_boot:
                    reject("TX reboot during run",{"boot_id":tx_boot},{"boot_id":current})
                tx_boot=current
        observation_gaps=[b["mono"]-a["mono"] for a,b in zip(samples,samples[1:])]
        if any(not 0 < gap <= 2 for gap in observation_gaps):
            reject("observation gap",{"interval_seconds":"0 < interval <= 2"},
                   {"intervals_outside_contract":[gap for gap in observation_gaps if not 0 < gap <= 2]})
        for previous,current in zip(samples,samples[1:]):
            deltas(previous["node"],current["node"],COUNTERS)
            deltas(previous["rx"],current["rx"],("sequence",))
            deltas(previous["rx"]["transport"],current["rx"]["transport"],RX_COUNTERS)
        a,b=samples[0],samples[-1];duration=b["mono"]-a["mono"]
        wall_duration=b["t"]-a["t"]
        if abs(duration-120)>1 or abs(wall_duration-duration)>.05:
            reject("window/clock boundary invalid",{"monotonic_duration_seconds":"119..121",
                   "wall_monotonic_difference_seconds":"<=0.05"},
                   {"monotonic_duration_seconds":duration,"wall_duration_seconds":wall_duration,
                    "clock_difference_seconds":wall_duration-duration})
        ds=deltas(a["node"],b["node"],COUNTERS)
        denominator=ds["sequence_observations"]+ds["inferred_lost_frames"]
        if ds["accepted_csi_frames"]<=0 or denominator<=0:
            reject("no new RX4 data/denominator",{"accepted_csi_frames":">0","sequence_denominator":">0"},
                   {"accepted_csi_frames":ds["accepted_csi_frames"],"sequence_denominator":denominator})
        grid_accounting=check_grid_exclusions(samples,packets,c)
        transport=deltas(a["rx"]["transport"],b["rx"]["transport"],RX_COUNTERS)
        ingress_keys=("udp_datagrams_from_os","udp_queue_overflow_datagrams","udp_queue_stale_datagrams",
                      "udp_receive_errors","csi_datagrams_received","csi_frames_accepted_new",
                      "csi_frames_discarded","csi_queue_overflow_datagrams","csi_queue_stale_datagrams")
        ingress=deltas(a["server_ingress"],b["server_ingress"],ingress_keys) if "server_ingress" in a else None
        sounding=[];rx_times=[];rx128_times=[];sequences=[];timestamps=[];window_audit={}
        rx_packet_counts={"csi":0,"csi_64":0,"csi_128":0,"sync":0,"feature_state":0,"mesh_health":0}
        for packet in packets:
            if not a["t"]<=packet["t"]<b["t"]: continue
            data=packet["data"]
            if packet["port"]==4210:
                if packet["src"]!="192.168.4.1":
                    reject("unexpected source on TX sounding port",{"source_ip":"192.168.4.1"},
                           {"source_ip":packet["src"],"source_mac":packet["src_mac"],"port":packet["port"]})
                if packet["src_mac"]!=parse_mac(c["ap_mac"]).hex() or len(data)!=32 or data[:6]!=b"CSI_TX" or data[30:]!=b"\0\0":
                    reject("sounding identity/packet contract failed",
                           {"source_mac":parse_mac(c["ap_mac"]).hex(),"bytes":32,"magic":"CSI_TX","trailing_zero":True},
                           {"source_mac":packet["src_mac"],"bytes":len(data),"magic":data[:6].decode("ascii","replace"),
                            "trailing_zero":data[30:]==b"\0\0"})
                if run["phase"]=="before":
                    if data[6:8]!=b"\0\0" or data[16:]!=b"\0"*16:
                        reject("old sounding differs from reference",{"version":0,"bytes_16_to_31":"zero"},
                               {"version":list(data[6:8]),"bytes_16_to_31_zero":data[16:]==b"\0"*16})
                elif data[16:20]!=b"TX1\0" or data[6:8]!=b"\2\2" or data[28:30]!=bytes([6,50]) or int.from_bytes(data[20:28],"little")==0:
                    reject("v2 sounding fields invalid",{"version":[2,2],"marker":"TX1\\0","channel":6,"rate_hz":50,
                            "boot_id":"nonzero"},
                           {"version":list(data[6:8]),"marker":data[16:20].decode("ascii","replace"),
                            "channel":data[28],"rate_hz":data[29],"boot_id":int.from_bytes(data[20:28],"little")})
                if run["phase"]=="after" and int.from_bytes(data[20:28],"little")!=int(tx_boot,16):
                    reject("TX packet/status boot identity differs",{"boot_id":tx_boot},
                           {"boot_id":f"{int.from_bytes(data[20:28],'little'):016x}"})
                sounding.append(packet["t"])
                sequences.append(int.from_bytes(data[8:12],"little"))
                timestamps.append(int.from_bytes(data[12:16],"little"))
            elif packet["port"]==5005:
                if packet["src"]!=c["rx4_ip"]:
                    reject("unexpected source on RX4 CSI port",{"source_ip":c["rx4_ip"]},
                           {"source_ip":packet["src"],"source_mac":packet["src_mac"],"port":packet["port"]})
                kind=rx4_packet_kind(data,boot,binding)
                rx_packet_counts[kind]+=1
                if kind=="csi":
                    rx_times.append(packet["t"])
                    length=int.from_bytes(data[6:8],"little")
                    rx_packet_counts[f"csi_{length}"]+=1
                    if length==128:rx128_times.append(packet["t"])
                audit_packets([packet],c,run["phase"],boot,window_audit,tx_boot)
            else:
                reject("unexpected packet in filtered capture",{"ports":[4210,5005]},
                       {"port":packet["port"],"source_ip":packet.get("src")})
        tx_metric=gaps(sounding,a["t"],b["t"]);rx_metric=gaps(rx_times,a["t"],b["t"])
        if any(y<=x for x,y in zip(sequences,sequences[1:])) or any(y<x for x,y in zip(timestamps,timestamps[1:])):
            reject("unresolved TX sequence/time reset, duplicate or wrap",
                   {"sequence":"strictly increasing","timestamp":"nondecreasing"},
                   {"sequence_nonincreasing":any(y<=x for x,y in zip(sequences,sequences[1:])),
                    "timestamp_decreasing":any(y<x for x,y in zip(timestamps,timestamps[1:]))})
        if last_tx_sequence is not None and (sequences[0]<=last_tx_sequence or timestamps[0]<last_tx_ms):
            reject("TX reset/wrap between windows",{"first_sequence":f"> {last_tx_sequence}",
                   "first_timestamp":f">= {last_tx_ms}"},
                   {"first_sequence":sequences[0],"first_timestamp":timestamps[0]})
        last_tx_sequence,last_tx_ms=sequences[-1],timestamps[-1]
        rx128_metric=gaps(rx128_times,a["t"],b["t"])
        if rx128_metric["max_gap"]>=2:
            reject("RX4 128 capture freshness gap",{"maximum_gap_seconds":"<2"},
                   {"maximum_gap_seconds":rx128_metric["max_gap"]})
        if rx_metric["max_gap"]>=2:
            reject("RX4 freshness gap",{"maximum_gap_seconds":"<2"},{"maximum_gap_seconds":rx_metric["max_gap"]})
        metrics={"sounding_rate":tx_metric["rate"],"rx4_rate":ds["accepted_csi_frames"]/duration,
                 "sounding_interval_p95":tx_metric["interval_p95"],"sounding_max_gap":tx_metric["max_gap"],
                 "rx4_max_gap":rx128_metric["max_gap"],"sequence_gap_ratio":ds["inferred_lost_frames"]/denominator}
        for k in ("queue_full_drops","retries","errors","abandoned","socket_rebuilds","socket_send_failures","enomem_backoffs"):
            metrics["rx4_"+k]=transport[k]
        tx_deltas = deltas(a["tx"],b["tx"],("sent_packets","send_errors","missed_deadlines","socket_rebuilds","wifi_restarts")) if a.get("tx") else None
        results.append({"tx_deltas":tx_deltas,"sounding_intervals":tx_metric,
                        "server_ingress_global_deltas":ingress,
                        "sounding_sequence":[sequences[0],sequences[-1]],"sounding_boot_ms":[timestamps[0],timestamps[-1]],
                        "rx4_sequence_delta":deltas(a["rx"],b["rx"],("sequence",))["sequence"],
                        "rx4_queue_depth":[a["rx"]["transport"]["queue_depth"],b["rx"]["transport"]["queue_depth"]],
                        "rx4_rssi_dbm":[a["rx"]["rssi_dbm"],b["rx"]["rssi_dbm"]],
                        "duration":duration,"metrics":metrics,"server_deltas":ds,"sequence_denominator":denominator,
                        "transport_deltas":transport,"captured_sounding":len(sounding),"captured_rx4":len(rx_times),
                        "rx4_packet_counts":rx_packet_counts,"grid_accounting":grid_accounting,
                        "rx4_transport_intervals":rx_metric,"rx4_128_intervals":rx128_metric,
                        "rx4_last_seen_ms_max":max(s["node"]["last_seen_ms"] for s in samples),
                        "transport_sequence":{k:v for k,v in window_audit.items() if "sequence" in k},
                        "transport_sequence_denominator":window_audit.get("rx_sequence_observations",0)+window_audit.get("rx_sequence_missing",0),
                        "sequence_gap_scope":"server accepted 128; includes valid 64 exclusions, not radio loss",
                        "sig_mode":"not stored; no per-packet Non-HT/HT attribution"})
    return results


def freeze_limits(results,c):
    return {"schema":1,"config_sha256":digest(canonical(c)),"rule":"old-three-window-envelope",
            "warmup_seconds":60,"window_seconds":120,"window_count":3,
            "limits":{k:{"direction":"min" if k in ("sounding_rate","rx4_rate") else "max",
                          "value":(min if k in ("sounding_rate","rx4_rate") else max)(r["metrics"][k] for r in results)}
                      for k in results[0]["metrics"]}}


def compare(results,limits,c):
    if limits.get("config_sha256")!=digest(canonical(c)) or limits.get("rule")!="old-three-window-envelope":
        raise ValueError("limits belong to a different contract")
    if set(limits["limits"])!=set(results[0]["metrics"]): raise ValueError("missing comparison limits")
    if (limits.get("schema"),limits.get("warmup_seconds"),limits.get("window_seconds"),limits.get("window_count")) != (1,60,120,3):
        raise ValueError("comparison protocol differs")
    failures=[]
    for i,result in enumerate(results):
        for key,value in result["metrics"].items():
            limit=limits["limits"][key]
            if limit["direction"] != ("min" if key in ("sounding_rate","rx4_rate") else "max") or not math.isfinite(limit["value"]) or limit["value"]<0:
                raise ValueError("invalid comparison direction/value")
            if (limit["direction"]=="min" and value<limit["value"]) or (limit["direction"]=="max" and value>limit["value"]):
                failures.append({"window":i+1,"metric":key,"value":value,"limit":limit})
    return failures


def runtime_config_from_plan(c,config_raw,limits_raw,plan_path):
    """Translate only runtime identity; preserve the frozen comparison contract."""
    from tx1_usb import artifact, checked, validate_plan
    plan=json.loads(read_private(plan_path))
    validate_plan(plan)
    if plan.get("fixture") or plan.get("measurement_config") is None:
        raise ValueError("runtime identity requires a real plan with its frozen configuration")
    if checked(plan["measurement_config"])!=config_raw or checked(plan["limits"])!=limits_raw:
        raise ValueError("measurement configuration/limits differ from the bound plan")
    receipt=json.loads(checked(plan["build_receipt"]))
    return dict(c,candidate_image_sha256=receipt["runtime_image_sha256"],
                candidate_source_sha256=receipt["source_sha256"]),artifact(plan_path)


def runtime_config_from_rx4(c,observed):
    """Bind a DHCP address to fresh RX4 identity without changing the frozen config."""
    address=observed["node"]["transport_ip"]
    if (str(ipaddress.ip_address(address))!=address
            or ipaddress.ip_address(address) not in ipaddress.ip_network("192.168.4.0/24")
            or address in ("192.168.4.0","192.168.4.1","192.168.4.50","192.168.4.255")
            or observed.get("rx_status_target_ip")!=address):
        raise ValueError("RX4 live address is not independently bound to its status target")
    runtime=dict(c,rx4_ip=address)
    check_sample(observed,runtime)
    return runtime


def append_private_jsonl(path,value):
    with path.open("a",encoding="utf-8") as output:
        output.write(json.dumps(value,sort_keys=True,separators=(",",":"))+"\n")


def live_collect(c,phase,out):
    begin=time.monotonic();deadline=begin+TOTAL_BUDGET
    connection=connection_check(c)
    if not connection["internet_route_is_separate"]:
        reject("Internet default route is not separate from CSI WiFi",
               {"internet_interface":connection["internet_interface"],"default_route":"separate"},
               {"default_interface":connection["default_interface"],"csi_interface":connection["csi_interface"]})
    write_private(out/"connection-route.json",canonical(connection))
    # Require the already-running bound server; never bind 5005 or start another server.
    pids=command(["/usr/sbin/lsof","-t","-nP",f"-iTCP:{c['server_port']}","-sTCP:LISTEN"]).split()
    if len(set(pids))!=1: raise ValueError("existing server is not unique")
    executable=command(["/usr/sbin/lsof","-a","-p",pids[0],"-d","txt","-Fn"])
    candidates=[Path(x[1:]) for x in executable.splitlines() if x.startswith("n") and Path(x[1:]).is_file()]
    if not any(digest(p.read_bytes())==c["server_binary_sha256"] for p in candidates):
        raise ValueError("server binary differs")
    current=None;ready_state={};stable_start=None;frozen_c=c
    while time.monotonic()-begin<READY_BUDGET:
        try:
            current=sample(c,phase)
            resolved=runtime_config_from_rx4(frozen_c,current)
            if ready_state and resolved["rx4_ip"]!=c["rx4_ip"]:
                ready_state={};stable_start=None
            c=resolved
            check_progress(current,c,ready_state)
            append_private_jsonl(out/"readiness-samples.jsonl",{"stage":"readiness","sample":current})
            if stable_start is None:stable_start=time.monotonic()
            if time.monotonic()-stable_start>=20:
                break
        except (ValueError,RuntimeError,subprocess.TimeoutExpired,KeyError) as error:
            append_private_jsonl(out/"readiness-errors.jsonl",{"reason":str(error),"sample":current})
            ready_state={};stable_start=None
        time.sleep(1)
    else:
        raise ValueError("readiness budget expired")
    pcap=out/"capture.pcap";errors=out/"capture.log"
    capture=None;capture_audit={};progress={};warmup_samples=[]
    run={"phase":phase,"warmup_seconds":60,"windows":[],"connection":connection,
         "runtime_rx4_ip":c["rx4_ip"]}
    try:
        with errors.open("x") as log:
            capture=subprocess.Popen(["/usr/sbin/tcpdump","-i",connection["csi_interface"],"-n","-U","-s","2048","-w",str(pcap),
                                      "udp and (dst port 4210 or dst port 5005)"],stdout=subprocess.DEVNULL,stderr=log)
            warmup_end=time.monotonic()+WARMUP
            boot=current["node"]["boot_id"]
            warmup_index=0
            while time.monotonic()<warmup_end:
                if capture.poll() is not None:
                    raise ValueError("packet capture unavailable; no privilege escalation inside run")
                current=sample(c,phase,current,warmup_index%5==0)
                warmup_samples.append(current)
                append_private_jsonl(out/"warmup-samples.jsonl",{"stage":"warmup","sample":current})
                check_progress(current,c,progress)
                audit_capture(pcap,c,phase,boot,capture_audit,observed_tx_boot(current,phase))
                warmup_index+=1
                wait=min(warmup_end,time.monotonic()+1)-time.monotonic()
                if wait>0:time.sleep(wait)
            current=sample(c,phase,current,True)
            warmup_samples.append(current)
            append_private_jsonl(out/"warmup-samples.jsonl",{"stage":"warmup_end","sample":current})
            check_progress(current,c,progress)
            audit_capture(pcap,c,phase,boot,capture_audit,observed_tx_boot(current,phase))
            warmup_packets=pcap_packets(pcap,allow_partial=True)
            write_private(out/"warmup-packet-audit.json",canonical(capture_audit))
            write_private(out/"warmup-grid-accounting.json",canonical(check_grid_exclusions(warmup_samples,warmup_packets,c)))
            run["warmup_samples"]=warmup_samples
            series_start=time.monotonic()
            for window_index in range(WINDOWS):
                samples=[current] if window_index else []
                start=series_start+window_index*WINDOW;misses=0
                for index in range(1 if window_index else 0,WINDOW+1):
                    wait=start+index-time.monotonic()
                    if wait>0:time.sleep(wait)
                    if time.monotonic()>deadline:
                        raise ValueError("overall observation time limit")
                    if capture.poll() is not None:
                        raise ValueError("packet capture stopped before window end")
                    try:
                        current=sample(c,phase,current,index%5==0)
                        check_progress(current,c,progress)
                        samples.append(current);misses=0
                        audit_capture(pcap,c,phase,boot,capture_audit,observed_tx_boot(current,phase))
                        append_private_jsonl(out/"samples.jsonl",current)
                    except MeasurementError as error:
                        append_private_jsonl(out/"samples-failed.jsonl",{
                            "stage":"window","window":window_index+1,"sample":current,"reason":str(error),
                            "expected":error.expected,"actual":error.actual})
                        raise
                    except (RuntimeError,subprocess.TimeoutExpired):
                        misses+=1
                        if misses>=2:raise ValueError("two missing diagnostic samples")
                    if index%5==0:connection_check(c)
                run["windows"].append({"samples":samples})
                write_private(out/f"window-{window_index+1}-observations.json",canonical({"samples":samples}))
                completed=evaluate_windows({"phase":phase,"windows":[{"samples":samples}]},c,
                                           pcap_packets(pcap,allow_partial=True))
                write_private(out/f"window-{window_index+1}-result.json",canonical(completed[0]))
            capture.send_signal(signal.SIGINT);capture.wait(timeout=5)
            audit_capture(pcap,c,phase,boot,capture_audit,observed_tx_boot(current,phase))
            write_private(out/"complete-packet-audit.json",canonical(capture_audit))
        stats=errors.read_text();m=re.search(r"(\d+) packets dropped by kernel",stats)
        run["capture_drops"]=int(m.group(1)) if m else None
        write_private(out/"observations.json",canonical(run))
        return run,pcap_packets(pcap),c
    except Exception as error:
        write_private(out/"failure-snapshot.json",canonical({"reason":str(error),
            "expected":getattr(error,"expected",None),"actual":getattr(error,"actual",None),"sample":current,
            "completed_windows":len(list(out.glob("window-*-result.json"))),"packet_audit":capture_audit}))
        raise
    finally:
        if capture and capture.poll() is None:
            capture.send_signal(signal.SIGINT)
            try:capture.wait(timeout=5)
            except subprocess.TimeoutExpired:capture.kill();capture.wait(timeout=2)
        if errors.is_file():
            match=re.search(r"(\d+) packets dropped by kernel",errors.read_text())
            run["capture_drops"]=int(match.group(1)) if match else None
        run["warmup_samples"]=warmup_samples
        run["packet_audit"]=capture_audit
        write_private(out/"observations-partial.json",canonical(run))


def main(argv=None):
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("--config",type=Path,required=True)
    p.add_argument("--output",type=Path,required=True,help="new private results directory")
    p.add_argument("--phase",choices=("before","after","connection"),required=True)
    p.add_argument("--fixture",type=Path,help="pure offline observations/packets or command outputs")
    p.add_argument("--start-observation",action="store_true",help="explicitly permit later read-only network/capture operations")
    p.add_argument("--limits",type=Path)
    p.add_argument("--limits-sha256",help="frozen BEFORE flash; mandatory for after")
    p.add_argument("--plan",type=Path,help="after only: verified USB plan binding the frozen file identity to its runtime digest")
    args=p.parse_args(argv)
    created=False;previous_alarm=None
    try:
        config_raw=read_private(args.config);c=json.loads(config_raw);validate_config(c)
        limits=None;runtime_c=c;plan_binding=None
        if args.plan and args.phase!="after":raise ValueError("runtime plan binding is only supported for after")
        if args.phase=="after":
            if not args.limits or not args.limits_sha256:raise ValueError("pre-frozen limits required")
            raw=read_private(args.limits)
            if digest(raw)!=args.limits_sha256:raise ValueError("limit artifact changed")
            limits=json.loads(raw)
            if args.plan:runtime_c,plan_binding=runtime_config_from_plan(c,config_raw,raw,args.plan)
        if not args.fixture and not args.start_observation:raise ValueError("default start performs no observation")
        os.umask(0o077);args.output.mkdir(mode=0o700,parents=False,exist_ok=False)
        created=True
        if args.fixture:
            fixture=json.loads(args.fixture.read_text())
            if args.phase=="connection":
                result=connection_check(c,lambda command:fixture["commands"][" ".join(command)])
                write_private(args.output/"result.json",canonical(result));return PASS if result["internet_route_is_separate"] else UNAVAILABLE
            run=fixture["run"];packets=[dict(v,data=bytes.fromhex(v["data"])) for v in fixture["packets"]]
        elif args.phase=="connection":
            result=connection_check(c);write_private(args.output/"result.json",canonical(result));return PASS if result["internet_route_is_separate"] else UNAVAILABLE
        else:
            def deadline_expired(signum,frame):raise TimeoutError("absolute 630-second observation budget")
            previous_alarm=signal.signal(signal.SIGALRM,deadline_expired);signal.alarm(TOTAL_BUDGET-7)
            run,packets,runtime_c=live_collect(runtime_c,args.phase,args.output)
        if run["phase"]!=args.phase:raise ValueError("phase mismatch")
        results=evaluate(run,runtime_c,packets)
        failures=compare(results,limits,c) if limits else []
        if args.phase=="before":write_private(args.output/"limits.json",canonical(freeze_limits(results,c)))
        result={"config_sha256":digest(canonical(c)),"old_manifest_sha256":c["old_manifest_sha256"],
                "candidate_image_sha256":c["candidate_image_sha256"],"candidate_source_sha256":c["candidate_source_sha256"],
                "conditions_sha256":c["conditions_sha256"],
                "source_mac_sha256":digest(parse_mac(c["ap_mac"])),"phase":args.phase,"status":"not_passed" if failures else "passed","windows":results,"failures":failures,
                "scientific_validation":False,"rx1_rx2_rx3":"open","fixture":bool(args.fixture)}
        if "runtime_rx4_ip" in run:result["runtime_rx4_ip"]=runtime_c["rx4_ip"]
        if plan_binding:
            result["runtime_identity_plan"]=plan_binding
            result["candidate_runtime_image_sha256"]=runtime_c["candidate_image_sha256"]
            result["candidate_runtime_source_sha256"]=runtime_c["candidate_source_sha256"]
        write_private(args.output/"result.json",canonical(result))
        return REGRESSION if failures else PASS
    except (OSError,ValueError,KeyError,TypeError,RuntimeError,subprocess.SubprocessError) as error:
        if created and not (args.output/"result.json").exists():
            try:write_private(args.output/"result.json",canonical({"status":"unclear","reason":str(error),"phase":args.phase}))
            except OSError:pass
        print("Observation refused/incomplete; see private result. No configuration was changed.",file=sys.stderr)
        return UNCLEAR
    finally:
        if previous_alarm is not None:
            signal.alarm(0);signal.signal(signal.SIGALRM,previous_alarm)


if __name__=="__main__":raise SystemExit(main())

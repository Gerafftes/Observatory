#!/usr/bin/env python3
"""Bounded RX qualification/capture helper. Plan mode performs no device/network I/O.

Device operations require --execute-authorized and a separately approved fixed
deadline. This tool grants no hardware permission, flashes nothing, changes no
NVS/network configuration and starts/stops no server process.
"""
import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import threading
import time
import urllib.request
import urllib.parse
import uuid
import sys
sys.path.insert(0,str(Path(__file__).resolve().parent))
import rx_pcap

_spec=importlib.util.spec_from_file_location("rx_provision",Path(__file__).resolve().parents[1]/"firmware/esp32-csi-node/provision.py")
provision=importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(provision)
PHASE_SECONDS={"short":30,"baseline":180,"pair":300,"continuous":3600,
               "functional-baseline":30,"functional-pair":60}


def need(condition, reason):
    if not condition:
        raise ValueError(reason)


def load(path):
    return json.loads(Path(path).read_text(),object_pairs_hook=provision.strict_json_pairs)


def save(path,value):
    with Path(path).open("x",encoding="utf-8") as output:
        json.dump(value,output,sort_keys=True,indent=2)
        output.write("\n")


def validate(plan):
    need(plan.get("schema")=="observatory.rx-measurement-plan.v1","measurement plan schema")
    need(plan.get("phase") in PHASE_SECONDS,"known bounded phase required")
    need([n.get("node_id") for n in plan["nodes"]]==[1,2,3,4],"exactly RX1..RX4 in order")
    server=urllib.parse.urlsplit(plan["server_url"])
    need(server.scheme=="http" and server.hostname in ("127.0.0.1","localhost","::1")
         and not server.username and not server.password and server.path in ("","/"),"server must be an explicit loopback origin")
    for key in ("server_sha256","firmware_image_sha256","firmware_source_sha256"):
        need(len(plan[key])==64 and all(c in "0123456789abcdef" for c in plan[key]),"invalid bound SHA-256")
    need(type(plan["server_pid"]) is int and plan["server_pid"]>0,"actual server PID required")
    need(Path(plan["recordings_dir"]).is_absolute(),"explicit external server recording directory required")
    need(plan["recording_id"] and all(c in "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_" for c in plan["recording_id"]),"safe recording ID required")
    for node in plan["nodes"]:
        url=urllib.parse.urlsplit(node["url"])
        need(url.scheme in ("http","https") and url.hostname and not url.username and not url.password
             and url.path in ("","/") and not url.query and not url.fragment,"device origin")
        for key in ("device_id","config_sha256"):
            need(len(node[key])==64 and all(c in "0123456789abcdef" for c in node[key]),"physical/profile SHA-256 required")
        mode=node["mode_id"]
        expected=1 if plan["phase"] in ("pair","functional-pair") and node["node_id"] in (1,4) else 0
        need(node["rate_hz"]==50 and (mode==expected if plan["phase"]!="continuous" else mode in (0,3,4)),"phase requires exact configured profile and 50 Hz")
        need(type(node["generation"]) is int and node["generation"]>0 and node["token_env"],"generation/token environment name required")
    need(type(plan["udp_port"]) is int and 1<=plan["udp_port"]<=65535 and plan["csi_host_ip"],"explicit existing CSI host/UDP destination required")
    return plan


class Transport:
    def __init__(self,journal,clock=time,deadline=None):
        self.journal=journal
        self.clock=clock
        self.lock=threading.Lock()
        self.deadline=deadline
        self.deadline_mono=None if deadline is None else clock.monotonic_ns()+int((deadline-clock.time())*1e9)

    def request(self,origin,path,body=None,node=None):
        closing=body is not None and (path=="/api/v1/recording/stop" or (path=="/measurement" and body.get("seconds")==0))
        if self.deadline is not None and not closing:
            need(self.clock.time()<self.deadline and self.clock.monotonic_ns()<self.deadline_mono,"original monotonic/UTC authorization deadline exhausted")
        headers={"Content-Type":"application/json"}
        if node is not None:
            token=os.environ.get(node["token_env"],"")
            need(token and "\r" not in token and "\n" not in token,"existing bearer token missing")
            headers.update({"Authorization":"Bearer "+token,"X-RX-Device-ID":node["device_id"]})
        data=None if body is None else json.dumps(body,allow_nan=False).encode()
        request=urllib.request.Request(origin.rstrip("/")+path,data=data,headers=headers)
        begin=self.clock.monotonic_ns()
        try:
            with urllib.request.build_opener(urllib.request.ProxyHandler({}),provision.NoRedirect()).open(request,timeout=10) as response:
                raw=response.read(65537)
            need(len(raw)<=65536,"response exceeds bound")
            result=json.loads(raw,object_pairs_hook=provision.strict_json_pairs)
            need(isinstance(result,dict),"JSON object response required")
        except (OSError,ValueError) as error:
            with self.lock:
                self.journal.write(json.dumps({"node_id":None if node is None else node["node_id"],"path":path,
                    "method":"GET" if body is None else "POST","begin_ns":begin,"end_ns":self.clock.monotonic_ns(),
                    "outcome":"failed_or_unknown","error_type":type(error).__name__})+"\n")
                self.journal.flush()
            raise RuntimeError("Request failed/ambiguous; no automatic retry: "+path) from error
        end=self.clock.monotonic_ns()
        with self.lock:
            self.journal.write(json.dumps({"node_id":None if node is None else node["node_id"],
                "path":path,"method":"GET" if body is None else "POST","begin_ns":begin,"end_ns":end,"response":result},sort_keys=True)+"\n")
            self.journal.flush()
        return result,begin,end


def preflight(io,plan,node,critical_diag=None):
    get=lambda path:io.request(node["url"],path,node=node)[0]
    caps=get("/capabilities")
    need(caps.get("schema")=="observatory.rx-capabilities.v1" and caps.get("wire_version")==4
         and caps.get("device_id")==node["device_id"],"device capability/physical identity mismatch")
    cfg=get("/config")
    need(cfg.get("schema")=="observatory.rx-config.v1" and cfg.get("active")==cfg.get("stored")
         and cfg.get("reboot_required") is False,"pending or unknown active configuration")
    active=cfg["active"]
    need(active.get("target_ip")==plan["csi_host_ip"] and active.get("target_port")==plan["udp_port"],"device is not targeting the declared existing host/port")
    for key in ("node_id","generation","config_sha256","mode_id","peer_ids","rate_hz"):
        need(active.get(key)==node[key],"active profile differs: "+key)
    status,status_begin,status_end=io.request(node["url"],"/ota/status",node=node)
    diag,diag_begin,diag_end=critical_diag if critical_diag is not None else io.request(node["url"],"/diag",node=node)
    need(status.get("image_sha256")==plan["firmware_image_sha256"] and status.get("ota_state")=="VALID", "running image/VALID mismatch")
    need(diag.get("schema")=="observatory.rx-diagnostic.v2" and diag.get("source_sha256")==plan["firmware_source_sha256"]
         and diag.get("complete") is True,"complete source-bound diagnostics required")
    need(int(status["boot_id"],16)==diag["boot_id"] and diag["boot_id"]!=0,"same nonzero boot required")
    resource=diag["resources"]
    need(resource["heap_internal_free_bytes"]>=32768 and resource["heap_internal_min_bytes"]>=32768
         and resource["heap_internal_largest_bytes"]>=16384,"internal heap gate")
    for field in ("http_stack_min_bytes","callback_stack_min_bytes","sender_stack_min_bytes"):
        need(resource[field]>=1024,"runtime stack gate: "+field)
    need(diag["allocation_failures"]==0 and diag["saturated"] is False and diag["reconnect"]["saturated"] is False,"allocation/saturation gate")
    c=diag["callback"]; t=diag["transport"]
    need(c["received"]==sum(c[k] for k in ("invalid","foreign","rate_limited","admitted"))
         and c["admitted"]==c["copied"]+c["handoff_drops"],"callback accounting")
    need(t["captured"]==sum(t[k] for k in ("waiting","in_flight","aborted","serialize_errors","os_accepted","abandoned")),"transport accounting")
    for link in diag["links"]:
        need(link["captured"]==sum(link[k] for k in ("queued","os_accepted","abandoned","serialize_errors","aborted")),"reserved per-link capture accounting")
    need(diag["led"]["qualified_frames"]>0 and t["os_accepted"]>0,"fresh eligible producer CSI required; this is not a host/RF acceptance")
    return {"node_id":node["node_id"],"caps":caps,"config":cfg,"status":status,"diag":diag,
            "status_begin_ns":status_begin,"status_end_ns":status_end,"diag_begin_ns":diag_begin,"diag_end_ns":diag_end}


def wait(clock,until,deadline):
    while clock.monotonic_ns()<until:
        need(clock.time()+150<deadline,"fixed authorization deadline reserve exhausted")
        clock.sleep(min(30,(until-clock.monotonic_ns())/1e9))


def host_clock_sample(io,plan):
    value,begin,end=io.request(plan["server_url"],"/api/v1/system/clock")
    need(all(type(value.get(key)) is int and value[key]>=0 for key in ("host_monotonic_ns","host_unix_ns"))
         and value.get("clock_epoch_id") and end>=begin,"actual native host clock sample required")
    return {"server":value,"request_begin_ns":begin,"response_end_ns":end}


def clock_proof(before,after,epoch):
    samples=(before,after)
    need(all(row["server"]["clock_epoch_id"]==epoch for row in samples),"host clock epoch changed")
    need(before["response_end_ns"]<=after["request_begin_ns"]
         and before["server"]["host_monotonic_ns"]<=after["server"]["host_monotonic_ns"],"host clock calibration chronology changed")
    low=max(row["server"]["host_monotonic_ns"]-row["response_end_ns"] for row in samples)
    high=min(row["server"]["host_monotonic_ns"]-row["request_begin_ns"] for row in samples)
    uncertainty=(high-low+1)//2+1000  # Conservative bound for microsecond PCAP quantization.
    need(low<=high and uncertainty<=50_000_000,"50-ms native/local clock mapping uncertainty failed")
    offsets=[row["server"]["host_unix_ns"]-row["server"]["host_monotonic_ns"] for row in samples]
    need(abs(offsets[1]-offsets[0])<=50_000_000,"50-ms host wall/monotonic drift failed")
    return {"clock_epoch_id":epoch,"adapter_to_server_offset_interval_ns":[low,high],
            "mapping_uncertainty_ns":uncertainty,"pcap_timestamp_quantization_ns":1000,
            "unix_offset_change_ns":offsets[1]-offsets[0],
            "calibrations":list(samples)}


def need_live_peer_session(snapshot):
    need(snapshot["diag"].get("profile",{}).get("peer_session_ended") is False,
         "finite peer session ended or unknown")


def qualify_node(io,plan,node,deadline,clock=time):
    peek=preflight(io,plan,node)
    trial=plan["phase"]=="pair" and node["node_id"] in (1,4)
    if trial: need_live_peer_session(peek)
    age=peek["diag"]["snapshot_end_us"]
    if not trial and plan["phase"] in ("short","baseline"):
        need(age<=850_000_000,"Q900 entry window missed; no reset is performed")
        wait(clock,clock.monotonic_ns()+max(0,810_000_000-age)*1000,deadline)
    first=preflight(io,plan,node)
    if trial: need_live_peer_session(first)
    if not trial and plan["phase"] in ("short","baseline"):
        need(800_000_000<=first["diag"]["snapshot_end_us"]<=850_000_000,"Q900 first window")
        delay=max(61_000_000,915_000_000-first["diag"]["snapshot_end_us"])
    else:
        delay=61_000_000
    wait(clock,clock.monotonic_ns()+delay*1000,deadline)
    last=preflight(io,plan,node)
    if trial: need_live_peer_session(last)
    need(first["diag"]["boot_id"]==last["diag"]["boot_id"] and first["config"]==last["config"]
         and last["status_begin_ns"]>=first["status_end_ns"]+60_200_000_000,"spread VALID/boot/config proof")
    need(last["diag"]["transport"]["os_accepted"]>first["diag"]["transport"]["os_accepted"],"producer counter did not advance")
    if not trial and plan["phase"] in ("short","baseline"):
        need(900_000_000<=last["diag"]["snapshot_end_us"]<=920_000_000,"Q900 final window")
    if trial:
        need(last["diag"]["snapshot_end_us"]+450_000_000<900_000_000,"pair has insufficient original boot-window reserve")
    return {"node_id":node["node_id"],"kind":"pair-ready" if trial else "Q900" if plan["phase"] in ("short","baseline") else "continuous-ready",
            "first":first,"last":last}


def qualify_functional(io,plan,deadline,clock=time):
    need(plan["phase"] in ("functional-baseline","functional-pair"),"explicit functional phase required")
    first=[preflight(io,plan,node) for node in plan["nodes"]]
    wait(clock,max(row["status_end_ns"] for row in first)+10_200_000_000,deadline)
    last=[preflight(io,plan,node) for node in plan["nodes"]]
    rows=[]
    for node,before,after in zip(plan["nodes"],first,last):
        need(before["diag"]["boot_id"]==after["diag"]["boot_id"] and before["config"]==after["config"],"functional boot/profile changed")
        need(after["status_begin_ns"]-before["status_end_ns"]>=10_200_000_000,"functional VALID spread missing")
        need(after["diag"]["transport"]["os_accepted"]>before["diag"]["transport"]["os_accepted"],"functional producer did not advance")
        for snapshot in (before,after):
            need(snapshot["diag"]["led"]["driver_ok"] and snapshot["diag"]["led"]["code"]=="NORMAL"
                 and snapshot["caps"]["measurement_active"] is False,"functional readiness not healthy/free")
            if node["mode_id"] in (1,2):need_live_peer_session(snapshot)
        rows.append({"node_id":node["node_id"],"kind":"functional-ready","first":before,"last":after})
    return {"schema":"observatory.rx-functional-readiness.v1","nodes":rows,"Q900_proven":False,"full_tag14_acceptance":False}


def collect(io,plan,qualification,deadline,clock=time,capture=None):
    seconds=PHASE_SECONDS[plan["phase"]]
    functional=plan["phase"] in ("functional-baseline","functional-pair")
    need(qualification.get("schema")==("observatory.rx-functional-readiness.v1" if functional else "observatory.rx-qualification.v1")
         and [row["node_id"] for row in qualification["nodes"]]==[1,2,3,4],"complete ordered qualification receipt required")
    need(clock.time()+seconds+180<deadline,"fixed deadline cannot fit phase and close reserve")
    build=io.request(plan["server_url"],"/api/v1/system/build")[0]
    need(build.get("schema")=="observatory.host-build.v1" and build.get("executable_sha256")==plan["server_sha256"]
         and build.get("process_id")==plan["server_pid"],"running native host identity mismatch")
    owner="rx-"+uuid.uuid4().hex
    armed=[]; started=False; closed=False; stop_attempted=False
    snapshots=[]; stop=None
    capture_started=False
    try:
        clock_before=host_clock_sample(io,plan)
        for node in plan["nodes"]:
            snapshot=preflight(io,plan,node)
            proof=next((row for row in qualification["nodes"] if row["node_id"]==node["node_id"]),None)
            kind="functional-ready" if functional else "pair-ready" if plan["phase"]=="pair" and node["node_id"] in (1,4) else "continuous-ready" if plan["phase"]=="continuous" else "Q900"
            need(proof is not None and proof["kind"]==kind,"qualification kind mismatch")
            first,last=proof["first"],proof["last"]
            if kind=="pair-ready" or functional and node["mode_id"] in (1,2):
                for observation in (first,last,snapshot): need_live_peer_session(observation)
            need(first["diag"]["boot_id"]==last["diag"]["boot_id"] and first["config"]==last["config"]
                 and last["status_begin_ns"]>=first["status_end_ns"]+(10_200_000_000 if functional else 60_200_000_000),"qualification VALID/boot spread is missing")
            if kind=="Q900":
                need(800_000_000<=first["diag"]["snapshot_end_us"]<=850_000_000
                     and 900_000_000<=last["diag"]["snapshot_end_us"]<=920_000_000,"qualification boot windows missing")
            need(proof is not None and proof["last"]["diag"]["boot_id"]==snapshot["diag"]["boot_id"]
                 and proof["last"]["config"]==snapshot["config"] and snapshot["diag"]["snapshot_end_us"]>=last["diag"]["snapshot_end_us"],"qualification boot/profile binding differs")
            need(snapshot["caps"].get("measurement_active") is False,"existing measurement lease is active")
            if node["mode_id"] in (1,2):
                need(snapshot["diag"]["snapshot_end_us"]+(seconds+150)*1_000_000<900_000_000,"trial boot window exhausted")
            snapshots.append(snapshot)
            armed.append(node)
            lease=io.request(node["url"],"/measurement",{"seconds":min(seconds+120,3600),"owner":owner},node)[0]
            need(lease.get("measurement_active") is True and lease.get("measurement_owner")==owner,"owned measurement lease not confirmed")
        if capture is not None:
            need(clock.time()+seconds+170<deadline,"capture cannot fit original phase/close reserve")
            capture.start(); capture_started=True
            wait(clock,clock.monotonic_ns()+10_000_000_000,deadline)
        need(clock.time()+seconds+150<deadline,"phase lost its original close reserve during preflight")
        started=True  # A lost start response may still have created our recorder.
        receipt,_,returned=io.request(plan["server_url"],"/api/v1/recording/start",
            {"id":plan["recording_id"],"max_duration_seconds":min(seconds+5,3600)})
        need(receipt.get("success") is True and receipt.get("recording_id")==plan["recording_id"],"recorder start not confirmed")
        started=True
        need(receipt["clock_epoch_id"]==clock_before["server"]["clock_epoch_id"],"native start clock epoch changed")
        offset_low=clock_before["server"]["host_monotonic_ns"]-clock_before["response_end_ns"]
        # The lower offset bound schedules no earlier than the native target;
        # a delayed start response must not extend the recording duration.
        end=receipt["started_at_monotonic_ns"]+seconds*1_000_000_000-offset_low
        need(returned<=end,"start response arrived after the fixed native end")
        if seconds==3600:
            wait(clock,returned+1800_000_000_000,deadline)
            for node in armed:
                io.request(node["url"],"/measurement",{"seconds":1920,"owner":owner},node)
        wait(clock,end,deadline)
        # Stop and four device end snapshots start together. The host's own
        # producer-close timestamp, not response time, is the end proof.
        def stop_once():
            return io.request(plan["server_url"],"/api/v1/recording/stop",{"recording_id":plan["recording_id"]})[0]
        stop_attempted=True
        with ThreadPoolExecutor(max_workers=5) as pool:
            future=pool.submit(stop_once)
            critical=list(pool.map(lambda node:io.request(node["url"],"/diag",node=node),plan["nodes"]))
            stop=future.result()
        with ThreadPoolExecutor(max_workers=4) as pool:
            ends=list(pool.map(lambda pair:preflight(io,plan,pair[0],critical_diag=pair[1]),zip(plan["nodes"],critical)))
        closed=stop.get("stopped") is True
        if not closed and stop.get("error")=="no recording in progress":
            # The 3600s server watchdog may win the explicit-stop race. Read
            # its actual closed metadata; do not send a second mutation.
            meta=load(Path(plan["recordings_dir"])/(plan["recording_id"]+".raw-csi.v1.meta.json"))
            need(meta.get("status")=="completed" and meta.get("recording_id")==plan["recording_id"]
                 and meta.get("incomplete") is False and meta.get("dropped_frames")==0 and meta.get("writer_error") is None,"watchdog metadata is not a complete close")
            stop={"stopped":True,"success":True,"recording_id":meta["recording_id"],"receipt_source":"closed_metadata",
                  "raw_sha256":meta["raw_sha256"],"closed_at":{"host_monotonic_ns":meta["ended_at_monotonic_ns"],"host_unix_ns":meta["ended_at_unix_ns"],"clock_epoch_id":meta["clock_epoch_id"]}}
            closed=True
        need(closed and stop.get("success") is True,"recorder closed incompletely/ambiguously")
        boundary=stop["closed_at"]
        need(boundary["clock_epoch_id"]==receipt["clock_epoch_id"],"host clock epoch changed")
        clamp=boundary["host_monotonic_ns"]-(receipt["started_at_monotonic_ns"]+seconds*1_000_000_000)
        need(0<=clamp<=50_000_000,"50-ms host producer end clamp failed")
        for before,after in zip(snapshots,ends):
            need(before["diag"]["boot_id"]==after["diag"]["boot_id"] and before["config"]==after["config"],"phase boot/config changed")
            need(after["caps"].get("measurement_active") is True and after["caps"].get("measurement_owner")==owner,"owned lease expired or changed before end")
        end_span=max(row["diag_end_ns"] for row in ends)-min(row["diag_begin_ns"] for row in ends)
        # The 50-ms contract bounds the real producer end and clock mapping.
        # Full device HTTP transfers are wider observations, not that end clock.
        capture_proof=None
        if capture is not None:
            wait(clock,clock.monotonic_ns()+10_000_000_000,deadline)
            capture_proof=capture.close(); capture_started=False
            need(capture_proof["complete"],"owned PCAP did not close completely")
        clock_after=host_clock_sample(io,plan)
        need(clock_before["server"]["host_monotonic_ns"]<=receipt["started_at_monotonic_ns"]
             and clock_after["server"]["host_monotonic_ns"]>=boundary["host_monotonic_ns"],"clock samples do not enclose native producer controls")
        mapping=clock_proof(clock_before,clock_after,receipt["clock_epoch_id"])
        return {"schema":"observatory.rx-phase-receipt.v1","phase":plan["phase"],"start":receipt,"stop":stop,
                "before":snapshots,"after":ends,"pcap":capture_proof,"clock_proof":mapping,
                "host_end_clamp_ns":clamp,"device_end_bracket_ns":end_span,
                "acceptance_scope":"functional-only" if functional else "phase-evidence-only","full_tag14_acceptance":False}
    finally:
        if capture_started:
            try: capture.close()
            except (OSError,RuntimeError,ValueError): pass
        if started and not closed and not stop_attempted:
            io.request(plan["server_url"],"/api/v1/recording/stop",{"recording_id":plan["recording_id"]})
        for node in armed:
            # Clearing is bound to our owner; another measurement cannot be released.
            try:
                io.request(node["url"],"/measurement",{"seconds":0,"owner":owner},node)
            except (RuntimeError,ValueError):
                pass  # The bounded RAM lease expires; never retry a mutation.


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action",choices=("plan","qualify","collect"))
    parser.add_argument("--plan",required=True,type=Path)
    parser.add_argument("--output",type=Path)
    parser.add_argument("--qualification",type=Path)
    parser.add_argument("--execute-authorized",action="store_true")
    parser.add_argument("--deadline-unix",type=float)
    args=parser.parse_args()
    plan=validate(load(args.plan))
    if args.action=="plan":
        print(json.dumps({"phase":plan["phase"],"duration_seconds":PHASE_SECONDS[plan["phase"]],
            "node_ids":[1,2,3,4],"hardware_access":False,"full_tag14_acceptance":False},indent=2))
        return
    need(args.execute_authorized and args.output and args.deadline_unix is not None,"separate live authorization, new output and fixed deadline required")
    need(time.time()<args.deadline_unix<=time.time()+21600,"fixed authorization end must be within six hours")
    args.output.mkdir(mode=0o700)
    with (args.output/"http.private.jsonl").open("x") as journal:
        io=Transport(journal,deadline=args.deadline_unix)
        if args.action=="qualify" and plan["phase"] in ("functional-baseline","functional-pair"):
            result=qualify_functional(io,plan,args.deadline_unix)
        elif args.action=="qualify":
            # Non-pair Qs use the baseline receipt after the approved pair restarts.
            nodes=plan["nodes"] if plan["phase"]!="pair" else [n for n in plan["nodes"] if n["node_id"] in (1,4)]
            with ThreadPoolExecutor(max_workers=4) as pool:
                rows=list(pool.map(lambda node:qualify_node(io,plan,node,args.deadline_unix),nodes))
            if plan["phase"]=="pair":
                need(args.qualification,"pair requires the bound baseline Q900 receipt for RX2/RX3")
                rows+= [row for row in load(args.qualification)["nodes"] if row["node_id"] in (2,3)]
            result={"schema":"observatory.rx-qualification.v1","nodes":sorted(rows,key=lambda row:row["node_id"])}
        else:
            need(args.qualification,"collect requires a qualification receipt")
            need(plan.get("pcap_interface") and type(plan.get("udp_port")) is int,"live capture requires explicit existing interface/UDP port")
            class OwnedCapture:
                child=None
                proof=None
                def start(self): self.child=rx_pcap.Capture(args.output,plan["pcap_interface"],plan["udp_port"],time)
                def close(self):
                    if self.proof is None:
                        self.proof=self.child.close()
                        save(args.output/"pcap-close.private.json",self.proof)
                    return self.proof
            result=collect(io,plan,load(args.qualification),args.deadline_unix,capture=OwnedCapture())
        save(args.output/"receipt.private.json",result)
    print("Bounded operation ended; receipt: "+str(args.output/"receipt.private.json"))


if __name__=="__main__":
    main()

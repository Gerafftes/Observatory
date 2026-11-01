"""Owned non-promiscuous tcpdump capture and read-only raw-wire accounting.

No interface configuration, monitor mode, resets, or device writes. Capture is
only called by an explicitly authorized phase; parsing itself opens no socket.
"""
from collections import Counter
import hashlib
from pathlib import Path
import re
import signal
import struct
import subprocess


def digest(path):
    with Path(path).open("rb") as source:
        return hashlib.file_digest(source,"sha256").hexdigest()


class Capture:
    def __init__(self,directory,interface,port,clock):
        if not re.fullmatch(r"[A-Za-z0-9_.-]{1,32}",interface) or not 1<=port<=65535:
            raise ValueError("explicit capture interface/UDP port required")
        self.path=Path(directory)/"wire.private.pcap"
        self.log=Path(directory)/"tcpdump.private.log"
        self.output=self.log.open("xb")
        self.process=subprocess.Popen(["/usr/sbin/tcpdump","-p","-n","-s","0","-U",
            "--time-stamp-precision=micro","-i",interface,"-w",str(self.path),"udp","dst","port",str(port)],
            stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=self.output)
        self.pid=self.process.pid
        clock.sleep(0.5)
        if self.process.poll() is not None:
            self.output.close()
            raise RuntimeError("owned tcpdump failed before recording; inspect its saved log")

    def close(self):
        if self.process.poll() is None:
            self.process.send_signal(signal.SIGINT)
        try:
            code=self.process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            self.process.terminate()
            try: code=self.process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                self.process.kill(); code=self.process.wait(timeout=3)
        self.output.close()
        text=self.log.read_text(errors="replace")
        dropped=re.search(r"(\d+) packets dropped by kernel",text)
        captured=re.search(r"(\d+) packets captured",text)
        complete=code==0 and dropped is not None and int(dropped[1])==0 and captured is not None
        return {"schema":"observatory.rx-pcap-close.v1","process_id":self.pid,"process_exit_code":code,
            "kernel_drops":int(dropped[1]) if dropped else None,"packets_captured":int(captured[1]) if captured else None,
            "sha256":digest(self.path) if self.path.exists() else None,"complete":complete}


def packets(path):
    with Path(path).open("rb") as source:
        header=source.read(24)
        variants={b"\xd4\xc3\xb2\xa1":("<",1000),b"\x4d\x3c\xb2\xa1":("<",1),
                  b"\xa1\xb2\xc3\xd4":(">",1000),b"\xa1\xb2\x3c\x4d":(">",1)}
        if len(header)!=24 or header[:4] not in variants: raise ValueError("classic PCAP header required")
        endian,scale=variants[header[:4]]
        _,_,_,_,snaplen,link=struct.unpack(endian+"HHIIII",header[4:])
        if link!=1 or snaplen>16*1024*1024: raise ValueError("Ethernet PCAP with bounded snapshot required")
        while True:
            record=source.read(16)
            if not record: break
            if len(record)!=16: raise ValueError("partial PCAP record header")
            sec,sub,length,original=struct.unpack(endian+"IIII",record)
            if length!=original or length>snaplen: raise ValueError("truncated/oversized captured packet")
            frame=source.read(length)
            if len(frame)!=length: raise ValueError("partial PCAP packet")
            yield sec*1_000_000_000+sub*scale,frame


def udp(frame):
    if len(frame)<14: raise ValueError("short Ethernet frame")
    offset=14; kind=int.from_bytes(frame[12:14],"big")
    for _ in range(2):
        if kind not in (0x8100,0x88a8): break
        if len(frame)<offset+4: raise ValueError("short VLAN header")
        kind=int.from_bytes(frame[offset+2:offset+4],"big"); offset+=4
    if kind!=0x0800: return None
    if len(frame)<offset+20: raise ValueError("short IPv4 header")
    size=(frame[offset]&15)*4
    if size<20 or frame[offset]>>4!=4 or len(frame)<offset+size+8: raise ValueError("invalid IPv4 header")
    if frame[offset+9]!=17: return None
    if int.from_bytes(frame[offset+6:offset+8],"big")&0x3fff: raise ValueError("fragmented UDP requires a separate complete reassembly proof")
    start=offset+size; length=int.from_bytes(frame[start+4:start+6],"big")
    ip_length=int.from_bytes(frame[offset+2:offset+4],"big")
    if length<8 or ip_length!=size+length or len(frame)<offset+ip_length: raise ValueError("invalid/truncated UDP datagram")
    return frame[start+8:start+length]


def normalized(payload):
    if len(payload)<172 or payload[:4]!=b"\x01\x00\x11\xc5": return None
    start=20+payload[5]*int.from_bytes(payload[6:8],"little")*2+40
    if len(payload) not in (start+112,start+144) or payload[start:start+8]!=b"\x61\xa0\x11\xc5\x04\x01\x70\x00": return None
    callback=int.from_bytes(payload[start+16:start+24],"little")
    dequeued=int.from_bytes(payload[start+80:start+88],"little")
    sent=int.from_bytes(payload[start+88:start+96],"little")
    if not 0<callback<=dequeued<=sent<=callback+100000: return None
    data=bytearray(payload); data[start+88:start+96]=b"\0"*8
    return hashlib.sha256(data).hexdigest()


def audit(path,close,native,start_unix_ns,end_unix_ns):
    if not close.get("complete") or close.get("kernel_drops")!=0 or digest(path)!=close.get("sha256"):
        raise ValueError("PCAP close/hash/drop proof failed")
    expected=set(native["wire_payload_sha256s"])
    retries=set(native["wire_retry_normalized_sha256s"])
    found=set(); counts=Counter(); interior_missing=0; boundary=0; total=0
    previous=None; first=None
    for stamp,frame in packets(path):
        if previous is not None and stamp<previous: raise ValueError("PCAP wall clock regression")
        previous=stamp; total+=1
        if first is None: first=stamp
        payload=udp(frame)
        if payload is None or payload[:4]!=b"\x01\x00\x11\xc5": continue
        wire=hashlib.sha256(payload).hexdigest(); identity=normalized(payload)
        counts[identity]+=1
        if wire in expected: found.add(wire)
        if start_unix_ns+50_000_000<=stamp<end_unix_ns-50_000_000:
            if wire not in expected and identity not in retries: interior_missing+=1
        else: boundary+=1
    if total!=close["packets_captured"]: raise ValueError("PCAP packet count differs from tcpdump close")
    return {"schema":"observatory.rx-pcap-accounting.v1","recorded_payloads":len(expected),
        "recorded_payloads_seen":len(found),"unexplained_interior_datagrams":interior_missing,
        "boundary_datagrams_separate":boundary,"retry_duplicate_datagrams":sum(max(0,count-1) for key,count in counts.items() if key in retries),
        "wire_accounting_pass":found==expected and interior_missing==0 and first is not None
            and first<=start_unix_ns-50_000_000 and previous>=end_unix_ns+50_000_000,
        "rf_loss_proven":False,"full_tag14_acceptance":False}


def main():
    import argparse
    import json
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pcap",required=True,type=Path)
    parser.add_argument("--close",required=True,type=Path)
    parser.add_argument("--native",required=True,type=Path)
    parser.add_argument("--phase-receipt",required=True,type=Path)
    parser.add_argument("--output",required=True,type=Path)
    args=parser.parse_args()
    read=lambda path:json.loads(path.read_text())
    native=read(args.native); phase=read(args.phase_receipt)
    if native["recording_id"]!=phase["start"]["recording_id"] or native["raw_sha256"]!=phase["stop"]["raw_sha256"]:
        raise ValueError("native/phase recorder hash binding differs")
    start=phase["start"]["started_at_unix_ns"]
    report=audit(args.pcap,read(args.close),native,start,start+native["duration_seconds"]*1_000_000_000)
    report["raw_window_gates_pass"]=native["raw_window_gates_pass"]
    with args.output.open("x") as output:
        json.dump(report,output,sort_keys=True,indent=2); output.write("\n")
    print("Read-only wire accounting ended: "+str(args.output))


if __name__=="__main__": main()

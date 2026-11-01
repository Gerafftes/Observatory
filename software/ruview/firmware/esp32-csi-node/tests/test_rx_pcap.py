"""PCAP framing/ledger against synthetic v4 datagrams; never captures a device."""
import importlib.util
import hashlib
from pathlib import Path
import struct
import tempfile
import unittest
from unittest.mock import patch

SPEC=importlib.util.spec_from_file_location("rx_pcap",Path(__file__).resolve().parents[3]/"scripts/rx_pcap.py")
pcap=importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(pcap)


def payload():
    packet=bytearray(20+256+40+112)
    packet[:4]=b"\x01\x00\x11\xc5"; packet[4]=1; packet[5]=1; packet[6:8]=(128).to_bytes(2,"little")
    start=20+256+40
    packet[start:start+8]=b"\x61\xa0\x11\xc5\x04\x01\x70\x00"
    for offset,value in [(16,1000000),(80,1000100),(88,1000200)]: packet[start+offset:start+offset+8]=value.to_bytes(8,"little")
    return packet


def ethernet(data):
    ip=bytearray(20); ip[0]=0x45; ip[2:4]=(20+8+len(data)).to_bytes(2,"big"); ip[9]=17
    udp=struct.pack("!HHHH",8032,5005,len(data)+8,0)+data
    return b"\x02"*12+b"\x08\x00"+ip+udp


class PcapTests(unittest.TestCase):
    def test_capture_requests_portable_microseconds_and_decodes_their_units(self):
        with tempfile.TemporaryDirectory() as directory:
            packet=ethernet(payload())
            raw=struct.pack('<IHHIIII',0xa1b2c3d4,2,4,0,0,65535,1)
            raw+=struct.pack('<IIII',11,123456,len(packet),len(packet))+packet
            class Child:
                pid=12345
                done=False
                def poll(self): return 0 if self.done else None
                def send_signal(self,_): self.done=True
                def wait(self,timeout): return 0
            class Clock:
                def sleep(self,_): pass
            def spawn(args,**kwargs):
                self.assertIn('--time-stamp-precision=micro',args)
                self.assertIn('-p',args)
                self.assertNotIn('-I',args)
                Path(args[args.index('-w')+1]).write_bytes(raw)
                kwargs['stderr'].write(b'1 packets captured\n0 packets dropped by kernel\n')
                return Child()
            with patch.object(pcap.subprocess,'Popen',side_effect=spawn):
                capture=pcap.Capture(directory,'en0',5005,Clock())
                proof=capture.close()
            self.assertTrue(proof['complete'])
            self.assertEqual(list(pcap.packets(capture.path)),[(11_123_456_000,packet)])

    def test_closed_packet_ledger_counts_retries_and_preserves_boundary_ambiguity(self):
        data=payload(); normal=pcap.normalized(data)
        retry=bytearray(data); retry[-112+88:-112+96]=(1000300).to_bytes(8,"little")
        self.assertEqual(normal,pcap.normalized(retry))
        with tempfile.TemporaryDirectory() as directory:
            path=Path(directory)/"wire.pcap"
            with path.open("wb") as output:
                output.write(struct.pack("<IHHIIII",0xa1b23c4d,2,4,0,0,65535,1))
                for second,packet in [(9,data),(11,data),(12,retry),(21,data)]:
                    frame=ethernet(packet); output.write(struct.pack("<IIII",second,0,len(frame),len(frame))); output.write(frame)
            close={"complete":True,"kernel_drops":0,"packets_captured":4,"sha256":pcap.digest(path)}
            native={"wire_payload_sha256s":[hashlib.sha256(data).hexdigest()],"wire_retry_normalized_sha256s":[normal]}
            report=pcap.audit(path,close,native,10_000_000_000,20_000_000_000)
            self.assertTrue(report["wire_accounting_pass"])
            self.assertFalse(report["rf_loss_proven"])
            self.assertEqual(report["retry_duplicate_datagrams"],3)
            wrong=dict(native,wire_payload_sha256s=["0"*64],wire_retry_normalized_sha256s=[])
            self.assertFalse(pcap.audit(path,close,wrong,10_000_000_000,20_000_000_000)["wire_accounting_pass"])
            close["kernel_drops"]=1
            with self.assertRaises(ValueError): pcap.audit(path,close,native,10_000_000_000,20_000_000_000)
    def test_truncated_udp_or_partial_pcap_is_rejected(self):
        with self.assertRaises(ValueError): pcap.udp(ethernet(payload())[:-1])
        with tempfile.TemporaryDirectory() as directory:
            path=Path(directory)/"short.pcap"; path.write_bytes(b"\xd4\xc3\xb2\xa1")
            with self.assertRaises(ValueError): list(pcap.packets(path))
        data=payload(); data[-112+88:-112+96]=(1100001).to_bytes(8,"little")
        self.assertIsNone(pcap.normalized(data))


if __name__=="__main__": unittest.main()

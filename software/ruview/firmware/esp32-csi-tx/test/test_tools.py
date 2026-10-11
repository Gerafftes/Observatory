lazy import copy
lazy import hashlib
lazy import importlib.util
lazy import io
lazy import json
lazy import os
lazy from pathlib import Path
lazy import struct
lazy import sys
lazy import tempfile
lazy import unittest
lazy from unittest.mock import patch
lazy import zlib
lazy from test_provision import profile, old_nvs, IDF

TOOLS=Path(__file__).resolve().parents[1]/"tools"
sys.path.insert(0,str(TOOLS))
lazy import tx1_measure as measure
lazy import tx1_usb as usb
lazy import build_release


def config():
    return dict(ap_mac="02:11:22:33:44:56",rx4_ip="192.168.4.3",rx4_image_sha256="a"*64,
                server_binary_sha256="b"*64,server_port=8080,hotspot_interface="en7",
                hotspot_hardware_port="iPhone USB",channel=6,rate_hz=50,old_manifest_sha256="c"*64,candidate_image_sha256="d"*64,candidate_source_sha256="e"*64,conditions_sha256="f"*64)


def fixture(phase="before"):
    c=config();run={"phase":phase,"warmup_seconds":60,"capture_drops":0,"windows":[]};packets=[]
    for window in range(3):
        samples=[];start=1060+window*120
        for second in range(121):
            count=1+50*(window*120+second)
            skipped=2*window+sum(start+edge<start+second for edge in (25.5,75.5))
            n=dict(node_id=4,last_seen_ms=0,status="active",source_binding_attested=True,
                   filter_enforced=True,source_matched_filter=True,identity_valid=True,protocol_version=2,
                   boot_id="0000000000000001",accepted_csi_frames=count,sequence_observations=count,
                   inferred_lost_frames=0,skipped_grid_frames=skipped,transport_ip=c["rx4_ip"])
            rx=dict(node_id="RX4",protocol_version=2,boot_id=n["boot_id"],image_sha256=c["rx4_image_sha256"],ip=c["rx4_ip"],
                    sequence=count,rssi_dbm=-40,transport={"queue_depth":0,**{k:(count if k in ("sent","enqueued") else 0) for k in measure.RX_COUNTERS}})
            ingress={k:(count if k in ("udp_datagrams_from_os","csi_datagrams_received","csi_frames_accepted_new") else 0)
                     for k in measure.SERVER_INGRESS_COUNTERS}
            tx=None if phase=="before" else dict(boot_check=1,image_sha256=c["candidate_image_sha256"],source_sha256=c["candidate_source_sha256"],boot_id="0000000000000002",channel=6,rate_hz=50,
                      source_mac_sha256=measure.digest(measure.parse_mac(c["ap_mac"])),sent_packets=count,
                      send_errors=0,missed_deadlines=0,socket_rebuilds=0,wifi_restarts=0)
            sample_time=start+second
            samples.append(dict(t=sample_time,mono=60+window*120+second,observed_start_utc=sample_time-.005,
                                node=n,rx=rx,server_ingress=ingress,tx=tx))
        run["windows"].append({"samples":samples})
        rx_events=[(start+index/50+.001,128) for index in range(6000)]
        rx_events.extend((start+edge,64) for edge in (25.5,75.5))
        for sequence,(t,subcarriers) in enumerate(sorted(rx_events)):
            index=window*6002+sequence
            header=struct.pack("<IBBHIIbbBB",0xC5110001,4,1,subcarriers,2437,index,-40,-90,0,0)
            trailer=struct.pack("<IBBH",0xC511A060,1,7,40)+bytes.fromhex(measure.digest(measure.parse_mac(c["ap_mac"])))
            identity=struct.pack("<IBBHQ",0xC511A061,2,1,24,1)+bytes(8)
            packets.append(dict(t=t,port=5005,src=c["rx4_ip"],src_mac="021122334457",
                                data=header+bytes(2*subcarriers)+trailer+identity))
        for index in range(6000):
            t=start+index/50+.001
            data=bytearray(32);data[:6]=b"CSI_TX";struct.pack_into("<II",data,8,window*6000+index,(window*6000+index)*20)
            if phase=="after":data[6:8]=b"\2\2";data[16:19]=b"TX1";struct.pack_into("<Q",data,20,2);data[28:30]=bytes([6,50])
            packets.append(dict(t=t,port=4210,src="192.168.4.1",src_mac="021122334456",data=bytes(data)))
    packets.sort(key=lambda packet:packet["t"])
    run["warmup_samples"]=[copy.deepcopy(run["windows"][0]["samples"][0])]
    return c,run,packets


def private(path,data):
    path.write_bytes(data);path.chmod(0o600);return path


class MeasurementTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):cls.c,cls.observations,cls.packets=fixture()

    def test_old_new_windows_and_frozen_limits(self):
        with patch("socket.socket",side_effect=AssertionError("no network")),patch("subprocess.run",side_effect=AssertionError("no subprocess")):
            results=measure.evaluate(self.observations,self.c,self.packets)
            limits=measure.freeze_limits(results,self.c)
            malformed=copy.deepcopy(self.packets)
            legacy=next(p for p in malformed if p["port"]==4210)
            legacy_data=bytearray(legacy["data"]);legacy_data[16]=ord("T");legacy["data"]=bytes(legacy_data)
            with self.assertRaisesRegex(ValueError,"old sounding differs from reference"):
                measure.evaluate(self.observations,self.c,malformed)
            c,run,packets=fixture("after")
            after=measure.evaluate(run,c,packets)
            self.assertEqual(measure.compare(after,limits,c),[])
            after[1]["metrics"]["rx4_rate"]-=1
            self.assertEqual(measure.compare(after,limits,c)[0]["metric"],"rx4_rate")
            self.assertEqual(results[0]["sequence_denominator"],6000)
            self.assertEqual(results[0]["rx4_packet_counts"]["csi_64"],2)
            self.assertEqual(results[0]["rx4_packet_counts"]["csi_128"],6000)
            self.assertEqual(results[0]["grid_accounting"]["skipped_grid_frames"],2)
            self.assertEqual(results[0]["server_ingress_global_deltas"]["csi_frames_accepted_new"],6000)
            self.assertEqual(results[0]["metrics"]["rx4_max_gap"],results[0]["rx4_128_intervals"]["max_gap"])
            del limits["limits"]["rx4_rate"]
            with self.assertRaises(ValueError):measure.compare(after,limits,c)

    def test_runtime_plan_translation_preserves_frozen_config_and_limits(self):
        c,run,packets=fixture("after");runtime_sha="9"*64;runtime_source="8"*64
        before=copy.deepcopy(c)
        for sample in run["warmup_samples"]+[s for w in run["windows"] for s in w["samples"]]:
            sample["tx"]["image_sha256"]=runtime_sha
            sample["tx"]["source_sha256"]=runtime_source
        limits=measure.freeze_limits(measure.evaluate(self.observations,self.c,self.packets),c)
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);root.chmod(0o700)
            cfg=private(root/"config.json",measure.canonical(c))
            bounds=private(root/"limits.json",measure.canonical(limits))
            receipt=private(root/"receipt.json",measure.canonical({"runtime_image_sha256":runtime_sha,
                "source_sha256":runtime_source}))
            plan=private(root/"plan.json",measure.canonical({"fixture":False,
                "measurement_config":usb.artifact(cfg),"limits":usb.artifact(bounds),
                "build_receipt":usb.artifact(receipt)}))
            payload={"run":run,"packets":[dict(p,data=p["data"].hex()) for p in packets]}
            data=private(root/"fixture.json",measure.canonical(payload))
            args=["--config",str(cfg),"--phase","after","--limits",str(bounds),
                  "--limits-sha256",measure.digest(bounds.read_bytes()),"--fixture",str(data)]
            self.assertEqual(measure.main(args+["--output",str(root/"untranslated")]),measure.UNCLEAR)
            with patch.object(usb,"validate_plan") as validated:
                self.assertEqual(measure.main(args+["--plan",str(plan),"--output",str(root/"translated")]),measure.PASS)
                validated.assert_called_once()
            result=json.loads((root/"translated/result.json").read_text())
            self.assertEqual(result["config_sha256"],measure.digest(measure.canonical(c)))
            self.assertEqual(result["candidate_image_sha256"],c["candidate_image_sha256"])
            self.assertEqual(result["candidate_runtime_image_sha256"],runtime_sha)
            self.assertEqual(result["candidate_runtime_source_sha256"],runtime_source)
            self.assertEqual(result["runtime_identity_plan"],usb.artifact(plan))
            self.assertEqual(c,before)
            self.assertEqual(json.loads(bounds.read_text()),limits)
            with patch.object(usb,"validate_plan"):
                for config_raw,limits_raw in ((b"{}",bounds.read_bytes()),(cfg.read_bytes(),b"{}")):
                    with self.assertRaisesRegex(ValueError,"differ from the bound plan"):
                        measure.runtime_config_from_plan(c,config_raw,limits_raw,plan)
            with patch.object(usb,"validate_plan",side_effect=ValueError("tampered plan")):
                with self.assertRaisesRegex(ValueError,"tampered plan"):
                    measure.runtime_config_from_plan(c,cfg.read_bytes(),bounds.read_bytes(),plan)

    def test_stale_boot_reset_and_zero_denominator_rejected(self):
        for key,value in [("last_seen_ms",2000),("boot_id","0000000000000002"),("source_binding_attested",False)]:
            run=copy.deepcopy(self.observations);run["windows"][0]["samples"][1]["node"][key]=value
            with self.subTest(key=key),self.assertRaises(ValueError):measure.evaluate(run,self.c,self.packets)
        run=copy.deepcopy(self.observations)
        for sample in run["windows"][0]["samples"]:sample["node"]["sequence_observations"]=1
        with self.assertRaises(ValueError):measure.evaluate(run,self.c,self.packets)
        run=copy.deepcopy(self.observations);run["capture_drops"]=1
        with self.assertRaises(ValueError):measure.evaluate(run,self.c,self.packets)

    def test_runtime_rx4_dhcp_binding_preserves_limits_and_rejects_identity_drift(self):
        c,run,packets=fixture();original=copy.deepcopy(c);address="192.168.4.9"
        baseline=measure.evaluate(run,c,packets);limits=measure.freeze_limits(baseline,c)
        for s in run["warmup_samples"]+[s for w in run["windows"] for s in w["samples"]]:
            s["node"]["transport_ip"]=address;s["rx"]["ip"]=address;s["rx_status_target_ip"]=address
        for p in packets:
            if p["port"]==5005:p["src"]=address
        observed=run["warmup_samples"][0]
        runtime=measure.runtime_config_from_rx4(c,observed)
        self.assertEqual(measure.evaluate(run,runtime,packets),baseline)
        self.assertEqual(measure.compare(baseline,limits,c),[])
        self.assertEqual(c,original)
        for section,key,value in (("rx","image_sha256","f"*64),("rx","boot_id","0000000000000002"),
                                  ("rx","ip","192.168.4.8"),("node","last_seen_ms",2000),
                                  ("node","identity_valid",False),("node","transport_ip","192.168.4.1")):
            bad=copy.deepcopy(observed);bad[section][key]=value
            with self.subTest(key=key),self.assertRaises(ValueError):measure.runtime_config_from_rx4(c,bad)
        bad=copy.deepcopy(observed);bad["rx_status_target_ip"]="192.168.4.8"
        with self.assertRaises(ValueError):measure.runtime_config_from_rx4(c,bad)
        with self.assertRaises(ValueError):measure.check_sample(bad,runtime)
        bad=copy.deepcopy(observed);bad["node"]["transport_ip"]="192.168.4.8"
        with self.assertRaises(ValueError):measure.check_sample(bad,runtime)
        bad_packets=copy.deepcopy(packets)
        next(p for p in bad_packets if p["port"]==5005)["src"]=c["rx4_ip"]
        with self.assertRaises(ValueError):measure.evaluate(run,runtime,bad_packets)

    def test_rx4_gate_uses_rx4_binding_without_requiring_all_four_receivers(self):
        node=copy.deepcopy(self.observations["windows"][0]["samples"][0]["node"])
        rx=copy.deepcopy(self.observations["windows"][0]["samples"][0]["rx"])
        nodes={"nodes":[node],"source_binding_consistent_across_nodes":False}
        ingress={"csi_ingress":{"counters":{k:0 for k in measure.SERVER_INGRESS_COUNTERS}}}
        with patch.object(measure,"get_json",side_effect=[nodes,rx,ingress]):
            observed=measure.sample(self.c,"before")
        self.assertFalse(observed["aggregate_source_binding_consistent_across_nodes"])
        measure.check_sample(observed,self.c)
        observed["node"]["source_matched_filter"]=False
        with self.assertRaises(ValueError):measure.check_sample(observed,self.c)

    def test_rx4_mixed_raster_grid_accounting_and_128_progress_are_independent(self):
        c,run,packets=fixture()
        broken=copy.deepcopy(run)
        broken["windows"][0]["samples"][-1]["node"]["skipped_grid_frames"]+=1
        with self.assertRaisesRegex(ValueError,"unexplained server grid exclusions"):
            measure.evaluate(broken,c,packets)

        rx=next(packet for packet in packets if packet["port"]==5005 and int.from_bytes(packet["data"][6:8],"little")==128)
        bad=copy.deepcopy(packets);bad_rx=next(p for p in bad if p["port"]==5005 and p["t"]==rx["t"])
        data=bytearray(bad_rx["data"]);data[19]=0x80;bad_rx["data"]=bytes(data)
        with self.assertRaisesRegex(ValueError,"raster/header"):
            measure.evaluate(run,c,bad)

        state={};first=copy.deepcopy(run["windows"][0]["samples"][0]);second=copy.deepcopy(first)
        first["mono"]=0;second["mono"]=2;second["t"]+=2;second["observed_start_utc"]+=2
        second["node"]["accepted_csi_frames"]=first["node"]["accepted_csi_frames"]
        with self.assertRaisesRegex(ValueError,"accepted 128 CSI progress stale"):
            measure.check_progress(first,c,state);measure.check_progress(second,c,state)

    def test_partial_pcap_is_accepted_only_for_incremental_capture_reads(self):
        header=struct.pack("<IHHIIII",0xa1b2c3d4,2,4,0,0,2048,1)
        with tempfile.TemporaryDirectory() as d:
            path=Path(d)/"partial.pcap";path.write_bytes(header+bytes(7))
            self.assertEqual(measure.pcap_packets(path,allow_partial=True),[])
            with self.assertRaisesRegex(ValueError,"truncated pcap record"):
                measure.pcap_packets(path)

    def test_v2_packet_and_source_mismatch(self):
        c,run,packets=fixture("after")
        sounding=next(packet for packet in packets if packet["port"]==4210)
        sounding["src_mac"]="021122334499"
        with self.assertRaises(ValueError):measure.evaluate(run,c,packets)
        sounding["src_mac"]="021122334456";data=bytearray(sounding["data"]);data[20]=3;sounding["data"]=bytes(data)
        with self.assertRaises(ValueError):measure.evaluate(run,c,packets)

    def test_pcap_parser_checks_real_wire_lengths(self):
        data=self.packets[0]["data"]
        ip=bytearray(20);ip[0]=0x45;ip[9]=17
        ip[12:16]=bytes([192,168,4,1]);ip[16:20]=bytes([192,168,4,255])
        struct.pack_into("!H",ip,2,28+len(data))
        frame=bytes.fromhex("ffffffffffff0211223344560800")+ip+struct.pack("!HHHH",9999,4210,len(data)+8,0)+data
        header=struct.pack("<IHHIIII",0xa1b2c3d4,2,4,0,0,2048,1)
        record=struct.pack("<IIII",1060,1000,len(frame),len(frame))+frame
        with tempfile.TemporaryDirectory() as d:
            path=Path(d)/"fixture.pcap";path.write_bytes(header+record)
            self.assertEqual(measure.pcap_packets(path)[0]["data"],data)
            self.assertEqual(measure.pcap_packets(path)[0]["src_mac"],"021122334456")
            path.write_bytes(header+record[:-1])
            with self.assertRaises(ValueError):measure.pcap_packets(path)

    def test_live_orchestration_uses_only_injected_io_and_fixed_windows(self):
        now=[1000.0];capture_calls=[]
        class Capture:
            stopped=False
            def poll(self):return 0 if self.stopped else None
            def send_signal(self,sig):self.stopped=True;self.log.write("0 packets dropped by kernel\n");self.log.flush()
            def wait(self,timeout):return 0
        capture=Capture()
        def spawn(args,**kw):capture_calls.append(args);capture.log=kw["stderr"];return capture
        def sample(c,phase,cached=None,refresh=True):
            s=copy.deepcopy(self.observations["windows"][0]["samples"][0]);s["t"]=s["mono"]=now[0]
            s["node"]["transport_ip"]=s["rx"]["ip"]=s["rx_status_target_ip"]="192.168.4.9"
            if now[0]<1003:s["node"]["last_seen_ms"]=2000
            s["node"]["accepted_csi_frames"]=int(now[0]*50);return s
        with tempfile.TemporaryDirectory() as d:
            out=Path(d);out.chmod(0o700);binary=out/"server";binary.write_bytes(b"fixture")
            c=dict(self.c,server_binary_sha256=measure.digest(b"fixture"))
            def command(args):return "42\n" if "-sTCP:LISTEN" in args else "n"+str(binary)+"\n"
            with patch.object(measure,"connection_check",return_value={"csi_interface":"en0","internet_route_is_separate":True,"internet_interface":"en7","default_interface":"en7"}),patch.object(measure,"command",side_effect=command),patch.object(measure,"sample",side_effect=sample),patch.object(measure,"pcap_packets",return_value=[]),patch.object(measure,"evaluate_windows",return_value=[{"metrics":{}}]),patch.object(measure.subprocess,"Popen",side_effect=spawn),patch.object(measure.time,"monotonic",side_effect=lambda:now[0]),patch.object(measure.time,"sleep",side_effect=lambda t:now.__setitem__(0,now[0]+t)),patch("socket.socket",side_effect=AssertionError("no network")):
                run,_,runtime=measure.live_collect(c,"before",out)
            self.assertEqual(runtime["rx4_ip"],"192.168.4.9")
            self.assertEqual(c["rx4_ip"],self.c["rx4_ip"])
            self.assertEqual([len(w["samples"]) for w in run["windows"]],[121]*3)
            self.assertEqual(run["windows"][1]["samples"][0],run["windows"][0]["samples"][-1])
            self.assertEqual(now[0],1443)
            self.assertEqual(len(capture_calls),1);self.assertEqual(capture_calls[0][0],"/usr/sbin/tcpdump")

    def test_readonly_two_interface_connection_fixtures(self):
        replies={
            "/usr/sbin/networksetup -listallhardwareports":"Hardware Port: Wi-Fi\nDevice: en0\nHardware Port: iPhone USB\nDevice: en7\n",
            "/usr/sbin/networksetup -getairportnetwork en0":"Current Wi-Fi Network: csi-test\n",
            "/sbin/route -n get 192.168.4.1":"interface: en0\ngateway: link#4\n",
            "/sbin/route -n get default":"interface: en7\ngateway: 172.20.10.1\n",
            "/usr/sbin/ipconfig getpacket en0":"yiaddr = 192.168.4.2\nsubnet_mask (ip): 255.255.255.0\nrouter (ip_mult): {192.168.4.1}\nserver_identifier (ip): 192.168.4.1\n",
            "/usr/sbin/arp -n -i en0 192.168.4.1":"? (192.168.4.1) at 2:11:22:33:44:56 on en0 ifscope\n",
            "/usr/sbin/arp -an -i en0":"",
            "/sbin/ifconfig -a":"en0: flags=UP\n\tinet 192.168.4.2 netmask 0xffffff00\n\tinet 192.168.4.50 netmask 0xffffffff\nen7: flags=UP\n\tinet 172.20.10.2 netmask 0xfffffff0\n"}
        runner=lambda cmd:replies[" ".join(cmd)]
        result=measure.connection_check(self.c,runner)
        self.assertTrue(result["usb_hotspot_route"])
        self.assertTrue(result["internet_route_is_separate"])
        self.assertEqual(result["ssid_status"],"visible-csi-test")

        modern=dict(self.c)
        modern.pop("hotspot_interface");modern.pop("hotspot_hardware_port")
        modern.update(internet_interface="en7",internet_hardware_port="Ethernet",
                      criteria_sha256="1"*64,provision_sha256="2"*64,start_helper_sha256="3"*64)
        measure.validate_config(modern)
        lan_replies=dict(replies)
        lan_replies["/usr/sbin/networksetup -listallhardwareports"]="Hardware Port: Wi-Fi\nDevice: en0\nHardware Port: Ethernet\nDevice: en7\n"
        lan_runner=lambda cmd:lan_replies[" ".join(cmd)]
        lan=measure.connection_check(modern,lan_runner)
        self.assertEqual(lan["internet_interface"],"en7")
        self.assertEqual(lan["internet_hardware_port"],"Ethernet")
        self.assertTrue(lan["internet_route_is_separate"])
        lan_replies["/sbin/route -n get default"]="interface: en0\ngateway: 192.168.4.1\n"
        with self.assertRaisesRegex(ValueError,"bound LAN Internet interface"):
            measure.connection_check(modern,lan_runner)

        # If macOS hides the SSID while disconnected or probing the API fails,
        # the independent DHCP and gateway-MAC identity must still pass.
        replies["/usr/sbin/networksetup -getairportnetwork en0"]="You are not associated with a Wi-Fi network.\n"
        hidden=measure.connection_check(self.c,runner)
        self.assertEqual(hidden["ssid_status"],"not-exposed")
        self.assertEqual(hidden["wifi_identity"],"expected-gateway-mac")
        def api_unavailable(cmd):
            if " ".join(cmd)=="/usr/sbin/networksetup -getairportnetwork en0":
                raise RuntimeError("networksetup returned nonzero")
            return replies[" ".join(cmd)]
        self.assertEqual(measure.connection_check(self.c,api_unavailable)["ssid_status"],"not-exposed")

        replies["/usr/sbin/networksetup -getairportnetwork en0"]="Current Wi-Fi Network: guest\n"
        with self.assertRaises(ValueError):measure.connection_check(self.c,runner)
        replies["/usr/sbin/networksetup -getairportnetwork en0"]="Current Wi-Fi Network: csi-test\n"
        replies["/sbin/route -n get default"]="interface: en0\ngateway: 192.168.4.1\n"
        same_interface=measure.connection_check(self.c,runner)
        self.assertFalse(same_interface["usb_hotspot_route"])
        self.assertFalse(same_interface["internet_route_is_separate"])
        replies["/sbin/ifconfig -a"]=replies["/sbin/ifconfig -a"].replace("0xffffffff","0xffffff00")
        with self.assertRaises(ValueError):measure.connection_check(self.c,runner)

    def test_safe_default_and_fixture_cli(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);cfg=private(root/"c.json",json.dumps(self.c).encode())
            with patch("subprocess.run",side_effect=AssertionError("no process")),patch("socket.socket",side_effect=AssertionError("no network")):
                self.assertEqual(measure.main(["--config",str(cfg),"--output",str(root/"not-started"),"--phase","before"]),4)
                self.assertFalse((root/"not-started").exists())
                payload={"run":self.observations,"packets":[dict(p,data=p["data"].hex()) for p in self.packets]}
                f=root/"fixture.json";f.write_text(json.dumps(payload))
                self.assertEqual(measure.main(["--config",str(cfg),"--output",str(root/"before"),"--phase","before","--fixture",str(f)]),0)
            self.assertEqual((root/"before/result.json").stat().st_mode&0o777,0o600)
            self.assertTrue(json.loads((root/"before/result.json").read_text())["fixture"])


class FakeUSB:
    def __init__(self,data):self.data=bytearray(data);self.writes=[];self.boots=0;self.mismatch=False;self.identities=0;self.reads=0;self.identity=None
    def identify(self,expected):
        self.identities+=1
        self.identity={"factory_base_mac":expected["factory_base_mac"],"secure_version":0}
    def read(self,offset,size):
        self.reads+=1
        result=bytes(self.data[offset:offset+size])
        if self.mismatch and self.writes:self.mismatch=False;return b"\0"+result[1:]
        return result
    def write(self,offset,data):
        self.writes.append((offset,len(data)))
        end=offset+(len(data)+4095)//4096*4096
        self.data[offset:end]=data+b"\xff"*(end-offset-len(data))
    def boot(self):self.boots+=1


class USBTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.root=Path(self.temp.name)
        spec=importlib.util.spec_from_file_location("gen_part",IDF/"components/partition_table/gen_esp32part.py")
        module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
        tx=TOOLS.parent
        table=module.PartitionTable.from_csv((tx/"partitions.csv").read_text()).to_binary()
        raw=bytearray(b"\xff"*0x1000000);raw[0x8000:0x8000+len(table)]=table
        raw[0x9000:0xe000]=old_nvs()
        for offset,seq in ((0xe000,1),(0xf000,0)):
            struct.pack_into("<I",raw,offset,seq)
            struct.pack_into("<I",raw,offset+28,zlib.crc32(raw[offset:offset+4],0xffffffff)&0xffffffff)
        private(self.root/"flash-read-1.bin",raw)
        private(self.root/"flash-read-2.bin",raw)
        parts=usb.partitions(bytes(raw[0x8000:0x9000]))
        backup={"files":{"flash-read-1.bin":{"size":len(raw),"sha256":usb.sha(raw)},
                         "flash-read-2.bin":{"size":len(raw),"sha256":usb.sha(raw)}},
                "verification":{"status":"FIXTURE","readbacks_byte_equal":True},
                "partitions":[dict(name=k,offset=v["offset"],size_bytes=v["size"]) for k,v in parts.items()],
                "acquisition":{"factory_base_mac":"02:11:22:33:44:55","chip":"fixture"}}
        self.backup=private(self.root/"manifest.json",usb.canonical(backup))
        from esptool.bin_image import ESP32S3FirmwareImage, ImageSegment
        image=ESP32S3FirmwareImage();desc=bytearray(256);struct.pack_into("<I",desc,0,0xabcd5432);desc[64:128]=b"e"*64
        image.segments=[ImageSegment(0x3c000020,desc)];image.save(str(self.root/"esp32-csi-tx.bin"))
        app=(self.root/"esp32-csi-tx.bin").read_bytes()
        (self.root/"config").mkdir();(self.root/"partition_table").mkdir()
        private(self.root/"config/sdkconfig.json",usb.canonical(dict(TX_PROTECT_ORIGINAL_APP=True,ESPTOOLPY_FLASHSIZE_4MB=True,ESP_PHY_MAX_WIFI_TX_POWER=11,APP_REPRODUCIBLE_BUILD=True)))
        private(self.root/"partition_table/partition-table.bin",table)
        private(self.root/"esp32-csi-tx.elf",b"fixture-elf")
        receipt={"candidate":{"file":"esp32-csi-tx.bin","size":len(app),"sha256":usb.sha(app),"intended_offset":0x150000,"slot_size":0x140000},
                 "hardware_validated":False,"write_authorized":False,"source_sha256":"e"*64,"idf":"v5.4","led_strip":"3.0.3","reproducible_build":True,
                 "runtime_image_sha256":usb.app_image(app).calc_digest.hex(),
                 "artifacts":{name:usb.sha((self.root/name).read_bytes()) for name in ("esp32-csi-tx.bin","esp32-csi-tx.elf","config/sdkconfig.json","partition_table/partition-table.bin")}}
        private(self.root/"receipt.json",usb.canonical(receipt))
        private(self.root/"profile.json",json.dumps(profile(ssid="csi-test")).encode())
        c,r,packets=fixture();c["old_manifest_sha256"]=usb.sha(self.backup.read_bytes())
        c["candidate_image_sha256"]=usb.app_image(app).calc_digest.hex()
        results=measure.evaluate(r,c,packets);limits=measure.freeze_limits(results,c)
        baseline={"phase":"before","status":"passed","windows":results,"fixture":True,
                  "config_sha256":measure.digest(measure.canonical(c)),"old_manifest_sha256":c["old_manifest_sha256"],
                  "source_mac_sha256":measure.digest(measure.parse_mac(c["ap_mac"])),
                  "candidate_image_sha256":c["candidate_image_sha256"],"candidate_source_sha256":c["candidate_source_sha256"]}
        private(self.root/"baseline.json",usb.canonical(baseline));private(self.root/"limits.json",usb.canonical(limits))
        self.plan=usb.prepare(self.backup,self.backup,self.root/"receipt.json",self.root/"profile.json",self.root/"limits.json",self.root/"baseline.json",IDF,self.root/"plan",fixture=True)
        self.original=bytes(raw)
    def tearDown(self):self.temp.cleanup()

    def test_manifest_bound_migration_and_full_restore(self):
        device=FakeUSB(self.original)
        events=[]
        self.assertEqual(usb.perform(self.plan,device,"migrate",progress=lambda event:events.append((event,device.reads,len(device.writes)))),0)
        reads=[e for e,_,_ in events if e["stage"]=="pre-write-read"]
        self.assertEqual([e["offset"] for e in reads],list(range(0,0x1000000,0x40000)))
        self.assertEqual([e["completed_blocks"] for e in reads],list(range(64)))
        verified=next((e,reads,writes) for e,reads,writes in events if e["stage"]=="pre-write-read-complete")
        self.assertEqual(verified,({"stage":"pre-write-read-complete","matches_bound_backup":True},64,0))
        self.assertEqual(sum(e["stage"]=="post-write-read" for e,_,_ in events),64)
        self.assertEqual(events[-1][0],{"stage":"complete","boot_requested":False})
        self.assertEqual([o for o,n in device.writes],[0x150000,0x9000,0xe000])
        self.assertEqual(device.data[:0x9000],self.original[:0x9000])
        self.assertEqual(device.data[0x10000:0x150000],self.original[0x10000:0x150000])
        self.assertEqual(device.boots,0)
        self.assertEqual(usb.perform(self.plan,device,"restore",boot_after=True),0)
        self.assertEqual(bytes(device.data),self.original);self.assertEqual(device.boots,1)

    def test_refuse_changed_preimage_and_manifest(self):
        device=FakeUSB(self.original);device.data[10]=0
        readback_dir=self.root/"failed-read";readback_dir.mkdir()
        with self.assertRaisesRegex(ValueError,"device changed since bound backup"):
            usb.perform(self.plan,device,"migrate",readback_dir=readback_dir)
        self.assertEqual(device.writes,[])
        self.assertEqual((readback_dir/"flash-read-before-write.bin").read_bytes(),bytes(device.data))
        plan=copy.deepcopy(self.plan);plan["writes"][0]["offset"]=0x10000
        with self.assertRaises(ValueError):usb.perform(plan,FakeUSB(self.original),"migrate")
        with self.assertRaises(ValueError):usb.validate_clearance(self.plan,"a"*64,{"plan_sha256":"a"*64})

    def test_one_preapproved_restore_only(self):
        device=FakeUSB(self.original);device.mismatch=True
        self.assertEqual(usb.perform(self.plan,device,"migrate",allow_restore=True),6)
        self.assertEqual(device.writes,[(0x150000,self.plan["writes"][0]["artifact"]["size"]),(0,0x1000000)])
        self.assertEqual(bytes(device.data),self.original)

    def test_timeout_and_modified_nvs_cannot_trigger_more_writes(self):
        device=FakeUSB(self.original);times=iter([0,0,1300,1300])
        with self.assertRaises(TimeoutError):usb.perform(self.plan,device,"migrate",allow_restore=True,clock=lambda:next(times))
        self.assertEqual(device.writes,[])
        path=Path(self.plan["writes"][1]["artifact"]["path"])
        data=bytearray(path.read_bytes());data[-1]=0;path.write_bytes(data)
        plan=copy.deepcopy(self.plan);plan["writes"][1]["artifact"]=usb.artifact(path)
        with self.assertRaises(ValueError):usb.validate_plan(plan)

    def test_sector_comparison_allows_mutable_partition_and_stops_on_app0_change(self):
        old_table=usb.partitions(self.original[0x8000:0x9000])
        current=bytearray(self.original);current[0x290000+17]=0
        table=usb.partitions(current[0x8000:0x9000])
        report=usb.compare_flash_images(self.original,bytes(current),old_table,table)
        self.assertEqual(report["status"],"PASS")
        self.assertEqual(report["changed_by_region"]["spiffs"]["sectors"],1)
        current[0x10000+17]=0
        report=usb.compare_flash_images(self.original,bytes(current),old_table,table)
        self.assertEqual(report["status"],"HARD_STOP")
        self.assertFalse(report["critical_checks"]["app0"]["unchanged"])

    def test_unallocated_flash_difference_is_retained_for_classification(self):
        old_table=usb.partitions(self.original[0x8000:0x9000])
        current=bytearray(self.original);current[0x400000+1]=0
        report=usb.compare_flash_images(self.original,bytes(current),old_table,old_table)
        self.assertEqual(report["status"],"REVIEW_UNALLOCATED")
        self.assertEqual(report["changed_by_region"]["unallocated"]["sectors"],1)

    def test_capture_saves_two_full_readbacks_and_compares_without_writing(self):
        out=self.root/"snapshot"
        with patch.object(usb,"assert_private_path",side_effect=lambda p:Path(p).resolve()):
            result=usb.capture_pre_migration_backup(self.backup,out,backend_factory=lambda:FakeUSB(self.original))
        self.assertEqual(result["status"],"PASS")
        self.assertEqual((out/"flash-read-1.bin").read_bytes(),self.original)
        self.assertEqual((out/"flash-read-2.bin").read_bytes(),self.original)
        manifest=json.loads((out/"manifest.json").read_text())
        self.assertTrue(manifest["verification"]["readbacks_byte_equal"])
        self.assertEqual((out/"flash-comparison.json").stat().st_mode&0o777,0o600)

    def test_capture_can_hold_application_stopped_through_plan_preparation(self):
        for hold in (False,True):
            device=FakeUSB(self.original);events=[];out=self.root/f"snapshot-hold-{hold}"
            with patch.object(usb,"assert_private_path",side_effect=lambda p:Path(p).resolve()):
                result=usb.capture_pre_migration_backup(self.backup,out,backend_factory=lambda:device,
                    hold_in_bootloader=hold,progress=events.append)
            self.assertEqual(result["status"],"PASS")
            self.assertEqual(device.boots,0 if hold else 1)
            self.assertEqual(device.reads,128)
            self.assertEqual(device.writes,[])
            manifest=json.loads((out/"manifest.json").read_text())
            self.assertEqual(manifest["boot_requested"],not hold)
            self.assertEqual(manifest["hold_in_bootloader_requested"],hold)
            self.assertEqual(sum(e["stage"]=="flash-read-block" for e in events),128)
            self.assertEqual(events[-1],{"stage":"flash-read-saved","pass_index":2,"completed_blocks":64})

    def test_capture_retains_nonidentical_full_readbacks_for_review(self):
        class DriftingUSB(FakeUSB):
            def read(self,offset,size):
                if self.reads==64:
                    self.data[0x290001]^=1
                return super().read(offset,size)
        out=self.root/"snapshot-drift"
        with patch.object(usb,"assert_private_path",side_effect=lambda p:Path(p).resolve()):
            result=usb.capture_pre_migration_backup(self.backup,out,backend_factory=lambda:DriftingUSB(self.original))
        self.assertEqual(result["status"],"UNREPRODUCIBLE")
        first=(out/"flash-read-1.bin").read_bytes()
        second=(out/"flash-read-2.bin").read_bytes()
        self.assertEqual(len(first),0x1000000)
        self.assertEqual(len(second),0x1000000)
        self.assertNotEqual(first,second)
        manifest=json.loads((out/"manifest.json").read_text())
        self.assertEqual(manifest["verification"]["status"],"UNREPRODUCIBLE")
        comparison=json.loads((out/"flash-comparison.json").read_text())
        self.assertEqual(comparison["status"],"UNREPRODUCIBLE")

    def test_capture_failure_keeps_safe_phase_and_block_context(self):
        class FailingUSB(FakeUSB):
            def read(self,offset,size):raise OSError(6,"Device not configured")
        out=self.root/"snapshot-failed";status={}
        with patch.object(usb,"assert_private_path",side_effect=lambda p:Path(p).resolve()):
            with self.assertRaisesRegex(OSError,"Device not configured"):
                usb.capture_pre_migration_backup(self.backup,out,backend_factory=lambda:FailingUSB(self.original),status=status)
        self.assertEqual(status["failure_phase"],"flash-read-block")
        self.assertEqual(status["failure_type"],"OSError")
        self.assertEqual(status["failure_errno"],6)
        self.assertEqual((status["pass_index"],status["offset"],status["completed_blocks"]),(1,0,0))
        self.assertEqual(list(out.iterdir()),[])

    def test_capture_cli_persists_private_diagnostic_context(self):
        out=self.root/"capture-cli-failed"
        def fail_capture(reference_path,output,status,hold_in_bootloader=False,progress=None):
            Path(output).mkdir(mode=0o700)
            status.update(stage="flash-read-io",pass_index=1,offset=0,completed_blocks=0,
                          failure_phase="flash-read-io",failure_type="OSError",failure_errno=6)
            raise OSError(6,"Device not configured")
        with patch.object(usb,"assert_private_path",side_effect=lambda p:Path(p).resolve()), \
             patch.object(usb,"capture_pre_migration_backup",side_effect=fail_capture):
            result=usb.main(["capture","--reference-backup-manifest",str(self.backup),"--output",str(out),
                             "--start","--approve-reference-sha256",usb.sha(self.backup.read_bytes())])
        self.assertEqual(result,usb.FAILED)
        failure=json.loads((out/"failure.json").read_text())
        self.assertEqual(failure["capture_state"]["failure_errno"],6)
        self.assertEqual(failure["capture_state"]["offset"],0)
        self.assertEqual((out/"failure.json").stat().st_mode&0o777,0o600)

    def test_current_backup_with_mutable_drift_remains_valid_against_7a_history(self):
        current=bytearray(self.original);current[0x290000+31]=0
        current=bytes(current);current_dir=self.root/"current";current_dir.mkdir()
        private(current_dir/"flash-read-1.bin",current);private(current_dir/"flash-read-2.bin",current)
        historical=json.loads(self.backup.read_text())
        table=usb.partitions(current[0x8000:0x9000])
        diff=usb.compare_flash_images(self.original,current,table,table)
        diff_path=private(current_dir/"flash-comparison.json",usb.canonical(diff))
        current_backup={"reference_manifest_sha256":usb.sha(self.backup.read_bytes()),
            "acquisition":historical["acquisition"],
            "files":{"flash-read-1.bin":usb.artifact(current_dir/"flash-read-1.bin"),
                     "flash-read-2.bin":usb.artifact(current_dir/"flash-read-2.bin")},
            "partitions":historical["partitions"],"comparison":usb.artifact(diff_path),
            "verification":{"status":"PASS","readbacks_byte_equal":True}}
        current_manifest=private(current_dir/"manifest.json",usb.canonical(current_backup))
        plan=usb.prepare(current_manifest,self.backup,self.root/"receipt.json",self.root/"profile.json",
            self.root/"limits.json",self.root/"baseline.json",IDF,self.root/"drift-plan",fixture=True)
        self.assertEqual(plan["original"]["sha256"],usb.sha(current))
        self.assertEqual(plan["historical_backup_manifest"]["sha256"],usb.sha(self.backup.read_bytes()))
        self.assertEqual(plan["comparison"]["sha256"],usb.sha(diff_path.read_bytes()))

    def test_strict_whole_file_to_runtime_binding_preserves_frozen_baseline(self):
        app=(self.root/"esp32-csi-tx.bin").read_bytes()
        c=config();c["old_manifest_sha256"]=usb.sha(self.backup.read_bytes())
        c["candidate_image_sha256"]=usb.sha(app)
        c["candidate_source_sha256"]="e"*64
        config_path=private(self.root/"bound-measurement.json",measure.canonical(c))
        _,run,packets=fixture("before")
        windows=measure.evaluate(run,c,packets)
        limits=measure.freeze_limits(windows,c)
        baseline={"phase":"before","status":"passed","fixture":False,"failures":[],
            "windows":windows,"config_sha256":measure.digest(measure.canonical(c)),
            "old_manifest_sha256":c["old_manifest_sha256"],
            "source_mac_sha256":measure.digest(measure.parse_mac(c["ap_mac"])),
            "candidate_image_sha256":usb.sha(app),"candidate_source_sha256":c["candidate_source_sha256"],
            "conditions_sha256":c["conditions_sha256"]}
        baseline_path=private(self.root/"bound-original-result.json",usb.canonical(baseline))
        limits_path=private(self.root/"bound-limits.json",usb.canonical(limits))
        runtime_sha=usb.app_image(app).calc_digest.hex()
        translated=copy.deepcopy(baseline)
        translated["candidate_image_sha256"]=runtime_sha
        translated["binding_derivation"]={
            "schema":"tx1-7a-file-to-runtime-binding-v1",
            "BASELINE_ID":"TX1-7A-20260926T152253Z-MIXED-V2",
            "original_result":usb.artifact(baseline_path),
            "original_limits":usb.artifact(limits_path),
            "original_config":usb.artifact(config_path),
            "build_receipt":usb.artifact(self.root/"receipt.json"),
            "candidate_file_sha256":usb.sha(app),
            "candidate_runtime_image_sha256":runtime_sha,
            "changed_field":"candidate_image_sha256",
            "measurements_and_limits_changed":False,
            "new_baseline":False,
            "write_authorized":False,
            "reason":"unit fixture exercises the sealed identity translation"}
        binding_path=private(self.root/"baseline-runtime-binding.json",usb.canonical(translated))
        observed=usb.validate_baseline_runtime_binding(binding_path,baseline_path,limits_path,
            self.root/"receipt.json",baseline,limits,json.loads((self.root/"receipt.json").read_text()),
            app,json.loads((self.root/"profile.json").read_text()),usb.sha(self.backup.read_bytes()))
        self.assertEqual(observed,(usb.artifact(binding_path),usb.artifact(config_path)))

        plan=usb.prepare(self.backup,self.backup,self.root/"receipt.json",self.root/"profile.json",
            limits_path,baseline_path,IDF,self.root/"strict-plan",fixture=True,
            baseline_binding_path=binding_path)
        self.assertEqual(usb.validate_plan(plan),self.original)
        tampered=copy.deepcopy(translated);tampered["conditions_sha256"]="0"*64
        tampered_path=private(self.root/"baseline-runtime-binding-tampered.json",usb.canonical(tampered))
        with self.assertRaisesRegex(ValueError,"translation changed frozen measurements"):
            usb.validate_baseline_runtime_binding(tampered_path,baseline_path,limits_path,
                self.root/"receipt.json",baseline,limits,json.loads((self.root/"receipt.json").read_text()),
                app,json.loads((self.root/"profile.json").read_text()),usb.sha(self.backup.read_bytes()))

        # A different candidate gets an explicit comparison binding. The original
        # result, limits, configuration and v1 translation stay byte-identical.
        frozen={p:p.read_bytes() for p in (baseline_path,limits_path,config_path,binding_path)}
        candidate_dir=self.root/"diagnostic-candidate";candidate_dir.mkdir()
        from esptool.bin_image import ESP32S3FirmwareImage, ImageSegment
        image=ESP32S3FirmwareImage();desc=bytearray(256)
        struct.pack_into("<I",desc,0,0xabcd5432);desc[64:128]=b"f"*64
        image.segments=[ImageSegment(0x3c000020,desc)]
        image.save(str(candidate_dir/"esp32-csi-tx.bin"))
        for name in ("esp32-csi-tx.elf","config/sdkconfig.json","partition_table/partition-table.bin"):
            target=candidate_dir/name;target.parent.mkdir(parents=True,exist_ok=True)
            private(target,(self.root/name).read_bytes())
        candidate_app=(candidate_dir/"esp32-csi-tx.bin").read_bytes()
        candidate_receipt=copy.deepcopy(json.loads((self.root/"receipt.json").read_text()))
        candidate_receipt["source_sha256"]="f"*64
        candidate_receipt["runtime_image_sha256"]=usb.app_image(candidate_app).calc_digest.hex()
        candidate_receipt["candidate"].update(size=len(candidate_app),sha256=usb.sha(candidate_app))
        candidate_receipt["artifacts"]={name:usb.sha((candidate_dir/name).read_bytes())
            for name in candidate_receipt["artifacts"]}
        candidate_receipt_path=private(candidate_dir/"receipt.json",usb.canonical(candidate_receipt))
        comparison=copy.deepcopy(baseline)
        comparison.update(candidate_image_sha256=candidate_receipt["runtime_image_sha256"],
                          candidate_source_sha256=candidate_receipt["source_sha256"])
        comparison["binding_derivation"]={
            "schema":"tx1-7a-candidate-comparison-binding-v2",
            "BASELINE_ID":"TX1-7A-20260926T152253Z-MIXED-V2",
            "original_result":usb.artifact(baseline_path),
            "original_limits":usb.artifact(limits_path),
            "original_config":usb.artifact(config_path),
            "original_binding":usb.artifact(binding_path),
            "original_build_receipt":usb.artifact(self.root/"receipt.json"),
            "build_receipt":usb.artifact(candidate_receipt_path),
            "candidate_file_sha256":usb.sha(candidate_app),
            "candidate_runtime_image_sha256":candidate_receipt["runtime_image_sha256"],
            "changed_fields":["candidate_image_sha256","candidate_source_sha256"],
            "measurements_and_limits_changed":False,"new_baseline":False,"write_authorized":False,
            "reason":"compare the diagnostic candidate against the same frozen baseline"}
        comparison_path=private(self.root/"candidate-comparison.json",usb.canonical(comparison))
        candidate_plan=usb.prepare(self.backup,self.backup,candidate_receipt_path,self.root/"profile.json",
            limits_path,baseline_path,IDF,self.root/"candidate-plan",fixture=True,
            baseline_binding_path=comparison_path)
        self.assertEqual(usb.validate_plan(candidate_plan),self.original)
        self.assertEqual({p:p.read_bytes() for p in frozen},frozen)
        for label,edit in (
            ("measurements",lambda b:b["windows"].clear()),
            ("source",lambda b:b.update(candidate_source_sha256="0"*64)),
            ("conditions",lambda b:b.update(conditions_sha256="0"*64)),
            ("provenance",lambda b:b["binding_derivation"].update(original_limits=usb.artifact(config_path))),
            ("authorization",lambda b:b["binding_derivation"].update(write_authorized=True))):
            tampered=copy.deepcopy(comparison);edit(tampered)
            tampered_path=private(self.root/(label+"-comparison.json"),usb.canonical(tampered))
            with self.subTest(label=label),self.assertRaises(ValueError):
                usb.prepare(self.backup,self.backup,candidate_receipt_path,self.root/"profile.json",
                    limits_path,baseline_path,IDF,self.root/(label+"-plan"),fixture=True,
                    baseline_binding_path=tampered_path)

    def test_backend_disables_sdk_automatic_reflash(self):
        backend=usb.USB();backend.esp=type("MockESP",(),{})()
        with patch("esptool.cmds.write_flash") as write:
            backend.write(0x150000,b"fixture")
        self.assertEqual(backend.esp.WRITE_FLASH_ATTEMPTS,1)
        self.assertFalse(write.call_args.args[1].force)
        self.assertEqual(write.call_args.args[1].flash_size,"keep")

    def test_default_run_and_import_never_open_device(self):
        with patch.object(usb,"USB",side_effect=AssertionError("no device")),patch("socket.socket",side_effect=AssertionError("no network")):
            self.assertEqual(usb.main(["run","--plan",str(self.root/"plan/plan.json"),"--operation","migrate","--output",str(self.root/"dry")]),0)
            self.assertEqual(usb.main(["run","--plan",str(self.root/"plan/plan.json"),"--operation","migrate","--start","--output",str(self.root/"blocked")]),2)


class SourceTests(unittest.TestCase):
    def test_boot_diagnostics_cannot_be_silenced_or_require_psram(self):
        config=dict(ESP_CONSOLE_UART_DEFAULT=True,ESP_CONSOLE_UART_NUM=0,
                    ESP_CONSOLE_UART_BAUDRATE=115200,ESP_CONSOLE_SECONDARY_USB_SERIAL_JTAG=True,
                    ESP_SYSTEM_PANIC_PRINT_REBOOT=True,
                    ESP_SYSTEM_PANIC_REBOOT_DELAY_SECONDS=3,ESP_MAIN_TASK_STACK_SIZE=8192,
                    ESP_BROWNOUT_DET=True,
                    ESP_TASK_WDT_EN=True,ESP_TASK_WDT_INIT=True,ESP_TASK_WDT_TIMEOUT_S=10)
        build_release.validate_boot_diagnostics_config(config)
        for key in config:
            with self.subTest(key=key), self.assertRaises(ValueError):
                build_release.validate_boot_diagnostics_config({k:v for k,v in config.items() if k!=key})
        for key in ("SPIRAM","ESP_SYSTEM_PANIC_SILENT_REBOOT"):
            with self.subTest(key=key), self.assertRaises(ValueError):
                build_release.validate_boot_diagnostics_config(dict(config,**{key:True}))
        for stack_bytes in (3584, 4096, True):
            with self.subTest(main_stack_bytes=stack_bytes), self.assertRaises(ValueError):
                build_release.validate_boot_diagnostics_config(
                    dict(config,ESP_MAIN_TASK_STACK_SIZE=stack_bytes))

    def test_migration_build_requires_reproducible_mode(self):
        config=dict(TX_PROTECT_ORIGINAL_APP=True,ESPTOOLPY_FLASHSIZE_4MB=True,ESP_PHY_MAX_WIFI_TX_POWER=11)
        with self.assertRaises(ValueError):usb.validate_build_config(config)
        config["APP_REPRODUCIBLE_BUILD"]=True
        usb.validate_build_config(config)

    def test_public_export_contains_header_and_no_private_config(self):
        names=build_release.public_sources()
        self.assertIn("main/tx_config.h",names);self.assertNotIn("tx_config.h",names)
        self.assertFalse(any(n.startswith(("build/","private/")) for n in names))

    def test_help_and_imports_never_start_devices_or_processes(self):
        with patch("serial.Serial",side_effect=AssertionError("no serial")),patch("serial.tools.list_ports.comports",side_effect=AssertionError("no enumeration")),patch("socket.socket",side_effect=AssertionError("no network")),patch("subprocess.run",side_effect=AssertionError("no process")):
            for module in (measure,usb,build_release):
                with self.assertRaises(SystemExit) as result:module.main(["--help"])
                self.assertEqual(result.exception.code,0)


if __name__=="__main__":unittest.main()

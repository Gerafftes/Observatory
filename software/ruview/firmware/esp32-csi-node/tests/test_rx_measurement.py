"""Actual phase/qualification orchestrator with fake time and HTTP, no devices."""
import copy
import io as buffers
import importlib.util
import threading
import unittest
from pathlib import Path

SPEC=importlib.util.spec_from_file_location("rx_measurement",Path(__file__).resolve().parents[3]/"scripts/rx_measurement.py")
rx=importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(rx)


class Clock:
    seconds=0
    def time(self): return 1_700_000_000+self.seconds
    def monotonic_ns(self): return int(self.seconds*1e9)
    def sleep(self,seconds): self.seconds+=seconds


def plan():
    return {"schema":"observatory.rx-measurement-plan.v1","phase":"baseline","recording_id":"test-recording",
        "server_url":"http://127.0.0.1:8765","csi_host_ip":"192.0.2.100","udp_port":5005,"server_sha256":"b"*64,"server_pid":123,
        "recordings_dir":"/synthetic/external/data/recordings","firmware_image_sha256":"c"*64,"firmware_source_sha256":"d"*64,
        "nodes":[{"node_id":node,"url":f"http://192.0.2.{node}:8032","device_id":str(node)*64,
            "config_sha256":"a"*64,"generation":7,"mode_id":0,"peer_ids":[],"rate_hz":50,
            "token_env":f"SYNTHETIC_RX{node}_TOKEN"} for node in range(1,5)]}


class IO:
    def __init__(self,clock,plan):
        self.clock=clock; self.plan=plan; self.owners={}; self.starts=0; self.stops=0
        self.clamp=0; self.fail_stop=False; self.fail_start=False; self.bad_node=0; self.peer_ended=False
    def request(self,origin,path,body=None,node=None):
        stamp=self.clock.monotonic_ns()
        if path=="/api/v1/system/build": result={"schema":"observatory.host-build.v1","executable_sha256":"b"*64,"process_id":123}
        elif path=="/api/v1/system/clock": result={"clock_epoch_id":"test-clock","host_monotonic_ns":stamp,"host_unix_ns":int(self.clock.time()*1e9)}
        elif path=="/api/v1/recording/start":
            self.starts+=1
            if self.fail_start: raise RuntimeError("ambiguous start")
            self.start=stamp
            result={"success":True,"recording_id":"test-recording","clock_epoch_id":"test-clock","started_at_monotonic_ns":stamp}
        elif path=="/api/v1/recording/stop":
            self.stops+=1
            if self.fail_stop: raise RuntimeError("ambiguous stop")
            result={"success":True,"stopped":True,"closed_at":{"clock_epoch_id":"test-clock","host_monotonic_ns":stamp+self.clamp}}
        else:
            key=node["node_id"]
            caps={"schema":"observatory.rx-capabilities.v1","wire_version":4,"device_id":node["device_id"],
                "measurement_active":key in self.owners,"measurement_owner":self.owners.get(key,"")}
            if path=="/capabilities": result=caps
            elif path=="/measurement":
                if key in self.owners and self.owners[key]!=body["owner"]: raise ValueError("foreign owner")
                if body["seconds"]: self.owners[key]=body["owner"]
                else: self.owners.pop(key,None)
                result=dict(caps,measurement_active=key in self.owners,measurement_owner=self.owners.get(key,""))
            elif path=="/config":
                active={name:node[name] for name in ("node_id","generation","config_sha256","mode_id","peer_ids","rate_hz")}
                active.update(target_ip=self.plan["csi_host_ip"],target_port=self.plan["udp_port"])
                if key==self.bad_node: active["generation"]+=1
                result={"schema":"observatory.rx-config.v1","active":active,"stored":active,"reboot_required":False}
            elif path=="/ota/status": result={"image_sha256":"c"*64,"ota_state":"VALID","boot_id":f"{key:016x}"}
            elif path=="/diag":
                count=int(self.clock.seconds*10)+1
                result={"schema":"observatory.rx-diagnostic.v2","source_sha256":"d"*64,"complete":True,
                    "boot_id":key,"snapshot_end_us":int(self.clock.seconds*1e6),"allocation_failures":0,"saturated":False,
                    "reconnect":{"saturated":False},"resources":{"heap_internal_free_bytes":64000,"heap_internal_min_bytes":64000,
                        "heap_internal_largest_bytes":32000,"http_stack_min_bytes":2000,"callback_stack_min_bytes":2000,"sender_stack_min_bytes":2000},
                    "callback":{"received":count,"invalid":0,"foreign":0,"rate_limited":0,"admitted":count,"copied":count,"handoff_drops":0},
                    "transport":{"captured":count,"waiting":0,"in_flight":0,"aborted":0,"serialize_errors":0,"os_accepted":count,"abandoned":0},
                    "links":[{"captured":count,"queued":0,"os_accepted":count,"abandoned":0,"serialize_errors":0,"aborted":0}],
                    "led":{"qualified_frames":count,"driver_ok":True,"code":"NORMAL"},"profile":{"peer_session_ended":self.peer_ended}}
            else: raise AssertionError(path)
        return result,stamp,stamp


class MeasurementTests(unittest.TestCase):
    def setUp(self):
        self.plan=rx.validate(plan()); self.clock=Clock(); self.io=IO(self.clock,self.plan)
    def proof(self):
        rows=[]; current=self.clock.seconds
        for node in self.plan["nodes"]:
            self.clock.seconds=810; first=rx.preflight(self.io,self.plan,node)
            self.clock.seconds=915; last=rx.preflight(self.io,self.plan,node)
            rows.append({"node_id":node["node_id"],"kind":"Q900","first":first,"last":last})
        self.clock.seconds=max(current,915)
        return {"schema":"observatory.rx-qualification.v1","nodes":rows}
    def test_qualified_phase_closes_once_and_releases_own_leases(self):
        proof=self.proof()
        receipt=rx.collect(self.io,self.plan,proof,self.clock.time()+3600,self.clock)
        self.assertFalse(receipt["full_tag14_acceptance"])
        self.assertEqual((self.io.starts,self.io.stops,self.io.owners),(1,1,{}))
    def test_short_functional_evidence_is_explicit_and_never_a_Q900_substitute(self):
        self.plan['phase']='functional-baseline';rx.validate(self.plan);self.clock.seconds=100
        proof=rx.qualify_functional(self.io,self.plan,self.clock.time()+3600,self.clock)
        self.assertFalse(proof['Q900_proven'])
        self.assertEqual(proof['schema'],'observatory.rx-functional-readiness.v1')
        original=dict(self.plan,phase='baseline')
        with self.assertRaisesRegex(ValueError,'qualification receipt'):rx.collect(self.io,original,proof,self.clock.time()+3600,self.clock)
        receipt=rx.collect(self.io,self.plan,proof,self.clock.time()+3600,self.clock)
        self.assertEqual(receipt['acceptance_scope'],'functional-only')
        self.assertFalse(receipt['full_tag14_acceptance'])
    def test_functional_pair_still_rejects_ended_engine(self):
        self.plan=self.pair_plan();self.plan['phase']='functional-pair';rx.validate(self.plan)
        self.io=IO(self.clock,self.plan);self.io.peer_ended=True
        with self.assertRaisesRegex(ValueError,'peer session ended'):
            rx.qualify_functional(self.io,self.plan,self.clock.time()+3600,self.clock)
    def pair_plan(self):
        result=copy.deepcopy(self.plan);result['phase']='pair'
        for node in (result['nodes'][0],result['nodes'][3]):
            node['mode_id']=1;node['peer_ids']=[5-node['node_id']]
        return rx.validate(result)
    def test_ended_peer_session_is_readable_but_cannot_qualify_pair(self):
        self.plan=self.pair_plan();self.io=IO(self.clock,self.plan);self.io.peer_ended=True
        self.clock.seconds=100
        self.assertTrue(rx.preflight(self.io,self.plan,self.plan['nodes'][0])['diag']['profile']['peer_session_ended'])
        with self.assertRaisesRegex(ValueError,'peer session ended'):
            rx.qualify_node(self.io,self.plan,self.plan['nodes'][0],self.clock.time()+3600,self.clock)
        self.assertEqual(self.clock.seconds,100)
        self.assertEqual((self.io.starts,self.io.owners),(0,{}))
    def test_peer_engine_ending_during_valid_spread_is_not_pair_ready(self):
        self.plan=self.pair_plan()
        class EndsLater(IO):
            def request(self,origin,path,body=None,node=None):
                self.peer_ended=self.clock.seconds>=161
                return super().request(origin,path,body,node)
        self.io=EndsLater(self.clock,self.plan);self.clock.seconds=100
        with self.assertRaisesRegex(ValueError,'peer session ended'):
            rx.qualify_node(self.io,self.plan,self.plan['nodes'][0],self.clock.time()+3600,self.clock)
    def test_pair_session_ending_after_qualification_never_arms_or_starts(self):
        self.plan=self.pair_plan()
        class PairAges(IO):
            def request(self,origin,path,body=None,node=None):
                result=super().request(origin,path,body,node)
                if node and node['node_id'] in (1,4) and path=='/diag':
                    result[0]['snapshot_end_us']=max(0,int((self.clock.seconds-800)*1e6))
                return result
        self.io=PairAges(self.clock,self.plan);proof=self.proof()
        for row in proof['nodes']:
            if row['node_id'] in (1,4):row['kind']='pair-ready'
        self.io.peer_ended=True
        with self.assertRaisesRegex(ValueError,'peer session ended'):
            rx.collect(self.io,self.plan,proof,self.clock.time()+3600,self.clock)
        self.assertEqual((self.io.starts,self.io.owners),(0,{}))
    def test_delayed_start_ack_uses_calibrated_native_start_for_end(self):
        class DelayedStartIO(IO):
            offset=5_000_000_000_000
            def request(self,origin,path,body=None,node=None):
                result,begin,end=super().request(origin,path,body,node)
                if path=='/api/v1/system/clock':result['host_monotonic_ns']+=self.offset
                elif path=='/api/v1/recording/start':
                    result['started_at_monotonic_ns']+=self.offset
                    self.clock.sleep(.117)
                    end=self.clock.monotonic_ns()
                elif path=='/api/v1/recording/stop':result['closed_at']['host_monotonic_ns']+=self.offset
                return result,begin,end
        self.io=DelayedStartIO(self.clock,self.plan)
        receipt=rx.collect(self.io,self.plan,self.proof(),self.clock.time()+3600,self.clock)
        self.assertEqual(receipt['host_end_clamp_ns'],0)
        self.assertEqual((self.io.starts,self.io.stops,self.io.owners),(1,1,{}))
    def test_critical_end_diag_is_first_device_request_before_postchecks(self):
        class EndOrderIO(IO):
            def __init__(self, clock, phase):
                super().__init__(clock, phase)
                self.first_end_request = {}
            def request(self, origin, path, body=None, node=None):
                if node and self.starts and self.clock.monotonic_ns() >= self.start + 180_000_000_000:
                    self.first_end_request.setdefault(node['node_id'], path)
                return super().request(origin, path, body, node)
        self.io = EndOrderIO(self.clock, self.plan)
        rx.collect(self.io,self.plan,self.proof(),self.clock.time()+3600,self.clock)
        self.assertEqual(self.io.first_end_request, {node:'/diag' for node in range(1,5)})
    def test_stop_and_four_critical_diagnostics_can_all_run_together(self):
        class BarrierIO(IO):
            def __init__(self, clock, phase):
                super().__init__(clock, phase)
                self.end_gate=threading.Barrier(5,timeout=2)
            def request(self,origin,path,body=None,node=None):
                if self.starts and (path=='/api/v1/recording/stop' or node and path=='/diag'):
                    self.end_gate.wait()
                return super().request(origin,path,body,node)
        self.io=BarrierIO(self.clock,self.plan)
        rx.collect(self.io,self.plan,self.proof(),self.clock.time()+3600,self.clock)
        self.assertEqual(self.io.stops,1)
    def test_bad_profile_never_starts_recorder_and_releases_partial_arm(self):
        proof=self.proof(); self.io.bad_node=3
        with self.assertRaisesRegex(ValueError,"profile differs"):
            rx.collect(self.io,self.plan,proof,self.clock.time()+3600,self.clock)
        self.assertEqual((self.io.starts,self.io.owners),(0,{}))
    def test_device_http_tail_does_not_replace_the_native_end_clock(self):
        class SlowEndIO(IO):
            def request(self,origin,path,body=None,node=None):
                result=super().request(origin,path,body,node)
                if node and path=='/diag' and self.stops:
                    self.clock.sleep(0.12)
                    return result[0],result[1],self.clock.monotonic_ns()
                return result
        self.io=SlowEndIO(self.clock,self.plan)
        receipt=rx.collect(self.io,self.plan,self.proof(),self.clock.time()+3600,self.clock)
        self.assertGreater(receipt['device_end_bracket_ns'],50_000_000)
        self.assertEqual(receipt['host_end_clamp_ns'],0)
        self.assertLessEqual(receipt['clock_proof']['mapping_uncertainty_ns'],50_000_000)
    def test_clock_mapping_keeps_distinct_origins_and_rejects_wide_or_changed_epoch(self):
        first={'server':{'clock_epoch_id':'epoch','host_monotonic_ns':1000,'host_unix_ns':1700000000000001000},
               'request_begin_ns':1000000000000,'response_end_ns':1000000002000}
        last=copy.deepcopy(first)
        for key in ('host_monotonic_ns','host_unix_ns'):last['server'][key]+=1_000_000_000
        last['request_begin_ns']+=1_000_000_000;last['response_end_ns']+=1_000_000_000
        proof=rx.clock_proof(first,last,'epoch')
        self.assertLess(proof['adapter_to_server_offset_interval_ns'][1],0)
        self.assertEqual(proof['mapping_uncertainty_ns'],2000)
        self.assertEqual(proof['pcap_timestamp_quantization_ns'],1000)
        boundary=[copy.deepcopy(first),copy.deepcopy(last)]
        for row in boundary:row['response_end_ns']=row['request_begin_ns']+99_999_000
        with self.assertRaisesRegex(ValueError,'uncertainty'):rx.clock_proof(*boundary,'epoch')
        changed=copy.deepcopy(last);changed['server']['clock_epoch_id']='another'
        with self.assertRaisesRegex(ValueError,'epoch changed'):rx.clock_proof(first,changed,'epoch')
        for row in (first,last):row['response_end_ns']+=200_000_000
        with self.assertRaisesRegex(ValueError,'uncertainty'):rx.clock_proof(first,last,'epoch')
    def test_ambiguous_start_gets_one_identity_bound_close_and_no_start_retry(self):
        self.io.fail_start=True
        with self.assertRaisesRegex(RuntimeError,"ambiguous start"):
            rx.collect(self.io,self.plan,self.proof(),self.clock.time()+3600,self.clock)
        self.assertEqual((self.io.starts,self.io.stops,self.io.owners),(1,1,{}))
    def test_ambiguous_stop_is_never_repeated(self):
        self.io.fail_stop=True
        with self.assertRaisesRegex(RuntimeError,"ambiguous stop"):
            rx.collect(self.io,self.plan,self.proof(),self.clock.time()+3600,self.clock)
        self.assertEqual((self.io.starts,self.io.stops,self.io.owners),(1,1,{}))
    def test_late_end_and_fixed_deadline_are_real_failures(self):
        self.io.clamp=50_000_001
        with self.assertRaisesRegex(ValueError,"50-ms"):
            rx.collect(self.io,self.plan,self.proof(),self.clock.time()+3600,self.clock)
        self.assertEqual(self.io.stops,1)
        with self.assertRaisesRegex(ValueError,"fixed deadline"):
            rx.collect(self.io,self.plan,self.proof(),self.clock.time()+180,self.clock)
    def test_q900_waits_on_original_boot_and_missed_window_does_not_reset(self):
        result=rx.qualify_node(self.io,self.plan,self.plan["nodes"][0],self.clock.time()+3600,self.clock)
        self.assertEqual(result["kind"],"Q900")
        self.assertEqual(result["first"]["diag"]["snapshot_end_us"],810_000_000)
        self.assertEqual(result["last"]["diag"]["snapshot_end_us"],915_000_000)
        self.clock.seconds=860
        with self.assertRaisesRegex(ValueError,"entry window missed"):
            rx.qualify_node(self.io,self.plan,self.plan["nodes"][0],self.clock.time()+3600,self.clock)
        self.assertEqual(self.io.starts,0)
    def test_lower_capture_rate_or_wrong_phase_cannot_relax_tag14(self):
        changed=copy.deepcopy(self.plan); changed["nodes"][0]["rate_hz"]=20
        with self.assertRaises(ValueError): rx.validate(changed)

    def test_pcap_preroll_and_owned_close_are_bounded(self):
        class Capture:
            starts=closes=0
            def start(self): self.starts+=1
            def close(self): self.closes+=1; return {"complete":True}
        capture=Capture(); proof=self.proof(); begin=self.clock.seconds
        receipt=rx.collect(self.io,self.plan,proof,self.clock.time()+3600,self.clock,capture=capture)
        self.assertEqual((capture.starts,capture.closes),(1,1))
        self.assertEqual(self.clock.seconds-begin,200)
        self.assertTrue(receipt["pcap"]["complete"])
    def test_monotonic_deadline_cannot_be_extended_by_wall_clock_rollback(self):
        transport=rx.Transport(buffers.StringIO(),self.clock,deadline=self.clock.time()+10)
        self.clock.seconds=11
        self.clock.time=lambda:1_699_999_000
        with self.assertRaisesRegex(ValueError,"monotonic/UTC"):
            transport.request("http://127.0.0.1:0","/never-sent")


if __name__=="__main__": unittest.main()

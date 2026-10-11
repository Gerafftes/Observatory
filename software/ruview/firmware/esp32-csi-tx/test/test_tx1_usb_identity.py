"""Offline checks for the two supported TX1 USB access-path shapes."""
lazy import copy
lazy from pathlib import Path
lazy from types import SimpleNamespace
lazy import sys
lazy import unittest
lazy from unittest.mock import patch

sys.path.insert(0,str(Path(__file__).resolve().parents[1]/"tools"))
lazy import tx1_usb as usb


MAC="02:11:22:33:44:55"
CHIP="ESP32-S3 (QFN56) (revision v0.2)"
SECURITY={"flags":0,"flash_crypt_cnt":0,"key_purposes":[0,0,0,0,0,0,12],
          "chip_id":9,"api_version":0,"parsed_flags":{
              "SECURE_BOOT_EN":False,"SECURE_BOOT_AGGRESSIVE_REVOKE":False,
              "SECURE_DOWNLOAD_ENABLE":False,"SECURE_BOOT_KEY_REVOKE0":False,
              "SECURE_BOOT_KEY_REVOKE1":False,"SECURE_BOOT_KEY_REVOKE2":False,
              "SOFT_DIS_JTAG":False,"HARD_DIS_JTAG":False,"DIS_USB":False,
              "DIS_DOWNLOAD_DCACHE":False,"DIS_DOWNLOAD_ICACHE":False}}
EXPECTED={"usb":{"vid":0x1a86,"pid":0x55d3,"serial_number":"FIXTURE-BRIDGE-001",
                 "location":"fixture-1-1","product":"USB Single Serial","manufacturer":None},
          "factory_base_mac":MAC,"chip":CHIP,"security_info_rom":SECURITY,
          "flash_jedec_id":"0x184068"}


def port(vid,pid,serial,product,manufacturer,device):
    return SimpleNamespace(vid=vid,pid=pid,serial_number=serial,location="fixture-1-1",
                           product=product,manufacturer=manufacturer,device=device)


BRIDGE=port(0x1a86,0x55d3,"FIXTURE-BRIDGE-001","USB Single Serial",None,"/dev/cu.bridge")
NATIVE=port(0x303a,0x1001,MAC.upper(),"USB JTAG/serial debug unit", "Espressif","/dev/cu.native")


class FakeROM:
    IMAGE_CHIP_ID=9
    observed_mac=MAC
    observed_chip=CHIP
    fail_at=None
    secure_version=0
    efuse_bits=0
    security=None
    forced_secure_download_mode=False
    instances=[]

    def __init__(self,device,baud):
        self._port=SimpleNamespace(port=device,exclusive=False)
        self.connect_mode=None
        self.stub_calls=0
        self.secure_download_mode=self.forced_secure_download_mode
        self.IS_STUB=False
        self.instances.append(self)

    def connect(self,mode,attempts):
        self.connect_mode=mode
        assert attempts==1
        assert self._port.timeout==3
        if self.fail_at=="connect":raise OSError("ROM unavailable")

    def get_security_info(self,cache):
        assert cache is False
        if self.fail_at=="security":raise OSError("identity read unavailable")
        return copy.deepcopy(self.security)

    def read_reg(self,address):
        bits=self.efuse_bits|(self.secure_version<<142)
        return (bits>>(8*(address-0x6000702c)))&0xffffffff
    def read_mac(self):
        if self.fail_at=="mac":raise OSError("MAC read unavailable")
        return bytes.fromhex(self.observed_mac.replace(":",""))
    def get_chip_description(self):return self.observed_chip
    def run_stub(self):
        self.stub_calls+=1
        self.IS_STUB=True
        return self
    def flash_spi_attach(self,value):pass
    def flash_id(self):return 0x184068
    def flash_set_parameters(self,value):pass
    def change_baud(self,value):pass


class SecurityInfoTests(unittest.TestCase):
    def test_list_and_tuple_compare_equally_without_mutation(self):
        for actual_type in (list,tuple):
            for expected_type in (list,tuple):
                with self.subTest(actual=actual_type,expected=expected_type):
                    actual=copy.deepcopy(SECURITY);expected=copy.deepcopy(SECURITY)
                    actual["key_purposes"]=actual_type(actual["key_purposes"])
                    expected["key_purposes"]=expected_type(expected["key_purposes"])
                    saved=copy.deepcopy((actual,expected))
                    usb.compare_security_info(actual,expected)
                    self.assertEqual((actual,expected),saved)

    def test_every_numeric_field_and_key_purpose_is_compared(self):
        for name in ("flags","flash_crypt_cnt","chip_id","api_version"):
            actual=copy.deepcopy(SECURITY);actual[name]+=1
            with self.subTest(field=name),self.assertRaisesRegex(ValueError,name+": expected="):
                usb.compare_security_info(actual,SECURITY)
        for index in range(7):
            actual=copy.deepcopy(SECURITY);actual["key_purposes"][index]+=1
            with self.subTest(index=index),self.assertRaisesRegex(ValueError,rf"key_purposes\[{index}\]: expected="):
                usb.compare_security_info(actual,SECURITY)

    def test_every_parsed_flag_is_compared(self):
        for name in SECURITY["parsed_flags"]:
            actual=copy.deepcopy(SECURITY);actual["parsed_flags"][name]=True
            with self.subTest(flag=name),self.assertRaisesRegex(ValueError,"parsed_flags."+name+": expected="):
                usb.compare_security_info(actual,SECURITY)

    def test_missing_fields_fail_even_when_missing_on_both_sides(self):
        for side in ("ROM","backup","both"):
            for name in SECURITY:
                actual=copy.deepcopy(SECURITY);expected=copy.deepcopy(SECURITY)
                if side in ("ROM","both"):del actual[name]
                if side in ("backup","both"):del expected[name]
                with self.subTest(side=side,field=name),self.assertRaisesRegex(ValueError,"missing=.*"+name):
                    usb.compare_security_info(actual,expected)
            for name in SECURITY["parsed_flags"]:
                actual=copy.deepcopy(SECURITY);expected=copy.deepcopy(SECURITY)
                if side in ("ROM","both"):del actual["parsed_flags"][name]
                if side in ("backup","both"):del expected["parsed_flags"][name]
                with self.subTest(side=side,flag=name),self.assertRaisesRegex(ValueError,"missing=.*"+name):
                    usb.compare_security_info(actual,expected)

    def test_unknown_fields_are_not_ignored(self):
        for nested in (False,True):
            for side in ("ROM","backup","both"):
                actual=copy.deepcopy(SECURITY);expected=copy.deepcopy(SECURITY)
                for source,info in (("ROM",actual),("backup",expected)):
                    if side in (source,"both"):
                        (info["parsed_flags"] if nested else info)["UNKNOWN_SECURITY_FIELD"]=0
                with self.subTest(nested=nested,side=side),self.assertRaisesRegex(ValueError,"unexpected=.*UNKNOWN_SECURITY_FIELD"):
                    usb.compare_security_info(actual,expected)

    def test_invalid_types_ranges_and_lengths_fail(self):
        invalid=[]
        for name,width in (("flags",32),("flash_crypt_cnt",8),("chip_id",32),("api_version",32)):
            invalid.extend((name,value) for value in (False,0.0,"0",None,-1,1<<width))
        invalid.extend(("key_purposes",value) for value in (None,{},"00000012",[0]*6,[0]*8,[False]*7,[0.0]*7,[-1]*7,[256]*7))
        invalid.extend(("parsed_flags",value) for value in (None,[],False))
        for name,value in invalid:
            bad=copy.deepcopy(SECURITY);bad[name]=value
            for actual,expected in ((bad,SECURITY),(SECURITY,bad)):
                with self.subTest(field=name,value=value),self.assertRaisesRegex(ValueError,name):
                    usb.compare_security_info(actual,expected)
        for name in SECURITY["parsed_flags"]:
            bad=copy.deepcopy(SECURITY);bad["parsed_flags"][name]=0
            with self.subTest(flag=name),self.assertRaisesRegex(ValueError,name):
                usb.compare_security_info(bad,SECURITY)
        for invalid_info in (None,[],False):
            with self.assertRaisesRegex(ValueError,"expected dictionary"):
                usb.compare_security_info(invalid_info,SECURITY)

    def test_multiple_changes_report_each_concrete_value(self):
        actual=copy.deepcopy(SECURITY);actual["flags"]=1;actual["key_purposes"][6]=11
        with self.assertRaises(ValueError) as result:usb.compare_security_info(actual,SECURITY)
        self.assertIn("flags: expected=0, actual=1",str(result.exception))
        self.assertIn("key_purposes[6]: expected=12, actual=11",str(result.exception))


class TX1USBIdentityTests(unittest.TestCase):
    def setUp(self):
        FakeROM.observed_mac=MAC
        FakeROM.observed_chip=CHIP
        FakeROM.fail_at=None
        FakeROM.secure_version=0
        FakeROM.efuse_bits=0
        FakeROM.forced_secure_download_mode=False
        FakeROM.security=copy.deepcopy(SECURITY)
        FakeROM.security["key_purposes"]=tuple(SECURITY["key_purposes"])
        FakeROM.instances=[]

    def identify(self,ports,expected=EXPECTED):
        with patch("serial.tools.list_ports.comports",return_value=ports), \
             patch("esptool.targets.esp32s3.ESP32S3ROM",FakeROM), \
             patch("subprocess.run",return_value=SimpleNamespace(returncode=1,stdout=b"")):
            backend=usb.USB()
            backend.identify(expected)
            return backend

    def test_historical_and_confirmed_native_paths(self):
        for access,mode in ((BRIDGE,"default_reset"),(NATIVE,"usb_reset")):
            with self.subTest(access=access.device):
                backend=self.identify([access])
                self.assertEqual(backend.esp._port.port,access.device)
                self.assertEqual(backend.esp.connect_mode,mode)
                self.assertEqual(backend.esp.stub_calls,1)

    def test_unknown_or_ambiguous_usb_path_is_rejected_before_rom(self):
        unknown=port(0x303a,0x1001,"OTHER","USB JTAG/serial debug unit","Espressif","/dev/cu.other")
        for ports in ([unknown],[BRIDGE,NATIVE]):
            with self.subTest(ports=len(ports)),self.assertRaises(ValueError):self.identify(ports)
            self.assertEqual(FakeROM.instances,[])

    def test_wrong_factory_mac_is_rejected_before_stub(self):
        FakeROM.observed_mac="02:11:22:33:44:56"
        for access in (BRIDGE,NATIVE):
            with self.subTest(access=access.device),self.assertRaisesRegex(ValueError,"ESP identity differs"):
                self.identify([access])
            self.assertEqual(FakeROM.instances[-1].stub_calls,0)

    def test_wrong_chip_revision_is_rejected_before_stub(self):
        FakeROM.observed_chip="ESP32-S3 (QFN56) (revision v0.1)"
        with self.assertRaisesRegex(ValueError,"ESP identity differs"):
            self.identify([NATIVE])
        self.assertEqual(FakeROM.instances[0].stub_calls,0)

    def test_nonzero_secure_version_still_rejects_before_stub(self):
        FakeROM.secure_version=1
        with self.assertRaisesRegex(ValueError,"security/download/anti-rollback gate failed"):
            self.identify([NATIVE])
        self.assertEqual(FakeROM.instances[0].stub_calls,0)

    def test_security_changes_or_missing_fields_reject_before_stub(self):
        variants=[]
        changed=copy.deepcopy(SECURITY);changed["key_purposes"][6]=11;variants.append(changed)
        missing=copy.deepcopy(SECURITY);del missing["parsed_flags"]["SECURE_BOOT_EN"];variants.append(missing)
        for security in variants:
            FakeROM.security=security
            with self.subTest(security=security),self.assertRaises(ValueError):self.identify([NATIVE])
            self.assertEqual(FakeROM.instances[-1].stub_calls,0)

    def test_every_efuse_gate_still_rejects_before_stub(self):
        for position in (42,43,82,83,84,116,128,133,*range(142,158)):
            FakeROM.efuse_bits=1<<position
            with self.subTest(bit=position),self.assertRaisesRegex(ValueError,"security/download/anti-rollback gate failed"):
                self.identify([NATIVE])
            self.assertEqual(FakeROM.instances[-1].stub_calls,0)

    def test_matching_backup_does_not_bypass_absolute_rom_security_gates(self):
        for name in ("chip_id","flash_crypt_cnt","SECURE_BOOT_EN","SECURE_DOWNLOAD_ENABLE"):
            expected=copy.deepcopy(EXPECTED)
            security=expected["security_info_rom"]
            if name in security:security[name]+=1
            else:security["parsed_flags"][name]=True
            FakeROM.security=copy.deepcopy(security)
            with self.subTest(field=name),self.assertRaisesRegex(ValueError,"security state differs; no bypass/stub"):
                self.identify([NATIVE],expected)
            self.assertEqual(FakeROM.instances[-1].stub_calls,0)
        FakeROM.security=copy.deepcopy(SECURITY)
        FakeROM.forced_secure_download_mode=True
        with self.assertRaisesRegex(ValueError,"security state differs; no bypass/stub"):
            self.identify([NATIVE])
        self.assertEqual(FakeROM.instances[-1].stub_calls,0)

    def test_failed_rom_or_identity_read_is_rejected_before_stub(self):
        for failure in ("connect","security","mac"):
            with self.subTest(failure=failure):
                FakeROM.instances=[]
                FakeROM.fail_at=failure
                with self.assertRaises(OSError):self.identify([NATIVE])
                self.assertEqual(FakeROM.instances[0].stub_calls,0)


if __name__=="__main__":unittest.main()

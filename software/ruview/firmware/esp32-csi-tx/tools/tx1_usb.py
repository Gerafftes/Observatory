#!/usr/bin/env python3.15
"""Manifest-bound USB migration/recovery. Default run is OFFLINE; never write on import/help."""
from __future__ import annotations
lazy import argparse
lazy import contextlib
lazy import copy
lazy import hashlib
lazy import io
lazy import json
lazy import os
lazy import re
lazy import signal
lazy from pathlib import Path
lazy import struct
lazy import sys
lazy import time
lazy import zlib
lazy from datetime import datetime, timezone

sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
lazy from provision import read_private, write_private, validate, parse_mac, merge_nvs

PASS, REFUSED, FAILED, RESTORED = 0, 2, 5, 6
GATES=("old_bootloader_image_compatibility", "shared_data_backward_compatibility",
       "old_runtime_identity_phy_power", "ota_selection_undefined_supported", "manual_recovery_ready")
LAYOUT={"nvs":(1,2,0x9000,0x5000),"otadata":(1,0,0xe000,0x2000),
        "app0":(0,16,0x10000,0x140000),"app1":(0,17,0x150000,0x140000),
        "spiffs":(1,130,0x290000,0x160000),"coredump":(1,3,0x3f0000,0x10000)}
DISABLED_CONFIG=("SECURE_BOOT","SECURE_FLASH_ENC_ENABLED","BOOTLOADER_APP_ANTI_ROLLBACK",
                 "NVS_ENCRYPTION","ESP_WIFI_NVS_ENABLED","ESP_PHY_CALIBRATION_AND_DATA_STORAGE",
                 "ESP_PHY_INIT_DATA_IN_PARTITION","ESP_COREDUMP_ENABLE_TO_FLASH",
                 "TX_VERIFIED_ROLLBACK","BOOTLOADER_APP_ROLLBACK_ENABLE")


def sha(data): return hashlib.sha256(data).hexdigest()
def canonical(value): return json.dumps(value,sort_keys=True,separators=(",",":")).encode()


def artifact(path):
    if path.is_symlink() or not path.is_file():raise ValueError("artifact missing/symlink")
    path=path.resolve()
    return {"path":str(path),"size":path.stat().st_size,"sha256":sha(path.read_bytes())}


def checked(record):
    path=Path(record["path"])
    if path.is_symlink() or not path.is_file(): raise ValueError("artifact missing/symlink")
    data=path.read_bytes()
    if len(data)!=record["size"] or sha(data)!=record["sha256"]: raise ValueError("artifact changed")
    return data


def partitions(table):
    found={};ends=[]
    for offset in range(0,0x1000,32):
        entry=table[offset:offset+32]
        if entry[:2]==b"\xeb\xeb":
            if hashlib.md5(table[:offset]).digest()!=entry[16:]:raise ValueError("table MD5 mismatch")
            break
        if entry==b"\xff"*32:raise ValueError("partition table MD5 missing")
        magic,kind,sub,start,size,label,flags=struct.unpack("<HBBII16sI",entry)
        name=label.split(b"\0",1)[0].decode("ascii")
        if magic!=0x50aa or flags or not size or start%4096 or size%4096 or name in found:
            raise ValueError("unsupported partition entry")
        if start<0x9000 or start+size>0x400000:raise ValueError("partition outside old address contract")
        found[name]={"type":kind,"subtype":sub,"offset":start,"size":size}
        ends.append((start,start+size))
    else:raise ValueError("unterminated partition table")
    ordered=sorted(ends)
    if any(a[1]>b[0] for a,b in zip(ordered,ordered[1:])):raise ValueError("partition overlap")
    return found


def read_backup_pair(manifest_path, require_pair=True):
    manifest_path=Path(manifest_path)
    if manifest_path.is_symlink() or not manifest_path.is_file():raise ValueError("backup manifest missing/symlink")
    manifest=json.loads(manifest_path.read_text())
    root=manifest_path.parent;images=[]
    for name in ("flash-read-1.bin","flash-read-2.bin"):
        record=manifest.get("files",{}).get(name)
        if record is None:
            if require_pair:raise ValueError("two full flash readbacks are required")
            if not images:raise ValueError("first full flash readback is missing")
            images.append(images[0]);continue
        path=root/name
        if path.is_symlink() or not path.is_file():raise ValueError(f"{name} missing/symlink")
        data=path.read_bytes()
        if len(data)!=0x1000000 or record.get("sha256")!=sha(data) or record.get("size",len(data))!=len(data):
            raise ValueError(f"{name} size/hash differs from backup manifest")
        images.append(data)
    if images[0]!=images[1]:raise ValueError("backup full readbacks are not byte-identical")
    if len(images[0])!=0x1000000:raise ValueError("backup flash geometry is not 16 MiB")
    return manifest,images[0]


def partition_for_offset(offset,table):
    if offset<0x8000:return "bootloader"
    if offset<0x9000:return "partition_table"
    for name,part in table.items():
        if part["offset"]<=offset<part["offset"]+part["size"]:return name
    return "unallocated"


def compare_flash_images(historical,current,historical_table,current_table):
    if len(historical)!=0x1000000 or len(current)!=0x1000000:
        raise ValueError("flash comparison requires two complete 16 MiB images")
    changes=[];by_partition={}
    for offset in range(0,len(historical),0x1000):
        old=historical[offset:offset+0x1000];new=current[offset:offset+0x1000]
        if old==new:continue
        region=partition_for_offset(offset,historical_table)
        changed=sum(a!=b for a,b in zip(old,new))
        changes.append({"offset":offset,"offset_hex":hex(offset),"region":region,
                        "changed_bytes":changed,"old_sha256":sha(old),"current_sha256":sha(new)})
        entry=by_partition.setdefault(region,{"sectors":0,"changed_bytes":0})
        entry["sectors"]+=1;entry["changed_bytes"]+=changed
    critical_ranges={"bootloader":(0,0x8000),"partition_table":(0x8000,0x9000)}
    if "app0" in historical_table:
        p=historical_table["app0"];critical_ranges["app0"]=(p["offset"],p["offset"]+p["size"])
    critical={}
    for name,(start,end) in critical_ranges.items():
        critical[name]={"offset":start,"size":end-start,
                        "historical_sha256":sha(historical[start:end]),
                        "current_sha256":sha(current[start:end]),
                        "unchanged":historical[start:end]==current[start:end]}
    table_same=historical_table==current_table
    immutable_ok=table_same and all(v["unchanged"] for v in critical.values())
    status="HARD_STOP" if not immutable_ok else ("REVIEW_UNALLOCATED" if by_partition.get("unallocated") else "PASS")
    return {"schema":1,"sector_size":0x1000,"historical_sha256":sha(historical),
            "current_sha256":sha(current),"historical_partitions":historical_table,
            "current_partitions":current_table,"partition_table_matches":table_same,
            "critical_checks":critical,"immutable_regions_unchanged":immutable_ok,
            "changed_sector_count":len(changes),"changed_by_region":by_partition,
            "changed_sectors":changes,"status":status}


def assert_private_path(path):
    private_root=Path(__file__).resolve().parents[5]/"private"
    if private_root.is_symlink() or not private_root.is_dir():raise ValueError("repository private directory missing/symlink")
    resolved=Path(path).resolve()
    if not resolved.is_relative_to(private_root.resolve()):raise ValueError("device backup output must stay under repository private/")
    return resolved


def snapshot_deadline_expired(signum,frame):raise TimeoutError("absolute 2400-second snapshot budget")


def app_image(data):
    # Espressif's installed image parser only reads the supplied in-memory bytes.
    from esptool.bin_image import LoadFirmwareImage
    image=LoadFirmwareImage("esp32s3",io.BytesIO(data));image.verify()
    if image.chip_id!=9 or image.checksum!=image.calculate_checksum():raise ValueError("invalid S3 image/checksum")
    if image.append_digest and image.stored_digest!=image.calc_digest:raise ValueError("image digest mismatch")
    descriptors=[s.data for s in image.segments if s.data[:4]==struct.pack("<I",0xabcd5432)]
    if len(descriptors)!=1 or struct.unpack_from("<I",descriptors[0],4)[0]!=0:
        raise ValueError("missing/unsupported app secure_version; never burn or relax eFuses")
    return image


def validate_build_config(config):
    if any(config.get(k,False) for k in DISABLED_CONFIG):raise ValueError("unsafe migration build config")
    if not config.get("APP_REPRODUCIBLE_BUILD"):
        raise ValueError("build is not configured for reproducible outputs")
    if not config.get("TX_PROTECT_ORIGINAL_APP") or not config.get("ESPTOOLPY_FLASHSIZE_4MB") or config.get("ESP_PHY_MAX_WIFI_TX_POWER")!=11:
        raise ValueError("build does not preserve the old slot/flash/power contract")


def receipt_candidate_path(receipt,receipt_path=None):
    path=Path(receipt["candidate"]["file"])
    if not path.is_absolute():
        if receipt_path is None:raise ValueError("relative candidate path needs its build receipt")
        path=Path(receipt_path).resolve().parent/path
    return path


def validate_receipt(receipt,app,receipt_path=None):
    candidate=receipt["candidate"];path=receipt_candidate_path(receipt,receipt_path)
    if (sha(app)!=candidate["sha256"] or len(app)!=candidate["size"]
            or candidate.get("intended_offset")!=0x150000 or candidate.get("slot_size")!=0x140000):
        raise ValueError("candidate/slot receipt differs")
    if receipt.get("idf")!="v5.4" or receipt.get("led_strip")!="3.0.3" or receipt.get("reproducible_build") is not True or not re.fullmatch("[0-9a-f]{64}",receipt.get("source_sha256","")):
        raise ValueError("unreviewed source/SDK/component receipt")
    if receipt["source_sha256"].encode() not in app:raise ValueError("source identity not embedded in app")
    required={"esp32-csi-tx.bin","esp32-csi-tx.elf","partition_table/partition-table.bin","config/sdkconfig.json"}
    if set(receipt["artifacts"])!=required:raise ValueError("build artifacts incomplete")
    for name,checksum in receipt["artifacts"].items():
        artifact_path=path.parent/name
        if artifact_path.is_symlink() or sha(artifact_path.read_bytes())!=checksum:
            raise ValueError("build artifact changed")
    config=json.loads((path.parent/"config/sdkconfig.json").read_text());validate_build_config(config)
    table=partitions((path.parent/"partition_table/partition-table.bin").read_bytes().ljust(4096,b"\xff"))
    if {k:tuple(v[x] for x in ("type","subtype","offset","size")) for k,v in table.items()}!=LAYOUT:
        raise ValueError("compiled partition table differs")
    if receipt.get("runtime_image_sha256")!=app_image(app).calc_digest.hex():raise ValueError("runtime digest differs")
    if receipt.get("hardware_validated") is not False or receipt.get("write_authorized") is not False:
        raise ValueError("build receipt is not an authorization")


def validate_baseline_runtime_binding(binding_path,baseline_path,limits_path,receipt_path,
                                      baseline,limits,receipt,app,profile,historical_manifest_sha):
    """Verify the sealed 7A whole-file-SHA to candidate runtime-SHA translation."""
    binding=json.loads(read_private(Path(binding_path)))
    derivation=binding.get("binding_derivation")
    if type(derivation) is not dict:raise ValueError("strict baseline identity translation is missing")
    if derivation.get("schema")=="tx1-7a-candidate-comparison-binding-v2":
        return validate_candidate_comparison_binding(binding_path,baseline_path,limits_path,
            receipt_path,baseline,limits,receipt,app,profile,historical_manifest_sha,binding)
    runtime_sha=app_image(app).calc_digest.hex()
    expected={
        "schema":"tx1-7a-file-to-runtime-binding-v1",
        "BASELINE_ID":"TX1-7A-20260926T152253Z-MIXED-V2",
        "original_result":artifact(Path(baseline_path)),
        "original_limits":artifact(Path(limits_path)),
        "build_receipt":artifact(Path(receipt_path)),
        "candidate_file_sha256":sha(app),
        "candidate_runtime_image_sha256":runtime_sha,
        "changed_field":"candidate_image_sha256",
        "measurements_and_limits_changed":False,
        "new_baseline":False,
        "write_authorized":False,
    }
    if any(derivation.get(key)!=value for key,value in expected.items()):
        raise ValueError("baseline identity translation provenance differs")
    if set(derivation)!=(set(expected)|{"original_config","reason"}):
        raise ValueError("baseline identity translation fields changed")
    config_record=derivation["original_config"]
    if type(config_record) is not dict or not isinstance(config_record.get("path"),str):
        raise ValueError("baseline measurement configuration reference missing")
    config_path=Path(config_record["path"])
    if artifact(config_path)!=config_record:raise ValueError("baseline measurement configuration changed")
    config=json.loads(read_private(config_path))
    from tx1_measure import canonical as measurement_canonical, digest as measurement_digest, validate_config
    validate_config(config)
    if (binding.get("phase")!="before" or binding.get("status")!="passed" or binding.get("fixture") is not False
            or binding.get("failures")!=[] or len(binding.get("windows",[]))!=3):
        raise ValueError("translated baseline is not a complete real 7A result")
    if binding.get("candidate_image_sha256")!=runtime_sha or binding.get("candidate_source_sha256")!=receipt.get("source_sha256"):
        raise ValueError("translated baseline runtime/source identity differs")
    translated=copy.deepcopy(binding);translated.pop("binding_derivation")
    translated["candidate_image_sha256"]=baseline.get("candidate_image_sha256")
    if translated!=baseline:raise ValueError("translation changed frozen measurements or baseline fields")
    if (baseline.get("candidate_image_sha256")!=sha(app)
            or baseline.get("candidate_source_sha256")!=receipt.get("source_sha256")
            or config.get("candidate_image_sha256")!=sha(app)
            or config.get("candidate_source_sha256")!=receipt.get("source_sha256")
            or config.get("old_manifest_sha256")!=historical_manifest_sha
            or config.get("conditions_sha256")!=baseline.get("conditions_sha256")
            or measurement_digest(measurement_canonical(config))!=baseline.get("config_sha256")
            or baseline.get("config_sha256")!=limits.get("config_sha256")
            or baseline.get("source_mac_sha256")!=sha(parse_mac(profile["ap_mac"]))):
        raise ValueError("original 7A configuration/source/build bindings differ")
    return artifact(Path(binding_path)),artifact(config_path)


def validate_candidate_comparison_binding(binding_path,baseline_path,limits_path,receipt_path,
        baseline,limits,receipt,app,profile,historical_manifest_sha,binding):
    """Bind a different candidate to the same measured baseline, without rewriting it."""
    validate_receipt(receipt,app,receipt_path)
    derivation=binding["binding_derivation"]
    original_binding_record=derivation.get("original_binding")
    original_receipt_record=derivation.get("original_build_receipt")
    if not isinstance(original_binding_record,dict) or not isinstance(original_receipt_record,dict):
        raise ValueError("original candidate comparison provenance missing")
    original_binding=json.loads(checked(original_binding_record))
    if original_binding.get("binding_derivation",{}).get("schema")!="tx1-7a-file-to-runtime-binding-v1":
        raise ValueError("comparison requires the original v1 binding; no recursive translations")
    original_receipt=json.loads(checked(original_receipt_record))
    original_receipt_path=Path(original_receipt_record["path"])
    original_app=receipt_candidate_path(original_receipt,original_receipt_path).read_bytes()
    validate_receipt(original_receipt,original_app,original_receipt_path)
    observed_original,config_record=validate_baseline_runtime_binding(
        original_binding_record["path"],baseline_path,limits_path,original_receipt_path,
        baseline,limits,original_receipt,original_app,profile,historical_manifest_sha)
    if observed_original!=original_binding_record or artifact(original_receipt_path)!=original_receipt_record:
        raise ValueError("original candidate comparison artifacts changed")
    expected={
        "schema":"tx1-7a-candidate-comparison-binding-v2",
        "BASELINE_ID":"TX1-7A-20260926T152253Z-MIXED-V2",
        "original_result":artifact(Path(baseline_path)),
        "original_limits":artifact(Path(limits_path)),
        "original_config":config_record,
        "original_binding":original_binding_record,
        "original_build_receipt":original_receipt_record,
        "build_receipt":artifact(Path(receipt_path)),
        "candidate_file_sha256":sha(app),
        "candidate_runtime_image_sha256":app_image(app).calc_digest.hex(),
        "changed_fields":["candidate_image_sha256","candidate_source_sha256"],
        "measurements_and_limits_changed":False,
        "new_baseline":False,
        "write_authorized":False,
    }
    if set(derivation)!=(set(expected)|{"reason"}) or any(derivation.get(k)!=v for k,v in expected.items()):
        raise ValueError("candidate comparison provenance differs")
    if not isinstance(derivation["reason"],str) or not derivation["reason"].strip():
        raise ValueError("candidate comparison reason missing")
    if (binding.get("candidate_image_sha256")!=expected["candidate_runtime_image_sha256"]
            or binding.get("candidate_source_sha256")!=receipt.get("source_sha256")):
        raise ValueError("candidate comparison runtime/source identity differs")
    restored=copy.deepcopy(binding);restored.pop("binding_derivation")
    for name in expected["changed_fields"]:restored[name]=baseline.get(name)
    if restored!=baseline:raise ValueError("comparison changed frozen measurements or baseline fields")
    return artifact(Path(binding_path)),config_record


def select_app1(old):
    if len(old)!=0x2000:raise ValueError("invalid otadata size")
    for offset,sequence in ((0,1),(0x1000,0)):
        if struct.unpack_from("<I",old,offset)[0]!=sequence or struct.unpack_from("<I",old,offset+24)[0]!=0xffffffff:
            raise ValueError("old OTA selection differs from secured snapshot")
        crc=zlib.crc32(old[offset:offset+4],0xffffffff)&0xffffffff
        if struct.unpack_from("<I",old,offset+28)[0]!=crc:raise ValueError("OTA CRC mismatch")
    result=bytearray(old)
    record=bytearray(b"\xff"*32)
    struct.pack_into("<I",record,0,2)  # sequence 2 -> ota_1, UNDEFINED; no rollback claim
    struct.pack_into("<I",record,28,zlib.crc32(record[:4],0xffffffff)&0xffffffff)
    result[0x1000:0x1020]=record
    return bytes(result)


def prepare(backup_path,historical_backup_path,receipt_path,profile_path,limits_path,baseline_path,idf,out,fixture=False,
            baseline_binding_path=None):
    backup,original=read_backup_pair(backup_path,require_pair=True)
    historical,historical_original=read_backup_pair(historical_backup_path,require_pair=True)
    historical_raw=Path(historical_backup_path).read_bytes()
    if (backup.get("reference_manifest_sha256")!=sha(historical_raw)
            and (not fixture or Path(backup_path).resolve()!=Path(historical_backup_path).resolve())):
        raise ValueError("current backup is not bound to the historical baseline backup")
    if backup.get("verification",{}).get("status") not in ("PASS","FIXTURE"):
        raise ValueError("current backup pair has not passed byte/hash verification")
    table=partitions(original[0x8000:0x9000])
    historical_table=partitions(historical_original[0x8000:0x9000])
    comparison=compare_flash_images(historical_original,original,historical_table,table)
    if comparison["status"]!="PASS":raise ValueError("current-vs-historical flash comparison is not migration-safe")
    table=partitions(original[0x8000:0x9000])
    if {k:tuple(v[x] for x in ("type","subtype","offset","size")) for k,v in table.items()}!=LAYOUT:
        raise ValueError("saved layout does not match reviewed migration profile")
    for part in backup["partitions"]:
        record=table[part["name"]]
        if record["offset"]!=part["offset"] or record["size"]!=part["size_bytes"]:
            raise ValueError("backup metadata/table disagreement")
    receipt=json.loads(receipt_path.read_text());candidate=receipt["candidate"]
    app_path=receipt_candidate_path(receipt,receipt_path);app=app_path.read_bytes()
    validate_receipt(receipt,app,receipt_path)
    if sha(app)!=candidate["sha256"] or len(app)!=candidate["size"] or len(app)>table["app1"]["size"]:
        raise ValueError("candidate hash/slot mismatch")
    image=app_image(app)
    if receipt.get("hardware_validated") is not False or receipt.get("write_authorized") is not False:
        raise ValueError("build receipt must distinguish software evidence from authorization")
    profile=json.loads(read_private(profile_path));validate(profile)
    if profile["ssid"]!="csi-test" or profile["channel"]!=6 or profile["rate_hz"]!=50:
        raise ValueError("private migration profile violates productive network contract")
    if profile["base_mac"].lower()!=backup["acquisition"]["factory_base_mac"].lower():
        raise ValueError("profile is bound to another board")
    baseline=json.loads(read_private(baseline_path));limits=json.loads(read_private(limits_path))
    if baseline.get("phase")!="before" or baseline.get("status")!="passed" or len(baseline.get("windows",[]))!=3:
        raise ValueError("successful old RX4 baseline is required")
    if baseline.get("fixture") is not False and not fixture:raise ValueError("fixtures/unknown evidence cannot authorize hardware")
    if limits.get("rule")!="old-three-window-envelope" or not limits.get("limits"):
        raise ValueError("frozen comparison limits missing")
    if (baseline.get("config_sha256")!=limits.get("config_sha256")
            or baseline.get("old_manifest_sha256")!=sha(historical_raw)
            or baseline.get("source_mac_sha256")!=sha(parse_mac(profile["ap_mac"]))):
        raise ValueError("baseline/limits/device source binding differs")
    from tx1_measure import freeze_limits
    frozen=freeze_limits(baseline["windows"], {})
    frozen["config_sha256"]=baseline["config_sha256"]
    if limits!=frozen:raise ValueError("limits are not the frozen old three-window envelope")
    runtime_sha=image.calc_digest.hex()
    baseline_binding_record=None;measurement_config_record=None
    if baseline_binding_path:
        baseline_binding_record,measurement_config_record=validate_baseline_runtime_binding(
            baseline_binding_path,baseline_path,limits_path,receipt_path,baseline,limits,receipt,app,profile,sha(historical_raw))
    elif not fixture and baseline.get("candidate_image_sha256")!=runtime_sha:
        raise ValueError("baseline whole-file identity needs its strict runtime-identity translation")
    if baseline_binding_record is None and (
            baseline.get("candidate_image_sha256")!=runtime_sha
            or baseline.get("candidate_source_sha256")!=receipt.get("source_sha256")):
        raise ValueError("baseline does not identify this candidate build")
    out.mkdir(mode=0o700,parents=False,exist_ok=False)
    comparison_path=Path(backup.get("comparison",{}).get("path",""))
    if comparison_path.is_file():
        comparison_record=checked(backup["comparison"])
        if json.loads(comparison_record)!=comparison:raise ValueError("stored flash comparison differs from recomputation")
        comparison_artifact=backup["comparison"]
    elif fixture:
        comparison_path=out/"flash-comparison.json";write_private(comparison_path,canonical(comparison))
        comparison_artifact=artifact(comparison_path)
    else:
        raise ValueError("current backup lacks its partition/sector comparison artifact")
    nvs=table["nvs"];ota=table["otadata"]
    merged=merge_nvs(original[nvs["offset"]:nvs["offset"]+nvs["size"]],profile,idf)
    selection=select_app1(original[ota["offset"]:ota["offset"]+ota["size"]])
    write_private(out/"nvs-merged.bin",merged);write_private(out/"otadata-app1.bin",selection)
    # All writes are derived from the saved table. Original app0 and table excluded.
    plan={"schema":2,"fixture":fixture,"backup_manifest":artifact(Path(backup_path)),
          "historical_backup_manifest":artifact(Path(historical_backup_path)),
          "historical_original":artifact(Path(historical_backup_path).parent/"flash-read-1.bin"),
          "comparison":comparison_artifact,"original":artifact(Path(backup_path).parent/"flash-read-1.bin"),
          "build_receipt":artifact(receipt_path),"baseline":artifact(baseline_path),"limits":artifact(limits_path),
          "baseline_binding":baseline_binding_record,"measurement_config":measurement_config_record,
          "profile":artifact(profile_path),"partitions":table,"device":backup["acquisition"],
          "idf_path":str(idf.resolve()),"nvs_parser":artifact(idf/"components/nvs_flash/nvs_partition_tool/nvs_parser.py"),
          "writes":[dict(artifact=artifact(app_path),offset=table["app1"]["offset"],kind="app1"),
                    dict(artifact=artifact(out/"nvs-merged.bin"),offset=nvs["offset"],kind="nvs"),
                    dict(artifact=artifact(out/"otadata-app1.bin"),offset=ota["offset"],kind="otadata")],
          "budgets":{"total_seconds":3600,"read_seconds":1200,"block_bytes":0x40000},
          "required_external_gates":list(GATES),"failure_policy":"stop-in-recovery; optional explicitly approved original restore",
          "bootloader_write":False,"partition_table_write":False,"automatic_rollback_proven":False}
    write_private(out/"plan.json",canonical(plan))
    return plan


def validate_plan(plan):
    if plan.get("schema")!=2 or plan.get("budgets")!={"total_seconds":3600,"read_seconds":1200,"block_bytes":0x40000}:
        raise ValueError("unknown plan/budget")
    original=checked(plan["original"])
    backup=json.loads(checked(plan["backup_manifest"]))
    historical=json.loads(checked(plan["historical_backup_manifest"]))
    historical_original=checked(plan["historical_original"])
    if plan["device"]!=backup["acquisition"] or sha(original)!=backup["files"]["flash-read-1.bin"]["sha256"]:
        raise ValueError("device/original is not bound to the backup manifest")
    current_pair_manifest,current_pair=read_backup_pair(Path(plan["backup_manifest"]["path"]),require_pair=True)
    if current_pair!=original or current_pair_manifest!=backup:
        raise ValueError("current restoration backup does not match its two readbacks")
    historical_pair_manifest,historical_pair=read_backup_pair(Path(plan["historical_backup_manifest"]["path"]),require_pair=True)
    if historical_pair!=historical_original or historical_pair_manifest!=historical:
        raise ValueError("historical comparison backup does not match its two readbacks")
    historical_raw=checked(plan["historical_backup_manifest"])
    if (backup.get("reference_manifest_sha256")!=sha(historical_raw)
            and (not plan.get("fixture") or plan["backup_manifest"]["path"]!=plan["historical_backup_manifest"]["path"])):
        raise ValueError("current backup is not bound to historical 7A baseline")
    if backup.get("verification",{}).get("status") not in ("PASS","FIXTURE"):
        raise ValueError("current backup pair verification missing")
    if len(original)!=0x1000000 or partitions(original[0x8000:0x9000])!=plan["partitions"]:
        raise ValueError("plan layout differs from original")
    if {k:tuple(v[x] for x in ("type","subtype","offset","size")) for k,v in plan["partitions"].items()}!=LAYOUT:
        raise ValueError("unreviewed layout")
    if plan.get("bootloader_write") is not False or plan.get("partition_table_write") is not False or plan.get("automatic_rollback_proven") is not False or plan.get("required_external_gates")!=list(GATES):
        raise ValueError("migration scope/gates changed")
    for name in ("build_receipt","baseline","limits","profile"):checked(plan[name])
    historical_table=partitions(historical_original[0x8000:0x9000])
    comparison=compare_flash_images(historical_original,original,historical_table,plan["partitions"])
    if comparison["status"]!="PASS" or json.loads(checked(plan["comparison"]))!=comparison:
        raise ValueError("immutable flash comparison or mutable-region classification changed")
    expected=("app1","nvs","otadata")
    if tuple(w["kind"] for w in plan["writes"])!=expected:raise ValueError("write set/order differs")
    for w in plan["writes"]:
        part=plan["partitions"][w["kind"]];data=checked(w["artifact"])
        if w["offset"]!=part["offset"] or len(data)>part["size"] or (w["kind"]!="app1" and len(data)!=part["size"]):
            raise ValueError("write outside approved partition")
    candidate=checked(plan["writes"][0]["artifact"])
    receipt=json.loads(checked(plan["build_receipt"]))
    validate_receipt(receipt,candidate,plan["build_receipt"]["path"])
    if sha(candidate)!=receipt["candidate"]["sha256"] or len(candidate)!=receipt["candidate"]["size"]:
        raise ValueError("candidate differs from build receipt")
    app_image(candidate)
    profile=json.loads(checked(plan["profile"]));validate(profile)
    if profile["ssid"]!="csi-test" or profile["channel"]!=6 or profile["rate_hz"]!=50 or profile["base_mac"].lower()!=plan["device"]["factory_base_mac"].lower():
        raise ValueError("productive profile differs")
    parser_path=Path(plan["idf_path"])/"components/nvs_flash/nvs_partition_tool/nvs_parser.py"
    if Path(plan["nvs_parser"]["path"])!=parser_path:raise ValueError("parser path differs")
    checked(plan["nvs_parser"])
    if checked(plan["writes"][1]["artifact"])!=merge_nvs(original[0x9000:0xe000],profile,Path(plan["idf_path"])):
        raise ValueError("NVS is not the additive import of this profile/original")
    baseline=json.loads(checked(plan["baseline"]));limits=json.loads(checked(plan["limits"]))
    from tx1_measure import freeze_limits
    frozen=freeze_limits(baseline["windows"],{});frozen["config_sha256"]=baseline["config_sha256"]
    runtime_sha=app_image(candidate).calc_digest.hex()
    binding_record=plan.get("baseline_binding");config_record=plan.get("measurement_config")
    if binding_record is None:
        if config_record is not None:raise ValueError("unbound baseline measurement configuration")
        image_identity_ok=baseline.get("candidate_image_sha256")==runtime_sha
    else:
        if config_record is None:raise ValueError("baseline measurement configuration binding missing")
        observed_binding,observed_config=validate_baseline_runtime_binding(
            binding_record["path"],plan["baseline"]["path"],plan["limits"]["path"],plan["build_receipt"]["path"],
            baseline,limits,receipt,candidate,profile,plan["historical_backup_manifest"]["sha256"])
        if observed_binding!=binding_record or observed_config!=config_record:
            raise ValueError("baseline identity translation artifacts changed")
        image_identity_ok=True
    if (limits!=frozen or baseline.get("phase")!="before" or baseline.get("status")!="passed"
            or baseline.get("old_manifest_sha256")!=plan["historical_backup_manifest"]["sha256"]
            or baseline.get("source_mac_sha256")!=sha(parse_mac(profile["ap_mac"]))
            or not image_identity_ok
            or (baseline.get("candidate_source_sha256")!=receipt.get("source_sha256") and binding_record is None)):
        raise ValueError("baseline/build/source/limits binding differs")
    if checked(plan["writes"][2]["artifact"])!=select_app1(original[0xe000:0x10000]):
        raise ValueError("unexpected OTA selection")
    return original


def validate_clearance(plan,plan_hash,clearance):
    if plan.get("fixture") or clearance.get("plan_sha256")!=plan_hash:
        raise ValueError("hardware execution forbidden for fixture/different plan")
    if clearance.get("device_secure_version")!=0:raise ValueError("secure-version state not resolved to reviewed value")
    if set(clearance.get("evidence",{}))!=set(GATES):raise ValueError("external compatibility gates incomplete")
    for record in clearance["evidence"].values():
        if not checked(record):raise ValueError("empty compatibility evidence")
    baseline=json.loads(checked(plan["baseline"]))
    if baseline.get("fixture") is not False or baseline.get("status")!="passed":raise ValueError("real old baseline missing")


def set_capture_state(state,stage,**details):
    if state is None:return
    state.clear();state.update({"stage":stage,**details})


def read_full(backend,deadline,clock=time.monotonic,progress=None):
    start=clock();blocks=[]
    for offset in range(0,0x1000000,0x40000):
        if clock()>deadline or clock()-start>1200:raise TimeoutError("full read time budget")
        if progress is not None:progress(offset,len(blocks))
        block=backend.read(offset,0x40000)
        if len(block)!=0x40000:raise ValueError("incomplete read")
        blocks.append(block)
    return b"".join(blocks)


def capture_pre_migration_backup(reference_path,out,backend_factory=None,clock=time.monotonic,status=None,
                                hold_in_bootloader=False,progress=None):
    if backend_factory is None:backend_factory=USB
    if status is None:status={}
    set_capture_state(status,"validate-reference-backup")
    reference_path=assert_private_path(reference_path);out=Path(out)
    if out.is_symlink() or out.exists():raise ValueError("backup output already exists or is a symlink")
    out=assert_private_path(out)
    historical,historical_image=read_backup_pair(reference_path,require_pair=True)
    historical_table=partitions(historical_image[0x8000:0x9000])
    if {k:tuple(v[x] for x in ("type","subtype","offset","size")) for k,v in historical_table.items()}!=LAYOUT:
        raise ValueError("historical backup layout differs from reviewed migration contract")
    out.mkdir(mode=0o700,parents=False,exist_ok=False)
    backend=backend_factory();readbacks=[];read_records=[];booted=False;boot_attempted=False
    if hasattr(backend,"capture_status"):backend.capture_status=status
    try:
        set_capture_state(status,"identify-device")
        backend.identify(historical["acquisition"])
        if progress is not None:progress({"stage":"identity-verified"})
        identity=getattr(backend,"identity",None)
        if not isinstance(identity,dict) or identity.get("factory_base_mac")!=historical["acquisition"]["factory_base_mac"]:
            raise ValueError("current USB identity did not match the historical TX1")
        for index in (1,2):
            started=datetime.now(timezone.utc).isoformat()
            def block_progress(offset,completed,index=index):
                set_capture_state(status,"flash-read-block",pass_index=index,offset=offset,completed_blocks=completed)
                if progress is not None:progress(dict(status))
            image=read_full(backend,clock()+1200,clock,progress=block_progress)
            path=out/f"flash-read-{index}.bin";write_private(path,image)
            readbacks.append(image)
            set_capture_state(status,"flash-read-saved",pass_index=index,completed_blocks=64)
            if progress is not None:progress(dict(status))
            read_records.append({"pass":index,"started_utc":started,"finished_utc":datetime.now(timezone.utc).isoformat(),
                                 "image":artifact(path),"device_identity":identity})
        if not hold_in_bootloader:
            set_capture_state(status,"normal-boot")
            boot_attempted=True;backend.boot();booted=True
    except Exception as error:
        status["failure_phase"]=status.get("stage")
        status["failure_type"]=type(error).__name__
        if isinstance(getattr(error,"errno",None),int):status["failure_errno"]=error.errno
        raise
    finally:
        if not hold_in_bootloader and not boot_attempted and getattr(backend,"esp",None) is not None:
            status["cleanup_stage"]="normal-boot-after-capture-stop"
            try:boot_attempted=True;backend.boot()
            except Exception as error:
                status["cleanup_failure_type"]=type(error).__name__
                if isinstance(getattr(error,"errno",None),int):status["cleanup_failure_errno"]=error.errno
                raise
        close=getattr(backend,"close",None)
        if close:close()
    byte_equal=readbacks[0]==readbacks[1]
    current_table=None;comparison=None
    if byte_equal:
        try:
            current_table=partitions(readbacks[0][0x8000:0x9000])
            comparison=compare_flash_images(historical_image,readbacks[0],historical_table,current_table)
        except (ValueError,struct.error) as error:
            comparison={"schema":1,"status":"HARD_STOP","reason":str(error),
                        "historical_sha256":sha(historical_image),"current_sha256":sha(readbacks[0]),
                        "partition_table_matches":False,"immutable_regions_unchanged":False,
                        "changed_sector_count":None,"changed_by_region":{},"changed_sectors":[]}
    else:
        comparison={"schema":1,"status":"UNREPRODUCIBLE","reason":"independent full readbacks differ",
                    "historical_sha256":sha(historical_image),"current_sha256":sha(readbacks[0]),
                    "second_read_sha256":sha(readbacks[1]),"partition_table_matches":None,
                    "immutable_regions_unchanged":None,"changed_sector_count":None,
                    "changed_by_region":{},"changed_sectors":[]}
    comparison_path=out/"flash-comparison.json";write_private(comparison_path,canonical(comparison))
    current_sha=sha(readbacks[0]);critical_ok=comparison.get("status")=="PASS"
    verification_status="PASS" if byte_equal and critical_ok else comparison.get("status","FAILED")
    acquisition=dict(historical["acquisition"])
    acquisition["current_readback"]={"reference_manifest_sha256":sha(reference_path.read_bytes()),
        "captured_utc":datetime.now(timezone.utc).isoformat(),"device_identity":identity,
        "independent_reads":read_records,"secure_version":identity.get("secure_version"),
        "flash_size_bytes":0x1000000}
    parts=current_table or historical_table
    manifest={"schema":1,"created_utc":datetime.now(timezone.utc).isoformat(),
        "boot_requested":booted,"hold_in_bootloader_requested":hold_in_bootloader,
        "reference_manifest_sha256":sha(reference_path.read_bytes()),"acquisition":acquisition,
        "files":{"flash-read-1.bin":artifact(out/"flash-read-1.bin"),
                 "flash-read-2.bin":artifact(out/"flash-read-2.bin")},
        "partitions":[dict(name=name,offset=value["offset"],size_bytes=value["size"])
                      for name,value in parts.items()],
        "comparison":artifact(comparison_path),
        "verification":{"status":verification_status,"flash_size_bytes":0x1000000,
            "readbacks_byte_equal":byte_equal,"readbacks_sha256_equal":sha(readbacks[0])==sha(readbacks[1]),
            "current_sha256":current_sha,"critical_regions_unchanged":comparison.get("immutable_regions_unchanged"),
            "comparison_status":comparison.get("status"),"secure_version":identity.get("secure_version")}}
    manifest_path=out/"manifest.json";write_private(manifest_path,canonical(manifest))
    result={"status":verification_status,"manifest":artifact(manifest_path),
            "readback_1_sha256":sha(readbacks[0]),"readback_2_sha256":sha(readbacks[1]),
            "readbacks_byte_equal":byte_equal,"comparison_status":comparison.get("status"),
            "changed_sector_count":comparison.get("changed_sector_count"),
            "changed_by_region":comparison.get("changed_by_region",{}),
            "critical_checks":comparison.get("critical_checks",{}),
            "secure_version":identity.get("secure_version"),"boot_requested":booted}
    write_private(out/"result.json",canonical(result))
    return result


def perform(plan,backend,operation,allow_restore=False,boot_after=False,clock=time.monotonic,readback_dir=None,progress=None):
    """Backend is supplied explicitly; tests use only bytearray-backed devices."""
    if operation not in ("migrate","restore"):raise ValueError("unknown operation")
    original=validate_plan(plan);deadline=clock()+3600;wrote=False
    def report(stage,**details):
        if progress is not None:progress({"stage":stage,**details})
    def full_read(stage):
        return read_full(backend,deadline,clock,
            progress=lambda offset,completed:report(stage,offset=offset,completed_blocks=completed,total_blocks=64))
    report("identify-device")
    backend.identify(plan["device"])
    report("identity-verified")
    try:
        before=full_read("pre-write-read")
        if readback_dir:write_private(Path(readback_dir)/"flash-read-before-write.bin",before)
        if operation=="migrate" and before!=original:raise ValueError("device changed since bound backup; no write")
        report("pre-write-read-complete",matches_bound_backup=before==original)
        if operation=="restore":
            report("restore-write",offset=0,size=len(original))
            wrote=True;backend.write(0,original)
            after_restore=full_read("restore-readback")
            if readback_dir:write_private(Path(readback_dir)/"flash-read-after-restore.bin",after_restore)
            if after_restore!=original:raise ValueError("restore readback differs")
        else:
            expected=bytearray(original)
            for w in plan["writes"]:
                if clock()>deadline:raise TimeoutError("write sequence time budget")
                data=checked(w["artifact"]);offset=w["offset"]
                report("migration-write",offset=offset,size=len(data))
                wrote=True;backend.write(offset,data)
                # Sector padding may be erased by normal esptool writes.
                end=offset+((len(data)+4095)//4096)*4096
                expected[offset:end]=data+b"\xff"*(end-offset-len(data))
                if backend.read(offset,end-offset)!=expected[offset:end]:raise ValueError("region verification failed")
            after_write=full_read("post-write-read")
            if readback_dir:write_private(Path(readback_dir)/"flash-read-after-write.bin",after_write)
            if after_write!=bytes(expected):raise ValueError("full post-write comparison differs")
        if clock()>deadline:raise TimeoutError("operation time budget")
        report("full-flash-verified")
        if boot_after:backend.boot()
        report("complete",boot_requested=boot_after)
        return PASS
    except (OSError,ValueError,RuntimeError,TimeoutError) as error:
        # Never improvise retries. One pre-authorized recovery, only after writing began.
        if operation=="migrate" and wrote and allow_restore and not isinstance(error,TimeoutError) and clock()<deadline:
            backend.identify(plan["device"])
            report("recovery-write",offset=0,size=len(original))
            backend.write(0,original)
            after_restore=full_read("recovery-readback")
            if readback_dir:write_private(Path(readback_dir)/"flash-read-after-restore.bin",after_restore)
            if after_restore!=original:raise ValueError("single recovery attempt failed")
            if boot_after:backend.boot()
            report("recovery-complete",boot_requested=boot_after)
            return RESTORED
        raise


def compare_security_info(actual,expected):
    """Compare every pinned esptool 4.12 ROM field; JSON changes only tuple to list."""
    integers={"flags":32,"flash_crypt_cnt":8,"chip_id":32,"api_version":32}
    flags=("SECURE_BOOT_EN","SECURE_BOOT_AGGRESSIVE_REVOKE","SECURE_DOWNLOAD_ENABLE",
           "SECURE_BOOT_KEY_REVOKE0","SECURE_BOOT_KEY_REVOKE1","SECURE_BOOT_KEY_REVOKE2",
           "SOFT_DIS_JTAG","HARD_DIS_JTAG","DIS_USB","DIS_DOWNLOAD_DCACHE","DIS_DOWNLOAD_ICACHE")
    required=set(integers)|{"key_purposes","parsed_flags"}

    def fields(info,source):
        if type(info) is not dict:
            raise ValueError(f"security_info_rom {source}: expected dictionary")
        if set(info)!=required:
            raise ValueError(f"security_info_rom {source}: missing={sorted(required-set(info))!r}; "
                             f"unexpected={sorted(set(info)-required,key=str)!r}")
        result={}
        for name,width in integers.items():
            value=info[name]
            if type(value) is not int or not 0<=value<1<<width:
                raise ValueError(f"security_info_rom.{name} {source}: expected uint{width}, actual={value!r}")
            result[name]=value
        purposes=info["key_purposes"]
        if type(purposes) not in (list,tuple) or len(purposes)!=7:
            raise ValueError(f"security_info_rom.key_purposes {source}: expected seven list/tuple entries")
        for index,value in enumerate(purposes):
            name=f"key_purposes[{index}]"
            if type(value) is not int or not 0<=value<256:
                raise ValueError(f"security_info_rom.{name} {source}: expected uint8, actual={value!r}")
            result[name]=value
        parsed=info["parsed_flags"]
        if type(parsed) is not dict:
            raise ValueError(f"security_info_rom.parsed_flags {source}: expected dictionary")
        if set(parsed)!=set(flags):
            raise ValueError(f"security_info_rom.parsed_flags {source}: missing={sorted(set(flags)-set(parsed))!r}; "
                             f"unexpected={sorted(set(parsed)-set(flags),key=str)!r}")
        for name in flags:
            value=parsed[name]
            if type(value) is not bool:
                raise ValueError(f"security_info_rom.parsed_flags.{name} {source}: expected bool, actual={value!r}")
            result[f"parsed_flags.{name}"]=value
        return result

    before=fields(expected,"backup");current=fields(actual,"ROM")
    differences=[f"{name}: expected={value!r}, actual={current[name]!r}"
                 for name,value in before.items() if current[name]!=value]
    if differences:
        raise ValueError("security state changed since backup: "+"; ".join(differences))


def select_tx1_usb_port(ports,expected):
    """Allow only the backed-up bridge or TX1's confirmed native USB identity."""
    bridge=expected["usb"]
    native=(0x303a,0x1001,expected["factory_base_mac"].upper(),
            "USB JTAG/serial debug unit","Espressif")
    matches=[]
    for port in ports:
        historical=(port.vid,port.pid,port.serial_number,port.location,port.product,port.manufacturer)==(
            bridge["vid"],bridge["pid"],bridge["serial_number"],bridge["location"],
            bridge["product"],bridge["manufacturer"])
        confirmed_native=(port.vid,port.pid,port.serial_number,port.product,port.manufacturer)==native
        if historical or confirmed_native:
            matches.append((port,"usb_reset" if confirmed_native else "default_reset"))
    if len(matches)!=1:raise ValueError("TX1 USB access path absent or ambiguous")
    return matches[0]


class USB:
    """Loaded only after explicit start, approval digest and external gates."""
    def __init__(self):self.esp=None;self.identity=None;self.secure_version=None;self.capture_status=None
    def _capture_phase(self,stage,**details):set_capture_state(self.capture_status,stage,**details)
    def identify(self,expected):
        self._capture_phase("load-usb-backend")
        import esptool
        from esptool.targets.esp32s3 import ESP32S3ROM
        from serial.tools import list_ports
        import subprocess
        if esptool.__version__!="4.12.0":raise ValueError("unreviewed esptool version")
        self._capture_phase("select-usb-path")
        port,reset_mode=select_tx1_usb_port(list_ports.comports(),expected)
        if self.esp is None:
            occupied=subprocess.run(["/usr/sbin/lsof","-t",port.device],capture_output=True,timeout=3)
            if occupied.returncode not in (0,1) or occupied.stdout.strip():raise ValueError("serial port already owned/unknown")
            self.esp=ESP32S3ROM(port.device,baud=115200)
            self.esp._port.exclusive=True
            # esptool's command timeout is temporary; streaming reads also need a bound.
            self.esp._port.timeout=3
            self._capture_phase("rom-connect")
            self.esp.connect(mode=reset_mode,attempts=1)
        elif self.esp._port.port!=port.device:
            raise ValueError("TX1 USB path changed during session")
        esp=self.esp
        self._capture_phase("rom-security-check")
        info=esp.get_security_info(cache=False)
        compare_security_info(info,expected["security_info_rom"])
        if info["chip_id"]!=ESP32S3ROM.IMAGE_CHIP_ID or info["flash_crypt_cnt"]!=0 or esp.secure_download_mode or any(
                info["parsed_flags"].get(k) for k in ("SECURE_BOOT_EN","SECURE_DOWNLOAD_ENABLE")):
            raise ValueError("security state differs; no bypass/stub")
        words=[esp.read_reg(0x6000702c+4*i) for i in range(6)]
        bits=sum(v<<(32*i) for i,v in enumerate(words))
        self.secure_version=(bits>>142)&0xffff
        for pos,width in ((42,1),(43,1),(82,3),(116,1),(128,1),(133,1),(142,16)):
            if (bits>>pos)&((1<<width)-1):raise ValueError("security/download/anti-rollback gate failed")
        mac=":".join(f"{v:02x}" for v in esp.read_mac())
        if mac!=expected["factory_base_mac"] or esp.get_chip_description()!=expected["chip"]:
            raise ValueError("ESP identity differs")
        self.identity={"factory_base_mac":mac,"chip":esp.get_chip_description(),
                       "secure_version":self.secure_version,"security_info_rom":info,
                       "usb_device":port.device,"usb_vid":port.vid,"usb_pid":port.pid,
                       "usb_serial_number":port.serial_number,"usb_location":port.location,
                       "usb_product":port.product,"usb_manufacturer":port.manufacturer,
                       "reset_mode":reset_mode}
        self._capture_phase("stub-upload")
        if not esp.IS_STUB:self.esp=esp.run_stub()
        self._capture_phase("flash-interface-setup")
        self.esp.flash_spi_attach(0)
        self._capture_phase("flash-id-check")
        self.identity["flash_jedec_id"]=hex(self.esp.flash_id())
        if self.identity["flash_jedec_id"]!=expected["flash_jedec_id"]:raise ValueError("physical flash differs")
        self._capture_phase("flash-parameters")
        self.esp.flash_set_parameters(0x1000000)
        self._capture_phase("stub-baud-change")
        self.esp.change_baud(230400)
        self._capture_phase("identify-complete")
    def read(self,offset,length):
        if self.capture_status is not None:
            self.capture_status.update({"stage":"flash-read-io","offset":offset,"length":length})
        return self.esp.read_flash(offset,length)
    def write(self,offset,data):
        from esptool.cmds import write_flash
        datafile=io.BytesIO(data);datafile.name="manifest-bound-private-image.bin"
        args=argparse.Namespace(chip="esp32s3",addr_filename=[(offset,datafile)],
            flash_mode="keep",flash_freq="keep",flash_size="keep",compress=True,no_compress=False,
            no_stub=False,force=False,encrypt=False,encrypt_files=None,erase_all=False,
            ignore_flash_encryption_efuse_setting=False,verify=False)
        # Disable esptool's automatic reconnect/reflash loop. Recovery is explicit.
        self.esp.WRITE_FLASH_ATTEMPTS=1
        write_flash(self.esp,args)
    def boot(self):
        self.esp._port.setDTR(False);self.esp._port.setRTS(True);time.sleep(.1);self.esp._port.setRTS(False)
    def close(self):
        if self.esp is not None:self.esp._port.close()


def main(argv=None):
    p=argparse.ArgumentParser(description=__doc__);sub=p.add_subparsers(dest="command",required=True)
    prep=sub.add_parser("prepare",help="offline only; creates a private proposed write plan")
    for name in ("backup-manifest","historical-backup-manifest","build-receipt","profile","limits","baseline","idf-path","output"):
        prep.add_argument("--"+name,type=Path,required=True)
    prep.add_argument("--baseline-binding",type=Path,help="sealed translation when 7A stored the candidate whole-file SHA")
    capture=sub.add_parser("capture",help="read and retain two complete private pre-migration flash snapshots")
    capture.add_argument("--reference-backup-manifest",type=Path,required=True)
    capture.add_argument("--output",type=Path,required=True)
    capture.add_argument("--start",action="store_true",help="explicitly permit read-only TX1 USB access and reset")
    capture.add_argument("--approve-reference-sha256",help="must match the verified historical backup manifest")
    capture.add_argument("--hold-in-bootloader",action="store_true",help="do not resume the application after capture or capture failure; prevents NVS drift before a separately approved migration")
    run=sub.add_parser("run",help="offline artifact validation unless --start is explicitly given")
    run.add_argument("--plan",type=Path,required=True);run.add_argument("--operation",choices=("migrate","restore"),required=True)
    run.add_argument("--output",type=Path,required=True);run.add_argument("--start",action="store_true")
    run.add_argument("--approve-plan-sha256");run.add_argument("--clearance",type=Path)
    run.add_argument("--approve-restore-sha256",help="explicitly authorizes one original restore after migration failure")
    run.add_argument("--boot-after-verify",action="store_true")
    args=p.parse_args(argv);backend=None;created=False;previous_alarm=None
    try:
        os.umask(0o077)
        if args.command=="prepare":
            prepare(args.backup_manifest,args.historical_backup_manifest,args.build_receipt,args.profile,args.limits,args.baseline,args.idf_path,args.output,
                    baseline_binding_path=args.baseline_binding)
            print("Private USB plan prepared; external gates and explicit start still required.");return PASS
        if args.command=="capture":
            if not args.start:raise ValueError("read-only device access requires --start")
            args.reference_backup_manifest=assert_private_path(args.reference_backup_manifest)
            args.output=assert_private_path(args.output)
            if args.output.is_symlink() or args.output.exists():raise ValueError("backup output already exists or is a symlink")
            ref_raw=read_private(args.reference_backup_manifest)
            if args.approve_reference_sha256!=sha(ref_raw):raise ValueError("reference backup digest approval absent/mismatched")
            previous_alarm=signal.signal(signal.SIGALRM,snapshot_deadline_expired)
            signal.alarm(2400)
            capture_status={}
            try:result=capture_pre_migration_backup(args.reference_backup_manifest,args.output,status=capture_status,
                    hold_in_bootloader=args.hold_in_bootloader,
                    progress=lambda event:print(json.dumps({"usb_progress":event}),flush=True))
            except (OSError,ValueError,RuntimeError,KeyError,TypeError,TimeoutError,ImportError) as error:
                if args.output.is_dir():
                    write_private(args.output/"failure.json",canonical({"status":"stopped","reason":str(error),
                        "no_retry":True,"capture_state":capture_status}))
                print("Read-only flash snapshot stopped; inspect the private result before a new attempt.",file=sys.stderr)
                return FAILED
            if result["status"]!="PASS":
                print("Read-only flash snapshot retained but failed a backup invariant.",file=sys.stderr)
                return FAILED
            print("Two independent TX1 flash readbacks matched; private comparison is ready for review.")
            return PASS
        raw=read_private(args.plan);plan=json.loads(raw);validate_plan(plan)
        if args.start:
            if args.approve_plan_sha256!=sha(raw) or args.clearance is None:raise ValueError("explicit plan approval/clearance absent")
            validate_clearance(plan,sha(raw),json.loads(read_private(args.clearance)))
            if args.approve_restore_sha256 and args.approve_restore_sha256!=plan["original"]["sha256"]:
                raise ValueError("recovery approval belongs to another original")
            args.output=assert_private_path(args.output)
        args.output.mkdir(mode=0o700,parents=False,exist_ok=False);created=True
        if not args.start:
            write_private(args.output/"result.json",canonical({"status":"offline-valid","device_access":False,
                          "external_gates":list(GATES),"write_authorized":False}))
            print("Offline artifact validation passed; no device opened.");return PASS
        with (args.output/"usb-private.log").open("x",buffering=1) as log,contextlib.redirect_stdout(log),contextlib.redirect_stderr(log):
            def deadline_expired(signum,frame):raise TimeoutError("absolute 3600-second USB budget")
            previous_alarm=signal.signal(signal.SIGALRM,deadline_expired);signal.alarm(3600)
            backend=USB()
            result=perform(plan,backend,args.operation,bool(args.approve_restore_sha256),args.boot_after_verify,readback_dir=args.output,
                           progress=lambda event:print(json.dumps({"usb_progress":event}),flush=True))
        write_private(args.output/"result.json",canonical({"exit_code":result,"operation":args.operation,
                       "boot_requested":args.boot_after_verify,"external_network_acceptance":"not performed"}))
        return result
    except (OSError,ValueError,RuntimeError,KeyError,TypeError,TimeoutError,ImportError) as error:
        if created:
            write_private(args.output/"failure.json",canonical({"status":"stopped","reason":str(error),"no_retry":True}))
        print("USB workflow refused/stopped. No additional attempt; inspect private result.",file=sys.stderr)
        return FAILED if backend else REFUSED
    finally:
        if previous_alarm is not None:
            signal.alarm(0);signal.signal(signal.SIGALRM,previous_alarm)
        if backend:backend.close()


if __name__=="__main__":raise SystemExit(main())

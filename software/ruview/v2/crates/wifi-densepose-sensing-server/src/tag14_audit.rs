//! Deterministic, read-only audit of closed unified RX recordings.
//! This proves raw accounting and the original 3-second frame-window gates.
//! Device/RPC/PCAP/resource/end-state acceptance remains separate evidence.
use super::{position_artifact, position_setup::SealedPositionSetup, raw_csi_recording as raw};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest,Sha256};
use std::{collections::{BTreeMap,BTreeSet},fs::File,io::{BufRead,BufReader,Read},path::Path};

#[derive(Deserialize)]
struct Metadata {
    recording_id:String, setup_id:String, setup_sha256:String, sealed_setup:SealedPositionSetup,
    status:String,incomplete:bool,dropped_frames:u64,writer_error:Option<serde_json::Value>,
    frames_written:u64,raw_sha256:String,clock_epoch_id:String,
    started_at_monotonic_ns:u64,ended_at_monotonic_ns:u64,
}
#[derive(Default)]
struct Link { observed:u64, qualified:Vec<u64>, layouts:BTreeMap<String,u64> }

fn qualified(frame:&raw::RawCsiFrame) -> bool {
    let Some(identity)=frame.protocol_identity else { return false };
    let Some(unified)=identity.unified_rx else { return false };
    frame.center_frequency_mhz==2437 && frame.antenna_count==1 && frame.ppdu_type==0
        && frame.flags & !(raw::TRANSIENT_SYNC_FLAG|32)==0 && identity.rx_state==Some(0)
        && crate::csi_layout::qualified(frame).is_ok()
        && frame.physical_layout.is_some_and(|e|e.processing==15 && e.shift==0)
        && identity.csi_config_flags==Some(11)
        && if unified.source_node_id==0 {frame.subcarrier_count==128 && unified.sig_mode==1}
            else {frame.subcarrier_count==64 && unified.sig_mode==0}
}

// Exact historical rule: windows start at seconds 5..duration-3 and last 3s.
// At least 15 TX / 6 peer frames, >=2.5s coverage, no gap >=1s, >=5 / 2 Hz.
fn window(times:&[u64], begin:u64, peer:bool)->Option<(u64,u64)> {
    let lo=times.partition_point(|time| *time<begin);
    let hi=times.partition_point(|time| *time<begin+3_000_000_000);
    let selected=&times[lo..hi];
    if selected.len() < if peer {6} else {15} { return None; }
    let first=*selected.first()?; let last=*selected.last()?; let span=last-first;
    if span<2_500_000_000 || selected.windows(2).any(|pair| pair[1]<=pair[0] || pair[1]-pair[0]>=1_000_000_000)
        || (selected.len()-1) as u128*1_000_000_000 < (if peer {2} else {5})*u128::from(span) { return None; }
    Some((first,last))
}

fn required_windows(seconds:u16)->u64 {
    let denominator=u64::from(seconds)-7;
    match seconds {
        60=>48,180=>156,300=>264,
        // The separately authorized stability run requires >=90% of windows.
        3600=>(9*denominator+9)/10,
        _=>denominator,
    }
}

pub(crate) fn run(path:&Path, seconds:u16)->Result<serde_json::Value,String> {
    if !matches!(seconds,30|60|180|300|3600) { return Err("audit duration must be 30, 60 diagnostic, 180, 300 or 3600 seconds".into()); }
    let name=path.file_name().and_then(|v|v.to_str()).ok_or("recording filename missing")?;
    let id=name.strip_suffix(raw::RAW_CSI_FILE_SUFFIX).ok_or("raw CSI suffix missing")?;
    raw::validate_recording_id(id).map_err(|error|error.to_string())?;
    let metadata_path=path.with_file_name(format!("{id}.raw-csi.v1.meta.json"));
    let metadata_bytes=std::fs::read(&metadata_path).map_err(|e|e.to_string())?;
    let meta:Metadata=serde_json::from_slice(&metadata_bytes).map_err(|e|e.to_string())?;
    meta.sealed_setup.validate_recording_identity(Some(&meta.setup_id),Some(&meta.setup_sha256))?;
    if meta.recording_id!=id || meta.status!="completed" || meta.incomplete || meta.dropped_frames!=0
        || meta.writer_error.is_some_and(|value|!value.is_null()) { return Err("recording is not completely closed without reported loss".into()); }
    let begin=meta.started_at_monotonic_ns;
    let end=begin.checked_add(u64::from(seconds)*1_000_000_000).ok_or("phase time overflow")?;
    let end_clamp=meta.ended_at_monotonic_ns.checked_sub(end).ok_or("recording closed before the fixed phase end")?;
    let mut reader=BufReader::new(File::open(path).map_err(|e|e.to_string())?);
    let mut digest=Sha256::new(); let mut line=Vec::new(); let mut count=0u64;
    let mut epochs=raw::RecordingEpochGuard::default(); let mut previous_time=0;
    let mut wire_hashes=BTreeSet::new(); let mut retry_hashes=BTreeSet::new();
    let mut wall_anchor:Option<(i128,i128)>=None; let mut wall_drift=0u64;
    let mut sequences=BTreeMap::<u8,crate::csi_identity::SequenceWindow>::new();
    let sequence_origin=std::time::Instant::now(); let mut reordered_frames=0u64;
    let mut links=BTreeMap::<(u8,u8),Link>::new(); let mut expected=BTreeSet::new();
    for rx in 1..=4 {
        let profile=meta.sealed_setup.rx_profile(rx).ok_or("Tag14 requires all four unified profile bindings")?;
        let mode_ok=match seconds {
            30|180 => profile.mode==0,
            60|300 => profile.mode==if rx==1 || rx==4 {1} else {0},
            _ => matches!(profile.mode,0|3|4),
        };
        if profile.rate_hz!=50 || !mode_ok {return Err("Tag14 profile/rate differs from the bounded phase contract".into());}
        expected.insert((rx,0));
        for source in &profile.peer_ids {expected.insert((rx,*source));}
    }
    loop {
        line.clear(); let bytes=reader.by_ref().take(32769).read_until(b'\n',&mut line).map_err(|e|e.to_string())?;
        if bytes==0 {break;}
        if bytes>32768 || line.last()!=Some(&b'\n') || count>=1_000_000 {return Err("raw line/frame bound exceeded or unterminated line".into());}
        digest.update(&line);
        let frame=raw::decode_json_line(std::str::from_utf8(&line).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
        let identity=frame.protocol_identity.ok_or("boot identity missing")?;
        identity.unified_rx.ok_or("Tag14 unified audit requires protocol v4")?;
        meta.sealed_setup.validate_raw_csi_source_identity(&frame)?;
        epochs.validate(&frame)?; epochs.observe_written(&frame);
        if frame.session_id.as_deref()!=Some(id) || frame.clock_epoch_id.as_deref()!=Some(&meta.clock_epoch_id) {
            return Err("recording/session clock identity changed".into());
        }
        let time=frame.host_monotonic_ns.ok_or("monotonic frame time missing")?;
        if time<previous_time || time<begin || time>meta.ended_at_monotonic_ns {return Err("frame lies outside closed monotonic capture".into());}
        previous_time=time;
        if let Some((first_wall,first_mono))=wall_anchor {
            let difference=(i128::from(frame.host_timestamp_unix_ns)-first_wall)-(i128::from(time)-first_mono);
            wall_drift=wall_drift.max(u64::try_from(difference.abs()).unwrap_or(u64::MAX));
        } else {wall_anchor=Some((i128::from(frame.host_timestamp_unix_ns),i128::from(time)));}
        let mut packet=frame.to_packet().map_err(|error|error.to_string())?;
        wire_hashes.insert(position_artifact::sha256_bytes(&packet));
        let stamp=raw::ESP32_CSI_HEADER_LEN+frame.iq_pairs.len()*2+raw::TX_SOURCE_BINDING_TRAILER_LEN+88;
        packet[stamp..stamp+8].fill(0); // Only the v4 send-attempt timestamp varies on a retry.
        retry_hashes.insert(position_artifact::sha256_bytes(&packet));
        let arrival=sequence_origin.checked_add(std::time::Duration::from_nanos(time-begin))
            .ok_or("sequence arrival time overflow")?;
        if let Some(sequence)=sequences.get_mut(&frame.rx_id) {
            match sequence.admit(frame.sequence,arrival,true) {
                crate::csi_identity::Admission::Accepted { .. } => {},
                crate::csi_identity::Admission::Reordered => reordered_frames+=1,
                _ => return Err("duplicate or expired/backward receiver sequence".into()),
            }
        } else {
            sequences.insert(frame.rx_id,crate::csi_identity::SequenceWindow::new(frame.sequence,arrival));
        }
        let unified=identity.unified_rx.unwrap();
        let link=links.entry((frame.rx_id,unified.source_node_id)).or_default();
        link.observed+=1;
        *link.layouts.entry(format!("{}:{}:{}:{:02x}",frame.antenna_count,frame.subcarrier_count,frame.ppdu_type,frame.flags)).or_default()+=1;
        if time<end && qualified(&frame) && (unified.source_node_id!=0
            || meta.sealed_setup.raw_csi_frame_matches_expected_grid(&frame)?) {link.qualified.push(time);}
        count+=1;
    }
    let wire_gaps:u64=sequences.values().map(|sequence|sequence.gaps()).sum();
    let raw_hash=format!("{:x}",digest.finalize());
    if count!=meta.frames_written || raw_hash!=meta.raw_sha256 {return Err("closed row count/hash differs from actual raw bytes".into());}
    for rx in 1..=4 {expected.insert((rx,0));}
    let denominator=u64::from(seconds)-7;
    let required=required_windows(seconds);
    let mut results=Vec::new(); let mut all_pass=end_clamp<=50_000_000 && wall_drift<=50_000_000; let mut joint=0u64;
    for key in &expected {
        let empty=Link::default(); let link=links.get(key).unwrap_or(&empty);
        let peer=key.1!=0;
        let windows=(5..u64::from(seconds)-2).filter(|s|window(&link.qualified,begin+s*1_000_000_000,peer).is_some()).count() as u64;
        let minimum=if peer {2*u64::from(seconds)} else {5*u64::from(seconds)};
        let pass=windows>=required && link.qualified.len() as u64>=minimum;
        all_pass &=pass;
        results.push(json!({"receiver_node":key.0,"source_node":key.1,"observed":link.observed,
            "qualified_frames":link.qualified.len(),"qualified_windows":windows,"window_denominator":denominator,
            "required_windows":required,"layouts":link.layouts,"raw_frame_gate_pass":pass}));
    }
    for second in 5..u64::from(seconds)-2 {
        let ranges:Option<Vec<_>>=(1..=4).map(|rx| links.get(&(rx,0)).and_then(|link|window(&link.qualified,begin+second*1_000_000_000,false))).collect();
        if let Some(ranges)=ranges {
            let first=ranges.iter().map(|r|r.0).max().unwrap(); let last=ranges.iter().map(|r|r.1).min().unwrap();
            if last.saturating_sub(first)>=2_500_000_000 {joint+=1;}
        }
    }
    all_pass &=joint>=required;
    Ok(json!({"schema":"observatory.tag14-raw-audit.v1","recording_id":id,"duration_seconds":seconds,
        "raw_sha256":raw_hash,"metadata_sha256":position_artifact::sha256_bytes(&metadata_bytes),"frames":count,"end_clamp_ns":end_clamp,"links":results,
        "joint_primary_windows":joint,"wire_sequence_gaps_unexplained":wire_gaps,"reordered_frames":reordered_frames,
        "wire_payload_sha256s":wire_hashes,"wire_retry_normalized_sha256s":retry_hashes,"maximum_wall_clock_drift_ns":wall_drift,
        "raw_window_gates_pass":all_pass,"full_tag14_acceptance":false,
        "acceptance_contract":if seconds==60 {"functional-pair-60s-not-Tag14"} else {"standard-phase-evidence"},
        "remaining_evidence":["device qualification/resources","PCAP/host/firmware accounting and clock proof","Default and process/storage closure"],
        "gain_and_clipping":"unknown","scope":"closed raw bytes and qualified frame windows"}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn continuous_uses_original_ninety_percent_and_preserves_short_phase_contracts() {
        assert_eq!(required_windows(30),23);
        assert_eq!(required_windows(60),48);
        assert_eq!(required_windows(180),156);
        assert_eq!(required_windows(300),264);
        assert_eq!(required_windows(3600),3234);
        assert!(3233<required_windows(3600));
        assert!(3234>=required_windows(3600));
    }

    fn rewrite_frames(path:&Path, edit:impl FnOnce(&mut Vec<raw::RawCsiFrame>)) {
        let mut frames:Vec<_>=std::fs::read_to_string(path).unwrap().lines()
            .map(|line|raw::decode_json_line(line).unwrap()).collect();
        edit(&mut frames);
        let text:String=frames.iter().map(|frame|raw::encode_json_line(frame).unwrap()).collect();
        std::fs::write(path,text).unwrap();
        let metadata=path.with_file_name("synthetic-tag14.raw-csi.v1.meta.json");
        let mut meta:serde_json::Value=serde_json::from_slice(&std::fs::read(&metadata).unwrap()).unwrap();
        meta["raw_sha256"]=json!(position_artifact::sha256_file(path).unwrap());
        meta["frames_written"]=json!(frames.len());
        std::fs::write(metadata,serde_json::to_vec(&meta).unwrap()).unwrap();
    }

    #[test]
    fn reordered_raw_preserves_arrival_gates_and_counts_only_remaining_gaps() {
        let (_dir,path)=capture_at_rate(180,false,true); let original=run(&path,180).unwrap();
        rewrite_frames(&path,|frames| {
            let indices:Vec<_>=frames.iter().enumerate().filter(|(_,f)|f.rx_id==2)
                .take(2).map(|(i,_)|i).collect();
            let a=indices[0]; let b=indices[1];
            let (at,am)=(frames[a].host_timestamp_unix_ns,frames[a].host_monotonic_ns);
            let (bt,bm)=(frames[b].host_timestamp_unix_ns,frames[b].host_monotonic_ns);
            frames.swap(a,b);
            frames[a].host_timestamp_unix_ns=at;frames[a].host_monotonic_ns=am;
            frames[b].host_timestamp_unix_ns=bt;frames[b].host_monotonic_ns=bm;
        });
        let reordered=run(&path,180).unwrap();
        assert_eq!(reordered["reordered_frames"],1);
        assert_eq!(reordered["wire_sequence_gaps_unexplained"],0);
        for field in ["raw_window_gates_pass","joint_primary_windows","links","end_clamp_ns"] {
            assert_eq!(reordered[field],original[field]);
        }
        rewrite_frames(&path,|frames| {frames.retain(|f| !(f.rx_id==2 && f.sequence==20));});
        assert_eq!(run(&path,180).unwrap()["wire_sequence_gaps_unexplained"],1);
    }

    #[test]
    fn duplicates_stale_reordering_and_receive_clock_regression_fail_audit() {
        for case in 0..3 {
            let (_dir,path)=capture_at_rate(30,false,true);
            rewrite_frames(&path,|frames| {
                let indices:Vec<_>=frames.iter().enumerate().filter(|(_,f)|f.rx_id==2)
                    .take(10).map(|(i,_)|i).collect();
                match case {
                    0=>frames[indices[1]].sequence=frames[indices[0]].sequence,
                    1=>{let a=indices[0];let b=indices[9];
                        let seq=frames[a].sequence;frames[a].sequence=frames[b].sequence;frames[b].sequence=seq;},
                    _=>frames[indices[1]].host_monotonic_ns=Some(0),
                }
            });
            assert!(run(&path,30).is_err(),"negative case {case}");
        }
    }

    #[test]
    fn historical_window_rules_have_exact_coverage_rate_and_missing_link_behavior() {
        let tx:Vec<_>=(0..15).map(|i|5_000_000_000+i*200_000_000).collect();
        assert!(window(&tx,5_000_000_000,false).is_some());
        assert!(window(&tx[..14],5_000_000_000,false).is_none());
        let peer:Vec<_>=(0..6).map(|i|5_000_000_000+i*500_000_000).collect();
        assert!(window(&peer,5_000_000_000,true).is_some());
        assert!(window(&[],5_000_000_000,true).is_none());
        let mut gap=tx; gap[5]=gap[4]; assert!(window(&gap,5_000_000_000,false).is_none());
    }
    fn capture(seconds:u16, omit_peer:bool) -> (tempfile::TempDir,std::path::PathBuf) {
        capture_at_rate(seconds, omit_peer, false)
    }
    fn capture_at_rate(seconds:u16, omit_peer:bool, full_rate:bool) -> (tempfile::TempDir,std::path::PathBuf) {
        use std::io::Write;
        let mode=if matches!(seconds,60|300) {1} else {0};
        let (setup,template)=crate::position_setup::tests::unified_setup_and_frame(mode);
        let directory=tempfile::TempDir::new().unwrap(); let id="synthetic-tag14";
        let path=directory.path().join(format!("{id}.raw-csi.v1.jsonl"));
        let mut file=File::create(&path).unwrap(); let mut count=0; let mut sequence=[0u32;5];
        let ticks_per_second=if full_rate {200} else {10};
        let tx_every=if full_rate {if seconds==300 {5} else {4}} else {2};
        let peer_every=if full_rate {20} else {5};
        for tick in 0..u64::from(seconds)*ticks_per_second {
            let time=1_000_000+tick*(1_000_000_000/ticks_per_second);
            let mut sources=Vec::new();
            for rx in 1..=4 {
                let interval=if full_rate && seconds==300 && (rx==2 || rx==3) {4} else {tx_every};
                if tick%interval==0 {sources.push((rx,0));}
            }
            if matches!(seconds,60|300) && tick%peer_every==0 {sources.push((1,4)); if !omit_peer {sources.push((4,1));}}
            for (rx,source) in sources {
                let mut frame=template.clone(); frame.rx_id=rx;
                frame.schema_version=raw::RAW_CSI_SCHEMA_VERSION; frame.session_id=Some(id.into());
                frame.host_monotonic_ns=Some(time); frame.clock_epoch_id=Some("audit-test".into());
                frame.host_timestamp_unix_ns=1_700_000_000_000_000_000+time;
                frame.sequence=sequence[usize::from(rx)]; sequence[usize::from(rx)]+=1;
                let identity=frame.protocol_identity.as_mut().unwrap(); identity.boot_id=u64::from(rx);
                identity.callback_started_us=Some(1_000_000+time/1000);
                let u=identity.unified_rx.as_mut().unwrap();
                u.dequeued_us=1_000_001+time/1000; u.send_attempt_us=1_000_002+time/1000;
                u.receiver_mac[5]=rx; u.config_sha256=[0xa7+rx-1;32]; u.source_node_id=source;
                if source>0 {
                    u.source_mac=[2,0,0,0,3,source]; u.sig_mode=0;
                    frame.source_binding.as_mut().unwrap().tx_filter_sha256=position_artifact::sha256_bytes(&u.source_mac);
                    frame.subcarrier_count=64; frame.iq_pairs.truncate(64);
                    let e=frame.physical_layout.as_mut().unwrap(); e.iq_bytes=128;e.sig_mode=0;
                }
                if rx==2 || rx==3 {u.mode=0;u.peer_count=0;}
                file.write_all(raw::encode_json_line(&frame).unwrap().as_bytes()).unwrap(); count+=1;
            }
        }
        drop(file);
        let metadata=json!({"recording_id":id,"setup_id":setup.setup_id(),"setup_sha256":setup.setup_sha256(),
            "sealed_setup":setup,"status":"completed","incomplete":false,"dropped_frames":0,"writer_error":null,
            "frames_written":count,"raw_sha256":position_artifact::sha256_file(&path).unwrap(),
            "clock_epoch_id":"audit-test","started_at_monotonic_ns":1_000_000,
            "ended_at_monotonic_ns":1_000_000+u64::from(seconds)*1_000_000_000+10_000_000});
        std::fs::write(path.with_file_name(format!("{id}.raw-csi.v1.meta.json")),serde_json::to_vec(&metadata).unwrap()).unwrap();
        (directory,path)
    }

    #[test]
    #[ignore = "exports synthetic full-rate recordings only to an explicitly supplied new external directory"]
    fn export_capacity_benchmark_recordings() {
        let output=std::path::PathBuf::from(std::env::var_os("RX_AUDIT_BENCHMARK_OUTPUT").expect("explicit external output required"));
        assert!(output.is_absolute() && !output.starts_with(env!("CARGO_MANIFEST_DIR")));
        std::fs::create_dir(&output).unwrap();
        for seconds in [180,300] {
            let (_directory,path)=capture_at_rate(seconds,false,true);
            let target=output.join(seconds.to_string()); std::fs::create_dir(&target).unwrap();
            std::fs::copy(&path,target.join(path.file_name().unwrap())).unwrap();
            let meta=path.with_file_name("synthetic-tag14.raw-csi.v1.meta.json");
            std::fs::copy(&meta,target.join(meta.file_name().unwrap())).unwrap();
        }
    }

    #[test]
    fn functional_pair_60s_uses_real_masks_and_requires_both_directions() {
        let (_directory,path)=capture(60,false);
        let report=run(&path,60).unwrap();
        assert_eq!(report["raw_window_gates_pass"],true);
        assert_eq!(report["acceptance_contract"],"functional-pair-60s-not-Tag14");
        assert_eq!(report["full_tag14_acceptance"],false);
        assert_eq!(report["links"].as_array().unwrap().len(),6);
        let (_directory,missing)=capture(60,true);
        assert_eq!(run(&missing,60).unwrap()["raw_window_gates_pass"],false);
    }

    #[test]
    fn complete_native_audit_uses_real_decoder_and_deterministic_negative_gates() {
        for seconds in [180,300] {
            let (_directory,path)=capture(seconds,false);
            let first=run(&path,seconds).unwrap(); assert_eq!(first["raw_window_gates_pass"],true);
            assert_eq!(first,run(&path,seconds).unwrap());
            assert_eq!(first["full_tag14_acceptance"],false,"raw evidence alone cannot certify complete live acceptance");
            let meta_path=path.with_file_name("synthetic-tag14.raw-csi.v1.meta.json");
            let mut meta:serde_json::Value=serde_json::from_slice(&std::fs::read(&meta_path).unwrap()).unwrap();
            meta["ended_at_monotonic_ns"]=json!(1_000_000+u64::from(seconds)*1_000_000_000+50_000_001);
            std::fs::write(&meta_path,serde_json::to_vec(&meta).unwrap()).unwrap();
            assert_eq!(run(&path,seconds).unwrap()["raw_window_gates_pass"],false);
            meta["raw_sha256"]=json!("0".repeat(64)); std::fs::write(&meta_path,serde_json::to_vec(&meta).unwrap()).unwrap();
            assert!(run(&path,seconds).is_err());
        }
        let (_directory,path)=capture(300,true);
        assert_eq!(run(&path,300).unwrap()["raw_window_gates_pass"],false,"missing direction cannot be hidden by another link");
    }

}

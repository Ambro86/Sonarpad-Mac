#!/usr/bin/env python3
"""Local tests of the production recorder transport, timing and FFmpeg export.
The capture process is simulated: this does NOT validate Apple permissions/hardware.
"""
import json
import math
import os
import re
from pathlib import Path
import shutil
import struct
import subprocess
import sys
import wave

ROOT = Path(__file__).resolve().parents[1]
WORK = ROOT / 'target' / 'podcast-recording-tests'
WORK.mkdir(parents=True, exist_ok=True)
ids = re.findall(r'const (ID_[A-Z_]+): i32 = (\d+);', (ROOT/'src/main.rs').read_text(encoding='utf-8'))
for name in ['ID_TOOLS_MEDIA_JOIN', 'ID_TOOLS_BATCH_AUDIOBOOKS', 'ID_TOOLS_RECORD_PODCAST']:
    value = dict(ids)[name]
    assert [key for key, number in ids if number == value] == [name], (name, value)

FFMPEG = shutil.which(sys.argv[1] if len(sys.argv) > 1 else 'ffmpeg')
if not FFMPEG:
    raise SystemExit('FFmpeg is required for the local export tests.')

def run(args, **kwargs):
    return subprocess.run([str(x) for x in args], check=True, **kwargs)

manifest = '''[package]
name="podcast-recording-tests"
version="0.0.0"
edition="2024"
[dependencies]
uuid={version="1.19",features=["v4"]}
serde_json="1.0"
hound="3.5"
base64="0.22"
'''
(WORK / 'Cargo.toml').write_text(manifest, encoding='utf-8')
shutil.copyfile(ROOT / 'Cargo.lock', WORK / 'Cargo.lock')
(WORK / 'src').mkdir(exist_ok=True)
source = r'''
#![allow(dead_code)]
#[path="ENGINE"] mod podcast_recording_engine;
#[path="JOIN"] mod media_join_engine;
use podcast_recording_engine::*;
use std::path::PathBuf;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}, mpsc};
use base64::Engine;
use std::io::Write;
fn emit(data: serde_json::Value) { println!("{data}"); std::io::stdout().flush().unwrap(); }
fn pcm(time: f64) {
    let bytes: Vec<_> = (0..48000).flat_map(|i| {
        let value = (i as f32 * 440.0 * std::f32::consts::TAU / 48000.0).sin() * 0.2;
        value.to_le_bytes().into_iter().chain(value.to_le_bytes())
    }).collect();
    emit(serde_json::json!({"event":"audio","source":"microphone","time":time,"data":base64::engine::general_purpose::STANDARD.encode(bytes)}));
}
fn mock(args: &[String]) {
    emit(serde_json::json!({"event":"ready"})); pcm(0.0);
    if args.iter().any(|s| s=="fail") {
        emit(serde_json::json!({"event":"error","code":"capture","detail":"Simulated device loss"})); return;
    }
    let mut line = String::new();
    while std::io::stdin().read_line(&mut line).unwrap()>0 {
        match line.trim() {
            "pause" => emit(serde_json::json!({"event":"paused","seconds":1.0})),
            "resume" => { emit(serde_json::json!({"event":"resumed","seconds":1.0})); pcm(1.0); }
            "stop" => { emit(serde_json::json!({"event":"stopped","seconds":2.0})); return; }
            _ => ()
        }
        line.clear();
    }
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    if args.iter().any(|s|s=="--microphone") { mock(&args); return; }
    let ffmpeg=PathBuf::from(&args[2]); let root=PathBuf::from(&args[3]);
    if args[1]=="export" {
        let output=root.join(&args[4]); let mode=&args[5];
        let mut tracks=Vec::new();
        if mode!="system" { tracks.push((root.join("mic.wav"), if mode=="quiet" {0.25} else {1.0})); }
        if mode!="mic" { tracks.push((root.join("system.wav"), if mode=="quiet" {0.5} else {1.0})); }
        let mut values=Vec::new();
        export(&ffmpeg,&tracks,&output,2.0,&AtomicBool::new(false),&mut |p|values.push(p)).unwrap();
        assert_eq!(values.last(),Some(&100)); assert!(values.windows(2).all(|v|v[1]>=v[0]));
        let original=std::fs::read(&output).unwrap();
        assert!(export(&ffmpeg,&tracks,&output,2.0,&AtomicBool::new(false),&mut |_|{}).is_err());
        assert_eq!(std::fs::read(&output).unwrap(),original);
        let cancelled=root.join("cancelled.wav");
        assert!(export(&ffmpeg,&tracks,&cancelled,2.0,&AtomicBool::new(true),&mut |_|{}).is_err());
        assert!(!cancelled.exists());
    } else {
        let mode=args[4].clone(); let output=root.join(format!("transport-{mode}.wav"));
        let _=std::fs::remove_file(&output);
        let (tx,rx)=mpsc::channel(); let (events,receive)=mpsc::channel();
        let cancel=Arc::new(AtomicBool::new(false)); let c=cancel.clone(); let out=output.clone(); let helper=std::env::current_exe().unwrap();
        let config=Config{microphone:Some(mode.clone()),system:false,mic_gain:1.0,system_gain:1.0,output:out};
        let worker=std::thread::spawn(move || record(&helper,&ffmpeg,config,rx,&c,&events));
        let mut starts=0; let mut paused=false; let mut percents=Vec::new();
        loop {
            match receive.recv_timeout(std::time::Duration::from_secs(20)).expect("recording timed out") {
                Event::Recording => {
                    starts+=1;
                    if mode=="cancel" { cancel.store(true,Ordering::Relaxed); }
                    else if starts==1 { tx.send(Control::Pause).unwrap(); }
                    else { tx.send(Control::Stop).unwrap(); }
                }
                Event::Paused => { paused=true; tx.send(Control::Resume).unwrap(); }
                Event::Saving(p) => percents.push(p),
                Event::Done(path) => {
                    assert_eq!(mode,"normal"); assert!(paused); assert_eq!(starts,2);
                    assert_eq!(percents.last(),Some(&100));
                    let wav=hound::WavReader::open(path).unwrap(); assert_eq!(wav.duration(),96000); break;
                }
                Event::Failed(error,recovery) => {
                    assert_ne!(mode,"normal","{error}"); assert!(!output.exists());
                    let dir=recovery.unwrap(); assert!(dir.is_dir());
                    let wav=hound::WavReader::open(dir.join("microphone.wav")).unwrap();
                    if mode=="fail" { assert_eq!(wav.duration(),48000); }
                    std::fs::remove_dir_all(dir).unwrap(); break;
                }
                _ => ()
            }
        }
        worker.join().unwrap();
    }
}
'''.replace('ENGINE', (ROOT/'src/podcast_recording_engine.rs').as_posix()).replace('JOIN', (ROOT/'src/media_join_engine.rs').as_posix())
(WORK / 'src/main.rs').write_text(source, encoding='utf-8')
run(['cargo','test','--manifest-path',WORK/'Cargo.toml','--target-dir',ROOT/'target','--','--nocapture'])
run(['cargo','build','--manifest-path',WORK/'Cargo.toml','--target-dir',ROOT/'target'])
exe=ROOT/'target/debug'/('podcast-recording-tests.exe' if os.name=='nt' else 'podcast-recording-tests')
for name,freq in [('mic',440),('system',880)]:
    with wave.open(str(WORK/f'{name}.wav'),'wb') as wav:
        wav.setnchannels(2); wav.setsampwidth(2); wav.setframerate(48000)
        wav.writeframes(b''.join(struct.pack('<hh',*[int(0.2*32767*math.sin(i*freq*math.tau/48000))]*2) for i in range(96000)))
for ext in ['mp3','m4a','wav']:
    for mode in ['mic','system','both','quiet']:
        output=WORK/f'export-{mode}.{ext}'; output.unlink(missing_ok=True)
        run([exe,'export',FFMPEG,WORK,output.name,mode])
        decoded=run([FFMPEG,'-v','error','-i',output,'-f','f32le','-af','pan=mono|c0=c0','-ac','1','-ar','48000','pipe:1'],capture_output=True).stdout
        data=struct.unpack('<'+'f'*(len(decoded)//4),decoded)
        assert abs(len(data)/48000-2.0)<0.1,(output,len(data))
        middle=data[12000:84000]
        for freq in [440,880]:
            real=sum(v*math.cos(i*freq*math.tau/48000) for i,v in enumerate(middle))
            imag=sum(v*math.sin(i*freq*math.tau/48000) for i,v in enumerate(middle))
            amplitude=2*math.hypot(real,imag)/len(middle)
            expected=0 if (mode=='mic' and freq==880 or mode=='system' and freq==440) else 0.2
            if mode=='quiet': expected*=0.25 if freq==440 else 0.5
            assert abs(amplitude-expected)<0.025,(output,freq,amplitude,expected)
        assert max(abs(x) for x in data)<0.98
        print('PASS export, duration, source levels, protection, cancellation:',output.name,flush=True)
for mode in ['normal','fail','cancel']:
    run([exe,'transport',FFMPEG,WORK,mode])
    print('PASS simulated capture transport:',mode,flush=True)
print('PASS: 12 real exports; simulated capture/pause/resume/device loss/cancel. Apple capture is not exercised.',flush=True)

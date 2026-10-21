"""Test the real catalog module and compile the wxDragon window on any host.

No Mac audio hardware/UI interaction is simulated. --live tests the service
using SONARMUSIC_LIVE_TOKEN from the environment, never written to disk.
"""
from pathlib import Path
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
WORK = ROOT / 'target/sonarmusic-tests'
WORK.mkdir(parents=True, exist_ok=True)
(WORK / 'src').mkdir(exist_ok=True)
(WORK / 'Cargo.toml').write_text('''[package]
name="sonarmusic-tests"
version="0.0.0"
edition="2024"
[workspace]
[dependencies]
serde_json="1.0"
reqwest={version="0.12",features=["blocking","json","rustls-tls"]}
wxdragon="0.9.18"
''', encoding='utf8')
shutil.copyfile(ROOT / 'Cargo.lock', WORK / 'Cargo.lock')
harness = r'''
#![allow(dead_code)]
use std::sync::{Arc,Mutex};
use wxdragon::prelude::*;
#[path="CORE"] mod sonarmusic;
#[path="UI"] mod sonarmusic_window;
const SONARPAD_ROUTE_CLIENT_TOKEN:&str="";
struct Settings {ui_language:String}
#[derive(Clone)] struct AudioDescriptionLaunchContext;
struct YoutubeSearchResult {title:String,url:String,is_collection:bool,duration:Option<String>}
fn normalize_ui_language(s:&str)->String{s.into()}
fn show_message_dialog(_: &Frame,_:&str,_:&str){}
fn append_podcast_log(_:&str){}
fn open_youtube_results_dialog_with_title(_: &Dialog,_:&Arc<Mutex<Settings>>,_:Vec<YoutubeSearchResult>,_:Option<()>,_:&AudioDescriptionLaunchContext,_:bool,_:&str){}
'''
harness = harness.replace('CORE', (ROOT/'src/sonarmusic.rs').as_posix())
harness = harness.replace('UI', (ROOT/'src/sonarmusic_window.rs').as_posix())
(WORK/'src/lib.rs').write_bytes(harness.encode('utf8'))
command = ['cargo', 'test', '--offline', '--manifest-path', str(WORK/'Cargo.toml'),
           '--target-dir', str(ROOT/'target'), '--', '--nocapture']
if '--live' in sys.argv:
    command.append('--include-ignored')
subprocess.run(command, check=True)

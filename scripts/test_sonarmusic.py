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
#[path="PLAYBACK"] mod youtube_playback;
#[path="UI"] mod sonarmusic_window;
#[path="LABELS"] mod result_labels;
const SONARPAD_ROUTE_CLIENT_TOKEN:&str="";
struct Settings {ui_language:String, sonarmusic_search_type:String, sonarmusic_favorites:Vec<serde_json::Value>}
impl Settings { fn save(&self) {} }
#[derive(Clone)] struct AudioDescriptionLaunchContext;
struct YoutubeSearchResult {title:String,url:String,is_collection:bool,duration:Option<String>}
fn normalize_ui_language(s:&str)->String{s.into()}
fn show_message_dialog(_: &Frame,_:&str,_:&str){}
fn append_podcast_log(_:&str){}
fn open_youtube_results_dialog_with_title(_: &Dialog,_:&Arc<Mutex<Settings>>,_:Vec<YoutubeSearchResult>,_:Option<()>,_:&AudioDescriptionLaunchContext,_:bool,_:&str){}
'''
harness = harness.replace('PLAYBACK', (ROOT/'src/youtube_playback.rs').as_posix())
harness = harness.replace('CORE', (ROOT/'src/sonarmusic.rs').as_posix())
harness = harness.replace('LABELS', (ROOT/'src/result_labels.rs').as_posix())
harness = harness.replace('UI', (ROOT/'src/sonarmusic_window.rs').as_posix())

# Compile the actual MPV command builder too, with filesystem/UI helpers stubbed.
main_source = (ROOT / 'src/main.rs').read_text(encoding='utf8')
command_source = main_source[main_source.index('fn configure_youtube_mpv_command('):main_source.index('fn spawn_youtube_mpv(')]
harness += r"""
mod playback_command_tests {
use std::path::{Path,PathBuf};
use std::process::Command;
const YOUTUBE_MPV_STREAM_FORMAT:&str="18/best[height<=360][ext=mp4]/best[height<=480]/best";
const YOUTUBE_PLAYER_EXTRACTOR_ARGS:&str="youtube:player_client=android";
mod podcast_player {pub fn bundled_mpv_executable_path()->Option<std::path::PathBuf>{None}}
fn ytdlp_executable_path()->PathBuf {std::env::current_exe().unwrap()}
fn media_bookmarks_enabled()->bool{false}
fn prepare_mpv_runtime_dirs(_:bool)->Result<PathBuf,String>{Ok(PathBuf::from("config"))}
fn bundled_mpv_input_conf_path()->PathBuf{PathBuf::from("input.conf")}
fn write_mpv_accessibility_script(_:&Path,_:&str,_:Option<&str>)->Result<PathBuf,String>{Ok(PathBuf::from("access.lua"))}
fn append_podcast_log(_:&str){}
fn attach_mpv_accessibility_script(c:&mut Command,p:&Path){c.arg(format!("--script={}",p.display()));}
fn is_youtube_url(s:&str)->bool{s.starts_with("https://www.youtube.com/")}
fn mpv_watch_later_dir()->PathBuf{PathBuf::from("bookmarks")}
fn bundled_deno_candidate(_:&Path)->Option<PathBuf>{None}
""" + command_source + r"""
#[test] fn ordinary_youtube_keeps_watch_url() {
 let mut c=Command::new("mpv");configure_youtube_mpv_command(&mut c,"https://www.youtube.com/watch?v=4K6cvc27_CE","Title",None).unwrap();
 let args:Vec<_>=c.get_args().map(|a|a.to_string_lossy().into_owned()).collect();
 assert_eq!(args[0],"https://www.youtube.com/watch?v=4K6cvc27_CE");assert!(!args.iter().any(|a|a=="--ytdl=no"));
}
#[test] fn music_direct_keeps_accessibility_and_disables_extractor() {
 let mut c=Command::new("mpv");configure_youtube_mpv_command_with_input(&mut c,"https://www.youtube.com/watch?v=4K6cvc27_CE","Title",Some(Path::new("test.sock")),Some("https://rr1.googlevideo.com/videoplayback")).unwrap();
 let args:Vec<_>=c.get_args().map(|a|a.to_string_lossy().into_owned()).collect();
 assert_eq!(args[0],"https://rr1.googlevideo.com/videoplayback");
 for expected in ["--ytdl=no","--script=access.lua","--input-ipc-server=test.sock","--title=Sonarpad - Title","--aid=auto"] {assert!(args.iter().any(|a|a==expected),"missing {expected}");}
}
}
"""

# Type-check the real GUI without linking wxWidgets into a headless test binary.
# This also works on Windows hosts whose cached wxWidgets uses a different CRT.
(WORK/'src/lib.rs').write_bytes(harness.encode('utf8'))
subprocess.run(['cargo', 'check', '--offline', '--tests', '--manifest-path', str(WORK/'Cargo.toml'),
                '--target-dir', str(ROOT/'target')], check=True)
core_header = '#![allow(dead_code)]\nconst SONARPAD_ROUTE_CLIENT_TOKEN:&str="";\n'
core_header += '#[path="' + (ROOT/'src/sonarmusic.rs').as_posix() + '"] mod sonarmusic;\n'
core_header += '#[path="' + (ROOT/'src/youtube_playback.rs').as_posix() + '"] mod youtube_playback;\n'
harness = core_header + harness[harness.index('mod playback_command_tests {'):]
manifest = (WORK/'Cargo.toml').read_text(encoding='utf8').replace('wxdragon="0.9.18"\n','')
(WORK/'Cargo.toml').write_text(manifest,encoding='utf8')
(WORK/'src/lib.rs').write_bytes(harness.encode('utf8'))
command = ['cargo', 'test', '--offline', '--manifest-path', str(WORK/'Cargo.toml'),
           '--target-dir', str(ROOT/'target'), '--', '--nocapture']
if '--live' in sys.argv:
    command.append('--include-ignored')
subprocess.run(command, check=True)

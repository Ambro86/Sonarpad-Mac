"""Exercise the actual Rust cutter engine with the release-configured FFmpeg."""
import json
import math
import os
from pathlib import Path
import struct
import subprocess
import sys
import urllib.request
import wave

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "media-cutter-test-results"


def production_build_script():
    workflow = (ROOT / ".github/workflows/macos-app-dmg.yml").read_text()
    start = workflow.index("      - name: Build bundled ffmpeg from source")
    end = workflow.index("\n      - name:", start + 10)
    block = workflow[start:end].split("        run: |\n", 1)[1]
    script = "\n".join(line[10:] if line.startswith("          ") else line for line in block.splitlines())
    script = script.replace("${{ matrix.ffmpeg_macos_min }}", os.environ["MACOSX_DEPLOYMENT_TARGET"])
    script = script.replace("${{ matrix.rust_target }}", os.environ["TEST_RUST_TARGET"])
    assert "${{" not in script, "unresolved workflow expression"
    Path("build-test-ffmpeg.sh").write_text(script + "\n")


def extract(source, prefix):
    start = source.index(prefix)
    return source[start:source.index("\n}", start) + 2]


def build_harness():
    source = (ROOT / "src/media_cutter.rs").read_text(encoding="utf-8")
    work = ROOT / "target/media-cutter-harness"
    work.mkdir(parents=True, exist_ok=True)
    code = r"""
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use uuid::Uuid;
const MIN_PART_SECONDS: f64 = 0.10;
fn ffmpeg_executable_path() -> Option<PathBuf> { std::env::var_os("TEST_FFMPEG").map(PathBuf::from) }
fn append_podcast_log(message: &str) { eprintln!("{message}"); }
struct Labels { processing: &'static str }
fn labels() -> Labels { Labels { processing: "Processing" } }
"""
    code += "#[derive(Clone, Copy)]\n" + extract(source, "enum VideoRotation {") + "\n"
    for name in ["MediaPart", "ProbeInfo", "AddedTrackSettings", "ExportProgress", "ExportSnapshot"]:
        code += "#[derive(Clone)]\n" + extract(source, f"struct {name} {{") + "\n"
    code += extract(source, "impl MediaPart {") + "\n"
    for name in ["ffprobe_path", "parse_ffmpeg_clock", "probe_media_with_ffmpeg", "probe_media", "rotation_filter", "ffmpeg_time", "run_ffmpeg_step", "write_concat_file", "copy_file", "export_media"]:
        code += extract(source, f"fn {name}(") + "\n"
    code += r"""
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let case = &args[1];
    let dir = PathBuf::from(&args[2]);
    let video = case.starts_with("video") || case.starts_with("silent");
    let input = dir.join(if case.starts_with("silent") { "silent.mp4" } else if video { "source.mp4" } else { "source.wav" });
    let probe = probe_media(&input).unwrap();
    assert_eq!(probe.has_video, video);
    let parts = if case.contains("multi") {
        vec![MediaPart { start: 1.0, end: 2.0, keep: true }, MediaPart { start: 2.0, end: 3.0, keep: false }, MediaPart { start: 3.0, end: 4.0, keep: true }]
    } else { vec![MediaPart { start: 1.0, end: 3.0, keep: true }] };
    let rotation = if case.contains("right") { VideoRotation::Right } else if case.contains("left") { VideoRotation::Left } else if case.contains("half") { VideoRotation::Half } else { VideoRotation::None };
    let added_track = case.contains("mix").then(|| AddedTrackSettings { path: dir.join("added.wav"), original_volume: 60, new_volume: 40, loop_track: true });
    let progress = Arc::new(Mutex::new(ExportProgress { percent: 0, message: String::new(), finished: false, result: None }));
    let cancel = Arc::new(AtomicBool::new(case == "cancel"));
    let output = dir.join(format!("{case}.{}", if video { "mp4" } else { "m4a" }));
    export_media(ExportSnapshot { input, output: output.clone(), parts, probe, rotation, added_track }, progress.clone(), cancel);
    let progress = progress.lock().unwrap();
    let result = progress.result.as_ref().unwrap();
    if case == "cancel" {
        assert_eq!(result.as_ref().unwrap_err(), "__CANCELLED__");
        assert!(!output.exists());
        return;
    }
    match result {
        Ok(path) => {
            let probe = probe_media(path).unwrap();
            assert!((probe.duration - 2.0).abs() < 0.6, "duration {}", probe.duration);
            assert_eq!(progress.percent, 100);
            assert!(progress.finished);
            assert_eq!(probe.has_video, video);
            assert!(probe.has_audio || case == "silent-single");
            println!("PASS {case}: duration={:.3}", probe.duration);
        }
        Err(error) => { eprintln!("FAIL {case}: {error}"); std::process::exit(1); }
    }
}
"""
    (work / "main.rs").write_text(code, encoding="utf-8")
    (work / "Cargo.toml").write_text('''[package]
name = "sonarpad-media-cutter-test"
version = "0.0.0"
edition = "2024"
[[bin]]
name = "media-cutter-test"
path = "main.rs"
[dependencies]
uuid = { version = "1.19", features = ["v4"] }
serde_json = "1.0"
''')
    subprocess.run(["cargo", "build", "--manifest-path", str(work / "Cargo.toml")], check=True)
    return work / "target/debug/media-cutter-test"


def run():
    OUT.mkdir(exist_ok=True)
    fixtures = OUT / "media files with spaces"
    fixtures.mkdir(exist_ok=True)
    for name, duration, frequency in [("source.wav", 6, 440), ("added.wav", 0.5, 220)]:
        with wave.open(str(fixtures / name), "wb") as audio:
            audio.setnchannels(1)
            audio.setsampwidth(2)
            audio.setframerate(48000)
            audio.writeframes(b"".join(struct.pack("<h", int(2000 * math.sin(2 * math.pi * frequency * i / 48000))) for i in range(int(48000 * duration))))
    video = fixtures / "source.mp4"
    if not video.exists():
        urllib.request.urlretrieve("https://media.w3.org/2010/05/sintel/trailer.mp4", video)
    ffmpeg = Path(sys.argv[1]).resolve()
    subprocess.run([str(ffmpeg), "-y", "-i", str(video), "-t", "6", "-an", "-c:v", "copy", str(fixtures / "silent.mp4")], check=True, capture_output=True)
    executable = build_harness()
    env = os.environ.copy()
    env["TEST_FFMPEG"] = str(ffmpeg)
    env["PATH"] = "/usr/bin:/bin"  # No Homebrew ffprobe: test the actual bundle fallback.
    results = []
    for case in ["audio-single", "audio-multi", "audio-mix", "video-single", "video-multi", "video-right", "video-left", "video-half", "video-mix", "silent-single", "silent-mix", "cancel"]:
        result = subprocess.run([str(executable), case, str(fixtures)], env=env, text=True, capture_output=True, timeout=90)
        (OUT / f"{case}.log").write_text(result.stdout + result.stderr)
        print(case, "PASS" if result.returncode == 0 else "FAIL", flush=True)
        if result.returncode:
            print(result.stderr[-2000:], flush=True)
        results.append({"case": case, "passed": result.returncode == 0})
    (OUT / "summary.json").write_text(json.dumps(results, indent=2))
    assert all(r["passed"] for r in results), "See media-cutter-test-results for the actual FFmpeg errors"


if __name__ == "__main__":
    if sys.argv[1] == "--prepare-build":
        production_build_script()
    else:
        run()

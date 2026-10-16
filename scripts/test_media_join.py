"""Run the production joining engine against real short media, without the GUI.

Usage: python scripts/test_media_join.py /path/to/bundled/ffmpeg
The runner can execute on macOS or Windows; a Windows run checks the engine,
not native macOS accessibility or the shipped macOS binary.
"""
import json
import math
from pathlib import Path
import shutil
import struct
import subprocess
import sys
import wave

ROOT = Path(__file__).resolve().parents[1]
WORK = ROOT / "target/media-join-tests"


def run(args, **kwargs):
    return subprocess.run([str(s) for s in args], check=True, **kwargs)


def main():
    ffmpeg = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else Path(shutil.which("ffmpeg")).resolve()
    ffprobe = ffmpeg.with_name("ffprobe.exe" if sys.platform == "win32" else "ffprobe")
    if not ffprobe.exists():
        ffprobe = Path(shutil.which("ffprobe"))
    WORK.mkdir(parents=True, exist_ok=True)
    (WORK / "Cargo.toml").write_text('''[package]
name="media-join-tests"
version="0.0.0"
edition="2024"
[workspace]
[[bin]]
name="media-join-tests"
path="main.rs"
[dependencies]
uuid={version="1.19",features=["v4"]}
''', encoding="utf-8")
    engine = (ROOT / "src/media_join_engine.rs").as_posix()
    (WORK / "main.rs").write_text(f'#[path={json.dumps(engine)}] mod engine;\n' + r'''
use std::{path::PathBuf, sync::{Arc, atomic::{AtomicBool, Ordering}}};
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let ffmpeg = PathBuf::from(&args[1]);
    let output = PathBuf::from(&args[2]);
    let inputs: Vec<_> = args[4..].iter().map(PathBuf::from).collect();
    let flag = Arc::new(AtomicBool::new(false));
    let mut progress = Vec::new();
    let result = engine::join(&ffmpeg, &inputs, &output, flag.clone(), &mut |p| {
        progress.push(p);
        if args[3] == "cancel" && p > 0 { flag.store(true, Ordering::Relaxed); }
    });
    assert!(progress.windows(2).all(|v| v[0] < v[1]));
    if args[3] == "cancel" {
        assert_eq!(result.unwrap_err(), "__CANCELLED__");
        assert!(!progress.contains(&100));
    } else if args[3] == "error" {
        assert!(result.is_err()); assert!(!progress.contains(&100));
    } else {
        result.unwrap(); assert_eq!(progress.first(), Some(&0));
        assert_eq!(progress.last(), Some(&100));
        assert!(progress.iter().any(|p| *p > 0 && *p < 100));
    }
}
''', encoding="utf-8")
    run(["cargo", "test", "--manifest-path", WORK / "Cargo.toml", "--offline"])
    run(["cargo", "build", "--manifest-path", WORK / "Cargo.toml", "--offline"])
    exe = WORK / "target/debug" / ("media-join-tests.exe" if sys.platform == "win32" else "media-join-tests")
    for index, (rate, channels, freq) in enumerate([(44100, 1, 440), (48000, 2, 880)]):
        with wave.open(str(WORK / f"audio{index}.wav"), "wb") as writer:
            writer.setparams((channels, 2, rate, 0, "NONE", ""))
            values = [int(8000 * math.sin(2 * math.pi * freq * frame / rate)) for frame in range(rate)]
            writer.writeframes(b"".join(struct.pack("<h", value) * channels for value in values))
        run([ffmpeg, "-v", "error", "-y", "-f", "lavfi", "-i", f"color=c={'red' if index == 0 else 'green'}:s={'64x48' if index == 0 else '96x64'}:r={25 if index == 0 else 30}",
             "-i", WORK / f"audio{index}.wav", "-t", "1", "-c:v", "mpeg4", "-c:a", "aac", WORK / f"video{index}.mp4"])
    inputs = [WORK / "audio0.wav", WORK / "audio1.wav"]
    videos = [WORK / "video0.mp4", WORK / "video1.mp4"]

    def probe(path):
        data = run([ffprobe, "-v", "error", "-show_streams", "-show_format", "-of", "json", path], capture_output=True)
        return json.loads(data.stdout)

    cases = 0
    for ext in ["mp3", "m4a", "wav", "flac", "ogg", "opus", "aiff", "mp4", "mkv", "avi", "mov"]:
        video = ext in ["mp4", "mkv", "avi", "mov"]
        output = WORK / f"joined.{ext}"
        run([exe, ffmpeg, output, "ok", *(videos if video else inputs)])
        metadata = probe(output)
        assert abs(float(metadata["format"]["duration"]) - 2) < .25, metadata
        assert any(s["codec_type"] == "video" for s in metadata["streams"]) == video
        assert any(s["codec_type"] == "audio" for s in metadata["streams"])
        if video:
            # Decode every frame, including the join boundary; inspect both scenes.
            decoded = run([ffmpeg, "-v", "error", "-i", output, "-an", "-pix_fmt", "rgb24", "-f", "rawvideo", "-"], capture_output=True).stdout
            size = 64 * 48 * 3
            frames = len(decoded) // size
            assert 57 <= frames <= 64, frames
            first = decoded[5 * size:6 * size]
            last = decoded[-6 * size:-5 * size]
            assert sum(first[0::3]) > sum(first[1::3]) * 2
            assert sum(last[1::3]) > sum(last[0::3]) * 2
        cases += 1
    # Sample-level ordering and resampling: 440 Hz first, then 880 Hz.
    def frequencies(path):
        with wave.open(str(path), "rb") as reader:
            assert reader.getnchannels() == 2 and reader.getframerate() == 48000
            raw = reader.readframes(reader.getnframes())
        samples = struct.unpack("<" + "h" * (len(raw) // 2), raw)[::2]
        def count(start):
            section = samples[start:start + 24000]
            return sum(a <= 0 < b for a, b in zip(section, section[1:])) * 2
        return count(12000), count(60000)
    assert all(abs(a-b) < 5 for a, b in zip(frequencies(WORK / "joined.wav"), (440, 880)))
    run([exe, ffmpeg, WORK / "reverse.wav", "ok", *reversed(inputs)])
    assert all(abs(a-b) < 5 for a, b in zip(frequencies(WORK / "reverse.wav"), (880, 440)))
    run([exe, ffmpeg, WORK / "extracted.wav", "ok", *videos])
    assert not any(s["codec_type"] == "video" for s in probe(WORK / "extracted.wav")["streams"])
    # Silent video still gets the correct duration and an aligned silent track.
    silent = WORK / "silent.mp4"
    run([ffmpeg, "-v", "error", "-y", "-i", videos[0], "-an", "-c:v", "copy", silent])
    run([exe, ffmpeg, WORK / "silent-joined.mp4", "ok", silent, videos[1]])
    assert abs(float(probe(WORK / "silent-joined.mp4")["format"]["duration"]) - 2) < .25
    output = WORK / "protected.wav"
    output.write_bytes(b"keep original")
    run([exe, ffmpeg, output, "cancel", *inputs])
    assert output.read_bytes() == b"keep original"
    bad = WORK / "corrupt.mp4"
    bad.write_bytes(b"invalid")
    run([exe, ffmpeg, output, "error", inputs[0], bad])
    assert output.read_bytes() == b"keep original"
    original = inputs[0].read_bytes()
    run([exe, ffmpeg, inputs[0], "error", *inputs])
    assert inputs[0].read_bytes() == original
    assert not list(WORK.glob(".sonarpad-join-*"))
    print(f"PASS: {cases} output formats; decoded video scenes; audio order and resampling; extraction; silent video; cancellation; invalid inputs; source protection.")


if __name__ == "__main__":
    main()

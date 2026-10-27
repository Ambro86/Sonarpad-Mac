"""Exercise the actual Mac encode_mp3 arguments with real FFmpeg.
Runs locally on Windows too; does not simulate or validate the Mac GUI.
"""
import array
import json
import math
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import wave

ROOT = Path(__file__).resolve().parents[1]
source = (ROOT / "src/audio_description.rs").read_text(encoding="utf-8")
encode = source[source.index("fn encode_mp3("):source.index("fn audio_description_mkv_fallback_path(")]
bitrate = re.search(r"const BITRATE_KBPS: u32 = \d+;", source).group()
ffmpeg = shutil.which("ffmpeg")
ffprobe = shutil.which("ffprobe")
assert ffmpeg and ffprobe, "FFmpeg and ffprobe must be installed"
with tempfile.TemporaryDirectory(prefix="sonarpad-mac-mp3-") as temporary:
    work = Path(temporary)
    harness = work / "encode.rs"
    harness.write_text(r'''use std::path::Path;
use std::sync::{Arc, atomic::AtomicBool};
fn run_ffmpeg(args: &[String], _: &Arc<AtomicBool>) -> Result<(), String> {
    let result = std::process::Command::new(std::env::var("TEST_FFMPEG").unwrap())
        .args(args).output().map_err(|e| e.to_string())?;
    if result.status.success() { Ok(()) } else {
        Err(String::from_utf8_lossy(&result.stderr).to_string())
    }
}
''' + bitrate + "\n" + encode + r'''
fn main() {
    let args: Vec<_> = std::env::args().collect();
    encode_mp3(Path::new(&args[1]), Path::new(&args[2]), &Arc::new(AtomicBool::new(false))).unwrap();
}
''', encoding="utf-8")
    binary = work / ("encode.exe" if __import__("os").name == "nt" else "encode")
    subprocess.run(["rustc", str(harness), "-o", str(binary)], check=True)
    for rate, channels, seconds in [(8000, 1, 3.137), (16000, 2, 3.137),
            (22050, 2, 120.137), (24000, 2, 3.137), (44100, 2, 3.137),
            (48000, 2, 3.137), (96000, 2, 3.137), (22050, 2, 0.017)]:
        wav = work / "input.wav"
        output = work / "output.mp3"
        samples = array.array("h", (int(12000 * math.sin(i * 440 * math.tau / rate))
            for i in range(int(rate * seconds)) for _ in range(channels)))
        if __import__("sys").byteorder != "little": samples.byteswap()
        with wave.open(str(wav), "wb") as writer:
            writer.setparams((channels, 2, rate, 0, "NONE", "not compressed"))
            writer.writeframes(samples.tobytes())
        env = dict(__import__("os").environ, TEST_FFMPEG=ffmpeg)
        subprocess.run([str(binary), str(wav), str(output)], env=env, check=True)
        data = json.loads(subprocess.check_output([ffprobe, "-v", "error", "-show_entries",
            "format=duration", "-of", "json", str(output)]))
        duration = float(data["format"]["duration"])
        assert abs(duration - seconds) < 0.2, (rate, duration, seconds)
        subprocess.run([ffmpeg, "-v", "error", "-xerror", "-i", str(output),
            "-f", "null", "-"], check=True, capture_output=True)
        print(f"PASS: {rate} Hz, {channels} channels, {seconds}s -> MP3 {duration:.3f}s")

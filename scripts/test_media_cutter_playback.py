"""Test production playback ranges, and optionally render them through real MPV.

python scripts/test_media_cutter_playback.py --mpv /path/to/mpv --ffmpeg /path/to/ffmpeg
"""
import argparse
from array import array
import math
from pathlib import Path
import subprocess
import tempfile
import wave

ROOT = Path(__file__).resolve().parents[1]
source = (ROOT / "src/media_cutter.rs").read_text(encoding="utf-8")


def extract(marker):
    start = source.index(marker)
    return source[start:source.index("\n}", start) + 2]


parser = argparse.ArgumentParser()
parser.add_argument("--mpv")
parser.add_argument("--ffmpeg", default="ffmpeg")
args = parser.parse_args()
code = "use std::path::Path;\nconst MIN_PART_SECONDS: f64 = 0.1;\n"
code += "#[derive(Clone, Debug)]\n" + extract("struct MediaPart {")
code += extract("impl MediaPart {")
for name in ["remaining_kept_ranges", "kept_position", "kept_seek_position", "playback_edl", "audition_ranges", "clamp_parts", "split_part_at", "delete_range", "restore_part_index", "adjust_part_boundary", "audition_resume_position"]:
    code += extract(f"fn {name}(")
code += "use std::process::Child;\nuse std::time::Instant;\n"
code += extract("struct PreviewState {")
code += extract("impl PreviewState {")
code += r'''
fn main() {
    let mut preview = PreviewState::new();
    preview.playing = true;
    preview.base_position = 15.0;
    preview.audition_bounds = Some((10.0, 20.0));
    preview.pause(30.0);
    assert!(!preview.playing);
    assert_eq!(preview.base_position, 15.0);
    assert_eq!(preview.audition_bounds, Some((10.0, 20.0)));
    assert_eq!(audition_resume_position(preview.audition_bounds, preview.base_position, 10.0, 20.0), 15.0);
    preview.stop_child();
    assert_eq!(preview.audition_bounds, None);
    assert_eq!(audition_resume_position(None, 15.0, 10.0, 20.0), 10.0);
    assert_eq!(audition_resume_position(Some((10.0, 20.0)), 15.0, 10.0, 20.0), 15.0);
    assert_eq!(audition_resume_position(Some((10.0, 20.0)), 20.0, 10.0, 20.0), 10.0);
    assert_eq!(audition_resume_position(Some((10.0, 20.0)), 15.0, 20.0, 30.0), 20.0);
    let parts = vec![
        MediaPart { start: 0.0, end: 2.0, keep: true },
        MediaPart { start: 2.0, end: 4.0, keep: false },
        MediaPart { start: 4.0, end: 6.0, keep: true },
    ];
    let ranges = remaining_kept_ranges(&parts, 0.0);
    assert_eq!(ranges.len(), 2);
    assert_eq!(kept_position(&ranges, 2.0), 4.0);
    assert_eq!(kept_position(&ranges, 3.0), 5.0);
    assert_eq!(kept_position(&ranges, 99.0), 6.0);
    assert_eq!(kept_seek_position(&parts, 1.0, 2.0), 5.0);
    assert_eq!(kept_seek_position(&parts, 5.0, -2.0), 1.0);
    assert_eq!(kept_seek_position(&parts, 5.0, -99.0), 0.0);
    let resumed = remaining_kept_ranges(&parts, 3.0);
    assert_eq!(resumed.len(), 1);
    assert_eq!(resumed[0].start, 4.0);
    let tail = remaining_kept_ranges(&parts, 5.95);
    assert!((kept_position(&tail, 0.01) - 5.96).abs() < 0.00001);
    assert!(remaining_kept_ranges(&parts, 6.0).is_empty());
    let deleted: Vec<_> = parts.iter().map(|p| MediaPart { keep: false, ..p.clone() }).collect();
    assert!(remaining_kept_ranges(&deleted, 0.0).is_empty());
    assert!(remaining_kept_ranges(&[], 0.0).is_empty());
    let restored: Vec<_> = parts.iter().map(|p| MediaPart { keep: true, ..p.clone() }).collect();
    assert_eq!(kept_position(&restored, 3.0), 3.0);
    let multiple = vec![
        MediaPart { start: 1.0, end: 2.0, keep: true },
        MediaPart { start: 3.0, end: 4.0, keep: true },
        MediaPart { start: 7.0, end: 8.0, keep: true },
    ];
    assert_eq!(kept_position(&multiple, 2.0), 7.0);
    assert!(audition_ranges(&parts, 2.0, 4.0).is_empty());
    let audition = audition_ranges(&parts, 1.0, 5.0);
    assert_eq!(audition.len(), 2);
    assert_eq!((audition[0].start, audition[0].end), (1.0, 2.0));
    assert_eq!((audition[1].start, audition[1].end), (4.0, 5.0));
    let mut edited = vec![MediaPart { start: 0.0, end: 6.0, keep: true }];
    let selected = split_part_at(&mut edited, 2.0).unwrap();
    assert_eq!(selected, 1);
    assert_eq!(edited[selected].start, 2.0);
    delete_range(&mut edited, 2.0, 4.0, 6.0);
    assert!(audition_ranges(&edited, 2.0, 4.0).is_empty());
    assert_eq!(kept_seek_position(&edited, 3.0, -1.0), 1.0);
    assert_eq!(remaining_kept_ranges(&edited, 3.0)[0].start, 4.0);
    let history = vec![(2.0,4.0)];
    assert!(adjust_part_boundary(&mut edited, 2, true, 0.5, 6.0));
    assert_eq!(restore_part_index(&edited, 1, &history), Some(1));
    assert_eq!(restore_part_index(&edited, 2, &history), Some(1));
    edited[1].keep = true;
    assert_eq!(restore_part_index(&edited, 1, &history), None);
    let path = std::env::args().nth(1).unwrap();
    let edl = playback_edl(Path::new(&path), &ranges);
    assert!(edl.starts_with(&format!("edl://%{}%", path.len())));
    println!("{edl}");
}
'''
with tempfile.TemporaryDirectory(prefix="sonarpad-playback-") as folder:
    folder = Path(folder)
    rust = folder / "playback.rs"
    exe = folder / "playback-test.exe"
    rust.write_text(code, encoding="utf-8")
    subprocess.run(["rustc", "--edition=2024", str(rust), "-o", str(exe)], check=True)
    # Include EDL delimiters and non-ASCII characters in the actual source path.
    audio = folder / "prova, taglio; è.wav"
    edl = subprocess.check_output([str(exe), str(audio)], encoding="utf-8").strip()
    print("Playback planning: cuts, resume, forward/backward, restore, empty and Unicode paths passed.")
    if args.mpv:
        rate = 48000
        samples = array("h", (
            int(10000 * math.sin(2 * math.pi * [440, 880, 1320][i // (2 * rate)] * i / rate))
            for i in range(6 * rate)
        ))
        with wave.open(str(audio), "wb") as output:
            output.setparams((1, 2, rate, 0, "NONE", "not compressed"))
            output.writeframes(samples.tobytes())
        rendered = folder / "rendered.wav"
        result = subprocess.run([
            args.mpv, "--no-config", "--no-terminal", "--vid=no",
            "--ao=pcm", f"--ao-pcm-file={rendered}", edl,
        ], capture_output=True, timeout=30)
        assert result.returncode == 0, result.stderr.decode(errors="replace")
        pcm = subprocess.check_output([
            args.ffmpeg, "-v", "error", "-i", str(rendered),
            "-ac", "1", "-ar", str(rate), "-f", "s16le", "-",
        ])
        data = array("h", pcm)
        assert abs(len(data) / rate - 4.0) < 0.05, len(data) / rate
        # Every middle window must contain only the retained 440/1320 Hz tones.
        for second, expected in [(0.5, 440), (1.5, 440), (2.5, 1320), (3.5, 1320)]:
            chunk = data[int(second * rate):int((second + 0.25) * rate)]
            hz = sum(a <= 0 < b for a, b in zip(chunk, chunk[1:])) / 0.25
            assert abs(hz - expected) < 8, (second, hz, expected)
        print("Real MPV audio: 6 seconds become 4; removed 880 Hz segment is absent.")
        video = folder / "video, taglio; è.mp4"
        subprocess.run([
            args.ffmpeg, "-v", "error", "-f", "lavfi", "-i", "color=red:s=32x32:r=5:d=6",
            "-i", str(audio), "-vf",
            "drawbox=c=green:t=fill:enable='gte(t,2)*lt(t,4)',drawbox=c=blue:t=fill:enable='gte(t,4)'",
            "-c:v", "libx264", "-g", "5", "-c:a", "aac", "-shortest", str(video),
        ], check=True, capture_output=True)
        video_edl = subprocess.check_output([str(exe), str(video)], encoding="utf-8").strip()
        video_out = folder / "edited.mkv"
        result = subprocess.run([
            args.mpv, "--no-config", "--no-terminal", f"--o={video_out}",
            "--ovc=ffv1", "--oac=pcm_s16le", video_edl,
        ], capture_output=True, timeout=30)
        assert result.returncode == 0, result.stderr.decode(errors="replace")
        frames = subprocess.check_output([
            args.ffmpeg, "-v", "error", "-i", str(video_out), "-an",
            "-pix_fmt", "rgb24", "-f", "rawvideo", "-",
        ])
        frame_size = 32 * 32 * 3
        colors = [tuple(frames[i:i + 3]) for i in range(0, len(frames), frame_size)]
        assert 19 <= len(colors) <= 21, len(colors)
        assert all(r > g + 50 or b > g + 50 for r, g, b in colors), colors
        assert colors[0][0] > 200 and colors[-1][2] > 200, colors
        print("Real MPV video: retained red/blue frames only; deleted green section is absent.")

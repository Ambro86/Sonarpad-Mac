//! Ordered joining using the application's bundled FFmpeg, as in the media cutter.
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::Duration;

pub const FORMATS: &[&str] = &[
    "mp3", "m4a", "wav", "flac", "ogg", "opus", "aiff", "mp4", "mkv", "avi", "mov",
];

fn check(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Relaxed) {
        Err("__CANCELLED__".into())
    } else {
        Ok(())
    }
}

#[derive(Debug)]
struct Media {
    duration: f64,
    audio: bool,
    video: Option<(u32, u32)>,
}

fn parse_probe(text: &str) -> Result<Media, String> {
    let raw = text
        .split("Duration: ")
        .nth(1)
        .and_then(|s| s.split(',').next())
        .ok_or("Cannot determine media duration")?;
    let parts: Vec<_> = raw
        .trim()
        .split(':')
        .map(str::parse::<f64>)
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())?;
    if parts.len() != 3 {
        return Err("Invalid duration".into());
    }
    let duration = parts[0] * 3600.0 + parts[1] * 60.0 + parts[2];
    if !duration.is_finite() || duration <= 0.0 {
        return Err("Empty media file".into());
    }
    let video = text
        .lines()
        .filter(|l| l.contains("Video:") && !l.contains("attached pic"))
        .find_map(|l| {
            l.split([',', ' ']).find_map(|word| {
                let (w, h) = word.split_once('x')?;
                Some((w.parse::<u32>().ok()?, h.parse::<u32>().ok()?))
            })
        });
    Ok(Media {
        duration,
        video,
        audio: text.contains("Audio:"),
    })
}

pub(crate) fn run(
    ffmpeg: &Path,
    args: &[String],
    duration: f64,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(f64),
) -> Result<(), String> {
    check(cancel)?;
    let mut command = Command::new(ffmpeg);
    command
        .args([
            "-nostdin",
            "-hide_banner",
            "-nostats",
            "-progress",
            "pipe:1",
            "-y",
        ])
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().map_err(|e| format!("FFmpeg: {e}"))?;
    let stdout = child.stdout.take().ok_or("Missing FFmpeg output")?;
    let stderr = child.stderr.take().ok_or("Missing FFmpeg diagnostics")?;
    let (tx, rx) = mpsc::channel();
    let output_reader = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if let Some(value) = line.strip_prefix("out_time_us=")
                && let Ok(time) = value.parse::<f64>()
            {
                let _ = tx.send(time / 1_000_000.0);
            }
        }
    });
    let error_reader = std::thread::spawn(move || {
        let mut tail = Vec::new();
        for byte in BufReader::new(stderr).bytes().map_while(Result::ok) {
            tail.push(byte);
            if tail.len() > 16384 {
                tail.drain(..8192);
            }
        }
        String::from_utf8_lossy(&tail).into_owned()
    });
    let status = loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
        }
        for time in rx.try_iter() {
            progress((time / duration).clamp(0.0, 1.0));
        }
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => std::thread::sleep(Duration::from_millis(40)),
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(e);
            }
        }
    };
    let _ = output_reader.join();
    let errors = error_reader.join().unwrap_or_default();
    check(cancel)?;
    if !status.map_err(|e| e.to_string())?.success() {
        return Err(format!("FFmpeg: {errors}"));
    }
    progress(1.0);
    Ok(())
}

pub fn join(
    ffmpeg: &Path,
    inputs: &[PathBuf],
    output: &Path,
    cancel: Arc<AtomicBool>,
    progress: &mut dyn FnMut(u32),
) -> Result<(), String> {
    check(&cancel)?;
    if inputs.len() < 2 {
        return Err("Select at least two media files".into());
    }
    let extension = output
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !FORMATS.contains(&extension.as_str()) {
        return Err("Unsupported output format".into());
    }
    let output_real = std::fs::canonicalize(output).ok();
    if inputs.iter().any(|p| {
        p == output
            || output_real
                .as_ref()
                .is_some_and(|out| std::fs::canonicalize(p).ok().as_ref() == Some(out))
    }) {
        return Err("Input and output must be different files".into());
    }
    let video = ["mp4", "mkv", "avi", "mov"].contains(&extension.as_str());
    let mut media = Vec::new();
    for input in inputs {
        check(&cancel)?;
        // FFmpeg prints stream metadata even though -i without an output exits nonzero.
        let result = Command::new(ffmpeg)
            .args(["-nostdin", "-hide_banner", "-i"])
            .arg(input)
            .output()
            .map_err(|e| e.to_string())?;
        let info = parse_probe(&String::from_utf8_lossy(&result.stderr))?;
        if video && info.video.is_none() {
            return Err(format!("Input has no video stream: {}", input.display()));
        }
        if !video && !info.audio {
            return Err(format!("Input has no audio stream: {}", input.display()));
        }
        media.push(info);
    }
    let total: f64 = media.iter().map(|m| m.duration).sum();
    let work = output
        .parent()
        .unwrap_or(Path::new("."))
        .join(format!(".sonarpad-join-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&work).map_err(|e| e.to_string())?;
    let mut last = None;
    let mut report = |pct| {
        if last.is_none_or(|old| pct > old) {
            last = Some(pct);
            progress(pct);
        }
    };
    let result = (|| {
        report(0);
        let mut manifest = String::from("ffconcat version 1.0\n");
        let mut done = 0.0;
        let (width, height) = media[0].video.unwrap_or((0, 0));
        let width = (width + 1) & !1;
        let height = (height + 1) & !1;
        for (index, input) in inputs.iter().enumerate() {
            let name = format!("clip{index}.{}", if video { "mkv" } else { "wav" });
            let mut args = vec!["-i".into(), input.to_string_lossy().into_owned()];
            if video && !media[index].audio {
                args.extend(["-f", "lavfi", "-i", "anullsrc=r=48000:cl=stereo,aformat=sample_fmts=s16"].map(String::from));
            }
            if video {
                args.extend(
                    [
                        "-map",
                        "0:v:0",
                        "-map",
                        if media[index].audio { "0:a:0" } else { "1:a:0" },
                        "-c:v",
                        "mpeg4",
                        "-q:v",
                        "4",
                        "-pix_fmt",
                        "yuv420p",
                        "-vf",
                    ]
                    .map(String::from),
                );
                args.push(format!("scale={width}:{height}:force_original_aspect_ratio=decrease,pad={width}:{height}:(ow-iw)/2:(oh-ih)/2,setsar=1,fps=30"));
                args.extend([
                    "-af".into(),
                    "apad".into(),
                    "-t".into(),
                    format!("{:.9}", media[index].duration),
                ]);
            } else {
                args.extend(["-map", "0:a:0", "-vn"].map(String::from));
            }
            args.extend(["-ar", "48000", "-ac", "2", "-c:a", "pcm_s16le"].map(String::from));
            args.push(work.join(&name).to_string_lossy().into_owned());
            run(ffmpeg, &args, media[index].duration, &cancel, &mut |p| {
                report(((done + p * media[index].duration) / total * 80.0) as u32)
            })?;
            manifest.push_str(&format!("file '{name}'\n"));
            done += media[index].duration;
        }
        let list = work.join("joined.ffconcat");
        std::fs::write(&list, manifest).map_err(|e| e.to_string())?;
        let staged = work.join(format!("result.{extension}"));
        let mut args = vec![
            "-f".into(),
            "concat".into(),
            "-i".into(),
            list.to_string_lossy().into_owned(),
        ];
        if video {
            args.extend(["-c:v", "copy"].map(String::from));
        }
        let audio: &[&str] = match extension.as_str() {
            "mp3" | "avi" => &["-c:a", "libmp3lame", "-b:a", "192k"],
            "m4a" | "mp4" | "mkv" | "mov" => &["-c:a", "aac", "-b:a", "192k"],
            "wav" => &["-c:a", "pcm_s16le"],
            "aiff" => &["-c:a", "pcm_s16be"],
            "flac" => &["-c:a", "flac"],
            "opus" => &["-c:a", "opus", "-strict", "-2", "-b:a", "128k"],
            "ogg" => &["-c:a", "vorbis", "-strict", "-2", "-q:a", "5"],
            _ => unreachable!(),
        };
        args.extend(audio.iter().map(|s| (*s).into()));
        args.push(staged.to_string_lossy().into_owned());
        run(ffmpeg, &args, total, &cancel, &mut |p| {
            report(80 + (p * 19.0) as u32)
        })?;
        check(&cancel)?;
        std::fs::rename(&staged, output).map_err(|e| e.to_string())?;
        report(100);
        Ok(())
    })();
    if let Err(e) = std::fs::remove_dir_all(work) {
        eprintln!("Media join cleanup: {e}");
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn probe_ignores_cover_and_reads_video_size() {
        let m = parse_probe("Duration: 01:02:03.50, start: 0\nStream: Video: png, rgb24, 600x600 (attached pic)\nStream: Audio: aac\n").unwrap();
        assert_eq!(m.duration, 3723.5);
        assert!(m.audio);
        assert!(m.video.is_none());
        assert_eq!(
            parse_probe(
                "Duration: 00:00:02.00,\nStream: Video: h264, yuv420p, 640x360 [SAR 1:1]\n"
            )
            .unwrap()
            .video,
            Some((640, 360))
        );
        assert!(parse_probe("Duration: N/A").is_err());
    }
    #[test]
    fn validation_happens_before_starting_ffmpeg() {
        let flag = Arc::new(AtomicBool::new(false));
        assert!(
            join(
                Path::new("nonexistent"),
                &[],
                Path::new("out.wav"),
                flag.clone(),
                &mut |_| panic!()
            )
            .is_err()
        );
        let files = vec![PathBuf::from("a.wav"), PathBuf::from("b.wav")];
        assert!(
            join(
                Path::new("nonexistent"),
                &files,
                &files[0],
                flag.clone(),
                &mut |_| panic!()
            )
            .unwrap_err()
            .contains("different")
        );
        flag.store(true, Ordering::Relaxed);
        assert_eq!(
            join(
                Path::new("nonexistent"),
                &files,
                Path::new("out.wav"),
                flag,
                &mut |_| panic!()
            )
            .unwrap_err(),
            "__CANCELLED__"
        );
    }
}

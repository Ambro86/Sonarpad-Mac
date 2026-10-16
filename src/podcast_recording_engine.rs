//! Audio capture transport and export. GUI-free so the real pipeline can be tested locally.
use base64::Engine;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};

pub const FORMATS: &[&str] = &["mp3", "m4a", "wav"];
pub const RATE: u32 = 48_000;
#[derive(Clone, Debug)]
pub struct Device {
    pub id: String,
    pub name: String,
}
pub fn helper_path() -> Result<PathBuf, String> {
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    let folder = executable.parent().ok_or("Missing application folder")?;
    [
        folder.join("podcast-capture"),
        folder.join("../Resources/podcast-capture"),
    ]
    .into_iter()
    .find(|p| p.is_file())
    .ok_or_else(|| "__HELPER_MISSING__".into())
}
pub fn devices(helper: &Path) -> Result<(bool, Vec<Device>), String> {
    let result = Command::new(helper)
        .arg("--devices")
        .output()
        .map_err(|e| e.to_string())?;
    if !result.status.success() {
        return Err(String::from_utf8_lossy(&result.stderr).into_owned());
    }
    let data: serde_json::Value =
        serde_json::from_slice(&result.stdout).map_err(|e| e.to_string())?;
    let devices = data["devices"]
        .as_array()
        .ok_or("Invalid device response")?
        .iter()
        .filter_map(|d| {
            Some(Device {
                id: d["id"].as_str()?.into(),
                name: d["name"].as_str()?.into(),
            })
        })
        .collect();
    Ok((data["system"].as_bool().unwrap_or(false), devices))
}
#[derive(Clone)]
pub struct Config {
    pub microphone: Option<String>,
    pub system: bool,
    pub mic_gain: f64,
    pub system_gain: f64,
    pub output: PathBuf,
}
#[derive(Debug)]
pub enum Control {
    Pause,
    Resume,
    Stop,
}
#[derive(Clone, Debug)]
pub enum Event {
    Starting,
    Recording,
    Paused,
    Elapsed(f64),
    Saving(u32),
    Done(PathBuf),
    Failed(String, Option<PathBuf>),
}

/// A source on the common active-recording timeline. Pauses are removed by capture helper.
/// Missing packets become silence; overlapping/late packets never duplicate already-written audio.
pub struct Track {
    writer: hound::WavWriter<BufWriter<File>>,
    frames: u64,
}
impl Track {
    pub fn new(path: &Path) -> Result<Self, String> {
        Ok(Self {
            writer: hound::WavWriter::create(
                path,
                hound::WavSpec {
                    channels: 2,
                    sample_rate: RATE,
                    bits_per_sample: 32,
                    sample_format: hound::SampleFormat::Float,
                },
            )
            .map_err(|e| e.to_string())?,
            frames: 0,
        })
    }
    fn silence_to(&mut self, end: u64) -> Result<(), String> {
        for _ in self.frames..end {
            self.writer.write_sample(0f32).map_err(|e| e.to_string())?;
            self.writer.write_sample(0f32).map_err(|e| e.to_string())?;
        }
        self.frames = self.frames.max(end);
        Ok(())
    }
    pub fn push(&mut self, seconds: f64, bytes: &[u8]) -> Result<(), String> {
        if !seconds.is_finite() || seconds < 0.0 || !bytes.len().is_multiple_of(8) {
            return Err("Invalid audio packet".into());
        }
        let start = (seconds * RATE as f64).round() as u64;
        if seconds > 86_400.0 {
            return Err("Capture timestamp discontinuity".into());
        }
        self.silence_to(start)?;
        let skip = (self.frames - start).min((bytes.len() / 8) as u64) as usize;
        for frame in bytes[skip * 8..].as_chunks::<8>().0 {
            for sample in frame.as_chunks::<4>().0 {
                let value = f32::from_le_bytes(*sample);
                self.writer
                    .write_sample(if value.is_finite() { value } else { 0.0 })
                    .map_err(|e| e.to_string())?;
            }
            self.frames += 1;
        }
        Ok(())
    }
    pub fn flush(&mut self) -> Result<(), String> {
        self.writer.flush().map_err(|e| e.to_string())
    }
    pub fn finish(mut self, seconds: f64) -> Result<(), String> {
        if !seconds.is_finite() || seconds < 0.0 {
            return Err("Invalid recording duration".into());
        }
        self.silence_to((seconds * RATE as f64).round() as u64)?;
        self.writer.finalize().map_err(|e| e.to_string())
    }
}

pub fn export(
    ffmpeg: &Path,
    tracks: &[(PathBuf, f64)],
    output: &Path,
    duration: f64,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(u32),
) -> Result<(), String> {
    if tracks.is_empty() || !duration.is_finite() || duration <= 0.0 {
        return Err("Empty recording".into());
    }
    if tracks
        .iter()
        .any(|(_, gain)| !gain.is_finite() || !(0.0..=2.0).contains(gain))
    {
        return Err("Invalid recording volume".into());
    }
    if output.exists() {
        return Err("__EXISTS__".into());
    }
    let extension = output
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !FORMATS.contains(&extension.as_str()) {
        return Err("Unsupported recording format".into());
    }
    let staged = output.with_file_name(format!(".sonarpad-{}.{}", uuid::Uuid::new_v4(), extension));
    let result = (|| {
        let mut args = Vec::new();
        for (path, _) in tracks {
            args.extend(["-i".into(), path.to_string_lossy().into_owned()]);
        }
        let mut filter = String::new();
        for (i, (_, gain)) in tracks.iter().enumerate() {
            filter.push_str(&format!("[{i}:a]volume={gain}[a{i}];"));
        }
        for i in 0..tracks.len() {
            filter.push_str(&format!("[a{i}]"));
        }
        filter.push_str(&format!("amix=inputs={}:duration=longest:normalize=0,alimiter=limit=0.95:level=0:latency=1[out]", tracks.len()));
        args.extend([
            "-filter_complex".into(),
            filter,
            "-map".into(),
            "[out]".into(),
            "-t".into(),
            duration.to_string(),
        ]);
        let codec: &[&str] = match extension.as_str() {
            "wav" => &["-c:a", "pcm_s16le"],
            "mp3" => &["-c:a", "libmp3lame", "-b:a", "192k"],
            _ => &["-c:a", "aac", "-b:a", "192k", "-movflags", "+faststart"],
        };
        args.extend(codec.iter().map(|s| s.to_string()));
        args.push(staged.to_string_lossy().into_owned());
        crate::media_join_engine::run(ffmpeg, &args, duration, cancel, &mut |pct| {
            progress((pct * 99.0) as u32)
        })?;
        if cancel.load(Ordering::Relaxed) {
            return Err("__CANCELLED__".into());
        }
        // Exclusive publication: never overwrite a recording created while we were encoding.
        let mut destination = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output)
            .map_err(|e| e.to_string())?;
        let copy = (|| {
            let mut source = File::open(&staged)?;
            std::io::copy(&mut source, &mut destination)?;
            destination.sync_all()
        })();
        drop(destination);
        if let Err(e) = copy {
            let _ = std::fs::remove_file(output);
            return Err(e.to_string());
        }
        progress(100);
        Ok(())
    })();
    let _ = std::fs::remove_file(staged);
    result
}

// Always reap the native helper, including disk/protocol errors and parent cancellation.
struct CaptureChild(std::process::Child);
impl Drop for CaptureChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

pub fn record(
    helper: &Path,
    ffmpeg: &Path,
    config: Config,
    controls: mpsc::Receiver<Control>,
    cancel: &AtomicBool,
    events: &mpsc::Sender<Event>,
) {
    let mut recovery = None;
    let result = (|| -> Result<(), String> {
        if !config.system && config.microphone.is_none() {
            return Err("__NO_SOURCE__".into());
        }
        if config.output.exists() {
            return Err("__EXISTS__".into());
        }
        let parent = config
            .output
            .parent()
            .filter(|p| p.is_dir())
            .ok_or("Invalid destination folder")?;
        let work = parent.join(format!(".sonarpad-recording-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&work).map_err(|e| e.to_string())?;
        recovery = Some(work.clone());
        let mic_path = work.join("microphone.wav");
        let system_path = work.join("system.wav");
        let mut mic = config
            .microphone
            .as_ref()
            .map(|_| Track::new(&mic_path))
            .transpose()?;
        let mut system = config
            .system
            .then(|| Track::new(&system_path))
            .transpose()?;
        let mut command = Command::new(helper);
        if config.system {
            command.arg("--system");
        }
        if let Some(id) = &config.microphone {
            command.args(["--microphone", id]);
        }
        let mut child = CaptureChild(
            command
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|e| e.to_string())?,
        );
        let mut stdin = child.0.stdin.take().ok_or("Missing capture command pipe")?;
        let stdout = child.0.stdout.take().ok_or("Missing capture audio pipe")?;
        let stderr = child.0.stderr.take().ok_or("Missing capture diagnostics")?;
        let error_reader = std::thread::spawn(move || {
            let mut tail = Vec::new();
            for byte in BufReader::new(stderr).bytes().map_while(Result::ok) {
                tail.push(byte);
                if tail.len() > 8192 {
                    tail.drain(..4096);
                }
            }
            String::from_utf8_lossy(&tail).into_owned()
        });
        let (tx, rx) = mpsc::sync_channel(32);
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        let _ = events.send(Event::Starting);
        let mut ready = false;
        let mut stopped = false;
        let mut duration = 0.0;
        let mut deadline = None;
        let mut last_flush = Instant::now();
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err("__CANCELLED__".into());
            }
            if ready && deadline.is_none() {
                for control in controls.try_iter() {
                    let name = match control {
                        Control::Pause => "pause",
                        Control::Resume => "resume",
                        Control::Stop => "stop",
                    };
                    writeln!(stdin, "{name}")
                        .and_then(|_| stdin.flush())
                        .map_err(|e| e.to_string())?;
                    if matches!(control, Control::Stop) {
                        deadline = Some(Instant::now() + Duration::from_secs(10));
                        break;
                    }
                }
            }
            if deadline.is_some_and(|end| Instant::now() > end) {
                return Err("Capture stop timed out".into());
            }
            if last_flush.elapsed() >= Duration::from_secs(2) {
                if let Some(track) = mic.as_mut() {
                    track.flush()?;
                }
                if let Some(track) = system.as_mut() {
                    track.flush()?;
                }
                last_flush = Instant::now();
            }
            match rx.recv_timeout(Duration::from_millis(30)) {
                Ok(line) => {
                    let data: serde_json::Value =
                        serde_json::from_str(&line.map_err(|e| e.to_string())?)
                            .map_err(|e| e.to_string())?;
                    match data["event"].as_str().unwrap_or("") {
                        "ready" => {
                            ready = true;
                            let _ = events.send(Event::Recording);
                        }
                        "paused" => {
                            let _ = events.send(Event::Paused);
                        }
                        "resumed" => {
                            let _ = events.send(Event::Recording);
                        }
                        "time" => {
                            duration = data["seconds"].as_f64().unwrap_or(duration);
                            let _ = events.send(Event::Elapsed(duration));
                        }
                        "audio" => {
                            let raw = base64::engine::general_purpose::STANDARD
                                .decode(data["data"].as_str().ok_or("Missing PCM data")?)
                                .map_err(|e| e.to_string())?;
                            let track = match data["source"].as_str() {
                                Some("microphone") => &mut mic,
                                Some("system") => &mut system,
                                _ => return Err("Invalid audio source".into()),
                            };
                            track.as_mut().ok_or("Unexpected audio source")?.push(
                                data["time"].as_f64().ok_or("Missing audio timestamp")?,
                                &raw,
                            )?;
                        }
                        "stopped" => {
                            duration = data["seconds"].as_f64().ok_or("Missing duration")?;
                            stopped = true;
                            break;
                        }
                        "error" => {
                            return Err(format!(
                                "{}: {}",
                                data["code"].as_str().unwrap_or("capture"),
                                data["detail"].as_str().unwrap_or("")
                            ));
                        }
                        _ => return Err("Invalid capture event".into()),
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
        let exit = child.0.wait().map_err(|e| e.to_string())?;
        let diagnostics = error_reader.join().unwrap_or_default();
        if !stopped || !exit.success() {
            return Err(format!("Capture interrupted: {diagnostics}"));
        }
        let mut inputs = Vec::new();
        if let Some(track) = mic {
            track.finish(duration)?;
            inputs.push((mic_path, config.mic_gain));
        }
        if let Some(track) = system {
            track.finish(duration)?;
            inputs.push((system_path, config.system_gain));
        }
        let _ = events.send(Event::Saving(0));
        export(
            ffmpeg,
            &inputs,
            &config.output,
            duration,
            cancel,
            &mut |pct| {
                let _ = events.send(Event::Saving(pct));
            },
        )?;
        let _ = std::fs::remove_dir_all(&work);
        recovery = None;
        Ok(())
    })();
    let event = match result {
        Ok(()) => Event::Done(config.output),
        Err(error) => Event::Failed(error, recovery),
    };
    let _ = events.send(event);
}

#[cfg(test)]
mod tests {
    use super::*;
    fn packet(value: f32, frames: usize) -> Vec<u8> {
        (0..frames * 2).flat_map(|_| value.to_le_bytes()).collect()
    }
    #[test]
    fn timestamp_gaps_and_overlaps_preserve_alignment() {
        let path = std::env::temp_dir().join(format!("{}.wav", uuid::Uuid::new_v4()));
        let mut track = Track::new(&path).unwrap();
        track.push(0.1, &packet(0.25, 4800)).unwrap();
        track.push(0.15, &packet(0.5, 4800)).unwrap();
        // After a long pause the helper resumes at active time, without the paused interval.
        track.push(0.25, &packet(0.75, 4800)).unwrap();
        track.finish(0.4).unwrap();
        let mut wav = hound::WavReader::open(&path).unwrap();
        let data: Vec<_> = wav.samples::<f32>().map(Result::unwrap).collect();
        assert_eq!(data.len(), 38400);
        assert!(data[..9600].iter().all(|v| *v == 0.0));
        assert!(data[9600..19200].iter().all(|v| *v == 0.25));
        assert!(data[19200..24000].iter().all(|v| *v == 0.5));
        assert!(data[24000..33600].iter().all(|v| *v == 0.75));
        assert!(data[33600..].iter().all(|v| *v == 0.0));
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn rejects_invalid_packets_and_clock_jumps() {
        let path = std::env::temp_dir().join(format!("{}.wav", uuid::Uuid::new_v4()));
        let mut track = Track::new(&path).unwrap();
        assert!(track.push(f64::NAN, &[0; 8]).is_err());
        assert!(track.push(-1.0, &[0; 8]).is_err());
        assert!(track.push(0.0, &[0; 3]).is_err());
        assert!(track.push(100000.0, &[0; 8]).is_err());
        track.finish(0.0).unwrap();
        std::fs::remove_file(path).unwrap();
    }
}

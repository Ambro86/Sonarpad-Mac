//! FFmpeg process handling and a bounded fallback for damaged audio sources.
//!
//! The ordinary command line is passed through unchanged. Recovery is selected
//! only by an audio-decoder error recorded here, never by a path/permission
//! error or cancellation. The Mac bundle already includes the FFmpeg CLI;
//! restarting that decoder replaces Windows' native decoder flush/seek.

use std::collections::VecDeque;
use std::fs::{self, File};
use std::io::{BufReader, Read, Seek};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const STDERR_LIMIT: usize = 64 * 1024;
const LINE_LIMIT: usize = 8 * 1024;
const CORRUPTION_WINDOW_LIMIT: u32 = 96;
const CORRUPTION_STREAM_LIMIT: u32 = 128;
const RECOVERY_MARKER: &str = "[audio_decode_recovery_required]";
const SAMPLE_RATE: u32 = 48_000;
const CHANNELS: u16 = 2;
const BYTES_PER_FRAME: u64 = 4;
const MAX_RESYNCS: u32 = 45;
const MAX_EMPTY_ATTEMPTS: u32 = 3;
const RECOVERY_IDLE_LIMIT: Duration = Duration::from_secs(30);
static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

/// Only this module can mark a failure as suitable for audio recovery.
pub(crate) fn needs_audio_recovery(error: &str) -> bool {
    error.starts_with(RECOVERY_MARKER)
}

fn is_local_or_configuration_failure(line: &str) -> bool {
    [
        "permission denied",
        "operation not permitted",
        "no such file or directory",
        "is a directory",
        "read-only file system",
        "no space left on device",
        "disk quota exceeded",
        "cannot allocate memory",
        "resource temporarily unavailable",
        "error opening output",
        "unable to choose an output format",
        "unable to find a suitable output format",
        "error initializing output",
        "error initializing filter",
        "error reinitializing filter",
        "no such filter",
        "unknown encoder",
        "unknown decoder",
        "encoder not found",
        "decoder not found",
        "unrecognized option",
        "option not found",
        "matches no streams",
        "does not contain any stream",
    ]
    .iter()
    .any(|marker| line.contains(marker))
}

fn is_corrupt_audio_message(line: &str) -> bool {
    [
        "header missing",
        "invalid new backstep",
        "backstep is not positive",
        "big_values too big",
        "invalid block type",
        "invalid audio packet",
        "invalid packet size",
        "invalid frame size",
        "invalid syncword",
        "frame sync error",
        "packet corrupt",
        "corrupt decoded frame",
        "audio frame is corrupted",
        "invalid data found when processing input",
        "prediction is not allowed in aac-lc",
        "sbr was found before the first channel element",
    ]
    .iter()
    .any(|marker| line.contains(marker))
        || (line.contains("channel element") && line.contains("not allocated"))
        || (line.contains("error decoding") && line.contains("invalid"))
}

fn is_informational_log(line: &str) -> bool {
    let mut message = line.trim_start();
    for _ in 0..2 {
        if ["[info]", "[verbose]", "[debug]", "[trace]"]
            .iter()
            .any(|level| message.starts_with(level))
        {
            return true;
        }
        if !message.starts_with('[') {
            break;
        }
        let Some(end) = message.find(']') else {
            break;
        };
        message = message[end + 1..].trim_start();
    }
    false
}

#[derive(Default)]
struct StderrState {
    tail: VecDeque<u8>,
    pending_line: Vec<u8>,
    bad_in_window: u32,
    bad_in_stream: u32,
    budget_triggered: bool,
    last_line_was_corrupt: bool,
    other_failure: bool,
    read_error: Option<String>,
    decoded_frames: Option<Arc<AtomicU64>>,
}

impl StderrState {
    fn observe_line(&mut self) {
        if self.pending_line.is_empty() {
            return;
        }
        let line = String::from_utf8_lossy(&self.pending_line).to_ascii_lowercase();
        self.pending_line.clear();
        if let Some(counter) = &self.decoded_frames
            && line.contains("[parsed_ashowinfo_")
            && let Some(raw) = line.split("nb_samples:").nth(1)
            && let Some(frames) = raw
                .split_whitespace()
                .next()
                .and_then(|n| n.parse::<u64>().ok())
        {
            counter.fetch_add(frames, Ordering::Relaxed);
        }
        // Fallback includes frame-count metadata at info level. A source
        // filename printed there must not masquerade as a decoder/I/O error.
        if is_informational_log(&line) {
            self.last_line_was_corrupt = false;
            return;
        }
        if is_local_or_configuration_failure(&line) {
            self.other_failure = true;
            self.last_line_was_corrupt = false;
            return;
        }
        // FFmpeg compresses repeated diagnostics by default. Those packets
        // still consume the same budget as individually printed failures.
        let repetitions = line
            .split("last message repeated ")
            .nth(1)
            .and_then(|raw| raw.split_whitespace().next())
            .and_then(|raw| raw.parse::<u32>().ok());
        let count = if self.last_line_was_corrupt && repetitions.is_some() {
            repetitions.unwrap_or(0)
        } else if is_corrupt_audio_message(&line) {
            self.last_line_was_corrupt = true;
            1
        } else {
            self.last_line_was_corrupt = false;
            0
        };
        self.bad_in_window = self.bad_in_window.saturating_add(count);
        self.bad_in_stream = self.bad_in_stream.saturating_add(count);
        self.budget_triggered |= self.bad_in_window >= CORRUPTION_WINDOW_LIMIT
            || self.bad_in_stream >= CORRUPTION_STREAM_LIMIT;
    }

    fn push(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            if self.tail.len() == STDERR_LIMIT {
                self.tail.pop_front();
            }
            self.tail.push_back(byte);
            if byte == b'\n' || byte == b'\r' {
                self.observe_line();
            } else {
                self.pending_line.push(byte);
                if self.pending_line.len() == LINE_LIMIT {
                    self.observe_line();
                }
            }
        }
    }

    fn budget_exceeded(&self) -> bool {
        !self.other_failure && self.budget_triggered
    }

    fn diagnostic(&self) -> String {
        let bytes: Vec<_> = self.tail.iter().copied().collect();
        String::from_utf8_lossy(&bytes).trim().to_string()
    }
}

fn terminate_and_reap(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

struct RunOptions<'a> {
    guard_corrupt_audio: bool,
    stop_on_decode_error: bool,
    output_path: Option<&'a Path>,
    idle_limit: Option<Duration>,
    decoded_frames: Option<Arc<AtomicU64>>,
}

enum ProcessStop {
    Exited(ExitStatus),
    Corruption,
    Cancelled,
    Failed(String),
}

struct ProcessReport {
    corrupt_audio_errors: u32,
    other_failure: bool,
    diagnostic: String,
}

/// Drain stderr concurrently: waiting for exit before reading can deadlock
/// FFmpeg as soon as repeated decoder diagnostics fill its pipe.
pub(crate) fn run(
    ffmpeg: &Path,
    args: &[String],
    cancel: &AtomicBool,
    guard_corrupt_audio: bool,
) -> Result<(), String> {
    let output = args.last().map(PathBuf::from);
    run_process(
        ffmpeg,
        args,
        cancel,
        RunOptions {
            guard_corrupt_audio,
            stop_on_decode_error: false,
            output_path: output.as_deref(),
            idle_limit: None,
            decoded_frames: None,
        },
    )
    .map(|_| ())
}

/// The primary decoder keeps its ordinary arguments. A successful exit alone
/// is insufficient only when decoder corruption was also reported and the WAV
/// contains no PCM, is truncated, or is far shorter than the known source.
pub(crate) fn run_audio_decode(
    ffmpeg: &Path,
    args: &[String],
    cancel: &AtomicBool,
    expected_duration: f64,
) -> Result<(), String> {
    let output = args.last().map(PathBuf::from);
    let report = run_process(
        ffmpeg,
        args,
        cancel,
        RunOptions {
            guard_corrupt_audio: true,
            stop_on_decode_error: false,
            output_path: output.as_deref(),
            idle_limit: None,
            decoded_frames: None,
        },
    )?;
    if report.other_failure {
        return Err(format!("FFmpeg fallito: {}", report.diagnostic));
    }
    if report.corrupt_audio_errors == 0 {
        return Ok(());
    }
    let Some(output) = output else { return Ok(()) };
    let extent = decoded_wav_extent(&output)?;
    if cancel.load(Ordering::Relaxed) {
        return Err("cancelled".to_string());
    }
    if extent.frames == 0
        || extent.truncated
        || (expected_duration.is_finite()
            && expected_duration > 30.0
            && extent.seconds < expected_duration * 0.8)
    {
        return Err(format!(
            "{RECOVERY_MARKER} FFmpeg: audio incompleto dopo errori del decoder ({:.3} s, attesi circa {expected_duration:.3} s): {}",
            extent.seconds, report.diagnostic
        ));
    }
    Ok(())
}

#[derive(Default)]
struct WavExtent {
    frames: u64,
    seconds: f64,
    truncated: bool,
}

fn decoded_wav_extent(path: &Path) -> Result<WavExtent, String> {
    let reader = match hound::WavReader::open(path) {
        Ok(reader) => reader,
        Err(hound::Error::IoError(error))
            if !matches!(
                error.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::UnexpectedEof
            ) =>
        {
            return Err(format!("Verifica WAV sorgente fallita: {error}"));
        }
        Err(_) => {
            return Ok(WavExtent {
                truncated: true,
                ..WavExtent::default()
            });
        }
    };
    let spec = reader.spec();
    let sample_bytes = u64::from(spec.bits_per_sample.div_ceil(8));
    let channels = u64::from(spec.channels);
    if sample_bytes == 0 || channels == 0 || spec.sample_rate == 0 {
        return Ok(WavExtent {
            truncated: true,
            ..WavExtent::default()
        });
    }
    let declared_samples = u64::from(reader.len());
    let declared_bytes = declared_samples.saturating_mul(sample_bytes);
    let mut file = reader.into_inner();
    let data_start = file
        .stream_position()
        .map_err(|error| format!("Verifica posizione WAV fallita: {error}"))?;
    let file_size = file
        .get_ref()
        .metadata()
        .map_err(|error| format!("Verifica dimensione WAV fallita: {error}"))?
        .len();
    let available_bytes = file_size.saturating_sub(data_start);
    let frames = declared_bytes.min(available_bytes) / (channels * sample_bytes);
    Ok(WavExtent {
        frames,
        seconds: frames as f64 / f64::from(spec.sample_rate),
        truncated: available_bytes < declared_bytes || !declared_samples.is_multiple_of(channels),
    })
}

fn run_process(
    ffmpeg: &Path,
    args: &[String],
    cancel: &AtomicBool,
    options: RunOptions<'_>,
) -> Result<ProcessReport, String> {
    if cancel.load(Ordering::Relaxed) {
        return Err("cancelled".to_string());
    }
    let mut child = Command::new(ffmpeg)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("Avvio FFmpeg fallito: {error}"))?;
    let Some(mut stderr) = child.stderr.take() else {
        terminate_and_reap(&mut child);
        return Err("FFmpeg stderr non disponibile".to_string());
    };
    let state = Arc::new(Mutex::new(StderrState {
        decoded_frames: options.decoded_frames,
        ..StderrState::default()
    }));
    let reader_state = Arc::clone(&state);
    let reader = match thread::Builder::new()
        .name("ad-ffmpeg-stderr".to_string())
        .spawn(move || {
            let mut bytes = [0_u8; 8192];
            loop {
                match stderr.read(&mut bytes) {
                    Ok(0) => break,
                    Ok(count) => reader_state.lock().unwrap().push(&bytes[..count]),
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(error) => {
                        reader_state.lock().unwrap().read_error = Some(error.to_string());
                        break;
                    }
                }
            }
            reader_state.lock().unwrap().observe_line();
        }) {
        Ok(reader) => reader,
        Err(error) => {
            terminate_and_reap(&mut child);
            return Err(format!("Avvio lettura FFmpeg fallito: {error}"));
        }
    };
    let mut output_size = 0_u64;
    let mut last_output_growth = Instant::now();
    let stopped = loop {
        if cancel.load(Ordering::Relaxed) {
            terminate_and_reap(&mut child);
            break ProcessStop::Cancelled;
        }
        if let Some(path) = options.output_path
            && let Ok(metadata) = fs::metadata(path)
        {
            if metadata.len() > output_size {
                last_output_growth = Instant::now();
                state.lock().unwrap().bad_in_window = 0;
            }
            output_size = metadata.len();
        }
        let (exceeded, read_error) = {
            let state = state.lock().unwrap();
            (
                state.budget_exceeded()
                    || (options.stop_on_decode_error
                        && state.bad_in_stream > 0
                        && !state.other_failure),
                state.read_error.clone(),
            )
        };
        if let Some(error) = read_error {
            terminate_and_reap(&mut child);
            break ProcessStop::Failed(format!("Lettura FFmpeg fallita: {error}"));
        }
        if options.guard_corrupt_audio && exceeded {
            terminate_and_reap(&mut child);
            break ProcessStop::Corruption;
        }
        match child.try_wait() {
            Ok(Some(status)) => break ProcessStop::Exited(status),
            Ok(None) => {}
            Err(error) => {
                terminate_and_reap(&mut child);
                break ProcessStop::Failed(format!("Controllo FFmpeg fallito: {error}"));
            }
        }
        if let Some(limit) = options.idle_limit
            && last_output_growth.elapsed() >= limit
        {
            terminate_and_reap(&mut child);
            break ProcessStop::Failed(
                "Recupero audio FFmpeg interrotto: nessun avanzamento della decodifica."
                    .to_string(),
            );
        }
        thread::sleep(Duration::from_millis(50));
    };
    if reader.join().is_err() {
        return Err("Lettura diagnostica FFmpeg interrotta".to_string());
    }
    if cancel.load(Ordering::Relaxed) || matches!(stopped, ProcessStop::Cancelled) {
        return Err("cancelled".to_string());
    }
    let state = state.lock().unwrap();
    if let Some(error) = &state.read_error {
        return Err(format!("Lettura FFmpeg fallita: {error}"));
    }
    let diagnostic = state.diagnostic();
    match stopped {
        ProcessStop::Exited(status)
            if status.success()
                && !(options.guard_corrupt_audio
                    && (state.budget_exceeded()
                        || (options.stop_on_decode_error && state.other_failure)
                        || (options.stop_on_decode_error
                            && state.bad_in_stream > 0
                            && !state.other_failure))) =>
        {
            Ok(ProcessReport {
                corrupt_audio_errors: state.bad_in_stream,
                other_failure: state.other_failure,
                diagnostic,
            })
        }
        ProcessStop::Exited(_) | ProcessStop::Corruption => {
            if options.guard_corrupt_audio && state.bad_in_stream > 0 && !state.other_failure {
                Err(format!(
                    "{RECOVERY_MARKER} FFmpeg: audio danneggiato ({} errori): {diagnostic}",
                    state.bad_in_stream
                ))
            } else {
                Err(format!("FFmpeg fallito: {diagnostic}"))
            }
        }
        ProcessStop::Cancelled => Err("cancelled".to_string()),
        ProcessStop::Failed(error) => Err(error),
    }
}

struct RecoveryTempDir(PathBuf);

impl RecoveryTempDir {
    fn create(output: &Path) -> Result<Self, String> {
        let parent = output
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        for _ in 0..8 {
            let serial = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
            let path = parent.join(format!(
                "sonarpad-audio-recovery-{}-{timestamp}-{serial}",
                std::process::id()
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self(path)),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(format!("Creazione audio temporaneo fallita: {error}")),
            }
        }
        Err("Impossibile creare il file audio temporaneo per il recupero".to_string())
    }
}

impl Drop for RecoveryTempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn recovery_args(
    input: &Path,
    raw: &Path,
    stream: Option<i32>,
    start: f64,
    remaining: f64,
) -> Vec<String> {
    let mut args = vec![
        "-hide_banner".to_string(),
        "-loglevel".to_string(),
        "level+info".to_string(),
        "-nostdin".to_string(),
        "-y".to_string(),
        // Strict decoding is fallback-only. Its first bad packet ends the
        // current segment, leaving its good prefix available for recovery.
        "-xerror".to_string(),
        "-err_detect".to_string(),
        "explode".to_string(),
        "-ss".to_string(),
        format!("{start:.9}"),
        "-i".to_string(),
        input.to_string_lossy().to_string(),
    ];
    if let Some(index) = stream {
        args.extend(["-map".to_string(), format!("0:{index}")]);
    }
    args.extend([
        "-vn".to_string(),
        "-sn".to_string(),
        "-dn".to_string(),
        "-ac".to_string(),
        CHANNELS.to_string(),
        "-ar".to_string(),
        SAMPLE_RATE.to_string(),
        "-af".to_string(),
        // Count real resampled frames before timestamp synchronization: any
        // automatic gap padding must consume the same limited silence budget.
        "aresample=48000,aformat=sample_fmts=s16:channel_layouts=stereo,ashowinfo,aresample=async=1:first_pts=0".to_string(),
        "-t".to_string(),
        format!("{remaining:.9}"),
        "-c:a".to_string(),
        "pcm_s16le".to_string(),
        "-f".to_string(),
        "s16le".to_string(),
        raw.to_string_lossy().to_string(),
    ]);
    args
}

/// Recover into a private WAV and publish only a validated result. Seeking is
/// accurate transcoding (no `-copyts` / stream copy), so a restarted decoder
/// cannot append the earlier prefix again. The media cursor always derives
/// from output frames, including silence already inserted at damaged regions.
pub(crate) fn recover_source_audio(
    ffmpeg: &Path,
    input: &Path,
    output: &Path,
    audio_stream: Option<i32>,
    source_duration: f64,
    cancel: &AtomicBool,
) -> Result<(), String> {
    if cancel.load(Ordering::Relaxed) {
        return Err("cancelled".to_string());
    }
    if !source_duration.is_finite() || source_duration <= 0.0 {
        return Err("Durata non valida per il recupero audio FFmpeg".to_string());
    }
    if audio_stream.is_some_and(|index| index < 0) {
        return Err("Indice traccia non valido per il recupero audio FFmpeg".to_string());
    }
    let max_frames = (source_duration * SAMPLE_RATE as f64).ceil() as u64;
    if max_frames > (u32::MAX as u64 - 44) / BYTES_PER_FRAME {
        return Err("Audio troppo lungo per il WAV temporaneo di recupero".to_string());
    }
    let silence_budget =
        ((source_duration * 0.02).clamp(2.0, 45.0) * SAMPLE_RATE as f64).floor() as u64;
    let temp = RecoveryTempDir::create(output)?;
    let raw = temp.0.join("segment.pcm");
    let staged_wav = temp.0.join("recovered.wav");
    let spec = hound::WavSpec {
        channels: CHANNELS,
        sample_rate: SAMPLE_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(&staged_wav, spec)
        .map_err(|error| format!("Creazione WAV di recupero fallita: {error}"))?;
    let mut written_frames = 0_u64;
    let mut decoded_frames = 0_u64;
    let mut silent_frames = 0_u64;
    let mut resyncs = 0_u32;
    let mut empty_attempts = 0_u32;

    while written_frames < max_frames {
        if cancel.load(Ordering::Relaxed) {
            return Err("cancelled".to_string());
        }
        if raw.exists() {
            fs::remove_file(&raw)
                .map_err(|error| format!("Pulizia segmento audio fallita: {error}"))?;
        }
        let current = written_frames as f64 / SAMPLE_RATE as f64;
        let args = recovery_args(
            input,
            &raw,
            audio_stream,
            current,
            source_duration - current,
        );
        let observed_frames = Arc::new(AtomicU64::new(0));
        let result = run_process(
            ffmpeg,
            &args,
            cancel,
            RunOptions {
                guard_corrupt_audio: true,
                // Some FFmpeg builds exit with status 0 even when -xerror has
                // stopped an audio decoder. Such a prefix is not a real EOF.
                stop_on_decode_error: true,
                output_path: Some(&raw),
                idle_limit: Some(RECOVERY_IDLE_LIMIT),
                decoded_frames: Some(Arc::clone(&observed_frames)),
            },
        );
        let corruption = match result {
            Ok(_) => false,
            Err(error) if needs_audio_recovery(&error) => true,
            Err(error) => return Err(error),
        };
        if cancel.load(Ordering::Relaxed) {
            return Err("cancelled".to_string());
        }
        let raw_size = match fs::metadata(&raw) {
            Ok(metadata) => metadata.len(),
            Err(error) if corruption && error.kind() == std::io::ErrorKind::NotFound => 0,
            Err(error) => return Err(format!("Lettura segmento audio fallita: {error}")),
        };
        if !corruption && raw_size % BYTES_PER_FRAME != 0 {
            return Err("Recupero FFmpeg: segmento PCM incompleto".to_string());
        }
        // A killed decoder can leave a final partial frame; retain only its
        // whole frames. A successfully exited decoder must be fully aligned.
        let segment_frames = (raw_size / BYTES_PER_FRAME).min(max_frames - written_frames);
        let observed = observed_frames.load(Ordering::Relaxed);
        if segment_frames > 0 && observed == 0 {
            return Err(
                "Recupero FFmpeg: conteggio dei campioni decodificati non disponibile".to_string(),
            );
        }
        let implicit_silence = segment_frames.saturating_sub(observed);
        silent_frames = silent_frames.saturating_add(implicit_silence);
        if silent_frames > silence_budget {
            return Err(
                "Recupero FFmpeg: superato il limite di silenzio per audio danneggiato".to_string(),
            );
        }
        if segment_frames == 0 {
            empty_attempts += 1;
            if !corruption || empty_attempts >= MAX_EMPTY_ATTEMPTS {
                return Err("Recupero FFmpeg interrotto: il decoder non produce audio".to_string());
            }
        } else {
            empty_attempts = 0;
            let file = File::open(&raw)
                .map_err(|error| format!("Apertura PCM di recupero fallita: {error}"))?;
            let mut reader = BufReader::new(file);
            let mut buffer = [0_u8; 8192];
            let mut remaining = segment_frames * BYTES_PER_FRAME;
            while remaining > 0 {
                if cancel.load(Ordering::Relaxed) {
                    return Err("cancelled".to_string());
                }
                let count = remaining.min(buffer.len() as u64) as usize;
                reader
                    .read_exact(&mut buffer[..count])
                    .map_err(|error| format!("Lettura PCM di recupero fallita: {error}"))?;
                for sample in buffer[..count].as_chunks::<2>().0 {
                    writer
                        .write_sample(i16::from_le_bytes([sample[0], sample[1]]))
                        .map_err(|error| format!("Scrittura WAV di recupero fallita: {error}"))?;
                }
                remaining -= count as u64;
            }
            written_frames += segment_frames;
            decoded_frames += segment_frames.saturating_sub(implicit_silence);
        }
        if !corruption {
            break; // A real EOF, subject to the completeness check below.
        }
        let next = written_frames
            .saturating_add(SAMPLE_RATE as u64)
            .min(max_frames);
        let skipped = next - written_frames;
        resyncs += 1;
        silent_frames = silent_frames.saturating_add(skipped);
        if resyncs > MAX_RESYNCS || silent_frames > silence_budget || skipped == 0 {
            return Err(
                "Recupero FFmpeg: esaurito il limite di tentativi sui pacchetti danneggiati"
                    .to_string(),
            );
        }
        for frame in 0..skipped {
            if frame % 2048 == 0 && cancel.load(Ordering::Relaxed) {
                return Err("cancelled".to_string());
            }
            for _ in 0..CHANNELS {
                writer
                    .write_sample(0_i16)
                    .map_err(|error| format!("Scrittura silenzio di recupero fallita: {error}"))?;
            }
        }
        written_frames = next;
    }
    let actual_duration = written_frames as f64 / SAMPLE_RATE as f64;
    if decoded_frames == 0 || (source_duration > 30.0 && actual_duration < source_duration * 0.8) {
        return Err(format!(
            "Recupero FFmpeg: audio incompleto ({actual_duration:.1} s, attesi circa {source_duration:.1} s)"
        ));
    }
    writer
        .finalize()
        .map_err(|error| format!("Chiusura WAV di recupero fallita: {error}"))?;
    let check = hound::WavReader::open(&staged_wav)
        .map_err(|error| format!("Verifica WAV di recupero fallita: {error}"))?;
    if check.spec() != spec || check.duration() as u64 != written_frames {
        return Err("Recupero FFmpeg: WAV finale non valido".to_string());
    }
    drop(check);
    if cancel.load(Ordering::Relaxed) {
        return Err("cancelled".to_string());
    }
    // The temporary file shares its filesystem with the destination. On Mac
    // rename atomically replaces a failed internal WAV only after validation.
    fs::rename(&staged_wav, output)
        .map_err(|error| format!("Installazione WAV di recupero fallita: {error}"))?;
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn test_dir() -> RecoveryTempDir {
        RecoveryTempDir::create(&std::env::temp_dir().join("sonarpad-ffmpeg-test.wav")).unwrap()
    }

    fn fake_ffmpeg(dir: &Path, body: &str) -> PathBuf {
        let path = dir.join("fake_ffmpeg.py");
        fs::write(&path, format!("#!/usr/bin/env python3\nimport os, pathlib, signal, struct, sys, time\nsignal.alarm(10)\n{body}\n")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    fn fake_segment_decoder(dir: &Path, body: &str) -> PathBuf {
        fake_ffmpeg(
            dir,
            &format!(
                r#"
args = sys.argv[1:]
start = float(args[args.index('-ss') + 1])
stream = args[args.index('-map') + 1] if '-map' in args else 'auto'
calls = pathlib.Path(__file__).with_suffix('.calls')
with calls.open('a') as handle:
    handle.write(f'{{start:.9f}} {{stream}}\n')
def pcm(frames, value, observed=None, partial=False):
    pathlib.Path(args[-1]).write_bytes(struct.pack('<hh', value, -value) * frames + (b'x' if partial else b''))
    if frames:
        count = frames if observed is None else observed
        print(f'[Parsed_ashowinfo_2 @ 0x1] n:0 pts:0 pts_time:0 rate:48000 nb_samples:{{count}}', file=sys.stderr, flush=True)
def corrupt():
    print('[mp3float @ 0x1] Header missing', file=sys.stderr, flush=True)
    sys.exit(1)
{body}
"#
            ),
        )
    }

    fn samples(path: &Path) -> Vec<i16> {
        hound::WavReader::open(path)
            .unwrap()
            .into_samples::<i16>()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    }

    fn assert_only_expected_files(dir: &Path, expected: &[&str]) {
        let mut actual: Vec<_> = fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        actual.sort();
        let mut expected: Vec<_> = expected.iter().map(|name| name.to_string()).collect();
        expected.sort();
        assert_eq!(actual, expected);
    }

    #[test]
    fn stderr_is_drained_before_exit_and_retained_tail_is_bounded() {
        let dir = test_dir();
        let script = fake_ffmpeg(
            &dir.0,
            r#"
signal.alarm(3)
os.write(2, b'noise' * 500000)
os.write(2, b'\nFINAL_DIAGNOSTIC\n')
sys.exit(1)
"#,
        );
        let error = run(&script, &[], &AtomicBool::new(false), false).unwrap_err();
        assert!(error.ends_with("FINAL_DIAGNOSTIC"));
        assert!(error.len() <= STDERR_LIMIT + 100);
        assert!(!needs_audio_recovery(&error));
    }

    #[test]
    fn compressed_corruption_stops_a_running_decoder_only_when_guarded() {
        let dir = test_dir();
        let script = fake_ffmpeg(
            &dir.0,
            r#"
os.write(2, b'[mp3float @ 0x1] Header missing\nLast message repeated 130 times\n')
time.sleep(5)
"#,
        );
        let started = Instant::now();
        let error = run(&script, &[], &AtomicBool::new(false), true).unwrap_err();
        assert!(started.elapsed() < Duration::from_secs(3));
        assert!(needs_audio_recovery(&error), "{error}");
        let script = fake_ffmpeg(
            &dir.0,
            r#"
assert sys.argv[1:] == ['one argument with spaces', 'quote\'file', '-unchanged']
os.write(2, b'[mp3float @ 0x1] Header missing\nLast message repeated 130 times\n')
"#,
        );
        run(
            &script,
            &[
                "one argument with spaces".to_string(),
                "quote'file".to_string(),
                "-unchanged".to_string(),
            ],
            &AtomicBool::new(false),
            false,
        )
        .unwrap();
    }

    #[test]
    fn local_failures_and_missing_executable_never_request_audio_recovery() {
        let dir = test_dir();
        let script = fake_ffmpeg(
            &dir.0,
            r#"
os.write(2, b'[mp3float @ 0x1] Header missing\nError opening output file Header missing.wav: Permission denied\n')
sys.exit(1)
"#,
        );
        let cancel = AtomicBool::new(false);
        let error = run(&script, &[], &cancel, true).unwrap_err();
        assert!(!needs_audio_recovery(&error));
        let error = run(&dir.0.join("missing_ffmpeg"), &[], &cancel, true).unwrap_err();
        assert!(!needs_audio_recovery(&error));
        assert!(!needs_audio_recovery(
            "FFmpeg fallito: [audio_decode_recovery_required] in filename"
        ));
    }

    #[test]
    fn stop_terminates_the_child_and_preserves_existing_output() {
        let dir = test_dir();
        let script = fake_ffmpeg(
            &dir.0,
            r#"
pathlib.Path(__file__).with_suffix('.pid').write_text(str(os.getpid()))
time.sleep(8)
"#,
        );
        let output = dir.0.join("out.wav");
        fs::write(&output, b"previous complete file").unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let stop = Arc::clone(&cancel);
        let stopper = thread::spawn(move || {
            thread::sleep(Duration::from_millis(150));
            stop.store(true, Ordering::Relaxed);
        });
        let started = Instant::now();
        let error = recover_source_audio(
            &script,
            &dir.0.join("input.wav"),
            &output,
            None,
            5.0,
            &cancel,
        )
        .unwrap_err();
        stopper.join().unwrap();
        assert_eq!(error, "cancelled");
        assert!(started.elapsed() < Duration::from_secs(3));
        assert_eq!(fs::read(&output).unwrap(), b"previous complete file");
        let pid = fs::read_to_string(script.with_extension("pid")).unwrap();
        assert!(
            !Command::new("kill")
                .args(["-0", pid.trim()])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .unwrap()
                .success()
        );
        assert_only_expected_files(&dir.0, &["fake_ffmpeg.py", "fake_ffmpeg.pid", "out.wav"]);
    }

    #[test]
    fn recovery_keeps_prefix_seeks_forward_and_inserts_only_the_missing_second() {
        let dir = test_dir();
        let script = fake_segment_decoder(
            &dir.0,
            r#"
assert stream == '0:3'
if start == 0.0:
    pcm(24000, 111, partial=True)
    corrupt()
assert start == 1.5
pcm(72000, 222)
"#,
        );
        let output = dir.0.join("out.wav");
        fs::write(&output, b"previous file").unwrap();
        recover_source_audio(
            &script,
            &dir.0.join("input film.avi"),
            &output,
            Some(3),
            3.0,
            &AtomicBool::new(false),
        )
        .unwrap();
        let got = samples(&output);
        assert_eq!(got.len(), 3 * SAMPLE_RATE as usize * CHANNELS as usize);
        assert!(
            got[..48000]
                .as_chunks::<2>()
                .0
                .iter()
                .all(|frame| *frame == [111, -111])
        );
        assert!(got[48000..144000].iter().all(|sample| *sample == 0));
        assert!(
            got[144000..]
                .as_chunks::<2>()
                .0
                .iter()
                .all(|frame| *frame == [222, -222])
        );
        assert_eq!(
            fs::read_to_string(script.with_extension("calls")).unwrap(),
            "0.000000000 0:3\n1.500000000 0:3\n"
        );
        assert_only_expected_files(&dir.0, &["fake_ffmpeg.py", "fake_ffmpeg.calls", "out.wav"]);
    }

    #[test]
    fn fallback_decoder_error_is_not_mistaken_for_eof_when_exit_status_is_zero() {
        let dir = test_dir();
        let script = fake_segment_decoder(
            &dir.0,
            r#"
print("[info] Input #0, from 'Permission denied Header missing.wav':", file=sys.stderr, flush=True)
if start == 0.0:
    pcm(24000, 111)
    print('[mp3float @ 0x1] Header missing', file=sys.stderr, flush=True)
    sys.exit(0)
assert start == 1.5
pcm(72000, 222)
"#,
        );
        let output = dir.0.join("out.wav");
        recover_source_audio(
            &script,
            &dir.0.join("input.avi"),
            &output,
            None,
            3.0,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(samples(&output).len(), 288000);
        assert_eq!(
            fs::read_to_string(script.with_extension("calls")).unwrap(),
            "0.000000000 auto\n1.500000000 auto\n"
        );
    }

    #[test]
    fn primary_false_success_with_corruption_enters_recovery_and_produces_complete_audio() {
        let dir = test_dir();
        let script = fake_ffmpeg(
            &dir.0,
            r#"
args = sys.argv[1:]
if '-ss' not in args:
    import wave
    assert '-xerror' not in args
    with wave.open(args[-1], 'wb') as output:
        output.setnchannels(2)
        output.setsampwidth(2)
        output.setframerate(48000)
        output.writeframes(struct.pack('<hh', 111, -111) * 48000)
    print('[mp3float @ 0x1] Header missing', file=sys.stderr, flush=True)
    sys.exit(0)
assert args[args.index('-map') + 1] == '0:3'
start = float(args[args.index('-ss') + 1])
frames = 24000 if start == 0 else 1416000
assert start in (0.0, 1.5)
pathlib.Path(args[-1]).write_bytes(struct.pack('<hh', 222, -222) * frames)
print(f'[Parsed_ashowinfo_2 @ 0x1] [info] n:0 rate:48000 nb_samples:{frames}', file=sys.stderr, flush=True)
if start == 0:
    print('[mp3float @ 0x1] Header missing', file=sys.stderr, flush=True)
sys.exit(0)
"#,
        );
        let input = dir.0.join("input.avi");
        let output = dir.0.join("out.wav");
        let args = vec![
            "-hide_banner".to_string(),
            "-loglevel".to_string(),
            "error".to_string(),
            "-i".to_string(),
            input.to_string_lossy().to_string(),
            "-map".to_string(),
            "0:3".to_string(),
            output.to_string_lossy().to_string(),
        ];
        let cancel = AtomicBool::new(false);
        let error = run_audio_decode(&script, &args, &cancel, 31.0).unwrap_err();
        assert!(needs_audio_recovery(&error), "{error}");
        assert_eq!(hound::WavReader::open(&output).unwrap().duration(), 48000);
        recover_source_audio(&script, &input, &output, Some(3), 31.0, &cancel).unwrap();
        let got = samples(&output);
        assert_eq!(got.len(), 31 * SAMPLE_RATE as usize * CHANNELS as usize);
        assert!(got[48000..144000].iter().all(|sample| *sample == 0));
        assert!(got[144000..].iter().any(|sample| *sample != 0));
    }

    #[test]
    fn primary_duration_guard_requires_corruption_and_excludes_local_failures() {
        for (seconds, expected, diagnostic, marked, error_expected) in [
            (1, 60.0, "", false, false),
            (25, 31.0, "Header missing", false, false),
            (1, 10.0, "Header missing", false, false),
            (0, 1.0, "Header missing", true, true),
            (
                1,
                31.0,
                "Header missing\\nError opening output file: Permission denied",
                false,
                true,
            ),
        ] {
            let dir = test_dir();
            let script = fake_ffmpeg(
                &dir.0,
                &format!(
                    r#"
import wave
with wave.open(sys.argv[-1], 'wb') as output:
    output.setnchannels(2)
    output.setsampwidth(2)
    output.setframerate(48000)
    output.writeframes(struct.pack('<hh', 111, -111) * (48000 * {seconds}))
print('{diagnostic}', file=sys.stderr, flush=True)
"#
                ),
            );
            let output = dir.0.join("out.wav");
            let result = run_audio_decode(
                &script,
                &[output.to_string_lossy().to_string()],
                &AtomicBool::new(false),
                expected,
            );
            assert_eq!(result.is_err(), error_expected, "{result:?}");
            assert_eq!(
                result
                    .as_ref()
                    .err()
                    .is_some_and(|error| needs_audio_recovery(error)),
                marked,
                "{result:?}"
            );
        }
    }

    #[test]
    fn primary_corruption_guard_checks_physical_pcm_not_only_the_wav_header() {
        let dir = test_dir();
        let script = fake_ffmpeg(
            &dir.0,
            r#"
import wave
path = pathlib.Path(sys.argv[-1])
with wave.open(str(path), 'wb') as output:
    output.setnchannels(2)
    output.setsampwidth(2)
    output.setframerate(48000)
    output.writeframes(struct.pack('<hh', 111, -111) * 48000)
raw = bytearray(path.read_bytes())
raw[4:8] = (36 + 31 * 48000 * 4).to_bytes(4, 'little')
raw[40:44] = (31 * 48000 * 4).to_bytes(4, 'little')
path.write_bytes(raw)
print('[mp3float @ 0x1] Header missing', file=sys.stderr, flush=True)
"#,
        );
        let output = dir.0.join("out.wav");
        let error = run_audio_decode(
            &script,
            &[output.to_string_lossy().to_string()],
            &AtomicBool::new(false),
            31.0,
        )
        .unwrap_err();
        assert!(needs_audio_recovery(&error), "{error}");
    }

    #[test]
    fn corrupted_segments_with_some_audio_do_not_reset_the_silence_budget() {
        let dir = test_dir();
        let script = fake_segment_decoder(&dir.0, "pcm(480, 111)\ncorrupt()");
        let output = dir.0.join("out.wav");
        fs::write(&output, b"previous complete file").unwrap();
        let error = recover_source_audio(
            &script,
            &dir.0.join("input.avi"),
            &output,
            None,
            20.0,
            &AtomicBool::new(false),
        )
        .unwrap_err();
        assert!(error.contains("limite di tentativi"), "{error}");
        let calls = fs::read_to_string(script.with_extension("calls")).unwrap();
        assert_eq!(
            calls,
            "0.000000000 auto\n1.010000000 auto\n2.020000000 auto\n"
        );
        assert_eq!(fs::read(&output).unwrap(), b"previous complete file");
        assert_only_expected_files(&dir.0, &["fake_ffmpeg.py", "fake_ffmpeg.calls", "out.wav"]);
    }

    #[test]
    fn empty_repeated_decoding_stops_before_fabricating_a_soundtrack() {
        let dir = test_dir();
        let script = fake_segment_decoder(&dir.0, "pcm(0, 0)\ncorrupt()");
        let output = dir.0.join("out.wav");
        let error = recover_source_audio(
            &script,
            &dir.0.join("input.avi"),
            &output,
            None,
            600.0,
            &AtomicBool::new(false),
        )
        .unwrap_err();
        assert!(error.contains("non produce audio"), "{error}");
        assert_eq!(
            fs::read_to_string(script.with_extension("calls"))
                .unwrap()
                .lines()
                .count(),
            3
        );
        assert!(!output.exists());
        assert_only_expected_files(&dir.0, &["fake_ffmpeg.py", "fake_ffmpeg.calls"]);
    }

    #[test]
    fn incomplete_pcm_and_incomplete_duration_are_rejected_transactionally() {
        for (body, duration, expected) in [
            ("pcm(48000, 111, partial=True)", 1.0, "PCM incompleto"),
            ("pcm(48000, 111)", 60.0, "audio incompleto"),
            ("pcm(0, 0)", 1.0, "non produce audio"),
            ("pcm(192000, 0, observed=48000)", 4.0, "limite di silenzio"),
            (
                "pcm(48000, 111)\nprint('[error] Error opening output file: Permission denied', file=sys.stderr)",
                1.0,
                "FFmpeg fallito",
            ),
        ] {
            let dir = test_dir();
            let script = fake_segment_decoder(&dir.0, body);
            let output = dir.0.join("out.wav");
            fs::write(&output, b"previous complete file").unwrap();
            let error = recover_source_audio(
                &script,
                &dir.0.join("input.avi"),
                &output,
                None,
                duration,
                &AtomicBool::new(false),
            )
            .unwrap_err();
            assert!(error.contains(expected), "{error}");
            assert_eq!(fs::read(&output).unwrap(), b"previous complete file");
            assert_only_expected_files(&dir.0, &["fake_ffmpeg.py", "fake_ffmpeg.calls", "out.wav"]);
        }
    }

    fn real_ffmpeg() -> Option<PathBuf> {
        let path = std::env::var_os("SONARPAD_TEST_FFMPEG")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("ffmpeg"));
        if Command::new(&path)
            .arg("-version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
        {
            Some(path)
        } else {
            eprintln!("FFmpeg non presente: prova con decoder reale omessa.");
            None
        }
    }

    fn real_input(path: &Path, seconds: u32) -> Vec<i16> {
        let mut writer = hound::WavWriter::create(
            path,
            hound::WavSpec {
                channels: CHANNELS,
                sample_rate: SAMPLE_RATE,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            },
        )
        .unwrap();
        let mut expected = Vec::new();
        for frame in 0..SAMPLE_RATE * seconds {
            let level = ((frame % 30001) as i32 - 15000) as i16;
            for sample in [level, -level] {
                writer.write_sample(sample).unwrap();
                expected.push(sample);
            }
        }
        writer.finalize().unwrap();
        expected
    }

    #[test]
    fn real_ffmpeg_normal_path_and_healthy_recovery_preserve_the_pcm() {
        let Some(ffmpeg) = real_ffmpeg() else { return };
        let dir = test_dir();
        let input = dir.0.join("input with spaces.wav");
        let expected = real_input(&input, 3);
        let output = dir.0.join("normal.wav");
        let args: Vec<String> = [
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-fflags",
            "+discardcorrupt",
            "-err_detect",
            "ignore_err",
            "-i",
        ]
        .into_iter()
        .map(str::to_string)
        .chain([input.to_string_lossy().to_string()])
        .chain(
            ["-vn", "-ac", "2", "-ar", "48000", "-c:a", "pcm_s16le"]
                .into_iter()
                .map(str::to_string),
        )
        .chain([output.to_string_lossy().to_string()])
        .collect();
        run(&ffmpeg, &args, &AtomicBool::new(false), true).unwrap();
        assert_eq!(samples(&output), expected);
        let direct = dir.0.join("direct.wav");
        let mut direct_args = args;
        *direct_args.last_mut().unwrap() = direct.to_string_lossy().to_string();
        assert!(
            Command::new(&ffmpeg)
                .args(direct_args)
                .status()
                .unwrap()
                .success()
        );
        assert_eq!(fs::read(&output).unwrap(), fs::read(&direct).unwrap());
        let recovered = dir.0.join("recovered.wav");
        recover_source_audio(
            &ffmpeg,
            &input,
            &recovered,
            Some(0),
            3.0,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(samples(&recovered), expected);
    }

    #[test]
    fn real_ffmpeg_accurate_seek_does_not_repeat_preceding_samples() {
        let Some(ffmpeg) = real_ffmpeg() else { return };
        let dir = test_dir();
        let input = dir.0.join("input.wav");
        let expected = real_input(&input, 3);
        let raw = dir.0.join("segment.pcm");
        let args = recovery_args(&input, &raw, Some(0), 0.5, 1.5);
        let observed = Arc::new(AtomicU64::new(0));
        run_process(
            &ffmpeg,
            &args,
            &AtomicBool::new(false),
            RunOptions {
                guard_corrupt_audio: true,
                stop_on_decode_error: true,
                output_path: Some(&raw),
                idle_limit: Some(RECOVERY_IDLE_LIMIT),
                decoded_frames: Some(Arc::clone(&observed)),
            },
        )
        .unwrap();
        let got: Vec<i16> = fs::read(raw)
            .unwrap()
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        assert_eq!(got, expected[48000..192000]);
        assert!(observed.load(Ordering::Relaxed) >= 72000);
    }

    #[test]
    fn real_ffmpeg_recovers_an_avi_mp3_decoder_failure_without_publishing_only_its_prefix() {
        let Some(ffmpeg) = real_ffmpeg() else { return };
        let dir = test_dir();
        let input = dir.0.join("input.wav");
        real_input(&input, 6);
        let avi = dir.0.join("input.avi");
        let encoded = Command::new(&ffmpeg)
            .args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
            .arg(&input)
            .args(["-c:a", "libmp3lame", "-b:a", "128k"])
            .arg(&avi)
            .output()
            .unwrap();
        if !encoded.status.success()
            && String::from_utf8_lossy(&encoded.stderr).contains("Unknown encoder")
        {
            eprintln!("Encoder MP3 di prova non presente: fixture AVI omessa.");
            return;
        }
        assert!(
            encoded.status.success(),
            "{}",
            String::from_utf8_lossy(&encoded.stderr)
        );
        let mut bytes = fs::read(&avi).unwrap();
        let packets: Vec<usize> = bytes
            .windows(4)
            .enumerate()
            .filter_map(|(offset, marker)| {
                if marker != b"00wb" || offset + 8 >= bytes.len() {
                    return None;
                }
                let size =
                    u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap()) as usize;
                ((16..5000).contains(&size) && offset + 8 + size <= bytes.len()).then_some(offset)
            })
            .collect();
        assert!(packets.len() > 43);
        for offset in &packets[35..43] {
            bytes[offset + 8..offset + 24].fill(0);
        }
        let corrupt = dir.0.join("corrupt.avi");
        fs::write(&corrupt, bytes).unwrap();
        let reference = dir.0.join("reference.wav");
        recover_source_audio(
            &ffmpeg,
            &avi,
            &reference,
            Some(0),
            6.0,
            &AtomicBool::new(false),
        )
        .unwrap();
        let recovered = dir.0.join("recovered.wav");
        recover_source_audio(
            &ffmpeg,
            &corrupt,
            &recovered,
            Some(0),
            6.0,
            &AtomicBool::new(false),
        )
        .unwrap();
        let got = samples(&recovered);
        assert_eq!(got.len(), 6 * SAMPLE_RATE as usize * CHANNELS as usize);
        assert_eq!(&got[..48000], &samples(&reference)[..48000]);
        assert!(got[96000..144000].iter().all(|sample| *sample == 0));
        assert!(got[480000..].iter().any(|sample| *sample != 0));
    }

    #[test]
    fn real_ffmpeg_automatic_gap_padding_cannot_exceed_corruption_budget() {
        let Some(ffmpeg) = real_ffmpeg() else { return };
        let dir = test_dir();
        let input = dir.0.join("input.wav");
        real_input(&input, 6);
        let gapped = dir.0.join("gapped.mka");
        assert!(
            Command::new(&ffmpeg)
                .args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
                .arg(&input)
                .args(["-af", "aselect='not(between(t,1,4))'", "-c:a", "pcm_s16le"])
                .arg(&gapped)
                .status()
                .unwrap()
                .success()
        );
        let output = dir.0.join("out.wav");
        let error = recover_source_audio(
            &ffmpeg,
            &gapped,
            &output,
            Some(0),
            6.0,
            &AtomicBool::new(false),
        )
        .unwrap_err();
        assert!(error.contains("limite di silenzio"), "{error}");
        assert!(!output.exists());
    }
}

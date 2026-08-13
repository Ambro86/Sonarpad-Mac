use serde::Deserialize;
use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Duration;

const BRIDGE_FILE_NAME: &str = "faster_whisper_bridge";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BridgeModel {
    Small,
    Medium,
    LargeV3,
}

impl BridgeModel {
    pub fn as_name(self) -> &'static str {
        match self {
            Self::Small => "small",
            Self::Medium => "medium",
            Self::LargeV3 => "large-v3",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct BridgeTranscriptionResult {
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub error: String,
    #[serde(default)]
    pub backend: String,
    #[serde(default)]
    pub compute_type: String,
    #[serde(default)]
    pub language: String,
}

pub type BridgePercentCallback = Box<dyn FnMut(i32) + Send>;
pub type BridgeStageCallback = Box<dyn FnMut(&str) + Send>;

pub struct BridgeProgressCallbacks {
    pub transcription: Option<BridgePercentCallback>,
    pub stage: Option<BridgeStageCallback>,
}

fn app_bundle_resources_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let macos_dir = exe.parent()?;
    let contents_dir = macos_dir.parent()?;
    if contents_dir.file_name().and_then(|name| name.to_str()) != Some("Contents") {
        return None;
    }
    Some(contents_dir.join("Resources"))
}

fn bridge_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(resources) = app_bundle_resources_dir() {
        candidates.push(
            resources
                .join("transcription")
                .join("faster_whisper_bridge")
                .join(BRIDGE_FILE_NAME),
        );
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(root) = exe
            .parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.parent())
    {
        candidates.push(
            root.join("bridge")
                .join("dist")
                .join("faster_whisper_bridge")
                .join(BRIDGE_FILE_NAME),
        );
        candidates.push(root.join("bridge").join(BRIDGE_FILE_NAME));
    }
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(
            cwd.join("bridge")
                .join("dist")
                .join("faster_whisper_bridge")
                .join(BRIDGE_FILE_NAME),
        );
        candidates.push(cwd.join("bridge").join(BRIDGE_FILE_NAME));
    }
    candidates
}

fn ensure_bridge(cancel: &Arc<AtomicBool>) -> Result<PathBuf, String> {
    if cancel.load(Ordering::Relaxed) {
        return Err("cancelled".to_string());
    }
    for candidate in bridge_candidates() {
        if candidate.is_file() {
            crate::append_podcast_log(&format!(
                "transcription.worker path={}",
                candidate.display()
            ));
            return Ok(candidate);
        }
    }
    Err("Il worker faster-whisper non è presente nel bundle di Sonarpad. Reinstalla l'applicazione.".to_string())
}

fn model_cache_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("Library")
        .join("Application Support")
        .join("Sonarpad")
        .join("models")
        .join("faster-whisper")
}

fn terminate_bridge_process_tree(child: &mut Child) {
    let pid = child.id();
    crate::append_podcast_log(&format!("transcription.worker cancel_terminate pid={pid}"));
    let _ = Command::new("/bin/kill")
        .arg("-TERM")
        .arg(format!("-{pid}"))
        .status();
    for _ in 0..20 {
        match child.try_wait() {
            Ok(Some(_)) => return,
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(_) => break,
        }
    }
    let _ = child.kill();
    let _ = child.wait();
}

fn read_stderr(stderr: impl Read + Send + 'static) -> mpsc::Receiver<String> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stderr);
        let mut raw = Vec::new();
        let text = match reader.read_to_end(&mut raw) {
            Ok(_) => String::from_utf8_lossy(&raw).trim().to_string(),
            Err(error) => format!("read bridge stderr failed: {error}"),
        };
        let _ = tx.send(text);
    });
    rx
}

fn spawn_stdout_reader(stdout: impl Read + Send + 'static) -> mpsc::Receiver<String> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            match line {
                Ok(line) => {
                    if tx.send(line).is_err() {
                        break;
                    }
                }
                Err(error) => {
                    let _ = tx.send(format!("ERROR:stdout_read:{error}"));
                    break;
                }
            }
        }
    });
    rx
}

pub fn transcribe_media(
    input_path: &Path,
    model: BridgeModel,
    cancel: Arc<AtomicBool>,
    mut callbacks: BridgeProgressCallbacks,
) -> Result<BridgeTranscriptionResult, String> {
    let bridge_path = ensure_bridge(&cancel)?;
    let model_cache = model_cache_dir();
    fs::create_dir_all(&model_cache)
        .map_err(|error| format!("create faster-whisper model cache failed: {error}"))?;

    let work_dir = model_cache
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_else(std::env::temp_dir);

    let mut child = Command::new(&bridge_path)
        .process_group(0)
        .arg("--input")
        .arg(input_path)
        .arg("--model")
        .arg(model.as_name())
        .arg("--download-root")
        .arg(&model_cache)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("PYTHONUTF8", "1")
        .env("PYTHONIOENCODING", "utf-8")
        .current_dir(work_dir)
        .spawn()
        .map_err(|error| format!("start faster-whisper worker failed: {error}"))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "faster-whisper worker stdout unavailable".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "faster-whisper worker stderr unavailable".to_string())?;
    let line_rx = spawn_stdout_reader(stdout);
    let stderr_rx = read_stderr(stderr);
    let mut bridge_result = None;

    loop {
        if cancel.load(Ordering::Relaxed) {
            terminate_bridge_process_tree(&mut child);
            return Err("cancelled".to_string());
        }

        match line_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(line) => {
                let trimmed = line.trim();
                if let Some(stage) = trimmed.strip_prefix("STAGE:") {
                    if let Some(callback) = callbacks.stage.as_mut() {
                        callback(stage.trim());
                    }
                    continue;
                }
                if let Some(raw_pct) = trimmed.strip_prefix("PROGRESS:") {
                    if let Ok(pct) = raw_pct.trim().parse::<i32>()
                        && let Some(callback) = callbacks.transcription.as_mut()
                    {
                        callback(pct.clamp(0, 100));
                    }
                    continue;
                }
                if trimmed.starts_with('{')
                    && let Ok(parsed) = serde_json::from_str::<BridgeTranscriptionResult>(trimmed)
                {
                    bridge_result = Some(parsed);
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {}
        }

        match child.try_wait() {
            Ok(Some(status)) => {
                while let Ok(line) = line_rx.try_recv() {
                    let trimmed = line.trim();
                    if let Some(raw_pct) = trimmed.strip_prefix("PROGRESS:") {
                        if let Ok(pct) = raw_pct.trim().parse::<i32>()
                            && let Some(callback) = callbacks.transcription.as_mut()
                        {
                            callback(pct.clamp(0, 100));
                        }
                    } else if let Some(stage) = trimmed.strip_prefix("STAGE:") {
                        if let Some(callback) = callbacks.stage.as_mut() {
                            callback(stage.trim());
                        }
                    } else if trimmed.starts_with('{')
                        && let Ok(parsed) = serde_json::from_str::<BridgeTranscriptionResult>(trimmed)
                    {
                        bridge_result = Some(parsed);
                    }
                }
                if let Some(result) = bridge_result {
                    if result.ok {
                        crate::append_podcast_log(&format!(
                            "transcription.worker completed backend={} compute_type={} language={}",
                            if result.backend.is_empty() { "unknown" } else { &result.backend },
                            if result.compute_type.is_empty() { "unknown" } else { &result.compute_type },
                            if result.language.is_empty() { "unknown" } else { &result.language },
                        ));
                        if let Some(callback) = callbacks.transcription.as_mut() {
                            callback(100);
                        }
                        return Ok(result);
                    }
                    let error_text = if result.error.trim().is_empty() {
                        "faster-whisper worker returned an unknown error".to_string()
                    } else {
                        result.error
                    };
                    crate::append_podcast_log(&format!(
                        "transcription.worker failed error={}",
                        error_text.replace(['\r', '\n'], " ")
                    ));
                    return Err(error_text);
                }
                let stderr_text = stderr_rx
                    .recv_timeout(Duration::from_millis(250))
                    .unwrap_or_default();
                return Err(if stderr_text.is_empty() {
                    format!("faster-whisper worker exited with status {status}")
                } else {
                    stderr_text
                });
            }
            Ok(None) => {}
            Err(error) => {
                terminate_bridge_process_tree(&mut child);
                return Err(format!("wait faster-whisper worker failed: {error}"));
            }
        }
    }
}

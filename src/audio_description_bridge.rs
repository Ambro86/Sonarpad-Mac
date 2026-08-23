use serde::{Deserialize, Serialize};
use std::fs;
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const BRIDGE_FILE_NAME: &str = "audio_description_bridge";

#[derive(Debug, Clone, Serialize)]
pub struct AudioDescriptionPreparedChunk {
    pub path: String,
    pub start_sec: f64,
    pub end_sec: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BridgeCharacter {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AudioDescriptionBridgeRequest {
    pub input_path: String,
    pub audio_wav_path: Option<String>,
    pub duration_sec: f64,
    pub chunks: Vec<AudioDescriptionPreparedChunk>,
    pub language: String,
    pub verbosity: String,
    pub allow_extended_pauses: bool,
    pub recognize_characters: bool,
    pub initial_character_glossary: Vec<BridgeCharacter>,
    pub gemini_api_key: String,
    pub gemini_model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resume: Option<AudioDescriptionBridgeResume>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AudioDescriptionBridgeResume {
    pub completed_chunks: usize,
    pub descriptions: Vec<BridgeDescription>,
    pub character_glossary: Vec<BridgeCharacter>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BridgeInterval {
    pub start_sec: f64,
    pub end_sec: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeDescription {
    pub start_sec: f64,
    #[serde(default)]
    pub visual_start_sec: Option<f64>,
    #[serde(default)]
    pub end_sec: f64,
    pub text: String,
    #[serde(default)]
    pub mandatory: bool,
    #[serde(default)]
    pub slot_id: String,
    #[serde(default)]
    pub slot_start_sec: Option<f64>,
    #[serde(default)]
    pub slot_end_sec: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AudioDescriptionBridgeResult {
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub cancelled: bool,
    #[serde(default)]
    pub error: String,
    #[serde(default)]
    pub log_path: String,
    #[serde(default)]
    pub duration_sec: f64,
    #[serde(default)]
    pub chunk_duration_sec: u32,
    #[serde(default)]
    pub analysis_engine: String,
    #[serde(default)]
    pub protected_intervals: Vec<BridgeInterval>,
    #[serde(default)]
    pub descriptions: Vec<BridgeDescription>,
    #[serde(default)]
    pub character_glossary: Vec<BridgeCharacter>,
    #[serde(default)]
    pub gemini_model: String,
}

#[derive(Debug, Clone, Deserialize)]
struct BridgeStatus {
    #[serde(default)]
    stage: String,
    #[serde(default)]
    message: String,
}

#[derive(Debug, Clone, Deserialize)]
struct BridgeQuota {
    #[serde(default)]
    model: String,
    #[serde(default)]
    error: String,
}

#[derive(Debug, Clone, Deserialize)]
struct BridgeOverload {
    #[serde(default)]
    model: String,
    #[serde(default)]
    error: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AudioDescriptionBridgeCheckpoint {
    #[serde(default)]
    pub completed_chunks: usize,
    #[serde(default)]
    pub total_chunks: usize,
    #[serde(default)]
    pub descriptions: Vec<BridgeDescription>,
    #[serde(default)]
    pub character_glossary: Vec<BridgeCharacter>,
    #[serde(default)]
    pub gemini_model: String,
}

#[derive(Debug, Clone)]
pub enum AudioDescriptionQuotaDecision {
    SwitchModel(String),
    Wait,
    Stop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioDescriptionOverloadDecision {
    Wait,
    Stop,
}

pub type AudioDescriptionBridgePercentCallback = Box<dyn FnMut(i32) + Send>;
pub type AudioDescriptionBridgeStatusCallback = Box<dyn FnMut(&str, &str) + Send>;
pub type AudioDescriptionBridgeQuotaCallback =
    Box<dyn FnMut(&str, &str) -> AudioDescriptionQuotaDecision + Send>;
pub type AudioDescriptionBridgeOverloadCallback =
    Box<dyn FnMut(&str, &str) -> AudioDescriptionOverloadDecision + Send>;
pub type AudioDescriptionBridgeCheckpointCallback =
    Box<dyn FnMut(&AudioDescriptionBridgeCheckpoint) + Send>;

pub struct AudioDescriptionBridgeCallbacks {
    pub download: Option<AudioDescriptionBridgePercentCallback>,
    pub progress: Option<AudioDescriptionBridgePercentCallback>,
    pub status: Option<AudioDescriptionBridgeStatusCallback>,
    pub quota: Option<AudioDescriptionBridgeQuotaCallback>,
    pub overload: Option<AudioDescriptionBridgeOverloadCallback>,
    pub checkpoint: Option<AudioDescriptionBridgeCheckpointCallback>,
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
                .join("audio-description")
                .join("audio_description_bridge")
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
                .join("audio_description_bridge")
                .join(BRIDGE_FILE_NAME),
        );
        candidates.push(root.join("bridge").join(BRIDGE_FILE_NAME));
    }
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(
            cwd.join("bridge")
                .join("dist")
                .join("audio_description_bridge")
                .join(BRIDGE_FILE_NAME),
        );
        candidates.push(cwd.join("bridge").join(BRIDGE_FILE_NAME));
    }
    candidates
}

fn ensure_bridge(
    cancel: &Arc<AtomicBool>,
    _progress: &mut Option<Box<dyn FnMut(i32) + Send>>,
) -> Result<PathBuf, String> {
    if cancel.load(Ordering::Relaxed) {
        return Err("cancelled".to_string());
    }
    for candidate in bridge_candidates() {
        if candidate.is_file() {
            crate::append_podcast_log(&format!(
                "audio_description.worker path={}",
                candidate.display()
            ));
            return Ok(candidate);
        }
    }
    Err("Il worker per l'audiodescrizione IA non è presente nel bundle di Sonarpad. Reinstalla l'applicazione.".to_string())
}

fn decode_bridge_text(raw: &[u8]) -> String {
    String::from_utf8_lossy(raw).into_owned()
}

fn terminate_bridge_process_tree(child: &mut Child) {
    let pid = child.id();
    crate::append_podcast_log(&format!(
        "audio_description.worker cancel_terminate pid={pid}"
    ));
    // The worker is started in its own process group, so this also reaches any
    // PyInstaller/runtime descendants before they can be re-parented.
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

fn temporary_request_path() -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "sonarpad_audio_description_{}_{}.json",
        std::process::id(),
        stamp
    ))
}

pub fn run_audio_description_bridge(
    request: &AudioDescriptionBridgeRequest,
    cancel: Arc<AtomicBool>,
    mut callbacks: AudioDescriptionBridgeCallbacks,
) -> Result<AudioDescriptionBridgeResult, String> {
    let bridge_path = ensure_bridge(&cancel, &mut callbacks.download)?;
    if cancel.load(Ordering::Relaxed) {
        return Err("cancelled".to_string());
    }

    let request_path = temporary_request_path();
    let request_json = serde_json::to_vec(request)
        .map_err(|error| format!("serialize audio-description request failed: {error}"))?;
    fs::write(&request_path, request_json)
        .map_err(|error| format!("write audio-description request failed: {error}"))?;

    let run_result = (|| -> Result<AudioDescriptionBridgeResult, String> {
        let mut child = Command::new(&bridge_path)
            .process_group(0)
            .arg("--request")
            .arg(&request_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| format!("start audio-description worker failed: {error}"))?;

        let mut child_stdin = child
            .stdin
            .take()
            .ok_or_else(|| "audio-description worker stdin unavailable".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "audio-description worker stdout unavailable".to_string())?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| "audio-description worker stderr unavailable".to_string())?;
        let stderr_thread = std::thread::spawn(move || {
            const STDERR_TAIL_LINES: usize = 40;
            let mut reader = BufReader::new(stderr);
            let mut raw_line = Vec::new();
            let mut tail = VecDeque::<String>::with_capacity(STDERR_TAIL_LINES);
            loop {
                raw_line.clear();
                match reader.read_until(b'\n', &mut raw_line) {
                    Ok(0) => break,
                    Ok(_) => {
                        let line = decode_bridge_text(&raw_line).trim().to_string();
                        if line.is_empty() {
                            continue;
                        }
                        crate::append_podcast_log(&format!(
                            "audio_description.worker {line}"
                        ));
                        if tail.len() == STDERR_TAIL_LINES {
                            tail.pop_front();
                        }
                        tail.push_back(line);
                    }
                    Err(error) => {
                        crate::append_podcast_log(&format!(
                            "Audio description: reading worker stderr failed: {error}"
                        ));
                        break;
                    }
                }
            }
            tail.into_iter().collect::<Vec<_>>().join("\n")
        });

        let (line_tx, line_rx) = mpsc::channel::<Result<String, String>>();
        let stdout_thread = std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            let mut raw_line = Vec::new();
            loop {
                raw_line.clear();
                match reader.read_until(b'\n', &mut raw_line) {
                    Ok(0) => break,
                    Ok(_) => {
                        let line = decode_bridge_text(&raw_line).trim().to_string();
                        if line_tx.send(Ok(line)).is_err() {
                            break;
                        }
                    }
                    Err(error) => {
                        if line_tx
                            .send(Err(format!(
                                "read audio-description worker failed: {error}"
                            )))
                            .is_err()
                        {
                            crate::append_podcast_log(
                                "Audio description: worker output receiver closed while reporting a read error",
                            );
                        }
                        break;
                    }
                }
            }
        });

        let mut result: Option<AudioDescriptionBridgeResult> = None;
        loop {
            if cancel.load(Ordering::SeqCst) {
                crate::append_podcast_log("audio_description.worker cancellation_received");
                terminate_bridge_process_tree(&mut child);
                // Dropping the JoinHandles detaches the pipe-reader threads. They
                // will finish when macOS closes the killed worker's pipe handles,
                // while this job thread can notify the UI immediately.
                let _detached_stdout_thread = stdout_thread;
                let _detached_stderr_thread = stderr_thread;
                return Err("cancelled".to_string());
            }
            match line_rx.recv_timeout(Duration::from_millis(100)) {
                Ok(Ok(line)) => {
                    if let Some(raw_pct) = line.strip_prefix("PROGRESS:") {
                        if let Ok(pct) = raw_pct.trim().parse::<i32>()
                            && let Some(callback) = callbacks.progress.as_mut()
                        {
                            callback(pct.clamp(0, 100));
                        }
                    } else if let Some(raw_status) = line.strip_prefix("STATUS:") {
                        if let Ok(status) = serde_json::from_str::<BridgeStatus>(raw_status)
                            && let Some(callback) = callbacks.status.as_mut()
                        {
                            callback(&status.stage, &status.message);
                        }
                    } else if let Some(raw_checkpoint) = line.strip_prefix("CHECKPOINT:") {
                        let checkpoint = serde_json::from_str::<AudioDescriptionBridgeCheckpoint>(
                            raw_checkpoint,
                        )
                        .map_err(|error| {
                            format!(
                                "invalid checkpoint event from audio-description worker: {error}"
                            )
                        })?;
                        if let Some(callback) = callbacks.checkpoint.as_mut() {
                            callback(&checkpoint);
                        }
                    } else if let Some(raw_quota) = line.strip_prefix("QUOTA:") {
                        let quota =
                            serde_json::from_str::<BridgeQuota>(raw_quota).map_err(|error| {
                                format!(
                                    "invalid quota event from audio-description worker: {error}"
                                )
                            })?;
                        let decision = callbacks
                            .quota
                            .as_mut()
                            .map(|callback| callback(&quota.model, &quota.error))
                            .unwrap_or(AudioDescriptionQuotaDecision::Wait);
                        let reply = match decision {
                            AudioDescriptionQuotaDecision::SwitchModel(model) => {
                                serde_json::json!({"action": "switch", "model": model})
                            }
                            AudioDescriptionQuotaDecision::Wait => {
                                serde_json::json!({"action": "wait"})
                            }
                            AudioDescriptionQuotaDecision::Stop => {
                                serde_json::json!({"action": "stop"})
                            }
                        };
                        writeln!(child_stdin, "{reply}").map_err(|error| {
                            format!(
                                "write quota decision to audio-description worker failed: {error}"
                            )
                        })?;
                        child_stdin.flush().map_err(|error| {
                            format!(
                                "flush quota decision to audio-description worker failed: {error}"
                            )
                        })?;
                    } else if let Some(raw_overload) = line.strip_prefix("OVERLOAD:") {
                        let overload = serde_json::from_str::<BridgeOverload>(raw_overload)
                            .map_err(|error| {
                                format!(
                                    "invalid overload event from audio-description worker: {error}"
                                )
                            })?;
                        crate::append_podcast_log(&format!(
                            "audio_description.overload model={} error={}",
                            overload.model, overload.error
                        ));
                        let decision = callbacks
                            .overload
                            .as_mut()
                            .map(|callback| callback(&overload.model, &overload.error))
                            .unwrap_or(AudioDescriptionOverloadDecision::Wait);
                        let reply = match decision {
                            AudioDescriptionOverloadDecision::Wait => {
                                crate::append_podcast_log(
                                    "audio_description.overload decision=wait",
                                );
                                serde_json::json!({"action": "wait"})
                            }
                            AudioDescriptionOverloadDecision::Stop => {
                                crate::append_podcast_log(
                                    "audio_description.overload decision=stop",
                                );
                                serde_json::json!({"action": "stop"})
                            }
                        };
                        writeln!(child_stdin, "{reply}").map_err(|error| {
                            format!(
                                "write overload decision to audio-description worker failed: {error}"
                            )
                        })?;
                        child_stdin.flush().map_err(|error| {
                            format!(
                                "flush overload decision to audio-description worker failed: {error}"
                            )
                        })?;
                    } else if let Some(raw_result) = line.strip_prefix("RESULT:") {
                        result = serde_json::from_str(raw_result).ok();
                    } else if !line.is_empty() {
                        crate::append_podcast_log(&format!("Audio description worker: {line}"));
                    }
                }
                Ok(Err(error)) => {
                    if let Err(error) = child.kill() {
                        crate::append_podcast_log(&format!(
                            "Audio description: worker kill after stdout read error failed: {error}"
                        ));
                    }
                    if let Err(error) = child.wait() {
                        crate::append_podcast_log(&format!(
                            "Audio description: worker wait after stdout read error failed: {error}"
                        ));
                    }
                    if stdout_thread.join().is_err() {
                        crate::append_podcast_log(
                            "Audio description: stdout reader thread panicked after worker read error",
                        );
                    }
                    if stderr_thread.join().is_err() {
                        crate::append_podcast_log(
                            "Audio description: stderr reader thread panicked after worker read error",
                        );
                    }
                    return Err(error);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }

        if stdout_thread.join().is_err() {
            crate::append_podcast_log("Audio description: stdout reader thread panicked");
        }
        let status = child
            .wait()
            .map_err(|error| format!("wait audio-description worker failed: {error}"))?;
        let stderr_text = stderr_thread.join().unwrap_or_default();
        let output = result.ok_or_else(|| {
            if stderr_text.trim().is_empty() {
                format!("audio-description worker returned no result ({status})")
            } else {
                format!(
                    "audio-description worker returned no result ({status}): {}",
                    stderr_text.trim()
                )
            }
        })?;
        if output.cancelled {
            return Err("cancelled".to_string());
        }
        if !output.ok {
            let mut error = if output.error.trim().is_empty() {
                format!("audio-description analysis failed ({status})")
            } else {
                output.error.clone()
            };
            if !output.log_path.trim().is_empty() {
                error.push_str(&format!("\nLog: {}", output.log_path));
            }
            return Err(error);
        }
        if output.chunk_duration_sec != 180 {
            return Err(format!(
                "audio-description worker returned unsupported chunk duration: {}",
                output.chunk_duration_sec
            ));
        }
        if output.analysis_engine != "pyannote-segmentation-onnx" {
            return Err(format!(
                "audio-description worker returned unexpected analysis engine: {}",
                output.analysis_engine
            ));
        }
        Ok(output)
    })();

    if let Err(error) = fs::remove_file(&request_path) {
        crate::append_podcast_log(&format!(
            "Audio description: remove temporary worker request failed: {error}"
        ));
    }
    run_result
}

use crate::audio_description_bridge::{
    AudioDescriptionBridgeCallbacks, AudioDescriptionBridgeRequest, AudioDescriptionBridgeResult,
    AudioDescriptionPreparedChunk, AudioDescriptionQuotaDecision, BridgeCharacter, BridgeInterval,
    run_audio_description_bridge,
};
use crate::edge_tts::VoiceInfo;
use crate::{Settings, append_podcast_log};
use hound::{SampleFormat, WavReader, WavSpec, WavWriter};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, mpsc};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::runtime::Runtime;
use wxdragon::prelude::*;

const CHUNK_SECONDS: f64 = 180.0;
const GEMINI_MAX_CHUNK_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_SHIFT_SEC: f64 = 5.0;
const MIN_EXTENDED_ANCHOR_SEC: f64 = 1.0;
const MIX_SAMPLE_RATE: u32 = 48_000;
const MIX_CHANNELS: u16 = 2;
const DUCKING_DB: f32 = -15.0;
const FADE_MS: u32 = 150;
const BITRATE_KBPS: u32 = 192;
const PROJECT_FORMAT: &str = "sonarpad-audio-description-project";
const CATALOG_FORMAT: &str = "sonarpad-character-catalog";
const PROJECT_VERSION: u32 = 1;
const EDGE_TRAILING_MIN_REMOVE_MS: u64 = 60;
const EDGE_TRAILING_KEEP_MS: u64 = 30;
const EDGE_TRAILING_SEEK_MS: u64 = 5;
const EDGE_TRAILING_WINDOW_MS: u64 = 60;
const MAX_CHARACTER_DESCRIPTION_CHARS: usize = 2_000;
const ID_AUDIO_DESCRIPTION_START: i32 = 7100;
const ID_AUDIO_DESCRIPTION_PROGRESS_CANCEL: i32 = 7101;
const ID_AUDIO_DESCRIPTION_CLOSE: i32 = 7102;
const ID_AUDIO_DESCRIPTION_PROJECT_CLOSE: i32 = 7103;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Verbosity {
    Brief,
    Standard,
    Detailed,
}

impl Verbosity {
    fn as_bridge(self) -> &'static str {
        match self {
            Self::Brief => "short",
            Self::Standard => "standard",
            Self::Detailed => "detailed",
        }
    }

    fn from_settings(value: &str) -> Self {
        match value {
            "short" => Self::Brief,
            "standard" => Self::Standard,
            _ => Self::Detailed,
        }
    }
}

#[derive(Clone, Debug)]
struct CreateJob {
    input_path: PathBuf,
    output_path: PathBuf,
    language_code: String,
    verbosity: Verbosity,
    allow_extended_pauses: bool,
    recognize_characters: bool,
    save_project: bool,
    keep_character_catalog: bool,
    catalog: Option<CharacterCatalog>,
    tts_engine: String,
    tts_voice: String,
    rate: i32,
    pitch: i32,
    volume: i32,
    gemini_api_key: String,
    gemini_model: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct CharacterCatalogFile {
    format: String,
    version: u32,
    name: String,
    created_at_utc: String,
    updated_at_utc: String,
    #[serde(default)]
    characters: Vec<BridgeCharacter>,
}

#[derive(Clone, Debug)]
struct CharacterCatalog {
    name: String,
    path: PathBuf,
    characters: Vec<BridgeCharacter>,
}

#[derive(Clone, Debug)]
struct SynthesizedDescription {
    original_index: usize,
    text: String,
    desired_start_sec: f64,
    mandatory: bool,
    slot_id: String,
    slot_start_sec: Option<f64>,
    slot_end_sec: Option<f64>,
    pcm: Arc<[i16]>,
    duration_sec: f64,
}

#[derive(Clone, Debug)]
struct ScheduledDescription {
    original_index: usize,
    text: String,
    desired_start_sec: f64,
    mandatory: bool,
    slot_id: String,
    slot_start_sec: Option<f64>,
    slot_end_sec: Option<f64>,
    start_sec: f64,
    pcm: Arc<[i16]>,
    duration_sec: f64,
    extended_pause: bool,
}

#[derive(Clone, Debug)]
struct DroppedDescription {
    text: String,
    desired_start_sec: f64,
    mandatory: bool,
    slot_id: String,
    duration_sec: f64,
    reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectInterval {
    pub start_sec: f64,
    pub end_sec: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectDescription {
    pub id: usize,
    pub text: String,
    pub original_text: String,
    #[serde(default)]
    pub rendered_text: String,
    pub modified: bool,
    pub gemini_start_sec: f64,
    #[serde(default)]
    pub mandatory: bool,
    #[serde(default)]
    pub slot_id: String,
    #[serde(default)]
    pub slot_start_sec: Option<f64>,
    #[serde(default)]
    pub slot_end_sec: Option<f64>,
    pub source_start_sec: f64,
    pub output_start_sec: f64,
    pub output_end_sec: f64,
    pub tts_duration_sec: f64,
    pub extended_pause: bool,
    pub extended_pause_duration_sec: f64,
    pub duck_start_sec: Option<f64>,
    pub duck_end_sec: Option<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectExcluded {
    pub id: usize,
    pub text: String,
    pub gemini_start_sec: f64,
    #[serde(default)]
    pub mandatory: bool,
    #[serde(default)]
    pub slot_id: String,
    pub tts_duration_sec: f64,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AudioDescriptionProject {
    pub format: String,
    pub version: u32,
    pub created_at_utc: String,
    pub updated_at_utc: String,
    pub source_path: PathBuf,
    pub output_mp3_path: PathBuf,
    pub source_duration_sec: f64,
    pub output_duration_sec: f64,
    pub language: String,
    pub language_code: String,
    pub verbosity: String,
    pub allow_extended_pauses: bool,
    #[serde(default = "default_true")]
    pub recognize_characters: bool,
    pub gemini_model: String,
    pub tts_engine: String,
    pub tts_voice: String,
    pub tts_rate: i32,
    pub tts_pitch: i32,
    pub tts_volume: i32,
    pub bitrate_kbps: u32,
    pub ducking_db: f32,
    pub fade_ms: u32,
    pub protected_intervals: Vec<ProjectInterval>,
    pub descriptions: Vec<ProjectDescription>,
    pub excluded_descriptions: Vec<ProjectExcluded>,
}

fn default_true() -> bool {
    true
}

#[derive(Clone, Debug)]
struct JobOutcome {
    output_path: PathBuf,
    project_path: Option<PathBuf>,
    catalog_path: Option<PathBuf>,
    generated: usize,
    inserted: usize,
    extended: usize,
    dropped: usize,
    dropped_mandatory: usize,
}

#[derive(Clone, Default)]
struct ProgressState {
    progress: i32,
    status: String,
    done: Option<Result<JobOutcome, String>>,
    quota: Option<QuotaUiRequest>,
}

#[derive(Clone)]
struct QuotaUiRequest {
    model: String,
    error: String,
    sender: mpsc::SyncSender<AudioDescriptionQuotaDecision>,
}

fn tr_map() -> &'static HashMap<String, String> {
    static IT: OnceLock<HashMap<String, String>> = OnceLock::new();
    static EN: OnceLock<HashMap<String, String>> = OnceLock::new();
    static FR: OnceLock<HashMap<String, String>> = OnceLock::new();
    static ES: OnceLock<HashMap<String, String>> = OnceLock::new();
    static PT: OnceLock<HashMap<String, String>> = OnceLock::new();
    static CS: OnceLock<HashMap<String, String>> = OnceLock::new();
    static PL: OnceLock<HashMap<String, String>> = OnceLock::new();
    let lang = Settings::load().ui_language;
    let (slot, raw) = match lang.as_str() {
        "en" => (&EN, include_str!("../i18n/audio_description_en.json")),
        "fr" => (&FR, include_str!("../i18n/audio_description_fr.json")),
        "es" => (&ES, include_str!("../i18n/audio_description_es.json")),
        "pt" => (&PT, include_str!("../i18n/audio_description_pt.json")),
        "cs" => (&CS, include_str!("../i18n/audio_description_cs.json")),
        "pl" => (&PL, include_str!("../i18n/audio_description_pl.json")),
        _ => (&IT, include_str!("../i18n/audio_description_it.json")),
    };
    slot.get_or_init(|| serde_json::from_str(raw).unwrap_or_default())
}

fn tr(key: &str) -> String {
    tr_map()
        .get(key)
        .cloned()
        .unwrap_or_else(|| key.to_string())
}

fn trf(key: &str, values: &[(&str, String)]) -> String {
    let mut text = tr(key);
    for (name, value) in values {
        text = text.replace(&format!("{{{name}}}"), value);
    }
    text
}

pub fn menu_label() -> String {
    tr("audio_description.title")
}
pub fn save_folder_label() -> String {
    tr("audio_description.save_folder")
}

fn storage_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("Library")
        .join("Application Support")
        .join("Sonarpad")
}

fn default_output_dir() -> PathBuf {
    let configured = Settings::load().audio_description_save_folder;
    let base = if configured.trim().is_empty() {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("Documents")
            .join("Sonarpad")
            .join("Audiodescriptions")
    } else {
        PathBuf::from(configured)
    };
    let _ = fs::create_dir_all(&base);
    base
}

fn catalog_dir() -> PathBuf {
    let dir = default_output_dir().join("Catalogs");
    let _ = fs::create_dir_all(&dir);
    dir
}

fn cache_dir(prefix: &str) -> Result<PathBuf, String> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let dir = storage_dir().join("audio_description_cache").join(format!(
        "{prefix}_{}_{}",
        std::process::id(),
        stamp
    ));
    fs::create_dir_all(&dir)
        .map_err(|e| format!("Impossibile creare la cache audiodescrizione: {e}"))?;
    Ok(dir)
}

fn now_utc() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn sanitize_filename(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        if ch.is_alphanumeric() || matches!(ch, ' ' | '-' | '_' | '.') {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    let trimmed = out.trim().trim_matches('.').trim();
    if trimmed.is_empty() {
        "audiodescrizione".to_string()
    } else {
        trimmed.to_string()
    }
}

fn suggested_catalog_name(input: &str) -> String {
    Path::new(input)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(str::trim)
        .filter(|stem| !stem.is_empty())
        .unwrap_or_default()
        .to_string()
}

fn catalog_path_for_name(name: &str) -> PathBuf {
    catalog_dir().join(format!(
        "{}_character_catalog.json",
        sanitize_filename(name).replace(' ', "_").to_lowercase()
    ))
}

fn list_catalogs() -> Vec<CharacterCatalog> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(catalog_dir()) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let Ok(raw) = fs::read(&path) else {
            continue;
        };
        let Ok(file) = serde_json::from_slice::<CharacterCatalogFile>(&raw) else {
            continue;
        };
        if file.format != CATALOG_FORMAT {
            continue;
        }
        out.push(CharacterCatalog {
            name: file.name,
            path,
            characters: file.characters,
        });
    }
    out.sort_by_key(|item| item.name.to_lowercase());
    out
}

fn normalized_catalog_character(character: &BridgeCharacter) -> Option<BridgeCharacter> {
    let name = character
        .name
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let description = character
        .description
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if name.is_empty() || description.is_empty() {
        return None;
    }
    Some(BridgeCharacter {
        id: character.id.trim().to_string(),
        name,
        description,
    })
}

fn catalog_name_tokens(name: &str) -> Vec<String> {
    name.split_whitespace()
        .map(|token| {
            token
                .chars()
                .filter(|character| character.is_alphanumeric())
                .flat_map(char::to_lowercase)
                .collect::<String>()
        })
        .filter(|token| !token.is_empty())
        .collect()
}

fn find_catalog_identity(
    characters: &[BridgeCharacter],
    candidate: &BridgeCharacter,
) -> Option<usize> {
    let candidate_id = candidate.id.trim().to_lowercase();
    if !candidate_id.is_empty() {
        let mut matches = characters
            .iter()
            .enumerate()
            .filter(|(_, character)| character.id.trim().eq_ignore_ascii_case(&candidate.id))
            .map(|(index, _)| index);
        if let Some(first) = matches.next()
            && matches.next().is_none()
        {
            return Some(first);
        }
    }

    let mut name_matches = characters
        .iter()
        .enumerate()
        .filter(|(_, character)| character.name.trim().eq_ignore_ascii_case(&candidate.name))
        .map(|(index, _)| index);
    if let Some(first) = name_matches.next()
        && name_matches.next().is_none()
    {
        return Some(first);
    }

    let candidate_tokens = catalog_name_tokens(&candidate.name);
    if candidate_id.is_empty() || candidate_tokens.len() != 1 || candidate_tokens[0].len() < 3 {
        return None;
    }
    let candidate_token = &candidate_tokens[0];
    let id_prefix = format!("{candidate_id}_");
    let alias_matches = characters
        .iter()
        .enumerate()
        .filter(|(_, character)| {
            character.id.to_lowercase().starts_with(&id_prefix)
                && catalog_name_tokens(&character.name).contains(candidate_token)
        })
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    match alias_matches.as_slice() {
        [index] => Some(*index),
        _ => None,
    }
}

fn catalog_description_tokens(text: &str) -> Vec<String> {
    text.split(|character: char| !character.is_alphanumeric())
        .map(|token| token.to_lowercase())
        .filter(|token| !token.is_empty())
        .fold(Vec::<String>::new(), |mut tokens, token| {
            if !tokens.contains(&token) {
                tokens.push(token);
            }
            tokens
        })
}

fn catalog_description_coverage(candidate: &str, established: &str) -> f32 {
    let candidate_tokens = catalog_description_tokens(candidate);
    if candidate_tokens.is_empty() {
        return 1.0;
    }
    let established_tokens = catalog_description_tokens(established);
    if established_tokens.is_empty() {
        return 0.0;
    }
    let shared = candidate_tokens
        .iter()
        .filter(|token| established_tokens.contains(token))
        .count();
    shared as f32 / candidate_tokens.len() as f32
}

fn catalog_description_sentences(text: &str) -> Vec<String> {
    text.split_inclusive(['.', '!', '?'])
        .map(str::trim)
        .filter(|sentence| !sentence.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn merge_catalog_description(existing: &str, observed: &str) -> String {
    let existing = existing.trim();
    let observed = observed.trim();
    if existing.is_empty() {
        return observed.to_string();
    }
    if observed.is_empty() {
        return existing.to_string();
    }

    let mut merged = existing.to_string();
    for sentence in catalog_description_sentences(observed) {
        let words = catalog_description_tokens(&sentence);
        if words.len() <= 2 {
            continue;
        }
        if catalog_description_coverage(&sentence, &merged) >= 0.65 {
            continue;
        }
        let separator = if matches!(merged.chars().last(), Some('.' | '!' | '?')) {
            " "
        } else {
            ". "
        };
        let candidate = format!("{merged}{separator}{sentence}");
        if candidate.chars().count() > MAX_CHARACTER_DESCRIPTION_CHARS {
            break;
        }
        merged = candidate;
    }
    merged
}

fn merge_catalog(
    existing: &[BridgeCharacter],
    observed: &[BridgeCharacter],
) -> Vec<BridgeCharacter> {
    let mut merged = Vec::<BridgeCharacter>::new();
    for character in existing {
        let Some(candidate) = normalized_catalog_character(character) else {
            continue;
        };
        if let Some(index) = find_catalog_identity(&merged, &candidate) {
            merged[index].description =
                merge_catalog_description(&merged[index].description, &candidate.description);
            if merged[index].id.is_empty() && !candidate.id.is_empty() {
                merged[index].id = candidate.id;
            }
        } else {
            merged.push(candidate);
        }
    }

    let authoritative_count = merged.len();
    for character in observed {
        let Some(candidate) = normalized_catalog_character(character) else {
            continue;
        };
        if let Some(index) = find_catalog_identity(&merged, &candidate) {
            merged[index].description =
                merge_catalog_description(&merged[index].description, &candidate.description);
            if index >= authoritative_count
                && merged[index].id.is_empty()
                && !candidate.id.is_empty()
            {
                merged[index].id = candidate.id;
            }
        } else {
            merged.push(candidate);
        }
    }
    merged
}

fn save_catalog(
    catalog: &CharacterCatalog,
    observed: &[BridgeCharacter],
) -> Result<PathBuf, String> {
    let merged = merge_catalog(&catalog.characters, observed);
    let created = fs::read(&catalog.path)
        .ok()
        .and_then(|raw| serde_json::from_slice::<CharacterCatalogFile>(&raw).ok())
        .map(|f| f.created_at_utc)
        .unwrap_or_else(now_utc);
    let file = CharacterCatalogFile {
        format: CATALOG_FORMAT.to_string(),
        version: 1,
        name: catalog.name.clone(),
        created_at_utc: created,
        updated_at_utc: now_utc(),
        characters: merged,
    };
    if let Some(parent) = catalog.path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(
        &catalog.path,
        serde_json::to_vec_pretty(&file).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("Salvataggio catalogo personaggi fallito: {e}"))?;
    Ok(catalog.path.clone())
}

fn ffmpeg_path() -> PathBuf {
    crate::ffmpeg_executable_path().unwrap_or_else(|| PathBuf::from("ffmpeg"))
}

fn run_ffmpeg(args: &[String], cancel: &Arc<AtomicBool>) -> Result<(), String> {
    if cancel.load(Ordering::Relaxed) {
        return Err("cancelled".to_string());
    }
    let mut child = std::process::Command::new(ffmpeg_path())
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Avvio FFmpeg fallito: {e}"))?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| "FFmpeg stderr non disponibile".to_string())?;
    loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("cancelled".to_string());
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut raw = Vec::new();
                let _ = stderr.read_to_end(&mut raw);
                if status.success() {
                    return Ok(());
                }
                return Err(format!(
                    "FFmpeg fallito: {}",
                    String::from_utf8_lossy(&raw).trim()
                ));
            }
            Ok(None) => thread::sleep(Duration::from_millis(100)),
            Err(e) => return Err(format!("Controllo FFmpeg fallito: {e}")),
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct MediaProbe {
    duration_sec: f64,
    has_audio: bool,
}

fn parse_ffmpeg_duration(value: &str) -> Option<f64> {
    let mut parts = value.trim().split(':');
    let hours = parts.next()?.parse::<f64>().ok()?;
    let minutes = parts.next()?.parse::<f64>().ok()?;
    let seconds = parts.next()?.parse::<f64>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    let total = hours * 3600.0 + minutes * 60.0 + seconds;
    (total.is_finite() && total > 0.0).then_some(total)
}

fn probe_media(input: &Path) -> Result<MediaProbe, String> {
    let output = std::process::Command::new(ffmpeg_path())
        .arg("-hide_banner")
        .arg("-i")
        .arg(input)
        .output()
        .map_err(|e| format!("Analisi del file multimediale fallita: {e}"))?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    let duration_sec = stderr
        .lines()
        .find_map(|line| {
            let marker = "Duration:";
            let pos = line.find(marker)?;
            let tail = line[pos + marker.len()..].trim_start();
            let value = tail.split(',').next()?.trim();
            if value == "N/A" {
                None
            } else {
                parse_ffmpeg_duration(value)
            }
        })
        .ok_or_else(|| "FFmpeg non ha restituito la durata del video.".to_string())?;
    let has_audio = stderr.lines().any(|line| line.contains(" Audio:"));
    Ok(MediaProbe {
        duration_sec,
        has_audio,
    })
}

fn write_silent_source_wav(path: &Path, duration_sec: f64) -> Result<(), String> {
    let spec = WavSpec {
        channels: MIX_CHANNELS,
        sample_rate: MIX_SAMPLE_RATE,
        bits_per_sample: 16,
        sample_format: SampleFormat::Int,
    };
    let mut writer = WavWriter::create(path, spec).map_err(|e| e.to_string())?;
    let frames = (duration_sec * MIX_SAMPLE_RATE as f64).ceil() as u64;
    for _ in 0..frames {
        for _ in 0..MIX_CHANNELS {
            writer.write_sample(0i16).map_err(|e| e.to_string())?;
        }
    }
    writer.finalize().map_err(|e| e.to_string())
}

fn decode_source_audio(
    input: &Path,
    wav: &Path,
    cancel: &Arc<AtomicBool>,
) -> Result<MediaProbe, String> {
    let probe = probe_media(input)?;
    if !probe.has_audio {
        append_podcast_log("audio_description.source has_audio=false; using silent source track");
        write_silent_source_wav(wav, probe.duration_sec)?;
        return Ok(probe);
    }
    let args = vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-y".into(),
        "-i".into(),
        input.to_string_lossy().to_string(),
        "-vn".into(),
        "-ac".into(),
        MIX_CHANNELS.to_string(),
        "-ar".into(),
        MIX_SAMPLE_RATE.to_string(),
        "-c:a".into(),
        "pcm_s16le".into(),
        wav.to_string_lossy().to_string(),
    ];
    run_ffmpeg(&args, cancel)?;
    let reader =
        WavReader::open(wav).map_err(|e| format!("Audio sorgente WAV non leggibile: {e}"))?;
    let spec = reader.spec();
    let decoded_duration = reader.duration() as f64 / spec.sample_rate.max(1) as f64;
    if (decoded_duration - probe.duration_sec).abs() > 2.0 {
        append_podcast_log(&format!(
            "audio_description.source duration_probe={:.3} decoded_audio={:.3}",
            probe.duration_sec, decoded_duration
        ));
    }
    Ok(probe)
}

fn create_pyannote_wav(
    source_wav: &Path,
    target: &Path,
    cancel: &Arc<AtomicBool>,
) -> Result<(), String> {
    let args = vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-y".into(),
        "-i".into(),
        source_wav.to_string_lossy().to_string(),
        "-ac".into(),
        "1".into(),
        "-ar".into(),
        "16000".into(),
        "-c:a".into(),
        "pcm_s16le".into(),
        target.to_string_lossy().to_string(),
    ];
    run_ffmpeg(&args, cancel)
}

fn prepare_chunks(
    input: &Path,
    duration: f64,
    dir: &Path,
    cancel: &Arc<AtomicBool>,
) -> Result<Vec<AudioDescriptionPreparedChunk>, String> {
    if duration <= CHUNK_SECONDS {
        let size = fs::metadata(input)
            .map_err(|e| format!("Lettura dimensione file fallita: {e}"))?
            .len();
        if size == 0 || size >= GEMINI_MAX_CHUNK_BYTES {
            return Err(format!(
                "Il file Gemini ha una dimensione non supportata: {}",
                input.display()
            ));
        }
        return Ok(vec![AudioDescriptionPreparedChunk {
            path: input.to_string_lossy().to_string(),
            start_sec: 0.0,
            end_sec: duration,
        }]);
    }

    if cancel.load(Ordering::Relaxed) {
        return Err("cancelled".to_string());
    }
    let extension = "mkv";
    let prefix = "gemini_chunk_";
    let output_pattern = dir.join(format!("{prefix}%04d.{extension}"));
    let segment_format = "matroska";
    let args = vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-y".into(),
        "-i".into(),
        input.to_string_lossy().to_string(),
        "-map".into(),
        "0:v:0".into(),
        "-map".into(),
        "0:a?".into(),
        "-c".into(),
        "copy".into(),
        "-f".into(),
        "segment".into(),
        "-segment_time".into(),
        format!("{CHUNK_SECONDS:.0}"),
        "-segment_start_number".into(),
        "1".into(),
        "-reset_timestamps".into(),
        "1".into(),
        "-segment_format".into(),
        segment_format.into(),
        output_pattern.to_string_lossy().to_string(),
    ];
    run_ffmpeg(&args, cancel)?;

    let suffix = format!(".{extension}");
    let mut paths = fs::read_dir(dir)
        .map_err(|e| format!("Lettura cartella chunk fallita: {e}"))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(prefix) && name.ends_with(suffix.as_str()))
        })
        .collect::<Vec<_>>();
    paths.sort();
    if paths.is_empty() {
        return Err("FFmpeg non ha creato segmenti Gemini.".to_string());
    }

    let path_count = paths.len();
    let mut chunks = Vec::with_capacity(path_count);
    let mut cursor = 0.0_f64;
    for (index, path) in paths.into_iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return Err("cancelled".to_string());
        }
        let size = fs::metadata(&path).map_err(|e| e.to_string())?.len();
        if size == 0 || size >= GEMINI_MAX_CHUNK_BYTES {
            return Err(format!(
                "Il chunk Gemini ha una dimensione non supportata: {}",
                path.display()
            ));
        }
        let measured = probe_media(&path)
            .map(|probe| probe.duration_sec)
            .unwrap_or(CHUNK_SECONDS)
            .max(0.001);
        let start_sec = cursor;
        let end_sec = if index + 1 == path_count {
            duration
        } else {
            (start_sec + measured).min(duration)
        };
        if end_sec <= start_sec {
            return Err("Timeline dei chunk Gemini non valida.".to_string());
        }
        append_podcast_log(&format!(
            "audio_description.chunk index={} start={:.3} end={:.3} measured={:.3} size_mb={:.1} format={}",
            index + 1,
            start_sec,
            end_sec,
            measured,
            size as f64 / (1024.0 * 1024.0),
            extension
        ));
        chunks.push(AudioDescriptionPreparedChunk {
            path: path.to_string_lossy().to_string(),
            start_sec,
            end_sec,
        });
        if index + 1 < path_count {
            cursor = end_sec;
        }
    }
    Ok(chunks)
}

fn convert_mp3_bytes_to_pcm(
    bytes: &[u8],
    dir: &Path,
    index: usize,
    cancel: &Arc<AtomicBool>,
) -> Result<Vec<i16>, String> {
    let mp3 = dir.join(format!("tts_{index:04}.mp3"));
    let wav = dir.join(format!("tts_{index:04}.wav"));
    fs::write(&mp3, bytes).map_err(|e| format!("Scrittura TTS temporaneo fallita: {e}"))?;
    let args = vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-y".into(),
        "-i".into(),
        mp3.to_string_lossy().to_string(),
        "-ac".into(),
        MIX_CHANNELS.to_string(),
        "-ar".into(),
        MIX_SAMPLE_RATE.to_string(),
        "-c:a".into(),
        "pcm_s16le".into(),
        wav.to_string_lossy().to_string(),
    ];
    run_ffmpeg(&args, cancel)?;
    let reader = WavReader::open(&wav).map_err(|e| format!("TTS WAV non leggibile: {e}"))?;
    let samples: Result<Vec<i16>, _> = reader.into_samples::<i16>().collect();
    let samples = samples.map_err(|e| format!("Lettura TTS WAV fallita: {e}"))?;
    let _ = fs::remove_file(mp3);
    let _ = fs::remove_file(wav);
    if samples.is_empty() {
        return Err("La voce ha prodotto audio vuoto.".to_string());
    }
    Ok(samples)
}

fn pcm_rms_dbfs(samples: &[i16]) -> f32 {
    if samples.is_empty() {
        return -120.0;
    }
    let sum = samples
        .iter()
        .map(|sample| {
            let value = *sample as f64 / 32768.0;
            value * value
        })
        .sum::<f64>();
    let rms = (sum / samples.len() as f64).sqrt();
    if rms <= 1.0e-9 {
        -120.0
    } else {
        (20.0 * rms.log10()) as f32
    }
}

fn trim_edge_trailing_silence(samples: &mut Vec<i16>) -> usize {
    if samples.is_empty() {
        return 0;
    }
    let channels = MIX_CHANNELS.max(1) as usize;
    let frames = samples.len() / channels;
    let minimum_input_frames = ((MIX_SAMPLE_RATE as u64 * 100) / 1000).max(1) as usize;
    if frames < minimum_input_frames {
        return 0;
    }

    let threshold_db = (-55.0_f32).max(pcm_rms_dbfs(samples) - 35.0);
    let seek_frames = ((MIX_SAMPLE_RATE as u64 * EDGE_TRAILING_SEEK_MS) / 1000).max(1) as usize;
    let window_frames = ((MIX_SAMPLE_RATE as u64 * EDGE_TRAILING_WINDOW_MS) / 1000).max(1) as usize;
    if frames < window_frames {
        return 0;
    }
    let keep_frames = ((MIX_SAMPLE_RATE as u64 * EDGE_TRAILING_KEEP_MS) / 1000) as usize;
    let minimum_remove_frames =
        ((MIX_SAMPLE_RATE as u64 * EDGE_TRAILING_MIN_REMOVE_MS) / 1000).max(1) as usize;

    let last_slice_start = frames.saturating_sub(window_frames);
    let mut slice_starts: Vec<usize> = (0..=last_slice_start).step_by(seek_frames).collect();
    if slice_starts.last().copied() != Some(last_slice_start) {
        slice_starts.push(last_slice_start);
    }
    let mut silent_starts = Vec::new();
    for start in slice_starts {
        let end = start.saturating_add(window_frames).min(frames);
        let start_sample = start.saturating_mul(channels);
        let end_sample = end.saturating_mul(channels).min(samples.len());
        if pcm_rms_dbfs(&samples[start_sample..end_sample]) <= threshold_db {
            silent_starts.push(start);
        }
    }
    let Some(mut previous) = silent_starts.first().copied() else {
        return 0;
    };
    let mut current_start = previous;
    let mut silent_ranges = Vec::new();
    for start in silent_starts.into_iter().skip(1) {
        let continuous = start == previous.saturating_add(seek_frames);
        let has_gap = start > previous.saturating_add(window_frames);
        if !continuous && has_gap {
            silent_ranges.push((
                current_start,
                previous.saturating_add(window_frames).min(frames),
            ));
            current_start = start;
        }
        previous = start;
    }
    silent_ranges.push((
        current_start,
        previous.saturating_add(window_frames).min(frames),
    ));

    if silent_ranges.len() == 1 && silent_ranges[0] == (0, frames) {
        return 0;
    }
    let mut previous_silence_end = 0_usize;
    let mut last_nonsilent_end = None;
    for (silence_start, silence_end) in &silent_ranges {
        if *silence_start > previous_silence_end {
            last_nonsilent_end = Some(*silence_start);
        }
        previous_silence_end = previous_silence_end.max(*silence_end);
    }
    if previous_silence_end < frames {
        last_nonsilent_end = Some(frames);
    }
    let Some(last_active_frame) = last_nonsilent_end else {
        return 0;
    };
    let keep_until = last_active_frame.saturating_add(keep_frames).min(frames);
    let removable_frames = frames.saturating_sub(keep_until);
    if removable_frames < minimum_remove_frames {
        return 0;
    }
    let old_len = samples.len();
    samples.truncate(keep_until.saturating_mul(channels));
    old_len.saturating_sub(samples.len())
}

#[derive(Clone, Copy)]
struct TtsParameters<'a> {
    engine: &'a str,
    voice: &'a str,
    rate: i32,
    pitch: i32,
    volume: i32,
}

fn synthesize_text_pcm(
    text: &str,
    tts: TtsParameters<'_>,
    rt: &Runtime,
    dir: &Path,
    index: usize,
    cancel: &Arc<AtomicBool>,
) -> Result<Arc<[i16]>, String> {
    if cancel.load(Ordering::Relaxed) {
        return Err("cancelled".to_string());
    }
    let mp3 = crate::synthesize_voice_chunk_blocking(
        tts.engine, text, tts.voice, tts.rate, tts.pitch, tts.volume, rt,
    )?;
    let mut pcm = convert_mp3_bytes_to_pcm(&mp3, dir, index, cancel)?;
    if !crate::is_system_voice_engine(tts.engine) {
        let removed = trim_edge_trailing_silence(&mut pcm);
        if removed > 0 {
            append_podcast_log(&format!(
                "audio_description.edge_trim cue={} removed_samples={}",
                index, removed
            ));
        }
    }
    if pcm.is_empty() {
        return Err("La voce ha prodotto audio vuoto.".to_string());
    }
    Ok(Arc::from(pcm))
}

fn normalize_intervals(intervals: &[BridgeInterval], duration: f64) -> Vec<(f64, f64)> {
    let mut values: Vec<_> = intervals
        .iter()
        .filter_map(|i| {
            let s = i.start_sec.max(0.0).min(duration);
            let e = i.end_sec.max(s).min(duration);
            (e > s).then_some((s, e))
        })
        .collect();
    values.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut merged: Vec<(f64, f64)> = Vec::new();
    for (s, e) in values {
        if let Some(last) = merged.last_mut()
            && s <= last.1
        {
            last.1 = last.1.max(e);
            continue;
        }
        merged.push((s, e));
    }
    merged
}

fn free_intervals(protected: &[(f64, f64)], duration: f64) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    let mut cursor = 0.0;
    for &(s, e) in protected {
        if s > cursor {
            out.push((cursor, s));
        }
        cursor = cursor.max(e);
    }
    if cursor < duration {
        out.push((cursor, duration));
    }
    out
}

fn subtract_reserved(free: &[(f64, f64)], reserved: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut blocks = reserved.to_vec();
    blocks.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut out = Vec::new();
    for &(fs_, fe) in free {
        let mut cur = fs_;
        for &(rs, re) in &blocks {
            if re <= cur || rs >= fe {
                continue;
            }
            if rs > cur {
                out.push((cur, rs.min(fe)));
            }
            cur = cur.max(re);
            if cur >= fe {
                break;
            }
        }
        if cur < fe {
            out.push((cur, fe));
        }
    }
    out
}

fn choose_slot(free: &[(f64, f64)], desired: f64, required: f64) -> Option<f64> {
    free.iter()
        .filter_map(|&(s, e)| {
            let upper = e - required;
            if upper < s {
                return None;
            }
            let x = desired.clamp(s, upper);
            let d = (x - desired).abs();
            (d <= MAX_SHIFT_SEC).then_some((d, x))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|x| x.1)
}

fn restrict_slot(free: &[(f64, f64)], d: &SynthesizedDescription) -> Vec<(f64, f64)> {
    let (Some(ss), Some(se)) = (d.slot_start_sec, d.slot_end_sec) else {
        return free.to_vec();
    };
    free.iter()
        .filter_map(|&(s, e)| {
            let a = s.max(ss);
            let b = e.min(se);
            (b > a).then_some((a, b))
        })
        .collect()
}

fn schedule_descriptions(
    items: &[SynthesizedDescription],
    protected: &[BridgeInterval],
    duration: f64,
    allow_extended: bool,
) -> (Vec<ScheduledDescription>, Vec<DroppedDescription>) {
    let free = free_intervals(&normalize_intervals(protected, duration), duration);
    let mut mandatory: Vec<_> = items.iter().filter(|x| x.mandatory).cloned().collect();
    let mut optional: Vec<_> = items.iter().filter(|x| !x.mandatory).cloned().collect();
    mandatory.sort_by(|a, b| a.desired_start_sec.total_cmp(&b.desired_start_sec));
    optional.sort_by(|a, b| a.desired_start_sec.total_cmp(&b.desired_start_sec));
    mandatory.extend(optional);
    let mut scheduled = Vec::new();
    let mut dropped = Vec::new();
    let mut reserved = Vec::new();
    for d in mandatory {
        let available = subtract_reserved(&free, &reserved);
        let candidates = if d.mandatory {
            restrict_slot(&available, &d)
        } else {
            available
        };
        if let Some(start) =
            choose_slot(&candidates, d.desired_start_sec, d.duration_sec.max(0.001))
        {
            reserved.push((start, start + d.duration_sec));
            scheduled.push(ScheduledDescription {
                original_index: d.original_index,
                text: d.text,
                desired_start_sec: d.desired_start_sec,
                mandatory: d.mandatory,
                slot_id: d.slot_id,
                slot_start_sec: d.slot_start_sec,
                slot_end_sec: d.slot_end_sec,
                start_sec: start,
                pcm: d.pcm,
                duration_sec: d.duration_sec,
                extended_pause: false,
            });
            continue;
        }
        if allow_extended {
            let anchor = candidates
                .iter()
                .filter_map(|&(s, e)| {
                    if e - s < MIN_EXTENDED_ANCHOR_SEC {
                        return None;
                    }
                    let x = d.desired_start_sec.clamp(s, e - MIN_EXTENDED_ANCHOR_SEC);
                    let dist = (x - d.desired_start_sec).abs();
                    (dist <= MAX_SHIFT_SEC).then_some((dist, x))
                })
                .min_by(|a, b| a.0.total_cmp(&b.0))
                .map(|x| x.1);
            if let Some(start) = anchor {
                reserved.push((start, start + MIN_EXTENDED_ANCHOR_SEC));
                scheduled.push(ScheduledDescription {
                    original_index: d.original_index,
                    text: d.text,
                    desired_start_sec: d.desired_start_sec,
                    mandatory: d.mandatory,
                    slot_id: d.slot_id,
                    slot_start_sec: d.slot_start_sec,
                    slot_end_sec: d.slot_end_sec,
                    start_sec: start,
                    pcm: d.pcm,
                    duration_sec: d.duration_sec,
                    extended_pause: true,
                });
                continue;
            }
        }
        dropped.push(DroppedDescription {
            text: d.text,
            desired_start_sec: d.desired_start_sec,
            mandatory: d.mandatory,
            slot_id: d.slot_id,
            duration_sec: d.duration_sec,
            reason: "no dialogue-free slot long enough after exact TTS duration check".to_string(),
        });
    }
    scheduled.sort_by(|a, b| a.start_sec.total_cmp(&b.start_sec));
    (scheduled, dropped)
}

fn mix_sample(source: i16, narration: i16, duck_gain: f32) -> i16 {
    let value = source as f32 * duck_gain + narration as f32;
    value.round().clamp(i16::MIN as f32, i16::MAX as f32) as i16
}

fn duck_gain_for(frame: u64, start: u64, end: u64, fade_frames: u64) -> f32 {
    let duck = 10f32.powf(DUCKING_DB / 20.0);
    if fade_frames == 0 {
        return if frame >= start && frame < end {
            duck
        } else {
            1.0
        };
    }
    if frame + fade_frames >= start && frame < start {
        let x = (frame + fade_frames - start) as f32 / fade_frames as f32;
        return 1.0 - (1.0 - duck) * x.clamp(0.0, 1.0);
    }
    if frame >= start && frame < end {
        return duck;
    }
    if frame >= end && frame < end + fade_frames {
        let x = (frame - end) as f32 / fade_frames as f32;
        return duck + (1.0 - duck) * x.clamp(0.0, 1.0);
    }
    1.0
}

fn render_mix(
    source_wav: &Path,
    output_wav: &Path,
    scheduled: &[ScheduledDescription],
    cancel: &Arc<AtomicBool>,
) -> Result<f64, String> {
    let mut source =
        WavReader::open(source_wav).map_err(|e| format!("Apertura audio sorgente fallita: {e}"))?;
    let spec = source.spec();
    if spec.sample_rate != MIX_SAMPLE_RATE
        || spec.channels != MIX_CHANNELS
        || spec.bits_per_sample != 16
    {
        return Err("Formato WAV interno inatteso.".to_string());
    }
    let out_spec = WavSpec {
        channels: MIX_CHANNELS,
        sample_rate: MIX_SAMPLE_RATE,
        bits_per_sample: 16,
        sample_format: SampleFormat::Int,
    };
    let mut writer = WavWriter::create(output_wav, out_spec).map_err(|e| e.to_string())?;
    let total_source_frames = source.duration() as u64;
    let mut samples = source.samples::<i16>();
    let fade_frames = (MIX_SAMPLE_RATE as u64 * FADE_MS as u64) / 1000;
    let mut source_frame = 0u64;
    let mut cue_index = 0usize;
    let mut output_frames = 0u64;
    while source_frame < total_source_frames {
        if cancel.load(Ordering::Relaxed) {
            return Err("cancelled".to_string());
        }
        while cue_index < scheduled.len()
            && scheduled[cue_index].extended_pause
            && (scheduled[cue_index].start_sec * MIX_SAMPLE_RATE as f64).round() as u64
                <= source_frame
        {
            let cue = &scheduled[cue_index];
            for &sample in cue.pcm.iter() {
                writer.write_sample(sample).map_err(|e| e.to_string())?;
            }
            output_frames += cue.pcm.len() as u64 / MIX_CHANNELS as u64;
            cue_index += 1;
        }
        let left = samples
            .next()
            .transpose()
            .map_err(|e| e.to_string())?
            .unwrap_or(0);
        let right = samples
            .next()
            .transpose()
            .map_err(|e| e.to_string())?
            .unwrap_or(0);
        let mut narration = [0i16; 2];
        let mut gain = 1.0f32;
        if let Some(cue) = scheduled.get(cue_index).filter(|c| !c.extended_pause) {
            let start = (cue.start_sec * MIX_SAMPLE_RATE as f64).round() as u64;
            let cue_frames = cue.pcm.len() as u64 / MIX_CHANNELS as u64;
            let end = start + cue_frames;
            gain = duck_gain_for(source_frame, start, end, fade_frames);
            if source_frame >= start && source_frame < end {
                let off = ((source_frame - start) * MIX_CHANNELS as u64) as usize;
                narration[0] = *cue.pcm.get(off).unwrap_or(&0);
                narration[1] = *cue.pcm.get(off + 1).unwrap_or(&0);
            }
            if source_frame + 1 >= end {
                cue_index += 1;
            }
        }
        writer
            .write_sample(mix_sample(left, narration[0], gain))
            .map_err(|e| e.to_string())?;
        writer
            .write_sample(mix_sample(right, narration[1], gain))
            .map_err(|e| e.to_string())?;
        source_frame += 1;
        output_frames += 1;
    }
    while cue_index < scheduled.len() {
        let cue = &scheduled[cue_index];
        if cue.extended_pause {
            for &sample in cue.pcm.iter() {
                writer.write_sample(sample).map_err(|e| e.to_string())?;
            }
            output_frames += cue.pcm.len() as u64 / MIX_CHANNELS as u64;
        }
        cue_index += 1;
    }
    writer.finalize().map_err(|e| e.to_string())?;
    Ok(output_frames as f64 / MIX_SAMPLE_RATE as f64)
}

fn encode_mp3(wav: &Path, output: &Path, cancel: &Arc<AtomicBool>) -> Result<(), String> {
    let args = vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-y".into(),
        "-i".into(),
        wav.to_string_lossy().to_string(),
        "-c:a".into(),
        "libmp3lame".into(),
        "-b:a".into(),
        format!("{BITRATE_KBPS}k"),
        output.to_string_lossy().to_string(),
    ];
    run_ffmpeg(&args, cancel)
}

fn project_path(output: &Path) -> PathBuf {
    let mut p = output.to_path_buf();
    p.set_extension("sonarpad-ad.json");
    p
}

fn build_project(
    job: &CreateJob,
    analysis: &AudioDescriptionBridgeResult,
    scheduled: &[ScheduledDescription],
    dropped: &[DroppedDescription],
    output_duration: f64,
) -> AudioDescriptionProject {
    let mut extra_offset = 0.0;
    let mut descriptions = Vec::new();
    for (id, d) in scheduled.iter().enumerate() {
        let output_start = d.start_sec + extra_offset;
        let ext = if d.extended_pause {
            d.duration_sec
        } else {
            0.0
        };
        let output_end = output_start + d.duration_sec;
        descriptions.push(ProjectDescription {
            id,
            text: d.text.clone(),
            original_text: d.text.clone(),
            rendered_text: d.text.clone(),
            modified: false,
            gemini_start_sec: d.desired_start_sec,
            mandatory: d.mandatory,
            slot_id: d.slot_id.clone(),
            slot_start_sec: d.slot_start_sec,
            slot_end_sec: d.slot_end_sec,
            source_start_sec: d.start_sec,
            output_start_sec: output_start,
            output_end_sec: output_end,
            tts_duration_sec: d.duration_sec,
            extended_pause: d.extended_pause,
            extended_pause_duration_sec: ext,
            duck_start_sec: (!d.extended_pause).then_some(output_start),
            duck_end_sec: (!d.extended_pause).then_some(output_end),
        });
        extra_offset += ext;
    }
    let excluded = dropped
        .iter()
        .enumerate()
        .map(|(id, d)| ProjectExcluded {
            id,
            text: d.text.clone(),
            gemini_start_sec: d.desired_start_sec,
            mandatory: d.mandatory,
            slot_id: d.slot_id.clone(),
            tts_duration_sec: d.duration_sec,
            reason: d.reason.clone(),
        })
        .collect();
    AudioDescriptionProject {
        format: PROJECT_FORMAT.into(),
        version: PROJECT_VERSION,
        created_at_utc: now_utc(),
        updated_at_utc: now_utc(),
        source_path: job.input_path.clone(),
        output_mp3_path: job.output_path.clone(),
        source_duration_sec: analysis.duration_sec,
        output_duration_sec: output_duration,
        language: job.language_code.clone(),
        language_code: job.language_code.clone(),
        verbosity: job.verbosity.as_bridge().into(),
        allow_extended_pauses: job.allow_extended_pauses,
        recognize_characters: job.recognize_characters,
        gemini_model: analysis.gemini_model.clone(),
        tts_engine: job.tts_engine.clone(),
        tts_voice: job.tts_voice.clone(),
        tts_rate: job.rate,
        tts_pitch: job.pitch,
        tts_volume: job.volume,
        bitrate_kbps: BITRATE_KBPS,
        ducking_db: DUCKING_DB,
        fade_ms: FADE_MS,
        protected_intervals: analysis
            .protected_intervals
            .iter()
            .map(|x| ProjectInterval {
                start_sec: x.start_sec,
                end_sec: x.end_sec,
            })
            .collect(),
        descriptions,
        excluded_descriptions: excluded,
    }
}

fn save_project(path: &Path, project: &AudioDescriptionProject) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(
        path,
        serde_json::to_vec_pretty(project).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("Salvataggio progetto fallito: {e}"))
}

fn fetch_gemini_models(api_key: &str) -> Result<Vec<String>, String> {
    let key = api_key.trim();
    if key.is_empty() {
        return Err(tr("audio_description.error.api_key"));
    }
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models?key={}",
        url::form_urlencoded::byte_serialize(key.as_bytes()).collect::<String>()
    );
    let response = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?
        .get(url)
        .send()
        .map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }
    let root: Value = response.json().map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    if let Some(models) = root.get("models").and_then(Value::as_array) {
        for model in models {
            let methods = model
                .get("supportedGenerationMethods")
                .and_then(Value::as_array);
            if !methods.is_some_and(|m| m.iter().any(|x| x.as_str() == Some("generateContent"))) {
                continue;
            }
            if let Some(name) = model.get("name").and_then(Value::as_str) {
                out.push(name.trim_start_matches("models/").to_string());
            }
        }
    }
    out.sort();
    out.dedup();
    if out.is_empty() {
        Err("Nessun modello Gemini compatibile trovato.".into())
    } else {
        Ok(out)
    }
}

fn bridge_progress_status(stage: &str, message: &str) -> String {
    match stage {
        "pyannote_analyzing" => tr("audio_description.progress.pyannote_analyzing"),
        "pyannote_no_audio" => tr("audio_description.progress.pyannote_no_audio"),
        "pyannote_done" => trf(
            "audio_description.progress.pyannote_done",
            &[("count", message.to_string())],
        ),
        "gemini_start" => tr("audio_description.progress.gemini_start"),
        "gemini_uploading" => tr("audio_description.progress.gemini_uploading"),
        "gemini_waiting" => tr("audio_description.progress.gemini_waiting"),
        "gemini_contacting" => tr("audio_description.progress.gemini_contacting"),
        "gemini_processing" => tr("audio_description.progress.gemini_processing"),
        "gemini_response" => tr("audio_description.progress.gemini_response"),
        "gemini_repair" => tr("audio_description.progress.gemini_repair"),
        "gemini_retry" => tr("audio_description.progress.gemini_retry"),
        "language_correction" => tr("audio_description.progress.language_correction"),
        "finalize" => tr("audio_description.progress.finalize"),
        "ready_for_tts" => tr("audio_description.progress.ready_for_tts"),
        "gemini_chunk" => {
            let details = serde_json::from_str::<Value>(message).ok();
            let current = details
                .as_ref()
                .and_then(|value| value.get("current"))
                .and_then(Value::as_u64)
                .unwrap_or(1);
            let total = details
                .as_ref()
                .and_then(|value| value.get("total"))
                .and_then(Value::as_u64)
                .unwrap_or(current);
            trf(
                "audio_description.progress.gemini_chunk",
                &[
                    ("current", current.to_string()),
                    ("total", total.to_string()),
                ],
            )
        }
        _ if !message.trim().is_empty() => message.to_string(),
        _ => tr("audio_description.status.running"),
    }
}

fn create_audio_description(
    job: &CreateJob,
    rt: &Runtime,
    cancel: Arc<AtomicBool>,
    state: Arc<Mutex<ProgressState>>,
) -> Result<JobOutcome, String> {
    if !job.input_path.is_file() {
        return Err(tr("audio_description.error.input"));
    }
    if job.output_path.as_os_str().is_empty() {
        return Err(tr("audio_description.error.output"));
    }
    let input_cmp = fs::canonicalize(&job.input_path).unwrap_or_else(|_| job.input_path.clone());
    let output_cmp = fs::canonicalize(&job.output_path).unwrap_or_else(|_| job.output_path.clone());
    if input_cmp == output_cmp {
        return Err(tr("audio_description.error.same_path"));
    }
    if job.gemini_api_key.trim().is_empty() {
        return Err(tr("audio_description.error.api_key"));
    }
    if job.tts_voice.trim().is_empty() {
        return Err(tr("audio_description.error.voice"));
    }
    if let Some(parent) = job.output_path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let work = cache_dir("job")?;
    let result = (|| {
        {
            let mut s = state.lock().unwrap();
            s.progress = 2;
            s.status = tr("audio_description.progress.analysis_prepare");
        }
        let source_wav = work.join("source.wav");
        let probe = decode_source_audio(&job.input_path, &source_wav, &cancel)?;
        let duration = probe.duration_sec;
        let pyannote = work.join("pyannote.wav");
        let audio_wav_path = if probe.has_audio {
            create_pyannote_wav(&source_wav, &pyannote, &cancel)?;
            Some(pyannote.to_string_lossy().to_string())
        } else {
            None
        };
        {
            let mut s = state.lock().unwrap();
            s.progress = 7;
            s.status = tr("audio_description.progress.chunk_prepare");
        }
        let chunks = prepare_chunks(&job.input_path, duration, &work, &cancel)?;
        let request = AudioDescriptionBridgeRequest {
            input_path: job.input_path.to_string_lossy().to_string(),
            audio_wav_path,
            duration_sec: duration,
            chunks,
            language: job.language_code.clone(),
            verbosity: job.verbosity.as_bridge().into(),
            allow_extended_pauses: job.allow_extended_pauses,
            recognize_characters: job.recognize_characters,
            initial_character_glossary: job
                .catalog
                .as_ref()
                .map(|c| c.characters.clone())
                .unwrap_or_default(),
            gemini_api_key: job.gemini_api_key.clone(),
            gemini_model: job.gemini_model.clone(),
        };
        let status_state = state.clone();
        let progress_state = state.clone();
        let quota_state = state.clone();
        let analysis = run_audio_description_bridge(
            &request,
            cancel.clone(),
            AudioDescriptionBridgeCallbacks {
                download: None,
                progress: Some(Box::new(move |pct| {
                    let mut s = progress_state.lock().unwrap();
                    s.progress = 10 + (pct.clamp(0, 100) * 45 / 100);
                })),
                status: Some(Box::new(move |stage, message| {
                    status_state.lock().unwrap().status = bridge_progress_status(stage, message);
                })),
                quota: Some(Box::new(move |model, error| {
                    let (tx, rx) = mpsc::sync_channel(1);
                    quota_state.lock().unwrap().quota = Some(QuotaUiRequest {
                        model: model.to_string(),
                        error: error.to_string(),
                        sender: tx,
                    });
                    rx.recv().unwrap_or(AudioDescriptionQuotaDecision::Stop)
                })),
            },
        )?;
        if analysis.descriptions.is_empty() {
            return Err("Gemini non ha restituito descrizioni.".to_string());
        }
        {
            let mut s = state.lock().unwrap();
            s.progress = 55;
            s.status = tr("audio_description.progress.tts");
        }
        let mut synthesized = Vec::with_capacity(analysis.descriptions.len());
        for (i, d) in analysis.descriptions.iter().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                return Err("cancelled".into());
            }
            let pcm = synthesize_text_pcm(
                &d.text,
                TtsParameters {
                    engine: &job.tts_engine,
                    voice: &job.tts_voice,
                    rate: job.rate,
                    pitch: job.pitch,
                    volume: job.volume,
                },
                rt,
                &work,
                i,
                &cancel,
            )?;
            let duration_sec = pcm.len() as f64 / (MIX_CHANNELS as f64 * MIX_SAMPLE_RATE as f64);
            synthesized.push(SynthesizedDescription {
                original_index: i,
                text: d.text.clone(),
                desired_start_sec: d.start_sec,
                mandatory: d.mandatory,
                slot_id: d.slot_id.clone(),
                slot_start_sec: d.slot_start_sec,
                slot_end_sec: d.slot_end_sec,
                pcm,
                duration_sec,
            });
            let mut s = state.lock().unwrap();
            s.progress = 55 + (((i + 1) as i32 * 25) / (analysis.descriptions.len().max(1) as i32));
        }
        {
            let mut s = state.lock().unwrap();
            s.status = tr("audio_description.progress.schedule");
            s.progress = 80;
        }
        let (scheduled, dropped) = schedule_descriptions(
            &synthesized,
            &analysis.protected_intervals,
            analysis.duration_sec,
            job.allow_extended_pauses,
        );
        if scheduled.is_empty() {
            return Err("Nessuna descrizione può essere inserita in sicurezza.".into());
        }
        let mix_wav = work.join("mix.wav");
        let output_duration = render_mix(&source_wav, &mix_wav, &scheduled, &cancel)?;
        {
            let mut s = state.lock().unwrap();
            s.status = tr("audio_description.progress.export");
            s.progress = 90;
        }
        let temp_output = work.join("final.mp3");
        encode_mp3(&mix_wav, &temp_output, &cancel)?;
        fs::copy(&temp_output, &job.output_path)
            .map_err(|e| format!("Salvataggio MP3 fallito: {e}"))?;
        let project_file = if job.save_project {
            let p = project_path(&job.output_path);
            let project = build_project(job, &analysis, &scheduled, &dropped, output_duration);
            save_project(&p, &project)?;
            Some(p)
        } else {
            None
        };
        let catalog_path = if job.keep_character_catalog {
            if let Some(c) = job.catalog.as_ref() {
                Some(save_catalog(c, &analysis.character_glossary)?)
            } else {
                None
            }
        } else {
            None
        };
        let extended = scheduled.iter().filter(|x| x.extended_pause).count();
        let dropped_mandatory = dropped.iter().filter(|x| x.mandatory).count();
        Ok(JobOutcome {
            output_path: job.output_path.clone(),
            project_path: project_file,
            catalog_path,
            generated: analysis.descriptions.len(),
            inserted: scheduled.len(),
            extended,
            dropped: dropped.len(),
            dropped_mandatory,
        })
    })();
    let _ = fs::remove_dir_all(&work);
    result
}

fn show_error(parent: &Dialog, message: &str) {
    let d = MessageDialog::builder(parent, message, &tr("audio_description.title"))
        .with_style(MessageDialogStyle::OK | MessageDialogStyle::IconError)
        .build();
    d.show_modal();
}

fn show_project_error(parent: &Dialog, message: &str) {
    let d = MessageDialog::builder(
        parent,
        message,
        &tr("audio_description.project.title"),
    )
    .with_style(MessageDialogStyle::OK | MessageDialogStyle::IconError)
    .build();
    d.show_modal();
}

fn show_project_edit_success(parent: &Dialog) {
    let d = MessageDialog::builder(
        parent,
        &tr("audio_description.project.edit_saved"),
        &tr("audio_description.project.edit_success_title"),
    )
    .with_style(MessageDialogStyle::OK | MessageDialogStyle::IconInformation)
    .build();
    d.show_modal();
}

fn show_completion(parent: &Dialog, message: &str) {
    let completion_dialog = Dialog::builder(parent, &tr("audio_description.status.complete"))
        .with_style(DialogStyle::DefaultDialogStyle | DialogStyle::ResizeBorder)
        .with_size(680, 360)
        .build();
    let completion_panel = Panel::builder(&completion_dialog).build();
    let completion_root = BoxSizer::builder(Orientation::Vertical).build();
    let completion_details = TextCtrl::builder(&completion_panel)
        .with_style(TextCtrlStyle::MultiLine | TextCtrlStyle::ReadOnly)
        .build();
    completion_details.set_value(message);
    completion_root.add(
        &completion_details,
        1,
        SizerFlag::Expand | SizerFlag::All,
        12,
    );
    let completion_buttons = BoxSizer::builder(Orientation::Horizontal).build();
    let completion_ok = Button::builder(&completion_panel)
        .with_id(ID_OK)
        .with_label("OK")
        .build();
    completion_buttons.add_spacer(1);
    completion_buttons.add(&completion_ok, 0, SizerFlag::All, 10);
    completion_root.add_sizer(&completion_buttons, 0, SizerFlag::Expand, 0);
    completion_panel.set_sizer(completion_root, true);
    completion_dialog.set_affirmative_id(ID_OK);
    let dialog_ok = completion_dialog;
    completion_ok.on_click(move |_| dialog_ok.end_modal(ID_OK));
    completion_ok.set_focus();
    completion_dialog.show_modal();
    completion_dialog.destroy();
}

fn choose_input(parent: &Dialog) -> Option<PathBuf> {
    let d = FileDialog::builder(parent)
        .with_message(&tr("audio_description.open_title"))
        .with_wildcard("Video|*.mp4;*.mkv;*.mov;*.m4v;*.avi;*.webm;*.mpeg;*.mpg|Tutti|*.*")
        .with_style(FileDialogStyle::Open | FileDialogStyle::FileMustExist)
        .build();
    if d.show_modal() == ID_OK {
        d.get_path().map(PathBuf::from)
    } else {
        None
    }
}
fn choose_output(parent: &Dialog, input: &Path) -> Option<PathBuf> {
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("video");
    let d = FileDialog::builder(parent)
        .with_message(&tr("audio_description.save_title"))
        .with_default_file(&format!("{}_audiodescritto.mp3", sanitize_filename(stem)))
        .with_wildcard("MP3|*.mp3")
        .with_style(FileDialogStyle::Save | FileDialogStyle::OverwritePrompt)
        .build();
    if d.show_modal() == ID_OK {
        d.get_path().map(PathBuf::from)
    } else {
        None
    }
}

fn quota_dialog(
    parent: &dyn WxWidget,
    model: &str,
    error: &str,
    api_key: &str,
) -> AudioDescriptionQuotaDecision {
    let d = Dialog::builder(parent, &tr("audio_description.quota.title"))
        .with_size(620, 300)
        .build();
    let p = Panel::builder(&d).build();
    let root = BoxSizer::builder(Orientation::Vertical).build();
    let msg = trf(
        "audio_description.quota.message",
        &[("model", model.to_string()), ("error", error.to_string())],
    );
    root.add(
        &StaticText::builder(&p).with_label(&msg).build(),
        0,
        SizerFlag::Expand | SizerFlag::All,
        8,
    );
    let choice = Choice::builder(&p).build();
    let models = fetch_gemini_models(api_key)
        .unwrap_or_default()
        .into_iter()
        .filter(|m| m != model)
        .collect::<Vec<_>>();
    for m in &models {
        choice.append(m);
    }
    if !models.is_empty() {
        choice.set_selection(0);
    }
    root.add(&choice, 0, SizerFlag::Expand | SizerFlag::All, 8);
    let row = BoxSizer::builder(Orientation::Horizontal).build();
    let sw = Button::builder(&p)
        .with_id(7001)
        .with_label(&tr("audio_description.quota.model_prompt"))
        .build();
    let wait = Button::builder(&p)
        .with_id(7002)
        .with_label(&tr("audio_description.quota.wait"))
        .build();
    let stop = Button::builder(&p)
        .with_id(ID_CANCEL)
        .with_label(&tr("audio_description.cancel"))
        .build();
    row.add(&sw, 0, SizerFlag::All, 5);
    row.add(&wait, 0, SizerFlag::All, 5);
    row.add(&stop, 0, SizerFlag::All, 5);
    root.add_sizer(&row, 0, SizerFlag::Expand, 0);
    p.set_sizer(root, true);
    let d1 = d;
    sw.on_click(move |_| d1.end_modal(7001));
    let d2 = d;
    wait.on_click(move |_| d2.end_modal(7002));
    let d3 = d;
    stop.on_click(move |_| d3.end_modal(ID_CANCEL));
    let r = d.show_modal();
    let result = if r == 7001 {
        models
            .get(choice.get_selection().unwrap_or(0) as usize)
            .cloned()
            .map(AudioDescriptionQuotaDecision::SwitchModel)
            .unwrap_or(AudioDescriptionQuotaDecision::Wait)
    } else if r == 7002 {
        AudioDescriptionQuotaDecision::Wait
    } else {
        AudioDescriptionQuotaDecision::Stop
    };
    d.destroy();
    result
}

fn run_with_progress(
    parent: &Frame,
    job: CreateJob,
    rt: Arc<Runtime>,
) -> Result<JobOutcome, String> {
    let progress_dialog = Dialog::builder(parent, &tr("audio_description.title"))
        .with_style(
            DialogStyle::Caption
                | DialogStyle::SystemMenu
                | DialogStyle::CloseBox
                | DialogStyle::StayOnTop,
        )
        .with_size(520, 180)
        .build();
    let progress_panel = Panel::builder(&progress_dialog).build();
    let progress_root = BoxSizer::builder(Orientation::Vertical).build();
    let progress_label = StaticText::builder(&progress_panel)
        .with_label(&tr("audio_description.status.running"))
        .build();
    progress_root.add(
        &progress_label,
        0,
        SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Top,
        12,
    );
    let progress_gauge = Gauge::builder(&progress_panel).with_range(100).build();
    progress_root.add(
        &progress_gauge,
        0,
        SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Top,
        12,
    );
    let progress_buttons = BoxSizer::builder(Orientation::Horizontal).build();
    let progress_cancel = Button::builder(&progress_panel)
        .with_id(ID_AUDIO_DESCRIPTION_PROGRESS_CANCEL)
        .with_label(&tr("audio_description.cancel"))
        .build();
    progress_buttons.add_spacer(1);
    progress_buttons.add(&progress_cancel, 0, SizerFlag::All, 10);
    progress_root.add_sizer(
        &progress_buttons,
        0,
        SizerFlag::Expand | SizerFlag::Bottom,
        0,
    );
    progress_panel.set_sizer(progress_root, true);

    let state = Arc::new(Mutex::new(ProgressState {
        progress: 0,
        status: tr("audio_description.status.running"),
        done: None,
        quota: None,
    }));
    let cancel = Arc::new(AtomicBool::new(false));
    let st = state.clone();
    let c = cancel.clone();
    let job_thread = job.clone();
    thread::spawn(move || {
        let r = create_audio_description(&job_thread, &rt, c, st.clone());
        st.lock().unwrap().done = Some(r);
    });

    let result = Rc::new(RefCell::new(None::<Result<JobOutcome, String>>));
    let finished = Rc::new(Cell::new(false));
    let cancel_pending = Rc::new(Cell::new(false));
    let cancel_button = cancel.clone();
    let cancel_pending_button = cancel_pending.clone();
    let progress_label_button = progress_label;
    progress_cancel.on_click(move |_| {
        if !cancel_pending_button.replace(true) {
            append_podcast_log("audio_description.progress.cancel_requested_button");
            cancel_button.store(true, Ordering::SeqCst);
            progress_cancel.enable(false);
            progress_label_button.set_label(&tr("audio_description.status.canceling"));
        }
    });
    let cancel_close = cancel.clone();
    let cancel_pending_close = cancel_pending.clone();
    let finished_close = finished.clone();
    let progress_label_close = progress_label;
    progress_dialog.on_close(move |event| {
        if finished_close.get() {
            event.skip(true);
            return;
        }
        if !cancel_pending_close.replace(true) {
            append_podcast_log("audio_description.progress.cancel_requested_close");
            cancel_close.store(true, Ordering::SeqCst);
            progress_cancel.enable(false);
            progress_label_close.set_label(&tr("audio_description.status.canceling"));
        }
        event.skip(false);
    });

    let timer = Rc::new(Timer::new(&progress_dialog));
    let timer_tick = timer.clone();
    let timer_handle = timer.clone();
    let state_tick = state.clone();
    let result_tick = result.clone();
    let finished_tick = finished.clone();
    let cancel_pending_tick = cancel_pending.clone();
    let dialog_tick = progress_dialog;
    let label_tick = progress_label;
    let gauge_tick = progress_gauge;
    let api_key = job.gemini_api_key.clone();
    timer_tick.on_tick(move |_| {
        let quota = { state_tick.lock().unwrap().quota.take() };
        if let Some(q) = quota {
            let decision = quota_dialog(&dialog_tick, &q.model, &q.error, &api_key);
            let _ = q.sender.send(decision);
        }
        let snap = state_tick.lock().unwrap().clone();
        if !cancel_pending_tick.get() {
            label_tick.set_label(&snap.status);
        }
        gauge_tick.set_value(snap.progress.clamp(0, 99));
        if let Some(done) = snap.done {
            if cancel_pending_tick.get() {
                append_podcast_log("audio_description.progress.cancel_completed");
            }
            timer_handle.stop();
            gauge_tick.set_value(100);
            *result_tick.borrow_mut() = Some(done);
            finished_tick.set(true);
            dialog_tick.end_modal(ID_OK);
        }
    });
    timer.start(100, false);
    progress_dialog.show_modal();
    timer.stop();
    progress_dialog.destroy();
    result
        .borrow_mut()
        .take()
        .unwrap_or_else(|| Err("cancelled".into()))
}

const AUDIO_DESCRIPTION_LANGUAGES: &[(&str, &str)] = &[
    ("audio_description.language_name.it", "it"),
    ("audio_description.language_name.en", "en"),
    ("audio_description.language_name.de", "de"),
    ("audio_description.language_name.es", "es"),
    ("audio_description.language_name.fr", "fr"),
    ("audio_description.language_name.pt", "pt"),
    ("audio_description.language_name.pt-BR", "pt-BR"),
    ("audio_description.language_name.cs", "cs"),
    ("audio_description.language_name.pl", "pl"),
    ("audio_description.language_name.ru", "ru"),
    ("audio_description.language_name.uk", "uk"),
    ("audio_description.language_name.sv", "sv"),
    ("audio_description.language_name.vi", "vi"),
    ("audio_description.language_name.zh", "zh"),
    ("audio_description.language_name.hi", "hi"),
];

fn language_choices() -> Vec<(String, &'static str)> {
    AUDIO_DESCRIPTION_LANGUAGES
        .iter()
        .map(|(translation_key, code)| (tr(translation_key), *code))
        .collect()
}

fn voice_matches_language(voice: &VoiceInfo, language: &str) -> bool {
    let wanted = language
        .split('-')
        .next()
        .unwrap_or(language)
        .to_ascii_lowercase();
    voice
        .locale
        .split('-')
        .next()
        .unwrap_or(&voice.locale)
        .eq_ignore_ascii_case(&wanted)
}

pub fn open_create_dialog(
    parent: &Frame,
    settings: &Arc<Mutex<Settings>>,
    rt: &Arc<Runtime>,
    voices_data: &Arc<Mutex<Vec<VoiceInfo>>>,
) {
    let saved = settings.lock().unwrap().clone();
    let d = Dialog::builder(parent, &tr("audio_description.title"))
        .with_style(DialogStyle::DefaultDialogStyle | DialogStyle::ResizeBorder)
        .with_size(760, 680)
        .build();
    let p = Panel::builder(&d).build();
    let root = BoxSizer::builder(Orientation::Vertical).build();
    let input = TextCtrl::builder(&p).build();
    let input_row = BoxSizer::builder(Orientation::Horizontal).build();
    input_row.add(
        &StaticText::builder(&p)
            .with_label(&tr("audio_description.input"))
            .build(),
        0,
        SizerFlag::AlignCenterVertical | SizerFlag::All,
        5,
    );
    input_row.add(&input, 1, SizerFlag::Expand | SizerFlag::All, 5);
    let input_btn = Button::builder(&p)
        .with_label(&tr("audio_description.browse_input"))
        .build();
    input_row.add(&input_btn, 0, SizerFlag::All, 5);
    root.add_sizer(&input_row, 0, SizerFlag::Expand, 0);
    let output = TextCtrl::builder(&p).build();
    let output_row = BoxSizer::builder(Orientation::Horizontal).build();
    output_row.add(
        &StaticText::builder(&p)
            .with_label(&tr("audio_description.output"))
            .build(),
        0,
        SizerFlag::AlignCenterVertical | SizerFlag::All,
        5,
    );
    output_row.add(&output, 1, SizerFlag::Expand | SizerFlag::All, 5);
    let output_btn = Button::builder(&p)
        .with_label(&tr("audio_description.browse_output"))
        .build();
    output_row.add(&output_btn, 0, SizerFlag::All, 5);
    root.add_sizer(&output_row, 0, SizerFlag::Expand, 0);
    let language = Choice::builder(&p).build();
    let langs = language_choices();
    for (name, _) in &langs {
        language.append(name);
    }
    let lang_index = langs
        .iter()
        .position(|(_, code)| *code == saved.audio_description_language)
        .unwrap_or(0);
    language.set_selection(lang_index as u32);
    let row = BoxSizer::builder(Orientation::Horizontal).build();
    row.add(
        &StaticText::builder(&p)
            .with_label(&tr("audio_description.language"))
            .build(),
        0,
        SizerFlag::AlignCenterVertical | SizerFlag::All,
        5,
    );
    row.add(&language, 1, SizerFlag::Expand | SizerFlag::All, 5);
    root.add_sizer(&row, 0, SizerFlag::Expand, 0);
    let verbosity = Choice::builder(&p).build();
    verbosity.append(&tr("audio_description.verbosity.brief"));
    verbosity.append(&tr("audio_description.verbosity.standard"));
    verbosity.append(&tr("audio_description.verbosity.detailed"));
    verbosity.set_selection(
        match Verbosity::from_settings(&saved.audio_description_verbosity) {
            Verbosity::Brief => 0,
            Verbosity::Standard => 1,
            Verbosity::Detailed => 2,
        },
    );
    let row = BoxSizer::builder(Orientation::Horizontal).build();
    row.add(
        &StaticText::builder(&p)
            .with_label(&tr("audio_description.verbosity"))
            .build(),
        0,
        SizerFlag::AlignCenterVertical | SizerFlag::All,
        5,
    );
    row.add(&verbosity, 1, SizerFlag::Expand | SizerFlag::All, 5);
    root.add_sizer(&row, 0, SizerFlag::Expand, 0);
    let extended = CheckBox::builder(&p)
        .with_label(&tr("audio_description.extended"))
        .build();
    extended.set_value(saved.audio_description_extended_pauses);
    root.add(&extended, 0, SizerFlag::Expand | SizerFlag::All, 5);
    let recognize = CheckBox::builder(&p)
        .with_label(&tr("audio_description.recognize_characters"))
        .build();
    recognize.set_value(saved.audio_description_recognize_characters);
    root.add(&recognize, 0, SizerFlag::Expand | SizerFlag::All, 5);
    let save_project_box = CheckBox::builder(&p)
        .with_label(&tr("audio_description.save_project"))
        .build();
    save_project_box.set_value(saved.audio_description_save_project);
    root.add(&save_project_box, 0, SizerFlag::Expand | SizerFlag::All, 5);
    let keep_catalog = CheckBox::builder(&p)
        .with_label(&tr("audio_description.keep_character_catalog"))
        .build();
    keep_catalog.set_value(
        saved.audio_description_keep_character_catalog
            && saved.audio_description_recognize_characters,
    );
    keep_catalog.show(saved.audio_description_recognize_characters);
    root.add(&keep_catalog, 0, SizerFlag::Expand | SizerFlag::All, 5);
    let catalogs = Rc::new(RefCell::new(list_catalogs()));
    let catalog_choice = Choice::builder(&p).build();
    catalog_choice.append(&tr("audio_description.character_catalog.new_option"));
    for c in catalogs.borrow().iter() {
        catalog_choice.append(&c.name);
    }
    let selected_catalog = catalogs
        .borrow()
        .iter()
        .position(|c| c.path.to_string_lossy() == saved.audio_description_character_catalog)
        .map(|x| x + 1)
        .unwrap_or(0);
    catalog_choice.set_selection(selected_catalog as u32);
    let catalog_row = BoxSizer::builder(Orientation::Horizontal).build();
    let catalog_label = StaticText::builder(&p)
        .with_label(&tr("audio_description.character_catalog.selection_label"))
        .build();
    catalog_row.add(
        &catalog_label,
        0,
        SizerFlag::AlignCenterVertical | SizerFlag::All,
        5,
    );
    catalog_row.add(&catalog_choice, 1, SizerFlag::Expand | SizerFlag::All, 5);
    let show_catalog_controls = saved.audio_description_recognize_characters
        && saved.audio_description_keep_character_catalog;
    catalog_label.show(show_catalog_controls);
    catalog_choice.show(show_catalog_controls);
    root.add_sizer(&catalog_row, 0, SizerFlag::Expand, 0);
    let catalog_name_row = BoxSizer::builder(Orientation::Horizontal).build();
    let catalog_name_label = StaticText::builder(&p)
        .with_label(&tr("audio_description.character_catalog.new_name_label"))
        .build();
    let catalog_name = TextCtrl::builder(&p).build();
    catalog_name_row.add(
        &catalog_name_label,
        0,
        SizerFlag::AlignCenterVertical | SizerFlag::All,
        5,
    );
    catalog_name_row.add(&catalog_name, 1, SizerFlag::Expand | SizerFlag::All, 5);
    let show_new_catalog_name = show_catalog_controls && selected_catalog == 0;
    catalog_name_label.show(show_new_catalog_name);
    catalog_name.show(show_new_catalog_name);
    root.add_sizer(&catalog_name_row, 0, SizerFlag::Expand, 0);
    let api_label = StaticText::builder(&p)
        .with_label(&tr("audio_description.gemini_api_key"))
        .build();
    let api = TextCtrl::builder(&p).build();
    api.set_value(&saved.audio_description_gemini_api_key);
    let api_get = Button::builder(&p)
        .with_label(&tr("audio_description.gemini_get_key"))
        .build();
    let api_row = BoxSizer::builder(Orientation::Horizontal).build();
    api_row.add(
        &api_label,
        0,
        SizerFlag::AlignCenterVertical | SizerFlag::All,
        5,
    );
    api_row.add(&api, 1, SizerFlag::Expand | SizerFlag::All, 5);
    api_row.add(&api_get, 0, SizerFlag::All, 5);
    root.add_sizer(&api_row, 0, SizerFlag::Expand, 0);
    let model_label = StaticText::builder(&p)
        .with_label(&tr("audio_description.gemini_model"))
        .build();
    let model = Choice::builder(&p).build();
    model.append(&saved.audio_description_gemini_model);
    model.set_selection(0);
    let refresh = Button::builder(&p)
        .with_label(&tr("audio_description.gemini_refresh_models"))
        .build();
    let model_row = BoxSizer::builder(Orientation::Horizontal).build();
    model_row.add(
        &model_label,
        0,
        SizerFlag::AlignCenterVertical | SizerFlag::All,
        5,
    );
    model_row.add(&model, 1, SizerFlag::Expand | SizerFlag::All, 5);
    model_row.add(&refresh, 0, SizerFlag::All, 5);
    root.add_sizer(&model_row, 0, SizerFlag::Expand, 0);
    let engine = Choice::builder(&p).build();
    engine.append(&tr("audio_description.engine.edge"));
    engine.append(&tr("audio_description.engine.system"));
    let initial_engine = if crate::is_system_voice_engine(&saved.audio_description_tts_engine) {
        1
    } else {
        0
    };
    engine.set_selection(initial_engine);
    let engine_row = BoxSizer::builder(Orientation::Horizontal).build();
    engine_row.add(
        &StaticText::builder(&p)
            .with_label(&tr("audio_description.engine"))
            .build(),
        0,
        SizerFlag::AlignCenterVertical | SizerFlag::All,
        5,
    );
    engine_row.add(&engine, 1, SizerFlag::Expand | SizerFlag::All, 5);
    root.add_sizer(&engine_row, 0, SizerFlag::Expand, 0);
    let voice = Choice::builder(&p).build();
    let voices_edge = voices_data.lock().unwrap().clone();
    let voices_system = crate::load_system_voices();
    let active_voices = Rc::new(RefCell::new(Vec::<VoiceInfo>::new()));
    let fill_voice: Rc<dyn Fn(u32, u32)> = {
        let active = active_voices.clone();
        let voice_c = voice;
        let voices_edge = voices_edge.clone();
        let voices_system = voices_system.clone();
        let langs = langs.clone();
        Rc::new(move |engine_idx, lang_idx| {
            voice_c.clear();
            let code = langs.get(lang_idx as usize).map(|x| x.1).unwrap_or("it");
            let src = if engine_idx == 1 {
                &voices_system
            } else {
                &voices_edge
            };
            let list = src
                .iter()
                .filter(|v| voice_matches_language(v, code))
                .cloned()
                .collect::<Vec<_>>();
            for v in &list {
                voice_c.append(&v.friendly_name);
            }
            if !list.is_empty() {
                voice_c.set_selection(0);
            }
            *active.borrow_mut() = list;
        })
    };
    fill_voice(initial_engine, lang_index as u32);
    let preferred_voice = if saved.audio_description_tts_voice.trim().is_empty() {
        if initial_engine == 1 {
            saved.system_voice.clone()
        } else {
            saved.voice.clone()
        }
    } else {
        saved.audio_description_tts_voice.clone()
    };
    if let Some(index) = active_voices
        .borrow()
        .iter()
        .position(|item| item.short_name == preferred_voice)
    {
        voice.set_selection(index as u32);
    }
    let voice_row = BoxSizer::builder(Orientation::Horizontal).build();
    voice_row.add(
        &StaticText::builder(&p)
            .with_label(&tr("audio_description.voice"))
            .build(),
        0,
        SizerFlag::AlignCenterVertical | SizerFlag::All,
        5,
    );
    voice_row.add(&voice, 1, SizerFlag::Expand | SizerFlag::All, 5);
    root.add_sizer(&voice_row, 0, SizerFlag::Expand, 0);
    let actions = BoxSizer::builder(Orientation::Horizontal).build();
    let modify = Button::builder(&p)
        .with_label(&tr("audio_description.modify_project"))
        .build();
    let start = Button::builder(&p)
        .with_id(ID_AUDIO_DESCRIPTION_START)
        .with_label(&tr("audio_description.start"))
        .build();
    let close = Button::builder(&p)
        .with_id(ID_AUDIO_DESCRIPTION_CLOSE)
        .with_label(&tr("audio_description.close"))
        .build();
    actions.add(&modify, 0, SizerFlag::All, 8);
    actions.add_spacer(1);
    actions.add(&start, 0, SizerFlag::All, 8);
    actions.add(&close, 0, SizerFlag::All, 8);
    root.add_sizer(&actions, 0, SizerFlag::Expand, 0);
    p.set_sizer(root, true);
    let d_input = d;
    input_btn.on_click(move |_| {
        if let Some(path) = choose_input(&d_input) {
            input.set_value(&path.to_string_lossy());
            if catalog_choice.get_selection().unwrap_or(0) == 0
                && catalog_name.get_value().trim().is_empty()
            {
                let suggested = suggested_catalog_name(&path.to_string_lossy());
                if !suggested.is_empty() {
                    catalog_name.set_value(&suggested);
                }
            }
            if output.get_value().trim().is_empty() {
                let mut dest = default_output_dir()
                    .join(path.file_stem().and_then(|s| s.to_str()).unwrap_or("video"));
                dest.set_extension("mp3");
                let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("video");
                dest = default_output_dir()
                    .join(format!("{}_audiodescritto.mp3", sanitize_filename(stem)));
                output.set_value(&dest.to_string_lossy());
            }
        }
    });
    let d_output = d;
    output_btn.on_click(move |_| {
        let ip = PathBuf::from(input.get_value());
        if let Some(path) = choose_output(&d_output, &ip) {
            output.set_value(&path.to_string_lossy());
        }
    });
    let catalog_label_toggle = catalog_label;
    let catalog_choice_toggle = catalog_choice;
    let catalog_name_label_toggle = catalog_name_label;
    let catalog_name_toggle = catalog_name;
    let panel_catalog = p;
    let dialog_catalog = d;
    keep_catalog.on_toggled(move |_| {
        let show_catalog = recognize.get_value() && keep_catalog.get_value();
        catalog_label_toggle.show(show_catalog);
        catalog_choice_toggle.show(show_catalog);
        let show_name = show_catalog && catalog_choice_toggle.get_selection().unwrap_or(0) == 0;
        catalog_name_label_toggle.show(show_name);
        catalog_name_toggle.show(show_name);
        if show_name && catalog_name_toggle.get_value().trim().is_empty() {
            let suggested = suggested_catalog_name(&input.get_value());
            if !suggested.is_empty() {
                catalog_name_toggle.set_value(&suggested);
            }
        }
        panel_catalog.layout();
        dialog_catalog.layout();
    });
    let catalog_name_label_choice = catalog_name_label;
    let catalog_name_choice = catalog_name;
    let panel_catalog_choice = p;
    let dialog_catalog_choice = d;
    catalog_choice.on_selection_changed(move |_| {
        let show_name = recognize.get_value()
            && keep_catalog.get_value()
            && catalog_choice.get_selection().unwrap_or(0) == 0;
        catalog_name_label_choice.show(show_name);
        catalog_name_choice.show(show_name);
        if show_name && catalog_name_choice.get_value().trim().is_empty() {
            let suggested = suggested_catalog_name(&input.get_value());
            if !suggested.is_empty() {
                catalog_name_choice.set_value(&suggested);
            }
        }
        panel_catalog_choice.layout();
        dialog_catalog_choice.layout();
    });
    let keep_catalog_show = keep_catalog;
    let catalog_label_show = catalog_label;
    let catalog_choice_show = catalog_choice;
    let catalog_name_label_show = catalog_name_label;
    let catalog_name_show = catalog_name;
    let panel_recognize = p;
    let dialog_recognize = d;
    recognize.on_toggled(move |_| {
        let on = recognize.get_value();
        keep_catalog_show.show(on);
        if !on {
            keep_catalog_show.set_value(false);
        }
        let show_catalog = on && keep_catalog_show.get_value();
        catalog_label_show.show(show_catalog);
        catalog_choice_show.show(show_catalog);
        let show_name = show_catalog && catalog_choice_show.get_selection().unwrap_or(0) == 0;
        catalog_name_label_show.show(show_name);
        catalog_name_show.show(show_name);
        panel_recognize.layout();
        dialog_recognize.layout();
    });
    let d_api = d;
    api_get.on_click(move |_| {
        if let Err(e) = crate::open_url_in_browser("https://aistudio.google.com/app/apikey") {
            show_error(&d_api, &e);
        }
    });
    let d_refresh = d;
    refresh.on_click(move |_| match fetch_gemini_models(&api.get_value()) {
        Ok(models) => {
            let selected = model.get_string_selection().unwrap_or_default();
            model.clear();
            for m in &models {
                model.append(m);
            }
            let pos = models.iter().position(|m| m == &selected).unwrap_or(0);
            if !models.is_empty() {
                model.set_selection(pos as u32);
            }
        }
        Err(e) => show_error(
            &d_refresh,
            &trf("audio_description.gemini_error_models", &[("error", e)]),
        ),
    });
    let fill_e = fill_voice.clone();
    engine.on_selection_changed(move |_| {
        fill_e(
            engine.get_selection().unwrap_or(0),
            language.get_selection().unwrap_or(0),
        )
    });
    let fill_l = fill_voice.clone();
    language.on_selection_changed(move |_| {
        fill_l(
            engine.get_selection().unwrap_or(0),
            language.get_selection().unwrap_or(0),
        )
    });
    let open_project_requested = Rc::new(Cell::new(false));
    let open_project_requested_button = Rc::clone(&open_project_requested);
    let d_modify = d;
    modify.on_click(move |_| {
        append_podcast_log("audio_description.create.open_project_requested");
        open_project_requested_button.set(true);
        d_modify.end_modal(ID_AUDIO_DESCRIPTION_CLOSE);
    });
    d.set_escape_id(ID_AUDIO_DESCRIPTION_CLOSE);
    let d_close = d;
    close.on_click(move |_| {
        append_podcast_log("audio_description.create.close_requested_button");
        d_close.end_modal(ID_AUDIO_DESCRIPTION_CLOSE);
    });
    let d_window_close = d;
    d.on_close(move |event| {
        append_podcast_log("audio_description.create.close_requested_window");
        d_window_close.end_modal(ID_AUDIO_DESCRIPTION_CLOSE);
        event.skip(false);
    });
    let quit_requested = Rc::new(Cell::new(false));
    let quit_requested_menu = quit_requested.clone();
    let d_quit = d;
    d.bind_internal(EventType::MENU, move |event| {
        if event.get_id() == crate::ID_EXIT {
            append_podcast_log("audio_description.create.quit_requested_menu");
            quit_requested_menu.set(true);
            d_quit.end_modal(ID_AUDIO_DESCRIPTION_CLOSE);
        } else {
            event.skip(true);
        }
    });
    let parent_run = *parent;
    let settings_run = settings.clone();
    let rt_run = rt.clone();
    start.on_click(move |_| {
        let input_path = PathBuf::from(input.get_value());
        let output_path = PathBuf::from(output.get_value());
        let lang_idx = language.get_selection().unwrap_or(0) as usize;
        let language_code = langs.get(lang_idx).map(|x| x.1).unwrap_or("it").to_string();
        let verbosity_value = match verbosity.get_selection().unwrap_or(2) {
            0 => Verbosity::Brief,
            1 => Verbosity::Standard,
            _ => Verbosity::Detailed,
        };
        let engine_value = if engine.get_selection().unwrap_or(0) == 1 {
            "system".to_string()
        } else {
            "microsoft".to_string()
        };
        let voice_idx = voice.get_selection().unwrap_or(0) as usize;
        let voice_value = active_voices
            .borrow()
            .get(voice_idx)
            .map(|v| v.short_name.clone())
            .unwrap_or_default();
        let model_value = model
            .get_string_selection()
            .unwrap_or_else(|| saved.audio_description_gemini_model.clone());
        let catalog = if keep_catalog.get_value() {
            let sel = catalog_choice.get_selection().unwrap_or(0) as usize;
            if sel == 0 {
                let name = catalog_name.get_value().trim().to_string();
                if name.is_empty() {
                    show_error(&d, &tr("audio_description.character_catalog.name_error"));
                    catalog_name.set_focus();
                    return;
                }
                Some(CharacterCatalog {
                    name: name.clone(),
                    path: catalog_path_for_name(&name),
                    characters: Vec::new(),
                })
            } else {
                catalogs.borrow().get(sel - 1).cloned()
            }
        } else {
            None
        };
        let job = CreateJob {
            input_path,
            output_path,
            language_code: language_code.clone(),
            verbosity: verbosity_value,
            allow_extended_pauses: extended.get_value(),
            recognize_characters: recognize.get_value(),
            save_project: save_project_box.get_value(),
            keep_character_catalog: keep_catalog.get_value(),
            catalog: catalog.clone(),
            tts_engine: engine_value.clone(),
            tts_voice: voice_value.clone(),
            rate: saved.rate,
            pitch: saved.pitch,
            volume: saved.volume,
            gemini_api_key: api.get_value(),
            gemini_model: model_value.clone(),
        };
        {
            let mut st = settings_run.lock().unwrap();
            st.audio_description_gemini_api_key = job.gemini_api_key.clone();
            st.audio_description_gemini_model = model_value;
            st.audio_description_language = language_code;
            st.audio_description_tts_engine = engine_value.clone();
            st.audio_description_tts_voice = voice_value.clone();
            st.audio_description_verbosity = verbosity_value.as_bridge().to_string();
            st.audio_description_extended_pauses = job.allow_extended_pauses;
            st.audio_description_recognize_characters = job.recognize_characters;
            st.audio_description_save_project = job.save_project;
            st.audio_description_keep_character_catalog = job.keep_character_catalog;
            st.audio_description_character_catalog = catalog
                .as_ref()
                .map(|c| c.path.to_string_lossy().to_string())
                .unwrap_or_default();
            st.save();
        }
        match run_with_progress(&parent_run, job, rt_run.clone()) {
            Ok(out) => {
                let output_to_open = out.output_path.clone();
                let mut msg = trf(
                    "audio_description.success_details",
                    &[
                        ("path", out.output_path.display().to_string()),
                        ("count", out.generated.to_string()),
                        ("normal", (out.inserted - out.extended).to_string()),
                        ("pauses", out.extended.to_string()),
                        ("dropped", out.dropped.to_string()),
                    ],
                );
                if out.dropped_mandatory > 0 {
                    msg.push_str(&format!(
                        "\n\n{}",
                        trf(
                            "audio_description.warning.mandatory_dropped",
                            &[("count", out.dropped_mandatory.to_string())]
                        )
                    ));
                }
                if let Some(p) = out.project_path {
                    msg.push_str(&format!(
                        "\n\n{}",
                        trf(
                            "audio_description.project_saved",
                            &[("path", p.display().to_string())]
                        )
                    ));
                }
                if let Some(p) = out.catalog_path {
                    msg.push_str(&format!(
                        "\n\n{}",
                        trf(
                            "audio_description.catalog_output",
                            &[("path", p.display().to_string())]
                        )
                    ));
                }
                show_completion(&d, &msg);
                append_podcast_log(&format!(
                    "audio_description.create.open_output_requested path={}",
                    output_to_open.display()
                ));
                if let Err(error) = crate::open_local_media_with_mpv(&output_to_open) {
                    append_podcast_log(&format!(
                        "audio_description.create.open_output_failed path={} err={}",
                        output_to_open.display(),
                        error
                    ));
                    show_error(&d, &error);
                } else {
                    append_podcast_log(&format!(
                        "audio_description.create.open_output_completed path={}",
                        output_to_open.display()
                    ));
                }
            }
            Err(e) => {
                if e == "cancelled" {
                    append_podcast_log("audio_description.create.closed_after_cancel");
                    d.end_modal(ID_AUDIO_DESCRIPTION_CLOSE);
                } else {
                    show_error(&d, &e);
                }
            }
        }
    });
    d.show_modal();
    d.destroy();
    if quit_requested.get() {
        append_podcast_log("audio_description.create.quit_forwarded_to_main");
        parent.close(false);
    } else if open_project_requested.get() {
        append_podcast_log("audio_description.create.open_project_after_close");
        open_project_editor(parent, rt, voices_data);
    }
}

fn format_mmss(seconds: f64) -> String {
    let value = seconds.max(0.0);
    let minutes = (value / 60.0).floor() as u32;
    let remaining = value - minutes as f64 * 60.0;
    format!("{minutes:02}:{remaining:05.2}")
}

fn project_description_details(project: &AudioDescriptionProject, index: usize) -> String {
    let Some(description) = project.descriptions.get(index) else {
        return tr("audio_description.project.status.ready");
    };
    let mode = if description.extended_pause {
        tr("audio_description.project.extended")
    } else {
        tr("audio_description.project.normal")
    };
    trf(
        "audio_description.project.details",
        &[
            ("path", project.output_mp3_path.display().to_string()),
            ("source", format_mmss(description.source_start_sec)),
            ("start", format_mmss(description.output_start_sec)),
            ("end", format_mmss(description.output_end_sec)),
            ("duration", format!("{:.3}", description.tts_duration_sec)),
            ("mode", mode),
        ],
    )
}

fn project_edit_available_duration(
    project: &AudioDescriptionProject,
    index: usize,
) -> Result<Option<f64>, String> {
    let description = project
        .descriptions
        .get(index)
        .ok_or_else(|| tr("audio_description.project.no_selection"))?;
    if description.extended_pause {
        return Ok(None);
    }
    let protected: Vec<BridgeInterval> = project
        .protected_intervals
        .iter()
        .map(|interval| BridgeInterval {
            start_sec: interval.start_sec,
            end_sec: interval.end_sec,
        })
        .collect();
    let normalized = normalize_intervals(&protected, project.source_duration_sec);
    let free = free_intervals(&normalized, project.source_duration_sec);
    let start = description.source_start_sec.max(0.0);
    let Some((_, gap_end)) = free
        .iter()
        .find(|(gap_start, gap_end)| start + 0.001 >= *gap_start && start <= *gap_end + 0.001)
    else {
        return Ok(Some(0.0));
    };
    let next_description_start = project
        .descriptions
        .iter()
        .enumerate()
        .filter(|(candidate_index, candidate)| {
            *candidate_index != index && candidate.source_start_sec > start + 0.001
        })
        .map(|(_, candidate)| candidate.source_start_sec)
        .min_by(f64::total_cmp);
    let available_end = next_description_start
        .map(|next_start| gap_end.min(next_start))
        .unwrap_or(*gap_end);
    Ok(Some((available_end - start).max(0.0)))
}

fn synthesize_project_text_duration_with_voice(
    project: &AudioDescriptionProject,
    text: &str,
    index: usize,
    tts_voice: &str,
    rt: &Runtime,
) -> Result<f64, String> {
    let dir = cache_dir("project_edit")?;
    let cancel = Arc::new(AtomicBool::new(false));
    let result = (|| {
        let pcm = synthesize_text_pcm(
            text,
            TtsParameters {
                engine: &project.tts_engine,
                voice: tts_voice,
                rate: project.tts_rate,
                pitch: project.tts_pitch,
                volume: project.tts_volume,
            },
            rt,
            &dir,
            index,
            &cancel,
        )?;
        Ok(pcm.len() as f64 / (MIX_CHANNELS as f64 * MIX_SAMPLE_RATE as f64))
    })();
    let _ = fs::remove_dir_all(&dir);
    result
}

fn synthesize_project_text_duration(
    project: &AudioDescriptionProject,
    text: &str,
    index: usize,
    rt: &Runtime,
) -> Result<f64, String> {
    synthesize_project_text_duration_with_voice(project, text, index, &project.tts_voice, rt)
}

#[derive(Clone, Debug)]
struct ProjectVoiceFitError {
    source_start_sec: f64,
    actual_sec: f64,
}

#[derive(Clone, Debug, Default)]
struct ProjectVoiceValidationState {
    progress: i32,
    done: Option<Result<(), String>>,
    fit_error: Option<ProjectVoiceFitError>,
}

fn validate_project_voice(
    project: &AudioDescriptionProject,
    tts_voice: &str,
    rt: &Runtime,
    state: &Arc<Mutex<ProjectVoiceValidationState>>,
) -> Result<(), String> {
    let total = project.descriptions.len().max(1);
    let work = cache_dir("project_voice_check")?;
    let cancel = Arc::new(AtomicBool::new(false));
    let result = (|| {
        let mut synthesized = Vec::with_capacity(project.descriptions.len());
        for (index, description) in project.descriptions.iter().enumerate() {
            let pcm = synthesize_text_pcm(
                &description.text,
                TtsParameters {
                    engine: &project.tts_engine,
                    voice: tts_voice,
                    rate: project.tts_rate,
                    pitch: project.tts_pitch,
                    volume: project.tts_volume,
                },
                rt,
                &work,
                index,
                &cancel,
            )?;
            let duration_sec =
                pcm.len() as f64 / (MIX_CHANNELS as f64 * MIX_SAMPLE_RATE as f64);
            synthesized.push(SynthesizedDescription {
                original_index: index,
                text: description.text.clone(),
                desired_start_sec: description.gemini_start_sec,
                mandatory: description.mandatory,
                slot_id: description.slot_id.clone(),
                slot_start_sec: description.slot_start_sec,
                slot_end_sec: description.slot_end_sec,
                pcm,
                duration_sec,
            });
            state.lock().unwrap().progress =
                ((index + 1) as i32 * 90 / total as i32).clamp(0, 90);
        }

        let protected = project
            .protected_intervals
            .iter()
            .map(|interval| BridgeInterval {
                start_sec: interval.start_sec,
                end_sec: interval.end_sec,
            })
            .collect::<Vec<_>>();
        let (_, dropped) = schedule_descriptions(
            &synthesized,
            &protected,
            project.source_duration_sec,
            project.allow_extended_pauses,
        );
        if let Some(first) = dropped.first() {
            let source_start_sec = project
                .descriptions
                .get(first.original_index)
                .map(|description| description.source_start_sec)
                .unwrap_or(first.desired_start_sec);
            state.lock().unwrap().fit_error = Some(ProjectVoiceFitError {
                source_start_sec,
                actual_sec: first.duration_sec,
            });
            return Err("voice_does_not_fit".to_string());
        }
        state.lock().unwrap().progress = 100;
        Ok(())
    })();
    let _ = fs::remove_dir_all(&work);
    result
}

fn run_project_voice_validation_with_progress(
    parent: &Dialog,
    project: AudioDescriptionProject,
    tts_voice: String,
    runtime: Arc<Runtime>,
) -> (Result<(), String>, Option<ProjectVoiceFitError>) {
    let progress_dialog = Dialog::builder(
        parent,
        &tr("audio_description.project.voice_check_title"),
    )
    .with_style(
        DialogStyle::Caption
            | DialogStyle::SystemMenu
            | DialogStyle::CloseBox
            | DialogStyle::StayOnTop,
    )
    .with_size(520, 150)
    .build();
    let panel = Panel::builder(&progress_dialog).build();
    let root = BoxSizer::builder(Orientation::Vertical).build();
    let label = StaticText::builder(&panel)
        .with_label(&tr("audio_description.project.voice_check_status"))
        .build();
    root.add(
        &label,
        0,
        SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Top,
        12,
    );
    let gauge = Gauge::builder(&panel).with_range(100).build();
    root.add(
        &gauge,
        0,
        SizerFlag::Expand | SizerFlag::All,
        12,
    );
    panel.set_sizer(root, true);

    let state = Arc::new(Mutex::new(ProjectVoiceValidationState::default()));
    let thread_state = Arc::clone(&state);
    thread::spawn(move || {
        let result = validate_project_voice(&project, &tts_voice, &runtime, &thread_state);
        thread_state.lock().unwrap().done = Some(result);
    });

    let result = Rc::new(RefCell::new(None::<(
        Result<(), String>,
        Option<ProjectVoiceFitError>,
    )>));
    let timer = Rc::new(Timer::new(&progress_dialog));
    let timer_tick = Rc::clone(&timer);
    let timer_handle = Rc::clone(&timer);
    let state_tick = Arc::clone(&state);
    let result_tick = Rc::clone(&result);
    let dialog_tick = progress_dialog;
    timer_tick.on_tick(move |_| {
        let snapshot = state_tick.lock().unwrap().clone();
        gauge.set_value(snapshot.progress.clamp(0, 99));
        if let Some(done) = snapshot.done {
            timer_handle.stop();
            gauge.set_value(100);
            *result_tick.borrow_mut() = Some((done, snapshot.fit_error));
            dialog_tick.end_modal(ID_OK);
        }
    });
    progress_dialog.on_close(move |event| {
        event.skip(false);
    });
    timer.start(100, false);
    progress_dialog.show_modal();
    timer.stop();
    progress_dialog.destroy();
    result
        .borrow_mut()
        .take()
        .unwrap_or_else(|| (Err("voice_check_failed".to_string()), None))
}

fn project_file_dialog(parent: &Frame) -> Option<PathBuf> {
    let d = FileDialog::builder(parent)
        .with_message(&tr("audio_description.project.open_title"))
        .with_wildcard("Progetto Sonarpad|*.sonarpad-ad.json|JSON|*.json|Tutti|*.*")
        .with_style(FileDialogStyle::Open | FileDialogStyle::FileMustExist)
        .build();
    if d.show_modal() == ID_OK {
        d.get_path().map(PathBuf::from)
    } else {
        None
    }
}

fn load_project(path: &Path) -> Result<AudioDescriptionProject, String> {
    let raw = fs::read(path).map_err(|e| e.to_string())?;
    let p: AudioDescriptionProject =
        serde_json::from_slice(&raw).map_err(|e| format!("Progetto non valido: {e}"))?;
    if p.format != PROJECT_FORMAT || p.version != PROJECT_VERSION {
        return Err("Formato progetto non supportato.".into());
    }
    Ok(p)
}

fn rebuild_project(
    project: &mut AudioDescriptionProject,
    project_file: &Path,
    rt: &Runtime,
    cancel: Arc<AtomicBool>,
    state: Arc<Mutex<ProgressState>>,
) -> Result<JobOutcome, String> {
    let work = cache_dir("project")?;
    let result = (|| {
        let source = work.join("source.wav");
        let duration = decode_source_audio(&project.source_path, &source, &cancel)?.duration_sec;
        let mut synthesized = Vec::new();
        for (index, description) in project.descriptions.iter().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                return Err("cancelled".to_string());
            }
            let pcm = synthesize_text_pcm(
                &description.text,
                TtsParameters {
                    engine: &project.tts_engine,
                    voice: &project.tts_voice,
                    rate: project.tts_rate,
                    pitch: project.tts_pitch,
                    volume: project.tts_volume,
                },
                rt,
                &work,
                index,
                &cancel,
            )?;
            let duration_sec = pcm.len() as f64 / (MIX_CHANNELS as f64 * MIX_SAMPLE_RATE as f64);
            synthesized.push(SynthesizedDescription {
                original_index: index,
                text: description.text.clone(),
                desired_start_sec: description.gemini_start_sec,
                mandatory: description.mandatory,
                slot_id: description.slot_id.clone(),
                slot_start_sec: description.slot_start_sec,
                slot_end_sec: description.slot_end_sec,
                pcm,
                duration_sec,
            });
            state.lock().unwrap().progress =
                10 + ((index + 1) as i32 * 70 / project.descriptions.len().max(1) as i32);
        }
        let protected = project
            .protected_intervals
            .iter()
            .map(|x| BridgeInterval {
                start_sec: x.start_sec,
                end_sec: x.end_sec,
            })
            .collect::<Vec<_>>();
        let (scheduled, dropped) = schedule_descriptions(
            &synthesized,
            &protected,
            duration,
            project.allow_extended_pauses,
        );
        if scheduled.is_empty() {
            return Err("Nessuna descrizione può essere inserita in sicurezza.".to_string());
        }
        let mix = work.join("mix.wav");
        let output_duration = render_mix(&source, &mix, &scheduled, &cancel)?;
        let temporary_mp3 = work.join("project-rebuilt.mp3");
        encode_mp3(&mix, &temporary_mp3, &cancel)?;
        fs::copy(&temporary_mp3, &project.output_mp3_path)
            .map_err(|e| format!("Salvataggio MP3 fallito: {e}"))?;

        let previous = project.clone();
        let mut extra_offset = 0.0;
        let mut descriptions = Vec::new();
        for (new_id, scheduled_description) in scheduled.iter().enumerate() {
            let old = previous
                .descriptions
                .get(scheduled_description.original_index)
                .ok_or_else(|| "Indice descrizione progetto non valido.".to_string())?;
            let output_start = scheduled_description.start_sec + extra_offset;
            let extended_pause_duration_sec = if scheduled_description.extended_pause {
                scheduled_description.duration_sec
            } else {
                0.0
            };
            let output_end = output_start + scheduled_description.duration_sec;
            descriptions.push(ProjectDescription {
                id: new_id,
                text: old.text.clone(),
                original_text: old.original_text.clone(),
                rendered_text: old.text.clone(),
                modified: old.text != old.original_text,
                gemini_start_sec: old.gemini_start_sec,
                mandatory: old.mandatory,
                slot_id: old.slot_id.clone(),
                slot_start_sec: old.slot_start_sec,
                slot_end_sec: old.slot_end_sec,
                source_start_sec: scheduled_description.start_sec,
                output_start_sec: output_start,
                output_end_sec: output_end,
                tts_duration_sec: scheduled_description.duration_sec,
                extended_pause: scheduled_description.extended_pause,
                extended_pause_duration_sec,
                duck_start_sec: (!scheduled_description.extended_pause).then_some(output_start),
                duck_end_sec: (!scheduled_description.extended_pause).then_some(output_end),
            });
            extra_offset += extended_pause_duration_sec;
        }
        let excluded_descriptions = dropped
            .iter()
            .enumerate()
            .map(|(id, dropped_description)| ProjectExcluded {
                id,
                text: dropped_description.text.clone(),
                gemini_start_sec: dropped_description.desired_start_sec,
                mandatory: dropped_description.mandatory,
                slot_id: dropped_description.slot_id.clone(),
                tts_duration_sec: dropped_description.duration_sec,
                reason: dropped_description.reason.clone(),
            })
            .collect::<Vec<_>>();

        project.source_duration_sec = duration;
        project.output_duration_sec = output_duration;
        project.updated_at_utc = now_utc();
        project.descriptions = descriptions;
        project.excluded_descriptions = excluded_descriptions;
        save_project(project_file, project)?;

        Ok(JobOutcome {
            output_path: project.output_mp3_path.clone(),
            project_path: Some(project_file.to_path_buf()),
            catalog_path: None,
            generated: project.descriptions.len() + project.excluded_descriptions.len(),
            inserted: project.descriptions.len(),
            extended: project
                .descriptions
                .iter()
                .filter(|d| d.extended_pause)
                .count(),
            dropped: project.excluded_descriptions.len(),
            dropped_mandatory: project
                .excluded_descriptions
                .iter()
                .filter(|d| d.mandatory)
                .count(),
        })
    })();
    let _ = fs::remove_dir_all(work);
    result
}

fn run_project_export_with_progress(
    parent: &Dialog,
    mut project: AudioDescriptionProject,
    project_file: PathBuf,
    runtime: Arc<Runtime>,
) -> Result<JobOutcome, String> {
    append_podcast_log("audio_description.project.export_started");
    let progress_dialog = Dialog::builder(parent, &tr("audio_description.project.title"))
        .with_style(
            DialogStyle::Caption
                | DialogStyle::SystemMenu
                | DialogStyle::CloseBox
                | DialogStyle::StayOnTop,
        )
        .with_size(520, 180)
        .build();
    let panel = Panel::builder(&progress_dialog).build();
    let root = BoxSizer::builder(Orientation::Vertical).build();
    let label = StaticText::builder(&panel)
        .with_label(&tr("audio_description.project.status.exporting"))
        .build();
    root.add(
        &label,
        0,
        SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Top,
        12,
    );
    let gauge = Gauge::builder(&panel).with_range(100).build();
    root.add(
        &gauge,
        0,
        SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Top,
        12,
    );
    let buttons = BoxSizer::builder(Orientation::Horizontal).build();
    let cancel_button = Button::builder(&panel)
        .with_id(ID_AUDIO_DESCRIPTION_PROGRESS_CANCEL)
        .with_label(&tr("audio_description.cancel"))
        .build();
    buttons.add_spacer(1);
    buttons.add(&cancel_button, 0, SizerFlag::All, 10);
    root.add_sizer(&buttons, 0, SizerFlag::Expand | SizerFlag::Bottom, 0);
    panel.set_sizer(root, true);

    let state = Arc::new(Mutex::new(ProgressState::default()));
    let cancel = Arc::new(AtomicBool::new(false));
    let thread_state = Arc::clone(&state);
    let thread_cancel = Arc::clone(&cancel);
    thread::spawn(move || {
        let result = rebuild_project(
            &mut project,
            &project_file,
            &runtime,
            thread_cancel,
            Arc::clone(&thread_state),
        );
        thread_state.lock().unwrap().done = Some(result);
    });

    let result = Rc::new(RefCell::new(None::<Result<JobOutcome, String>>));
    let finished = Rc::new(Cell::new(false));
    let cancel_pending = Rc::new(Cell::new(false));
    let cancel_button_flag = Arc::clone(&cancel);
    let cancel_pending_button = Rc::clone(&cancel_pending);
    cancel_button.on_click(move |_| {
        if !cancel_pending_button.replace(true) {
            append_podcast_log("audio_description.project.export_cancel_requested_button");
            cancel_button_flag.store(true, Ordering::SeqCst);
            cancel_button.enable(false);
            label.set_label(&tr("audio_description.status.canceling"));
        }
    });
    let cancel_close = Arc::clone(&cancel);
    let cancel_pending_close = Rc::clone(&cancel_pending);
    let finished_close = Rc::clone(&finished);
    progress_dialog.on_close(move |event| {
        if finished_close.get() {
            event.skip(true);
            return;
        }
        if !cancel_pending_close.replace(true) {
            append_podcast_log("audio_description.project.export_cancel_requested_close");
            cancel_close.store(true, Ordering::SeqCst);
            cancel_button.enable(false);
            label.set_label(&tr("audio_description.status.canceling"));
        }
        event.skip(false);
    });

    let timer = Rc::new(Timer::new(&progress_dialog));
    let timer_tick = Rc::clone(&timer);
    let timer_handle = Rc::clone(&timer);
    let state_tick = Arc::clone(&state);
    let result_tick = Rc::clone(&result);
    let finished_tick = Rc::clone(&finished);
    let cancel_pending_tick = Rc::clone(&cancel_pending);
    let dialog_tick = progress_dialog;
    timer_tick.on_tick(move |_| {
        let snapshot = state_tick.lock().unwrap().clone();
        gauge.set_value(snapshot.progress.clamp(0, 99));
        if let Some(done) = snapshot.done {
            if cancel_pending_tick.get() {
                append_podcast_log("audio_description.project.export_cancel_completed");
            }
            timer_handle.stop();
            gauge.set_value(100);
            *result_tick.borrow_mut() = Some(done);
            finished_tick.set(true);
            dialog_tick.end_modal(ID_OK);
        }
    });
    timer.start(100, false);
    progress_dialog.show_modal();
    timer.stop();
    progress_dialog.destroy();
    result
        .borrow_mut()
        .take()
        .unwrap_or_else(|| Err("cancelled".to_string()))
}

pub fn open_project_editor(
    parent: &Frame,
    rt: &Arc<Runtime>,
    voices_data: &Arc<Mutex<Vec<VoiceInfo>>>,
) {
    let Some(path) = project_file_dialog(parent) else {
        return;
    };
    let project_value = match load_project(&path) {
        Ok(project) => project,
        Err(error) => {
            let dialog =
                MessageDialog::builder(parent, &error, &tr("audio_description.project.title"))
                    .with_style(MessageDialogStyle::OK | MessageDialogStyle::IconError)
                    .build();
            dialog.show_modal();
            return;
        }
    };
    let project = Rc::new(RefCell::new(project_value));
    let dialog = Dialog::builder(parent, &tr("audio_description.project.title"))
        .with_style(DialogStyle::DefaultDialogStyle | DialogStyle::ResizeBorder)
        .with_size(760, 520)
        .build();
    let panel = Panel::builder(&dialog).build();
    let root = BoxSizer::builder(Orientation::Vertical).build();

    root.add(
        &StaticText::builder(&panel)
            .with_label(&tr("audio_description.project.descriptions"))
            .build(),
        0,
        SizerFlag::Expand | SizerFlag::All,
        5,
    );
    let choice = Choice::builder(&panel).build();
    for description in &project.borrow().descriptions {
        choice.append(&format!(
            "{} - {}",
            format_mmss(description.source_start_sec),
            description.text
        ));
    }
    if !project.borrow().descriptions.is_empty() {
        choice.set_selection(0);
    }
    root.add(&choice, 0, SizerFlag::Expand | SizerFlag::All, 5);

    root.add(
        &StaticText::builder(&panel)
            .with_label(&tr("audio_description.project.text"))
            .build(),
        0,
        SizerFlag::Expand | SizerFlag::All,
        5,
    );
    let text = TextCtrl::builder(&panel)
        .with_style(TextCtrlStyle::MultiLine)
        .build();
    if let Some(description) = project.borrow().descriptions.first() {
        text.set_value(&description.text);
    }
    root.add(&text, 1, SizerFlag::Expand | SizerFlag::All, 5);

    let voice = Choice::builder(&panel).build();
    let voices_edge = voices_data.lock().unwrap().clone();
    let voices_system = crate::load_system_voices();
    let project_engine_is_system = crate::is_system_voice_engine(&project.borrow().tts_engine);
    let project_language = project.borrow().language_code.clone();
    let source_voices = if project_engine_is_system {
        &voices_system
    } else {
        &voices_edge
    };
    let mut project_voice_options = source_voices
        .iter()
        .filter(|candidate| voice_matches_language(candidate, &project_language))
        .cloned()
        .collect::<Vec<_>>();
    let current_project_voice = project.borrow().tts_voice.clone();
    if !current_project_voice.trim().is_empty()
        && !project_voice_options
            .iter()
            .any(|candidate| candidate.short_name == current_project_voice)
    {
        project_voice_options.insert(
            0,
            VoiceInfo {
                short_name: current_project_voice.clone(),
                friendly_name: current_project_voice,
                locale: project_language.clone(),
                suggested_codec: String::new(),
            },
        );
    }
    let project_voices = Rc::new(RefCell::new(project_voice_options));
    for candidate in project_voices.borrow().iter() {
        voice.append(&candidate.friendly_name);
    }
    if let Some(index) = project_voices
        .borrow()
        .iter()
        .position(|candidate| candidate.short_name == project.borrow().tts_voice)
    {
        voice.set_selection(index as u32);
    } else if !project_voices.borrow().is_empty() {
        voice.set_selection(0);
    }
    let voice_row = BoxSizer::builder(Orientation::Horizontal).build();
    voice_row.add(
        &StaticText::builder(&panel)
            .with_label(&tr("audio_description.voice"))
            .build(),
        0,
        SizerFlag::AlignCenterVertical | SizerFlag::All,
        5,
    );
    voice_row.add(&voice, 1, SizerFlag::Expand | SizerFlag::All, 5);
    root.add_sizer(&voice_row, 0, SizerFlag::Expand, 0);

    let initial_status = if project.borrow().descriptions.is_empty() {
        tr("audio_description.project.status.ready")
    } else {
        project_description_details(&project.borrow(), 0)
    };
    let status = StaticText::builder(&panel)
        .with_label(&initial_status)
        .build();
    root.add(&status, 0, SizerFlag::Expand | SizerFlag::All, 5);

    let row = BoxSizer::builder(Orientation::Horizontal).build();
    let apply = Button::builder(&panel)
        .with_label(&tr("audio_description.project.apply"))
        .build();
    let play = Button::builder(&panel)
        .with_label(&tr("audio_description.project.play_description"))
        .build();
    let delete = Button::builder(&panel)
        .with_label(&tr("audio_description.project.delete_description"))
        .build();
    let export = Button::builder(&panel)
        .with_label(&tr("audio_description.project.export"))
        .build();
    let close = Button::builder(&panel)
        .with_id(ID_AUDIO_DESCRIPTION_PROJECT_CLOSE)
        .with_label(&tr("audio_description.close"))
        .build();
    for button in [&apply, &play, &delete, &export, &close] {
        row.add(button, 0, SizerFlag::All, 4);
    }
    root.add_sizer(&row, 0, SizerFlag::Expand, 0);
    panel.set_sizer(root, true);

    let project_selection = Rc::clone(&project);
    choice.on_selection_changed(move |_| {
        if let Some(index) = choice.get_selection().map(|value| value as usize)
            && let Some(description) = project_selection.borrow().descriptions.get(index)
        {
            text.set_value(&description.text);
            status.set_label(&project_description_details(
                &project_selection.borrow(),
                index,
            ));
        }
    });

    let project_voice = Rc::clone(&project);
    let path_voice = path.clone();
    let project_voices_change = Rc::clone(&project_voices);
    let rt_voice = Arc::clone(rt);
    let dialog_voice = dialog;
    let voice_change_guard = Rc::new(Cell::new(false));
    let voice_change_guard_event = Rc::clone(&voice_change_guard);
    voice.on_selection_changed(move |_| {
        if voice_change_guard_event.get() {
            return;
        }
        let selected_index = voice.get_selection().unwrap_or(0) as usize;
        let candidate = {
            project_voices_change
                .borrow()
                .get(selected_index)
                .cloned()
        };
        let Some(candidate) = candidate else {
            return;
        };
        if candidate.short_name == project_voice.borrow().tts_voice {
            return;
        }

        let snapshot = project_voice.borrow().clone();
        let previous_voice = snapshot.tts_voice.clone();
        let (validation, fit_error) = run_project_voice_validation_with_progress(
            &dialog_voice,
            snapshot,
            candidate.short_name.clone(),
            Arc::clone(&rt_voice),
        );

        match validation {
            Ok(()) => {
                {
                    let mut mutable = project_voice.borrow_mut();
                    mutable.tts_voice = candidate.short_name.clone();
                    mutable.updated_at_utc = now_utc();
                }
                if let Err(error) = save_project(&path_voice, &project_voice.borrow()) {
                    project_voice.borrow_mut().tts_voice = previous_voice.clone();
                    show_project_error(&dialog_voice, &error);
                    voice_change_guard_event.set(true);
                    let previous_index = project_voices_change
                        .borrow()
                        .iter()
                        .position(|item| item.short_name == previous_voice);
                    if let Some(previous_index) = previous_index {
                        voice.set_selection(previous_index as u32);
                    }
                    voice_change_guard_event.set(false);
                    return;
                }
                let message = trf(
                    "audio_description.project.voice_changed",
                    &[(
                        "count",
                        project_voice.borrow().descriptions.len().to_string(),
                    )],
                );
                let info = MessageDialog::builder(
                    &dialog_voice,
                    &message,
                    &tr("audio_description.project.voice_changed_title"),
                )
                .with_style(MessageDialogStyle::OK | MessageDialogStyle::IconInformation)
                .build();
                info.show_modal();
            }
            Err(error) => {
                let message = if error == "voice_does_not_fit" {
                    fit_error.map_or_else(
                        || tr("audio_description.project.voice_change_failed"),
                        |fit| {
                            trf(
                                "audio_description.project.voice_too_long",
                                &[
                                    ("time", format_mmss(fit.source_start_sec)),
                                    ("actual", format!("{:.3}", fit.actual_sec)),
                                ],
                            )
                        },
                    )
                } else {
                    trf(
                        "audio_description.project.voice_check_error",
                        &[("error", error)],
                    )
                };
                show_project_error(&dialog_voice, &message);
                voice_change_guard_event.set(true);
                let previous_index = project_voices_change
                    .borrow()
                    .iter()
                    .position(|item| item.short_name == previous_voice);
                if let Some(previous_index) = previous_index {
                    voice.set_selection(previous_index as u32);
                }
                voice_change_guard_event.set(false);
            }
        }
    });

    let project_apply = Rc::clone(&project);
    let path_apply = path.clone();
    let dialog_apply = dialog;
    let rt_apply = Arc::clone(rt);
    apply.on_click(move |_| {
        let Some(index) = choice.get_selection().map(|value| value as usize) else {
            show_project_error(&dialog_apply, &tr("audio_description.project.no_selection"));
            return;
        };
        let value = text.get_value().trim().to_string();
        if value.is_empty() {
            show_project_error(&dialog_apply, &tr("audio_description.project.error_empty"));
            return;
        }
        if project_apply
            .borrow()
            .descriptions
            .get(index)
            .is_some_and(|description| description.text == value)
        {
            status.set_label(&tr("audio_description.project.edit_saved"));
            return;
        }

        status.set_label(&tr("audio_description.project.status.checking_duration"));
        let available = match project_edit_available_duration(&project_apply.borrow(), index) {
            Ok(value) => value,
            Err(error) => {
                show_project_error(&dialog_apply, &error);
                return;
            }
        };
        let duration = match synthesize_project_text_duration(
            &project_apply.borrow(),
            &value,
            index,
            &rt_apply,
        ) {
            Ok(value) => value,
            Err(error) => {
                show_project_error(&dialog_apply, &error);
                return;
            }
        };
        if let Some(available) = available
            && duration > available + 0.001
        {
            show_project_error(
                &dialog_apply,
                &trf(
                    "audio_description.project.error_too_long",
                    &[
                        ("actual", format!("{duration:.3}")),
                        ("available", format!("{available:.3}")),
                    ],
                ),
            );
            status.set_label(&project_description_details(&project_apply.borrow(), index));
            return;
        }

        {
            let mut mutable = project_apply.borrow_mut();
            if let Some(description) = mutable.descriptions.get_mut(index) {
                description.text = value.clone();
                description.rendered_text = value;
                description.modified = description.text != description.original_text;
            }
            mutable.updated_at_utc = now_utc();
        }
        if let Err(error) = save_project(&path_apply, &project_apply.borrow()) {
            show_project_error(&dialog_apply, &error);
            return;
        }
        choice.clear();
        for description in &project_apply.borrow().descriptions {
            choice.append(&format!(
                "{} - {}",
                format_mmss(description.source_start_sec),
                description.text
            ));
        }
        choice.set_selection(index as u32);
        status.set_label(&tr("audio_description.project.edit_saved"));
        show_project_edit_success(&dialog_apply);
    });

    let project_play = Rc::clone(&project);
    let rt_play = Arc::clone(rt);
    let dialog_play = dialog;
    play.on_click(move |_| {
        let Some(index) = choice.get_selection().map(|value| value as usize) else {
            show_error(&dialog_play, &tr("audio_description.project.no_selection"));
            return;
        };
        let preview_text = text.get_value().trim().to_string();
        if preview_text.is_empty() {
            show_error(&dialog_play, &tr("audio_description.project.error_empty"));
            return;
        }
        let snapshot = project_play.borrow().clone();
        let bytes = match crate::synthesize_voice_chunk_blocking(
            &snapshot.tts_engine,
            &preview_text,
            &snapshot.tts_voice,
            snapshot.tts_rate,
            snapshot.tts_pitch,
            snapshot.tts_volume,
            &rt_play,
        ) {
            Ok(bytes) => bytes,
            Err(error) => {
                show_error(
                    &dialog_play,
                    &trf(
                        "audio_description.project.preview_error",
                        &[("error", error)],
                    ),
                );
                return;
            }
        };
        let preview_dir = match cache_dir("preview") {
            Ok(dir) => dir,
            Err(error) => {
                show_error(&dialog_play, &error);
                return;
            }
        };
        let preview_file = preview_dir.join(format!("preview-{index}.mp3"));
        if let Err(error) = fs::write(&preview_file, bytes) {
            show_error(
                &dialog_play,
                &trf(
                    "audio_description.project.preview_error",
                    &[("error", error.to_string())],
                ),
            );
            let _ = fs::remove_dir_all(&preview_dir);
            return;
        }
        thread::spawn(move || {
            let _ = std::process::Command::new("/usr/bin/afplay")
                .arg(&preview_file)
                .status();
            let _ = fs::remove_dir_all(&preview_dir);
        });
    });

    let project_delete = Rc::clone(&project);
    let path_delete = path.clone();
    let dialog_delete = dialog;
    delete.on_click(move |_| {
        let Some(index) = choice.get_selection().map(|value| value as usize) else {
            show_error(
                &dialog_delete,
                &tr("audio_description.project.no_selection"),
            );
            return;
        };
        if project_delete.borrow().descriptions.len() <= 1 {
            show_error(
                &dialog_delete,
                &tr("audio_description.project.delete_last_error"),
            );
            return;
        }
        if !crate::ask_yes_no_dialog(
            &dialog_delete,
            &tr("audio_description.project.title"),
            &tr("audio_description.project.delete_confirm"),
        ) {
            return;
        }
        {
            let mut mutable = project_delete.borrow_mut();
            mutable.descriptions.remove(index);
            for (new_id, description) in mutable.descriptions.iter_mut().enumerate() {
                description.id = new_id;
            }
            mutable.updated_at_utc = now_utc();
        }
        if let Err(error) = save_project(&path_delete, &project_delete.borrow()) {
            show_error(&dialog_delete, &error);
            return;
        }
        choice.clear();
        for description in &project_delete.borrow().descriptions {
            choice.append(&format!(
                "{} - {}",
                format_mmss(description.source_start_sec),
                description.text
            ));
        }
        let next_index = index.min(project_delete.borrow().descriptions.len() - 1);
        choice.set_selection(next_index as u32);
        if let Some(description) = project_delete.borrow().descriptions.get(next_index) {
            text.set_value(&description.text);
        }
        status.set_label(&tr("audio_description.project.description_deleted"));
    });

    let project_export = Rc::clone(&project);
    let path_export = path.clone();
    let rt_export = Arc::clone(rt);
    let dialog_export = dialog;
    export.on_click(move |_| {
        if let Some(index) = choice.get_selection().map(|value| value as usize) {
            let draft = text.get_value().trim().to_string();
            if project_export
                .borrow()
                .descriptions
                .get(index)
                .is_some_and(|description| description.text != draft)
            {
                show_error(
                    &dialog_export,
                    &tr("audio_description.project.apply_before_export"),
                );
                return;
            }
        }
        let result = run_project_export_with_progress(
            &dialog_export,
            project_export.borrow().clone(),
            path_export.clone(),
            Arc::clone(&rt_export),
        );
        match result {
            Ok(_) => {
                append_podcast_log("audio_description.project.export_completed");
                show_completion(&dialog_export, &tr("audio_description.project.success"));
                append_podcast_log("audio_description.project.editor_closed_after_export");
                dialog_export.end_modal(ID_AUDIO_DESCRIPTION_PROJECT_CLOSE);
            }
            Err(error) if error == "cancelled" => {
                append_podcast_log("audio_description.project.editor_closed_after_cancel");
                dialog_export.end_modal(ID_AUDIO_DESCRIPTION_PROJECT_CLOSE);
            }
            Err(error) => {
                append_podcast_log("audio_description.project.export_failed");
                show_error(&dialog_export, &error);
            }
        }
    });

    dialog.set_escape_id(ID_AUDIO_DESCRIPTION_PROJECT_CLOSE);
    let dialog_close = dialog;
    close.on_click(move |_| {
        append_podcast_log("audio_description.project.close_requested_button");
        dialog_close.end_modal(ID_AUDIO_DESCRIPTION_PROJECT_CLOSE);
    });
    let dialog_window_close = dialog;
    dialog.on_close(move |event| {
        append_podcast_log("audio_description.project.close_requested_window");
        dialog_window_close.end_modal(ID_AUDIO_DESCRIPTION_PROJECT_CLOSE);
        event.skip(false);
    });
    let quit_requested = Rc::new(Cell::new(false));
    let quit_requested_menu = Rc::clone(&quit_requested);
    let dialog_quit = dialog;
    dialog.bind_internal(EventType::MENU, move |event| {
        if event.get_id() == crate::ID_EXIT {
            append_podcast_log("audio_description.project.quit_requested_menu");
            quit_requested_menu.set(true);
            dialog_quit.end_modal(ID_AUDIO_DESCRIPTION_PROJECT_CLOSE);
        } else {
            event.skip(true);
        }
    });
    dialog.show_modal();
    dialog.destroy();
    if quit_requested.get() {
        append_podcast_log("audio_description.project.quit_forwarded_to_main");
        parent.close(false);
    }
}

#[cfg(test)]
mod tests {
    use super::AUDIO_DESCRIPTION_LANGUAGES;
    use std::collections::HashMap;

    const UI_TRANSLATIONS: &[(&str, &str)] = &[
        ("it", include_str!("../i18n/audio_description_it.json")),
        ("en", include_str!("../i18n/audio_description_en.json")),
        ("fr", include_str!("../i18n/audio_description_fr.json")),
        ("es", include_str!("../i18n/audio_description_es.json")),
        ("pt", include_str!("../i18n/audio_description_pt.json")),
        ("cs", include_str!("../i18n/audio_description_cs.json")),
        ("pl", include_str!("../i18n/audio_description_pl.json")),
    ];

    #[test]
    fn every_audio_description_language_name_is_localized() {
        for (ui_language, raw) in UI_TRANSLATIONS {
            let translations: HashMap<String, String> =
                serde_json::from_str(raw).expect("valid audio-description translations");
            for (translation_key, _) in AUDIO_DESCRIPTION_LANGUAGES {
                assert!(
                    translations
                        .get(*translation_key)
                        .is_some_and(|label| !label.trim().is_empty()),
                    "missing {translation_key} for UI language {ui_language}"
                );
            }
        }
    }

    #[test]
    fn italian_language_names_are_displayed_in_italian() {
        let translations: HashMap<String, String> =
            serde_json::from_str(include_str!("../i18n/audio_description_it.json"))
                .expect("valid Italian audio-description translations");
        assert_eq!(
            translations["audio_description.language_name.en"],
            "Inglese"
        );
        assert_eq!(translations["audio_description.language_name.cs"], "Ceco");
    }
}

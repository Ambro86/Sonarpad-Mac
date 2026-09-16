use crate::audio_description_bridge::{
    AudioDescriptionBridgeCallbacks, AudioDescriptionBridgeCheckpoint, AudioDescriptionBridgeRequest,
    AudioDescriptionBridgeResult, AudioDescriptionBridgeResume, AudioDescriptionFixedReanalysisSlot,
    AudioDescriptionOverloadDecision,
    AudioDescriptionPreparedChunk,
    AudioDescriptionQuotaDecision, BridgeCharacter, BridgeDescription, BridgeInterval,
    run_audio_description_bridge,
};
use crate::edge_tts::VoiceInfo;
use crate::{Settings, append_podcast_log};
use hound::{SampleFormat, WavReader, WavSpec, WavWriter};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{Read, Seek, SeekFrom};
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
// Keep prepared clips comfortably below the bridge's 48 MiB inline threshold.
// High-bitrate films can otherwise force every clip through the Gemini Files API,
// which is slower and materially less reliable than inline delivery.
const GEMINI_INLINE_TARGET_CHUNK_BYTES: u64 = 40 * 1024 * 1024;
const GEMINI_MIN_SEGMENT_SECONDS: f64 = 30.0;
const GEMINI_SEGMENT_RETRY_LIMIT: usize = 5;
const GEMINI_COMPAT_TARGET_CHUNK_BYTES: u64 = 15 * 1024 * 1024;
const GEMINI_COMPAT_MIN_SEGMENT_SECONDS: f64 = 10.0;
const GEMINI_COMPAT_SEGMENT_RETRY_LIMIT: usize = 6;
const MAX_SHIFT_SEC: f64 = 5.0;
const MIN_EXTENDED_ANCHOR_SEC: f64 = 1.0;
const MIX_SAMPLE_RATE: u32 = 48_000;
const MIX_CHANNELS: u16 = 2;
const DUCKING_DB: f32 = -12.0;
const FADE_MS: u32 = 280;
const PRE_DUCK_MS: u32 = 180;
const RELEASE_MS: u32 = 600;
const BITRATE_KBPS: u32 = 192;
const PROJECT_FORMAT: &str = "sonarpad-audio-description-project";
const CATALOG_FORMAT: &str = "sonarpad-character-catalog";
const PROJECT_VERSION: u32 = 1;
const AUDIO_DESCRIPTION_PARTIAL_FORMAT: &str = "sonarpad-audio-description-partial";
const AUDIO_DESCRIPTION_PARTIAL_VERSION: u32 = 1;
const EDGE_TRAILING_MIN_REMOVE_MS: u64 = 60;
const EDGE_TRAILING_KEEP_MS: u64 = 30;
const EDGE_TRAILING_SEEK_MS: u64 = 5;
const EDGE_TRAILING_WINDOW_MS: u64 = 60;
const EDGE_AUDIO_DESCRIPTION_PARALLELISM: usize = 8;
const EMPTY_TTS_RETRY_DELAY_MS: u64 = 750;
const MAX_CHARACTER_DESCRIPTION_CHARS: usize = 2_000;
const ID_AUDIO_DESCRIPTION_START: i32 = 7100;
const ID_AUDIO_DESCRIPTION_PROGRESS_CANCEL: i32 = 7101;
const ID_AUDIO_DESCRIPTION_CLOSE: i32 = 7102;
const ID_AUDIO_DESCRIPTION_PROJECT_CLOSE: i32 = 7103;
const ID_AUDIO_DESCRIPTION_RESUME_BROWSE: i32 = 7104;
const CHECKPOINT_SUFFIX: &str = ".sonarpad-ad.partial.json";
const MAX_RECENT_PROJECT_FOLDERS: usize = 8;
const SONARPAD_AI_SERVICE_URL: &str = "https://sonarpad.com/sonarpad-ai";
const SONARPAD_AI_FORCED_GEMINI_MODEL: &str = "gemini-3.8-flash";

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
    recognize_screen_text: bool,
    save_project: bool,
    create_video_output: bool,
    delete_input_after_success: bool,
    keep_character_catalog: bool,
    catalog: Option<CharacterCatalog>,
    tts_engine: String,
    tts_voice: String,
    rate: i32,
    pitch: i32,
    volume: i32,
    audio_stream_index: Option<i32>,
    gemini_api_key: String,
    sonarpad_ai_service_url: String,
    sonarpad_ai_access_code: String,
    sonarpad_ai_device_id: String,
    gemini_model: String,
    fixed_reanalysis_slots: Vec<AudioDescriptionFixedReanalysisSlot>,
    resume_checkpoint_path: Option<PathBuf>,
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

#[derive(Clone, Debug, Serialize, Deserialize)]
struct AudioDescriptionPartialCatalog {
    name: String,
    path: PathBuf,
    #[serde(default)]
    characters: Vec<BridgeCharacter>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct AudioDescriptionPartialCheckpoint {
    format: String,
    version: u32,
    source_path: PathBuf,
    output_mp3_path: PathBuf,
    source_file_size: u64,
    source_duration_sec: f64,
    language_code: String,
    verbosity: String,
    allow_extended_pauses: bool,
    recognize_characters: bool,
    #[serde(default = "default_true")]
    recognize_screen_text: bool,
    save_project: bool,
    #[serde(default)]
    create_video_output: bool,
    #[serde(default)]
    delete_input_after_success: bool,
    keep_character_catalog: bool,
    tts_engine: String,
    tts_voice: String,
    rate: i32,
    pitch: i32,
    volume: i32,
    #[serde(default)]
    audio_stream_index: Option<i32>,
    gemini_model: String,
    character_catalog: Option<AudioDescriptionPartialCatalog>,
    completed_chunks: usize,
    total_chunks: usize,
    #[serde(default)]
    descriptions: Vec<BridgeDescription>,
    #[serde(default)]
    character_glossary: Vec<BridgeCharacter>,
}

#[derive(Clone, Debug)]
struct AudioDescriptionResumeSettings {
    gemini_model: String,
    completed_chunks: usize,
    total_chunks: usize,
}

#[derive(Clone, Debug)]
struct AudioDescriptionResumeSelection {
    checkpoint_path: PathBuf,
    gemini_model: String,
}

#[derive(Clone, Debug)]
struct AudioDescriptionResumeCandidate {
    path: PathBuf,
    label: String,
    modified: SystemTime,
}

#[derive(Clone, Debug)]
struct SynthesizedDescription {
    original_index: usize,
    text: String,
    desired_start_sec: f64,
    visual_start_sec: f64,
    visual_evidence_time_sec: Option<f64>,
    mandatory: bool,
    slot_id: String,
    slot_start_sec: Option<f64>,
    slot_end_sec: Option<f64>,
    pcm: Arc<[i16]>,
    duration_sec: f64,
}

#[derive(Clone, Debug)]
struct AudioDescriptionSynthesisTask {
    original_index: usize,
    synthesis_index: usize,
    text: String,
    desired_start_sec: f64,
    visual_start_sec: f64,
    visual_evidence_time_sec: Option<f64>,
    mandatory: bool,
    slot_id: String,
    slot_start_sec: Option<f64>,
    slot_end_sec: Option<f64>,
}

#[derive(Clone, Debug)]
struct ScheduledDescription {
    original_index: usize,
    text: String,
    desired_start_sec: f64,
    visual_evidence_time_sec: Option<f64>,
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
    original_index: usize,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visual_evidence_time_sec: Option<f64>,
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
    #[serde(default)]
    pub output_is_video: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_stream_index: Option<i32>,
    pub source_duration_sec: f64,
    pub output_duration_sec: f64,
    pub language: String,
    pub language_code: String,
    pub verbosity: String,
    pub allow_extended_pauses: bool,
    #[serde(default = "default_true")]
    pub recognize_characters: bool,
    #[serde(default = "default_true")]
    pub recognize_screen_text: bool,
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
    overload: Option<OverloadUiRequest>,
}

#[derive(Clone)]
struct QuotaUiRequest {
    model: String,
    error: String,
    sender: mpsc::SyncSender<AudioDescriptionQuotaDecision>,
}

#[derive(Clone)]
struct OverloadUiRequest {
    model: String,
    error: String,
    sender: mpsc::SyncSender<AudioDescriptionOverloadDecision>,
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

fn format_sonarpad_balance(balance: f64) -> String {
    let value = format!("{balance:.2}");
    let localized = if Settings::load().ui_language == "en" {
        value
    } else {
        value.replace('.', ",")
    };
    format!("{localized} €")
}

fn fetch_sonarpad_balance(access_code: &str, device_id: &str) -> Result<f64, String> {
    let access_code = access_code.trim();
    let device_id = device_id.trim();
    if !access_code.starts_with("sp_") {
        return Err("invalid Sonarpad AI access code".to_string());
    }
    if device_id.is_empty() {
        return Err("missing Sonarpad AI device identifier".to_string());
    }

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| error.to_string())?;
    let activate_url = format!("{SONARPAD_AI_SERVICE_URL}/v1/activate");
    let activate = client
        .post(&activate_url)
        .json(&serde_json::json!({
            "code": access_code,
            "device_id": device_id,
            "device_name": "Sonarpad Mac",
        }))
        .send()
        .map_err(|error| error.to_string())?;
    if !activate.status().is_success() {
        return Err(format!("HTTP {}", activate.status().as_u16()));
    }
    let activate_json: serde_json::Value = activate.json().map_err(|error| error.to_string())?;
    let session_token = activate_json
        .get("session_token")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    if !session_token.starts_with("sst_") {
        return Err("invalid session response".to_string());
    }

    let account_url = format!("{SONARPAD_AI_SERVICE_URL}/v1/account");
    let account = client
        .get(&account_url)
        .bearer_auth(session_token)
        .send()
        .map_err(|error| error.to_string())?;
    if !account.status().is_success() {
        return Err(format!("HTTP {}", account.status().as_u16()));
    }
    let account_json: serde_json::Value = account.json().map_err(|error| error.to_string())?;
    account_json
        .get("balance_eur")
        .and_then(serde_json::Value::as_f64)
        .ok_or_else(|| "missing balance".to_string())
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

#[derive(Clone, Debug)]
struct AudioTrackInfo {
    stream_index: i32,
    language: Option<String>,
    title: Option<String>,
    codec: String,
    channels: u32,
    is_default: bool,
}

fn ffprobe_path() -> PathBuf {
    let ffmpeg = ffmpeg_path();
    if let Some(parent) = ffmpeg.parent() {
        let candidate = parent.join(if cfg!(windows) { "ffprobe.exe" } else { "ffprobe" });
        if candidate.is_file() {
            return candidate;
        }
    }
    PathBuf::from(if cfg!(windows) { "ffprobe.exe" } else { "ffprobe" })
}

fn parse_audio_channel_layout(layout: &str) -> u32 {
    let lower = layout.trim().to_ascii_lowercase();
    if lower.contains("mono") {
        return 1;
    }
    if lower.contains("stereo") {
        return 2;
    }
    for channels in [16_u32, 12, 10, 8, 7, 6, 5, 4, 3, 2, 1] {
        if lower.contains(&format!("{channels} channels"))
            || lower.contains(&format!("{channels} channel"))
        {
            return channels;
        }
    }
    for (needle, channels) in [
        ("7.1", 8_u32),
        ("6.1", 7),
        ("5.1", 6),
        ("5.0", 5),
        ("4.1", 5),
        ("4.0", 4),
        ("3.1", 4),
        ("3.0", 3),
        ("2.1", 3),
        ("2.0", 2),
        ("1.0", 1),
    ] {
        if lower.contains(needle) {
            return channels;
        }
    }
    0
}

fn list_audio_tracks_from_ffmpeg(input: &Path) -> Result<Vec<AudioTrackInfo>, String> {
    let output = std::process::Command::new(ffmpeg_path())
        .arg("-hide_banner")
        .arg("-i")
        .arg(input)
        .output()
        .map_err(|e| format!("Analisi delle tracce audio fallita: {e}"))?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut tracks = Vec::new();
    let mut current_audio_track: Option<usize> = None;
    for raw_line in stderr.lines() {
        let line = raw_line.trim();
        if let Some(audio_pos) = line.find(": Audio:") {
            let stream_prefix = &line[..audio_pos];
            let Some(hash_pos) = stream_prefix.find('#') else {
                current_audio_track = None;
                continue;
            };
            let stream_spec = &stream_prefix[hash_pos + 1..];
            let Some(colon_pos) = stream_spec.find(':') else {
                current_audio_track = None;
                continue;
            };
            let stream_tail = &stream_spec[colon_pos + 1..];
            let index_digits = stream_tail
                .chars()
                .take_while(|ch| ch.is_ascii_digit())
                .collect::<String>();
            let Ok(stream_index) = index_digits.parse::<i32>() else {
                current_audio_track = None;
                continue;
            };
            let stream_metadata = &stream_tail[index_digits.len()..];
            let language = stream_metadata
                .find('(')
                .and_then(|open| {
                    stream_metadata[open + 1..]
                        .find(')')
                        .map(|close| &stream_metadata[open + 1..open + 1 + close])
                })
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string);
            let audio_detail = line[audio_pos + ": Audio:".len()..].trim();
            let codec = audio_detail
                .split(',')
                .next()
                .unwrap_or("audio")
                .trim()
                .to_string();
            tracks.push(AudioTrackInfo {
                stream_index,
                language,
                title: None,
                codec,
                channels: parse_audio_channel_layout(audio_detail),
                is_default: line.contains("(default)"),
            });
            current_audio_track = Some(tracks.len() - 1);
            continue;
        }
        if line.starts_with("Stream #") {
            current_audio_track = None;
            continue;
        }
        if let Some(track_index) = current_audio_track
            && let Some((key, value)) = line.split_once(':')
            && key.trim().eq_ignore_ascii_case("title")
        {
            let value = value.trim();
            if !value.is_empty() {
                tracks[track_index].title = Some(value.to_string());
            }
        }
    }
    Ok(tracks)
}

fn list_audio_tracks(input: &Path) -> Result<Vec<AudioTrackInfo>, String> {
    let output = std::process::Command::new(ffprobe_path())
        .arg("-v")
        .arg("error")
        .arg("-select_streams")
        .arg("a")
        .arg("-show_entries")
        .arg("stream=index,codec_name,channels:stream_tags=language,title:stream_disposition=default")
        .arg("-of")
        .arg("json")
        .arg(input)
        .output();
    let Ok(output) = output else {
        return list_audio_tracks_from_ffmpeg(input);
    };
    if !output.status.success() {
        return list_audio_tracks_from_ffmpeg(input);
    }
    let value: Value = match serde_json::from_slice(&output.stdout) {
        Ok(value) => value,
        Err(_) => return list_audio_tracks_from_ffmpeg(input),
    };
    let Some(streams) = value.get("streams").and_then(Value::as_array) else {
        return list_audio_tracks_from_ffmpeg(input);
    };
    let tracks = streams
        .iter()
        .filter_map(|stream| {
            let stream_index = stream.get("index")?.as_i64()? as i32;
            let tags = stream.get("tags");
            let language = tags
                .and_then(|tags| tags.get("language"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string);
            let title = tags
                .and_then(|tags| tags.get("title"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string);
            let codec = stream
                .get("codec_name")
                .and_then(Value::as_str)
                .unwrap_or("audio")
                .to_string();
            let channels = stream
                .get("channels")
                .and_then(Value::as_u64)
                .unwrap_or(0) as u32;
            let is_default = stream
                .get("disposition")
                .and_then(|value| value.get("default"))
                .and_then(Value::as_i64)
                .is_some_and(|value| value != 0);
            Some(AudioTrackInfo {
                stream_index,
                language,
                title,
                codec,
                channels,
                is_default,
            })
        })
        .collect::<Vec<_>>();
    Ok(tracks)
}

fn audio_track_label(ordinal: usize, track: &AudioTrackInfo) -> String {
    let mut names = Vec::new();
    if let Some(title) = track.title.as_deref().map(str::trim).filter(|value| !value.is_empty()) {
        names.push(title.to_string());
    }
    if let Some(language) = track
        .language
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        && !names.iter().any(|value| value.eq_ignore_ascii_case(language))
    {
        names.push(language.to_string());
    }
    if names.is_empty() {
        names.push(track.codec.clone());
    }
    let channel_text = if track.channels > 0 {
        format!("; {} ch", track.channels)
    } else {
        String::new()
    };
    format!(
        "{}. {} ({}{})",
        ordinal + 1,
        names.join(" - "),
        track.codec,
        channel_text
    )
}

fn choose_audio_description_track(
    parent: &Dialog,
    input: &Path,
) -> Result<Option<Option<i32>>, String> {
    let tracks = list_audio_tracks(input)?;
    if tracks.len() <= 1 {
        if let Some(track) = tracks.first()
            && track.channels > 2
        {
            append_podcast_log(&format!(
                "audio_description.multichannel_detected stream_index={} codec={} channels={} downmix_to_stereo=true",
                track.stream_index, track.codec, track.channels
            ));
        }
        return Ok(Some(None));
    }

    let default_selection = tracks.iter().position(|track| track.is_default).unwrap_or(0);
    append_podcast_log(&format!(
        "audio_description.multitrack_detected count={} default_selection={}",
        tracks.len(), default_selection
    ));

    let selector = Dialog::builder(parent, &tr("audio_description.audio_track.title"))
        .with_style(DialogStyle::DefaultDialogStyle)
        .with_size(560, 190)
        .build();
    let panel = Panel::builder(&selector).build();
    let root = BoxSizer::builder(Orientation::Vertical).build();
    root.add(
        &StaticText::builder(&panel)
            .with_label(&tr("audio_description.audio_track.label"))
            .build(),
        0,
        SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Top,
        8,
    );
    let choice = Choice::builder(&panel).build();
    choice.set_accessibility_label(&tr("audio_description.audio_track.label"));
    for (index, track) in tracks.iter().enumerate() {
        choice.append(&audio_track_label(index, track));
    }
    choice.set_selection(default_selection as u32);
    root.add(&choice, 0, SizerFlag::Expand | SizerFlag::All, 8);

    let buttons = BoxSizer::builder(Orientation::Horizontal).build();
    let ok = Button::builder(&panel).with_id(ID_OK).with_label("OK").build();
    let cancel = Button::builder(&panel)
        .with_id(ID_CANCEL)
        .with_label(&tr("audio_description.cancel"))
        .build();
    buttons.add(&ok, 0, SizerFlag::All, 8);
    buttons.add(&cancel, 0, SizerFlag::All, 8);
    root.add_sizer(&buttons, 0, SizerFlag::Expand, 0);
    panel.set_sizer(root, true);
    selector.set_affirmative_id(ID_OK);
    selector.set_escape_id(ID_CANCEL);

    let selected_stream = Rc::new(Cell::new(None::<i32>));
    let selected_ok = Rc::clone(&selected_stream);
    let tracks_ok = tracks.clone();
    let selector_ok = selector;
    ok.on_click(move |_| {
        let selected_index = choice.get_selection().unwrap_or(default_selection as u32) as usize;
        let Some(track) = tracks_ok.get(selected_index) else {
            return;
        };
        append_podcast_log(&format!(
            "audio_description.multitrack_selected stream_index={} title={:?} language={:?} codec={} channels={} default={}",
            track.stream_index, track.title, track.language, track.codec, track.channels, track.is_default
        ));
        if track.channels > 2 {
            append_podcast_log(&format!(
                "audio_description.multichannel_detected stream_index={} codec={} channels={} downmix_to_stereo=true",
                track.stream_index, track.codec, track.channels
            ));
        }
        selected_ok.set(Some(track.stream_index));
        selector_ok.end_modal(ID_OK);
    });
    let selector_cancel = selector;
    cancel.on_click(move |_| selector_cancel.end_modal(ID_CANCEL));
    let selector_close = selector;
    selector.on_close(move |event| {
        selector_close.end_modal(ID_CANCEL);
        event.skip(false);
    });
    choice.set_focus();
    selector.show_modal();
    selector.destroy();

    Ok(selected_stream.get().map(Some))
}

#[derive(Clone, Copy, Debug)]
struct MediaProbe {
    duration_sec: f64,
    format_start_sec: f64,
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

fn parse_ffmpeg_progress_duration(value: &str) -> Option<f64> {
    let mut best: Option<f64> = None;
    let mut remainder = value;
    while let Some(position) = remainder.find("time=") {
        remainder = &remainder[position + "time=".len()..];
        let token = remainder
            .split(|ch: char| ch.is_ascii_whitespace())
            .next()
            .unwrap_or_default()
            .trim();
        if token != "N/A"
            && let Some(duration) = parse_ffmpeg_duration(token)
            && best.is_none_or(|current| duration > current)
        {
            best = Some(duration);
        }
        if remainder.is_empty() {
            break;
        }
    }
    best
}

fn probe_media_duration_from_packets(input: &Path) -> Option<f64> {
    let output = std::process::Command::new(ffmpeg_path())
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("info")
        .arg("-i")
        .arg(input)
        .arg("-map")
        .arg("0:v:0?")
        .arg("-map")
        .arg("0:a:0?")
        .arg("-c")
        .arg("copy")
        .arg("-f")
        .arg("null")
        .arg("-")
        .output()
        .ok()?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_ffmpeg_progress_duration(&stderr).or_else(|| parse_ffmpeg_progress_duration(&stdout))
}

fn probe_media(input: &Path) -> Result<MediaProbe, String> {
    let output = std::process::Command::new(ffmpeg_path())
        .arg("-hide_banner")
        .arg("-i")
        .arg(input)
        .output()
        .map_err(|e| format!("Analisi del file multimediale fallita: {e}"))?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    let duration_line = stderr.lines().find(|line| line.contains("Duration:"));
    let metadata_duration = duration_line.and_then(|line| {
        let marker = "Duration:";
        let pos = line.find(marker)?;
        let tail = line[pos + marker.len()..].trim_start();
        let value = tail.split(',').next()?.trim();
        if value == "N/A" {
            None
        } else {
            parse_ffmpeg_duration(value)
        }
    });
    let format_start_sec = duration_line
        .and_then(|line| line.split("start:").nth(1))
        .and_then(|tail| tail.split(',').next())
        .and_then(|value| value.trim().parse::<f64>().ok())
        .filter(|value| value.is_finite())
        .unwrap_or(0.0);
    let duration_sec = if let Some(duration) = metadata_duration {
        duration
    } else if let Some(duration) = probe_media_duration_from_packets(input) {
        append_podcast_log(&format!(
            "audio_description.probe duration_fallback=packet_timestamps path={} duration={:.3}",
            input.display(),
            duration
        ));
        duration
    } else {
        let detail = stderr
            .lines()
            .rev()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("nessun dettaglio FFmpeg")
            .trim();
        return Err(format!(
            "FFmpeg non ha restituito la durata del video. Dettagli: {detail}"
        ));
    };
    let has_audio = stderr.lines().any(|line| line.contains(" Audio:"));
    Ok(MediaProbe {
        duration_sec,
        format_start_sec,
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
    preferred_audio_stream_index: Option<i32>,
    cancel: &Arc<AtomicBool>,
) -> Result<MediaProbe, String> {
    let mut probe = probe_media(input)?;
    let normalized_duration = normalize_audio_description_source_duration(
        input,
        probe.duration_sec,
        probe.format_start_sec,
    );
    if (normalized_duration - probe.duration_sec).abs() > 0.001 {
        append_podcast_log(&format!(
            "audio_description.source_duration_normalized raw={:.3} start_time={:.3} local={:.3} path={}",
            probe.duration_sec,
            probe.format_start_sec,
            normalized_duration,
            input.display()
        ));
        probe.duration_sec = normalized_duration;
    }
    if !probe.has_audio {
        append_podcast_log("audio_description.source has_audio=false; using silent source track");
        write_silent_source_wav(wav, probe.duration_sec)?;
        return Ok(probe);
    }
    let mut args = vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-y".into(),
        "-fflags".into(),
        "+discardcorrupt".into(),
        "-err_detect".into(),
        "ignore_err".into(),
        "-i".into(),
        input.to_string_lossy().to_string(),
    ];
    if let Some(stream_index) = preferred_audio_stream_index {
        args.push("-map".into());
        args.push(format!("0:{stream_index}"));
    }
    args.extend([
        "-vn".into(),
        "-ac".into(),
        MIX_CHANNELS.to_string(),
        "-ar".into(),
        MIX_SAMPLE_RATE.to_string(),
        "-c:a".into(),
        "pcm_s16le".into(),
        wav.to_string_lossy().to_string(),
    ]);
    run_ffmpeg(&args, cancel)?;

    // Keep the normal path byte-for-byte unchanged for files that already work.
    // Only if the resulting internal WAV is malformed, unexpectedly multichannel,
    // or contains an incomplete PCM frame do we re-decode it through a conservative
    // stereo fallback. This protects sources such as problematic 5.1/6.1/7.1 tracks
    // without changing the successful pipeline for ordinary media.
    if source_wav_needs_multichannel_fallback(wav)? {
        let fallback = wav.with_file_name("source-stereo-fallback.wav");
        let _ = fs::remove_file(&fallback);
        append_podcast_log(&format!(
            "audio_description.source_wav_guard fallback_started input={} target={}",
            input.display(),
            fallback.display()
        ));
        repair_source_wav_stereo_fallback(
            input,
            &fallback,
            preferred_audio_stream_index,
            cancel,
        )?;
        if source_wav_needs_multichannel_fallback(&fallback)? {
            let _ = fs::remove_file(&fallback);
            return Err(
                "Fallback audio stereo non riuscito: il WAV interno resta non allineato.".to_string(),
            );
        }
        fs::remove_file(wav)
            .map_err(|e| format!("Sostituzione WAV interno fallita: {e}"))?;
        fs::rename(&fallback, wav)
            .map_err(|e| format!("Installazione fallback WAV stereo fallita: {e}"))?;
        append_podcast_log(
            "audio_description.source_wav_guard fallback_completed stereo=2ch sample_rate=48000",
        );
    }

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

fn wav_pcm_data_alignment(path: &Path) -> Result<Option<bool>, String> {
    let mut file = fs::File::open(path)
        .map_err(|e| format!("Apertura WAV interno fallita: {e}"))?;
    let mut riff = [0u8; 12];
    file.read_exact(&mut riff)
        .map_err(|e| format!("Lettura intestazione WAV fallita: {e}"))?;
    if &riff[8..12] != b"WAVE" {
        return Ok(Some(false));
    }
    let is_rf64 = &riff[0..4] == b"RF64";
    if &riff[0..4] != b"RIFF" && !is_rf64 {
        return Ok(Some(false));
    }

    let mut block_align: Option<u16> = None;
    loop {
        let mut header = [0u8; 8];
        match file.read_exact(&mut header) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(error) => return Err(format!("Lettura chunk WAV fallita: {error}")),
        }
        let chunk_id = &header[0..4];
        let chunk_size = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as u64;

        if chunk_id == b"fmt " {
            if chunk_size < 16 {
                return Ok(Some(false));
            }
            let mut fmt = [0u8; 16];
            file.read_exact(&mut fmt)
                .map_err(|e| format!("Lettura formato WAV fallita: {e}"))?;
            block_align = Some(u16::from_le_bytes([fmt[12], fmt[13]]));
            let remaining = chunk_size.saturating_sub(16);
            if remaining > 0 {
                file.seek(SeekFrom::Current(remaining as i64))
                    .map_err(|e| format!("Scorrimento formato WAV fallito: {e}"))?;
            }
        } else if chunk_id == b"data" {
            let Some(block_align) = block_align.filter(|value| *value > 0) else {
                return Ok(Some(false));
            };
            if is_rf64 && chunk_size == u32::MAX as u64 {
                // RF64 keeps the real data size in ds64. Do not alter a valid long file
                // merely because this lightweight guard cannot resolve that size.
                return Ok(None);
            }
            return Ok(Some(chunk_size % block_align as u64 == 0));
        } else {
            file.seek(SeekFrom::Current(chunk_size as i64))
                .map_err(|e| format!("Scorrimento chunk WAV fallito: {e}"))?;
        }

        if chunk_size % 2 != 0 {
            file.seek(SeekFrom::Current(1))
                .map_err(|e| format!("Scorrimento padding WAV fallito: {e}"))?;
        }
    }
    Ok(Some(false))
}

fn source_wav_needs_multichannel_fallback(path: &Path) -> Result<bool, String> {
    let reader = match WavReader::open(path) {
        Ok(reader) => reader,
        Err(error) => {
            append_podcast_log(&format!(
                "audio_description.source_wav_guard unreadable_wav fallback=stereo_redecode error={error}"
            ));
            return Ok(true);
        }
    };
    let spec = reader.spec();
    if spec.sample_rate != MIX_SAMPLE_RATE
        || spec.channels != MIX_CHANNELS
        || spec.bits_per_sample != 16
        || spec.sample_format != SampleFormat::Int
    {
        append_podcast_log(&format!(
            "audio_description.source_wav_guard format_mismatch sample_rate={} channels={} bits={} sample_format={:?}",
            spec.sample_rate, spec.channels, spec.bits_per_sample, spec.sample_format
        ));
        return Ok(true);
    }
    drop(reader);
    match wav_pcm_data_alignment(path)? {
        Some(true) | None => Ok(false),
        Some(false) => {
            append_podcast_log(
                "audio_description.source_wav_guard frame_alignment_invalid fallback=stereo_redecode",
            );
            Ok(true)
        }
    }
}

fn repair_source_wav_stereo_fallback(
    input: &Path,
    target: &Path,
    preferred_audio_stream_index: Option<i32>,
    cancel: &Arc<AtomicBool>,
) -> Result<(), String> {
    let mut args = vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-y".into(),
        "-fflags".into(),
        "+discardcorrupt".into(),
        "-err_detect".into(),
        "ignore_err".into(),
        "-i".into(),
        input.to_string_lossy().to_string(),
    ];
    if let Some(stream_index) = preferred_audio_stream_index {
        args.push("-map".into());
        args.push(format!("0:{stream_index}"));
    }
    args.extend([
        "-vn".into(),
        "-af".into(),
        "aresample=async=1:first_pts=0".into(),
        "-ac".into(),
        MIX_CHANNELS.to_string(),
        "-ar".into(),
        MIX_SAMPLE_RATE.to_string(),
        "-c:a".into(),
        "pcm_s16le".into(),
        target.to_string_lossy().to_string(),
    ]);
    run_ffmpeg(&args, cancel)
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

fn remove_prepared_chunk_files(dir: &Path, prefix: &str, suffix: &str) -> Result<(), String> {
    let entries = fs::read_dir(dir).map_err(|e| format!("Lettura cartella chunk fallita: {e}"))?;
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        let matches = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with(prefix) && name.ends_with(suffix));
        if matches {
            fs::remove_file(&path).map_err(|e| {
                format!("Pulizia vecchio chunk Gemini fallita ({}): {e}", path.display())
            })?;
        }
    }
    Ok(())
}

fn collect_prepared_chunk_files(
    dir: &Path,
    prefix: &str,
    suffix: &str,
) -> Result<Vec<PathBuf>, String> {
    let mut paths = fs::read_dir(dir)
        .map_err(|e| format!("Lettura cartella chunk fallita: {e}"))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(prefix) && name.ends_with(suffix))
        })
        .collect::<Vec<_>>();
    paths.sort();
    if paths.is_empty() {
        return Err("FFmpeg non ha creato segmenti Gemini.".to_string());
    }
    Ok(paths)
}

fn timestamp_mux_error(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    lower.contains("unknown timestamp")
        || lower.contains("non monotonically increasing")
        || lower.contains("non-monotonous dts")
        || lower.contains("timestamps are unset")
        || lower.contains("error muxing a packet")
        || lower.contains("error submitting a packet to the muxer")
        || lower.contains("invalid argument")
        || lower.contains("invalid data found")
        || lower.contains("corrupt")
        || lower.contains("invalid packet")
}

fn segment_video_for_gemini_transcoded(
    input: &Path,
    dir: &Path,
    segment_seconds: f64,
    preferred_audio_stream_index: Option<i32>,
    cancel: &Arc<AtomicBool>,
) -> Result<Vec<PathBuf>, String> {
    let extension = "mkv";
    let prefix = "gemini_chunk_";
    let suffix = format!(".{extension}");
    remove_prepared_chunk_files(dir, prefix, &suffix)?;

    let output_pattern = dir.join(format!("{prefix}%04d.{extension}"));
    let args = vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-y".into(),
        "-fflags".into(),
        "+genpts+discardcorrupt".into(),
        "-err_detect".into(),
        "ignore_err".into(),
        "-i".into(),
        input.to_string_lossy().to_string(),
        "-map".into(),
        "0:v:0".into(),
        "-map".into(),
        preferred_audio_stream_index
            .map(|stream_index| format!("0:{stream_index}?"))
            .unwrap_or_else(|| "0:a?".to_string()),
        "-sn".into(),
        "-dn".into(),
        "-c:v".into(),
        "mpeg4".into(),
        "-q:v".into(),
        "5".into(),
        "-vf".into(),
        "scale=trunc(iw/2)*2:trunc(ih/2)*2,format=yuv420p".into(),
        "-c:a".into(),
        "aac".into(),
        "-b:a".into(),
        "128k".into(),
        "-avoid_negative_ts".into(),
        "make_zero".into(),
        "-f".into(),
        "segment".into(),
        "-segment_time".into(),
        format!("{segment_seconds:.3}"),
        "-segment_start_number".into(),
        "1".into(),
        "-reset_timestamps".into(),
        "1".into(),
        "-segment_format".into(),
        "matroska".into(),
        output_pattern.to_string_lossy().to_string(),
    ];
    run_ffmpeg(&args, cancel)?;
    collect_prepared_chunk_files(dir, prefix, &suffix)
}

fn segment_video_for_gemini(
    input: &Path,
    dir: &Path,
    segment_seconds: f64,
    preferred_audio_stream_index: Option<i32>,
    cancel: &Arc<AtomicBool>,
) -> Result<Vec<PathBuf>, String> {
    let extension = "mkv";
    let prefix = "gemini_chunk_";
    let suffix = format!(".{extension}");
    remove_prepared_chunk_files(dir, prefix, &suffix)?;

    let output_pattern = dir.join(format!("{prefix}%04d.{extension}"));
    // +genpts fixes a large class of AVI/legacy-container files without re-encoding.
    // Some damaged AVI files still expose packets with no usable timestamp at all;
    // in that case stream-copy into Matroska is impossible and we transparently
    // normalize the media below instead of surfacing an FFmpeg error to the user.
    let args = vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-y".into(),
        "-fflags".into(),
        "+genpts+discardcorrupt".into(),
        "-err_detect".into(),
        "ignore_err".into(),
        "-i".into(),
        input.to_string_lossy().to_string(),
        "-map".into(),
        "0:v:0".into(),
        "-map".into(),
        preferred_audio_stream_index
            .map(|stream_index| format!("0:{stream_index}?"))
            .unwrap_or_else(|| "0:a?".to_string()),
        "-sn".into(),
        "-dn".into(),
        "-c".into(),
        "copy".into(),
        "-avoid_negative_ts".into(),
        "make_zero".into(),
        "-f".into(),
        "segment".into(),
        "-segment_time".into(),
        format!("{segment_seconds:.3}"),
        "-segment_start_number".into(),
        "1".into(),
        "-reset_timestamps".into(),
        "1".into(),
        "-segment_format".into(),
        "matroska".into(),
        output_pattern.to_string_lossy().to_string(),
    ];
    match run_ffmpeg(&args, cancel) {
        Ok(()) => collect_prepared_chunk_files(dir, prefix, &suffix),
        Err(error) if error == "cancelled" => Err(error),
        Err(error) if timestamp_mux_error(&error) => {
            append_podcast_log(&format!(
                "audio_description.chunk_timestamp_recovery mode=transcode input={} segment_sec={:.3} reason={}",
                input.display(),
                segment_seconds,
                error.replace('\n', " ")
            ));
            segment_video_for_gemini_transcoded(
                input,
                dir,
                segment_seconds,
                preferred_audio_stream_index,
                cancel,
            )
            .map_err(
                |fallback_error| {
                    format!(
                        "FFmpeg non è riuscito a normalizzare i timestamp del video. Primo tentativo: {error}\nFallback: {fallback_error}"
                    )
                },
            )
        }
        Err(error) => Err(error),
    }
}

fn normalize_audio_description_source_duration(
    path: &Path,
    measured_duration_sec: f64,
    format_start_sec: f64,
) -> f64 {
    if !measured_duration_sec.is_finite() || measured_duration_sec <= 0.0 {
        return measured_duration_sec;
    }
    let is_matroska = path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("mkv") || extension.eq_ignore_ascii_case("webm")
        });
    if !is_matroska
        || !format_start_sec.is_finite()
        || format_start_sec <= CHUNK_SECONDS
        || measured_duration_sec <= format_start_sec
    {
        return measured_duration_sec;
    }
    let local_span = measured_duration_sec - format_start_sec;
    if local_span > 0.001 {
        local_span
    } else {
        measured_duration_sec
    }
}

fn normalize_prepared_chunk_duration(
    measured_duration_sec: f64,
    format_start_sec: f64,
    segment_seconds: f64,
) -> f64 {
    let expected = segment_seconds.max(1.0);
    if !measured_duration_sec.is_finite() || measured_duration_sec <= 0.0 {
        return expected;
    }
    if format_start_sec.is_finite()
        && format_start_sec > expected * 4.0
        && measured_duration_sec > format_start_sec
    {
        let local_span = measured_duration_sec - format_start_sec;
        if local_span > 0.001 && local_span <= expected * 4.0 {
            return local_span;
        }
    }
    measured_duration_sec
}

fn build_prepared_chunk_timeline(
    measured_chunks: &[(PathBuf, f64)],
    duration_sec: f64,
    reconcile_small_drift: bool,
) -> Option<Vec<AudioDescriptionPreparedChunk>> {
    if measured_chunks.is_empty() || !duration_sec.is_finite() || duration_sec <= 0.0 {
        return None;
    }
    let scale = if reconcile_small_drift {
        let measured_total = measured_chunks.iter().map(|(_, value)| *value).sum::<f64>();
        if !measured_total.is_finite() || measured_total <= duration_sec {
            return None;
        }
        let excess_ratio = (measured_total - duration_sec) / duration_sec;
        if !excess_ratio.is_finite() || excess_ratio <= 0.0 || excess_ratio > 0.02 {
            return None;
        }
        duration_sec / measured_total
    } else {
        1.0
    };

    let mut result = Vec::with_capacity(measured_chunks.len());
    let mut cursor = 0.0_f64;
    for (index, (path, measured)) in measured_chunks.iter().enumerate() {
        if !measured.is_finite() || *measured <= 0.0 {
            return None;
        }
        let start_sec = cursor;
        let end_sec = if index + 1 == measured_chunks.len() {
            duration_sec
        } else {
            (start_sec + measured * scale).min(duration_sec)
        };
        if !end_sec.is_finite() || end_sec <= start_sec {
            return None;
        }
        result.push(AudioDescriptionPreparedChunk {
            path: path.to_string_lossy().to_string(),
            start_sec,
            end_sec,
        });
        if index + 1 < measured_chunks.len() {
            cursor = end_sec;
        }
    }
    Some(result)
}

fn gemini_media_invalid_argument(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    let invalid = lower.contains("invalid_argument")
        || lower.contains("invalid argument")
        || lower.contains("invalid value");
    let http_400 = lower.contains("400")
        || lower.contains("code': 400")
        || lower.contains("\"code\":400");
    let credentials = lower.contains("api key")
        || lower.contains("api_key")
        || lower.contains("permission denied")
        || lower.contains("unauthenticated");
    http_400 && invalid && !credentials
}

fn gemini_media_processing_failed(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    let media_failure = (lower.contains("video processing failed on gemini's servers")
        && lower.contains("final state: failed"))
        || lower.contains("file_verification_failed");
    let credentials = lower.contains("api key")
        || lower.contains("api_key")
        || lower.contains("permission denied")
        || lower.contains("unauthenticated")
        || lower.contains("invalid_session");
    media_failure && !credentials
}

fn segment_video_for_gemini_compatibility(
    input: &Path,
    dir: &Path,
    segment_seconds: f64,
    preferred_audio_stream_index: Option<i32>,
    extension: &str,
    video_only_on_any_mux_error: bool,
    cancel: &Arc<AtomicBool>,
) -> Result<Vec<PathBuf>, String> {
    let extension = extension.trim_start_matches('.').to_ascii_lowercase();
    if extension != "mkv" && extension != "mp4" {
        return Err(format!("Contenitore di compatibilità Gemini non supportato: {extension}"));
    }
    fs::create_dir_all(dir).map_err(|e| format!("Creazione cache Gemini fallita: {e}"))?;
    let prefix = "gemini_compat_";
    let suffix = format!(".{extension}");
    remove_prepared_chunk_files(dir, prefix, &suffix)?;
    let output_pattern = dir.join(format!("{prefix}%04d.{extension}"));

    let make_args = |video_only: bool, transcode: bool| {
        let mut args = vec![
            "-hide_banner".into(), "-loglevel".into(), "error".into(), "-y".into(),
            "-fflags".into(), "+genpts+discardcorrupt".into(), "-err_detect".into(),
            "ignore_err".into(), "-i".into(), input.to_string_lossy().to_string(),
            "-map".into(), "0:v:0".into(),
        ];
        if video_only {
            args.push("-an".into());
        } else {
            args.push("-map".into());
            args.push(preferred_audio_stream_index
                .map(|stream_index| format!("0:{stream_index}?"))
                .unwrap_or_else(|| "0:a?".to_string()));
        }
        args.extend(["-sn".into(), "-dn".into()]);
        if transcode {
            args.extend([
                "-c:v".into(), "libx264".into(), "-preset".into(), "veryfast".into(),
                "-crf".into(), "23".into(), "-pix_fmt".into(), "yuv420p".into(),
            ]);
            if !video_only {
                args.extend(["-c:a".into(), "aac".into(), "-b:a".into(), "128k".into()]);
            }
        } else {
            args.extend(["-c".into(), "copy".into()]);
        }
        args.extend([
            "-avoid_negative_ts".into(), "make_zero".into(), "-f".into(), "segment".into(),
            "-segment_time".into(), format!("{segment_seconds:.3}"),
            "-segment_start_number".into(), "1".into(), "-reset_timestamps".into(), "1".into(),
            "-segment_format".into(), if extension == "mkv" { "matroska".into() } else { "mp4".into() },
            output_pattern.to_string_lossy().to_string(),
        ]);
        args
    };

    match run_ffmpeg(&make_args(false, false), cancel) {
        Ok(()) => collect_prepared_chunk_files(dir, prefix, &suffix),
        Err(error) if error == "cancelled" => Err(error),
        Err(primary_error) => {
            let may_drop_audio = video_only_on_any_mux_error || timestamp_mux_error(&primary_error);
            if may_drop_audio {
                remove_prepared_chunk_files(dir, prefix, &suffix)?;
                match run_ffmpeg(&make_args(true, false), cancel) {
                    Ok(()) => return collect_prepared_chunk_files(dir, prefix, &suffix),
                    Err(error) if error == "cancelled" => return Err(error),
                    Err(_) => {}
                }
            }
            remove_prepared_chunk_files(dir, prefix, &suffix)?;
            run_ffmpeg(&make_args(false, true), cancel).map_err(|fallback_error| {
                format!(
                    "Preparazione compatibilità Gemini fallita: {primary_error}; fallback transcodifica: {fallback_error}"
                )
            })?;
            collect_prepared_chunk_files(dir, prefix, &suffix)
        }
    }
}

fn prepare_gemini_compatibility_chunks(
    input: &Path,
    duration_sec: f64,
    dir: &Path,
    preferred_audio_stream_index: Option<i32>,
    cancel: &Arc<AtomicBool>,
    extension: &str,
    video_only_on_any_mux_error: bool,
) -> Result<Vec<AudioDescriptionPreparedChunk>, String> {
    if dir.exists() {
        fs::remove_dir_all(dir).map_err(|e| format!("Pulizia cache compatibilità Gemini fallita: {e}"))?;
    }
    fs::create_dir_all(dir).map_err(|e| format!("Creazione cache compatibilità Gemini fallita: {e}"))?;
    let mut segment_seconds = duration_sec
        .ceil()
        .max(1.0)
        .min(CHUNK_SECONDS);
    let mut attempt = 1usize;

    let paths = loop {
        let paths = segment_video_for_gemini_compatibility(
            input,
            dir,
            segment_seconds,
            preferred_audio_stream_index,
            extension,
            video_only_on_any_mux_error,
            cancel,
        )?;
        let max_chunk_bytes = paths
            .iter()
            .filter_map(|path| fs::metadata(path).ok().map(|metadata| metadata.len()))
            .max()
            .unwrap_or(0);
        if max_chunk_bytes == 0 {
            return Err("FFmpeg ha creato un segmento di compatibilità Gemini vuoto.".to_string());
        }
        if max_chunk_bytes <= GEMINI_COMPAT_TARGET_CHUNK_BYTES {
            break paths;
        }
        if attempt >= GEMINI_COMPAT_SEGMENT_RETRY_LIMIT
            || segment_seconds <= GEMINI_COMPAT_MIN_SEGMENT_SECONDS
        {
            return Err(format!(
                "I segmenti di compatibilità Gemini restano troppo grandi: {:.1} MB",
                max_chunk_bytes as f64 / (1024.0 * 1024.0)
            ));
        }
        let ratio = GEMINI_COMPAT_TARGET_CHUNK_BYTES as f64 / max_chunk_bytes as f64;
        let proposed = (segment_seconds * ratio * 0.80).floor();
        segment_seconds = proposed
            .max(GEMINI_COMPAT_MIN_SEGMENT_SECONDS)
            .min((segment_seconds - 1.0).max(GEMINI_COMPAT_MIN_SEGMENT_SECONDS));
        attempt += 1;
    };

    let mut measured_chunks = Vec::with_capacity(paths.len());
    for path in paths {
        let probe = probe_media(&path)?;
        let measured = normalize_prepared_chunk_duration(
            probe.duration_sec,
            probe.format_start_sec,
            segment_seconds,
        )
        .max(0.001);
        measured_chunks.push((path, measured));
    }
    if let Some(chunks) = build_prepared_chunk_timeline(&measured_chunks, duration_sec, false) {
        return Ok(chunks);
    }
    if let Some(chunks) = build_prepared_chunk_timeline(&measured_chunks, duration_sec, true) {
        return Ok(chunks);
    }
    Err("Timeline dei segmenti di compatibilità Gemini non valida.".to_string())
}

fn prepare_chunks(
    input: &Path,
    duration: f64,
    dir: &Path,
    preferred_audio_stream_index: Option<i32>,
    cancel: &Arc<AtomicBool>,
) -> Result<Vec<AudioDescriptionPreparedChunk>, String> {
    let input_size = fs::metadata(input)
        .map_err(|e| format!("Lettura dimensione file fallita: {e}"))?
        .len();
    if input_size == 0 {
        return Err(format!(
            "Il file Gemini è vuoto: {}",
            input.display()
        ));
    }
    if input_size >= GEMINI_MAX_CHUNK_BYTES {
        append_podcast_log(&format!(
            "audio_description.large_source_chunking path={} size_mb={:.1} duration_sec={:.3}",
            input.display(),
            input_size as f64 / (1024.0 * 1024.0),
            duration
        ));
    }
    if preferred_audio_stream_index.is_none()
        && duration <= CHUNK_SECONDS
        && input_size <= GEMINI_INLINE_TARGET_CHUNK_BYTES
    {
        return Ok(vec![AudioDescriptionPreparedChunk {
            path: input.to_string_lossy().to_string(),
            start_sec: 0.0,
            end_sec: duration,
        }]);
    }

    if cancel.load(Ordering::Relaxed) {
        return Err("cancelled".to_string());
    }

    // Start with three-minute stream-copy clips. If the source bitrate makes
    // those clips too large for reliable inline Gemini delivery, automatically
    // shorten the segment duration and redo the cheap stream-copy split. This
    // preserves the source frames/audio while avoiding a fragile Files API path.
    let mut segment_seconds = CHUNK_SECONDS.min(duration.max(GEMINI_MIN_SEGMENT_SECONDS));
    // Estimate a useful first segment duration from the full-file average bitrate.
    // This matters especially for multi-gigabyte movies: they must be chunked, not
    // rejected, and starting near the inline target avoids writing a complete set
    // of oversized temporary clips only to delete and split them again.
    if input_size > GEMINI_INLINE_TARGET_CHUNK_BYTES {
        let ratio = GEMINI_INLINE_TARGET_CHUNK_BYTES as f64 / input_size as f64;
        let estimated = (duration * ratio * 0.82)
            .max(GEMINI_MIN_SEGMENT_SECONDS)
            .min(segment_seconds);
        if estimated < segment_seconds - 0.5 {
            append_podcast_log(&format!(
                "audio_description.chunk_initial_estimate old_segment_sec={:.3} new_segment_sec={:.3} source_size_mb={:.1} target_mb={:.1}",
                segment_seconds,
                estimated,
                input_size as f64 / (1024.0 * 1024.0),
                GEMINI_INLINE_TARGET_CHUNK_BYTES as f64 / (1024.0 * 1024.0)
            ));
            segment_seconds = estimated;
        }
    }
    let mut attempt = 0_usize;
    let paths = loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("cancelled".to_string());
        }
        attempt += 1;
        let paths = segment_video_for_gemini(
            input,
            dir,
            segment_seconds,
            preferred_audio_stream_index,
            cancel,
        )?;
        let max_size = paths
            .iter()
            .filter_map(|path| fs::metadata(path).ok().map(|metadata| metadata.len()))
            .max()
            .unwrap_or(0);
        if max_size == 0 {
            return Err("FFmpeg ha creato un chunk Gemini vuoto.".to_string());
        }
        if max_size <= GEMINI_INLINE_TARGET_CHUNK_BYTES {
            if attempt > 1 {
                append_podcast_log(&format!(
                    "audio_description.chunk_adaptive_ready attempts={} segment_sec={:.3} max_size_mb={:.1}",
                    attempt,
                    segment_seconds,
                    max_size as f64 / (1024.0 * 1024.0)
                ));
            }
            break paths;
        }

        if attempt >= GEMINI_SEGMENT_RETRY_LIMIT {
            append_podcast_log(&format!(
                "audio_description.chunk_adaptive_fallback attempts={} segment_sec={:.3} max_size_mb={:.1}",
                attempt,
                segment_seconds,
                max_size as f64 / (1024.0 * 1024.0)
            ));
            break paths;
        }

        let previous = segment_seconds;
        let ratio = GEMINI_INLINE_TARGET_CHUNK_BYTES as f64 / max_size as f64;
        let proposed = (segment_seconds * ratio * 0.82).max(GEMINI_MIN_SEGMENT_SECONDS);
        if proposed >= previous - 0.5 {
            append_podcast_log(&format!(
                "audio_description.chunk_adaptive_fallback reason=min_segment segment_sec={:.3} max_size_mb={:.1}",
                previous,
                max_size as f64 / (1024.0 * 1024.0)
            ));
            break paths;
        }
        segment_seconds = proposed;
        append_podcast_log(&format!(
            "audio_description.chunk_adaptive_retry attempt={} old_segment_sec={:.3} new_segment_sec={:.3} max_size_mb={:.1} target_mb={:.1}",
            attempt,
            previous,
            segment_seconds,
            max_size as f64 / (1024.0 * 1024.0),
            GEMINI_INLINE_TARGET_CHUNK_BYTES as f64 / (1024.0 * 1024.0)
        ));
    };

    let path_count = paths.len();
    let mut measured_chunks = Vec::with_capacity(path_count);
    for path in paths {
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
        let probe = probe_media(&path).unwrap_or(MediaProbe {
            duration_sec: segment_seconds,
            format_start_sec: 0.0,
            has_audio: false,
        });
        let measured = normalize_prepared_chunk_duration(
            probe.duration_sec,
            probe.format_start_sec,
            segment_seconds,
        )
        .max(0.001);
        measured_chunks.push((path, measured));
    }
    if let Some(chunks) = build_prepared_chunk_timeline(&measured_chunks, duration, false) {
        return Ok(chunks);
    }
    let measured_total = measured_chunks.iter().map(|(_, value)| *value).sum::<f64>();
    let excess_ratio = if duration > 0.0 {
        (measured_total - duration) / duration
    } else {
        f64::INFINITY
    };
    if measured_total.is_finite()
        && measured_total > duration
        && excess_ratio.is_finite()
        && excess_ratio > 0.0
        && excess_ratio <= 0.02
    {
        append_podcast_log(&format!(
            "audio_description.chunk_timeline_reconcile measured_total={:.3} source_duration={:.3} excess_pct={:.3}",
            measured_total,
            duration,
            excess_ratio * 100.0
        ));
        if let Some(chunks) = build_prepared_chunk_timeline(&measured_chunks, duration, true) {
            return Ok(chunks);
        }
    }
    Err("Timeline dei chunk Gemini non valida.".to_string())
}

fn convert_mp3_bytes_to_pcm(
    bytes: &[u8],
    dir: &Path,
    index: usize,
    cancel: &Arc<AtomicBool>,
) -> Result<Vec<i16>, String> {
    if bytes.is_empty() {
        return Err("TTS audio vuoto: payload MP3 senza dati.".to_string());
    }

    let mp3 = dir.join(format!("tts_{index:04}.mp3"));
    let wav = dir.join(format!("tts_{index:04}.wav"));
    let cleanup = || {
        let _ = fs::remove_file(&mp3);
        let _ = fs::remove_file(&wav);
    };

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
    if let Err(error) = run_ffmpeg(&args, cancel) {
        cleanup();
        return Err(error);
    }

    let wav_len = fs::metadata(&wav).map(|metadata| metadata.len()).unwrap_or(0);
    if wav_len <= 44 {
        cleanup();
        return Err(format!("TTS WAV vuoto: {wav_len} byte."));
    }

    let reader = match WavReader::open(&wav) {
        Ok(reader) => reader,
        Err(error) if wav_len <= 128 => {
            cleanup();
            return Err(format!(
                "TTS WAV vuoto o troppo piccolo ({wav_len} byte): {error}"
            ));
        }
        Err(error) => {
            cleanup();
            return Err(format!("TTS WAV non leggibile: {error}"));
        }
    };
    let samples: Result<Vec<i16>, _> = reader.into_samples::<i16>().collect();
    let samples = match samples {
        Ok(samples) => samples,
        Err(error) => {
            cleanup();
            return Err(format!("Lettura TTS WAV fallita: {error}"));
        }
    };
    cleanup();

    if samples.is_empty() {
        return Err("TTS WAV vuoto: nessun campione PCM.".to_string());
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

fn audio_description_pcm_has_signal(samples: &[i16]) -> bool {
    samples.iter().any(|sample| i32::from(*sample).abs() > 0)
}

fn audio_description_tts_error_is_empty_output(error: &str) -> bool {
    let normalized = error.to_ascii_lowercase();
    normalized.contains("tts audio vuoto")
        || normalized.contains("tts wav vuoto")
        || normalized.contains("audio vuoto")
        || normalized.contains("nessun campione pcm")
}

fn wait_for_empty_tts_retry(cancel: &AtomicBool) -> Result<(), String> {
    const POLL_DELAY_MS: u64 = 75;
    let mut waited_ms = 0_u64;
    while waited_ms < EMPTY_TTS_RETRY_DELAY_MS {
        if cancel.load(Ordering::Relaxed) {
            return Err("cancelled".to_string());
        }
        let remaining_ms = EMPTY_TTS_RETRY_DELAY_MS.saturating_sub(waited_ms);
        let sleep_ms = remaining_ms.min(POLL_DELAY_MS);
        thread::sleep(Duration::from_millis(sleep_ms));
        waited_ms = waited_ms.saturating_add(sleep_ms);
    }
    Ok(())
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
    let spoken_text = crate::apply_voice_dictionary_to_text(text);
    if spoken_text != text {
        append_podcast_log(&format!(
            "audio_description.voice_dictionary_applied cue={} original_chars={} spoken_chars={}",
            index,
            text.chars().count(),
            spoken_text.chars().count()
        ));
    }

    let mut empty_attempt = 0_u64;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("cancelled".to_string());
        }

        let mp3 = match crate::synthesize_voice_chunk_blocking(
            tts.engine,
            &spoken_text,
            tts.voice,
            tts.rate,
            tts.pitch,
            tts.volume,
            rt,
        ) {
            Ok(audio) => audio,
            Err(error) => {
                if cancel.load(Ordering::Relaxed) {
                    return Err("cancelled".to_string());
                }
                if audio_description_tts_error_is_empty_output(&error) {
                    empty_attempt = empty_attempt.saturating_add(1);
                    append_podcast_log(&format!(
                        "audio_description.tts_empty_retry cue={} attempt={} stage=renderer error={}",
                        index, empty_attempt, error
                    ));
                    wait_for_empty_tts_retry(cancel.as_ref())?;
                    continue;
                }
                return Err(error);
            }
        };

        if mp3.is_empty() {
            empty_attempt = empty_attempt.saturating_add(1);
            append_podcast_log(&format!(
                "audio_description.tts_empty_retry cue={} attempt={} stage=mp3_payload",
                index, empty_attempt
            ));
            wait_for_empty_tts_retry(cancel.as_ref())?;
            continue;
        }

        let mut pcm = match convert_mp3_bytes_to_pcm(&mp3, dir, index, cancel) {
            Ok(pcm) => pcm,
            Err(error) if audio_description_tts_error_is_empty_output(&error) => {
                empty_attempt = empty_attempt.saturating_add(1);
                append_podcast_log(&format!(
                    "audio_description.tts_empty_retry cue={} attempt={} stage=wav_decode error={}",
                    index, empty_attempt, error
                ));
                wait_for_empty_tts_retry(cancel.as_ref())?;
                continue;
            }
            Err(error) => return Err(error),
        };

        if !crate::is_system_voice_engine(tts.engine) {
            let removed = trim_edge_trailing_silence(&mut pcm);
            if removed > 0 {
                append_podcast_log(&format!(
                    "audio_description.edge_trim cue={} removed_samples={}",
                    index, removed
                ));
            }
        }

        if pcm.is_empty() || !audio_description_pcm_has_signal(&pcm) {
            empty_attempt = empty_attempt.saturating_add(1);
            append_podcast_log(&format!(
                "audio_description.tts_empty_retry cue={} attempt={} stage=pcm_signal",
                index, empty_attempt
            ));
            wait_for_empty_tts_retry(cancel.as_ref())?;
            continue;
        }

        return Ok(Arc::from(pcm));
    }
}

fn audio_description_tts_parallelism(engine: &str, task_count: usize) -> usize {
    let requested = if crate::is_system_voice_engine(engine) {
        1
    } else {
        EDGE_AUDIO_DESCRIPTION_PARALLELISM
    };
    requested.min(task_count).max(1)
}

fn synthesize_description_tasks_parallel<F>(
    tasks: &[AudioDescriptionSynthesisTask],
    tts: TtsParameters<'_>,
    rt: &Runtime,
    dir: &Path,
    cancel: Arc<AtomicBool>,
    mut on_completed: F,
) -> Result<Vec<SynthesizedDescription>, String>
where
    F: FnMut(usize, usize),
{
    if tasks.is_empty() {
        return Ok(Vec::new());
    }

    let parallelism = audio_description_tts_parallelism(tts.engine, tasks.len());
    append_podcast_log(&format!(
        "audio_description.parallel_tts engine={} descriptions={} concurrency={}",
        tts.engine,
        tasks.len(),
        parallelism
    ));

    let mut synthesized = Vec::with_capacity(tasks.len());
    let mut completed = 0_usize;

    for batch_start in (0..tasks.len()).step_by(parallelism) {
        if cancel.load(Ordering::Relaxed) {
            return Err("cancelled".to_string());
        }
        let batch_end = (batch_start + parallelism).min(tasks.len());
        let batch = &tasks[batch_start..batch_end];

        let batch_results = thread::scope(|scope| {
            let mut handles = Vec::with_capacity(batch.len());
            for task in batch {
                let cancel = cancel.clone();
                handles.push(scope.spawn(move || {
                    let pcm = synthesize_text_pcm(
                        &task.text,
                        tts,
                        rt,
                        dir,
                        task.synthesis_index,
                        &cancel,
                    )?;
                    let duration_sec =
                        pcm.len() as f64 / (MIX_CHANNELS as f64 * MIX_SAMPLE_RATE as f64);
                    Ok::<SynthesizedDescription, String>(SynthesizedDescription {
                        original_index: task.original_index,
                        text: task.text.clone(),
                        desired_start_sec: task.desired_start_sec,
                        visual_start_sec: task.visual_start_sec,
                        visual_evidence_time_sec: task.visual_evidence_time_sec,
                        mandatory: task.mandatory,
                        slot_id: task.slot_id.clone(),
                        slot_start_sec: task.slot_start_sec,
                        slot_end_sec: task.slot_end_sec,
                        pcm,
                        duration_sec,
                    })
                }));
            }

            handles
                .into_iter()
                .map(|handle| {
                    handle.join().unwrap_or_else(|_| {
                        Err("Audio description: parallel TTS worker panicked".to_string())
                    })
                })
                .collect::<Vec<_>>()
        });

        for result in batch_results {
            if cancel.load(Ordering::Relaxed) {
                return Err("cancelled".to_string());
            }
            synthesized.push(result?);
            completed = completed.saturating_add(1);
            on_completed(completed, tasks.len());
        }
    }

    synthesized.sort_by_key(|description| description.original_index);
    Ok(synthesized)
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

fn choose_slot(
    free: &[(f64, f64)],
    desired_start: f64,
    visual_start: f64,
    required_duration: f64,
) -> Option<f64> {
    let visual_lower = (visual_start - MAX_SHIFT_SEC).max(0.0);
    let visual_upper = visual_start + MAX_SHIFT_SEC;
    free.iter()
        .filter_map(|&(gap_start, gap_end)| {
            let lower = gap_start.max(visual_lower);
            let upper = (gap_end - required_duration).min(visual_upper);
            if upper < lower {
                return None;
            }
            let start = desired_start.clamp(lower, upper);
            let distance_from_visual_origin = (start - visual_start).abs();
            (distance_from_visual_origin <= MAX_SHIFT_SEC + f64::EPSILON)
                .then_some(((start - desired_start).abs(), start))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, start)| start)
}

fn choose_pause_anchor(
    free: &[(f64, f64)],
    desired_start: f64,
    visual_start: f64,
) -> Option<f64> {
    let visual_lower = (visual_start - MAX_SHIFT_SEC).max(0.0);
    let visual_upper = visual_start + MAX_SHIFT_SEC;
    free.iter()
        .filter_map(|&(gap_start, gap_end)| {
            let lower = gap_start.max(visual_lower);
            let upper = (gap_end - MIN_EXTENDED_ANCHOR_SEC).min(visual_upper);
            if upper < lower {
                return None;
            }
            let start = desired_start.clamp(lower, upper);
            let distance_from_visual_origin = (start - visual_start).abs();
            (distance_from_visual_origin <= MAX_SHIFT_SEC + f64::EPSILON)
                .then_some(((start - desired_start).abs(), start))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, start)| start)
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
        if let Some(start) = choose_slot(
            &candidates,
            d.desired_start_sec,
            d.visual_start_sec,
            d.duration_sec.max(0.001),
        ) {
            reserved.push((start, start + d.duration_sec));
            scheduled.push(ScheduledDescription {
                original_index: d.original_index,
                text: d.text,
                desired_start_sec: d.visual_start_sec,
                visual_evidence_time_sec: d.visual_evidence_time_sec,
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
            let anchor = choose_pause_anchor(
                &candidates,
                d.desired_start_sec,
                d.visual_start_sec,
            );
            if let Some(start) = anchor {
                reserved.push((start, start + MIN_EXTENDED_ANCHOR_SEC));
                scheduled.push(ScheduledDescription {
                    original_index: d.original_index,
                    text: d.text,
                    desired_start_sec: d.visual_start_sec,
                    visual_evidence_time_sec: d.visual_evidence_time_sec,
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
            original_index: d.original_index,
            text: d.text,
            desired_start_sec: d.visual_start_sec,
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

fn smooth_duck_fade(position: f32) -> f32 {
    let position = position.clamp(0.0, 1.0);
    0.5 - 0.5 * (std::f32::consts::PI * position).cos()
}

fn merge_duck_intervals(mut intervals: Vec<(u64, u64)>, join_gap: u64) -> Vec<(u64, u64)> {
    if intervals.is_empty() {
        return intervals;
    }
    intervals.sort_by_key(|interval| interval.0);
    let mut merged: Vec<(u64, u64)> = Vec::with_capacity(intervals.len());
    for (start, end) in intervals {
        if let Some(last) = merged.last_mut()
            && start <= last.1.saturating_add(join_gap)
        {
            last.1 = last.1.max(end);
            continue;
        }
        merged.push((start, end));
    }
    merged
}

fn duck_gain_for(
    frame: u64,
    intervals: &[(u64, u64)],
    cursor: &mut usize,
    attack_frames: u64,
    preduck_frames: u64,
    release_frames: u64,
) -> f32 {
    let duck = 10f32.powf(DUCKING_DB / 20.0);
    while let Some((_, end)) = intervals.get(*cursor) {
        if frame > end.saturating_add(release_frames) {
            *cursor = cursor.saturating_add(1);
        } else {
            break;
        }
    }
    let Some((start, end)) = intervals.get(*cursor).copied() else {
        return 1.0;
    };
    let full_duck_start = start.saturating_sub(preduck_frames);
    if attack_frames > 0 && frame < full_duck_start {
        let attack_start = full_duck_start.saturating_sub(attack_frames);
        if frame >= attack_start {
            let position = (frame - attack_start) as f32 / attack_frames as f32;
            let eased = smooth_duck_fade(position);
            return 1.0 + (duck - 1.0) * eased;
        }
    }
    if frame >= full_duck_start && frame <= end {
        return duck;
    }
    if release_frames > 0 && frame > end && frame <= end.saturating_add(release_frames) {
        let position = (frame - end) as f32 / release_frames as f32;
        let eased = smooth_duck_fade(position);
        return duck + (1.0 - duck) * eased;
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
    let attack_frames = (MIX_SAMPLE_RATE as u64 * FADE_MS as u64) / 1000;
    let preduck_frames = (MIX_SAMPLE_RATE as u64 * PRE_DUCK_MS as u64) / 1000;
    let release_frames = (MIX_SAMPLE_RATE as u64 * RELEASE_MS as u64) / 1000;
    let duck_intervals = merge_duck_intervals(
        scheduled
            .iter()
            .filter(|cue| !cue.extended_pause)
            .map(|cue| {
                let start = (cue.start_sec * MIX_SAMPLE_RATE as f64).round() as u64;
                let cue_frames = cue.pcm.len() as u64 / MIX_CHANNELS as u64;
                (start, start.saturating_add(cue_frames))
            })
            .collect(),
        attack_frames.saturating_add(preduck_frames),
    );
    let mut duck_cursor = 0usize;
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
        let gain = duck_gain_for(
            source_frame,
            &duck_intervals,
            &mut duck_cursor,
            attack_frames,
            preduck_frames,
            release_frames,
        );
        if let Some(cue) = scheduled.get(cue_index).filter(|c| !c.extended_pause) {
            let start = (cue.start_sec * MIX_SAMPLE_RATE as f64).round() as u64;
            let cue_frames = cue.pcm.len() as u64 / MIX_CHANNELS as u64;
            let end = start + cue_frames;
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

fn audio_description_mkv_fallback_path(path: &Path) -> PathBuf {
    let mut fallback = path.to_path_buf();
    fallback.set_extension("mkv");
    if !fallback.exists() {
        return fallback;
    }
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("audio_description");
    for index in 1..=9_999 {
        let candidate = parent.join(format!("{stem}_fallback_{index}.mkv"));
        if !candidate.exists() {
            return candidate;
        }
    }
    parent.join(format!("{stem}_fallback_{}.mkv", std::process::id()))
}

fn audio_description_mp4_mux_error_allows_mkv_fallback(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    if lower.contains("saving canceled")
        || lower.contains("cancelled")
        || lower.contains("canceled")
        || lower.contains("no space left")
        || lower.contains("permission denied")
        || lower.contains("operation not permitted")
    {
        return false;
    }
    lower.contains("failed to write header")
        || lower.contains("av_interleaved_write_frame")
        || lower.contains("could not write header")
        || lower.contains("not currently supported in container")
        || lower.contains("codec not currently supported")
}

fn mux_audio_description_video(
    input_video: &Path,
    mixed_audio: &Path,
    output: &Path,
    cancel: &Arc<AtomicBool>,
) -> Result<(), String> {
    let args = vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-y".into(),
        "-i".into(),
        input_video.to_string_lossy().to_string(),
        "-i".into(),
        mixed_audio.to_string_lossy().to_string(),
        "-map".into(),
        "0:v:0".into(),
        "-map".into(),
        "1:a:0".into(),
        "-c:v".into(),
        "copy".into(),
        "-c:a".into(),
        "copy".into(),
        "-map_metadata".into(),
        "0".into(),
        "-shortest".into(),
        output.to_string_lossy().to_string(),
    ];
    run_ffmpeg(&args, cancel)
}

fn export_audio_description_media(
    input_video: &Path,
    mixed_mp3: &Path,
    requested_output: &Path,
    create_video_output: bool,
    cancel: &Arc<AtomicBool>,
) -> Result<PathBuf, String> {
    if !create_video_output {
        fs::copy(mixed_mp3, requested_output)
            .map_err(|error| format!("Salvataggio MP3 fallito: {error}"))?;
        return Ok(requested_output.to_path_buf());
    }

    match mux_audio_description_video(input_video, mixed_mp3, requested_output, cancel) {
        Ok(()) => Ok(requested_output.to_path_buf()),
        Err(primary_error) => {
            let wants_mp4 = requested_output
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("mp4"));
            if !wants_mp4
                || !audio_description_mp4_mux_error_allows_mkv_fallback(&primary_error)
                || cancel.load(Ordering::Relaxed)
            {
                return Err(primary_error);
            }
            if requested_output.exists() {
                let _ = fs::remove_file(requested_output);
            }
            let fallback = audio_description_mkv_fallback_path(requested_output);
            append_podcast_log(&format!(
                "audio_description.video_mp4_fallback primary_error={} fallback={}",
                primary_error,
                fallback.display()
            ));
            mux_audio_description_video(input_video, mixed_mp3, &fallback, cancel).map_err(
                |fallback_error| {
                    let _ = fs::remove_file(&fallback);
                    format!(
                        "{primary_error}; anche il fallback MKV è fallito: {fallback_error}"
                    )
                },
            )?;
            Ok(fallback)
        }
    }
}

fn project_path(output: &Path) -> PathBuf {
    let mut p = output.to_path_buf();
    p.set_extension("sonarpad-ad.json");
    p
}

fn partial_checkpoint_path(output: &Path) -> PathBuf {
    let mut p = output.to_path_buf();
    p.set_extension("sonarpad-ad.partial.json");
    p
}

fn load_partial_checkpoint(path: &Path) -> Result<AudioDescriptionPartialCheckpoint, String> {
    let raw = fs::read(path)
        .map_err(|error| format!("Impossibile leggere il checkpoint dell'audiodescrizione: {error}"))?;
    let checkpoint: AudioDescriptionPartialCheckpoint = serde_json::from_slice(&raw)
        .map_err(|error| format!("Checkpoint audiodescrizione non valido: {error}"))?;
    if checkpoint.format != AUDIO_DESCRIPTION_PARTIAL_FORMAT
        || checkpoint.version != AUDIO_DESCRIPTION_PARTIAL_VERSION
    {
        return Err("Formato del checkpoint audiodescrizione non supportato.".to_string());
    }
    if checkpoint.total_chunks == 0 || checkpoint.completed_chunks > checkpoint.total_chunks {
        return Err("Avanzamento del checkpoint audiodescrizione non valido.".to_string());
    }
    let source_metadata = fs::metadata(&checkpoint.source_path).map_err(|error| {
        format!("Il video salvato nel checkpoint non è disponibile: {error}")
    })?;
    if source_metadata.len() != checkpoint.source_file_size {
        return Err("Il video non corrisponde più al lavoro interrotto.".to_string());
    }
    Ok(checkpoint)
}

fn save_partial_checkpoint(
    path: &Path,
    job: &CreateJob,
    source_duration_sec: f64,
    checkpoint: &AudioDescriptionBridgeCheckpoint,
) -> Result<(), String> {
    let source_file_size = fs::metadata(&job.input_path)
        .map_err(|error| format!("Lettura dati del video fallita: {error}"))?
        .len();
    let character_catalog = job.catalog.as_ref().map(|catalog| AudioDescriptionPartialCatalog {
        name: catalog.name.clone(),
        path: catalog.path.clone(),
        characters: catalog.characters.clone(),
    });
    let value = AudioDescriptionPartialCheckpoint {
        format: AUDIO_DESCRIPTION_PARTIAL_FORMAT.to_string(),
        version: AUDIO_DESCRIPTION_PARTIAL_VERSION,
        source_path: job.input_path.clone(),
        output_mp3_path: job.output_path.clone(),
        source_file_size,
        source_duration_sec,
        language_code: job.language_code.clone(),
        verbosity: job.verbosity.as_bridge().to_string(),
        allow_extended_pauses: job.allow_extended_pauses,
        recognize_characters: job.recognize_characters,
        recognize_screen_text: job.recognize_screen_text,
        save_project: job.save_project,
        create_video_output: job.create_video_output,
        delete_input_after_success: job.delete_input_after_success,
        keep_character_catalog: job.keep_character_catalog,
        tts_engine: job.tts_engine.clone(),
        tts_voice: job.tts_voice.clone(),
        rate: job.rate,
        pitch: job.pitch,
        volume: job.volume,
        audio_stream_index: job.audio_stream_index,
        gemini_model: if checkpoint.gemini_model.trim().is_empty() {
            job.gemini_model.clone()
        } else {
            checkpoint.gemini_model.trim().to_string()
        },
        character_catalog,
        completed_chunks: checkpoint.completed_chunks,
        total_chunks: checkpoint.total_chunks,
        descriptions: checkpoint.descriptions.clone(),
        character_glossary: checkpoint.character_glossary.clone(),
    };
    let raw = serde_json::to_vec_pretty(&value)
        .map_err(|error| format!("Serializzazione checkpoint fallita: {error}"))?;
    let temporary = temporary_sibling_path(path, "partial");
    fs::write(&temporary, raw)
        .map_err(|error| format!("Salvataggio checkpoint fallito: {error}"))?;
    // On macOS/Unix, rename atomically replaces an existing file on the same volume.
    // This avoids a window where a crash could leave the job without a checkpoint.
    fs::rename(&temporary, path).map_err(|error| {
        let _ = fs::remove_file(&temporary);
        format!("Conferma checkpoint fallita: {error}")
    })
}

fn load_resume_settings(path: &Path) -> Result<AudioDescriptionResumeSettings, String> {
    let checkpoint = load_partial_checkpoint(path)?;
    Ok(AudioDescriptionResumeSettings {
        gemini_model: checkpoint.gemini_model,
        completed_chunks: checkpoint.completed_chunks,
        total_chunks: checkpoint.total_chunks,
    })
}

fn job_from_checkpoint(
    path: &Path,
    gemini_api_key: String,
    sonarpad_ai_service_url: String,
    sonarpad_ai_access_code: String,
    sonarpad_ai_device_id: String,
    gemini_model: String,
) -> Result<CreateJob, String> {
    let checkpoint = load_partial_checkpoint(path)?;
    let catalog = checkpoint.character_catalog.map(|catalog| CharacterCatalog {
        name: catalog.name,
        path: catalog.path,
        characters: catalog.characters,
    });
    Ok(CreateJob {
        input_path: checkpoint.source_path,
        output_path: checkpoint.output_mp3_path,
        language_code: checkpoint.language_code,
        verbosity: Verbosity::from_settings(&checkpoint.verbosity),
        allow_extended_pauses: checkpoint.allow_extended_pauses,
        recognize_characters: checkpoint.recognize_characters,
        recognize_screen_text: checkpoint.recognize_screen_text,
        save_project: checkpoint.save_project,
        create_video_output: checkpoint.create_video_output,
        delete_input_after_success: checkpoint.delete_input_after_success,
        keep_character_catalog: checkpoint.keep_character_catalog,
        catalog,
        tts_engine: checkpoint.tts_engine,
        tts_voice: checkpoint.tts_voice,
        rate: checkpoint.rate,
        pitch: checkpoint.pitch,
        volume: checkpoint.volume,
        audio_stream_index: checkpoint.audio_stream_index,
        gemini_api_key,
        sonarpad_ai_service_url,
        sonarpad_ai_access_code,
        sonarpad_ai_device_id,
        gemini_model,
        fixed_reanalysis_slots: Vec::new(),
        resume_checkpoint_path: Some(path.to_path_buf()),
    })
}

fn project_duck_bounds(
    output_start_sec: f64,
    output_end_sec: f64,
    extended_pause: bool,
) -> (Option<f64>, Option<f64>) {
    if extended_pause {
        (None, None)
    } else {
        (
            Some(
                (output_start_sec - (FADE_MS + PRE_DUCK_MS) as f64 / 1000.0).max(0.0),
            ),
            Some(output_end_sec + RELEASE_MS as f64 / 1000.0),
        )
    }
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
        let (duck_start_sec, duck_end_sec) =
            project_duck_bounds(output_start, output_end, d.extended_pause);
        descriptions.push(ProjectDescription {
            id,
            text: d.text.clone(),
            original_text: d.text.clone(),
            rendered_text: d.text.clone(),
            modified: false,
            gemini_start_sec: d.desired_start_sec,
            visual_evidence_time_sec: d.visual_evidence_time_sec,
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
            duck_start_sec,
            duck_end_sec,
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
        output_is_video: job.create_video_output,
        audio_stream_index: job.audio_stream_index,
        source_duration_sec: analysis.duration_sec,
        output_duration_sec: output_duration,
        language: job.language_code.clone(),
        language_code: job.language_code.clone(),
        verbosity: job.verbosity.as_bridge().into(),
        allow_extended_pauses: job.allow_extended_pauses,
        recognize_characters: job.recognize_characters,
        recognize_screen_text: job.recognize_screen_text,
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

fn temporary_sibling_path(path: &Path, label: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("audio_description");
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    let file_name = if extension.is_empty() {
        format!(".{stem}.{label}.{}.{}", std::process::id(), stamp)
    } else {
        format!(
            ".{stem}.{label}.{}.{}.{}",
            std::process::id(),
            stamp,
            extension
        )
    };
    parent.join(file_name)
}

fn commit_project_pair(
    temporary_mp3: &Path,
    final_mp3: &Path,
    temporary_project: &Path,
    final_project: &Path,
) -> Result<(), String> {
    let mp3_backup = temporary_sibling_path(final_mp3, "backup");
    let project_backup = temporary_sibling_path(final_project, "backup");
    let had_mp3 = final_mp3.exists();
    let had_project = final_project.exists();

    if had_mp3 {
        fs::rename(final_mp3, &mp3_backup)
            .map_err(|error| format!("Backup del vecchio MP3 fallito: {error}"))?;
    }
    if had_project && let Err(error) = fs::rename(final_project, &project_backup) {
        if had_mp3 {
            let _ = fs::rename(&mp3_backup, final_mp3);
        }
        return Err(format!("Backup del vecchio progetto fallito: {error}"));
    }

    if let Err(error) = fs::rename(temporary_mp3, final_mp3) {
        if had_project {
            let _ = fs::rename(&project_backup, final_project);
        }
        if had_mp3 {
            let _ = fs::rename(&mp3_backup, final_mp3);
        }
        return Err(format!("Aggiornamento MP3 fallito: {error}"));
    }
    if let Err(error) = fs::rename(temporary_project, final_project) {
        let _ = fs::remove_file(final_mp3);
        if had_mp3 {
            let _ = fs::rename(&mp3_backup, final_mp3);
        }
        if had_project {
            let _ = fs::rename(&project_backup, final_project);
        }
        return Err(format!("Aggiornamento progetto fallito: {error}"));
    }

    if had_mp3 {
        let _ = fs::remove_file(mp3_backup);
    }
    if had_project {
        let _ = fs::remove_file(project_backup);
    }
    Ok(())
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
        // Transient Gemini retries are self-healing and must not be presented as errors.
        // Keep the technical retry details in log.txt while the UI remains on a neutral waiting state.
        "gemini_retry" => tr("audio_description.progress.gemini_waiting"),
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
    if job.gemini_api_key.trim().is_empty() && job.sonarpad_ai_service_url.trim().is_empty() {
        return Err(tr("audio_description.error.api_key"));
    }
    if !job.sonarpad_ai_service_url.trim().is_empty()
        && job.sonarpad_ai_access_code.trim().is_empty()
    {
        return Err(tr("audio_description.error.sonarpad_code"));
    }
    if job.tts_voice.trim().is_empty() {
        return Err(tr("audio_description.error.voice"));
    }
    if let Some(parent) = job.output_path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let work = cache_dir("job")?;
    let checkpoint_path = job
        .resume_checkpoint_path
        .clone()
        .unwrap_or_else(|| partial_checkpoint_path(&job.output_path));
    let result = (|| {
        {
            let mut s = state.lock().unwrap();
            s.progress = 2;
            s.status = tr("audio_description.progress.analysis_prepare");
        }
        let source_wav = work.join("source.wav");
        let probe = decode_source_audio(
            &job.input_path,
            &source_wav,
            job.audio_stream_index,
            &cancel,
        )?;
        let duration = probe.duration_sec;
        let pyannote = work.join("pyannote.wav");
        let audio_wav_path = if !job.fixed_reanalysis_slots.is_empty() {
            // Segment reanalysis already owns the authoritative saved pauses.
            // Do not run Pyannote again and do not let a new silence analysis
            // redefine the project structure. Gemini only rewrites slot text.
            None
        } else if probe.has_audio {
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
        let chunks = prepare_chunks(
            &job.input_path,
            duration,
            &work,
            job.audio_stream_index,
            &cancel,
        )?;
        let resume = if job.resume_checkpoint_path.is_some() {
            match load_partial_checkpoint(&checkpoint_path) {
                Ok(checkpoint)
                    if checkpoint.total_chunks == chunks.len()
                        && (checkpoint.source_duration_sec - duration).abs() <= 0.5 =>
                {
                    Some(AudioDescriptionBridgeResume {
                        completed_chunks: checkpoint.completed_chunks,
                        descriptions: checkpoint.descriptions,
                        character_glossary: checkpoint.character_glossary,
                    })
                }
                Ok(checkpoint) => {
                    append_podcast_log(&format!(
                        "audio_description.resume_checkpoint_ignored reason=chunk_layout_changed old_chunks={} new_chunks={} old_duration={:.3} new_duration={:.3} path={}",
                        checkpoint.total_chunks,
                        chunks.len(),
                        checkpoint.source_duration_sec,
                        duration,
                        checkpoint_path.display()
                    ));
                    None
                }
                Err(error) => {
                    append_podcast_log(&format!(
                        "audio_description.resume_checkpoint_ignored reason=invalid error={} path={}",
                        error,
                        checkpoint_path.display()
                    ));
                    None
                }
            }
        } else {
            None
        };
        let request = AudioDescriptionBridgeRequest {
            input_path: job.input_path.to_string_lossy().to_string(),
            audio_wav_path,
            duration_sec: duration,
            chunks,
            language: job.language_code.clone(),
            verbosity: job.verbosity.as_bridge().into(),
            allow_extended_pauses: job.allow_extended_pauses,
            recognize_characters: job.recognize_characters,
            recognize_screen_text: job.recognize_screen_text,
            initial_character_glossary: job
                .catalog
                .as_ref()
                .map(|c| c.characters.clone())
                .unwrap_or_default(),
            ai_access_mode: if job.sonarpad_ai_service_url.trim().is_empty() {
                "personal".to_string()
            } else {
                "sonarpad".to_string()
            },
            gemini_api_key: job.gemini_api_key.clone(),
            sonarpad_ai_service_url: job.sonarpad_ai_service_url.clone(),
            sonarpad_ai_access_code: job.sonarpad_ai_access_code.clone(),
            sonarpad_ai_device_id: job.sonarpad_ai_device_id.clone(),
            gemini_model: job.gemini_model.clone(),
            fixed_reanalysis_slots: job.fixed_reanalysis_slots.clone(),
            resume,
        };
        let run_bridge_once = |bridge_request: &AudioDescriptionBridgeRequest| {
            let status_state = state.clone();
            let progress_state = state.clone();
            let quota_state = state.clone();
            let overload_state = state.clone();
            let checkpoint_job = job.clone();
            let checkpoint_target = checkpoint_path.clone();
            run_audio_description_bridge(
                bridge_request,
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
                    overload: Some(Box::new(move |model, error| {
                        let (tx, rx) = mpsc::sync_channel(1);
                        overload_state.lock().unwrap().overload = Some(OverloadUiRequest {
                            model: model.to_string(),
                            error: error.to_string(),
                            sender: tx,
                        });
                        rx.recv().unwrap_or(AudioDescriptionOverloadDecision::Stop)
                    })),
                    checkpoint: Some(Box::new(move |checkpoint| {
                        if let Err(error) = save_partial_checkpoint(
                            &checkpoint_target,
                            &checkpoint_job,
                            duration,
                            checkpoint,
                        ) {
                            append_podcast_log(&format!(
                                "audio_description.checkpoint_save_failed error={error}"
                            ));
                        } else {
                            append_podcast_log(&format!(
                                "audio_description.checkpoint_saved chunk={}/{} path={}",
                                checkpoint.completed_chunks,
                                checkpoint.total_chunks,
                                checkpoint_target.display()
                            ));
                        }
                    })),
                },
            )
        };

        // Preserve the normal path for files that already work. Compatibility
        // segmentation is activated only after Gemini rejects or fails to process
        // the original prepared media, matching the current Windows bridge policy.
        let mut analysis_result = run_bridge_once(&request);
        let mut try_mp4_fallback = false;

        if let Err(primary_error) = &analysis_result
            && gemini_media_processing_failed(primary_error)
            && !cancel.load(Ordering::Relaxed)
        {
            append_podcast_log(&format!(
                "audio_description.gemini_media_processing_failed fallback=mp4 error={primary_error}"
            ));
            state.lock().unwrap().status = tr("audio_description.progress.gemini_mp4_fallback");
            try_mp4_fallback = true;
        }

        if let Err(primary_error) = &analysis_result
            && gemini_media_invalid_argument(primary_error)
            && !cancel.load(Ordering::Relaxed)
        {
            append_podcast_log(&format!(
                "audio_description.gemini_invalid_argument fallback=small_mkv error={primary_error}"
            ));
            state.lock().unwrap().status = tr("audio_description.progress.gemini_small_fallback");
            let fallback_dir = work.join("gemini_fallback_small_mkv");
            match prepare_gemini_compatibility_chunks(
                &job.input_path,
                duration,
                &fallback_dir,
                job.audio_stream_index,
                &cancel,
                "mkv",
                false,
            ) {
                Ok(fallback_chunks) => {
                    let mut fallback_request = request.clone();
                    fallback_request.chunks = fallback_chunks;
                    fallback_request.resume = None;
                    analysis_result = run_bridge_once(&fallback_request);
                    if let Err(fallback_error) = &analysis_result
                        && (gemini_media_invalid_argument(fallback_error)
                            || gemini_media_processing_failed(fallback_error))
                        && !cancel.load(Ordering::Relaxed)
                    {
                        append_podcast_log(&format!(
                            "audio_description.gemini_small_mkv_failed fallback=mp4 error={fallback_error}"
                        ));
                        try_mp4_fallback = true;
                    }
                }
                Err(error) => {
                    append_podcast_log(&format!(
                        "audio_description.gemini_small_mkv_prepare_failed fallback=mp4 error={error}"
                    ));
                    try_mp4_fallback = true;
                }
            }
        }

        if try_mp4_fallback && !cancel.load(Ordering::Relaxed) {
            state.lock().unwrap().status = tr("audio_description.progress.gemini_mp4_fallback");
            let fallback_dir = work.join("gemini_fallback_mp4");
            match prepare_gemini_compatibility_chunks(
                &job.input_path,
                duration,
                &fallback_dir,
                job.audio_stream_index,
                &cancel,
                "mp4",
                true,
            ) {
                Ok(fallback_chunks) => {
                    let mut fallback_request = request.clone();
                    fallback_request.chunks = fallback_chunks;
                    fallback_request.resume = None;
                    analysis_result = run_bridge_once(&fallback_request);
                }
                Err(mp4_error) => {
                    let combined_error = match &analysis_result {
                        Err(previous_error) => Some(format!(
                            "{previous_error}\nGemini MP4 compatibility fallback could not be prepared: {mp4_error}"
                        )),
                        Ok(_) => None,
                    };
                    if let Some(combined_error) = combined_error {
                        analysis_result = Err(combined_error);
                    }
                }
            }
        }
        let analysis = analysis_result?;
        if analysis.descriptions.is_empty() {
            return Err("Gemini non ha restituito descrizioni.".to_string());
        }
        {
            let mut s = state.lock().unwrap();
            s.progress = 55;
            s.status = tr("audio_description.progress.tts");
        }
        let synthesis_tasks = analysis
            .descriptions
            .iter()
            .enumerate()
            .map(|(index, description)| AudioDescriptionSynthesisTask {
                original_index: index,
                synthesis_index: index,
                text: description.text.clone(),
                desired_start_sec: description.start_sec,
                visual_start_sec: description
                    .visual_start_sec
                    .unwrap_or(description.start_sec),
                visual_evidence_time_sec: description.visual_evidence_time_sec,
                mandatory: description.mandatory,
                slot_id: description.slot_id.clone(),
                slot_start_sec: description.slot_start_sec,
                slot_end_sec: description.slot_end_sec,
            })
            .collect::<Vec<_>>();
        let synthesized = synthesize_description_tasks_parallel(
            &synthesis_tasks,
            TtsParameters {
                engine: &job.tts_engine,
                voice: &job.tts_voice,
                rate: job.rate,
                pitch: job.pitch,
                volume: job.volume,
            },
            rt,
            &work,
            cancel.clone(),
            |completed, total| {
                let mut s = state.lock().unwrap();
                s.progress = 55 + ((completed as i32 * 25) / total.max(1) as i32);
            },
        )?;
        {
            let mut s = state.lock().unwrap();
            s.status = tr("audio_description.progress.schedule");
            s.progress = 80;
        }
        let (scheduled, dropped) = schedule_descriptions(
            &synthesized,
            &analysis.protected_intervals,
            analysis.duration_sec,
            job.allow_extended_pauses && !job.create_video_output,
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
        let actual_output = export_audio_description_media(
            &job.input_path,
            &temp_output,
            &job.output_path,
            job.create_video_output,
            &cancel,
        )?;
        let project_file = if job.save_project {
            let p = project_path(&actual_output);
            let mut project_job = job.clone();
            project_job.output_path = actual_output.clone();
            let project = build_project(
                &project_job,
                &analysis,
                &scheduled,
                &dropped,
                output_duration,
            );
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
            output_path: actual_output,
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
    if result.is_ok() && checkpoint_path.exists() {
        if let Err(error) = fs::remove_file(&checkpoint_path) {
            append_podcast_log(&format!(
                "audio_description.checkpoint_remove_after_success_failed error={error}"
            ));
        }
    }
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
    completion_details.set_accessibility_label(&tr("audio_description.completion.details"));
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
fn resume_candidate_project_name(path: &Path) -> String {
    let filename = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    let without_checkpoint = filename
        .strip_suffix(CHECKPOINT_SUFFIX)
        .unwrap_or(filename);
    let without_audio_description = without_checkpoint
        .strip_suffix("_audiodescritto")
        .unwrap_or(without_checkpoint);
    without_audio_description.replace('_', " ")
}

fn resume_candidate_label(path: &Path) -> Option<String> {
    let resume = load_resume_settings(path).ok()?;
    let name = resume_candidate_project_name(path);
    let folder = path
        .parent()
        .and_then(Path::file_name)
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    let progress = format!("{}/{}", resume.completed_chunks, resume.total_chunks);
    Some(if folder.is_empty() {
        format!("{name} — {progress}")
    } else {
        format!("{name} — {progress} — {folder}")
    })
}

fn is_resume_checkpoint_path(path: &Path) -> bool {
    path.is_file()
        && path
            .file_name()
            .and_then(|value| value.to_str())
            .map(|name| name.to_ascii_lowercase().ends_with(CHECKPOINT_SUFFIX))
            .unwrap_or(false)
}

fn remember_audio_description_project_folder(settings: &mut Settings, path: &Path) {
    let Some(folder) = path.parent().filter(|folder| !folder.as_os_str().is_empty()) else {
        return;
    };
    let folder = folder.to_string_lossy().trim().to_string();
    if folder.is_empty() {
        return;
    }
    settings
        .audio_description_recent_project_folders
        .retain(|known| !known.eq_ignore_ascii_case(&folder));
    settings
        .audio_description_recent_project_folders
        .insert(0, folder);
    settings
        .audio_description_recent_project_folders
        .truncate(MAX_RECENT_PROJECT_FOLDERS);
}

fn resume_candidate_directories() -> Vec<PathBuf> {
    let settings = Settings::load();
    let mut directories = Vec::new();
    for folder in std::iter::once(settings.audio_description_save_folder)
        .chain(settings.audio_description_recent_project_folders)
    {
        let folder = folder.trim();
        if folder.is_empty() {
            continue;
        }
        let path = PathBuf::from(folder);
        let key = path.to_string_lossy();
        if directories.iter().any(|known: &PathBuf| {
            known.to_string_lossy().eq_ignore_ascii_case(&key)
        }) {
            continue;
        }
        directories.push(path);
    }
    directories
}

fn discover_resume_candidates() -> Vec<AudioDescriptionResumeCandidate> {
    let mut candidates = Vec::new();
    let mut seen = Vec::<String>::new();
    for directory in resume_candidate_directories() {
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !is_resume_checkpoint_path(&path) {
                continue;
            }
            let key = path.to_string_lossy().to_string();
            if seen.iter().any(|known| known.eq_ignore_ascii_case(&key)) {
                continue;
            }
            let Some(label) = resume_candidate_label(&path) else {
                append_podcast_log(&format!(
                    "audio_description.resume_selector.invalid_checkpoint path={}",
                    path.display()
                ));
                continue;
            };
            let modified = entry
                .metadata()
                .ok()
                .and_then(|metadata| metadata.modified().ok())
                .unwrap_or(UNIX_EPOCH);
            seen.push(key);
            candidates.push(AudioDescriptionResumeCandidate {
                path,
                label,
                modified,
            });
        }
    }
    candidates.sort_by(|left, right| {
        right
            .modified
            .cmp(&left.modified)
            .then_with(|| left.label.to_lowercase().cmp(&right.label.to_lowercase()))
    });
    candidates
}

fn browse_resume_checkpoint(parent: &Dialog) -> Option<PathBuf> {
    let d = FileDialog::builder(parent)
        .with_message(&tr("audio_description.resume.open_title"))
        .with_wildcard(
            "Checkpoint audiodescrizione Sonarpad|*.sonarpad-ad.partial.json",
        )
        .with_style(FileDialogStyle::Open | FileDialogStyle::FileMustExist)
        .build();
    if d.show_modal() == ID_OK {
        d.get_path().map(PathBuf::from)
    } else {
        None
    }
}

fn ensure_resume_model_choice(choice: &Choice, values: &Rc<RefCell<Vec<String>>>, value: &str) {
    let value = value.trim();
    if value.is_empty() {
        return;
    }
    let index = {
        let mut values = values.borrow_mut();
        if let Some(index) = values.iter().position(|item| item == value) {
            index
        } else {
            choice.append(value);
            values.push(value.to_string());
            values.len() - 1
        }
    };
    choice.set_selection(index as u32);
}

fn choose_resume_checkpoint(
    parent: &Dialog,
    api_key: &str,
    fallback_model: &str,
) -> Option<AudioDescriptionResumeSelection> {
    let candidates = Rc::new(RefCell::new(discover_resume_candidates()));
    let has_candidates = !candidates.borrow().is_empty();

    let mut initial_models = fetch_gemini_models(api_key).unwrap_or_else(|error| {
        append_podcast_log(&format!(
            "audio_description.resume_selector.model_refresh_failed error={error}"
        ));
        Vec::new()
    });
    for candidate in candidates.borrow().iter() {
        if let Ok(resume) = load_resume_settings(&candidate.path) {
            let model = resume.gemini_model.trim();
            if !model.is_empty() && !initial_models.iter().any(|item| item == model) {
                initial_models.push(model.to_string());
            }
        }
    }
    let fallback_model = fallback_model.trim();
    if !fallback_model.is_empty() && !initial_models.iter().any(|item| item == fallback_model) {
        initial_models.push(fallback_model.to_string());
    }
    let model_values = Rc::new(RefCell::new(initial_models));

    let selector = Dialog::builder(parent, &tr("audio_description.resume.open_title"))
        .with_style(DialogStyle::DefaultDialogStyle | DialogStyle::ResizeBorder)
        .with_size(680, 320)
        .build();
    let panel = Panel::builder(&selector).build();
    let root = BoxSizer::builder(Orientation::Vertical).build();
    let hint = if has_candidates {
        tr("audio_description.resume.choose_hint")
    } else {
        tr("audio_description.resume.none_found")
    };
    root.add(
        &StaticText::builder(&panel).with_label(&hint).build(),
        0,
        SizerFlag::Expand | SizerFlag::All,
        8,
    );
    root.add(
        &StaticText::builder(&panel)
            .with_label(&tr("audio_description.resume.choose_label"))
            .build(),
        0,
        SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Top,
        8,
    );
    let choice = Choice::builder(&panel).build();
    choice.set_accessibility_label(&tr("audio_description.resume.choose_label"));
    for candidate in candidates.borrow().iter() {
        choice.append(&candidate.label);
    }
    if has_candidates {
        choice.set_selection(0);
    }
    choice.enable(has_candidates);
    root.add(&choice, 0, SizerFlag::Expand | SizerFlag::All, 8);

    root.add(
        &StaticText::builder(&panel)
            .with_label(&tr("audio_description.resume.model"))
            .build(),
        0,
        SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Top,
        8,
    );
    let model_choice = Choice::builder(&panel).build();
    model_choice.set_accessibility_label(&tr("audio_description.resume.model"));
    for model in model_values.borrow().iter() {
        model_choice.append(model);
    }
    if has_candidates {
        if let Some(candidate) = candidates.borrow().first()
            && let Ok(resume) = load_resume_settings(&candidate.path)
        {
            let preferred = if resume.gemini_model.trim().is_empty() {
                fallback_model
            } else {
                resume.gemini_model.trim()
            };
            ensure_resume_model_choice(&model_choice, &model_values, preferred);
        }
    } else if !fallback_model.is_empty() {
        ensure_resume_model_choice(&model_choice, &model_values, fallback_model);
    } else if !model_values.borrow().is_empty() {
        model_choice.set_selection(0);
    }
    root.add(&model_choice, 0, SizerFlag::Expand | SizerFlag::All, 8);

    let buttons = BoxSizer::builder(Orientation::Horizontal).build();
    let continue_button = Button::builder(&panel)
        .with_id(ID_OK)
        .with_label(&tr("audio_description.resume.start"))
        .build();
    let browse_button = Button::builder(&panel)
        .with_id(ID_AUDIO_DESCRIPTION_RESUME_BROWSE)
        .with_label(&tr("audio_description.resume.browse_other"))
        .build();
    let cancel_button = Button::builder(&panel)
        .with_id(ID_CANCEL)
        .with_label(&tr("audio_description.cancel"))
        .build();
    continue_button.enable(has_candidates);
    buttons.add(&continue_button, 0, SizerFlag::All, 8);
    buttons.add(&browse_button, 0, SizerFlag::All, 8);
    buttons.add(&cancel_button, 0, SizerFlag::All, 8);
    root.add_sizer(&buttons, 0, SizerFlag::Expand, 0);
    panel.set_sizer(root, true);

    selector.set_affirmative_id(if has_candidates {
        ID_OK
    } else {
        ID_AUDIO_DESCRIPTION_RESUME_BROWSE
    });
    selector.set_escape_id(ID_CANCEL);
    let selected = Rc::new(RefCell::new(None::<AudioDescriptionResumeSelection>));

    let candidates_changed = Rc::clone(&candidates);
    let model_values_changed = Rc::clone(&model_values);
    let fallback_model_changed = fallback_model.to_string();
    choice.on_selection_changed(move |_| {
        let Some(index) = choice.get_selection() else {
            return;
        };
        let Some(candidate) = candidates_changed.borrow().get(index as usize).cloned() else {
            return;
        };
        if let Ok(resume) = load_resume_settings(&candidate.path) {
            let preferred = if resume.gemini_model.trim().is_empty() {
                fallback_model_changed.as_str()
            } else {
                resume.gemini_model.trim()
            };
            ensure_resume_model_choice(&model_choice, &model_values_changed, preferred);
        }
    });

    let selector_continue = selector;
    let selected_continue = Rc::clone(&selected);
    let candidates_continue = Rc::clone(&candidates);
    continue_button.on_click(move |_| {
        let Some(index) = choice.get_selection() else {
            return;
        };
        let Some(candidate) = candidates_continue.borrow().get(index as usize).cloned() else {
            return;
        };
        let selected_model = model_choice.get_string_selection().unwrap_or_default();
        if selected_model.trim().is_empty() {
            show_error(&selector_continue, &tr("audio_description.error.model"));
            model_choice.set_focus();
            return;
        }
        *selected_continue.borrow_mut() = Some(AudioDescriptionResumeSelection {
            checkpoint_path: candidate.path,
            gemini_model: selected_model,
        });
        selector_continue.end_modal(ID_OK);
    });

    let selector_browse = selector;
    let candidates_browse = Rc::clone(&candidates);
    let model_values_browse = Rc::clone(&model_values);
    let fallback_model_browse = fallback_model.to_string();
    browse_button.on_click(move |_| {
        let Some(path) = browse_resume_checkpoint(&selector_browse) else {
            return;
        };
        let resume = match load_resume_settings(&path) {
            Ok(value) => value,
            Err(error) => {
                show_error(
                    &selector_browse,
                    &trf("audio_description.resume.invalid", &[("error", error)]),
                );
                return;
            }
        };
        let label = resume_candidate_label(&path)
            .unwrap_or_else(|| resume_candidate_project_name(&path));
        let index = {
            let mut candidates = candidates_browse.borrow_mut();
            if let Some(index) = candidates.iter().position(|item| item.path == path) {
                index
            } else {
                let modified = fs::metadata(&path)
                    .ok()
                    .and_then(|metadata| metadata.modified().ok())
                    .unwrap_or(UNIX_EPOCH);
                candidates.push(AudioDescriptionResumeCandidate {
                    path: path.clone(),
                    label: label.clone(),
                    modified,
                });
                choice.append(&label);
                candidates.len() - 1
            }
        };
        choice.enable(true);
        continue_button.enable(true);
        choice.set_selection(index as u32);
        let preferred = if resume.gemini_model.trim().is_empty() {
            fallback_model_browse.as_str()
        } else {
            resume.gemini_model.trim()
        };
        ensure_resume_model_choice(&model_choice, &model_values_browse, preferred);
        choice.set_focus();
    });

    let selector_cancel = selector;
    cancel_button.on_click(move |_| selector_cancel.end_modal(ID_CANCEL));
    let selector_close = selector;
    selector.on_close(move |event| {
        selector_close.end_modal(ID_CANCEL);
        event.skip(false);
    });

    if has_candidates {
        choice.set_focus();
    } else {
        browse_button.set_focus();
    }
    let result = selector.show_modal();
    selector.destroy();
    if result == ID_OK {
        selected.borrow().clone()
    } else {
        None
    }
}

fn suggested_audio_description_output(input: &Path, create_video_output: bool) -> PathBuf {
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("video");
    let extension = if create_video_output { "mp4" } else { "mp3" };
    default_output_dir().join(format!(
        "{}_audiodescritto.{extension}",
        sanitize_filename(stem)
    ))
}

fn choose_output(parent: &Dialog, input: &Path, create_video_output: bool) -> Option<PathBuf> {
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("video");
    let extension = if create_video_output { "mp4" } else { "mp3" };
    let wildcard = if create_video_output {
        "Video MP4|*.mp4|Video Matroska|*.mkv"
    } else {
        "MP3|*.mp3"
    };
    let d = FileDialog::builder(parent)
        .with_message(&tr("audio_description.save_title"))
        .with_default_file(&format!(
            "{}_audiodescritto.{extension}",
            sanitize_filename(stem)
        ))
        .with_wildcard(wildcard)
        .with_style(FileDialogStyle::Save | FileDialogStyle::OverwritePrompt)
        .build();
    if d.show_modal() == ID_OK {
        d.get_path().map(PathBuf::from)
    } else {
        None
    }
}

fn overload_dialog(
    parent: &dyn WxWidget,
    model: &str,
    error: &str,
) -> AudioDescriptionOverloadDecision {
    let d = Dialog::builder(parent, &tr("audio_description.overload.title"))
        .with_size(620, 250)
        .build();
    let p = Panel::builder(&d).build();
    let root = BoxSizer::builder(Orientation::Vertical).build();
    let msg = trf(
        "audio_description.overload.message",
        &[("model", model.to_string()), ("error", error.to_string())],
    );
    root.add(
        &StaticText::builder(&p).with_label(&msg).build(),
        0,
        SizerFlag::Expand | SizerFlag::All,
        8,
    );
    let row = BoxSizer::builder(Orientation::Horizontal).build();
    let wait = Button::builder(&p)
        .with_id(7101)
        .with_label(&tr("audio_description.overload.wait"))
        .build();
    let stop = Button::builder(&p)
        .with_id(ID_CANCEL)
        .with_label(&tr("audio_description.cancel"))
        .build();
    row.add(&wait, 0, SizerFlag::All, 5);
    row.add(&stop, 0, SizerFlag::All, 5);
    root.add_sizer(&row, 0, SizerFlag::Expand, 0);
    p.set_sizer(root, true);
    let d1 = d;
    wait.on_click(move |_| d1.end_modal(7101));
    let d2 = d;
    stop.on_click(move |_| d2.end_modal(ID_CANCEL));
    if d.show_modal() == 7101 {
        AudioDescriptionOverloadDecision::Wait
    } else {
        AudioDescriptionOverloadDecision::Stop
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
        overload: None,
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
        let overload = { state_tick.lock().unwrap().overload.take() };
        if let Some(o) = overload {
            let decision = overload_dialog(&dialog_tick, &o.model, &o.error);
            let _ = o.sender.send(decision);
        }
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

fn should_delete_input_after_success(delete_requested: bool, save_project: bool) -> bool {
    delete_requested && !save_project
}

#[cfg(target_os = "macos")]
fn move_input_video_to_trash(path: &Path) -> Result<(), String> {
    use objc2_foundation::{NSFileManager, NSURL};

    let url = NSURL::from_file_path(path)
        .ok_or_else(|| format!("invalid file URL: {}", path.display()))?;
    NSFileManager::defaultManager()
        .trashItemAtURL_resultingItemURL_error(&url, None)
        .map_err(|error| error.localizedDescription().to_string())
}

#[cfg(not(target_os = "macos"))]
fn move_input_video_to_trash(_path: &Path) -> Result<(), String> {
    Err("moving files to the Trash is not supported on this platform".to_string())
}

fn execute_audio_description_job(
    parent: &Frame,
    dialog: &Dialog,
    settings: &Arc<Mutex<Settings>>,
    rt: &Arc<Runtime>,
    job: CreateJob,
    selected_model: String,
) -> bool {
    let input_to_trash = should_delete_input_after_success(
        job.delete_input_after_success,
        job.save_project,
    )
    .then(|| job.input_path.clone());
    {
        let mut st = settings.lock().unwrap();
        let use_sonarpad_ai = !job.sonarpad_ai_service_url.trim().is_empty();
        st.audio_description_use_sonarpad_ai = use_sonarpad_ai;
        if use_sonarpad_ai {
            st.sonarpad_ai_access_code = job.sonarpad_ai_access_code.clone();
            if !job.sonarpad_ai_device_id.trim().is_empty() {
                st.sonarpad_ai_device_id = job.sonarpad_ai_device_id.clone();
            }
            // Sonarpad AI always uses the service model. Do not overwrite the
            // user's personal Gemini model preference with the forced service model.
        } else {
            // Keep the personal Gemini key independent from the Sonarpad AI credentials.
            // Keep the personal model independent from the forced Sonarpad AI model.
            st.audio_description_gemini_api_key = job.gemini_api_key.clone();
            st.audio_description_gemini_model = selected_model;
        }
        st.audio_description_language = job.language_code.clone();
        st.audio_description_tts_engine = job.tts_engine.clone();
        st.audio_description_tts_voice = job.tts_voice.clone();
        st.audio_description_tts_rate = Some(job.rate);
        st.audio_description_tts_volume = Some(job.volume);
        st.audio_description_verbosity = job.verbosity.as_bridge().to_string();
        st.audio_description_extended_pauses = job.allow_extended_pauses;
        st.audio_description_recognize_characters = job.recognize_characters;
        st.audio_description_recognize_screen_text = job.recognize_screen_text;
        st.audio_description_save_project = job.save_project;
        st.audio_description_create_video_output = job.create_video_output;
        st.audio_description_delete_video_after = job.delete_input_after_success;
        st.audio_description_keep_character_catalog = job.keep_character_catalog;
        st.audio_description_character_catalog = job
            .catalog
            .as_ref()
            .map(|c| c.path.to_string_lossy().to_string())
            .unwrap_or_default();
        if job.resume_checkpoint_path.is_none() {
            remember_audio_description_project_folder(&mut st, &job.output_path);
        }
        st.save();
    }
    match run_with_progress(parent, job, rt.clone()) {
        Ok(out) => {
            let output_to_open = out.output_path.clone();
            let trash_result = input_to_trash
                .as_ref()
                .map(|path| (path, move_input_video_to_trash(path)));
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
            if let Some((path, result)) = trash_result {
                match result {
                    Ok(()) => {
                        append_podcast_log(&format!(
                            "audio_description.create.input_trashed path={}",
                            path.display()
                        ));
                        msg.push_str(&format!(
                            "\n\n{}",
                            trf(
                                "audio_description.input_trashed",
                                &[("path", path.display().to_string())]
                            )
                        ));
                    }
                    Err(error) => {
                        append_podcast_log(&format!(
                            "audio_description.create.input_trash_failed path={} error={error}",
                            path.display()
                        ));
                        msg.push_str(&format!(
                            "\n\n{}",
                            trf(
                                "audio_description.input_trash_failed",
                                &[
                                    ("path", path.display().to_string()),
                                    ("error", error),
                                ]
                            )
                        ));
                    }
                }
            }
            show_completion(dialog, &msg);
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
                show_error(dialog, &error);
            } else {
                append_podcast_log(&format!(
                    "audio_description.create.open_output_completed path={}",
                    output_to_open.display()
                ));
            }
            false
        }
        Err(error) => {
            if error == "cancelled" {
                append_podcast_log("audio_description.create.closed_after_cancel");
                true
            } else {
                append_podcast_log(&format!("audio_description.create.failed error={error}"));
                show_error(dialog, &error);
                false
            }
        }
    }
}


#[derive(Clone, Debug)]
struct AudioDescriptionVoiceSettings {
    engine: String,
    voice: String,
    rate: i32,
    volume: i32,
}

fn audio_description_voice_rate_items() -> Vec<(String, i32)> {
    vec![
        (tr("audio_description.voice_settings.speed.extremely_slow"), -100),
        (tr("audio_description.voice_settings.speed.very_slow"), -60),
        (tr("audio_description.voice_settings.speed.slow"), -35),
        (tr("audio_description.voice_settings.speed.a_bit_slow"), -20),
        (tr("audio_description.voice_settings.speed.slightly_slow"), -10),
        (tr("audio_description.voice_settings.speed.normal"), 0),
        (tr("audio_description.voice_settings.speed.slightly_fast"), 10),
        (tr("audio_description.voice_settings.speed.a_bit_fast"), 20),
        (tr("audio_description.voice_settings.speed.fast"), 35),
        (tr("audio_description.voice_settings.speed.very_fast"), 50),
        (tr("audio_description.voice_settings.speed.super_fast"), 100),
    ]
}

fn audio_description_voice_volume_items() -> Vec<(String, i32)> {
    vec![
        (tr("audio_description.voice_settings.volume.very_low"), 25),
        (tr("audio_description.voice_settings.volume.low"), 40),
        (tr("audio_description.voice_settings.volume.a_bit_low"), 55),
        (tr("audio_description.voice_settings.volume.medium_low"), 70),
        (tr("audio_description.voice_settings.volume.slightly_low"), 85),
        (tr("audio_description.voice_settings.volume.normal"), 100),
        (tr("audio_description.voice_settings.volume.slightly_high"), 115),
        (tr("audio_description.voice_settings.volume.medium_high"), 130),
        (tr("audio_description.voice_settings.volume.a_bit_high"), 145),
        (tr("audio_description.voice_settings.volume.high"), 160),
        (tr("audio_description.voice_settings.volume.very_high"), 180),
        (tr("audio_description.voice_settings.volume.maximum"), 200),
    ]
}

fn nearest_audio_description_voice_value(values: &[(String, i32)], selected: i32) -> usize {
    values
        .iter()
        .enumerate()
        .min_by_key(|(_, (_, value))| (*value - selected).abs())
        .map(|(index, _)| index)
        .unwrap_or(0)
}

fn open_audio_description_voice_settings(
    parent: &Dialog,
    current: AudioDescriptionVoiceSettings,
    description_language: &str,
    voices_data: &Arc<Mutex<Vec<VoiceInfo>>>,
    preview_pitch: i32,
) -> Option<AudioDescriptionVoiceSettings> {
    let d = Dialog::builder(parent, &tr("audio_description.voice_settings.title"))
        .with_style(DialogStyle::DefaultDialogStyle | DialogStyle::ResizeBorder)
        .with_size(620, 470)
        .build();
    let p = Panel::builder(&d).build();
    let root = BoxSizer::builder(Orientation::Vertical).build();

    let engine_row = BoxSizer::builder(Orientation::Horizontal).build();
    engine_row.add(
        &StaticText::builder(&p)
            .with_label(&tr("audio_description.voice_settings.engine"))
            .build(),
        0,
        SizerFlag::AlignCenterVertical | SizerFlag::All,
        5,
    );
    let engine = Choice::builder(&p).build();
    engine.append(&tr("audio_description.engine.edge"));
    engine.append(&tr("audio_description.engine.system"));
    let initial_engine = if crate::is_system_voice_engine(&current.engine) { 1 } else { 0 };
    engine.set_selection(initial_engine);
    engine_row.add(&engine, 1, SizerFlag::Expand | SizerFlag::All, 5);
    root.add_sizer(&engine_row, 0, SizerFlag::Expand, 0);

    let langs = language_choices();
    let language_row = BoxSizer::builder(Orientation::Horizontal).build();
    language_row.add(
        &StaticText::builder(&p)
            .with_label(&tr("audio_description.voice_settings.language"))
            .build(),
        0,
        SizerFlag::AlignCenterVertical | SizerFlag::All,
        5,
    );
    let language = Choice::builder(&p).build();
    for (name, _) in &langs {
        language.append(name);
    }
    language_row.add(&language, 1, SizerFlag::Expand | SizerFlag::All, 5);
    root.add_sizer(&language_row, 0, SizerFlag::Expand, 0);

    let voice_row = BoxSizer::builder(Orientation::Horizontal).build();
    voice_row.add(
        &StaticText::builder(&p)
            .with_label(&tr("audio_description.voice_settings.voice"))
            .build(),
        0,
        SizerFlag::AlignCenterVertical | SizerFlag::All,
        5,
    );
    let voice = Choice::builder(&p).build();
    voice_row.add(&voice, 1, SizerFlag::Expand | SizerFlag::All, 5);
    root.add_sizer(&voice_row, 0, SizerFlag::Expand, 0);

    let rate_items = Rc::new(audio_description_voice_rate_items());
    let rate_row = BoxSizer::builder(Orientation::Horizontal).build();
    rate_row.add(
        &StaticText::builder(&p)
            .with_label(&tr("audio_description.voice_settings.rate"))
            .build(),
        0,
        SizerFlag::AlignCenterVertical | SizerFlag::All,
        5,
    );
    let rate = Choice::builder(&p).build();
    for (label, _) in rate_items.iter() {
        rate.append(label);
    }
    rate.set_selection(nearest_audio_description_voice_value(&rate_items, current.rate) as u32);
    rate_row.add(&rate, 1, SizerFlag::Expand | SizerFlag::All, 5);
    root.add_sizer(&rate_row, 0, SizerFlag::Expand, 0);

    let volume_items = Rc::new(audio_description_voice_volume_items());
    let volume_row = BoxSizer::builder(Orientation::Horizontal).build();
    volume_row.add(
        &StaticText::builder(&p)
            .with_label(&tr("audio_description.voice_settings.volume"))
            .build(),
        0,
        SizerFlag::AlignCenterVertical | SizerFlag::All,
        5,
    );
    let volume = Choice::builder(&p).build();
    for (label, _) in volume_items.iter() {
        volume.append(label);
    }
    volume.set_selection(nearest_audio_description_voice_value(&volume_items, current.volume) as u32);
    volume_row.add(&volume, 1, SizerFlag::Expand | SizerFlag::All, 5);
    root.add_sizer(&volume_row, 0, SizerFlag::Expand, 0);

    let voices_edge = voices_data.lock().unwrap().clone();
    let voices_system = crate::load_system_voices();
    let active_voices = Rc::new(RefCell::new(Vec::<VoiceInfo>::new()));
    let preferred_voice = current.voice.clone();

    let preferred_language = voices_edge
        .iter()
        .chain(voices_system.iter())
        .find(|item| item.short_name == preferred_voice)
        .map(|item| {
            item.locale
                .split(['-', '_'])
                .next()
                .unwrap_or(description_language)
                .to_ascii_lowercase()
        })
        .unwrap_or_else(|| description_language.to_ascii_lowercase());
    let initial_language = langs
        .iter()
        .position(|(_, code)| code.eq_ignore_ascii_case(&preferred_language))
        .or_else(|| {
            langs.iter()
                .position(|(_, code)| code.eq_ignore_ascii_case(description_language))
        })
        .unwrap_or(0);
    language.set_selection(initial_language as u32);

    let fill_voice: Rc<dyn Fn(u32, u32, &str)> = {
        let active = active_voices.clone();
        let voice_c = voice;
        let voices_edge = voices_edge.clone();
        let voices_system = voices_system.clone();
        let langs = langs.clone();
        Rc::new(move |engine_idx, lang_idx, preferred| {
            voice_c.clear();
            let code = langs.get(lang_idx as usize).map(|item| item.1).unwrap_or("it");
            let source = if engine_idx == 1 { &voices_system } else { &voices_edge };
            let list = source
                .iter()
                .filter(|item| voice_matches_language(item, code))
                .cloned()
                .collect::<Vec<_>>();
            let selected = list
                .iter()
                .position(|item| item.short_name == preferred)
                .unwrap_or(0);
            for item in &list {
                voice_c.append(&item.friendly_name);
            }
            if !list.is_empty() {
                voice_c.set_selection(selected as u32);
            }
            *active.borrow_mut() = list;
        })
    };
    fill_voice(initial_engine, initial_language as u32, &preferred_voice);

    let button_row = BoxSizer::builder(Orientation::Horizontal).build();
    let test = Button::builder(&p)
        .with_label(&tr("audio_description.voice_settings.test"))
        .build();
    let ok = Button::builder(&p)
        .with_id(ID_OK)
        .with_label(&tr("audio_description.voice_settings.ok"))
        .build();
    let cancel = Button::builder(&p)
        .with_id(ID_CANCEL)
        .with_label(&tr("audio_description.voice_settings.cancel"))
        .build();
    button_row.add(&test, 0, SizerFlag::All, 8);
    button_row.add_spacer(1);
    button_row.add(&ok, 0, SizerFlag::All, 8);
    button_row.add(&cancel, 0, SizerFlag::All, 8);
    root.add_sizer(&button_row, 0, SizerFlag::Expand, 0);
    p.set_sizer(root, true);
    d.set_affirmative_id(ID_OK);
    d.set_escape_id(ID_CANCEL);

    let fill_engine = fill_voice.clone();
    let preferred_engine = preferred_voice.clone();
    engine.on_selection_changed(move |_| {
        fill_engine(
            engine.get_selection().unwrap_or(0),
            language.get_selection().unwrap_or(0),
            &preferred_engine,
        );
    });
    let fill_language = fill_voice.clone();
    let preferred_language_voice = preferred_voice.clone();
    language.on_selection_changed(move |_| {
        fill_language(
            engine.get_selection().unwrap_or(0),
            language.get_selection().unwrap_or(0),
            &preferred_language_voice,
        );
    });

    let active_test = active_voices.clone();
    let rates_test = rate_items.clone();
    let volumes_test = volume_items.clone();
    test.on_click(move |_| {
        let voice_index = voice.get_selection().unwrap_or(0) as usize;
        let Some(selected_voice) = active_test.borrow().get(voice_index).cloned() else {
            return;
        };
        let engine_value = if engine.get_selection().unwrap_or(0) == 1 {
            "system".to_string()
        } else {
            "microsoft".to_string()
        };
        let rate_value = rates_test
            .get(rate.get_selection().unwrap_or(0) as usize)
            .map(|item| item.1)
            .unwrap_or(0);
        let volume_value = volumes_test
            .get(volume.get_selection().unwrap_or(0) as usize)
            .map(|item| item.1)
            .unwrap_or(100);
        crate::play_voice_preview(
            tr("audio_description.voice_settings.test_text"),
            engine_value,
            selected_voice.short_name,
            rate_value,
            preview_pitch,
            volume_value,
        );
    });

    let result = Rc::new(RefCell::new(None::<AudioDescriptionVoiceSettings>));
    let result_ok = result.clone();
    let active_ok = active_voices.clone();
    let rates_ok = rate_items.clone();
    let volumes_ok = volume_items.clone();
    let d_ok = d;
    ok.on_click(move |_| {
        let voice_index = voice.get_selection().unwrap_or(0) as usize;
        let Some(selected_voice) = active_ok.borrow().get(voice_index).cloned() else {
            show_error(&d_ok, &tr("audio_description.error.voice"));
            voice.set_focus();
            return;
        };
        let engine_value = if engine.get_selection().unwrap_or(0) == 1 {
            "system".to_string()
        } else {
            "microsoft".to_string()
        };
        let rate_value = rates_ok
            .get(rate.get_selection().unwrap_or(0) as usize)
            .map(|item| item.1)
            .unwrap_or(current.rate);
        let volume_value = volumes_ok
            .get(volume.get_selection().unwrap_or(0) as usize)
            .map(|item| item.1)
            .unwrap_or(current.volume);
        *result_ok.borrow_mut() = Some(AudioDescriptionVoiceSettings {
            engine: engine_value,
            voice: selected_voice.short_name,
            rate: rate_value,
            volume: volume_value,
        });
        d_ok.end_modal(ID_OK);
    });
    let d_cancel = d;
    cancel.on_click(move |_| d_cancel.end_modal(ID_CANCEL));

    engine.set_focus();
    let modal_result = d.show_modal();
    let selected = if modal_result == ID_OK {
        result.borrow().clone()
    } else {
        None
    };
    d.destroy();
    selected
}

pub fn open_create_dialog(
    parent: &Frame,
    settings: &Arc<Mutex<Settings>>,
    rt: &Arc<Runtime>,
    voices_data: &Arc<Mutex<Vec<VoiceInfo>>>,
) {
    open_create_dialog_impl(parent, parent, settings, rt, voices_data, None);
}

pub fn open_create_dialog_with_input(
    dialog_parent: &dyn WxWidget,
    main_parent: &Frame,
    settings: &Arc<Mutex<Settings>>,
    rt: &Arc<Runtime>,
    voices_data: &Arc<Mutex<Vec<VoiceInfo>>>,
    input_path: PathBuf,
) {
    open_create_dialog_impl(
        dialog_parent,
        main_parent,
        settings,
        rt,
        voices_data,
        Some(input_path),
    );
}

fn open_create_dialog_impl(
    dialog_parent: &dyn WxWidget,
    main_parent: &Frame,
    settings: &Arc<Mutex<Settings>>,
    rt: &Arc<Runtime>,
    voices_data: &Arc<Mutex<Vec<VoiceInfo>>>,
    initial_input: Option<PathBuf>,
) {
    let saved = settings.lock().unwrap().clone();
    let d = Dialog::builder(dialog_parent, &tr("audio_description.title"))
        .with_style(DialogStyle::DefaultDialogStyle | DialogStyle::ResizeBorder)
        .with_size(760, 680)
        .build();
    let p = Panel::builder(&d).build();
    let root = BoxSizer::builder(Orientation::Vertical).build();
    let input_label = StaticText::builder(&p)
        .with_label(&tr("audio_description.input"))
        .build();
    let input_btn = Button::builder(&p)
        .with_label(&tr("audio_description.browse_input"))
        .build();
    let input = TextCtrl::builder(&p).build();
    let input_row = BoxSizer::builder(Orientation::Horizontal).build();
    input_row.add(
        &input_label,
        0,
        SizerFlag::AlignCenterVertical | SizerFlag::All,
        5,
    );
    input_row.add(&input_btn, 0, SizerFlag::All, 5);
    input_row.add(&input, 1, SizerFlag::Expand | SizerFlag::All, 5);
    root.add_sizer(&input_row, 0, SizerFlag::Expand, 0);
    let output_label = StaticText::builder(&p)
        .with_label(&tr("audio_description.output"))
        .build();
    let output_btn = Button::builder(&p)
        .with_label(&tr("audio_description.browse_output"))
        .build();
    // Audio-only creation keeps the historical *_audiodescritto.mp3 default;
    // video-output mode switches the suggested destination to MP4.
    let output = TextCtrl::builder(&p).build();
    let output_row = BoxSizer::builder(Orientation::Horizontal).build();
    output_row.add(
        &output_label,
        0,
        SizerFlag::AlignCenterVertical | SizerFlag::All,
        5,
    );
    output_row.add(&output_btn, 0, SizerFlag::All, 5);
    output_row.add(&output, 1, SizerFlag::Expand | SizerFlag::All, 5);
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
    let language_label = StaticText::builder(&p)
        .with_label(&tr("audio_description.language"))
        .build();
    let row = BoxSizer::builder(Orientation::Horizontal).build();
    row.add(
        &language_label,
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
    let verbosity_label = StaticText::builder(&p)
        .with_label(&tr("audio_description.verbosity"))
        .build();
    let row = BoxSizer::builder(Orientation::Horizontal).build();
    row.add(
        &verbosity_label,
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
    let recognize_screen_text = CheckBox::builder(&p)
        .with_label(&tr("audio_description.recognize_screen_text"))
        .build();
    recognize_screen_text.set_value(saved.audio_description_recognize_screen_text);
    root.add(
        &recognize_screen_text,
        0,
        SizerFlag::Expand | SizerFlag::All,
        5,
    );
    let create_video_output = CheckBox::builder(&p)
        .with_label(&tr("audio_description.create_video"))
        .build();
    create_video_output.set_value(saved.audio_description_create_video_output);
    root.add(
        &create_video_output,
        0,
        SizerFlag::Expand | SizerFlag::All,
        5,
    );
    extended.enable(!saved.audio_description_create_video_output);
    let save_project_box = CheckBox::builder(&p)
        .with_label(&tr("audio_description.save_project"))
        .build();
    save_project_box.set_value(saved.audio_description_save_project);
    root.add(&save_project_box, 0, SizerFlag::Expand | SizerFlag::All, 5);
    let delete_input_box = CheckBox::builder(&p)
        .with_label(&tr("audio_description.delete_video_after"))
        .build();
    delete_input_box.set_value(saved.audio_description_delete_video_after);
    delete_input_box.show(!saved.audio_description_save_project);
    root.add(&delete_input_box, 0, SizerFlag::Expand | SizerFlag::All, 5);
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
    let ai_access_label = StaticText::builder(&p)
        .with_label(&tr("audio_description.ai_access"))
        .build();
    let ai_access = Choice::builder(&p).build();
    ai_access.append(&tr("audio_description.ai_access.personal"));
    ai_access.append(&tr("audio_description.ai_access.sonarpad"));
    ai_access.set_selection(if saved.audio_description_use_sonarpad_ai { 1 } else { 0 });
    let ai_access_row = BoxSizer::builder(Orientation::Horizontal).build();
    ai_access_row.add(
        &ai_access_label,
        0,
        SizerFlag::AlignCenterVertical | SizerFlag::All,
        5,
    );
    ai_access_row.add(&ai_access, 1, SizerFlag::Expand | SizerFlag::All, 5);
    root.add_sizer(&ai_access_row, 0, SizerFlag::Expand, 0);

    let api_label = StaticText::builder(&p)
        .with_label(&tr("audio_description.gemini_api_key"))
        .build();
    let api = TextCtrl::builder(&p)
        .with_style(TextCtrlStyle::Password)
        .build();
    api.set_value(&saved.audio_description_gemini_api_key);
    let api_visible = TextCtrl::builder(&p).build();
    api_visible.set_value(&saved.audio_description_gemini_api_key);
    api_visible.show(false);
    let show_api_key = CheckBox::builder(&p)
        .with_label(&tr("audio_description.gemini_show_api_key"))
        .build();
    show_api_key.set_value(false);
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
    api_row.add(&api_visible, 1, SizerFlag::Expand | SizerFlag::All, 5);
    api_row.add(
        &show_api_key,
        0,
        SizerFlag::AlignCenterVertical | SizerFlag::All,
        5,
    );
    api_row.add(&api_get, 0, SizerFlag::All, 5);
    root.add_sizer(&api_row, 0, SizerFlag::Expand, 0);

    let sonarpad_code_label = StaticText::builder(&p)
        .with_label(&tr("audio_description.sonarpad_code"))
        .build();
    let sonarpad_code = TextCtrl::builder(&p)
        .with_style(TextCtrlStyle::Password)
        .build();
    sonarpad_code.set_value(&saved.sonarpad_ai_access_code);
    let sonarpad_code_visible = TextCtrl::builder(&p).build();
    sonarpad_code_visible.set_value(&saved.sonarpad_ai_access_code);
    sonarpad_code_visible.show(false);
    let show_sonarpad_code = CheckBox::builder(&p)
        .with_label(&tr("audio_description.sonarpad_show_code"))
        .build();
    show_sonarpad_code.set_value(false);
    let sonarpad_code_row = BoxSizer::builder(Orientation::Horizontal).build();
    sonarpad_code_row.add(
        &sonarpad_code_label,
        0,
        SizerFlag::AlignCenterVertical | SizerFlag::All,
        5,
    );
    sonarpad_code_row.add(&sonarpad_code, 1, SizerFlag::Expand | SizerFlag::All, 5);
    sonarpad_code_row.add(
        &sonarpad_code_visible,
        1,
        SizerFlag::Expand | SizerFlag::All,
        5,
    );
    sonarpad_code_row.add(
        &show_sonarpad_code,
        0,
        SizerFlag::AlignCenterVertical | SizerFlag::All,
        5,
    );
    root.add_sizer(&sonarpad_code_row, 0, SizerFlag::Expand, 0);

    let sonarpad_balance_label = StaticText::builder(&p)
        .with_label(&tr("audio_description.sonarpad_balance"))
        .build();
    let sonarpad_balance = TextCtrl::builder(&p)
        .with_style(TextCtrlStyle::ReadOnly)
        .build();
    sonarpad_balance.set_value(&tr("audio_description.sonarpad_balance_unavailable"));
    let sonarpad_request_code = Button::builder(&p)
        .with_label(&tr("audio_description.sonarpad_request_code"))
        .build();
    let sonarpad_balance_row = BoxSizer::builder(Orientation::Horizontal).build();
    sonarpad_balance_row.add(
        &sonarpad_balance_label,
        0,
        SizerFlag::AlignCenterVertical | SizerFlag::All,
        5,
    );
    sonarpad_balance_row.add(&sonarpad_balance, 1, SizerFlag::Expand | SizerFlag::All, 5);
    sonarpad_balance_row.add(&sonarpad_request_code, 0, SizerFlag::All, 5);
    root.add_sizer(&sonarpad_balance_row, 0, SizerFlag::Expand, 0);

    let initial_sonarpad_ai = saved.audio_description_use_sonarpad_ai;
    sonarpad_code_label.show(initial_sonarpad_ai);
    sonarpad_code.show(initial_sonarpad_ai);
    sonarpad_code_visible.show(false);
    show_sonarpad_code.show(initial_sonarpad_ai);
    sonarpad_balance_label.show(initial_sonarpad_ai);
    sonarpad_balance.show(initial_sonarpad_ai);
    sonarpad_request_code.show(initial_sonarpad_ai);
    api_label.show(!initial_sonarpad_ai);
    api.show(!initial_sonarpad_ai);
    api_visible.show(false);
    show_api_key.show(!initial_sonarpad_ai);
    api_get.show(!initial_sonarpad_ai);

    let api_value: Rc<dyn Fn() -> String> = Rc::new(move || {
        if show_api_key.get_value() {
            api_visible.get_value()
        } else {
            api.get_value()
        }
    });
    let sonarpad_code_value: Rc<dyn Fn() -> String> = Rc::new(move || {
        if show_sonarpad_code.get_value() {
            sonarpad_code_visible.get_value()
        } else {
            sonarpad_code.get_value()
        }
    });
    let model_label = StaticText::builder(&p)
        .with_label(&tr("audio_description.gemini_model"))
        .build();
    let model = Choice::builder(&p).build();
    let personal_gemini_model = Rc::new(RefCell::new(saved.audio_description_gemini_model.clone()));
    let initial_model = if initial_sonarpad_ai {
        SONARPAD_AI_FORCED_GEMINI_MODEL
    } else {
        saved.audio_description_gemini_model.as_str()
    };
    model.append(initial_model);
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
    model.enable(!initial_sonarpad_ai);
    refresh.enable(!initial_sonarpad_ai);

    let balance_result = Arc::new(Mutex::new(None::<(String, Result<f64, String>)>));
    let last_balance_code = Rc::new(RefCell::new(String::new()));
    let settings_balance = settings.clone();
    let sonarpad_code_value_balance = Rc::clone(&sonarpad_code_value);
    let balance_result_request = Arc::clone(&balance_result);
    let last_balance_code_request = Rc::clone(&last_balance_code);
    let request_sonarpad_balance: Rc<dyn Fn()> = Rc::new(move || {
        if ai_access.get_selection().unwrap_or(0) != 1 {
            return;
        }
        let access_code = sonarpad_code_value_balance().trim().to_string();
        *last_balance_code_request.borrow_mut() = access_code.clone();
        if !access_code.starts_with("sp_") {
            sonarpad_balance.set_value(&tr("audio_description.sonarpad_balance_unavailable"));
            return;
        }
        let device_id = {
            let mut st = settings_balance.lock().unwrap();
            st.sonarpad_ai_access_code = access_code.clone();
            let device_id = st.sonarpad_ai_device_id.trim().to_string();
            st.save();
            device_id
        };
        if device_id.is_empty() {
            sonarpad_balance.set_value(&tr("audio_description.sonarpad_balance_unavailable"));
            return;
        }
        sonarpad_balance.set_value("…");
        let result_state = Arc::clone(&balance_result_request);
        thread::spawn(move || {
            let result = fetch_sonarpad_balance(&access_code, &device_id);
            *result_state.lock().unwrap() = Some((access_code, result));
        });
    });

    let balance_timer = Rc::new(Timer::new(&d));
    let balance_timer_tick = Rc::clone(&balance_timer);
    let balance_result_tick = Arc::clone(&balance_result);
    let last_seen_balance_code = Rc::new(RefCell::new(sonarpad_code_value().trim().to_string()));
    let last_seen_balance_code_tick = Rc::clone(&last_seen_balance_code);
    let stable_balance_ticks = Rc::new(Cell::new(0_u32));
    let stable_balance_ticks_tick = Rc::clone(&stable_balance_ticks);
    let last_balance_code_tick = Rc::clone(&last_balance_code);
    let sonarpad_code_value_tick = Rc::clone(&sonarpad_code_value);
    let request_sonarpad_balance_tick = Rc::clone(&request_sonarpad_balance);
    balance_timer_tick.on_tick(move |_| {
        if let Some((access_code, result)) = balance_result_tick.lock().unwrap().take() {
            let current_code = sonarpad_code_value_tick().trim().to_string();
            if ai_access.get_selection().unwrap_or(0) == 1 && current_code == access_code {
                match result {
                    Ok(balance) => sonarpad_balance.set_value(&format_sonarpad_balance(balance)),
                    Err(error) => {
                        append_podcast_log(&format!(
                            "audio_description.sonarpad_balance_unavailable error={error}"
                        ));
                        sonarpad_balance
                            .set_value(&tr("audio_description.sonarpad_balance_unavailable"));
                    }
                }
            }
        }

        if ai_access.get_selection().unwrap_or(0) != 1 {
            stable_balance_ticks_tick.set(0);
            return;
        }
        let current_code = sonarpad_code_value_tick().trim().to_string();
        if current_code != *last_seen_balance_code_tick.borrow() {
            *last_seen_balance_code_tick.borrow_mut() = current_code.clone();
            stable_balance_ticks_tick.set(0);
            if !current_code.starts_with("sp_") {
                sonarpad_balance.set_value(&tr("audio_description.sonarpad_balance_unavailable"));
            }
            return;
        }
        if current_code != *last_balance_code_tick.borrow() {
            let ticks = stable_balance_ticks_tick.get().saturating_add(1);
            stable_balance_ticks_tick.set(ticks);
            if ticks >= 4 {
                stable_balance_ticks_tick.set(0);
                request_sonarpad_balance_tick();
            }
        }
    });

    let ai_panel_toggle = p;
    let ai_dialog_toggle = d;
    let settings_ai_access = settings.clone();
    let api_value_ai_access = Rc::clone(&api_value);
    let sonarpad_code_value_ai_access = Rc::clone(&sonarpad_code_value);
    let request_sonarpad_balance_ai_access = Rc::clone(&request_sonarpad_balance);
    let personal_gemini_model_ai_access = Rc::clone(&personal_gemini_model);
    ai_access.on_selection_changed(move |_| {
        let service = ai_access.get_selection().unwrap_or(0) == 1;
        let current_model = model
            .get_string_selection()
            .unwrap_or_default()
            .trim()
            .to_string();

        // When entering Sonarpad AI, remember the personal model first. The
        // service model is forced exactly as on Windows and must never replace
        // the user's personal-model preference.
        if service
            && !current_model.is_empty()
            && current_model != SONARPAD_AI_FORCED_GEMINI_MODEL
        {
            *personal_gemini_model_ai_access.borrow_mut() = current_model.clone();
        }
        let personal_model = personal_gemini_model_ai_access.borrow().clone();

        // Persist both credential sets before changing the visible controls. The
        // inactive credential/model must never be cleared merely because the user
        // switches between the personal Gemini key and Sonarpad AI.
        {
            let mut st = settings_ai_access.lock().unwrap();
            st.audio_description_use_sonarpad_ai = service;
            st.audio_description_gemini_api_key = api_value_ai_access().trim().to_string();
            st.sonarpad_ai_access_code = sonarpad_code_value_ai_access().trim().to_string();
            if service {
                if !current_model.is_empty()
                    && current_model != SONARPAD_AI_FORCED_GEMINI_MODEL
                {
                    st.audio_description_gemini_model = current_model;
                }
            } else if !personal_model.trim().is_empty() {
                st.audio_description_gemini_model = personal_model.clone();
            }
            st.save();
        }

        model.clear();
        if service {
            model.append(SONARPAD_AI_FORCED_GEMINI_MODEL);
        } else {
            let restored = if personal_model.trim().is_empty() {
                "gemini-3.5-flash-lite"
            } else {
                personal_model.as_str()
            };
            model.append(restored);
        }
        model.set_selection(0);

        api_label.show(!service);
        if service {
            api.show(false);
            api_visible.show(false);
        } else if show_api_key.get_value() {
            api.show(false);
            api_visible.show(true);
        } else {
            api_visible.show(false);
            api.show(true);
        }
        show_api_key.show(!service);
        api_get.show(!service);
        sonarpad_code_label.show(service);
        if service && show_sonarpad_code.get_value() {
            sonarpad_code.show(false);
            sonarpad_code_visible.show(true);
        } else {
            sonarpad_code_visible.show(false);
            sonarpad_code.show(service);
        }
        show_sonarpad_code.show(service);
        sonarpad_balance_label.show(service);
        sonarpad_balance.show(service);
        sonarpad_request_code.show(service);
        model.enable(!service);
        refresh.enable(!service);
        if service {
            request_sonarpad_balance_ai_access();
        }
        ai_panel_toggle.layout();
        ai_dialog_toggle.layout();
    });

    // Keep the voice controls out of the main creation window, as on Windows.
    // Until the user opens "Regola voce", rate and volume deliberately inherit
    // the global speech settings so existing installations behave exactly as before.
    let initial_voice_engine = if crate::is_system_voice_engine(&saved.audio_description_tts_engine) {
        "system".to_string()
    } else {
        "microsoft".to_string()
    };
    let voices_edge = voices_data.lock().unwrap().clone();
    let voices_system = crate::load_system_voices();
    let initial_voice_source = if crate::is_system_voice_engine(&initial_voice_engine) {
        &voices_system
    } else {
        &voices_edge
    };
    let mut initial_voice = if saved.audio_description_tts_voice.trim().is_empty() {
        if crate::is_system_voice_engine(&initial_voice_engine) {
            saved.system_voice.clone()
        } else {
            saved.voice.clone()
        }
    } else {
        saved.audio_description_tts_voice.clone()
    };
    if initial_voice.trim().is_empty()
        || !initial_voice_source.iter().any(|item| item.short_name == initial_voice)
    {
        initial_voice = initial_voice_source
            .iter()
            .find(|item| voice_matches_language(item, &saved.audio_description_language))
            .or_else(|| initial_voice_source.first())
            .map(|item| item.short_name.clone())
            .unwrap_or_default();
    }
    let voice_settings = Rc::new(RefCell::new(AudioDescriptionVoiceSettings {
        engine: initial_voice_engine,
        voice: initial_voice,
        rate: saved.audio_description_tts_rate.unwrap_or(saved.rate),
        volume: saved.audio_description_tts_volume.unwrap_or(saved.volume),
    }));
    let actions = BoxSizer::builder(Orientation::Horizontal).build();
    let modify = Button::builder(&p)
        .with_label(&tr("audio_description.modify_project"))
        .build();
    let continue_interrupted = Button::builder(&p)
        .with_label(&tr("audio_description.resume.title"))
        .build();
    let voice_settings_button = Button::builder(&p)
        .with_label(&tr("audio_description.voice_settings"))
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
    actions.add(&continue_interrupted, 0, SizerFlag::All, 8);
    actions.add_spacer(1);
    actions.add(&voice_settings_button, 0, SizerFlag::All, 8);
    actions.add(&start, 0, SizerFlag::All, 8);
    actions.add(&close, 0, SizerFlag::All, 8);
    root.add_sizer(&actions, 0, SizerFlag::Expand, 0);
    p.set_sizer(root, true);

    if let Some(path) = initial_input.as_ref() {
        input.set_value(&path.to_string_lossy());
        if catalog_choice.get_selection().unwrap_or(0) == 0
            && catalog_name.get_value().trim().is_empty()
        {
            let suggested = suggested_catalog_name(&path.to_string_lossy());
            if !suggested.is_empty() {
                catalog_name.set_value(&suggested);
            }
        }
        let destination = suggested_audio_description_output(
            path,
            saved.audio_description_create_video_output,
        );
        output.set_value(&destination.to_string_lossy());
        append_podcast_log(&format!(
            "audio_description.create.prefilled_input source={} output={}",
            path.display(),
            destination.display()
        ));
    }

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
            // A newly selected source is a new audio-description job. Never carry
            // the destination of the previous completed job into it: regenerate the
            // suggested output from the new source even when the creation dialog stays open.
            let dest = suggested_audio_description_output(
                &path,
                create_video_output.get_value(),
            );
            output.set_value(&dest.to_string_lossy());
        }
    });
    let d_output = d;
    output_btn.on_click(move |_| {
        let ip = PathBuf::from(input.get_value());
        if let Some(path) = choose_output(&d_output, &ip, create_video_output.get_value()) {
            output.set_value(&path.to_string_lossy());
        }
    });
    let extended_video_toggle = extended;
    let output_video_toggle = output;
    let input_video_toggle = input;
    create_video_output.on_toggled(move |_| {
        let enabled = create_video_output.get_value();
        extended_video_toggle.enable(!enabled);
        let input_path = PathBuf::from(input_video_toggle.get_value());
        if !input_path.as_os_str().is_empty() {
            let current = PathBuf::from(output_video_toggle.get_value());
            let should_refresh = current.as_os_str().is_empty()
                || current
                    .extension()
                    .and_then(|value| value.to_str())
                    .is_some_and(|extension| {
                        extension.eq_ignore_ascii_case("mp3")
                            || extension.eq_ignore_ascii_case("mp4")
                            || extension.eq_ignore_ascii_case("mkv")
                    });
            if should_refresh {
                let destination = suggested_audio_description_output(&input_path, enabled);
                output_video_toggle.set_value(&destination.to_string_lossy());
            }
        }
    });

    let delete_input_toggle = delete_input_box;
    let panel_save_project = p;
    let dialog_save_project = d;
    save_project_box.on_toggled(move |_| {
        let show_delete_input = !save_project_box.get_value();
        delete_input_toggle.show(show_delete_input);
        panel_save_project.layout();
        dialog_save_project.layout();
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
    let api_password_toggle = api;
    let api_visible_toggle = api_visible;
    let api_panel_toggle = p;
    let api_dialog_toggle = d;
    show_api_key.on_toggled(move |_| {
        if show_api_key.get_value() {
            api_visible_toggle.set_value(&api_password_toggle.get_value());
            api_password_toggle.show(false);
            api_visible_toggle.show(true);
        } else {
            api_password_toggle.set_value(&api_visible_toggle.get_value());
            api_visible_toggle.show(false);
            api_password_toggle.show(true);
        }
        api_panel_toggle.layout();
        api_dialog_toggle.layout();
    });
    let sonarpad_password_toggle = sonarpad_code;
    let sonarpad_visible_toggle = sonarpad_code_visible;
    let sonarpad_panel_toggle = p;
    let sonarpad_dialog_toggle = d;
    show_sonarpad_code.on_toggled(move |_| {
        if show_sonarpad_code.get_value() {
            sonarpad_visible_toggle.set_value(&sonarpad_password_toggle.get_value());
            sonarpad_password_toggle.show(false);
            sonarpad_visible_toggle.show(true);
        } else {
            sonarpad_password_toggle.set_value(&sonarpad_visible_toggle.get_value());
            sonarpad_visible_toggle.show(false);
            sonarpad_password_toggle.show(true);
        }
        sonarpad_panel_toggle.layout();
        sonarpad_dialog_toggle.layout();
    });
    let d_sonarpad_request = d;
    sonarpad_request_code.on_click(move |_| {
        if let Err(error) = crate::open_url_in_browser("https://sonarpad.com/contact.php") {
            show_error(&d_sonarpad_request, &error);
        }
    });
    let d_api = d;
    api_get.on_click(move |_| {
        if let Err(e) = crate::open_url_in_browser("https://aistudio.google.com/app/apikey") {
            show_error(&d_api, &e);
        }
    });
    let d_refresh = d;
    let api_value_refresh = Rc::clone(&api_value);
    refresh.on_click(move |_| match fetch_gemini_models(&api_value_refresh()) {
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
    let voice_settings_language = voice_settings.clone();
    let voices_edge_language = voices_edge.clone();
    let voices_system_language = voices_system.clone();
    let langs_voice_language = langs.clone();
    language.on_selection_changed(move |_| {
        let language_code = langs_voice_language
            .get(language.get_selection().unwrap_or(0) as usize)
            .map(|item| item.1)
            .unwrap_or("it");
        let mut selected = voice_settings_language.borrow_mut();
        let source = if crate::is_system_voice_engine(&selected.engine) {
            &voices_system_language
        } else {
            &voices_edge_language
        };
        selected.voice = source
            .iter()
            .find(|item| voice_matches_language(item, language_code))
            .or_else(|| source.first())
            .map(|item| item.short_name.clone())
            .unwrap_or_default();
    });
    let voice_settings_dialog = voice_settings.clone();
    let settings_voice = settings.clone();
    let voices_voice = voices_data.clone();
    let langs_voice = langs.clone();
    let d_voice = d;
    voice_settings_button.on_click(move |_| {
        let description_language = langs_voice
            .get(language.get_selection().unwrap_or(0) as usize)
            .map(|item| item.1)
            .unwrap_or("it");
        let current = voice_settings_dialog.borrow().clone();
        if let Some(selected) = open_audio_description_voice_settings(
            &d_voice,
            current,
            description_language,
            &voices_voice,
            saved.pitch,
        ) {
            *voice_settings_dialog.borrow_mut() = selected.clone();
            let mut st = settings_voice.lock().unwrap();
            st.audio_description_tts_engine = selected.engine;
            st.audio_description_tts_voice = selected.voice;
            st.audio_description_tts_rate = Some(selected.rate);
            st.audio_description_tts_volume = Some(selected.volume);
            st.save();
        }
        voice_settings_button.set_focus();
    });
    let open_project_requested = Rc::new(Cell::new(false));
    let open_project_requested_button = Rc::clone(&open_project_requested);
    let d_modify = d;
    modify.on_click(move |_| {
        append_podcast_log("audio_description.create.open_project_requested");
        open_project_requested_button.set(true);
        d_modify.end_modal(ID_AUDIO_DESCRIPTION_CLOSE);
    });
    let settings_resume = settings.clone();
    let rt_resume = rt.clone();
    let parent_resume = *main_parent;
    let fallback_resume_model = saved.audio_description_gemini_model.clone();
    let d_resume = d;
    let api_value_resume = Rc::clone(&api_value);
    let sonarpad_code_value_resume = Rc::clone(&sonarpad_code_value);
    continue_interrupted.on_click(move |_| {
        let Some(selection) = choose_resume_checkpoint(
            &d_resume,
            &api_value_resume(),
            &fallback_resume_model,
        ) else {
            append_podcast_log(
                "audio_description.create.resume_selector_cancelled_closing_creation_window",
            );
            d_resume.end_modal(ID_AUDIO_DESCRIPTION_CLOSE);
            return;
        };
        {
            let mut settings = settings_resume.lock().unwrap();
            remember_audio_description_project_folder(&mut settings, &selection.checkpoint_path);
            settings.save();
        }
        let resume = match load_resume_settings(&selection.checkpoint_path) {
            Ok(value) => value,
            Err(error) => {
                show_error(
                    &d_resume,
                    &trf("audio_description.resume.invalid", &[("error", error)]),
                );
                return;
            }
        };
        let use_sonarpad_ai = ai_access.get_selection().unwrap_or(0) == 1;
        let personal_api_key_value = api_value_resume().trim().to_string();
        let sonarpad_code_value = sonarpad_code_value_resume().trim().to_string();
        if use_sonarpad_ai && sonarpad_code_value.is_empty() {
            show_error(&d_resume, &tr("audio_description.error.sonarpad_code"));
            if show_sonarpad_code.get_value() {
                sonarpad_code_visible.set_focus();
            } else {
                sonarpad_code.set_focus();
            }
            return;
        }
        if !use_sonarpad_ai && personal_api_key_value.is_empty() {
            show_error(&d_resume, &tr("audio_description.error.api_key"));
            api.set_focus();
            return;
        }
        let resume_device_id = settings_resume
            .lock()
            .unwrap()
            .sonarpad_ai_device_id
            .clone();
        let resume_model = if use_sonarpad_ai {
            SONARPAD_AI_FORCED_GEMINI_MODEL.to_string()
        } else {
            selection.gemini_model.clone()
        };
        let job = match job_from_checkpoint(
            &selection.checkpoint_path,
            if use_sonarpad_ai { String::new() } else { personal_api_key_value },
            if use_sonarpad_ai { SONARPAD_AI_SERVICE_URL.to_string() } else { String::new() },
            if use_sonarpad_ai { sonarpad_code_value } else { String::new() },
            if use_sonarpad_ai { resume_device_id } else { String::new() },
            resume_model.clone(),
        ) {
            Ok(job) => job,
            Err(error) => {
                show_error(
                    &d_resume,
                    &trf("audio_description.resume.invalid", &[("error", error)]),
                );
                return;
            }
        };
        append_podcast_log(&format!(
            "audio_description.create.resume_selected chunk={}/{} model={} path={}",
            resume.completed_chunks,
            resume.total_chunks,
            resume_model,
            selection.checkpoint_path.display()
        ));
        if execute_audio_description_job(
            &parent_resume,
            &d_resume,
            &settings_resume,
            &rt_resume,
            job,
            resume_model,
        ) {
            d_resume.end_modal(ID_AUDIO_DESCRIPTION_CLOSE);
        }
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
    let parent_run = *main_parent;
    let settings_run = settings.clone();
    let rt_run = rt.clone();
    let saved_start = saved.clone();
    let api_value_start = Rc::clone(&api_value);
    let sonarpad_code_value_start = Rc::clone(&sonarpad_code_value);
    start.on_click(move |_| {
        let model_value = model
            .get_string_selection()
            .unwrap_or_else(|| saved_start.audio_description_gemini_model.clone());
        if model_value.trim().is_empty() {
            show_error(&d, &tr("audio_description.error.model"));
            model.set_focus();
            return;
        }
        let input_path = PathBuf::from(input.get_value());
        let output_path = PathBuf::from(output.get_value());
        let audio_stream_index = match choose_audio_description_track(&d, &input_path) {
            Ok(Some(stream_index)) => stream_index,
            Ok(None) => {
                append_podcast_log(
                    "audio_description.multitrack_selection_cancelled closing_creation_window",
                );
                d.end_modal(ID_AUDIO_DESCRIPTION_CLOSE);
                return;
            }
            Err(error) => {
                show_error(&d, &error);
                input.set_focus();
                return;
            }
        };
        let lang_idx = language.get_selection().unwrap_or(0) as usize;
        let language_code = langs.get(lang_idx).map(|x| x.1).unwrap_or("it").to_string();
        let verbosity_value = match verbosity.get_selection().unwrap_or(2) {
            0 => Verbosity::Brief,
            1 => Verbosity::Standard,
            _ => Verbosity::Detailed,
        };
        let selected_voice_settings = voice_settings.borrow().clone();
        let engine_value = selected_voice_settings.engine.clone();
        let voice_value = selected_voice_settings.voice.clone();
        if voice_value.trim().is_empty() {
            show_error(&d, &tr("audio_description.error.voice"));
            voice_settings_button.set_focus();
            return;
        }
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
        let use_sonarpad_ai = ai_access.get_selection().unwrap_or(0) == 1;
        let personal_api_key_value = api_value_start().trim().to_string();
        let sonarpad_code_value = sonarpad_code_value_start().trim().to_string();
        if use_sonarpad_ai && sonarpad_code_value.is_empty() {
            show_error(&d, &tr("audio_description.error.sonarpad_code"));
            if show_sonarpad_code.get_value() {
                sonarpad_code_visible.set_focus();
            } else {
                sonarpad_code.set_focus();
            }
            return;
        }
        if !use_sonarpad_ai && personal_api_key_value.is_empty() {
            show_error(&d, &tr("audio_description.error.api_key"));
            api.set_focus();
            return;
        }
        let effective_model = if use_sonarpad_ai {
            SONARPAD_AI_FORCED_GEMINI_MODEL.to_string()
        } else {
            model_value.clone()
        };
        let job = CreateJob {
            input_path,
            output_path,
            language_code,
            verbosity: verbosity_value,
            allow_extended_pauses: extended.get_value() && !create_video_output.get_value(),
            recognize_characters: recognize.get_value(),
            recognize_screen_text: recognize_screen_text.get_value(),
            save_project: save_project_box.get_value(),
            create_video_output: create_video_output.get_value(),
            delete_input_after_success: delete_input_box.get_value(),
            keep_character_catalog: keep_catalog.get_value(),
            catalog,
            tts_engine: engine_value,
            tts_voice: voice_value,
            rate: selected_voice_settings.rate,
            pitch: saved_start.pitch,
            volume: selected_voice_settings.volume,
            audio_stream_index,
            gemini_api_key: if use_sonarpad_ai { String::new() } else { personal_api_key_value },
            sonarpad_ai_service_url: if use_sonarpad_ai {
                SONARPAD_AI_SERVICE_URL.to_string()
            } else {
                String::new()
            },
            sonarpad_ai_access_code: if use_sonarpad_ai {
                sonarpad_code_value
            } else {
                String::new()
            },
            sonarpad_ai_device_id: if use_sonarpad_ai {
                saved_start.sonarpad_ai_device_id.clone()
            } else {
                String::new()
            },
            gemini_model: effective_model.clone(),
            fixed_reanalysis_slots: Vec::new(),
            resume_checkpoint_path: None,
        };
        if execute_audio_description_job(
            &parent_run,
            &d,
            &settings_run,
            &rt_run,
            job,
            effective_model,
        ) {
            d.end_modal(ID_AUDIO_DESCRIPTION_CLOSE);
        }
    });
    balance_timer.start(250, false);
    if initial_sonarpad_ai {
        request_sonarpad_balance();
    }
    d.show_modal();
    balance_timer.stop();
    d.destroy();
    if quit_requested.get() {
        append_podcast_log("audio_description.create.quit_forwarded_to_main");
        main_parent.close(false);
    } else if open_project_requested.get() {
        append_podcast_log("audio_description.create.open_project_after_close");
        open_project_editor(main_parent, settings, rt, voices_data);
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
    done: Option<Result<AudioDescriptionProject, String>>,
    fit_error: Option<ProjectVoiceFitError>,
}

fn change_project_voice(
    project: &AudioDescriptionProject,
    project_file: &Path,
    tts_engine: &str,
    tts_voice: &str,
    rt: &Runtime,
    state: &Arc<Mutex<ProjectVoiceValidationState>>,
) -> Result<AudioDescriptionProject, String> {
    if project.descriptions.is_empty() {
        return Err(tr("audio_description.project.no_selection"));
    }
    let work = cache_dir("project_voice_change")?;
    let temporary_media = temporary_sibling_path(&project.output_mp3_path, "voice");
    let temporary_mp3 = work.join("voice-change.mp3");
    let temporary_project = temporary_sibling_path(project_file, "voice");
    let cancel = Arc::new(AtomicBool::new(false));
    let result = (|| {
        let synthesis_tasks = project
            .descriptions
            .iter()
            .enumerate()
            .map(|(index, description)| AudioDescriptionSynthesisTask {
                original_index: index,
                synthesis_index: index,
                text: description.text.clone(),
                desired_start_sec: description.gemini_start_sec,
                visual_start_sec: description.gemini_start_sec,
                visual_evidence_time_sec: description.visual_evidence_time_sec,
                mandatory: description.mandatory,
                slot_id: description.slot_id.clone(),
                slot_start_sec: description.slot_start_sec,
                slot_end_sec: description.slot_end_sec,
            })
            .collect::<Vec<_>>();
        let synthesized = synthesize_description_tasks_parallel(
            &synthesis_tasks,
            TtsParameters {
                engine: tts_engine,
                voice: tts_voice,
                rate: project.tts_rate,
                pitch: project.tts_pitch,
                volume: project.tts_volume,
            },
            rt,
            &work,
            cancel.clone(),
            |completed, total| {
                state.lock().unwrap().progress =
                    ((completed as i32 * 75) / total.max(1) as i32).clamp(0, 75);
            },
        )?;

        let source = work.join("source.wav");
        let source_duration = decode_source_audio(
            &project.source_path,
            &source,
            project.audio_stream_index,
            &cancel,
        )?.duration_sec;
        state.lock().unwrap().progress = 80;

        let protected = project
            .protected_intervals
            .iter()
            .map(|interval| BridgeInterval {
                start_sec: interval.start_sec,
                end_sec: interval.end_sec,
            })
            .collect::<Vec<_>>();
        let (scheduled, dropped) = schedule_descriptions(
            &synthesized,
            &protected,
            source_duration,
            project.allow_extended_pauses && !project.output_is_video,
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
        if scheduled.len() != project.descriptions.len() {
            return Err("voice_does_not_fit".to_string());
        }
        state.lock().unwrap().progress = 85;

        let mix = work.join("voice-change-mix.wav");
        let output_duration = render_mix(&source, &mix, &scheduled, &cancel)?;
        state.lock().unwrap().progress = 92;

        if let Some(parent) = project.output_mp3_path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("Salvataggio MP3 fallito: {e}"))?;
        }
        encode_mp3(&mix, &temporary_mp3, &cancel)?;
        if project.output_is_video {
            mux_audio_description_video(
                &project.source_path,
                &temporary_mp3,
                &temporary_media,
                &cancel,
            )?;
        } else {
            fs::copy(&temporary_mp3, &temporary_media)
                .map_err(|error| format!("Salvataggio MP3 fallito: {error}"))?;
        }
        let metadata = fs::metadata(&temporary_media)
            .map_err(|e| format!("Verifica del nuovo output audiodescritto fallita: {e}"))?;
        if metadata.len() == 0 {
            return Err("Il nuovo output audiodescritto è vuoto.".to_string());
        }
        state.lock().unwrap().progress = 96;

        let previous = project.clone();
        let mut extra_offset = 0.0;
        let mut descriptions = Vec::with_capacity(scheduled.len());
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
            let (duck_start_sec, duck_end_sec) = project_duck_bounds(
                output_start,
                output_end,
                scheduled_description.extended_pause,
            );
            descriptions.push(ProjectDescription {
                id: new_id,
                text: old.text.clone(),
                original_text: old.original_text.clone(),
                rendered_text: old.text.clone(),
                modified: old.text != old.original_text,
                gemini_start_sec: old.gemini_start_sec,
                visual_evidence_time_sec: old.visual_evidence_time_sec,
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
                duck_start_sec,
                duck_end_sec,
            });
            extra_offset += extended_pause_duration_sec;
        }

        let mut updated = previous;
        updated.updated_at_utc = now_utc();
        updated.source_duration_sec = source_duration;
        updated.output_duration_sec = output_duration;
        updated.tts_engine = tts_engine.to_string();
        updated.tts_voice = tts_voice.to_string();
        updated.descriptions = descriptions;

        save_project(&temporary_project, &updated)?;
        commit_project_pair(
            &temporary_media,
            &project.output_mp3_path,
            &temporary_project,
            project_file,
        )?;
        state.lock().unwrap().progress = 100;
        Ok(updated)
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temporary_media);
        let _ = fs::remove_file(&temporary_project);
    }
    let _ = fs::remove_dir_all(&work);
    result
}

fn run_project_voice_validation_with_progress(
    parent: &Dialog,
    project: AudioDescriptionProject,
    project_file: PathBuf,
    tts_engine: String,
    tts_voice: String,
    runtime: Arc<Runtime>,
) -> (
    Result<AudioDescriptionProject, String>,
    Option<ProjectVoiceFitError>,
) {
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
    root.add(&gauge, 0, SizerFlag::Expand | SizerFlag::All, 12);
    panel.set_sizer(root, true);

    let state = Arc::new(Mutex::new(ProjectVoiceValidationState::default()));
    let thread_state = Arc::clone(&state);
    thread::spawn(move || {
        let result = change_project_voice(
            &project,
            &project_file,
            &tts_engine,
            &tts_voice,
            &runtime,
            &thread_state,
        );
        thread_state.lock().unwrap().done = Some(result);
    });

    let result = Rc::new(RefCell::new(None::<(
        Result<AudioDescriptionProject, String>,
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

fn format_project_subtitle_timestamp(seconds: f64, millisecond_separator: char) -> String {
    let safe_seconds = if seconds.is_finite() {
        seconds.max(0.0)
    } else {
        0.0
    };
    let total_ms = (safe_seconds * 1000.0).round() as u64;
    let hours = total_ms / 3_600_000;
    let minutes = (total_ms / 60_000) % 60;
    let secs = (total_ms / 1000) % 60;
    let millis = total_ms % 1000;
    format!("{hours:02}:{minutes:02}:{secs:02}{millisecond_separator}{millis:03}")
}

fn normalized_project_subtitle_text(text: &str) -> String {
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .trim()
        .to_string()
}

fn render_project_srt(project: &AudioDescriptionProject) -> String {
    let mut output = String::new();
    let mut cue_number = 1usize;
    for description in &project.descriptions {
        let text = normalized_project_subtitle_text(&description.text);
        if text.is_empty() {
            continue;
        }
        let start = description.output_start_sec.max(0.0);
        let end = description.output_end_sec.max(start + 0.001);
        output.push_str(&format!(
            "{cue_number}\r\n{} --> {}\r\n{}\r\n\r\n",
            format_project_subtitle_timestamp(start, ','),
            format_project_subtitle_timestamp(end, ','),
            text.replace('\n', "\r\n")
        ));
        cue_number += 1;
    }
    output
}

fn render_project_vtt(project: &AudioDescriptionProject) -> String {
    let mut output = String::from("WEBVTT\r\n\r\n");
    for description in &project.descriptions {
        let text = normalized_project_subtitle_text(&description.text);
        if text.is_empty() {
            continue;
        }
        let start = description.output_start_sec.max(0.0);
        let end = description.output_end_sec.max(start + 0.001);
        output.push_str(&format!(
            "{} --> {}\r\n{}\r\n\r\n",
            format_project_subtitle_timestamp(start, '.'),
            format_project_subtitle_timestamp(end, '.'),
            text.replace('\n', "\r\n")
        ));
    }
    output
}

fn choose_project_subtitle_output(
    parent: &Dialog,
    project: &AudioDescriptionProject,
    extension: &str,
) -> Option<PathBuf> {
    let stem = project
        .output_mp3_path
        .file_stem()
        .and_then(|value| value.to_str())
        .or_else(|| project.source_path.file_stem().and_then(|value| value.to_str()))
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("audiodescrizione");
    let format_name = extension.to_ascii_uppercase();
    let wildcard = if extension.eq_ignore_ascii_case("vtt") {
        "WebVTT|*.vtt"
    } else {
        "SubRip|*.srt"
    };
    let dialog = FileDialog::builder(parent)
        .with_message(&trf(
            "audio_description.project.export_subtitle_title",
            &[("format", format_name)],
        ))
        .with_default_file(&format!("{stem}.{extension}"))
        .with_wildcard(wildcard)
        .with_style(FileDialogStyle::Save | FileDialogStyle::OverwritePrompt)
        .build();
    if dialog.show_modal() != ID_OK {
        return None;
    }
    let mut path = dialog.get_path().map(PathBuf::from)?;
    if !path
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case(extension))
    {
        path.set_extension(extension);
    }
    Some(path)
}

fn export_project_subtitles(
    parent: &Dialog,
    project: &AudioDescriptionProject,
    extension: &str,
) -> Result<Option<PathBuf>, String> {
    let Some(path) = choose_project_subtitle_output(parent, project, extension) else {
        return Ok(None);
    };
    let contents = if extension.eq_ignore_ascii_case("vtt") {
        render_project_vtt(project)
    } else {
        render_project_srt(project)
    };
    fs::write(&path, contents.as_bytes()).map_err(|error| error.to_string())?;
    Ok(Some(path))
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


#[derive(Clone, Debug)]
struct ProjectSegmentReanalysis {
    focus_index: usize,
    project: AudioDescriptionProject,
    segment_description_ids: Vec<usize>,
}

fn audio_description_job_from_project(
    project: &AudioDescriptionProject,
    input_path: PathBuf,
    output_path: PathBuf,
    settings: &Settings,
) -> CreateJob {
    let use_sonarpad_ai = settings.audio_description_use_sonarpad_ai;
    CreateJob {
        input_path,
        output_path,
        language_code: project.language_code.clone(),
        verbosity: Verbosity::from_settings(&project.verbosity),
        allow_extended_pauses: project.allow_extended_pauses,
        recognize_characters: project.recognize_characters,
        recognize_screen_text: project.recognize_screen_text,
        save_project: true,
        create_video_output: false,
        delete_input_after_success: false,
        keep_character_catalog: false,
        catalog: None,
        tts_engine: project.tts_engine.clone(),
        tts_voice: project.tts_voice.clone(),
        rate: project.tts_rate,
        pitch: project.tts_pitch,
        volume: project.tts_volume,
        audio_stream_index: None,
        gemini_api_key: if use_sonarpad_ai {
            String::new()
        } else {
            settings.audio_description_gemini_api_key.clone()
        },
        sonarpad_ai_service_url: if use_sonarpad_ai {
            SONARPAD_AI_SERVICE_URL.to_string()
        } else {
            String::new()
        },
        sonarpad_ai_access_code: if use_sonarpad_ai {
            settings.sonarpad_ai_access_code.clone()
        } else {
            String::new()
        },
        sonarpad_ai_device_id: if use_sonarpad_ai {
            settings.sonarpad_ai_device_id.clone()
        } else {
            String::new()
        },
        gemini_model: if use_sonarpad_ai {
            SONARPAD_AI_FORCED_GEMINI_MODEL.to_string()
        } else if settings.audio_description_gemini_model.trim().is_empty() {
            project.gemini_model.clone()
        } else {
            settings.audio_description_gemini_model.clone()
        },
        fixed_reanalysis_slots: Vec::new(),
        resume_checkpoint_path: None,
    }
}

fn reanalyze_project_segment(
    project: &AudioDescriptionProject,
    index: usize,
    settings: &Settings,
    rt: &Runtime,
    cancel: Arc<AtomicBool>,
    state: Arc<Mutex<ProgressState>>,
) -> Result<ProjectSegmentReanalysis, String> {
    let selected = project
        .descriptions
        .get(index)
        .ok_or_else(|| tr("audio_description.project.no_selection"))?;
    if !project.source_path.is_file() {
        return Err(format!(
            "File sorgente del progetto non disponibile: {}",
            project.source_path.display()
        ));
    }
    if cancel.load(Ordering::Relaxed) {
        return Err("cancelled".to_string());
    }
    if settings.audio_description_use_sonarpad_ai {
        if settings.sonarpad_ai_access_code.trim().is_empty() {
            return Err(tr("audio_description.error.sonarpad_code"));
        }
        if settings.sonarpad_ai_device_id.trim().is_empty() {
            return Err("Identificatore dispositivo Sonarpad AI non disponibile.".to_string());
        }
    } else if settings.audio_description_gemini_api_key.trim().is_empty() {
        return Err(tr("audio_description.error.api_key"));
    }

    let cache = cache_dir("reanalyze_segment")?;
    let result = (|| {
        {
            let mut progress = state.lock().unwrap();
            progress.progress = 1;
            progress.status = tr("audio_description.project.status.reanalyzing");
        }
        let source_duration = if project.source_duration_sec.is_finite()
            && project.source_duration_sec > 0.0
        {
            project.source_duration_sec
        } else {
            let probe_wav = cache.join("source-probe.wav");
            decode_source_audio(
                &project.source_path,
                &probe_wav,
                project.audio_stream_index,
                &cancel,
            )?
            .duration_sec
        };
        let chunk_dir = cache.join("mini-film-source");
        fs::create_dir_all(&chunk_dir).map_err(|error| error.to_string())?;
        let chunks = prepare_chunks(
            &project.source_path,
            source_duration,
            &chunk_dir,
            project.audio_stream_index,
            &cancel,
        )?;
        if cancel.load(Ordering::Relaxed) {
            return Err("cancelled".to_string());
        }
        let target_sec = if selected.gemini_start_sec.is_finite() {
            selected.gemini_start_sec.max(0.0)
        } else {
            selected.source_start_sec.max(0.0)
        };
        let chosen = chunks
            .iter()
            .find(|chunk| target_sec >= chunk.start_sec && target_sec < chunk.end_sec)
            .or_else(|| {
                chunks.iter().min_by(|left, right| {
                    let left_distance = if target_sec < left.start_sec {
                        left.start_sec - target_sec
                    } else if target_sec > left.end_sec {
                        target_sec - left.end_sec
                    } else {
                        0.0
                    };
                    let right_distance = if target_sec < right.start_sec {
                        right.start_sec - target_sec
                    } else if target_sec > right.end_sec {
                        target_sec - right.end_sec
                    } else {
                        0.0
                    };
                    left_distance.total_cmp(&right_distance)
                })
            })
            .cloned()
            .ok_or_else(|| "Nessun segmento di analisi disponibile.".to_string())?;
        let segment_start_sec = chosen.start_sec;
        let segment_end_sec = chosen.end_sec;
        let mini_film_path = PathBuf::from(&chosen.path);
        if !mini_film_path.is_file() {
            return Err("Creazione del mini-film per la rianalisi fallita.".to_string());
        }
        // The reanalysis mini-film must preserve the same soundtrack semantics as
        // normal creation. Never analyse a video-only fallback when the source has
        // audio, otherwise the mini-film and speech/silence timeline can drift.
        let source_has_audio = probe_media(&project.source_path)?.has_audio;
        let mini_has_audio = probe_media(&mini_film_path)?.has_audio;
        if source_has_audio && !mini_has_audio {
            return Err(
                "Sonarpad non ha potuto creare un segmento di rianalisi autonomo con la sua traccia audio; il segmento non è stato modificato."
                    .to_string(),
            );
        }

        // Windows parity: analyze the self-contained mini-film with the exact same
        // normal creation pipeline. The mini-film timeline is only an association
        // aid; it is never allowed to overwrite the saved project timeline.
        append_podcast_log(&format!(
            "audio_description.project.reanalyze_windows_parity source_range={:.3}-{:.3}",
            segment_start_sec, segment_end_sec
        ));
        let mini_output = cache.join("reanalyzed-mini-film-audiodescritto.mp3");
        let mini_job = audio_description_job_from_project(
            project,
            mini_film_path,
            mini_output,
            settings,
        );
        let mini_outcome = create_audio_description(
            &mini_job,
            rt,
            cancel.clone(),
            Arc::clone(&state),
        )?;
        if cancel.load(Ordering::Relaxed) {
            return Err("cancelled".to_string());
        }
        let mini_project_path = mini_outcome
            .project_path
            .ok_or_else(|| "La rianalisi non ha prodotto un progetto temporaneo.".to_string())?;
        let mini_project = load_project(&mini_project_path)?;
        append_podcast_log(&format!(
            "audio_description.project.reanalyze_mini_project descriptions={} duration={:.3}",
            mini_project.descriptions.len(), mini_project.source_duration_sec
        ));
        if mini_project.descriptions.is_empty() {
            return Err("La rianalisi non ha prodotto descrizioni utilizzabili.".to_string());
        }

        let segment_span_sec = (segment_end_sec - segment_start_sec).max(0.0);
        let mini_duration_sec = mini_project.source_duration_sec;
        if !segment_span_sec.is_finite()
            || segment_span_sec <= 0.0
            || !mini_duration_sec.is_finite()
            || mini_duration_sec <= 0.0
        {
            return Err("Cronologia del mini-film non valida; segmento non modificato.".to_string());
        }
        // CRITICAL INVARIANT: the mini-film timestamps below are used only to
        // associate fresh text to an existing saved description. source_start_sec,
        // gemini_start_sec, visual evidence, slot metadata and pause information in
        // the original project remain authoritative and are never copied from Gemini.
        let mini_to_source_scale = segment_span_sec / mini_duration_sec;
        if !mini_to_source_scale.is_finite() || !(0.98..=1.02).contains(&mini_to_source_scale) {
            return Err(format!(
                "Deriva temporale del mini-film troppo grande ({mini_to_source_scale:.6}); segmento non modificato."
            ));
        }
        let map_mini_time = |time_sec: f64| {
            let local = time_sec.max(0.0).min(mini_duration_sec);
            (segment_start_sec + local * mini_to_source_scale).min(segment_end_sec)
        };

        let mut segment_indices = project
            .descriptions
            .iter()
            .enumerate()
            .filter(|(_, description)| {
                let time = description.source_start_sec.max(0.0);
                time + 0.001 >= segment_start_sec && time < segment_end_sec + 0.001
            })
            .map(|(project_index, _)| project_index)
            .collect::<Vec<_>>();
        segment_indices.sort_by(|left, right| {
            project.descriptions[*left]
                .source_start_sec
                .total_cmp(&project.descriptions[*right].source_start_sec)
        });
        if segment_indices.is_empty() {
            return Err("Il segmento selezionato non contiene descrizioni salvate.".to_string());
        }
        let mut mini_indices = (0..mini_project.descriptions.len()).collect::<Vec<_>>();
        mini_indices.sort_by(|left, right| {
            mini_project.descriptions[*left]
                .source_start_sec
                .total_cmp(&mini_project.descriptions[*right].source_start_sec)
        });

        let old_count = segment_indices.len();
        let new_count = mini_indices.len();
        let skip_penalty = 4.0_f64;
        let mut cost = vec![vec![f64::INFINITY; new_count + 1]; old_count + 1];
        let mut step = vec![vec![0_u8; new_count + 1]; old_count + 1];
        cost[0][0] = 0.0;
        for old_pos in 0..=old_count {
            for new_pos in 0..=new_count {
                let current_cost = cost[old_pos][new_pos];
                if !current_cost.is_finite() {
                    continue;
                }
                if old_pos < old_count {
                    let candidate = current_cost + skip_penalty;
                    if candidate < cost[old_pos + 1][new_pos] {
                        cost[old_pos + 1][new_pos] = candidate;
                        step[old_pos + 1][new_pos] = 1;
                    }
                }
                if new_pos < new_count {
                    let candidate = current_cost + skip_penalty;
                    if candidate < cost[old_pos][new_pos + 1] {
                        cost[old_pos][new_pos + 1] = candidate;
                        step[old_pos][new_pos + 1] = 2;
                    }
                }
                if old_pos < old_count && new_pos < new_count {
                    let old_time = project.descriptions[segment_indices[old_pos]].source_start_sec;
                    let new_time = map_mini_time(
                        mini_project.descriptions[mini_indices[new_pos]].source_start_sec,
                    );
                    let distance = (old_time - new_time).abs();
                    if distance <= 12.0 {
                        let candidate = current_cost + distance;
                        if candidate < cost[old_pos + 1][new_pos + 1] {
                            cost[old_pos + 1][new_pos + 1] = candidate;
                            step[old_pos + 1][new_pos + 1] = 3;
                        }
                    }
                }
            }
        }
        let mut associations = Vec::new();
        let (mut old_pos, mut new_pos) = (old_count, new_count);
        while old_pos > 0 || new_pos > 0 {
            match step[old_pos][new_pos] {
                3 => {
                    associations.push((old_pos - 1, new_pos - 1));
                    old_pos -= 1;
                    new_pos -= 1;
                }
                1 => old_pos -= 1,
                2 => new_pos -= 1,
                _ if old_pos > 0 => old_pos -= 1,
                _ if new_pos > 0 => new_pos -= 1,
                _ => break,
            }
        }
        associations.reverse();

        let mut candidate = project.clone();
        let mut accepted = 0_usize;
        let mut changed = 0_usize;
        for (saved_pos, fresh_pos) in associations {
            let project_index = segment_indices[saved_pos];
            let fresh = &mini_project.descriptions[mini_indices[fresh_pos]];
            let fresh_text = fresh.text.trim();
            if fresh_text.is_empty() {
                continue;
            }
            let available = project_edit_available_duration(project, project_index)?;
            if let Some(available_sec) = available
                && fresh.tts_duration_sec > available_sec + 0.010
            {
                append_podcast_log(&format!(
                    "audio_description.project.reanalyze_keep_old id={} reason=fresh_too_long fresh={:.3} available={:.3}",
                    project.descriptions[project_index].id,
                    fresh.tts_duration_sec,
                    available_sec
                ));
                continue;
            }
            let saved = &mut candidate.descriptions[project_index];
            if saved.text != fresh_text {
                changed += 1;
            }
            saved.text = fresh_text.to_string();
            saved.rendered_text = if fresh.rendered_text.trim().is_empty() {
                fresh_text.to_string()
            } else {
                fresh.rendered_text.clone()
            };
            saved.tts_duration_sec = fresh.tts_duration_sec;
            saved.modified = saved.text != saved.original_text;
            // Deliberately preserve every original timing/pause field. Only text,
            // rendered_text and measured TTS duration come from the mini analysis.
            accepted += 1;
        }
        if accepted == 0 {
            return Err("Nessuna nuova descrizione può essere associata in sicurezza agli slot salvati; segmento non modificato.".to_string());
        }
        append_podcast_log(&format!(
            "audio_description.project.reanalyze_candidate mode=windows_parity accepted={} changed={} saved_slots={} fresh_descriptions={}",
            accepted, changed, old_count, new_count
        ));

        let mut output_offset_sec = 0.0_f64;
        for description in &mut candidate.descriptions {
            let duration = description.tts_duration_sec.max(0.0);
            description.output_start_sec = description.source_start_sec + output_offset_sec;
            description.output_end_sec = description.output_start_sec + duration;
            if description.extended_pause {
                description.extended_pause_duration_sec = duration;
                description.duck_start_sec = None;
                description.duck_end_sec = None;
                output_offset_sec += duration;
            } else {
                description.extended_pause_duration_sec = 0.0;
                let (duck_start_sec, duck_end_sec) = project_duck_bounds(
                    description.output_start_sec,
                    description.output_end_sec,
                    false,
                );
                description.duck_start_sec = duck_start_sec;
                description.duck_end_sec = duck_end_sec;
            }
        }
        candidate.output_duration_sec = candidate.source_duration_sec + output_offset_sec;
        candidate.gemini_model = mini_project.gemini_model.clone();
        candidate.updated_at_utc = now_utc();
        let segment_description_ids = segment_indices
            .iter()
            .map(|project_index| project.descriptions[*project_index].id)
            .collect::<Vec<_>>();
        let selected_id = selected.id;
        let focus_index = candidate
            .descriptions
            .iter()
            .position(|description| description.id == selected_id)
            .unwrap_or_else(|| index.min(candidate.descriptions.len().saturating_sub(1)));
        {
            let mut progress = state.lock().unwrap();
            progress.progress = 100;
            progress.status = tr("audio_description.project.status.reanalyzed_ready");
        }
        Ok(ProjectSegmentReanalysis {
            focus_index,
            project: candidate,
            segment_description_ids,
        })
    })();
    let _ = fs::remove_dir_all(&cache);
    result
}

fn run_project_reanalysis_with_progress(
    parent: &Dialog,
    project: AudioDescriptionProject,
    index: usize,
    settings: Settings,
    runtime: Arc<Runtime>,
) -> Result<ProjectSegmentReanalysis, String> {
    let progress_dialog = Dialog::builder(
        parent,
        &tr("audio_description.project.reanalyze_button"),
    )
    .with_style(
        DialogStyle::Caption
            | DialogStyle::SystemMenu
            | DialogStyle::CloseBox
            | DialogStyle::StayOnTop,
    )
    .with_size(560, 180)
    .build();
    let panel = Panel::builder(&progress_dialog).build();
    let root = BoxSizer::builder(Orientation::Vertical).build();
    let label = StaticText::builder(&panel)
        .with_label(&tr("audio_description.project.status.reanalyzing"))
        .build();
    root.add(&label, 0, SizerFlag::Expand | SizerFlag::All, 12);
    let gauge = Gauge::builder(&panel).with_range(100).build();
    root.add(&gauge, 0, SizerFlag::Expand | SizerFlag::All, 12);
    let cancel_button = Button::builder(&panel)
        .with_label(&tr("audio_description.cancel"))
        .build();
    root.add(&cancel_button, 0, SizerFlag::All, 10);
    panel.set_sizer(root, true);

    let state = Arc::new(Mutex::new(ProgressState {
        progress: 0,
        status: tr("audio_description.project.status.reanalyzing"),
        done: None,
        quota: None,
        overload: None,
    }));
    let cancel = Arc::new(AtomicBool::new(false));
    // Use a one-shot channel for the terminal result.  The previous implementation
    // polled an Arc<Mutex<Option<...>>> from the wx timer; on macOS the worker could
    // finish successfully while the modal progress dialog never observed the value,
    // leaving the UI stuck until the user closed it manually.  A channel gives the
    // worker exactly one hand-off and lets the UI consume it without sharing a lock.
    let (result_sender, result_receiver) = mpsc::channel::<Result<ProjectSegmentReanalysis, String>>();
    let quota_api_key = settings.audio_description_gemini_api_key.clone();
    let thread_settings = settings.clone();
    let thread_state = Arc::clone(&state);
    let thread_cancel = Arc::clone(&cancel);
    thread::spawn(move || {
        let outcome = reanalyze_project_segment(
            &project,
            index,
            &thread_settings,
            &runtime,
            thread_cancel,
            thread_state,
        );
        match &outcome {
            Ok(reanalysis) => append_podcast_log(&format!(
                "audio_description.project.reanalyze_worker_done ok=true affected={} focus={}",
                reanalysis.segment_description_ids.len(),
                reanalysis.focus_index
            )),
            Err(error) => append_podcast_log(&format!(
                "audio_description.project.reanalyze_worker_done ok=false error={}",
                error.replace('\n', " ")
            )),
        }
        if result_sender.send(outcome).is_err() {
            append_podcast_log("audio_description.project.reanalyze_result_send_failed");
        }
    });

    let cancelling = Rc::new(Cell::new(false));
    // Keep the modal-close path distinct from user cancellation.  wxDragon can
    // deliver a close event while end_modal()/destroy() is completing; without
    // this flag the reanalysis dialog used to interpret its own successful
    // programmatic close as Cancel and could prevent the caller from ever
    // receiving/applying the already-computed result.
    let finished = Rc::new(Cell::new(false));
    let cancel_click = Arc::clone(&cancel);
    let cancelling_click = Rc::clone(&cancelling);
    cancel_button.on_click(move |_| {
        if !cancelling_click.replace(true) {
            cancel_click.store(true, Ordering::SeqCst);
            cancel_button.enable(false);
            label.set_label(&tr("audio_description.status.canceling"));
        }
    });
    let cancel_close = Arc::clone(&cancel);
    let cancelling_close = Rc::clone(&cancelling);
    let finished_close = Rc::clone(&finished);
    progress_dialog.on_close(move |event| {
        if finished_close.get() {
            // Programmatic close after a terminal result: allow wxDragon to
            // finish the modal lifecycle.  This mirrors the proven export
            // progress dialog and must not be converted into cancellation.
            event.skip(true);
            return;
        }
        if !cancelling_close.replace(true) {
            cancel_close.store(true, Ordering::SeqCst);
            cancel_button.enable(false);
            label.set_label(&tr("audio_description.status.canceling"));
        }
        event.skip(false);
    });

    let ui_result = Rc::new(RefCell::new(None::<Result<ProjectSegmentReanalysis, String>>));
    let timer = Rc::new(Timer::new(&progress_dialog));
    let timer_tick = Rc::clone(&timer);
    let state_tick = Arc::clone(&state);
    let ui_result_tick = Rc::clone(&ui_result);
    let finished_tick = Rc::clone(&finished);
    let dialog_tick = progress_dialog;
    timer_tick.on_tick(move |_| {
        let overload = { state_tick.lock().unwrap().overload.take() };
        if let Some(request) = overload {
            let decision = overload_dialog(&dialog_tick, &request.model, &request.error);
            let _ = request.sender.send(decision);
        }
        let quota = { state_tick.lock().unwrap().quota.take() };
        if let Some(request) = quota {
            let decision = quota_dialog(
                &dialog_tick,
                &request.model,
                &request.error,
                &quota_api_key,
            );
            let _ = request.sender.send(decision);
        }
        let snapshot = state_tick.lock().unwrap().clone();
        if !cancelling.get() {
            label.set_label(&snapshot.status);
        }
        gauge.set_value(snapshot.progress.clamp(0, 99));
        match result_receiver.try_recv() {
            Ok(done) => {
                gauge.set_value(100);
                append_podcast_log("audio_description.project.reanalyze_result_received");
                *ui_result_tick.borrow_mut() = Some(done);
                // Set this before end_modal: the modal close event may fire
                // synchronously on macOS.
                finished_tick.set(true);
                dialog_tick.end_modal(ID_OK);
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                append_podcast_log("audio_description.project.reanalyze_result_channel_disconnected");
                *ui_result_tick.borrow_mut() = Some(Err(
                    "La rianalisi si è conclusa senza restituire il risultato alla finestra del progetto."
                        .to_string(),
                ));
                finished_tick.set(true);
                dialog_tick.end_modal(ID_OK);
            }
        }
    });
    timer.start(100, false);
    progress_dialog.show_modal();
    append_podcast_log("audio_description.project.reanalyze_modal_returned");
    timer.stop();
    // Own the terminal result BEFORE destroying the dialog.  On macOS the timer
    // callback and its captured UI state belong to the dialog event lifecycle;
    // reading the shared RefCell after destroy() could leave the hand-off trapped
    // behind already-destroyed wx objects.  From this point onward the result is a
    // plain Rust value, completely independent from the progress UI.
    let terminal_result = ui_result
        .borrow_mut()
        .take()
        .unwrap_or_else(|| Err("cancelled".to_string()));
    append_podcast_log("audio_description.project.reanalyze_result_extracted");
    // The modal may also return because the user cancelled.  Mark the lifecycle
    // finished before destroy() so the close handler can never veto destruction
    // of an already-returned modal dialog.
    finished.set(true);
    progress_dialog.destroy();
    append_podcast_log("audio_description.project.reanalyze_dialog_destroyed");
    terminal_result
}

fn rebuild_project(
    project: &mut AudioDescriptionProject,
    project_file: &Path,
    rt: &Runtime,
    cancel: Arc<AtomicBool>,
    state: Arc<Mutex<ProgressState>>,
    fixed_timeline: bool,
) -> Result<JobOutcome, String> {
    let work = cache_dir(if fixed_timeline {
        "project_reanalyzed_fixed"
    } else {
        "project"
    })?;
    let temporary_media = temporary_sibling_path(
        &project.output_mp3_path,
        if fixed_timeline { "reanalyzed_fixed" } else { "rebuilt" },
    );
    let temporary_project = temporary_sibling_path(
        project_file,
        if fixed_timeline { "reanalyzed_fixed" } else { "rebuilt" },
    );
    let result = (|| {
        let source = work.join("source.wav");
        let duration = decode_source_audio(
            &project.source_path,
            &source,
            project.audio_stream_index,
            &cancel,
        )?
        .duration_sec;
        let synthesis_tasks = project
            .descriptions
            .iter()
            .enumerate()
            .map(|(index, description)| AudioDescriptionSynthesisTask {
                original_index: index,
                synthesis_index: index,
                text: description.text.clone(),
                desired_start_sec: if fixed_timeline {
                    description.source_start_sec
                } else {
                    description.gemini_start_sec
                },
                visual_start_sec: description.gemini_start_sec,
                visual_evidence_time_sec: description.visual_evidence_time_sec,
                mandatory: description.mandatory,
                slot_id: description.slot_id.clone(),
                slot_start_sec: description.slot_start_sec,
                slot_end_sec: description.slot_end_sec,
            })
            .collect::<Vec<_>>();
        let synthesized = synthesize_description_tasks_parallel(
            &synthesis_tasks,
            TtsParameters {
                engine: &project.tts_engine,
                voice: &project.tts_voice,
                rate: project.tts_rate,
                pitch: project.tts_pitch,
                volume: project.tts_volume,
            },
            rt,
            &work,
            cancel.clone(),
            |completed, total| {
                state.lock().unwrap().progress =
                    10 + ((completed as i32 * 70) / total.max(1) as i32);
            },
        )?;
        let protected = project
            .protected_intervals
            .iter()
            .map(|x| BridgeInterval {
                start_sec: x.start_sec,
                end_sec: x.end_sec,
            })
            .collect::<Vec<_>>();

        let (scheduled, dropped) = if fixed_timeline {
            if project.output_is_video
                && project.descriptions.iter().any(|description| description.extended_pause)
            {
                return Err(
                    "Un progetto video non può contenere pause estese perché desincronizzerebbero il video."
                        .to_string(),
                );
            }
            let mut fixed = Vec::with_capacity(project.descriptions.len());
            for (index, description) in project.descriptions.iter().enumerate() {
                if cancel.load(Ordering::Relaxed) {
                    return Err("cancelled".to_string());
                }
                let rendered = synthesized
                    .iter()
                    .find(|candidate| candidate.original_index == index)
                    .ok_or_else(|| {
                        "Manca l'audio sintetizzato per una descrizione rianalizzata.".to_string()
                    })?;
                if let Some(available_sec) = project_edit_available_duration(project, index)?
                    && rendered.duration_sec > available_sec + 0.010
                {
                    return Err(trf(
                        "audio_description.project.error_too_long",
                        &[
                            ("actual", format!("{:.3}", rendered.duration_sec)),
                            ("available", format!("{available_sec:.3}")),
                        ],
                    ));
                }
                fixed.push(ScheduledDescription {
                    original_index: index,
                    text: description.text.clone(),
                    desired_start_sec: description.gemini_start_sec,
                    visual_evidence_time_sec: description.visual_evidence_time_sec,
                    mandatory: description.mandatory,
                    slot_id: description.slot_id.clone(),
                    slot_start_sec: description.slot_start_sec,
                    slot_end_sec: description.slot_end_sec,
                    start_sec: description.source_start_sec,
                    pcm: Arc::clone(&rendered.pcm),
                    duration_sec: rendered.duration_sec,
                    extended_pause: description.extended_pause,
                });
            }
            (fixed, Vec::<DroppedDescription>::new())
        } else {
            schedule_descriptions(
                &synthesized,
                &protected,
                duration,
                project.allow_extended_pauses && !project.output_is_video,
            )
        };
        if scheduled.is_empty() {
            return Err("Nessuna descrizione può essere inserita in sicurezza.".to_string());
        }
        state.lock().unwrap().progress = 85;
        let mix = work.join("mix.wav");
        let output_duration = render_mix(&source, &mix, &scheduled, &cancel)?;
        state.lock().unwrap().progress = 92;
        let temporary_audio = work.join("project-rebuilt.mp3");
        encode_mp3(&mix, &temporary_audio, &cancel)?;
        if project.output_is_video {
            mux_audio_description_video(
                &project.source_path,
                &temporary_audio,
                &temporary_media,
                &cancel,
            )?;
        } else {
            fs::copy(&temporary_audio, &temporary_media)
                .map_err(|error| format!("Salvataggio MP3 fallito: {error}"))?;
        }
        let metadata = fs::metadata(&temporary_media)
            .map_err(|error| format!("Verifica output audiodescritto fallita: {error}"))?;
        if metadata.len() == 0 {
            return Err("L'output audiodescritto ricostruito è vuoto.".to_string());
        }
        if cancel.load(Ordering::Relaxed) {
            return Err("cancelled".to_string());
        }

        let previous = project.clone();
        let mut extra_offset = 0.0;
        let mut descriptions = Vec::with_capacity(scheduled.len());
        for (new_id, scheduled_description) in scheduled.iter().enumerate() {
            let old = previous
                .descriptions
                .get(scheduled_description.original_index)
                .ok_or_else(|| "Indice descrizione progetto non valido.".to_string())?;
            let source_start = if fixed_timeline {
                old.source_start_sec
            } else {
                scheduled_description.start_sec
            };
            let output_start = source_start + extra_offset;
            let extended_pause_duration_sec = if scheduled_description.extended_pause {
                scheduled_description.duration_sec
            } else {
                0.0
            };
            let output_end = output_start + scheduled_description.duration_sec;
            let (duck_start_sec, duck_end_sec) =
                project_duck_bounds(output_start, output_end, old.extended_pause);
            descriptions.push(ProjectDescription {
                id: if fixed_timeline { old.id } else { new_id },
                text: old.text.clone(),
                original_text: old.original_text.clone(),
                rendered_text: old.text.clone(),
                modified: old.text != old.original_text,
                gemini_start_sec: old.gemini_start_sec,
                visual_evidence_time_sec: old.visual_evidence_time_sec,
                mandatory: old.mandatory,
                slot_id: old.slot_id.clone(),
                slot_start_sec: old.slot_start_sec,
                slot_end_sec: old.slot_end_sec,
                source_start_sec: source_start,
                output_start_sec: output_start,
                output_end_sec: output_end,
                tts_duration_sec: scheduled_description.duration_sec,
                extended_pause: old.extended_pause,
                extended_pause_duration_sec,
                duck_start_sec,
                duck_end_sec,
            });
            extra_offset += extended_pause_duration_sec;
        }
        let excluded_descriptions = if fixed_timeline {
            previous.excluded_descriptions.clone()
        } else {
            dropped
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
                .collect::<Vec<_>>()
        };

        project.source_duration_sec = duration;
        project.output_duration_sec = output_duration;
        project.updated_at_utc = now_utc();
        project.descriptions = descriptions;
        project.excluded_descriptions = excluded_descriptions;
        save_project(&temporary_project, project)?;
        if cancel.load(Ordering::Relaxed) {
            return Err("cancelled".to_string());
        }
        commit_project_pair(
            &temporary_media,
            &project.output_mp3_path,
            &temporary_project,
            project_file,
        )?;
        state.lock().unwrap().progress = 100;

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
    if result.is_err() {
        let _ = fs::remove_file(&temporary_media);
        let _ = fs::remove_file(&temporary_project);
    }
    let _ = fs::remove_dir_all(&work);
    result
}

fn run_project_export_with_progress(
    parent: &Dialog,
    mut project: AudioDescriptionProject,
    project_file: PathBuf,
    runtime: Arc<Runtime>,
    fixed_timeline: bool,
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
            fixed_timeline,
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

fn project_description_search_order(
    descriptions: &[ProjectDescription],
    query: &str,
) -> Vec<usize> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return (0..descriptions.len()).collect();
    }

    let mut matches = Vec::new();
    let mut remaining = Vec::new();
    for (index, description) in descriptions.iter().enumerate() {
        if description.text.to_lowercase().contains(&needle) {
            matches.push(index);
        } else {
            remaining.push(index);
        }
    }
    matches.extend(remaining);
    matches
}

fn selected_project_description_index(
    choice: &Choice,
    display_order: &RefCell<Vec<usize>>,
) -> Option<usize> {
    let display_index = choice.get_selection()? as usize;
    display_order.borrow().get(display_index).copied()
}

fn refresh_project_description_choice(
    choice: &Choice,
    descriptions: &[ProjectDescription],
    display_order: &RefCell<Vec<usize>>,
    query: &str,
    preferred_real_index: Option<usize>,
    reanalyzed_description_ids: Option<&HashSet<usize>>,
) -> Option<usize> {
    let order = project_description_search_order(descriptions, query);
    choice.clear();
    for index in &order {
        if let Some(description) = descriptions.get(*index) {
            let marker = reanalyzed_description_ids
                .is_some_and(|ids| ids.contains(&description.id))
                .then(|| tr("audio_description.project.reanalyzed_marker"));
            let label = if let Some(marker) = marker {
                format!(
                    "{} - {} - {}",
                    format_mmss(description.source_start_sec),
                    marker,
                    description.text
                )
            } else {
                format!(
                    "{} - {}",
                    format_mmss(description.source_start_sec),
                    description.text
                )
            };
            choice.append(&label);
        }
    }

    if order.is_empty() {
        *display_order.borrow_mut() = order;
        return None;
    }

    let display_index = preferred_real_index
        .and_then(|real_index| order.iter().position(|index| *index == real_index))
        .unwrap_or(0);
    let selected_real_index = order[display_index];
    *display_order.borrow_mut() = order;
    choice.set_selection(display_index as u32);
    Some(selected_real_index)
}

pub fn open_project_editor(
    parent: &Frame,
    settings: &Arc<Mutex<Settings>>,
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
    let pending_reanalysis = Rc::new(Cell::new(false));
    // Text edits can be staged across several descriptions before they are
    // validated and committed together, matching the Windows saved-project editor.
    let pending_text_edits = Rc::new(RefCell::new(HashMap::<usize, String>::new()));
    let last_selected_description = Rc::new(Cell::new(
        (!project.borrow().descriptions.is_empty()).then_some(0usize),
    ));
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
    let description_display_order = Rc::new(RefCell::new(Vec::<usize>::new()));
    let reanalyzed_description_ids = Rc::new(RefCell::new(HashSet::<usize>::new()));
    refresh_project_description_choice(
        &choice,
        &project.borrow().descriptions,
        &description_display_order,
        "",
        None,
        Some(&*reanalyzed_description_ids.borrow()),
    );
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

    let apply = Button::builder(&panel)
        .with_label(&tr("audio_description.project.apply"))
        .build();
    let reanalyze = Button::builder(&panel)
        .with_label(&tr("audio_description.project.reanalyze_button"))
        .build();
    let apply_reanalyzed = Button::builder(&panel)
        .with_label(&tr("audio_description.project.apply_reanalyzed"))
        .build();
    apply_reanalyzed.enable(false);
    let apply_row = BoxSizer::builder(Orientation::Horizontal).build();
    apply_row.add(&apply, 0, SizerFlag::All, 4);
    apply_row.add(&reanalyze, 0, SizerFlag::All, 4);
    apply_row.add(&apply_reanalyzed, 0, SizerFlag::All, 4);
    root.add_sizer(&apply_row, 0, SizerFlag::Expand, 0);

    let search_row = BoxSizer::builder(Orientation::Horizontal).build();
    search_row.add(
        &StaticText::builder(&panel)
            .with_label(&tr("audio_description.project.search"))
            .build(),
        0,
        SizerFlag::AlignCenterVertical | SizerFlag::All,
        5,
    );
    let search = TextCtrl::builder(&panel)
        .with_style(TextCtrlStyle::ProcessEnter)
        .build();
    let search_button = Button::builder(&panel)
        .with_label(&tr("audio_description.project.search_button"))
        .build();
    search_row.add(&search, 1, SizerFlag::Expand | SizerFlag::All, 5);
    search_row.add(&search_button, 0, SizerFlag::All, 5);
    root.add_sizer(&search_row, 0, SizerFlag::Expand, 0);

    let engine = Choice::builder(&panel).build();
    engine.append(&tr("audio_description.engine.edge"));
    engine.append(&tr("audio_description.engine.system"));
    let initial_project_engine = if crate::is_system_voice_engine(&project.borrow().tts_engine) {
        1
    } else {
        0
    };
    engine.set_selection(initial_project_engine);
    let engine_row = BoxSizer::builder(Orientation::Horizontal).build();
    engine_row.add(
        &StaticText::builder(&panel)
            .with_label(&tr("audio_description.engine"))
            .build(),
        0,
        SizerFlag::AlignCenterVertical | SizerFlag::All,
        5,
    );
    engine_row.add(&engine, 1, SizerFlag::Expand | SizerFlag::All, 5);
    root.add_sizer(&engine_row, 0, SizerFlag::Expand, 0);

    let voice = Choice::builder(&panel).build();
    let voices_edge = voices_data.lock().unwrap().clone();
    let voices_system = crate::load_system_voices();
    let project_language = project.borrow().language_code.clone();
    let project_voices = Rc::new(RefCell::new(Vec::<VoiceInfo>::new()));
    let fill_project_voices: Rc<dyn Fn(u32, Option<String>)> = {
        let active = Rc::clone(&project_voices);
        let voice_control = voice;
        let voices_edge = voices_edge.clone();
        let voices_system = voices_system.clone();
        let language_code = project_language.clone();
        Rc::new(move |engine_index, preferred_voice| {
            voice_control.clear();
            let source = if engine_index == 1 {
                &voices_system
            } else {
                &voices_edge
            };
            let list = source
                .iter()
                .filter(|candidate| voice_matches_language(candidate, &language_code))
                .cloned()
                .collect::<Vec<_>>();
            for candidate in &list {
                voice_control.append(&candidate.friendly_name);
            }
            let selection = preferred_voice
                .as_deref()
                .and_then(|preferred| {
                    list.iter()
                        .position(|candidate| candidate.short_name == preferred)
                })
                .unwrap_or(0);
            if !list.is_empty() {
                voice_control.set_selection(selection as u32);
            }
            *active.borrow_mut() = list;
        })
    };
    fill_project_voices(
        initial_project_engine,
        Some(project.borrow().tts_voice.clone()),
    );

    let change_voice = Button::builder(&panel)
        .with_label(&tr("audio_description.project.change_voice"))
        .build();
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
    voice_row.add(&change_voice, 0, SizerFlag::All, 5);
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
    let play = Button::builder(&panel)
        .with_label(&tr("audio_description.project.play_description"))
        .build();
    let delete = Button::builder(&panel)
        .with_label(&tr("audio_description.project.delete_description"))
        .build();
    let export = Button::builder(&panel)
        .with_label(&tr("audio_description.project.export"))
        .build();
    let export_srt = Button::builder(&panel)
        .with_label(&tr("audio_description.project.export_srt"))
        .build();
    let export_vtt = Button::builder(&panel)
        .with_label(&tr("audio_description.project.export_vtt"))
        .build();
    let close = Button::builder(&panel)
        .with_id(ID_AUDIO_DESCRIPTION_PROJECT_CLOSE)
        .with_label(&tr("audio_description.close"))
        .build();
    for button in [&play, &delete, &export, &export_srt, &export_vtt, &close] {
        row.add(button, 0, SizerFlag::All, 4);
    }
    root.add_sizer(&row, 0, SizerFlag::Expand, 0);
    panel.set_sizer(root, true);

    let project_selection = Rc::clone(&project);
    let display_order_selection = Rc::clone(&description_display_order);
    let pending_text_selection = Rc::clone(&pending_text_edits);
    let last_selected_selection = Rc::clone(&last_selected_description);
    choice.on_selection_changed(move |_| {
        if let Some(previous_index) = last_selected_selection.get()
            && let Some(previous) = project_selection.borrow().descriptions.get(previous_index)
        {
            let draft = text.get_value().trim().to_string();
            if draft != previous.text {
                pending_text_selection
                    .borrow_mut()
                    .insert(previous_index, draft);
            } else {
                pending_text_selection.borrow_mut().remove(&previous_index);
            }
        }
        if let Some(index) =
            selected_project_description_index(&choice, &display_order_selection)
            && let Some(description) = project_selection.borrow().descriptions.get(index)
        {
            let value = pending_text_selection
                .borrow()
                .get(&index)
                .cloned()
                .unwrap_or_else(|| description.text.clone());
            text.set_value(&value);
            last_selected_selection.set(Some(index));
            status.set_label(&project_description_details(
                &project_selection.borrow(),
                index,
            ));
        }
    });

    let project_search = Rc::clone(&project);
    let display_order_search = Rc::clone(&description_display_order);
    let pending_text_search = Rc::clone(&pending_text_edits);
    let last_selected_search = Rc::clone(&last_selected_description);
    let reanalyzed_ids_search = Rc::clone(&reanalyzed_description_ids);
    let run_description_search = Rc::new(move || {
        if let Some(previous_index) = last_selected_search.get()
            && let Some(previous) = project_search.borrow().descriptions.get(previous_index)
        {
            let draft = text.get_value().trim().to_string();
            if draft != previous.text {
                pending_text_search.borrow_mut().insert(previous_index, draft);
            } else {
                pending_text_search.borrow_mut().remove(&previous_index);
            }
        }
        let query = search.get_value();
        let preferred = if query.trim().is_empty() {
            selected_project_description_index(&choice, &display_order_search)
        } else {
            None
        };
        let selected = refresh_project_description_choice(
            &choice,
            &project_search.borrow().descriptions,
            &display_order_search,
            &query,
            preferred,
            Some(&*reanalyzed_ids_search.borrow()),
        );
        if let Some(index) = selected
            && let Some(description) = project_search.borrow().descriptions.get(index)
        {
            let value = pending_text_search
                .borrow()
                .get(&index)
                .cloned()
                .unwrap_or_else(|| description.text.clone());
            text.set_value(&value);
            last_selected_search.set(Some(index));
            status.set_label(&project_description_details(&project_search.borrow(), index));
        }
        choice.set_focus();
    });
    let run_description_search_button = Rc::clone(&run_description_search);
    search_button.on_click(move |_| run_description_search_button());
    let run_description_search_enter = Rc::clone(&run_description_search);
    search.on_text_enter(move |_| run_description_search_enter());

    let project_search_clear = Rc::clone(&project);
    let display_order_search_clear = Rc::clone(&description_display_order);
    let pending_text_search_clear = Rc::clone(&pending_text_edits);
    let last_selected_search_clear = Rc::clone(&last_selected_description);
    let reanalyzed_ids_search_clear = Rc::clone(&reanalyzed_description_ids);
    search.on_text_changed(move |_| {
        if !search.get_value().trim().is_empty() {
            return;
        }
        if let Some(previous_index) = last_selected_search_clear.get()
            && let Some(previous) = project_search_clear.borrow().descriptions.get(previous_index)
        {
            let draft = text.get_value().trim().to_string();
            if draft != previous.text {
                pending_text_search_clear
                    .borrow_mut()
                    .insert(previous_index, draft);
            } else {
                pending_text_search_clear.borrow_mut().remove(&previous_index);
            }
        }
        let preferred = last_selected_search_clear.get();
        let selected = refresh_project_description_choice(
            &choice,
            &project_search_clear.borrow().descriptions,
            &display_order_search_clear,
            "",
            preferred,
            Some(&*reanalyzed_ids_search_clear.borrow()),
        );
        if let Some(index) = selected
            && let Some(description) = project_search_clear.borrow().descriptions.get(index)
        {
            let value = pending_text_search_clear
                .borrow()
                .get(&index)
                .cloned()
                .unwrap_or_else(|| description.text.clone());
            text.set_value(&value);
            last_selected_search_clear.set(Some(index));
            status.set_label(&project_description_details(
                &project_search_clear.borrow(),
                index,
            ));
        }
    });

    let project_engine = Rc::clone(&project);
    let fill_project_voices_engine = Rc::clone(&fill_project_voices);
    engine.on_selection_changed(move |_| {
        let selected_engine = engine.get_selection().unwrap_or(0);
        let selected_is_system = selected_engine == 1;
        let current = project_engine.borrow();
        let current_is_system = crate::is_system_voice_engine(&current.tts_engine);
        let preferred = (selected_is_system == current_is_system)
            .then(|| current.tts_voice.clone());
        drop(current);
        fill_project_voices_engine(selected_engine, preferred);
    });

    let project_voice = Rc::clone(&project);
    let path_voice = path.clone();
    let project_voices_change = Rc::clone(&project_voices);
    let fill_project_voices_change = Rc::clone(&fill_project_voices);
    let rt_voice = Arc::clone(rt);
    let dialog_voice = dialog;
    let display_order_voice = Rc::clone(&description_display_order);
    let pending_text_voice = Rc::clone(&pending_text_edits);
    let reanalyzed_ids_voice = Rc::clone(&reanalyzed_description_ids);
    let last_selected_voice = Rc::clone(&last_selected_description);
    change_voice.on_click(move |_| {
        let selected_description_index =
            selected_project_description_index(&choice, &display_order_voice).unwrap_or(0);
        if let Some(description) = project_voice
            .borrow()
            .descriptions
            .get(selected_description_index)
        {
            let draft = text.get_value().trim().to_string();
            if draft != description.text {
                pending_text_voice
                    .borrow_mut()
                    .insert(selected_description_index, draft);
            } else {
                pending_text_voice
                    .borrow_mut()
                    .remove(&selected_description_index);
            }
            last_selected_voice.set(Some(selected_description_index));
        }
        if !pending_text_voice.borrow().is_empty() {
            show_project_error(
                &dialog_voice,
                &tr("audio_description.project.apply_before_export"),
            );
            return;
        }
        let selected_voice_index = voice.get_selection().unwrap_or(0) as usize;
        let candidate = project_voices_change
            .borrow()
            .get(selected_voice_index)
            .cloned();
        let Some(candidate) = candidate else {
            show_project_error(&dialog_voice, &tr("audio_description.error.voice"));
            return;
        };
        let selected_engine_index = engine.get_selection().unwrap_or(0);
        let candidate_engine = if selected_engine_index == 1 {
            "system".to_string()
        } else {
            "microsoft".to_string()
        };

        let snapshot = project_voice.borrow().clone();
        let same_engine = crate::is_system_voice_engine(&candidate_engine)
            == crate::is_system_voice_engine(&snapshot.tts_engine);
        if same_engine && candidate.short_name == snapshot.tts_voice {
            return;
        }
        let previous_engine_index = if crate::is_system_voice_engine(&snapshot.tts_engine) {
            1
        } else {
            0
        };
        let previous_voice = snapshot.tts_voice.clone();

        let (validation, fit_error) = run_project_voice_validation_with_progress(
            &dialog_voice,
            snapshot,
            path_voice.clone(),
            candidate_engine,
            candidate.short_name.clone(),
            Arc::clone(&rt_voice),
        );

        match validation {
            Ok(updated) => {
                *project_voice.borrow_mut() = updated;
                if !project_voice.borrow().descriptions.is_empty() {
                    let selected_description = selected_description_index
                        .min(project_voice.borrow().descriptions.len().saturating_sub(1));
                    let query = search.get_value();
                    let selected = refresh_project_description_choice(
                        &choice,
                        &project_voice.borrow().descriptions,
                        &display_order_voice,
                        &query,
                        Some(selected_description),
                        Some(&*reanalyzed_ids_voice.borrow()),
                    );
                    if let Some(selected_description) = selected
                        && let Some(description) =
                            project_voice.borrow().descriptions.get(selected_description)
                    {
                        text.set_value(&description.text);
                        status.set_label(&project_description_details(
                            &project_voice.borrow(),
                            selected_description,
                        ));
                    }
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
                engine.set_selection(previous_engine_index);
                fill_project_voices_change(
                    previous_engine_index,
                    Some(previous_voice.clone()),
                );
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
            }
        }
    });

    let project_reanalyze = Rc::clone(&project);
    let pending_reanalysis_run = Rc::clone(&pending_reanalysis);
    let settings_reanalyze = Arc::clone(settings);
    let rt_reanalyze = Arc::clone(rt);
    let dialog_reanalyze = dialog;
    let display_order_reanalyze = Rc::clone(&description_display_order);
    let pending_text_reanalyze = Rc::clone(&pending_text_edits);
    let last_selected_reanalyze = Rc::clone(&last_selected_description);
    let reanalyzed_ids_reanalyze = Rc::clone(&reanalyzed_description_ids);
    reanalyze.on_click(move |_| {
        if pending_reanalysis_run.get() {
            return;
        }
        let Some(index) = selected_project_description_index(&choice, &display_order_reanalyze)
        else {
            show_project_error(
                &dialog_reanalyze,
                &tr("audio_description.project.no_selection"),
            );
            return;
        };
        if let Some(description) = project_reanalyze.borrow().descriptions.get(index) {
            let draft = text.get_value().trim().to_string();
            if draft != description.text {
                pending_text_reanalyze.borrow_mut().insert(index, draft);
            } else {
                pending_text_reanalyze.borrow_mut().remove(&index);
            }
            last_selected_reanalyze.set(Some(index));
        }
        if !pending_text_reanalyze.borrow().is_empty() {
            show_project_error(
                &dialog_reanalyze,
                &tr("audio_description.project.apply_before_export"),
            );
            return;
        }
        let settings_snapshot = settings_reanalyze.lock().unwrap().clone();
        // IMPORTANT: clone the project in a separate statement.  Keeping
        // `project_reanalyze.borrow()` inside the scrutinee of the `match` can
        // extend the RefCell immutable borrow through the whole match statement.
        // The Ok arm then needs `borrow_mut()` to replace the project, which can
        // panic with BorrowMutError on macOS exactly after the progress dialog
        // closes.  A separate snapshot guarantees the Ref is dropped here.
        let project_snapshot = { project_reanalyze.borrow().clone() };
        let reanalysis_result = run_project_reanalysis_with_progress(
            &dialog_reanalyze,
            project_snapshot,
            index,
            settings_snapshot,
            Arc::clone(&rt_reanalyze),
        );
        append_podcast_log("audio_description.project.reanalyze_call_returned");
        match reanalysis_result {
            Ok(reanalysis) => {
                let affected_count = reanalysis.segment_description_ids.len();
                let focus_index = reanalysis.focus_index;

                // The Choice refresh can emit a selection-changed event.  Clear the
                // old selection/draft state before swapping in the reanalyzed project,
                // otherwise the old text can be captured as a pending edit against
                // the new description and appear again when the user revisits it.
                pending_text_reanalyze.borrow_mut().clear();
                last_selected_reanalyze.set(None);
                append_podcast_log("audio_description.project.reanalyze_apply_begin");
                *project_reanalyze.borrow_mut() = reanalysis.project;
                append_podcast_log("audio_description.project.reanalyze_project_replaced");
                *reanalyzed_ids_reanalyze.borrow_mut() = reanalysis
                    .segment_description_ids
                    .iter()
                    .copied()
                    .collect();
                pending_reanalysis_run.set(true);
                apply_reanalyzed.enable(true);
                reanalyze.enable(false);
                delete.enable(false);
                export.enable(false);
                change_voice.enable(false);
                let query = search.get_value();
                let selected = refresh_project_description_choice(
                    &choice,
                    &project_reanalyze.borrow().descriptions,
                    &display_order_reanalyze,
                    &query,
                    Some(focus_index),
                    Some(&*reanalyzed_ids_reanalyze.borrow()),
                );
                if let Some(selected_index) = selected
                    && let Some(description) =
                        project_reanalyze.borrow().descriptions.get(selected_index)
                {
                    text.set_value(&description.text);
                    last_selected_reanalyze.set(Some(selected_index));
                    // A synchronous Choice event may have run while rebuilding the
                    // control.  The visible text is now authoritative, so no stale
                    // draft from the pre-reanalysis project must survive.
                    pending_text_reanalyze.borrow_mut().clear();
                    status.set_label(&trf(
                        "audio_description.project.status.reanalyzed_ready_count",
                        &[("count", affected_count.to_string())],
                    ));
                    // Match the Windows editor: return focus to the descriptions so
                    // VoiceOver immediately announces the new text and the
                    // "segment reanalyzed" marker.
                    choice.set_focus();
                }
                append_podcast_log(&format!(
                    "audio_description.project.reanalyze_ready affected={} focus={}",
                    affected_count, focus_index
                ));
            }
            Err(error) if error == "cancelled" => {
                status.set_label(&tr("audio_description.project.status.ready"));
            }
            Err(error) => show_project_error(&dialog_reanalyze, &error),
        }
    });

    let project_apply_reanalyzed = Rc::clone(&project);
    let pending_reanalysis_commit = Rc::clone(&pending_reanalysis);
    let path_apply_reanalyzed = path.clone();
    let rt_apply_reanalyzed = Arc::clone(rt);
    let dialog_apply_reanalyzed = dialog;
    let display_order_apply_reanalyzed = Rc::clone(&description_display_order);
    apply_reanalyzed.on_click(move |_| {
        if !pending_reanalysis_commit.get() {
            return;
        }
        if let Some(index) =
            selected_project_description_index(&choice, &display_order_apply_reanalyzed)
        {
            let draft = text.get_value().trim().to_string();
            if draft.is_empty() {
                show_project_error(
                    &dialog_apply_reanalyzed,
                    &tr("audio_description.project.error_empty"),
                );
                return;
            }
            if project_apply_reanalyzed
                .borrow()
                .descriptions
                .get(index)
                .is_some_and(|description| description.text != draft)
            {
                show_project_error(
                    &dialog_apply_reanalyzed,
                    &tr("audio_description.project.apply_before_export"),
                );
                return;
            }
        }
        status.set_label(&tr("audio_description.project.status.exporting"));
        match run_project_export_with_progress(
            &dialog_apply_reanalyzed,
            project_apply_reanalyzed.borrow().clone(),
            path_apply_reanalyzed.clone(),
            Arc::clone(&rt_apply_reanalyzed),
            true,
        ) {
            Ok(_) => {
                pending_reanalysis_commit.set(false);
                show_completion(
                    &dialog_apply_reanalyzed,
                    &tr("audio_description.project.reanalyzed_applied"),
                );
                dialog_apply_reanalyzed.end_modal(ID_AUDIO_DESCRIPTION_PROJECT_CLOSE);
            }
            Err(error) if error == "cancelled" => {
                status.set_label(&tr("audio_description.project.status.reanalyzed_ready"));
            }
            Err(error) => {
                show_project_error(&dialog_apply_reanalyzed, &error);
                status.set_label(&tr("audio_description.project.status.reanalyzed_ready"));
            }
        }
    });

    let project_apply = Rc::clone(&project);
    let path_apply = path.clone();
    let dialog_apply = dialog;
    let rt_apply = Arc::clone(rt);
    let display_order_apply = Rc::clone(&description_display_order);
    let pending_reanalysis_apply = Rc::clone(&pending_reanalysis);
    let pending_text_apply = Rc::clone(&pending_text_edits);
    let reanalyzed_ids_apply = Rc::clone(&reanalyzed_description_ids);
    let last_selected_apply = Rc::clone(&last_selected_description);
    apply.on_click(move |_| {
        let Some(index) = selected_project_description_index(&choice, &display_order_apply) else {
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
            .is_some_and(|description| description.text != value)
        {
            pending_text_apply.borrow_mut().insert(index, value);
        } else {
            pending_text_apply.borrow_mut().remove(&index);
        }
        if pending_text_apply.borrow().is_empty() {
            status.set_label(&tr("audio_description.project.edit_saved"));
            return;
        }

        status.set_label(&tr("audio_description.project.status.checking_duration"));
        let mut staged = pending_text_apply
            .borrow()
            .iter()
            .map(|(edit_index, value)| (*edit_index, value.clone()))
            .collect::<Vec<_>>();
        staged.sort_by_key(|(edit_index, _)| *edit_index);
        let mut validated = Vec::<(usize, String)>::with_capacity(staged.len());
        for (edit_index, edit_value) in staged {
            if edit_value.trim().is_empty() {
                let query = search.get_value();
                refresh_project_description_choice(
                    &choice,
                    &project_apply.borrow().descriptions,
                    &display_order_apply,
                    &query,
                    Some(edit_index),
                    Some(&*reanalyzed_ids_apply.borrow()),
                );
                text.set_value(&edit_value);
                last_selected_apply.set(Some(edit_index));
                show_project_error(&dialog_apply, &tr("audio_description.project.error_empty"));
                return;
            }
            let available = match project_edit_available_duration(&project_apply.borrow(), edit_index) {
                Ok(value) => value,
                Err(error) => {
                    show_project_error(&dialog_apply, &error);
                    return;
                }
            };
            let duration = match synthesize_project_text_duration(
                &project_apply.borrow(),
                &edit_value,
                edit_index,
                &rt_apply,
            ) {
                Ok(value) => value,
                Err(error) => {
                    show_project_error(&dialog_apply, &error);
                    return;
                }
            };
            if let Some(available) = available
                && duration > available + 0.010
            {
                let query = search.get_value();
                refresh_project_description_choice(
                    &choice,
                    &project_apply.borrow().descriptions,
                    &display_order_apply,
                    &query,
                    Some(edit_index),
                    Some(&*reanalyzed_ids_apply.borrow()),
                );
                text.set_value(&edit_value);
                last_selected_apply.set(Some(edit_index));
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
                status.set_label(&project_description_details(&project_apply.borrow(), edit_index));
                return;
            }
            validated.push((edit_index, edit_value));
        }

        let mut updated_project = project_apply.borrow().clone();
        for (edit_index, edit_value) in &validated {
            if let Some(description) = updated_project.descriptions.get_mut(*edit_index) {
                description.text = edit_value.clone();
                description.modified = description.text != description.original_text;
            }
        }
        updated_project.updated_at_utc = now_utc();
        if !pending_reanalysis_apply.get()
            && let Err(error) = save_project(&path_apply, &updated_project)
        {
            show_project_error(&dialog_apply, &error);
            return;
        }
        *project_apply.borrow_mut() = updated_project;
        let applied_count = validated.len();
        pending_text_apply.borrow_mut().clear();
        let query = search.get_value();
        refresh_project_description_choice(
            &choice,
            &project_apply.borrow().descriptions,
            &display_order_apply,
            &query,
            Some(index),
            Some(&*reanalyzed_ids_apply.borrow()),
        );
        last_selected_apply.set(Some(index));
        let message = trf(
            "audio_description.project.edit_saved_multiple",
            &[("count", applied_count.to_string())],
        );
        status.set_label(&message);
        let info = MessageDialog::builder(
            &dialog_apply,
            &message,
            &tr("audio_description.project.edit_success_title"),
        )
        .with_style(MessageDialogStyle::OK | MessageDialogStyle::IconInformation)
        .build();
        info.show_modal();
    });

    let project_play = Rc::clone(&project);
    let rt_play = Arc::clone(rt);
    let dialog_play = dialog;
    let display_order_play = Rc::clone(&description_display_order);
    play.on_click(move |_| {
        let Some(index) = selected_project_description_index(&choice, &display_order_play) else {
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
    let display_order_delete = Rc::clone(&description_display_order);
    let pending_text_delete = Rc::clone(&pending_text_edits);
    let reanalyzed_ids_delete = Rc::clone(&reanalyzed_description_ids);
    let last_selected_delete = Rc::clone(&last_selected_description);
    delete.on_click(move |_| {
        let Some(index) = selected_project_description_index(&choice, &display_order_delete) else {
            show_error(
                &dialog_delete,
                &tr("audio_description.project.no_selection"),
            );
            return;
        };
        if let Some(description) = project_delete.borrow().descriptions.get(index) {
            let draft = text.get_value().trim().to_string();
            if draft != description.text {
                pending_text_delete.borrow_mut().insert(index, draft);
            } else {
                pending_text_delete.borrow_mut().remove(&index);
            }
        }
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
        let mut updated_project = project_delete.borrow().clone();
        updated_project.descriptions.remove(index);
        for (new_id, description) in updated_project.descriptions.iter_mut().enumerate() {
            description.id = new_id;
        }
        updated_project.updated_at_utc = now_utc();
        if let Err(error) = save_project(&path_delete, &updated_project) {
            show_error(&dialog_delete, &error);
            return;
        }
        *project_delete.borrow_mut() = updated_project;
        {
            let previous = std::mem::take(&mut *pending_text_delete.borrow_mut());
            let mut adjusted = HashMap::new();
            for (draft_index, draft) in previous {
                if draft_index == index {
                    continue;
                }
                let adjusted_index = if draft_index > index {
                    draft_index - 1
                } else {
                    draft_index
                };
                adjusted.insert(adjusted_index, draft);
            }
            *pending_text_delete.borrow_mut() = adjusted;
        }
        let next_index = index.min(project_delete.borrow().descriptions.len() - 1);
        let query = search.get_value();
        let selected = refresh_project_description_choice(
            &choice,
            &project_delete.borrow().descriptions,
            &display_order_delete,
            &query,
            Some(next_index),
            Some(&*reanalyzed_ids_delete.borrow()),
        );
        if let Some(selected_index) = selected
            && let Some(description) = project_delete.borrow().descriptions.get(selected_index)
        {
            let value = pending_text_delete
                .borrow()
                .get(&selected_index)
                .cloned()
                .unwrap_or_else(|| description.text.clone());
            text.set_value(&value);
            last_selected_delete.set(Some(selected_index));
        }
        status.set_label(&tr("audio_description.project.description_deleted"));
    });

    let project_export = Rc::clone(&project);
    let path_export = path.clone();
    let rt_export = Arc::clone(rt);
    let dialog_export = dialog;
    let display_order_export = Rc::clone(&description_display_order);
    let pending_text_export = Rc::clone(&pending_text_edits);
    let last_selected_export = Rc::clone(&last_selected_description);
    export.on_click(move |_| {
        if let Some(index) = selected_project_description_index(&choice, &display_order_export)
            && let Some(description) = project_export.borrow().descriptions.get(index)
        {
            let draft = text.get_value().trim().to_string();
            if draft != description.text {
                pending_text_export.borrow_mut().insert(index, draft);
            } else {
                pending_text_export.borrow_mut().remove(&index);
            }
            last_selected_export.set(Some(index));
        }
        if !pending_text_export.borrow().is_empty() {
            show_error(
                &dialog_export,
                &tr("audio_description.project.apply_before_export"),
            );
            return;
        }
        let result = run_project_export_with_progress(
            &dialog_export,
            project_export.borrow().clone(),
            path_export.clone(),
            Arc::clone(&rt_export),
            false,
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

    let project_export_srt = Rc::clone(&project);
    let dialog_export_srt = dialog;
    let display_order_export_srt = Rc::clone(&description_display_order);
    let pending_text_export_srt = Rc::clone(&pending_text_edits);
    let last_selected_export_srt = Rc::clone(&last_selected_description);
    export_srt.on_click(move |_| {
        if let Some(index) =
            selected_project_description_index(&choice, &display_order_export_srt)
            && let Some(description) = project_export_srt.borrow().descriptions.get(index)
        {
            let draft = text.get_value().trim().to_string();
            if draft != description.text {
                pending_text_export_srt.borrow_mut().insert(index, draft);
            } else {
                pending_text_export_srt.borrow_mut().remove(&index);
            }
            last_selected_export_srt.set(Some(index));
        }
        if !pending_text_export_srt.borrow().is_empty() {
            show_error(
                &dialog_export_srt,
                &tr("audio_description.project.apply_before_export"),
            );
            return;
        }
        let snapshot = project_export_srt.borrow();
        match export_project_subtitles(&dialog_export_srt, &snapshot, "srt") {
            Ok(Some(path)) => show_completion(
                &dialog_export_srt,
                &trf(
                    "audio_description.project.export_subtitle_success",
                    &[
                        ("format", "SRT".to_string()),
                        ("path", path.to_string_lossy().into_owned()),
                    ],
                ),
            ),
            Ok(None) => {}
            Err(error) => show_error(
                &dialog_export_srt,
                &trf(
                    "audio_description.project.export_subtitle_error",
                    &[("format", "SRT".to_string()), ("error", error)],
                ),
            ),
        }
    });

    let project_export_vtt = Rc::clone(&project);
    let dialog_export_vtt = dialog;
    let display_order_export_vtt = Rc::clone(&description_display_order);
    let pending_text_export_vtt = Rc::clone(&pending_text_edits);
    let last_selected_export_vtt = Rc::clone(&last_selected_description);
    export_vtt.on_click(move |_| {
        if let Some(index) =
            selected_project_description_index(&choice, &display_order_export_vtt)
            && let Some(description) = project_export_vtt.borrow().descriptions.get(index)
        {
            let draft = text.get_value().trim().to_string();
            if draft != description.text {
                pending_text_export_vtt.borrow_mut().insert(index, draft);
            } else {
                pending_text_export_vtt.borrow_mut().remove(&index);
            }
            last_selected_export_vtt.set(Some(index));
        }
        if !pending_text_export_vtt.borrow().is_empty() {
            show_error(
                &dialog_export_vtt,
                &tr("audio_description.project.apply_before_export"),
            );
            return;
        }
        let snapshot = project_export_vtt.borrow();
        match export_project_subtitles(&dialog_export_vtt, &snapshot, "vtt") {
            Ok(Some(path)) => show_completion(
                &dialog_export_vtt,
                &trf(
                    "audio_description.project.export_subtitle_success",
                    &[
                        ("format", "VTT".to_string()),
                        ("path", path.to_string_lossy().into_owned()),
                    ],
                ),
            ),
            Ok(None) => {}
            Err(error) => show_error(
                &dialog_export_vtt,
                &trf(
                    "audio_description.project.export_subtitle_error",
                    &[("format", "VTT".to_string()), ("error", error)],
                ),
            ),
        }
    });

    dialog.set_escape_id(ID_AUDIO_DESCRIPTION_PROJECT_CLOSE);
    let dialog_close = dialog;
    let project_close = Rc::clone(&project);
    let display_order_close = Rc::clone(&description_display_order);
    let pending_text_close = Rc::clone(&pending_text_edits);
    let last_selected_close = Rc::clone(&last_selected_description);
    let pending_reanalysis_close = Rc::clone(&pending_reanalysis);
    close.on_click(move |_| {
        append_podcast_log("audio_description.project.close_requested_button");
        if let Some(index) = selected_project_description_index(&choice, &display_order_close)
            && let Some(description) = project_close.borrow().descriptions.get(index)
        {
            let draft = text.get_value().trim().to_string();
            if draft != description.text {
                pending_text_close.borrow_mut().insert(index, draft);
            } else {
                pending_text_close.borrow_mut().remove(&index);
            }
            last_selected_close.set(Some(index));
        }
        if (!pending_text_close.borrow().is_empty() || pending_reanalysis_close.get())
            && !crate::ask_yes_no_dialog(
                &dialog_close,
                &tr("audio_description.project.title"),
                &tr("audio_description.project.unsaved_close"),
            )
        {
            return;
        }
        dialog_close.end_modal(ID_AUDIO_DESCRIPTION_PROJECT_CLOSE);
    });
    let dialog_window_close = dialog;
    let project_window_close = Rc::clone(&project);
    let display_order_window_close = Rc::clone(&description_display_order);
    let pending_text_window_close = Rc::clone(&pending_text_edits);
    let last_selected_window_close = Rc::clone(&last_selected_description);
    let pending_reanalysis_window_close = Rc::clone(&pending_reanalysis);
    dialog.on_close(move |event| {
        append_podcast_log("audio_description.project.close_requested_window");
        if let Some(index) =
            selected_project_description_index(&choice, &display_order_window_close)
            && let Some(description) = project_window_close.borrow().descriptions.get(index)
        {
            let draft = text.get_value().trim().to_string();
            if draft != description.text {
                pending_text_window_close.borrow_mut().insert(index, draft);
            } else {
                pending_text_window_close.borrow_mut().remove(&index);
            }
            last_selected_window_close.set(Some(index));
        }
        if (!pending_text_window_close.borrow().is_empty() || pending_reanalysis_window_close.get())
            && !crate::ask_yes_no_dialog(
                &dialog_window_close,
                &tr("audio_description.project.title"),
                &tr("audio_description.project.unsaved_close"),
            )
        {
            event.skip(false);
            return;
        }
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
    use super::{
        AUDIO_DESCRIPTION_LANGUAGES, ProjectDescription, project_description_search_order,
        should_delete_input_after_success,
    };
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

    fn test_project_description(id: usize, text: &str) -> ProjectDescription {
        ProjectDescription {
            id,
            text: text.to_string(),
            original_text: text.to_string(),
            rendered_text: text.to_string(),
            modified: false,
            gemini_start_sec: id as f64,
            visual_evidence_time_sec: None,
            mandatory: false,
            slot_id: String::new(),
            slot_start_sec: None,
            slot_end_sec: None,
            source_start_sec: id as f64,
            output_start_sec: id as f64,
            output_end_sec: id as f64 + 1.0,
            tts_duration_sec: 1.0,
            extended_pause: false,
            extended_pause_duration_sec: 0.0,
            duck_start_sec: None,
            duck_end_sec: None,
        }
    }

    #[test]
    fn project_description_search_moves_matches_to_top_without_hiding_other_rows() {
        let descriptions = vec![
            test_project_description(0, "Elrond osserva la sala."),
            test_project_description(1, "Galadriel entra nella stanza."),
            test_project_description(2, "GALADRIEL guarda verso il mare."),
            test_project_description(3, "Sauron si volta."),
        ];

        assert_eq!(
            project_description_search_order(&descriptions, "galadriel"),
            vec![1, 2, 0, 3]
        );
        assert_eq!(
            project_description_search_order(&descriptions, "  GALADRIEL  "),
            vec![1, 2, 0, 3]
        );
        assert_eq!(
            project_description_search_order(&descriptions, "nessun risultato"),
            vec![0, 1, 2, 3]
        );
        assert_eq!(
            project_description_search_order(&descriptions, ""),
            vec![0, 1, 2, 3]
        );
    }

    #[test]
    fn project_search_controls_are_localized_in_every_ui_language() {
        for (ui_language, raw) in UI_TRANSLATIONS {
            let translations: HashMap<String, String> =
                serde_json::from_str(raw).expect("valid audio-description translations");
            for key in [
                "audio_description.project.search",
                "audio_description.project.search_button",
            ] {
                assert!(
                    translations
                        .get(key)
                        .is_some_and(|label| !label.trim().is_empty()),
                    "missing {key} for UI language {ui_language}"
                );
            }
        }
    }

    #[test]
    fn delete_video_controls_are_localized_in_every_ui_language() {
        for (ui_language, raw) in UI_TRANSLATIONS {
            let translations: HashMap<String, String> =
                serde_json::from_str(raw).expect("valid audio-description translations");
            for key in [
                "audio_description.delete_video_after",
                "audio_description.input_trashed",
                "audio_description.input_trash_failed",
            ] {
                assert!(
                    translations
                        .get(key)
                        .is_some_and(|label| !label.trim().is_empty()),
                    "missing {key} for UI language {ui_language}"
                );
            }
        }
    }

    #[test]
    fn project_saving_always_prevents_deleting_the_source_video() {
        assert!(should_delete_input_after_success(true, false));
        assert!(!should_delete_input_after_success(true, true));
        assert!(!should_delete_input_after_success(false, false));
    }

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

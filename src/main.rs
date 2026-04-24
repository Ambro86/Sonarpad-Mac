#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod articles;
mod curl_client;
mod edge_tts;
mod file_loader;
mod podcast_player;
mod podcasts;
mod reader;

use docx_rs::{Docx, Paragraph, Run};
use printpdf::{BuiltinFont, Mm, Op, PdfDocument, PdfPage, PdfSaveOptions, Point, Pt, TextItem};
use quick_xml::Reader;
use quick_xml::events::Event;
use rodio::{Decoder, OutputStream, Sink, Source};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::HashSet;
use std::io::{BufReader, Cursor};
#[cfg(any(target_os = "macos", windows))]
use std::io::{Read, Write};
#[cfg(target_os = "macos")]
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;
#[cfg(target_os = "macos")]
use std::rc::Weak;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::runtime::Runtime;
#[cfg(any(target_os = "macos", windows))]
use uuid::Uuid;
use wxdragon::event::KeyboardEvent;
use wxdragon::prelude::*;
use wxdragon::timer::Timer;

const ID_OPEN: i32 = 101;
#[cfg(target_os = "macos")]
const ID_EXIT: i32 = wxdragon::ffi::WXD_ID_EXIT as i32;
#[cfg(not(target_os = "macos"))]
const ID_EXIT: i32 = 102;
#[cfg(target_os = "macos")]
const ID_ABOUT: i32 = wxdragon::ffi::WXD_ID_ABOUT as i32;
#[cfg(not(target_os = "macos"))]
const ID_ABOUT: i32 = 103;
const ID_DONATIONS: i32 = 104;
const ID_CHECK_UPDATES: i32 = 105;
const ID_CHANGELOG: i32 = 106;
const ID_START_PLAYBACK: i32 = 2000;
const ID_PLAY_PAUSE: i32 = 2001;
const ID_STOP: i32 = 2003;
const ID_SAVE: i32 = 2002;
#[cfg(target_os = "macos")]
const ID_SETTINGS: i32 = wxdragon::ffi::WXD_ID_PREFERENCES as i32;
#[cfg(not(target_os = "macos"))]
const ID_SETTINGS: i32 = 2004;
const ID_SAVE_TEXT: i32 = 2007;
const ID_SAVE_TEXT_AS: i32 = 2008;
const ID_PODCAST_BACKWARD: i32 = 2005;
const ID_PODCAST_FORWARD: i32 = 2006;
const ID_ARTICLES_ADD_SOURCE: i32 = 2100;
const ID_ARTICLES_DELETE_SOURCE: i32 = 2101;
const ID_ARTICLES_EDIT_SOURCE: i32 = 2102;
const ID_ARTICLES_REORDER_SOURCES: i32 = 2103;
const ID_ARTICLES_SORT_SOURCES_ALPHABETICALLY: i32 = 2104;
const ID_ARTICLES_IMPORT_SOURCES: i32 = 2105;
const ID_ARTICLES_EXPORT_SOURCES: i32 = 2106;
const ID_PODCASTS_ADD: i32 = 2300;
const ID_PODCASTS_DELETE: i32 = 2301;
const ID_PODCASTS_REORDER_SOURCES: i32 = 2302;
const ID_PODCASTS_SORT_SOURCES_ALPHABETICALLY: i32 = 2303;
const ID_PODCAST_DIALOG_OPEN: i32 = 4101;
const ID_PODCAST_DIALOG_SAVE_AS: i32 = 4102;
const ID_PODCAST_DIALOG_CLOSE: i32 = 4103;
const ID_AUDIOBOOK_DIALOG_CANCEL: i32 = 4104;
const ID_PODCASTS_CATEGORY_BASE: i32 = 2400;
const ID_PODCASTS_SOURCE_BASE: i32 = 2600;
const ID_PODCASTS_EPISODE_BASE: i32 = 30000;
const ID_PODCASTS_CATEGORY_PODCAST_BASE: i32 = 27000;
const ID_RADIO_SEARCH: i32 = 2350;
const ID_RADIO_DELETE_FAVORITE: i32 = 2351;
const ID_RADIO_ADD: i32 = 2352;
const ID_RADIO_EDIT_FAVORITE: i32 = 2353;
const ID_RADIO_REORDER_FAVORITES: i32 = 2354;
const ID_RADIO_FAVORITE_BASE: i32 = 6000;
const RADIO_BROWSER_LIMIT: &str = "100000";
const RADIO_RESULTS_PAGE_SIZE: usize = 25;
const ID_ARTICLES_SOURCE_BASE: i32 = 2200;
const ID_ARTICLE_FOLDER_DIALOG_BASE: i32 = 7000;
const ID_ARTICLE_SOURCE_DIALOG_BASE: i32 = 9000;
const ID_ARTICLES_ARTICLE_BASE: i32 = 10000;
const MAX_MENU_ARTICLES_PER_SOURCE: usize = 30;
const MAX_MENU_PODCAST_EPISODES_PER_SOURCE: usize = 30;
const PODCAST_SEEK_SECONDS: f64 = 30.0;
const PODCAST_SLIDER_RANGE: i32 = 1000;

const AUDIOBOOK_SAVE_THREADS: usize = 8;
const WXK_LEFT: i32 = 314;
const WXK_RIGHT: i32 = 316;
#[cfg(target_os = "macos")]
const WXK_MAC_CMD_PERIOD_SUFFIX: i32 = 315;
#[cfg(target_os = "macos")]
const APP_STORAGE_DIR_NAME: &str = "Sonarpad";

#[cfg(target_os = "macos")]
static MAC_NATIVE_FILE_DIALOG_OPEN: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "macos")]
static MAC_MENU_BAR_ACTIVE: AtomicBool = AtomicBool::new(false);

#[cfg(target_os = "macos")]
const MOD_CMD: &str = "Cmd";
#[cfg(not(target_os = "macos"))]
const MOD_CMD: &str = "Ctrl";

#[cfg(target_os = "macos")]
const MOD_ALT: &str = "Option";
#[cfg(not(target_os = "macos"))]
const MOD_ALT: &str = "Alt";

const SONARPAD_MINIMAL_RELEASES_URL: &str =
    "https://github.com/Ambro86/Sonarpad-Mac/releases/latest";
const SONARPAD_MINIMAL_RELEASES_API_URL: &str =
    "https://api.github.com/repos/Ambro86/Sonarpad-Mac/releases/latest";
#[derive(PartialEq, Clone, Copy, Debug)]
enum PlaybackStatus {
    Stopped,
    Playing,
    Paused,
}

struct GlobalPlayback {
    sink: Option<Arc<Sink>>,
    status: PlaybackStatus,
    download_finished: bool,
    refresh_requested: bool,
    generation: u64,
    cached_tts: Option<TtsPlaybackCache>,
}

#[derive(Clone)]
struct TtsPlaybackCache {
    text: String,
    voice: String,
    rate: i32,
    pitch: i32,
    volume: i32,
    chunks: Vec<Vec<u8>>,
}

struct ArticleMenuState {
    dirty: bool,
    loading_urls: HashSet<String>,
    pending_dialog: Option<PendingArticleMenuDialog>,
}

struct PodcastMenuState {
    dirty: bool,
    loading_urls: HashSet<String>,
    category_results: HashMap<u32, Vec<podcasts::PodcastSearchResult>>,
    category_loading: HashSet<u32>,
}

#[derive(Clone, Deserialize)]
struct RadioStation {
    name: String,
    stream_url: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct RadioFavorite {
    language_code: String,
    name: String,
    stream_url: String,
}

#[cfg(target_os = "macos")]
#[derive(Clone)]
struct MacRadioMpvSession {
    ipc_path: PathBuf,
    process_id: u32,
    stream_url: String,
}

#[cfg(target_os = "macos")]
struct MacRadioWindowState {
    session: Option<MacRadioMpvSession>,
    ipc: Option<UnixStream>,
    child: Option<std::process::Child>,
    next_request_id: u64,
    status: PlaybackStatus,
}

#[cfg(target_os = "macos")]
thread_local! {
    static ACTIVE_MAC_RADIO_STATES: RefCell<Vec<Weak<RefCell<MacRadioWindowState>>>> = const { RefCell::new(Vec::new()) };
}

struct RadioMenuState {
    dirty: bool,
    loading_languages: HashSet<String>,
    failed_languages: HashSet<String>,
    stations_by_language: HashMap<String, Vec<RadioStation>>,
    station_ids: HashMap<i32, RadioFavorite>,
    open_search_requested: bool,
    search_ever_opened: bool,
}

struct PodcastPlaybackState {
    player: Option<podcast_player::PodcastPlayer>,
    selected_episode: Option<podcasts::PodcastEpisode>,
    current_audio_url: String,
    status: PlaybackStatus,
}

struct SaveAudiobookState {
    completed_chunks: usize,
    completed: bool,
    cancelled: bool,
    error_message: Option<String>,
}

#[cfg(target_os = "macos")]
struct PendingMacUpdateInstall {
    work_dir: PathBuf,
    extracted_app_path: PathBuf,
}

enum PendingSaveDialog {
    Success,
    Error(String),
}

#[derive(Clone)]
enum PendingArticleMenuDialog {
    Folder(String),
    Source(usize),
}

enum PodcastDownloadAction {
    Open,
    SaveAs,
    Close,
}

struct ShortcutActions {
    start: Rc<dyn Fn()>,
    play_pause: Rc<dyn Fn()>,
    stop: Rc<dyn Fn()>,
    save: Rc<dyn Fn()>,
    settings: Rc<dyn Fn()>,
}

#[derive(Deserialize)]
struct GithubReleaseInfo {
    tag_name: String,
    #[cfg(target_os = "macos")]
    #[serde(default)]
    assets: Vec<GithubReleaseAsset>,
}

#[cfg(target_os = "macos")]
#[derive(Deserialize, Clone)]
struct GithubReleaseAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Serialize, Deserialize, Clone)]
struct Settings {
    #[serde(default = "default_ui_language")]
    ui_language: String,
    language: String,
    voice: String,
    rate: i32,
    pitch: i32,
    volume: i32,
    #[serde(default = "articles::default_italian_sources")]
    article_sources: Vec<articles::ArticleSource>,
    #[serde(default)]
    article_folders: Vec<String>,
    #[serde(default)]
    podcast_sources: Vec<podcasts::PodcastSource>,
    #[serde(default)]
    radio_favorites: Vec<RadioFavorite>,
    #[serde(default = "default_audiobook_format")]
    last_audiobook_format: String,
    #[serde(default)]
    last_audiobook_save_dir: String,
    #[serde(default = "default_text_save_format")]
    last_text_save_format: String,
    #[serde(default)]
    last_text_save_dir: String,
}

impl Settings {
    fn load() -> Self {
        if let Some(data) = read_app_storage_text("settings.json")
            && let Ok(mut settings) = serde_json::from_str::<Settings>(&data)
        {
            settings.ui_language = normalize_ui_language(&settings.ui_language);
            normalize_settings_data(&mut settings);
            return settings;
        }
        let mut settings = Settings {
            ui_language: default_ui_language(),
            language: "Italiano".to_string(),
            voice: "".to_string(),
            rate: 0,
            pitch: 0,
            volume: 100,
            article_sources: articles::default_italian_sources(),
            article_folders: Vec::new(),
            podcast_sources: Vec::new(),
            radio_favorites: Vec::new(),
            last_audiobook_format: default_audiobook_format(),
            last_audiobook_save_dir: String::new(),
            last_text_save_format: default_text_save_format(),
            last_text_save_dir: String::new(),
        };
        normalize_settings_data(&mut settings);
        settings
    }

    fn save(&self) {
        if let Ok(data) = serde_json::to_string_pretty(self)
            && let Err(err) = write_app_storage_text("settings.json", &data)
        {
            println!("ERROR: Salvataggio impostazioni fallito: {}", err);
        }
    }
}

fn default_ui_language() -> String {
    for key in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(value) = std::env::var(key) {
            let lower = value.to_lowercase();
            if lower.starts_with("it") {
                return "it".to_string();
            }
            if !lower.trim().is_empty() {
                return "en".to_string();
            }
        }
    }

    #[cfg(target_os = "macos")]
    if let Some(locale) = macos_system_locale() {
        let lower = locale.to_lowercase();
        if lower.starts_with("it") {
            return "it".to_string();
        }
        if !lower.trim().is_empty() {
            return "en".to_string();
        }
    }

    "en".to_string()
}

fn default_audiobook_format() -> String {
    "mp3".to_string()
}

fn default_text_save_format() -> String {
    "txt".to_string()
}

#[cfg(target_os = "macos")]
fn macos_system_locale() -> Option<String> {
    let output = std::process::Command::new("/usr/bin/defaults")
        .args(["read", "-g", "AppleLocale"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let locale = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if locale.is_empty() {
        None
    } else {
        Some(locale)
    }
}

fn normalize_ui_language(value: &str) -> String {
    if value.eq_ignore_ascii_case("en") || value.eq_ignore_ascii_case("english") {
        "en".to_string()
    } else {
        "it".to_string()
    }
}

fn system_language_code() -> String {
    for key in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(value) = std::env::var(key)
            && let Some(code) = parse_language_code(&value)
        {
            return code;
        }
    }

    #[cfg(target_os = "macos")]
    if let Some(locale) = macos_system_locale()
        && let Some(code) = parse_language_code(&locale)
    {
        return code;
    }

    Settings::load().ui_language
}

fn parse_language_code(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    let code = trimmed
        .split(['-', '_', '.', '@'])
        .next()
        .unwrap_or("")
        .trim()
        .to_lowercase();
    if code.is_empty() { None } else { Some(code) }
}

fn radio_menu_languages() -> Vec<(String, String)> {
    let mut items = vec![
        ("it".to_string(), get_language_name("it")),
        ("en".to_string(), get_language_name("en")),
        (
            "country:de".to_string(),
            radio_menu_entry_label("country:de"),
        ),
        (
            "country:ch".to_string(),
            radio_menu_entry_label("country:ch"),
        ),
        ("es".to_string(), get_language_name("es")),
        ("pt".to_string(), get_language_name("pt")),
        ("sv".to_string(), get_language_name("sv")),
        ("vi".to_string(), get_language_name("vi")),
        ("cs".to_string(), get_language_name("cs")),
        ("pl".to_string(), get_language_name("pl")),
        ("fr".to_string(), get_language_name("fr")),
        ("sr".to_string(), get_language_name("sr")),
        ("uk".to_string(), get_language_name("uk")),
        ("lt".to_string(), get_language_name("lt")),
        ("ru".to_string(), get_language_name("ru")),
        ("zh".to_string(), get_language_name("zh")),
    ];

    let preferred = system_language_code();
    if let Some(index) = items.iter().position(|(code, _)| *code == preferred) {
        let item = items.remove(index);
        items.insert(0, item);
    } else {
        items.insert(0, (preferred.clone(), get_language_name(&preferred)));
    }

    items
}

fn radio_menu_entry_label(code: &str) -> String {
    match code {
        "country:de" => {
            if Settings::load().ui_language == "it" {
                "Germania".to_string()
            } else {
                "Germany".to_string()
            }
        }
        "country:ch" => {
            if Settings::load().ui_language == "it" {
                "Svizzera".to_string()
            } else {
                "Switzerland".to_string()
            }
        }
        _ => get_language_name(code),
    }
}

#[derive(Deserialize)]
struct UiStrings {
    settings_title: String,
    about_title: String,
    donations_title: String,
    interface_language_label: String,
    voice_language_label: String,
    voice_label: String,
    rate_label: String,
    pitch_label: String,
    volume_label: String,
    #[cfg(target_os = "macos")]
    file_associations_label: String,
    #[cfg(target_os = "macos")]
    file_associations_button: String,
    #[cfg(target_os = "macos")]
    file_associations_success_message: String,
    #[cfg(target_os = "macos")]
    file_associations_error_message: String,
    ok: String,
    button_start_reading: String,
    button_play_podcast: String,
    button_pause_reading: String,
    button_resume_reading: String,
    button_pause_podcast: String,
    button_resume_podcast: String,
    button_stop_reading: String,
    button_stop_podcast: String,
    button_save_audiobook: String,
    button_settings: String,
    button_back_30: String,
    button_forward_30: String,
    menu_file: String,
    #[cfg(target_os = "macos")]
    menu_edit: String,
    menu_articles: String,
    menu_podcasts: String,
    menu_radio: String,
    menu_help: String,
    menu_open: String,
    menu_open_help: String,
    menu_save_text: String,
    menu_save_text_help: String,
    menu_save_text_as: String,
    menu_save_text_as_help: String,
    #[cfg(target_os = "macos")]
    menu_start: String,
    #[cfg(target_os = "macos")]
    menu_start_help: String,
    #[cfg(target_os = "macos")]
    menu_play_pause: String,
    #[cfg(target_os = "macos")]
    menu_play_pause_help: String,
    #[cfg(target_os = "macos")]
    menu_stop: String,
    #[cfg(target_os = "macos")]
    menu_stop_help: String,
    #[cfg(target_os = "macos")]
    menu_save: String,
    #[cfg(target_os = "macos")]
    menu_save_help: String,
    #[cfg(target_os = "macos")]
    menu_settings: String,
    #[cfg(target_os = "macos")]
    menu_settings_help: String,
    #[cfg(target_os = "macos")]
    menu_undo: String,
    #[cfg(target_os = "macos")]
    menu_undo_help: String,
    #[cfg(target_os = "macos")]
    menu_redo: String,
    #[cfg(target_os = "macos")]
    menu_redo_help: String,
    #[cfg(target_os = "macos")]
    menu_cut: String,
    #[cfg(target_os = "macos")]
    menu_cut_help: String,
    #[cfg(target_os = "macos")]
    menu_copy: String,
    #[cfg(target_os = "macos")]
    menu_copy_help: String,
    #[cfg(target_os = "macos")]
    menu_paste: String,
    #[cfg(target_os = "macos")]
    menu_paste_help: String,
    #[cfg(target_os = "macos")]
    menu_select_all: String,
    #[cfg(target_os = "macos")]
    menu_select_all_help: String,
    menu_exit: String,
    menu_exit_help: String,
    menu_about: String,
    menu_about_help: String,
    menu_donations: String,
    menu_donations_help: String,
    menu_changelog: String,
    menu_changelog_help: String,
    menu_updates: String,
    menu_updates_help: String,
    updates_title: String,
    changelog_title: String,
    podcast_error_title: String,
    yes: String,
    add_source: String,
    edit_source: String,
    delete_source: String,
    reorder_sources: String,
    import_sources: String,
    export_sources: String,
    add_podcast: String,
    delete_podcast: String,
    reorder_podcasts: String,
    keyword: String,
    podcast_label: String,
    source_label: String,
    folder_label: String,
    root_folder_name: String,
    open_folder: String,
    parent_folder: String,
    new_folder: String,
    move_to_folder: String,
    move_out_of_folders: String,
    folder_name_label: String,
    folder_empty: String,
    no_folders_available: String,
    title_label: String,
    url_or_source_label: String,
    move_up: String,
    move_down: String,
    confirm_delete_title: String,
    confirm_delete_rss_message: String,
    confirm_delete_podcast_message: String,
    sorted_articles_title: String,
    sorted_articles_message: String,
    imported_articles_title: String,
    imported_articles_message: String,
    exported_articles_title: String,
    exported_articles_message: String,
    import_articles_error_title: String,
    export_articles_error_title: String,
    sorted_podcasts_title: String,
    sorted_podcasts_message: String,
    loading_articles: String,
    no_articles_available: String,
    wait_loading_articles: String,
    refresh_source_for_articles: String,
    loading_podcasts: String,
    wait_loading_category_podcasts: String,
    no_podcasts_available: String,
    no_podcasts_for_category: String,
    add_this_podcast: String,
    loading_episodes: String,
    no_episodes_available: String,
    wait_loading_episodes: String,
    refresh_podcast_for_episodes: String,
    no_radios_available: String,
    radio_open_failed: String,
    save_podcast_episode: String,
    podcast_loading_title: String,
    podcast_ready: String,
    podcast_download_title: String,
    podcast_download_start: String,
    save_audiobook_title: String,
    create_audiobook_title: String,
    initializing: String,
    cancel: String,
    audiobook_conversion_failed: String,
    audiobook_file_not_saved: String,
    audiobook_conversion_error: String,
    conversion_error_title: String,
    audiobook_saved_ok: String,
    save_completed_title: String,
    cancelling_audiobook: String,
    audiobook_ffmpeg_missing: String,
    audiobook_m4b_conversion_failed: String,
    audiobook_m4a_conversion_failed: String,
    audiobook_wav_conversion_failed: String,
    podcast_downloaded_title: String,
    podcast_downloaded_message: String,
    open: String,
    save_as: String,
    save_filename_label: String,
    save_default_filename: String,
    save_format_label: String,
    save_folder_label: String,
    choose_folder: String,
    save_filename_empty: String,
    save_folder_not_selected: String,
    overwrite_existing_file: String,
    save_text_title: String,
    text_file_not_saved: String,
    text_saved_ok: String,
    unsaved_changes_message: String,
    unsaved_changes_title: String,
    close: String,
    open_document_title: String,
    analyzing_document: String,
    analyzing_pdf: String,
    document_loaded: String,
    about_message: String,
    add_radio: String,
    add_radio_title: String,
    edit_radio: String,
    edit_radio_title: String,
    reorder_radios: String,
    radio_favorites: String,
    delete_radio_favorite: String,
    radio_label: String,
    radio_url_label: String,
}

#[derive(Clone, Default)]
struct CurrentDocumentState {
    opened_path: Option<PathBuf>,
    direct_save_path: Option<PathBuf>,
}

fn parse_ui_strings(data: &str) -> UiStrings {
    serde_json::from_str(data).expect("invalid ui translation json")
}

fn ui_strings(ui_language: &str) -> &'static UiStrings {
    static UI_IT: OnceLock<UiStrings> = OnceLock::new();
    static UI_EN: OnceLock<UiStrings> = OnceLock::new();

    if normalize_ui_language(ui_language) == "en" {
        UI_EN.get_or_init(|| parse_ui_strings(include_str!("../i18n/ui_en.json")))
    } else {
        UI_IT.get_or_init(|| parse_ui_strings(include_str!("../i18n/ui_it.json")))
    }
}

fn current_ui_strings() -> &'static UiStrings {
    let ui_language = Settings::load().ui_language;
    ui_strings(&ui_language)
}

fn get_language_name(locale: &str) -> String {
    if Settings::load().ui_language == "en" {
        return get_language_name_en(locale);
    }

    get_language_name_it(locale)
}

fn get_language_name_en(locale: &str) -> String {
    let base = locale.split('-').next().unwrap_or(locale).to_lowercase();
    match base.as_str() {
        "af" => "Afrikaans".to_string(),
        "am" => "Amharic".to_string(),
        "ar" => "Arabic".to_string(),
        "az" => "Azerbaijani".to_string(),
        "bg" => "Bulgarian".to_string(),
        "bn" => "Bengali".to_string(),
        "bs" => "Bosnian".to_string(),
        "ca" => "Catalan".to_string(),
        "cs" => "Czech".to_string(),
        "cy" => "Welsh".to_string(),
        "da" => "Danish".to_string(),
        "it" => "Italian".to_string(),
        "en" => "English".to_string(),
        "fr" => "French".to_string(),
        "es" => "Spanish".to_string(),
        "de" => "German".to_string(),
        "el" => "Greek".to_string(),
        "et" => "Estonian".to_string(),
        "fa" => "Persian".to_string(),
        "fi" => "Finnish".to_string(),
        "ga" => "Irish".to_string(),
        "gu" => "Gujarati".to_string(),
        "he" => "Hebrew".to_string(),
        "hi" => "Hindi".to_string(),
        "hr" => "Croatian".to_string(),
        "hu" => "Hungarian".to_string(),
        "hy" => "Armenian".to_string(),
        "id" => "Indonesian".to_string(),
        "is" => "Icelandic".to_string(),
        "pt" => "Portuguese".to_string(),
        "kk" => "Kazakh".to_string(),
        "km" => "Khmer".to_string(),
        "kn" => "Kannada".to_string(),
        "ko" => "Korean".to_string(),
        "lo" => "Lao".to_string(),
        "lt" => "Lithuanian".to_string(),
        "lv" => "Latvian".to_string(),
        "mk" => "Macedonian".to_string(),
        "ml" => "Malayalam".to_string(),
        "mn" => "Mongolian".to_string(),
        "mr" => "Marathi".to_string(),
        "ms" => "Malay".to_string(),
        "mt" => "Maltese".to_string(),
        "my" => "Burmese".to_string(),
        "nb" | "no" => "Norwegian".to_string(),
        "ne" => "Nepali".to_string(),
        "nl" => "Dutch".to_string(),
        "pa" => "Punjabi".to_string(),
        "pl" => "Polish".to_string(),
        "ro" => "Romanian".to_string(),
        "ru" => "Russian".to_string(),
        "sk" => "Slovak".to_string(),
        "sl" => "Slovenian".to_string(),
        "sq" => "Albanian".to_string(),
        "sr" => "Serbian".to_string(),
        "sv" => "Swedish".to_string(),
        "sw" => "Swahili".to_string(),
        "ta" => "Tamil".to_string(),
        "te" => "Telugu".to_string(),
        "th" => "Thai".to_string(),
        "tr" => "Turkish".to_string(),
        "uk" => "Ukrainian".to_string(),
        "ur" => "Urdu".to_string(),
        "uz" => "Uzbek".to_string(),
        "vi" => "Vietnamese".to_string(),
        "zh" => "Chinese".to_string(),
        "ja" => "Japanese".to_string(),
        "zu" => "Zulu".to_string(),
        _ => locale.to_string(),
    }
}

fn get_language_name_it(locale: &str) -> String {
    let base = locale.split('-').next().unwrap_or(locale).to_lowercase();
    match base.as_str() {
        "af" => "Afrikaans".to_string(),
        "am" => "Amarico".to_string(),
        "ar" => "Arabo".to_string(),
        "az" => "Azero".to_string(),
        "bg" => "Bulgaro".to_string(),
        "bn" => "Bengalese".to_string(),
        "bs" => "Bosniaco".to_string(),
        "ca" => "Catalano".to_string(),
        "cs" => "Ceco".to_string(),
        "cy" => "Gallese".to_string(),
        "da" => "Danese".to_string(),
        "it" => "Italiano".to_string(),
        "en" => "Inglese".to_string(),
        "fr" => "Francese".to_string(),
        "es" => "Spagnolo".to_string(),
        "de" => "Tedesco".to_string(),
        "el" => "Greco".to_string(),
        "et" => "Estone".to_string(),
        "fa" => "Persiano".to_string(),
        "fi" => "Finlandese".to_string(),
        "ga" => "Irlandese".to_string(),
        "gu" => "Gujarati".to_string(),
        "he" => "Ebraico".to_string(),
        "hi" => "Hindi".to_string(),
        "hr" => "Croato".to_string(),
        "hu" => "Ungherese".to_string(),
        "hy" => "Armeno".to_string(),
        "id" => "Indonesiano".to_string(),
        "is" => "Islandese".to_string(),
        "pt" => "Portoghese".to_string(),
        "kk" => "Kazako".to_string(),
        "km" => "Khmer".to_string(),
        "kn" => "Kannada".to_string(),
        "ko" => "Coreano".to_string(),
        "lo" => "Lao".to_string(),
        "lt" => "Lituano".to_string(),
        "lv" => "Lettone".to_string(),
        "mk" => "Macedone".to_string(),
        "ml" => "Malayalam".to_string(),
        "mn" => "Mongolo".to_string(),
        "mr" => "Marathi".to_string(),
        "ms" => "Malese".to_string(),
        "mt" => "Maltese".to_string(),
        "my" => "Birmano".to_string(),
        "nb" | "no" => "Norvegese".to_string(),
        "ne" => "Nepalese".to_string(),
        "nl" => "Olandese".to_string(),
        "pa" => "Punjabi".to_string(),
        "pl" => "Polacco".to_string(),
        "ro" => "Rumeno".to_string(),
        "ru" => "Russo".to_string(),
        "sk" => "Slovacco".to_string(),
        "sl" => "Sloveno".to_string(),
        "sq" => "Albanese".to_string(),
        "sr" => "Serbo".to_string(),
        "sv" => "Svedese".to_string(),
        "sw" => "Swahili".to_string(),
        "ta" => "Tamil".to_string(),
        "te" => "Telugu".to_string(),
        "th" => "Thailandese".to_string(),
        "tr" => "Turco".to_string(),
        "uk" => "Ucraino".to_string(),
        "ur" => "Urdu".to_string(),
        "uz" => "Uzbeco".to_string(),
        "vi" => "Vietnamita".to_string(),
        "zh" => "Cinese".to_string(),
        "ja" => "Giapponese".to_string(),
        "zu" => "Zulu".to_string(),
        _ => locale.to_string(),
    }
}

const RATE_PRESETS: [(&str, i32); 7] = [
    ("Molto lenta", -60),
    ("Lenta", -30),
    ("Meno veloce", -15),
    ("Normale", 0),
    ("Veloce", 15),
    ("Più veloce", 30),
    ("Molto veloce", 60),
];

const PITCH_PRESETS: [(&str, i32); 5] = [
    ("Molto basso", -40),
    ("Basso", -20),
    ("Normale", 0),
    ("Alto", 20),
    ("Molto alto", 40),
];

const VOLUME_PRESETS: [(&str, i32); 5] = [
    ("Molto basso", 40),
    ("Basso", 70),
    ("Normale", 100),
    ("Alto", 140),
    ("Molto alto", 180),
];

fn nearest_preset_index(presets: &[(&str, i32)], value: i32) -> usize {
    presets
        .iter()
        .enumerate()
        .min_by_key(|(_, (_, v))| (*v - value).abs())
        .map(|(idx, _)| idx)
        .unwrap_or(0)
}

fn start_button_label(podcast_mode: bool) -> String {
    let ui = current_ui_strings();
    let shortcut = format!("{}+L", MOD_CMD);

    if podcast_mode {
        format!("{} ({shortcut})", ui.button_play_podcast)
    } else {
        format!("{} ({shortcut})", ui.button_start_reading)
    }
}

fn play_button_label(status: PlaybackStatus, podcast_mode: bool) -> String {
    let ui = current_ui_strings();
    let shortcut = format!("{}+P", MOD_CMD);

    if podcast_mode {
        match status {
            PlaybackStatus::Playing => format!("{} ({shortcut})", ui.button_pause_podcast),
            PlaybackStatus::Stopped | PlaybackStatus::Paused => {
                format!("{} ({shortcut})", ui.button_resume_podcast)
            }
        }
    } else {
        match status {
            PlaybackStatus::Playing => format!("{} ({shortcut})", ui.button_pause_reading),
            PlaybackStatus::Stopped | PlaybackStatus::Paused => {
                format!("{} ({shortcut})", ui.button_resume_reading)
            }
        }
    }
}

fn save_button_label() -> String {
    let ui = current_ui_strings();
    format!("{} ({}+{}+A)", ui.button_save_audiobook, MOD_CMD, MOD_ALT)
}

fn stop_button_label(podcast_mode: bool) -> String {
    let ui = current_ui_strings();
    if podcast_mode {
        format!("{} ({}+.)", ui.button_stop_podcast, MOD_CMD)
    } else {
        format!("{} ({}+.)", ui.button_stop_reading, MOD_CMD)
    }
}

fn settings_button_label() -> String {
    let ui = current_ui_strings();
    format!("{} ({}+,)", ui.button_settings, MOD_CMD)
}

#[cfg(target_os = "macos")]
fn settings_menu_label(label: &str) -> String {
    label.to_string()
}

fn update_menu_item_label(menubar: &MenuBar, id: i32, label: &str) {
    if let Some(item) = menubar.find_item(id) {
        item.set_label(label);
    }
}

type MainMenuStates<'a> = (
    &'a Arc<Mutex<ArticleMenuState>>,
    &'a Arc<Mutex<PodcastMenuState>>,
    &'a Arc<Mutex<RadioMenuState>>,
);

fn refresh_localized_main_ui(
    frame: &Frame,
    settings: &Arc<Mutex<Settings>>,
    menus: (&Menu, &Menu, &Menu),
    menu_states: MainMenuStates<'_>,
    buttons: (&Button, &Button, &Button, &Button),
) {
    let ui_language = settings.lock().unwrap().ui_language.clone();
    let ui = ui_strings(&ui_language);
    let (articles_menu, podcasts_menu, radio_menu) = menus;
    let (article_menu_state, podcast_menu_state, radio_menu_state) = menu_states;
    let (btn_save, btn_settings, btn_podcast_back, btn_podcast_forward) = buttons;

    if let Some(menubar) = frame.get_menu_bar() {
        #[cfg(target_os = "macos")]
        if menubar.get_menu_count() >= 5 {
            menubar.set_menu_label(0, &ui.menu_file);
            menubar.set_menu_label(1, &ui.menu_articles);
            menubar.set_menu_label(2, &ui.menu_podcasts);
            menubar.set_menu_label(3, &ui.menu_radio);
            menubar.set_menu_label(4, &ui.menu_help);
        }
        #[cfg(not(target_os = "macos"))]
        if menubar.get_menu_count() >= 5 {
            menubar.set_menu_label(0, &ui.menu_file);
            menubar.set_menu_label(1, &ui.menu_articles);
            menubar.set_menu_label(2, &ui.menu_podcasts);
            menubar.set_menu_label(3, &ui.menu_radio);
            menubar.set_menu_label(4, &ui.menu_help);
        }

        update_menu_item_label(&menubar, ID_OPEN, &ui.menu_open);
        update_menu_item_label(&menubar, ID_SAVE_TEXT, &ui.menu_save_text);
        update_menu_item_label(&menubar, ID_SAVE_TEXT_AS, &ui.menu_save_text_as);
        #[cfg(not(target_os = "macos"))]
        update_menu_item_label(&menubar, ID_EXIT, &ui.menu_exit);
        #[cfg(not(target_os = "macos"))]
        update_menu_item_label(&menubar, ID_ABOUT, &ui.menu_about);
        update_menu_item_label(&menubar, ID_DONATIONS, &ui.menu_donations);
        update_menu_item_label(&menubar, ID_CHANGELOG, &ui.menu_changelog);
        update_menu_item_label(&menubar, ID_CHECK_UPDATES, &ui.menu_updates);

        #[cfg(target_os = "macos")]
        {
            update_menu_item_label(&menubar, ID_START_PLAYBACK, &ui.menu_start);
            update_menu_item_label(&menubar, ID_PLAY_PAUSE, &ui.menu_play_pause);
            update_menu_item_label(&menubar, ID_STOP, &ui.menu_stop);
            update_menu_item_label(&menubar, ID_SAVE, &ui.menu_save);
            update_menu_item_label(
                &menubar,
                ID_SETTINGS,
                &settings_menu_label(&ui.menu_settings),
            );
        }
    }

    let article_loading_urls = article_menu_state.lock().unwrap().loading_urls.clone();
    rebuild_articles_menu(articles_menu, settings, &article_loading_urls);

    let (podcast_loading_urls, category_results, category_loading) = {
        let state = podcast_menu_state.lock().unwrap();
        (
            state.loading_urls.clone(),
            state.category_results.clone(),
            state.category_loading.clone(),
        )
    };
    rebuild_podcasts_menu(
        podcasts_menu,
        settings,
        &podcast_loading_urls,
        &category_results,
        &category_loading,
    );
    rebuild_radio_menu(radio_menu, settings, radio_menu_state);

    btn_save.set_label(&save_button_label());
    btn_settings.set_label(&settings_button_label());
    btn_podcast_back.set_label(&format!("{} ({}+Left)", ui.button_back_30, MOD_CMD));
    btn_podcast_forward.set_label(&format!("{} ({}+Right)", ui.button_forward_30, MOD_CMD));
    frame.layout();
}

#[cfg(target_os = "macos")]
fn command_shortcut_down(key_event: &KeyboardEvent) -> bool {
    key_event.cmd_down()
}

#[cfg(not(target_os = "macos"))]
fn command_shortcut_down(key_event: &KeyboardEvent) -> bool {
    key_event.cmd_down()
}

fn handle_shortcut_event(
    event: WindowEventData,
    actions: &ShortcutActions,
    podcast_seek_back: &Rc<RefCell<PodcastPlaybackState>>,
    podcast_seek_forward: &Rc<RefCell<PodcastPlaybackState>>,
) {
    if let WindowEventData::Keyboard(key_event) = &event {
        #[cfg(target_os = "macos")]
        {
            if mac_native_file_dialog_open() {
                event.skip(true);
                return;
            }

            let key_code = key_event.get_key_code().unwrap_or_default();
            let unicode_key = key_event.get_unicode_key().unwrap_or_default();

            let is_standard_edit_shortcut = command_shortcut_down(key_event)
                && !key_event.alt_down()
                && ((matches_ascii_key(key_code, unicode_key, 'c')
                    || matches_ascii_key(key_code, unicode_key, 'v')
                    || matches_ascii_key(key_code, unicode_key, 'x')
                    || matches_ascii_key(key_code, unicode_key, 'a')
                    || matches_ascii_key(key_code, unicode_key, 'z'))
                    || (key_event.shift_down() && matches_ascii_key(key_code, unicode_key, 'z')));
            if is_standard_edit_shortcut {
                event.skip(true);
                return;
            }

            if command_shortcut_down(key_event) && key_code == WXK_MAC_CMD_PERIOD_SUFFIX {
                append_podcast_log("mac_shortcut.trigger stop_suffix");
                (actions.stop)();
                return;
            }

            if command_shortcut_down(key_event) && !key_event.alt_down() && !key_event.shift_down()
            {
                match key_code {
                    _ if matches_ascii_key(key_code, unicode_key, 'l') => {
                        append_podcast_log("mac_shortcut.trigger start");
                        (actions.start)();
                        return;
                    }
                    _ if matches_ascii_key(key_code, unicode_key, 'p') => {
                        append_podcast_log("mac_shortcut.trigger play_pause");
                        (actions.play_pause)();
                        return;
                    }
                    WXK_LEFT => {
                        if podcast_seek_back.borrow().selected_episode.is_some() {
                            append_podcast_log("mac_shortcut.trigger seek_back");
                            seek_podcast_playback(podcast_seek_back, -PODCAST_SEEK_SECONDS);
                        }
                        return;
                    }
                    WXK_RIGHT => {
                        if podcast_seek_forward.borrow().selected_episode.is_some() {
                            append_podcast_log("mac_shortcut.trigger seek_forward");
                            seek_podcast_playback(podcast_seek_forward, PODCAST_SEEK_SECONDS);
                        }
                        return;
                    }
                    _ if matches_ascii_key(key_code, unicode_key, '.') => {
                        append_podcast_log("mac_shortcut.trigger stop");
                        (actions.stop)();
                        return;
                    }
                    _ if matches_settings_shortcut(key_code, unicode_key) => {
                        append_podcast_log(&format!(
                            "mac_shortcut.trigger settings key_code={key_code} unicode_key={unicode_key} cmd_down={} meta_down={}",
                            key_event.cmd_down(),
                            key_event.meta_down()
                        ));
                        (actions.settings)();
                        return;
                    }
                    _ => {}
                }
            } else if command_shortcut_down(key_event)
                && key_event.alt_down()
                && !key_event.shift_down()
                && matches_ascii_key(key_code, unicode_key, 'a')
            {
                append_podcast_log("mac_shortcut.trigger save");
                (actions.save)();
                return;
            }
            event.skip(true);
            return;
        }

        #[cfg(not(target_os = "macos"))]
        let key_code = key_event.get_key_code().unwrap_or_default();
        #[cfg(not(target_os = "macos"))]
        let unicode_key = key_event.get_unicode_key().unwrap_or_default();
        #[cfg(not(target_os = "macos"))]
        if command_shortcut_down(key_event) && !key_event.alt_down() && !key_event.shift_down() {
            match key_code {
                76 | 108 => (actions.start)(),
                80 | 112 => (actions.play_pause)(),
                WXK_LEFT => {
                    if podcast_seek_back.borrow().selected_episode.is_some() {
                        seek_podcast_playback(podcast_seek_back, -PODCAST_SEEK_SECONDS);
                    }
                }
                WXK_RIGHT => {
                    if podcast_seek_forward.borrow().selected_episode.is_some() {
                        seek_podcast_playback(podcast_seek_forward, PODCAST_SEEK_SECONDS);
                    }
                }
                _ if unicode_key == 46 => (actions.stop)(),
                _ if unicode_key == 44 => (actions.settings)(),
                _ => {}
            }
        } else if command_shortcut_down(key_event)
            && key_event.alt_down()
            && !key_event.shift_down()
        {
            match key_code {
                65 | 97 => (actions.save)(),
                _ => {}
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn matches_ascii_key(key_code: i32, unicode_key: i32, expected: char) -> bool {
    let expected_upper = expected.to_ascii_uppercase() as i32;
    let expected_lower = expected.to_ascii_lowercase() as i32;

    matches!(key_code, code if code == expected_upper || code == expected_lower)
        || matches!(
            unicode_key,
            code if code == expected_upper || code == expected_lower
        )
}

#[cfg(target_os = "macos")]
fn matches_settings_shortcut(key_code: i32, unicode_key: i32) -> bool {
    matches_ascii_key(key_code, unicode_key, ',')
        || matches!(key_code, 44 | 59 | 188)
        || matches!(unicode_key, 44 | 59)
}

#[cfg(target_os = "macos")]
fn mac_native_file_dialog_open() -> bool {
    MAC_NATIVE_FILE_DIALOG_OPEN.load(Ordering::Relaxed)
}

#[cfg(target_os = "macos")]
fn set_mac_native_file_dialog_open(open: bool) {
    MAC_NATIVE_FILE_DIALOG_OPEN.store(open, Ordering::Relaxed);
}

#[cfg(target_os = "macos")]
fn mac_menu_bar_active() -> bool {
    MAC_MENU_BAR_ACTIVE.load(Ordering::Relaxed)
}

#[cfg(target_os = "macos")]
fn set_mac_menu_bar_active(active: bool) {
    MAC_MENU_BAR_ACTIVE.store(active, Ordering::Relaxed);
}

#[cfg(target_os = "macos")]
fn mac_should_defer_menu_rebuilds() -> bool {
    mac_native_file_dialog_open() || mac_menu_bar_active()
}

fn about_title() -> &'static str {
    &current_ui_strings().about_title
}

fn about_message() -> String {
    current_ui_strings()
        .about_message
        .replace("{version}", env!("CARGO_PKG_VERSION"))
}

fn changelog_message() -> String {
    if Settings::load().ui_language == "it" {
        format!(
            "Sonarpad Per Mac {}\n\n\
Versione 0.2.6\n\
- Corretto un bug di wx/macOS per cui all'avvio poteva comparire un errore e sono stati stabilizzati i menu collegati.\n\
- Corretta la scorciatoia Cmd+, per il menu Impostazioni anche quando il focus si trova su editor e controlli.\n\
- Quando si salva un audiolibro il focus viene ora posizionato correttamente sul campo di testo e i nomi file con il punto non vengono piu tagliati.\n\
- Aggiunto supporto agli OPML di Lire con divisione in cartelle: le cartelle si aprono come sottomenu e le singole fonti in una finestrella dedicata.\n\
- Il riordino delle fonti articoli gestisce ora il nuovo sistema di cartelle con pulsanti Apri cartella, Cartella principale, Sposta in cartella e Sposta fuori dalle cartelle.\n\n\
Versione 0.2.5\n\
- Nuove finestre di salvataggio personalizzate per testo e audiolibri su macOS.\n\
- I campi nome file ora accettano correttamente Cmd+V, Cmd+A e gli altri comandi di editing.\n\
- Il programma ricorda ultima cartella e ultimo formato usati per salvataggio testo e audiolibri.\n\
- Aggiunto il salvataggio audiolibri anche in formato M4A e WAV.\n\
- Aggiunto il menu Radio con ricerca per lingua, aggiunta ai preferiti, aggiunta manuale di una stazione e modifica e riordino dei preferiti.\n\
- Migliorata la gestione delle fonti articoli inserite come siti: scoperta del feed dalla pagina e correzione del feed commenti.\n\
- Workflow release macOS aggiornato per includere anche l'artifact Catalina.\n\n\
Versione 0.2.4\n\
- Miglioramenti importanti all'OCR PDF su macOS con passaggio a pdfium e fallback piu robusti.\n\
- Aggiunto export M4B su macOS e affinato il salvataggio testo.\n\
- Migliorata la gestione delle fonti articoli e la protezione del refresh quando una fonte restituisce zero elementi.\n\
- Ottimizzata la sintesi Edge TTS con chunking e retry piu affidabili.\n\
- Aggiunta e poi raffinata la pipeline Catalina per build e packaging macOS.\n\n\
Versione 0.2.2\n\
- Migliorato il caricamento dei PDF su macOS con feedback piu chiaro e dialogo finale esplicito.\n\
- Ordinamento alfabetico delle fonti articoli.\n\
- Riparazioni al testo PDF e miglioramenti generali di localizzazione.\n\n\
Versione 0.2.1\n\
- Stabilizzati shortcut e menu macOS per avvio, pausa, stop e salvataggio.\n\
- Migliorata l'apertura esterna degli episodi podcast su macOS.\n\
- Corretta la persistenza delle impostazioni macOS.\n\
- Rafforzate le workflow di build Intel/macOS e la gestione di Xcode.\n\n\
Versione 0.2.0\n\
- Prima release macOS di Sonarpad Per Mac.\n\
- Supporto lettura testo, articoli e podcast con sintesi vocale.\n\
- Supporto PDF OCR su macOS, download aggiornamenti e pacchetti DMG dedicati.\n\
- Categorie podcast gerarchiche e primi shortcut globali/macOS.",
            env!("CARGO_PKG_VERSION")
        )
    } else {
        format!(
            "Sonarpad Per Mac {}\n\n\
Version 0.2.6\n\
- Fixed a wx/macOS bug that could show an error at startup and stabilized the related menus.\n\
- Fixed the Cmd+, shortcut for the Settings menu even when focus is on the editor or other controls.\n\
- When saving an audiobook, focus is now placed correctly on the text field and filenames containing a dot are no longer truncated.\n\
- Added support for Lire OPML files with folder grouping: folders open as submenus and individual sources open in a dedicated dialog.\n\
- Article source reordering now supports the new folder organization system with Open Folder, Root Folder, Move to Folder, and Move Out of Folders controls.\n\n\
Version 0.2.5\n\
- New custom save dialogs for text and audiobooks on macOS.\n\
- Filename fields now correctly accept Cmd+V, Cmd+A, and standard editing shortcuts.\n\
- The app now remembers the last folder and format used for text and audiobook saves.\n\
- Added audiobook saving in M4A and WAV format.\n\
- Added the Radio menu with language-based search, add to favorites, manual station entry, and favorite editing and reordering.\n\
- Improved article sources added as websites: feed discovery from the page and comments-feed fix.\n\
- macOS release workflow updated to include the Catalina artifact as well.\n\n\
Version 0.2.4\n\
- Major macOS PDF OCR improvements with the move to pdfium and stronger fallbacks.\n\
- Added M4B export on macOS and improved text saving.\n\
- Improved article source handling and protected refresh when a source returns zero items.\n\
- Improved Edge TTS chunking and retry behavior.\n\
- Added and refined the Catalina build and packaging pipeline.\n\n\
Version 0.2.2\n\
- Improved macOS PDF loading with clearer feedback and an explicit completion dialog.\n\
- Added alphabetical sorting for article sources.\n\
- Improved PDF text repair and localization.\n\n\
Version 0.2.1\n\
- Stabilized macOS shortcuts and menu actions for start, pause, stop, and save.\n\
- Improved external podcast episode opening on macOS.\n\
- Fixed macOS settings persistence.\n\
- Hardened Intel/macOS build workflows and Xcode selection.\n\n\
Version 0.2.0\n\
- First macOS release of Sonarpad Per Mac.\n\
- Text reading, articles, and podcast support with speech synthesis.\n\
- macOS PDF OCR support, update downloads, and dedicated DMG packages.\n\
- Hierarchical podcast categories and the first macOS shortcut work.",
            env!("CARGO_PKG_VERSION")
        )
    }
}

fn donations_title() -> &'static str {
    &current_ui_strings().donations_title
}

fn donations_message() -> &'static str {
    if Settings::load().ui_language == "it" {
        include_str!("../donations_it.txt")
    } else {
        include_str!("../donations_en.txt")
    }
}

fn open_donations_dialog(parent: &Frame) {
    let dialog = Dialog::builder(parent, donations_title())
        .with_style(DialogStyle::DefaultDialogStyle | DialogStyle::ResizeBorder)
        .with_size(640, 520)
        .build();
    let panel = Panel::builder(&dialog).build();
    let root = BoxSizer::builder(Orientation::Vertical).build();

    let text = TextCtrl::builder(&panel)
        .with_style(TextCtrlStyle::MultiLine | TextCtrlStyle::ReadOnly)
        .build();
    text.set_value(donations_message());
    root.add(&text, 1, SizerFlag::Expand | SizerFlag::All, 8);

    let button_row = BoxSizer::builder(Orientation::Horizontal).build();
    let btn_ok = Button::builder(&panel)
        .with_id(ID_OK)
        .with_label(&current_ui_strings().ok)
        .build();
    button_row.add_spacer(1);
    button_row.add(&btn_ok, 0, SizerFlag::All, 10);
    root.add_sizer(&button_row, 0, SizerFlag::Expand, 0);

    panel.set_sizer(root, true);
    dialog.set_affirmative_id(ID_OK);
    let dialog_ok = dialog;
    btn_ok.on_click(move |_| {
        dialog_ok.end_modal(ID_OK);
    });
    dialog.show_modal();
    dialog.destroy();
}

fn open_changelog_dialog(parent: &Frame) {
    let ui = current_ui_strings();
    let dialog = Dialog::builder(parent, &ui.changelog_title)
        .with_style(DialogStyle::DefaultDialogStyle | DialogStyle::ResizeBorder)
        .with_size(720, 560)
        .build();
    let panel = Panel::builder(&dialog).build();
    let root = BoxSizer::builder(Orientation::Vertical).build();

    let text = TextCtrl::builder(&panel)
        .with_style(TextCtrlStyle::MultiLine | TextCtrlStyle::ReadOnly)
        .build();
    text.set_value(&changelog_message());
    root.add(&text, 1, SizerFlag::Expand | SizerFlag::All, 8);

    let button_row = BoxSizer::builder(Orientation::Horizontal).build();
    let btn_ok = Button::builder(&panel)
        .with_id(ID_OK)
        .with_label(&current_ui_strings().ok)
        .build();
    button_row.add_spacer(1);
    button_row.add(&btn_ok, 0, SizerFlag::All, 10);
    root.add_sizer(&button_row, 0, SizerFlag::Expand, 0);

    panel.set_sizer(root, true);
    dialog.set_affirmative_id(ID_OK);
    let dialog_ok = dialog;
    btn_ok.on_click(move |_| {
        dialog_ok.end_modal(ID_OK);
    });
    dialog.show_modal();
    dialog.destroy();
}

fn show_modeless_message_dialog(parent: &Frame, title: &str, message: &str) {
    let dialog = Dialog::builder(parent, title)
        .with_style(DialogStyle::Caption | DialogStyle::SystemMenu | DialogStyle::CloseBox)
        .with_size(420, 180)
        .build();
    let panel = Panel::builder(&dialog).build();
    let root = BoxSizer::builder(Orientation::Vertical).build();

    let text = StaticText::builder(&panel).with_label(message).build();
    root.add(&text, 1, SizerFlag::Expand | SizerFlag::All, 12);

    let button_row = BoxSizer::builder(Orientation::Horizontal).build();
    let btn_ok = Button::builder(&panel)
        .with_id(ID_OK)
        .with_label(&current_ui_strings().ok)
        .build();
    button_row.add_spacer(1);
    button_row.add(&btn_ok, 0, SizerFlag::All, 10);
    root.add_sizer(&button_row, 0, SizerFlag::Expand, 0);

    panel.set_sizer(root, true);
    dialog.set_escape_id(ID_OK);
    let dialog_ok = dialog;
    btn_ok.on_click(move |_| {
        dialog_ok.destroy();
    });
    dialog.show(true);
}

fn show_message_dialog(parent: &Frame, title: &str, message: &str) {
    let dialog = MessageDialog::builder(parent, message, title)
        .with_style(MessageDialogStyle::OK | MessageDialogStyle::IconInformation)
        .build();
    localize_standard_dialog_buttons(&dialog);
    dialog.show_modal();
}

fn show_message_subdialog(parent: &Dialog, title: &str, message: &str) {
    let dialog = MessageDialog::builder(parent, message, title)
        .with_style(MessageDialogStyle::OK | MessageDialogStyle::IconInformation)
        .build();
    localize_standard_dialog_buttons(&dialog);
    dialog.show_modal();
}

fn reorder_feedback_message(moved_label: &str, target_label: &str, moved_up: bool) -> String {
    if Settings::load().ui_language == "it" {
        if moved_up {
            format!("{moved_label} ora e' sopra {target_label}.")
        } else {
            format!("{moved_label} ora e' sotto {target_label}.")
        }
    } else if moved_up {
        format!("{moved_label} is now above {target_label}.")
    } else {
        format!("{moved_label} is now below {target_label}.")
    }
}

fn move_to_folder_feedback_message(source_label: &str, folder_label: &str) -> String {
    if Settings::load().ui_language == "it" {
        format!("{source_label} e' stata spostata nella cartella {folder_label}.")
    } else {
        format!("{source_label} was moved to the folder {folder_label}.")
    }
}

fn move_out_of_folders_feedback_message(source_label: &str, root_label: &str) -> String {
    if Settings::load().ui_language == "it" {
        format!("{source_label} e' stata spostata in {root_label}.")
    } else {
        format!("{source_label} was moved to {root_label}.")
    }
}

fn write_docx_text(path: &Path, text: &str) -> Result<(), String> {
    let file = std::fs::File::create(path)
        .map_err(|err| format!("salvataggio file {} fallito: {}", path.display(), err))?;
    let mut docx = Docx::new();
    for line in text.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        let paragraph = if line.is_empty() {
            Paragraph::new()
        } else {
            Paragraph::new().add_run(Run::new().add_text(line))
        };
        docx = docx.add_paragraph(paragraph);
    }
    docx.build()
        .pack(file)
        .map_err(|err| format!("salvataggio DOCX {} fallito: {}", path.display(), err))
}

fn estimate_max_chars(page_width: f32, margin: f32, font_size: f32) -> usize {
    let usable_mm = page_width - (margin * 2.0);
    let avg_char_mm = (font_size * 0.3528) * 0.5;
    let estimate = (usable_mm / avg_char_mm) as usize;
    estimate.clamp(60, 110)
}

fn wrap_words(text: &str, max_chars: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        if current.is_empty() {
            current.push_str(word);
        } else if current.len() + 1 + word.len() <= max_chars {
            current.push(' ');
            current.push_str(word);
        } else {
            lines.push(current);
            current = word.to_string();
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

fn write_pdf_text(path: &Path, title: &str, text: &str) -> Result<(), String> {
    let page_width = Mm(210.0);
    let page_height = Mm(297.0);
    let margin: f32 = 18.0;
    let header_height: f32 = 18.0;
    let body_font_size: f32 = 12.0;
    let header_font_size: f32 = 14.0;
    let line_height: f32 = 14.0;
    let max_chars = estimate_max_chars(page_width.0, margin, body_font_size);
    let title = if title.trim().is_empty() {
        "Sonarpad"
    } else {
        title
    };

    let mut lines = Vec::new();
    for raw_line in text.lines() {
        let line = raw_line.trim_end_matches('\r');
        if line.trim().is_empty() {
            lines.push(String::new());
        } else {
            lines.extend(wrap_words(line, max_chars));
        }
    }
    if lines.is_empty() {
        lines.push(String::new());
    }

    let content_top = page_height.0 - margin - header_height;
    let content_bottom = margin;
    let mut page_contents: Vec<Vec<String>> = Vec::new();
    let mut current_page = Vec::new();
    let mut y = content_top;
    for line in lines {
        if y < content_bottom + line_height {
            page_contents.push(current_page);
            current_page = Vec::new();
            y = content_top;
        }
        current_page.push(line);
        y -= line_height;
    }
    if !current_page.is_empty() {
        page_contents.push(current_page);
    }

    let mut pdf_pages = Vec::with_capacity(page_contents.len());
    for page_lines in page_contents {
        let mut ops = Vec::new();
        let header_y = page_height.0 - margin - 8.0;
        ops.push(Op::StartTextSection);
        ops.push(Op::SetTextCursor {
            pos: Point::new(Mm(margin), Mm(header_y)),
        });
        ops.push(Op::SetFontSizeBuiltinFont {
            size: Pt(header_font_size),
            font: BuiltinFont::HelveticaBold,
        });
        ops.push(Op::WriteTextBuiltinFont {
            items: vec![TextItem::Text(title.to_string())],
            font: BuiltinFont::HelveticaBold,
        });
        ops.push(Op::EndTextSection);

        let mut current_y = content_top;
        for line in page_lines {
            if line.is_empty() {
                current_y -= line_height;
                continue;
            }
            ops.push(Op::StartTextSection);
            ops.push(Op::SetTextCursor {
                pos: Point::new(Mm(margin), Mm(current_y)),
            });
            ops.push(Op::SetFontSizeBuiltinFont {
                size: Pt(body_font_size),
                font: BuiltinFont::Helvetica,
            });
            ops.push(Op::WriteTextBuiltinFont {
                items: vec![TextItem::Text(line)],
                font: BuiltinFont::Helvetica,
            });
            ops.push(Op::EndTextSection);
            current_y -= line_height;
        }

        pdf_pages.push(PdfPage::new(page_width, page_height, ops));
    }

    let mut doc = PdfDocument::new(title);
    let bytes = doc
        .with_pages(pdf_pages)
        .save(&PdfSaveOptions::default(), &mut Vec::new());
    std::fs::write(path, bytes)
        .map_err(|err| format!("salvataggio PDF {} fallito: {}", path.display(), err))
}

fn bundled_ffmpeg_dir() -> Option<PathBuf> {
    let exe_path = std::env::current_exe().ok()?;
    let contents_dir = exe_path.parent()?.parent()?;
    let bundle_dir = contents_dir.join("Resources").join("ffmpeg");
    if bundle_dir.is_dir() {
        Some(bundle_dir)
    } else {
        None
    }
}

fn ffmpeg_executable_path() -> Option<PathBuf> {
    if let Some(bundle_dir) = bundled_ffmpeg_dir() {
        let candidate = bundle_dir.join("bin").join(if cfg!(windows) {
            "ffmpeg.exe"
        } else {
            "ffmpeg"
        });
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

#[cfg(target_os = "macos")]
fn current_macos_app_bundle_path() -> Result<PathBuf, String> {
    let exe_path =
        std::env::current_exe().map_err(|err| format!("lettura percorso app fallita: {err}"))?;
    for ancestor in exe_path.ancestors() {
        if ancestor
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("app"))
        {
            return Ok(ancestor.to_path_buf());
        }
    }
    Err("bundle app macOS non trovato".to_string())
}

#[cfg(target_os = "macos")]
fn write_macos_file_associations_script() -> Result<PathBuf, String> {
    let script_path = std::env::temp_dir().join(format!(
        "sonarpad_minimal_file_assoc_{}.swift",
        Uuid::new_v4()
    ));
    let mut file = std::fs::File::create(&script_path)
        .map_err(|err| format!("creazione script associazioni file fallita: {err}"))?;
    file.write_all(MACOS_FILE_ASSOCIATIONS_SWIFT.as_bytes())
        .map_err(|err| format!("scrittura script associazioni file fallita: {err}"))?;
    Ok(script_path)
}

#[cfg(target_os = "macos")]
fn set_macos_default_file_associations() -> Result<(), String> {
    let bundle_path = current_macos_app_bundle_path()?;
    let script_path = write_macos_file_associations_script()?;
    append_podcast_log(&format!(
        "mac_file_assoc.begin bundle={} script={}",
        bundle_path.display(),
        script_path.display()
    ));
    let output = Command::new("xcrun")
        .arg("swift")
        .arg(&script_path)
        .arg(&bundle_path)
        .output()
        .map_err(|err| format!("avvio helper associazioni file fallito: {err}"))?;
    if let Err(err) = std::fs::remove_file(&script_path) {
        eprintln!(
            "cleanup script associazioni file fallita {}: {}",
            script_path.display(),
            err
        );
    }
    if output.status.success() {
        append_podcast_log("mac_file_assoc.success");
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        append_podcast_log(&format!(
            "mac_file_assoc.failed status={} stdout={} stderr={}",
            output.status, stdout, stderr
        ));
        if !stderr.is_empty() {
            Err(stderr)
        } else if !stdout.is_empty() {
            Err(stdout)
        } else {
            Err("registrazione associazioni file non riuscita".to_string())
        }
    }
}

#[cfg(target_os = "macos")]
const MACOS_FILE_ASSOCIATIONS_SWIFT: &str = r#"import CoreServices
import Foundation
import UniformTypeIdentifiers

guard CommandLine.arguments.count >= 2 else {
    fputs("missing app bundle path\n", stderr)
    exit(2)
}

let bundlePath = CommandLine.arguments[1]
let bundleUrl = URL(fileURLWithPath: bundlePath)
guard let bundle = Bundle(url: bundleUrl) else {
    fputs("unable to load app bundle\n", stderr)
    exit(3)
}
guard let bundleIdentifier = bundle.bundleIdentifier, !bundleIdentifier.isEmpty else {
    fputs("missing bundle identifier\n", stderr)
    exit(4)
}

let registerStatus = LSRegisterURL(bundleUrl as CFURL, true)
if registerStatus != noErr {
    fputs("bundle registration failed: \(registerStatus)\n", stderr)
    exit(5)
}

let extensions = ["txt", "doc", "docx", "pdf", "epub", "rtf", "html", "htm", "xls", "xlsx", "ods", "png", "jpg", "jpeg", "gif", "bmp", "tif", "tiff", "webp", "heic"]
var failures: [String] = []
let nonFatalPermissionExtensions: Set<String> = ["html", "htm"]

for fileExtension in extensions {
    guard let type = UTType(filenameExtension: fileExtension) else {
        failures.append("\(fileExtension): unknown type")
        continue
    }

    let roleMasks: [LSRolesMask] = fileExtension == "html" || fileExtension == "htm"
        ? [.viewer, .editor]
        : [.all, .viewer, .editor]
    var applied = false
    var lastStatus = noErr

    for roleMask in roleMasks {
        let status = LSSetDefaultRoleHandlerForContentType(
            type.identifier as CFString,
            roleMask,
            bundleIdentifier as CFString
        )
        if status == noErr {
            applied = true
            break
        }
        lastStatus = status
    }

    if !applied {
        if nonFatalPermissionExtensions.contains(fileExtension), lastStatus == -54 {
            continue
        }
        failures.append("\(fileExtension): \(lastStatus)")
    }
}

if failures.isEmpty {
    print("ok")
    exit(0)
}

fputs(failures.joined(separator: "\n") + "\n", stderr)
exit(1)
"#;

fn convert_mp3_to_m4b(
    source_mp3: &Path,
    output_m4b: &Path,
    bitrate_kbps: u32,
) -> Result<(), String> {
    let ffmpeg_path = ffmpeg_executable_path().unwrap_or_else(|| {
        PathBuf::from(if cfg!(windows) {
            "ffmpeg.exe"
        } else {
            "ffmpeg"
        })
    });
    let mut command = Command::new(&ffmpeg_path);
    command
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-y")
        .arg("-fflags")
        .arg("+genpts")
        .arg("-i")
        .arg(source_mp3)
        .arg("-vn")
        .arg("-c:a")
        .arg("aac")
        .arg("-b:a")
        .arg(format!("{bitrate_kbps}k"))
        .arg("-ar")
        .arg("48000")
        .arg("-movflags")
        .arg("+faststart")
        .arg("-f")
        .arg("ipod")
        .arg(output_m4b);

    let output = command.output().map_err(|err| {
        if err.kind() == std::io::ErrorKind::NotFound {
            "__FFMPEG_MISSING__".to_string()
        } else {
            format!("avvio FFmpeg fallito: {err}")
        }
    })?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if stderr.is_empty() {
            Err("FFmpeg ha restituito un errore durante la conversione M4B.".to_string())
        } else {
            Err(stderr)
        }
    }
}

fn move_article_source_within_visible_list(
    sources: &mut Vec<articles::ArticleSource>,
    visible_indices: &[usize],
    current_index: usize,
    move_up: bool,
) -> bool {
    if visible_indices.len() < 2 {
        return false;
    }

    let target_index = if move_up {
        let Some(target) = current_index.checked_sub(1) else {
            return false;
        };
        target
    } else {
        let target = current_index + 1;
        if target >= visible_indices.len() {
            return false;
        }
        target
    };

    let Some(global_current) = visible_indices.get(current_index).copied() else {
        return false;
    };
    let Some(global_target) = visible_indices.get(target_index).copied() else {
        return false;
    };
    let Some(moved_source) = sources.get(global_current).cloned() else {
        return false;
    };

    sources.remove(global_current);
    let insert_index = if global_current < global_target {
        global_target.saturating_sub(1)
    } else {
        global_target
    };
    sources.insert(insert_index, moved_source);
    true
}

fn convert_mp3_to_m4a(
    source_mp3: &Path,
    output_m4a: &Path,
    bitrate_kbps: u32,
) -> Result<(), String> {
    let ffmpeg_path = ffmpeg_executable_path().unwrap_or_else(|| {
        PathBuf::from(if cfg!(windows) {
            "ffmpeg.exe"
        } else {
            "ffmpeg"
        })
    });
    let mut command = Command::new(&ffmpeg_path);
    command
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-y")
        .arg("-fflags")
        .arg("+genpts")
        .arg("-i")
        .arg(source_mp3)
        .arg("-vn")
        .arg("-c:a")
        .arg("aac")
        .arg("-b:a")
        .arg(format!("{bitrate_kbps}k"))
        .arg("-ar")
        .arg("48000")
        .arg("-movflags")
        .arg("+faststart")
        .arg(output_m4a);

    let output = command.output().map_err(|err| {
        if err.kind() == std::io::ErrorKind::NotFound {
            "__FFMPEG_MISSING__".to_string()
        } else {
            format!("avvio FFmpeg fallito: {err}")
        }
    })?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if stderr.is_empty() {
            Err("FFmpeg ha restituito un errore durante la conversione M4A.".to_string())
        } else {
            Err(stderr)
        }
    }
}

fn convert_mp3_to_wav(source_mp3: &Path, output_wav: &Path) -> Result<(), String> {
    let input = std::fs::File::open(source_mp3)
        .map_err(|err| format!("apertura MP3 temporaneo fallita: {err}"))?;
    let source = Decoder::new(BufReader::new(input))
        .map_err(|err| format!("decodifica MP3 fallita: {err}"))?;
    let spec = hound::WavSpec {
        channels: source.channels(),
        sample_rate: source.sample_rate(),
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(output_wav, spec)
        .map_err(|err| format!("creazione WAV fallita: {err}"))?;
    for sample in source.convert_samples::<i16>() {
        writer
            .write_sample(sample)
            .map_err(|err| format!("scrittura WAV fallita: {err}"))?;
    }
    writer
        .finalize()
        .map_err(|err| format!("finalizzazione WAV fallita: {err}"))
}

fn prompt_text_save_path(
    parent: &Frame,
    settings: &Arc<Mutex<Settings>>,
    suggested_path: Option<&Path>,
    preferred_extension: Option<&str>,
    current_text: &str,
) -> Option<PathBuf> {
    let ui = current_ui_strings();
    let settings_snapshot = settings.lock().unwrap().clone();
    let default_filename = suggested_path
        .and_then(|path| path.file_stem())
        .and_then(|stem| stem.to_str())
        .and_then(sanitize_filename_candidate)
        .or_else(|| first_line_filename_candidate(current_text))
        .unwrap_or_else(|| sanitize_filename(&ui.save_default_filename));
    let dialog = Dialog::builder(parent, &ui.save_text_title)
        .with_style(DialogStyle::Caption | DialogStyle::SystemMenu | DialogStyle::CloseBox)
        .with_size(520, 250)
        .build();
    let panel = Panel::builder(&dialog).build();
    let root = BoxSizer::builder(Orientation::Vertical).build();

    let name_label = StaticText::builder(&panel)
        .with_label(&ui.save_filename_label)
        .build();
    root.add(
        &name_label,
        0,
        SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Top,
        12,
    );

    let name_ctrl = TextCtrl::builder(&panel).build();
    name_ctrl.set_value(&default_filename);
    root.add(
        &name_ctrl,
        0,
        SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Top,
        12,
    );

    let format_label = StaticText::builder(&panel)
        .with_label(&ui.save_format_label)
        .build();
    root.add(
        &format_label,
        0,
        SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Top,
        12,
    );

    let format_choice = Choice::builder(&panel).build();
    format_choice.append("TXT");
    format_choice.append("DOCX");
    format_choice.append("PDF");
    format_choice.set_selection(
        match preferred_extension
            .map(|ext| ext.to_ascii_lowercase())
            .unwrap_or_else(|| settings_snapshot.last_text_save_format.to_ascii_lowercase())
            .as_str()
        {
            "docx" => 1,
            "pdf" => 2,
            _ => 0,
        },
    );
    root.add(
        &format_choice,
        0,
        SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Top,
        12,
    );

    let folder_label = StaticText::builder(&panel)
        .with_label(&ui.save_folder_label)
        .build();
    root.add(
        &folder_label,
        0,
        SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Top,
        12,
    );

    let initial_folder = if let Some(parent_folder) = suggested_path.and_then(|path| path.parent())
    {
        parent_folder.to_path_buf()
    } else if settings_snapshot.last_text_save_dir.trim().is_empty() {
        default_audiobook_save_folder()
    } else {
        PathBuf::from(&settings_snapshot.last_text_save_dir)
    };
    let folder_display = StaticText::builder(&panel)
        .with_label(&initial_folder.to_string_lossy())
        .build();
    root.add(
        &folder_display,
        0,
        SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Top,
        12,
    );

    let choose_folder_button = Button::builder(&panel)
        .with_label(&ui.choose_folder)
        .build();
    root.add(
        &choose_folder_button,
        0,
        SizerFlag::Left | SizerFlag::Right | SizerFlag::Top,
        12,
    );

    let buttons = BoxSizer::builder(Orientation::Horizontal).build();
    let save_button = Button::builder(&panel)
        .with_id(ID_OK)
        .with_label(&ui.save_as)
        .build();
    let cancel_button = Button::builder(&panel)
        .with_id(ID_CANCEL)
        .with_label(&ui.close)
        .build();
    buttons.add_spacer(1);
    buttons.add(&save_button, 0, SizerFlag::All, 10);
    buttons.add(&cancel_button, 0, SizerFlag::All, 10);
    root.add_sizer(&buttons, 0, SizerFlag::Expand, 0);

    panel.set_sizer(root, true);
    dialog.set_affirmative_id(ID_OK);
    dialog.set_escape_id(ID_CANCEL);

    let selected_folder = Rc::new(RefCell::new(initial_folder));
    let selected_path = Rc::new(RefCell::new(None::<PathBuf>));

    let dialog_choose = dialog;
    let folder_display_choose = folder_display;
    let selected_folder_choose = Rc::clone(&selected_folder);
    choose_folder_button.on_click(move |_| {
        let ui = current_ui_strings();
        let default_path = selected_folder_choose
            .borrow()
            .to_string_lossy()
            .into_owned();
        let dir_dialog =
            DirDialog::builder(&dialog_choose, &ui.choose_folder, &default_path).build();

        #[cfg(target_os = "macos")]
        set_mac_native_file_dialog_open(true);
        let dialog_result = dir_dialog.show_modal();
        #[cfg(target_os = "macos")]
        set_mac_native_file_dialog_open(false);

        if dialog_result != ID_OK {
            return;
        }

        if let Some(path) = dir_dialog.get_path() {
            let folder = PathBuf::from(path);
            folder_display_choose.set_label(&folder.to_string_lossy());
            *selected_folder_choose.borrow_mut() = folder;
        }
    });

    let dialog_save = dialog;
    let name_ctrl_save = name_ctrl;
    let format_choice_save = format_choice;
    let selected_folder_save = Rc::clone(&selected_folder);
    let selected_path_save = Rc::clone(&selected_path);
    let settings_save = Arc::clone(settings);
    save_button.on_click(move |_| {
        let ui = current_ui_strings();
        let filename = sanitize_filename(&name_ctrl_save.get_value());
        if filename.is_empty() {
            show_message_subdialog(&dialog_save, &ui.save_text_title, &ui.save_filename_empty);
            return;
        }

        let folder = selected_folder_save.borrow().clone();
        if folder.as_os_str().is_empty() {
            show_message_subdialog(
                &dialog_save,
                &ui.save_text_title,
                &ui.save_folder_not_selected,
            );
            return;
        }

        let extension = match format_choice_save.get_selection() {
            Some(1) => "docx",
            Some(2) => "pdf",
            _ => "txt",
        };
        let path = folder.join(format!("{filename}.{extension}"));

        if path.exists() {
            let overwrite_dialog = MessageDialog::builder(
                &dialog_save,
                &ui.overwrite_existing_file,
                &ui.save_text_title,
            )
            .with_style(MessageDialogStyle::YesNo | MessageDialogStyle::IconWarning)
            .build();
            localize_standard_dialog_buttons(&overwrite_dialog);
            if overwrite_dialog.show_modal() != ID_YES {
                return;
            }
        }

        {
            let mut locked = settings_save.lock().unwrap();
            locked.last_text_save_dir = folder.to_string_lossy().into_owned();
            locked.last_text_save_format = extension.to_string();
            locked.save();
        }

        *selected_path_save.borrow_mut() = Some(path);
        dialog_save.end_modal(ID_OK);
    });

    let dialog_cancel = dialog;
    cancel_button.on_click(move |_| {
        dialog_cancel.end_modal(ID_CANCEL);
    });

    name_ctrl.set_focus();
    name_ctrl.set_selection(0, default_filename.chars().count() as i64);

    let result = if dialog.show_modal() == ID_OK {
        selected_path.borrow().clone()
    } else {
        None
    };
    dialog.destroy();
    result
}

fn save_text_to_path(path_buf: &Path, text: &str) -> Result<(), String> {
    match path_buf
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .as_deref()
    {
        Some("docx") => write_docx_text(path_buf, text),
        Some("pdf") => {
            let title = path_buf
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("Sonarpad");
            write_pdf_text(path_buf, title, text)
        }
        _ => std::fs::write(path_buf, text)
            .map_err(|err| format!("salvataggio file {} fallito: {}", path_buf.display(), err)),
    }
}

fn is_plain_text_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("txt"))
}

fn set_current_document_state(state: &Arc<Mutex<CurrentDocumentState>>, path: Option<PathBuf>) {
    let direct_save_path = path.clone().filter(|path| is_plain_text_path(path));
    *state.lock().unwrap() = CurrentDocumentState {
        opened_path: path,
        direct_save_path,
    };
}

fn save_current_document(
    parent: &Frame,
    settings: &Arc<Mutex<Settings>>,
    text_ctrl: &TextCtrl,
    document_state: &Arc<Mutex<CurrentDocumentState>>,
) -> bool {
    if text_ctrl.get_value().trim().is_empty() {
        return true;
    }

    let current_text = text_ctrl.get_value();
    let state_snapshot = document_state.lock().unwrap().clone();
    let path_buf = if let Some(path) = state_snapshot.direct_save_path {
        path
    } else {
        let preferred_extension = if state_snapshot
            .opened_path
            .as_ref()
            .is_some_and(|path| !is_plain_text_path(path))
        {
            Some("txt")
        } else {
            None
        };
        let Some(path) = prompt_text_save_path(
            parent,
            settings,
            state_snapshot.opened_path.as_deref(),
            preferred_extension,
            &current_text,
        ) else {
            return false;
        };
        path
    };

    let result = save_text_to_path(&path_buf, &current_text);
    let ui = current_ui_strings();

    match result {
        Ok(()) => {
            {
                let mut state = document_state.lock().unwrap();
                state.direct_save_path =
                    Some(path_buf.clone()).filter(|path| is_plain_text_path(path));
                state.opened_path = Some(path_buf);
            }
            text_ctrl.set_modified(false);
            show_modeless_message_dialog(parent, &ui.save_completed_title, &ui.text_saved_ok);
            true
        }
        Err(err) => {
            show_modeless_message_dialog(
                parent,
                &ui.save_text_title,
                &format!("{} ({err})", ui.text_file_not_saved),
            );
            false
        }
    }
}

fn save_current_document_as(
    parent: &Frame,
    settings: &Arc<Mutex<Settings>>,
    text_ctrl: &TextCtrl,
    document_state: &Arc<Mutex<CurrentDocumentState>>,
) -> bool {
    if text_ctrl.get_value().trim().is_empty() {
        return true;
    }

    let state_snapshot = document_state.lock().unwrap().clone();
    let preferred_extension = state_snapshot.opened_path.as_ref().map(|_| "txt");

    let Some(path) = prompt_text_save_path(
        parent,
        settings,
        state_snapshot.opened_path.as_deref(),
        preferred_extension,
        &text_ctrl.get_value(),
    ) else {
        return false;
    };

    let current_text = text_ctrl.get_value();
    let result = save_text_to_path(&path, &current_text);
    let ui = current_ui_strings();

    match result {
        Ok(()) => {
            {
                let mut state = document_state.lock().unwrap();
                state.direct_save_path = Some(path.clone()).filter(|path| is_plain_text_path(path));
                state.opened_path = Some(path);
            }
            text_ctrl.set_modified(false);
            show_modeless_message_dialog(parent, &ui.save_completed_title, &ui.text_saved_ok);
            true
        }
        Err(err) => {
            show_modeless_message_dialog(
                parent,
                &ui.save_text_title,
                &format!("{} ({err})", ui.text_file_not_saved),
            );
            false
        }
    }
}

fn default_audiobook_save_folder() -> PathBuf {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return PathBuf::new();
    };

    let documents = home.join("Documents");
    if documents.is_dir() { documents } else { home }
}

fn prompt_audiobook_save_path(parent: &Frame, settings: &Arc<Mutex<Settings>>) -> Option<PathBuf> {
    let ui = current_ui_strings();
    let settings_snapshot = settings.lock().unwrap().clone();
    let default_filename = sanitize_filename(&ui.save_default_filename);
    let dialog = Dialog::builder(parent, &ui.save_audiobook_title)
        .with_style(DialogStyle::Caption | DialogStyle::SystemMenu | DialogStyle::CloseBox)
        .with_size(520, 250)
        .build();
    let panel = Panel::builder(&dialog).build();
    let root = BoxSizer::builder(Orientation::Vertical).build();

    let name_label = StaticText::builder(&panel)
        .with_label(&ui.save_filename_label)
        .build();
    root.add(
        &name_label,
        0,
        SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Top,
        12,
    );

    let name_ctrl = TextCtrl::builder(&panel).build();
    name_ctrl.set_value(&default_filename);
    root.add(
        &name_ctrl,
        0,
        SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Top,
        12,
    );

    let format_label = StaticText::builder(&panel)
        .with_label(&ui.save_format_label)
        .build();
    root.add(
        &format_label,
        0,
        SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Top,
        12,
    );

    let format_choice = Choice::builder(&panel).build();
    format_choice.append("MP3");
    format_choice.append("M4B");
    format_choice.append("M4A");
    format_choice.append("WAV");
    format_choice.set_selection(
        if settings_snapshot
            .last_audiobook_format
            .eq_ignore_ascii_case("m4b")
        {
            1
        } else if settings_snapshot
            .last_audiobook_format
            .eq_ignore_ascii_case("m4a")
        {
            2
        } else if settings_snapshot
            .last_audiobook_format
            .eq_ignore_ascii_case("wav")
        {
            3
        } else {
            0
        },
    );
    root.add(
        &format_choice,
        0,
        SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Top,
        12,
    );

    let folder_label = StaticText::builder(&panel)
        .with_label(&ui.save_folder_label)
        .build();
    root.add(
        &folder_label,
        0,
        SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Top,
        12,
    );

    let initial_folder = if settings_snapshot.last_audiobook_save_dir.trim().is_empty() {
        default_audiobook_save_folder()
    } else {
        PathBuf::from(&settings_snapshot.last_audiobook_save_dir)
    };
    let folder_display = StaticText::builder(&panel)
        .with_label(&initial_folder.to_string_lossy())
        .build();
    root.add(
        &folder_display,
        0,
        SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Top,
        12,
    );

    let choose_folder_button = Button::builder(&panel)
        .with_label(&ui.choose_folder)
        .build();
    root.add(
        &choose_folder_button,
        0,
        SizerFlag::Left | SizerFlag::Right | SizerFlag::Top,
        12,
    );

    let buttons = BoxSizer::builder(Orientation::Horizontal).build();
    let save_button = Button::builder(&panel)
        .with_id(ID_OK)
        .with_label(&ui.save_as)
        .build();
    let cancel_button = Button::builder(&panel)
        .with_id(ID_CANCEL)
        .with_label(&ui.close)
        .build();
    buttons.add_spacer(1);
    buttons.add(&save_button, 0, SizerFlag::All, 10);
    buttons.add(&cancel_button, 0, SizerFlag::All, 10);
    root.add_sizer(&buttons, 0, SizerFlag::Expand, 0);

    panel.set_sizer(root, true);
    dialog.set_affirmative_id(ID_OK);
    dialog.set_escape_id(ID_CANCEL);

    let selected_folder = Rc::new(RefCell::new(initial_folder));
    let selected_path = Rc::new(RefCell::new(None::<PathBuf>));

    let dialog_choose = dialog;
    let folder_display_choose = folder_display;
    let selected_folder_choose = Rc::clone(&selected_folder);
    choose_folder_button.on_click(move |_| {
        let ui = current_ui_strings();
        let default_path = selected_folder_choose
            .borrow()
            .to_string_lossy()
            .into_owned();
        let dir_dialog =
            DirDialog::builder(&dialog_choose, &ui.choose_folder, &default_path).build();

        #[cfg(target_os = "macos")]
        set_mac_native_file_dialog_open(true);
        let dialog_result = dir_dialog.show_modal();
        #[cfg(target_os = "macos")]
        set_mac_native_file_dialog_open(false);

        if dialog_result != ID_OK {
            return;
        }

        if let Some(path) = dir_dialog.get_path() {
            let folder = PathBuf::from(path);
            folder_display_choose.set_label(&folder.to_string_lossy());
            *selected_folder_choose.borrow_mut() = folder;
        }
    });

    let dialog_save = dialog;
    let name_ctrl_save = name_ctrl;
    let format_choice_save = format_choice;
    let selected_folder_save = Rc::clone(&selected_folder);
    let selected_path_save = Rc::clone(&selected_path);
    let settings_save = Arc::clone(settings);
    save_button.on_click(move |_| {
        let ui = current_ui_strings();
        let filename = sanitize_filename(&name_ctrl_save.get_value());
        if filename.is_empty() {
            show_message_subdialog(
                &dialog_save,
                &ui.save_audiobook_title,
                &ui.save_filename_empty,
            );
            return;
        }

        let folder = selected_folder_save.borrow().clone();
        if folder.as_os_str().is_empty() {
            show_message_subdialog(
                &dialog_save,
                &ui.save_audiobook_title,
                &ui.save_folder_not_selected,
            );
            return;
        }

        let extension = match format_choice_save.get_selection() {
            Some(1) => "m4b",
            Some(2) => "m4a",
            Some(3) => "wav",
            _ => "mp3",
        };
        let path = folder.join(format!("{filename}.{extension}"));

        if path.exists() {
            let overwrite_dialog = MessageDialog::builder(
                &dialog_save,
                &ui.overwrite_existing_file,
                &ui.save_audiobook_title,
            )
            .with_style(MessageDialogStyle::YesNo | MessageDialogStyle::IconWarning)
            .build();
            localize_standard_dialog_buttons(&overwrite_dialog);
            if overwrite_dialog.show_modal() != ID_YES {
                return;
            }
        }

        {
            let mut locked = settings_save.lock().unwrap();
            locked.last_audiobook_save_dir = folder.to_string_lossy().into_owned();
            locked.last_audiobook_format = extension.to_string();
            locked.save();
        }

        *selected_path_save.borrow_mut() = Some(path);
        dialog_save.end_modal(ID_OK);
    });

    let dialog_cancel = dialog;
    cancel_button.on_click(move |_| {
        dialog_cancel.end_modal(ID_CANCEL);
    });

    name_ctrl.set_focus();
    name_ctrl.set_selection(0, default_filename.chars().count() as i64);

    let result = if dialog.show_modal() == ID_OK {
        selected_path.borrow().clone()
    } else {
        None
    };
    dialog.destroy();
    result
}

fn prompt_downloaded_podcast_action(parent: &Frame) -> PodcastDownloadAction {
    let ui = current_ui_strings();
    let dialog = Dialog::builder(parent, &ui.podcast_downloaded_title)
        .with_style(DialogStyle::Caption | DialogStyle::SystemMenu | DialogStyle::CloseBox)
        .with_size(460, 180)
        .build();
    let panel = Panel::builder(&dialog).build();
    let root = BoxSizer::builder(Orientation::Vertical).build();

    let text = StaticText::builder(&panel)
        .with_label(&ui.podcast_downloaded_message)
        .build();
    root.add(&text, 1, SizerFlag::Expand | SizerFlag::All, 12);

    let button_row = BoxSizer::builder(Orientation::Horizontal).build();
    let btn_open = Button::builder(&panel)
        .with_id(ID_PODCAST_DIALOG_OPEN)
        .with_label(&ui.open)
        .build();
    let btn_save_as = Button::builder(&panel)
        .with_id(ID_PODCAST_DIALOG_SAVE_AS)
        .with_label(&ui.save_as)
        .build();
    let btn_close = Button::builder(&panel)
        .with_id(ID_PODCAST_DIALOG_CLOSE)
        .with_label(&ui.close)
        .build();
    button_row.add_spacer(1);
    button_row.add(&btn_open, 0, SizerFlag::All, 10);
    button_row.add(&btn_save_as, 0, SizerFlag::All, 10);
    button_row.add(&btn_close, 0, SizerFlag::All, 10);
    root.add_sizer(&button_row, 0, SizerFlag::Expand, 0);

    panel.set_sizer(root, true);
    dialog.set_escape_id(ID_PODCAST_DIALOG_CLOSE);

    let dialog_open = dialog;
    btn_open.on_click(move |_| {
        dialog_open.end_modal(ID_PODCAST_DIALOG_OPEN);
    });

    let dialog_save_as = dialog;
    btn_save_as.on_click(move |_| {
        dialog_save_as.end_modal(ID_PODCAST_DIALOG_SAVE_AS);
    });

    let dialog_close = dialog;
    btn_close.on_click(move |_| {
        dialog_close.end_modal(ID_PODCAST_DIALOG_CLOSE);
    });

    match dialog.show_modal() {
        ID_PODCAST_DIALOG_OPEN => PodcastDownloadAction::Open,
        ID_PODCAST_DIALOG_SAVE_AS => PodcastDownloadAction::SaveAs,
        _ => PodcastDownloadAction::Close,
    }
}

fn save_downloaded_podcast_file(
    parent: &Frame,
    file_path: &Path,
    suggested_name: &str,
) -> Result<(), String> {
    let ui = current_ui_strings();
    let extension = file_path
        .extension()
        .and_then(|ext| ext.to_str())
        .filter(|ext| !ext.trim().is_empty())
        .unwrap_or("mp3");
    let default_file = format!("{}.{}", sanitize_filename(suggested_name), extension);
    let wildcard = format!("File audio (*.{extension})|*.{extension}|Tutti|*.*");
    let dialog = FileDialog::builder(parent)
        .with_message(&ui.save_podcast_episode)
        .with_default_file(&default_file)
        .with_wildcard(&wildcard)
        .with_style(FileDialogStyle::Save | FileDialogStyle::OverwritePrompt)
        .build();

    #[cfg(target_os = "macos")]
    set_mac_native_file_dialog_open(true);
    let dialog_result = dialog.show_modal();
    #[cfg(target_os = "macos")]
    set_mac_native_file_dialog_open(false);

    if dialog_result != ID_OK {
        return Ok(());
    }

    let Some(destination_path) = dialog.get_path() else {
        return Ok(());
    };

    std::fs::copy(file_path, &destination_path)
        .map_err(|err| format!("salvataggio episodio podcast fallito: {}", err))?;
    append_podcast_log(&format!(
        "external_open.saved_copy source={} destination={}",
        file_path.display(),
        destination_path
    ));
    Ok(())
}

fn confirm_delete_dialog(parent: &Frame, title: &str, message: &str) -> bool {
    let ui = current_ui_strings();
    let dialog = Dialog::builder(parent, title)
        .with_style(DialogStyle::Caption | DialogStyle::SystemMenu | DialogStyle::CloseBox)
        .with_size(460, 170)
        .build();
    let panel = Panel::builder(&dialog).build();
    let root = BoxSizer::builder(Orientation::Vertical).build();

    let label = StaticText::builder(&panel).with_label(message).build();
    root.add(&label, 1, SizerFlag::Expand | SizerFlag::All, 12);

    let buttons = BoxSizer::builder(Orientation::Horizontal).build();
    let yes_button = Button::builder(&panel)
        .with_id(ID_YES)
        .with_label(&ui.yes)
        .build();
    let no_button = Button::builder(&panel)
        .with_id(ID_NO)
        .with_label(&ui.close)
        .build();
    buttons.add_spacer(1);
    buttons.add(&yes_button, 0, SizerFlag::All, 10);
    buttons.add(&no_button, 0, SizerFlag::All, 10);
    root.add_sizer(&buttons, 0, SizerFlag::Expand, 0);

    panel.set_sizer(root, true);
    dialog.set_affirmative_id(ID_YES);
    dialog.set_escape_id(ID_NO);

    let dialog_yes = dialog;
    yes_button.on_click(move |_| {
        dialog_yes.end_modal(ID_YES);
    });

    let dialog_no = dialog;
    no_button.on_click(move |_| {
        dialog_no.end_modal(ID_NO);
    });

    let confirmed = dialog.show_modal() == ID_YES;
    dialog.destroy();
    confirmed
}

#[cfg(target_os = "macos")]
fn should_load_file_with_progress(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            ext.eq_ignore_ascii_case("pdf")
                || ext.eq_ignore_ascii_case("png")
                || ext.eq_ignore_ascii_case("jpg")
                || ext.eq_ignore_ascii_case("jpeg")
                || ext.eq_ignore_ascii_case("gif")
                || ext.eq_ignore_ascii_case("bmp")
                || ext.eq_ignore_ascii_case("tif")
                || ext.eq_ignore_ascii_case("tiff")
                || ext.eq_ignore_ascii_case("webp")
                || ext.eq_ignore_ascii_case("heic")
        })
}

#[cfg(not(target_os = "macos"))]
fn should_load_file_with_progress(_path: &Path) -> bool {
    false
}

fn load_file_with_progress(parent: &Frame, path: &Path) -> Result<String, String> {
    let ui_language = Settings::load().ui_language;
    let ui = ui_strings(&ui_language);
    let progress =
        ProgressDialog::builder(parent, &ui.open_document_title, &ui.analyzing_document, 100)
            .with_style(ProgressDialogStyle::Smooth)
            .build();
    let state = Arc::new(Mutex::new(None::<Result<String, String>>));
    let state_thread = Arc::clone(&state);
    let path_buf = path.to_path_buf();
    std::thread::spawn(move || {
        let result = file_loader::load_any_file(&path_buf).map_err(|err| err.to_string());
        *state_thread.lock().unwrap() = Some(result);
    });

    let mut progress_value = 0;
    loop {
        std::thread::sleep(std::time::Duration::from_millis(100));
        if let Some(result) = state.lock().unwrap().take() {
            progress.destroy();
            if result.is_ok() {
                show_message_dialog(parent, &ui.open_document_title, &ui.document_loaded);
            }
            return result;
        }

        progress_value = (progress_value + 4).min(95);
        let _ = progress.update(progress_value, Some(&ui.analyzing_pdf));
        if progress_value >= 95 {
            progress_value = 20;
        }
    }
}

fn load_file_for_display(parent: &Frame, path: &Path) -> Result<String, String> {
    if should_load_file_with_progress(path) {
        load_file_with_progress(parent, path)
    } else {
        file_loader::load_any_file(path).map_err(|err| err.to_string())
    }
}

fn initial_open_path_from_args() -> Option<PathBuf> {
    std::env::args_os().skip(1).find_map(|arg| {
        #[cfg(target_os = "macos")]
        if arg.to_string_lossy().starts_with("-psn_") {
            return None;
        }

        let path = PathBuf::from(arg);
        if path.is_file() { Some(path) } else { None }
    })
}

fn normalize_version_tag(tag: &str) -> String {
    tag.trim()
        .trim_start_matches('v')
        .trim_start_matches('V')
        .to_string()
}

fn parse_version_triplet(version: &str) -> Option<(u64, u64, u64)> {
    let clean = normalize_version_tag(version);
    let numeric = clean.split(['-', '+']).next().unwrap_or("").trim();
    let mut parts = numeric.split('.');
    let major = parts.next()?.parse::<u64>().ok()?;
    let minor = parts.next().unwrap_or("0").parse::<u64>().ok()?;
    let patch = parts.next().unwrap_or("0").parse::<u64>().ok()?;
    Some((major, minor, patch))
}

fn is_newer_version(latest: &str, current: &str) -> bool {
    match (
        parse_version_triplet(latest),
        parse_version_triplet(current),
    ) {
        (Some(latest), Some(current)) => latest > current,
        _ => normalize_version_tag(latest) != normalize_version_tag(current),
    }
}

fn fetch_latest_release_info() -> Result<GithubReleaseInfo, String> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("SonarpadMinimalUpdater")
        .build()
        .map_err(|err| format!("creazione client aggiornamenti fallita: {}", err))?;
    client
        .get(SONARPAD_MINIMAL_RELEASES_API_URL)
        .send()
        .and_then(|response| response.error_for_status())
        .map_err(|err| format!("download release fallito: {}", err))?
        .json::<GithubReleaseInfo>()
        .map_err(|err| format!("lettura release fallita: {}", err))
}

fn open_url_in_browser(url: &str) -> Result<(), String> {
    append_podcast_log(&format!("browser.open.begin url={url}"));
    #[cfg(target_os = "macos")]
    let status = std::process::Command::new("/usr/bin/open")
        .arg(url)
        .status()
        .map_err(|err| format!("apertura browser fallita: {}", err))?;

    #[cfg(windows)]
    let status = std::process::Command::new("cmd")
        .args(["/C", "start", "", url])
        .status()
        .map_err(|err| format!("apertura browser fallita: {}", err))?;

    #[cfg(all(not(target_os = "macos"), not(windows)))]
    let status = std::process::Command::new("xdg-open")
        .arg(url)
        .status()
        .map_err(|err| format!("apertura browser fallita: {}", err))?;

    if status.success() {
        append_podcast_log(&format!("browser.open.success url={url}"));
        Ok(())
    } else {
        append_podcast_log(&format!(
            "browser.open.failed url={} code={:?}",
            url,
            status.code()
        ));
        Err(format!(
            "apertura browser fallita con codice {:?}",
            status.code()
        ))
    }
}

#[cfg(target_os = "macos")]
fn bundled_mpv_executable_path() -> Option<PathBuf> {
    let current_exe = std::env::current_exe().ok()?;
    let contents_dir = current_exe.parent()?.parent()?;
    let bundled_path = contents_dir
        .join("Resources")
        .join("mpv.app")
        .join("Contents")
        .join("MacOS")
        .join("mpv");
    bundled_path.is_file().then_some(bundled_path)
}

#[cfg(target_os = "macos")]
fn mac_radio_open_mpv_ipc(ipc_path: &Path) -> Result<UnixStream, String> {
    UnixStream::connect(ipc_path).map_err(|err| format!("apertura canale mpv fallita: {}", err))
}

#[cfg(target_os = "macos")]
fn mac_radio_build_mpv_ipc_message(command_json: &str, request_id: u64) -> Result<String, String> {
    let mut value: serde_json::Value = serde_json::from_str(command_json)
        .map_err(|err| format!("comando mpv non valido: {err}"))?;
    let Some(object) = value.as_object_mut() else {
        return Err("comando mpv non valido".to_string());
    };
    object.insert(
        "request_id".to_string(),
        serde_json::Value::Number(serde_json::Number::from(request_id)),
    );
    serde_json::to_string(&value).map_err(|err| format!("comando mpv non valido: {err}"))
}

#[cfg(target_os = "macos")]
fn mac_radio_read_mpv_response(
    ipc_path: &Path,
    stream: &mut UnixStream,
    request_id: u64,
) -> Result<serde_json::Value, String> {
    use std::io::BufRead as _;

    loop {
        let mut reader = BufReader::new(&mut *stream);
        let mut response = String::new();
        reader
            .read_line(&mut response)
            .map_err(|err| format!("lettura risposta mpv fallita: {err}"))?;
        let trimmed = response.trim();
        if trimmed.is_empty() {
            continue;
        }
        let parsed: serde_json::Value = serde_json::from_str(trimmed)
            .map_err(|err| format!("risposta mpv non valida: {err}"))?;
        if parsed
            .get("request_id")
            .and_then(|value| value.as_u64())
            .unwrap_or_default()
            != request_id
        {
            append_podcast_log(&format!(
                "radio.mpv.ipc.skip path={} expected_request_id={} response={trimmed}",
                ipc_path.display(),
                request_id
            ));
            continue;
        }
        return Ok(parsed);
    }
}

#[cfg(target_os = "macos")]
fn mac_radio_send_mpv_command_with_stream(
    stream: &mut UnixStream,
    ipc_path: &Path,
    request_id: u64,
    command_json: &str,
) -> Result<(), String> {
    let message = mac_radio_build_mpv_ipc_message(command_json, request_id)?;
    stream
        .write_all(message.as_bytes())
        .map_err(|err| format!("invio comando mpv fallito: {err}"))?;
    stream
        .write_all(b"\n")
        .map_err(|err| format!("invio comando mpv fallito: {err}"))?;
    stream
        .flush()
        .map_err(|err| format!("invio comando mpv fallito: {err}"))?;
    let _ = mac_radio_read_mpv_response(ipc_path, stream, request_id)?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn mac_radio_send_mpv_command_transient(ipc_path: &Path, command_json: &str) -> Result<(), String> {
    let mut stream = mac_radio_open_mpv_ipc(ipc_path)?;
    mac_radio_send_mpv_command_with_stream(&mut stream, ipc_path, 1, command_json)
}

#[cfg(target_os = "macos")]
fn mac_radio_ensure_ipc_connected(state: &Rc<RefCell<MacRadioWindowState>>) -> Result<(), String> {
    let ipc_path = state
        .borrow()
        .session
        .as_ref()
        .map(|session| session.ipc_path.clone())
        .ok_or_else(|| "nessuna sessione radio attiva".to_string())?;
    if state.borrow().ipc.is_some() {
        return Ok(());
    }
    let stream = mac_radio_open_mpv_ipc(&ipc_path)?;
    state.borrow_mut().ipc = Some(stream);
    Ok(())
}

#[cfg(target_os = "macos")]
fn mac_radio_send_mpv_command(
    state: &Rc<RefCell<MacRadioWindowState>>,
    command_json: &str,
) -> Result<(), String> {
    mac_radio_ensure_ipc_connected(state)?;
    let ipc_path = state
        .borrow()
        .session
        .as_ref()
        .map(|session| session.ipc_path.clone())
        .ok_or_else(|| "nessuna sessione radio attiva".to_string())?;
    let request_id = {
        let mut locked = state.borrow_mut();
        let request_id = locked.next_request_id;
        locked.next_request_id = locked.next_request_id.saturating_add(1);
        request_id
    };
    let result = {
        let mut locked = state.borrow_mut();
        let stream = locked
            .ipc
            .as_mut()
            .ok_or_else(|| "connessione mpv non disponibile".to_string())?;
        mac_radio_send_mpv_command_with_stream(stream, &ipc_path, request_id, command_json)
    };
    if result.is_err() {
        state.borrow_mut().ipc = None;
    }
    result
}

#[cfg(target_os = "macos")]
fn update_mac_radio_controls(
    state: &Rc<RefCell<MacRadioWindowState>>,
    toggle_button: &Button,
    stop_button: &Button,
) {
    let ui_language = Settings::load().ui_language;
    let play_label = if ui_language == "it" {
        "Riproduci"
    } else {
        "Play"
    };
    let pause_label = if ui_language == "it" {
        "Pausa"
    } else {
        "Pause"
    };
    match state.borrow().status {
        PlaybackStatus::Stopped => {
            toggle_button.set_label(play_label);
            toggle_button.enable(true);
            stop_button.enable(false);
        }
        PlaybackStatus::Playing => {
            toggle_button.set_label(pause_label);
            toggle_button.enable(true);
            stop_button.enable(true);
        }
        PlaybackStatus::Paused => {
            toggle_button.set_label(play_label);
            toggle_button.enable(true);
            stop_button.enable(true);
        }
    }
}

#[cfg(target_os = "macos")]
fn register_active_mac_radio_state(state: &Rc<RefCell<MacRadioWindowState>>) {
    ACTIVE_MAC_RADIO_STATES.with(|states| {
        let mut states = states.borrow_mut();
        states.retain(|entry| entry.upgrade().is_some());
        states.push(Rc::downgrade(state));
    });
}

#[cfg(target_os = "macos")]
fn stop_all_active_mac_radio_sessions() {
    ACTIVE_MAC_RADIO_STATES.with(|states| {
        let active_states = states
            .borrow()
            .iter()
            .filter_map(Weak::upgrade)
            .collect::<Vec<_>>();
        for state in &active_states {
            let _ = stop_mac_radio_session(state);
        }
    });
}

#[cfg(target_os = "macos")]
fn stop_mac_radio_session(state: &Rc<RefCell<MacRadioWindowState>>) -> Result<(), String> {
    let (session, mut ipc, mut child) = {
        let mut locked = state.borrow_mut();
        locked.status = PlaybackStatus::Stopped;
        locked.next_request_id = 1;
        (
            locked.session.take(),
            locked.ipc.take(),
            locked.child.take(),
        )
    };
    let Some(session) = session else {
        return Ok(());
    };

    let quit_result = if let Some(stream) = ipc.as_mut() {
        mac_radio_send_mpv_command_with_stream(
            stream,
            &session.ipc_path,
            1,
            r#"{"command":["quit"]}"#,
        )
    } else {
        mac_radio_send_mpv_command_transient(&session.ipc_path, r#"{"command":["quit"]}"#)
    };

    if let Some(child) = child.as_mut() {
        if quit_result.is_err()
            && let Err(err) = child.kill()
        {
            append_podcast_log(&format!(
                "radio.mpv.kill_failed pid={} err={err}",
                session.process_id
            ));
        }
        if let Err(err) = child.wait() {
            append_podcast_log(&format!(
                "radio.mpv.wait_failed pid={} err={err}",
                session.process_id
            ));
        }
    }

    if let Err(err) = std::fs::remove_file(&session.ipc_path)
        && err.kind() != std::io::ErrorKind::NotFound
    {
        append_podcast_log(&format!(
            "radio.mpv.socket_cleanup_failed path={} err={err}",
            session.ipc_path.display()
        ));
    }

    append_podcast_log(&format!(
        "radio.mpv.stopped pid={} url={}",
        session.process_id, session.stream_url
    ));
    quit_result
}

#[cfg(target_os = "macos")]
fn mac_radio_ipc_socket_path() -> PathBuf {
    Path::new("/tmp").join(format!("spd-radio-{}.sock", Uuid::new_v4().simple()))
}

#[cfg(target_os = "macos")]
fn launch_mac_radio_session(
    state: &Rc<RefCell<MacRadioWindowState>>,
    station_name: &str,
    stream_url: &str,
) -> Result<(), String> {
    let mpv_executable = bundled_mpv_executable_path().unwrap_or_else(|| PathBuf::from("mpv"));
    let ipc_path = mac_radio_ipc_socket_path();
    if let Err(err) = std::fs::remove_file(&ipc_path)
        && err.kind() != std::io::ErrorKind::NotFound
    {
        append_podcast_log(&format!(
            "radio.mpv.socket_prep_failed path={} err={err}",
            ipc_path.display()
        ));
    }

    let mut command = Command::new(&mpv_executable);
    if let Some(parent_dir) = mpv_executable.parent()
        && !parent_dir.as_os_str().is_empty()
    {
        command.current_dir(parent_dir);
    }
    command
        .arg(stream_url)
        .arg("--no-config")
        .arg("--no-video")
        .arg("--force-window=no")
        .arg("--idle=no")
        .arg("--no-terminal")
        .arg("--volume-max=300")
        .arg(format!("--input-ipc-server={}", ipc_path.display()))
        .arg(format!("--title={station_name}"));

    let mut child = command
        .spawn()
        .map_err(|err| format!("avvio mpv fallito: {err}"))?;

    for attempt in 0..150 {
        if let Ok(mut persistent_ipc) = mac_radio_open_mpv_ipc(&ipc_path) {
            let handshake_result = mac_radio_send_mpv_command_with_stream(
                &mut persistent_ipc,
                &ipc_path,
                1,
                r#"{"command":["get_property","pause"]}"#,
            );
            if let Err(err) = &handshake_result {
                append_podcast_log(&format!(
                    "radio.mpv.handshake_pending attempt={} path={} err={err}",
                    attempt + 1,
                    ipc_path.display()
                ));
            }
            let mut locked = state.borrow_mut();
            locked.session = Some(MacRadioMpvSession {
                ipc_path: ipc_path.clone(),
                process_id: child.id(),
                stream_url: stream_url.to_string(),
            });
            locked.ipc = Some(persistent_ipc);
            locked.child = Some(child);
            locked.next_request_id = 2;
            locked.status = PlaybackStatus::Playing;
            append_podcast_log(&format!(
                "radio.mpv.started pid={} path={} url={} handshake_ok={}",
                locked
                    .session
                    .as_ref()
                    .map(|session| session.process_id)
                    .unwrap_or_default(),
                ipc_path.display(),
                stream_url,
                handshake_result.is_ok()
            ));
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    append_podcast_log(&format!(
        "radio.mpv.start_timeout path={} url={}",
        ipc_path.display(),
        stream_url
    ));

    if let Err(err) = child.kill() {
        append_podcast_log(&format!("radio.mpv.launch_cleanup_kill_failed err={err}"));
    }
    if let Err(err) = child.wait() {
        append_podcast_log(&format!("radio.mpv.launch_cleanup_wait_failed err={err}"));
    }
    if let Err(err) = std::fs::remove_file(&ipc_path)
        && err.kind() != std::io::ErrorKind::NotFound
    {
        append_podcast_log(&format!(
            "radio.mpv.launch_cleanup_socket_failed path={} err={err}",
            ipc_path.display()
        ));
    }
    Err("inizializzazione controllo radio fallita".to_string())
}

#[cfg(target_os = "macos")]
fn open_radio_station(
    parent: &impl WxWidget,
    station_name: &str,
    stream_url: &str,
) -> Result<(), String> {
    append_podcast_log(&format!(
        "radio.macos.open.begin name={} url={}",
        station_name, stream_url
    ));
    let dialog = Dialog::builder(parent, station_name)
        .with_style(DialogStyle::Caption | DialogStyle::SystemMenu | DialogStyle::CloseBox)
        .with_size(360, 150)
        .build();
    let panel = Panel::builder(&dialog).build();
    let root = BoxSizer::builder(Orientation::Vertical).build();
    let ui = current_ui_strings();

    let title = StaticText::builder(&panel).with_label(station_name).build();
    root.add(
        &title,
        0,
        SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Top,
        12,
    );

    let buttons = BoxSizer::builder(Orientation::Horizontal).build();
    let toggle_button = Button::builder(&panel)
        .with_label(if Settings::load().ui_language == "it" {
            "Riproduci"
        } else {
            "Play"
        })
        .build();
    let stop_button = Button::builder(&panel)
        .with_label(if Settings::load().ui_language == "it" {
            "Ferma"
        } else {
            "Stop"
        })
        .build();
    let close_button = Button::builder(&panel).with_label(&ui.close).build();
    buttons.add(&toggle_button, 0, SizerFlag::All, 8);
    buttons.add(&stop_button, 0, SizerFlag::All, 8);
    buttons.add(&close_button, 0, SizerFlag::All, 8);
    root.add_sizer(
        &buttons,
        0,
        SizerFlag::AlignCentre | SizerFlag::Bottom | SizerFlag::Top,
        8,
    );

    panel.set_sizer(root, true);

    let state = Rc::new(RefCell::new(MacRadioWindowState {
        session: None,
        ipc: None,
        child: None,
        next_request_id: 1,
        status: PlaybackStatus::Stopped,
    }));
    register_active_mac_radio_state(&state);

    launch_mac_radio_session(&state, station_name, stream_url)?;
    update_mac_radio_controls(&state, &toggle_button, &stop_button);

    let state_toggle = Rc::clone(&state);
    let toggle_button_toggle = toggle_button;
    let stop_button_toggle = stop_button;
    let station_name_toggle = station_name.to_string();
    let stream_url_toggle = stream_url.to_string();
    let dialog_toggle = dialog;
    toggle_button.on_click(move |_| {
        let result = match state_toggle.borrow().status {
            PlaybackStatus::Playing => mac_radio_send_mpv_command(
                &state_toggle,
                r#"{"command":["set_property","pause",true]}"#,
            )
            .map(|()| {
                state_toggle.borrow_mut().status = PlaybackStatus::Paused;
            }),
            PlaybackStatus::Paused => mac_radio_send_mpv_command(
                &state_toggle,
                r#"{"command":["set_property","pause",false]}"#,
            )
            .map(|()| {
                state_toggle.borrow_mut().status = PlaybackStatus::Playing;
            }),
            PlaybackStatus::Stopped => {
                let _ = stop_mac_radio_session(&state_toggle);
                launch_mac_radio_session(&state_toggle, &station_name_toggle, &stream_url_toggle)
            }
        };
        if let Err(err) = result {
            show_message_subdialog(&dialog_toggle, "Radio", &err);
        }
        update_mac_radio_controls(&state_toggle, &toggle_button_toggle, &stop_button_toggle);
    });

    let state_stop = Rc::clone(&state);
    let toggle_button_stop = toggle_button;
    let stop_button_stop = stop_button;
    let dialog_stop = dialog;
    stop_button.on_click(move |_| {
        if let Err(err) = stop_mac_radio_session(&state_stop) {
            show_message_subdialog(&dialog_stop, "Radio", &err);
        }
        update_mac_radio_controls(&state_stop, &toggle_button_stop, &stop_button_stop);
    });

    let dialog_close_button = dialog;
    close_button.on_click(move |_| {
        dialog_close_button.close(true);
    });

    let timer = Rc::new(Timer::new(&dialog));
    let timer_tick = Rc::clone(&timer);
    let timer_tick_stop = Rc::clone(&timer);
    let dialog_tick = dialog;
    let state_tick = Rc::clone(&state);
    timer_tick.on_tick(move |_| {
        let exited = {
            let mut locked = state_tick.borrow_mut();
            if let Some(child) = locked.child.as_mut() {
                match child.try_wait() {
                    Ok(Some(_status)) => true,
                    Ok(None) => false,
                    Err(err) => {
                        append_podcast_log(&format!("radio.mpv.try_wait_failed err={err}"));
                        true
                    }
                }
            } else {
                false
            }
        };
        if exited {
            let _ = stop_mac_radio_session(&state_tick);
            timer_tick_stop.stop();
            dialog_tick.destroy();
        }
    });
    timer.start(500, false);

    let timer_close = Rc::clone(&timer);
    let state_close = Rc::clone(&state);
    dialog.on_close(move |event| {
        timer_close.stop();
        let _ = stop_mac_radio_session(&state_close);
        event.skip(true);
    });

    dialog.show(true);
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn open_radio_station(
    _parent: &impl WxWidget,
    _station_name: &str,
    stream_url: &str,
) -> Result<(), String> {
    open_url_in_browser(stream_url)
}

#[derive(Deserialize)]
struct RadioBrowserStation {
    #[serde(default)]
    name: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    url_resolved: String,
}

fn fetch_radio_browser_stations(language_code: &str) -> Result<Vec<RadioStation>, String> {
    const RADIO_BROWSER_MIRRORS: [&str; 3] = [
        "https://de1.api.radio-browser.info",
        "https://fi1.api.radio-browser.info",
        "https://at1.api.radio-browser.info",
    ];

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent("Sonarpad Radio/1.0")
        .build()
        .map_err(|err| err.to_string())?;
    let mut last_error = None;
    let mut stations = None;
    let mut query = vec![
        ("hidebroken", "true"),
        ("order", "clickcount"),
        ("reverse", "true"),
        ("limit", RADIO_BROWSER_LIMIT),
    ];
    if let Some(country_code) = language_code.strip_prefix("country:") {
        query.push(("countrycode", country_code));
    } else {
        let language = radio_browser_language_name(language_code);
        query.push(("language", language));
        query.push(("languageExact", "true"));
    }

    for mirror in RADIO_BROWSER_MIRRORS {
        match client
            .get(format!("{mirror}/json/stations/search"))
            .query(&query)
            .send()
            .and_then(|response| response.error_for_status())
        {
            Ok(response) => match response.json::<Vec<RadioBrowserStation>>() {
                Ok(value) => {
                    stations = Some(value);
                    break;
                }
                Err(err) => last_error = Some(err.to_string()),
            },
            Err(err) => last_error = Some(err.to_string()),
        }
    }

    let stations = stations
        .ok_or_else(|| last_error.unwrap_or_else(|| "radio browser request failed".to_string()))?;

    let stations = stations
        .into_iter()
        .filter_map(|station| {
            let stream_url = if station.url_resolved.trim().is_empty() {
                station.url.trim().to_string()
            } else {
                station.url_resolved.trim().to_string()
            };
            if stream_url.is_empty() {
                return None;
            }

            let name = if station.name.trim().is_empty() {
                stream_url.clone()
            } else {
                station.name.replace('&', "")
            };
            Some(RadioStation { name, stream_url })
        })
        .collect::<Vec<RadioStation>>();

    Ok(normalize_radio_stations(stations))
}

fn radio_browser_language_name(language_code: &str) -> &str {
    match language_code {
        "cs" => "czech",
        "en" => "english",
        "es" => "spanish",
        "fr" => "french",
        "it" => "italian",
        "lt" => "lithuanian",
        "pl" => "polish",
        "pt" => "portuguese",
        "ru" => "russian",
        "sr" => "serbian",
        "sv" => "swedish",
        "uk" => "ukrainian",
        "vi" => "vietnamese",
        "zh" => "chinese",
        _ => language_code,
    }
}

fn normalize_radio_stations(mut stations: Vec<RadioStation>) -> Vec<RadioStation> {
    stations
        .retain(|station| !station.name.trim().is_empty() && !station.stream_url.trim().is_empty());
    for station in &mut stations {
        station.name = station
            .name
            .replace('&', "")
            .replace('\t', " ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if canonical_radio_name(&station.name) == "rai radio tutta italiana" {
            station.name = "Rai Radio Tutta Italiana".to_string();
        }
        station.stream_url = normalize_radio_stream_url(&station.name, &station.stream_url);
    }
    stations.retain(|station| {
        let canonical = canonical_radio_name(&station.name);
        canonical != "rai" && canonical != "rai radio tutta italiana"
    });
    stations.sort_by(|left, right| {
        radio_name_priority(&left.name)
            .cmp(&radio_name_priority(&right.name))
            .then_with(|| left.name.len().cmp(&right.name.len()))
            .then_with(|| left.name.cmp(&right.name))
            .then_with(|| left.stream_url.cmp(&right.stream_url))
    });
    stations.dedup_by(|left, right| {
        canonical_radio_name(&left.name) == canonical_radio_name(&right.name)
            || left.stream_url == right.stream_url
    });
    stations
}

fn merge_radio_stations_preserving_local(
    local_stations: &[RadioStation],
    fetched_stations: Vec<RadioStation>,
) -> Vec<RadioStation> {
    let mut merged = local_stations.to_vec();
    let mut seen_names = local_stations
        .iter()
        .map(|station| canonical_radio_name(&station.name))
        .collect::<HashSet<String>>();
    let mut seen_urls = local_stations
        .iter()
        .map(|station| station.stream_url.clone())
        .collect::<HashSet<String>>();

    for station in fetched_stations {
        let canonical_name = canonical_radio_name(&station.name);
        if seen_names.contains(&canonical_name) || seen_urls.contains(&station.stream_url) {
            continue;
        }

        seen_names.insert(canonical_name);
        seen_urls.insert(station.stream_url.clone());
        merged.push(station);
    }

    merged.sort_by(|left, right| {
        radio_name_priority(&left.name)
            .cmp(&radio_name_priority(&right.name))
            .then_with(|| left.name.len().cmp(&right.name.len()))
            .then_with(|| left.name.cmp(&right.name))
            .then_with(|| left.stream_url.cmp(&right.stream_url))
    });
    merged
}

fn embedded_radio_stations() -> HashMap<String, Vec<RadioStation>> {
    let stations = serde_json::from_str::<HashMap<String, Vec<RadioStation>>>(include_str!(
        "../i18n/radio.json"
    ))
    .expect("invalid embedded radio json");

    stations
        .into_iter()
        .map(|(language_code, entries)| (language_code, normalize_radio_stations(entries)))
        .collect()
}

fn radio_favorite_menu_id(index: usize) -> i32 {
    ID_RADIO_FAVORITE_BASE + index as i32
}

fn favorite_from_station(language_code: &str, station: &RadioStation) -> RadioFavorite {
    RadioFavorite {
        language_code: language_code.to_string(),
        name: station.name.clone(),
        stream_url: station.stream_url.clone(),
    }
}

fn normalized_radio_name(value: &str) -> String {
    value
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn canonical_radio_name(value: &str) -> String {
    let mut normalized = normalized_radio_name(value);
    if let Some(rest) = normalized.strip_prefix("radio rai ") {
        normalized = format!("rai radio {rest}");
    }
    normalized = normalized
        .replace("rai radiouno", "rai radio 1")
        .replace("rai radiodue", "rai radio 2")
        .replace("rai radiotre", "rai radio 3")
        .replace("rai radio1", "rai radio 1")
        .replace("rai radio2", "rai radio 2")
        .replace("rai radio3", "rai radio 3")
        .replace("rai radio uno", "rai radio 1")
        .replace("rai radio due", "rai radio 2")
        .replace("rai radio tre", "rai radio 3");
    normalized
}

fn normalize_radio_stream_url(name: &str, stream_url: &str) -> String {
    let canonical_name = canonical_radio_name(name);
    if canonical_name == "radio24 il sole 24 ore"
        || canonical_name == "radio 24 il sole 24 ore"
        || canonical_name == "radio24"
        || canonical_name == "radio 24"
    {
        "http://shoutcast2.radio24.it:8000/;".to_string()
    } else {
        stream_url.trim().to_string()
    }
}

fn radio_name_priority(value: &str) -> (usize, usize, String) {
    let normalized = normalized_radio_name(value);
    let canonical = canonical_radio_name(value);
    let starts_with_rai_radio = usize::from(!normalized.starts_with("rai radio "));
    let starts_with_rai = usize::from(!normalized.starts_with("rai "));
    (starts_with_rai_radio, starts_with_rai, canonical)
}

fn radio_search_rank(name: &str, keyword: &str) -> (usize, usize, usize, String) {
    let normalized_name = normalized_radio_name(name);
    let canonical_name = canonical_radio_name(name);
    let exact = normalized_name == keyword;
    let starts_with = normalized_name.starts_with(keyword);
    let word_boundary = normalized_name.contains(&format!(" {keyword}"));
    let position = normalized_name.find(keyword).unwrap_or(usize::MAX);
    let is_keyword_only = canonical_name == keyword;
    let rai_radio_priority = if keyword == "rai" && canonical_name.starts_with("rai radio ") {
        0
    } else if keyword == "rai" && canonical_name.starts_with("rai ") {
        1
    } else {
        2
    };
    let tier = if exact {
        0
    } else if starts_with {
        1
    } else if word_boundary {
        2
    } else {
        3
    };

    let adjusted_tier = if is_keyword_only { tier + 10 } else { tier };

    (adjusted_tier, rai_radio_priority, position, canonical_name)
}

fn radio_name_matches_keyword(name: &str, keyword: &str) -> bool {
    let keyword = normalized_radio_name(keyword);
    if keyword.is_empty() {
        return false;
    }

    let canonical_name = canonical_radio_name(name);
    if canonical_name == keyword
        || canonical_name.starts_with(&format!("{keyword} "))
        || canonical_name.contains(&format!(" {keyword} "))
    {
        return true;
    }

    if keyword.contains(' ') {
        return false;
    }

    if keyword.len() < 4 {
        return false;
    }

    canonical_name
        .split_whitespace()
        .any(|word| word.starts_with(&keyword))
}

fn default_radio_language_selection(
    languages: &[(String, String)],
    stations_by_language: &HashMap<String, Vec<RadioStation>>,
) -> usize {
    let has_stations = |code: &str| {
        stations_by_language
            .get(code)
            .is_some_and(|stations| !stations.is_empty())
    };

    languages
        .iter()
        .position(|(code, _)| code == "it" && has_stations(code))
        .or_else(|| languages.iter().position(|(code, _)| has_stations(code)))
        .unwrap_or(0)
}

fn radio_label(favorite: &RadioFavorite) -> String {
    let canonical = canonical_radio_name(&favorite.name);
    let display_name = if canonical == "rai" && favorite.language_code == "it" {
        "Rai Radio generica".to_string()
    } else {
        favorite.name.clone()
    };
    if favorite.language_code == "custom" || favorite.language_code == "it" {
        display_name
    } else {
        format!(
            "{} ({})",
            display_name,
            radio_menu_entry_label(&favorite.language_code)
        )
    }
}

fn open_add_radio_dialog(parent: &Frame) -> Option<(String, String)> {
    let ui = current_ui_strings();
    let dialog = Dialog::builder(parent, &ui.add_radio_title)
        .with_style(DialogStyle::DefaultDialogStyle | DialogStyle::ResizeBorder)
        .with_size(560, 220)
        .build();
    let panel = Panel::builder(&dialog).build();
    let root = BoxSizer::builder(Orientation::Vertical).build();

    let title_row = BoxSizer::builder(Orientation::Horizontal).build();
    title_row.add(
        &StaticText::builder(&panel)
            .with_label(&ui.title_label)
            .build(),
        0,
        SizerFlag::AlignCenterVertical | SizerFlag::All,
        5,
    );
    let title_ctrl = TextCtrl::builder(&panel).build();
    title_row.add(&title_ctrl, 1, SizerFlag::Expand | SizerFlag::All, 5);
    root.add_sizer(&title_row, 0, SizerFlag::Expand, 0);

    let url_row = BoxSizer::builder(Orientation::Horizontal).build();
    url_row.add(
        &StaticText::builder(&panel)
            .with_label(&ui.radio_url_label)
            .build(),
        0,
        SizerFlag::AlignCenterVertical | SizerFlag::All,
        5,
    );
    let url_ctrl = TextCtrl::builder(&panel).build();
    url_row.add(&url_ctrl, 1, SizerFlag::Expand | SizerFlag::All, 5);
    root.add_sizer(&url_row, 0, SizerFlag::Expand, 0);

    let buttons = BoxSizer::builder(Orientation::Horizontal).build();
    let ok_button = Button::builder(&panel)
        .with_id(ID_OK)
        .with_label(&ui.ok)
        .build();
    let cancel_button = Button::builder(&panel)
        .with_id(ID_CANCEL)
        .with_label(&ui.cancel)
        .build();
    buttons.add_spacer(1);
    buttons.add(&ok_button, 0, SizerFlag::All, 10);
    buttons.add(&cancel_button, 0, SizerFlag::All, 10);
    root.add_sizer(&buttons, 0, SizerFlag::Expand, 0);
    panel.set_sizer(root, true);

    dialog.set_affirmative_id(ID_OK);
    dialog.set_escape_id(ID_CANCEL);

    let result = if dialog.show_modal() == ID_OK {
        let title = title_ctrl.get_value().trim().to_string();
        let url = url_ctrl.get_value().trim().to_string();
        if title.is_empty() || url.is_empty() {
            None
        } else {
            Some((title, url))
        }
    } else {
        None
    };

    dialog.destroy();
    result
}

fn open_edit_radio_favorite_dialog(
    parent: &Frame,
    settings: &Arc<Mutex<Settings>>,
) -> Option<(usize, String, String)> {
    let ui = current_ui_strings();
    let favorites = settings.lock().unwrap().radio_favorites.clone();
    if favorites.is_empty() {
        return None;
    }

    let dialog = Dialog::builder(parent, &ui.edit_radio_title)
        .with_style(DialogStyle::DefaultDialogStyle | DialogStyle::ResizeBorder)
        .with_size(560, 260)
        .build();
    let panel = Panel::builder(&dialog).build();
    let root = BoxSizer::builder(Orientation::Vertical).build();

    let radio_row = BoxSizer::builder(Orientation::Horizontal).build();
    radio_row.add(
        &StaticText::builder(&panel)
            .with_label(&ui.radio_label)
            .build(),
        0,
        SizerFlag::AlignCenterVertical | SizerFlag::All,
        5,
    );
    let choice_favorite = Choice::builder(&panel).build();
    for favorite in &favorites {
        choice_favorite.append(&radio_label(favorite));
    }
    choice_favorite.set_selection(0);
    radio_row.add(&choice_favorite, 1, SizerFlag::Expand | SizerFlag::All, 5);
    root.add_sizer(&radio_row, 0, SizerFlag::Expand, 0);

    let title_row = BoxSizer::builder(Orientation::Horizontal).build();
    title_row.add(
        &StaticText::builder(&panel)
            .with_label(&ui.title_label)
            .build(),
        0,
        SizerFlag::AlignCenterVertical | SizerFlag::All,
        5,
    );
    let title_ctrl = TextCtrl::builder(&panel).build();
    title_row.add(&title_ctrl, 1, SizerFlag::Expand | SizerFlag::All, 5);
    root.add_sizer(&title_row, 0, SizerFlag::Expand, 0);

    let url_row = BoxSizer::builder(Orientation::Horizontal).build();
    url_row.add(
        &StaticText::builder(&panel)
            .with_label(&ui.radio_url_label)
            .build(),
        0,
        SizerFlag::AlignCenterVertical | SizerFlag::All,
        5,
    );
    let url_ctrl = TextCtrl::builder(&panel).build();
    url_row.add(&url_ctrl, 1, SizerFlag::Expand | SizerFlag::All, 5);
    root.add_sizer(&url_row, 0, SizerFlag::Expand, 0);

    let selected_index = Rc::new(RefCell::new(0usize));
    if let Some(favorite) = favorites.first() {
        title_ctrl.set_value(&favorite.name);
        url_ctrl.set_value(&favorite.stream_url);
    }

    let title_ctrl_evt = title_ctrl;
    let url_ctrl_evt = url_ctrl;
    let choice_favorite_evt = choice_favorite;
    let favorites_evt = favorites.clone();
    let selected_index_evt = Rc::clone(&selected_index);
    choice_favorite.on_selection_changed(move |_| {
        if let Some(selection) = choice_favorite_evt.get_selection() {
            let selection = selection as usize;
            *selected_index_evt.borrow_mut() = selection;
            if let Some(favorite) = favorites_evt.get(selection) {
                title_ctrl_evt.set_value(&favorite.name);
                url_ctrl_evt.set_value(&favorite.stream_url);
            }
        }
    });

    let buttons = BoxSizer::builder(Orientation::Horizontal).build();
    let ok_button = Button::builder(&panel)
        .with_id(ID_OK)
        .with_label(&ui.ok)
        .build();
    let cancel_button = Button::builder(&panel)
        .with_id(ID_CANCEL)
        .with_label(&ui.cancel)
        .build();
    buttons.add_spacer(1);
    buttons.add(&ok_button, 0, SizerFlag::All, 10);
    buttons.add(&cancel_button, 0, SizerFlag::All, 10);
    root.add_sizer(&buttons, 0, SizerFlag::Expand, 0);
    panel.set_sizer(root, true);

    dialog.set_affirmative_id(ID_OK);
    dialog.set_escape_id(ID_CANCEL);

    let result = if dialog.show_modal() == ID_OK {
        let title = title_ctrl.get_value().trim().to_string();
        let url = url_ctrl.get_value().trim().to_string();
        if title.is_empty() || url.is_empty() {
            None
        } else {
            Some((*selected_index.borrow(), title, url))
        }
    } else {
        None
    };

    dialog.destroy();
    result
}

fn open_delete_radio_favorite_dialog(
    parent: &Frame,
    settings: &Arc<Mutex<Settings>>,
) -> Option<usize> {
    let ui = current_ui_strings();
    let favorites = settings.lock().unwrap().radio_favorites.clone();
    if favorites.is_empty() {
        return None;
    }

    let dialog = Dialog::builder(parent, &ui.delete_radio_favorite)
        .with_style(DialogStyle::DefaultDialogStyle | DialogStyle::ResizeBorder)
        .with_size(560, 160)
        .build();
    let panel = Panel::builder(&dialog).build();
    let root = BoxSizer::builder(Orientation::Vertical).build();

    let row = BoxSizer::builder(Orientation::Horizontal).build();
    row.add(
        &StaticText::builder(&panel)
            .with_label(&ui.radio_label)
            .build(),
        0,
        SizerFlag::AlignCenterVertical | SizerFlag::All,
        5,
    );
    let choice_favorite = Choice::builder(&panel).build();
    for favorite in &favorites {
        choice_favorite.append(&radio_label(favorite));
    }
    choice_favorite.set_selection(0);
    row.add(&choice_favorite, 1, SizerFlag::Expand | SizerFlag::All, 5);
    root.add_sizer(&row, 0, SizerFlag::Expand, 0);

    let selected_index = Rc::new(RefCell::new(0usize));
    let choice_favorite_evt = choice_favorite;
    let selected_index_evt = Rc::clone(&selected_index);
    choice_favorite.on_selection_changed(move |_| {
        if let Some(selection) = choice_favorite_evt.get_selection() {
            *selected_index_evt.borrow_mut() = selection as usize;
        }
    });

    let buttons = BoxSizer::builder(Orientation::Horizontal).build();
    let ok_button = Button::builder(&panel)
        .with_id(ID_OK)
        .with_label(&ui.ok)
        .build();
    buttons.add_spacer(1);
    buttons.add(&ok_button, 0, SizerFlag::All, 10);
    root.add_sizer(&buttons, 0, SizerFlag::Expand, 0);
    panel.set_sizer(root, true);

    dialog.set_affirmative_id(ID_OK);
    let dialog_ok = dialog;
    ok_button.on_click(move |_| {
        dialog_ok.end_modal(ID_OK);
    });

    let result = if dialog.show_modal() == ID_OK {
        Some(*selected_index.borrow())
    } else {
        None
    };

    dialog.destroy();
    result
}

fn open_reorder_radio_favorites_dialog(
    parent: &Frame,
    settings: &Arc<Mutex<Settings>>,
) -> Option<Vec<RadioFavorite>> {
    let ui = current_ui_strings();
    let favorites = settings.lock().unwrap().radio_favorites.clone();
    if favorites.len() < 2 {
        return None;
    }

    let dialog = Dialog::builder(parent, &ui.reorder_radios)
        .with_style(DialogStyle::DefaultDialogStyle | DialogStyle::ResizeBorder)
        .with_size(560, 220)
        .build();
    let panel = Panel::builder(&dialog).build();
    let root = BoxSizer::builder(Orientation::Vertical).build();

    let working_favorites = Rc::new(RefCell::new(favorites));

    let radio_row = BoxSizer::builder(Orientation::Horizontal).build();
    radio_row.add(
        &StaticText::builder(&panel)
            .with_label(&ui.radio_label)
            .build(),
        0,
        SizerFlag::AlignCenterVertical | SizerFlag::All,
        5,
    );
    let choice_favorite = Choice::builder(&panel).build();
    radio_row.add(&choice_favorite, 1, SizerFlag::Expand | SizerFlag::All, 5);
    root.add_sizer(&radio_row, 0, SizerFlag::Expand, 0);

    let action_row = BoxSizer::builder(Orientation::Horizontal).build();
    let move_up_button = Button::builder(&panel).with_label(&ui.move_up).build();
    let move_down_button = Button::builder(&panel).with_label(&ui.move_down).build();
    action_row.add(&move_up_button, 1, SizerFlag::All, 5);
    action_row.add(&move_down_button, 1, SizerFlag::All, 5);
    root.add_sizer(&action_row, 0, SizerFlag::Expand, 0);

    let buttons = BoxSizer::builder(Orientation::Horizontal).build();
    let ok_button = Button::builder(&panel)
        .with_id(ID_OK)
        .with_label(&ui.ok)
        .build();
    buttons.add_spacer(1);
    buttons.add(&ok_button, 0, SizerFlag::All, 10);
    root.add_sizer(&buttons, 0, SizerFlag::Expand, 0);
    panel.set_sizer(root, true);

    let refresh_choice = Rc::new({
        let working_favorites = Rc::clone(&working_favorites);
        move |choice: &Choice, selected_index: usize| {
            choice.clear();
            let current_favorites = working_favorites.borrow();
            for favorite in current_favorites.iter() {
                choice.append(&radio_label(favorite));
            }
            let max_index = current_favorites.len().saturating_sub(1);
            choice.set_selection(selected_index.min(max_index) as u32);
        }
    });

    refresh_choice(&choice_favorite, 0);

    let selected_index = Rc::new(RefCell::new(0usize));

    let choice_favorite_evt = choice_favorite;
    let selected_index_evt = Rc::clone(&selected_index);
    choice_favorite.on_selection_changed(move |_| {
        if let Some(selection) = choice_favorite_evt.get_selection() {
            *selected_index_evt.borrow_mut() = selection as usize;
        }
    });

    let choice_favorite_up = choice_favorite;
    let selected_index_up = Rc::clone(&selected_index);
    let working_favorites_up = Rc::clone(&working_favorites);
    let refresh_choice_up = Rc::clone(&refresh_choice);
    let dialog_up = dialog;
    move_up_button.on_click(move |_| {
        let current_index = *selected_index_up.borrow();
        if current_index == 0 {
            return;
        }
        let (moved_label, target_label) = {
            let favorites = working_favorites_up.borrow();
            (
                radio_label(&favorites[current_index]),
                radio_label(&favorites[current_index - 1]),
            )
        };
        {
            let mut favorites = working_favorites_up.borrow_mut();
            favorites.swap(current_index, current_index - 1);
        }
        let new_index = current_index - 1;
        *selected_index_up.borrow_mut() = new_index;
        refresh_choice_up(&choice_favorite_up, new_index);
        show_message_subdialog(
            &dialog_up,
            &ui.reorder_radios,
            &reorder_feedback_message(&moved_label, &target_label, true),
        );
    });

    let choice_favorite_down = choice_favorite;
    let selected_index_down = Rc::clone(&selected_index);
    let working_favorites_down = Rc::clone(&working_favorites);
    let refresh_choice_down = Rc::clone(&refresh_choice);
    let dialog_down = dialog;
    move_down_button.on_click(move |_| {
        let current_index = *selected_index_down.borrow();
        let len = working_favorites_down.borrow().len();
        if current_index + 1 >= len {
            return;
        }
        let (moved_label, target_label) = {
            let favorites = working_favorites_down.borrow();
            (
                radio_label(&favorites[current_index]),
                radio_label(&favorites[current_index + 1]),
            )
        };
        {
            let mut favorites = working_favorites_down.borrow_mut();
            favorites.swap(current_index, current_index + 1);
        }
        let new_index = current_index + 1;
        *selected_index_down.borrow_mut() = new_index;
        refresh_choice_down(&choice_favorite_down, new_index);
        show_message_subdialog(
            &dialog_down,
            &ui.reorder_radios,
            &reorder_feedback_message(&moved_label, &target_label, false),
        );
    });

    dialog.set_affirmative_id(ID_OK);
    let dialog_ok = dialog;
    ok_button.on_click(move |_| {
        dialog_ok.end_modal(ID_OK);
    });

    let result = if dialog.show_modal() == ID_OK {
        Some(working_favorites.borrow().clone())
    } else {
        None
    };

    dialog.destroy();
    result
}

fn open_radio_search_dialog(
    parent: &Frame,
    settings: &Arc<Mutex<Settings>>,
    radio_menu_state: &Arc<Mutex<RadioMenuState>>,
) {
    println!("DEBUG: Radio Search Dialog v5 - Enter");
    append_podcast_log("radio_search_dialog.enter_v5");

    let ui_language = Settings::load().ui_language;
    let languages = radio_menu_languages();

    println!("DEBUG: Radio Search Dialog - Building");
    let dialog = Dialog::builder(parent, "Cerca radio")
        .with_style(DialogStyle::DefaultDialogStyle | DialogStyle::ResizeBorder)
        .with_size(760, 260)
        .build();
    let panel = Panel::builder(&dialog).build();
    let root = BoxSizer::builder(Orientation::Vertical).build();

    let search_row = BoxSizer::builder(Orientation::Horizontal).build();
    search_row.add(
        &StaticText::builder(&panel)
            .with_label(if ui_language == "it" { "Testo" } else { "Text" })
            .build(),
        0,
        SizerFlag::AlignCenterVertical | SizerFlag::All,
        5,
    );
    let keyword_ctrl = TextCtrl::builder(&panel)
        .with_style(TextCtrlStyle::ProcessEnter)
        .build();
    search_row.add(&keyword_ctrl, 1, SizerFlag::Expand | SizerFlag::All, 5);
    root.add_sizer(&search_row, 0, SizerFlag::Expand, 0);

    let language_row = BoxSizer::builder(Orientation::Horizontal).build();
    language_row.add(
        &StaticText::builder(&panel)
            .with_label(if ui_language == "it" {
                "Lingua"
            } else {
                "Language"
            })
            .build(),
        0,
        SizerFlag::AlignCenterVertical | SizerFlag::All,
        5,
    );
    let choice_language = Choice::builder(&panel).build();
    for (_, label) in &languages {
        choice_language.append(label);
    }

    println!("DEBUG: Radio Search Dialog v5 - Getting initial selection");
    let initial_selection = {
        let state = radio_menu_state.lock().unwrap();
        default_radio_language_selection(&languages, &state.stations_by_language) as u32
    };
    choice_language.set_selection(initial_selection);
    language_row.add(&choice_language, 1, SizerFlag::Expand | SizerFlag::All, 5);
    root.add_sizer(&language_row, 0, SizerFlag::Expand, 0);

    let button_row = BoxSizer::builder(Orientation::Horizontal).build();
    let button_show_all = Button::builder(&panel)
        .with_label(if ui_language == "it" {
            "Visualizza tutte le stazioni della lingua selezionata"
        } else {
            "Show all stations for selected language"
        })
        .build();
    let button_search = Button::builder(&panel)
        .with_id(ID_OK)
        .with_label(if ui_language == "it" {
            "Ricerca"
        } else {
            "Search"
        })
        .build();
    let button_close = Button::builder(&panel)
        .with_id(ID_CANCEL)
        .with_label(if ui_language == "it" {
            "Chiudi"
        } else {
            "Close"
        })
        .build();
    button_row.add(&button_show_all, 1, SizerFlag::All, 5);
    button_row.add(&button_search, 0, SizerFlag::All, 5);
    button_row.add(&button_close, 0, SizerFlag::All, 5);
    root.add_sizer(&button_row, 0, SizerFlag::Expand, 0);

    panel.set_sizer(root, true);
    dialog.set_affirmative_id(ID_OK);
    dialog.set_escape_id(ID_CANCEL);

    let gather_results = Rc::new({
        let languages = languages.clone();
        let radio_menu_state_search = Arc::clone(radio_menu_state);
        move |language_selection: usize, keyword: &str, show_all: bool| {
            let mut results = Vec::<RadioFavorite>::new();
            let stations_by_language = {
                let state = radio_menu_state_search.lock().unwrap();
                state.stations_by_language.clone()
            };

            if let Some((language_code, _)) = languages.get(language_selection)
                && let Some(stations) = stations_by_language.get(language_code)
            {
                let keyword = keyword.trim().to_lowercase();
                for station in stations {
                    let matches = show_all
                        || (!keyword.is_empty()
                            && radio_name_matches_keyword(&station.name, &keyword));
                    if matches {
                        results.push(favorite_from_station(language_code, station));
                    }
                }
            }
            results.sort_by(|left, right| {
                if show_all || keyword.is_empty() {
                    canonical_radio_name(&left.name)
                        .cmp(&canonical_radio_name(&right.name))
                        .then_with(|| left.name.cmp(&right.name))
                } else {
                    radio_search_rank(&left.name, keyword)
                        .cmp(&radio_search_rank(&right.name, keyword))
                        .then_with(|| left.name.cmp(&right.name))
                }
            });
            results.dedup_by(|left, right| {
                canonical_radio_name(&left.name) == canonical_radio_name(&right.name)
            });
            results
        }
    });

    let choice_language_all = choice_language;
    let keyword_ctrl_all = keyword_ctrl;
    let dialog_show_all = dialog;
    let settings_show_all = Arc::clone(settings);
    let radio_menu_state_show_all = Arc::clone(radio_menu_state);
    let gather_results_show_all = Rc::clone(&gather_results);
    button_show_all.on_click(move |_| {
        append_podcast_log("radio_search_dialog.show_all_clicked");
        let selection = choice_language_all.get_selection().unwrap_or(0) as usize;
        let results = gather_results_show_all(selection, &keyword_ctrl_all.get_value(), true);
        open_radio_results_dialog(
            &dialog_show_all,
            &settings_show_all,
            &radio_menu_state_show_all,
            &results,
        );
    });

    let choice_language_search = choice_language;
    let keyword_ctrl_search = keyword_ctrl;
    let dialog_search = dialog;
    let ui_language_search = ui_language.clone();
    let settings_search = Arc::clone(settings);
    let radio_menu_state_search = Arc::clone(radio_menu_state);
    let gather_results_search = Rc::clone(&gather_results);
    let perform_search = Rc::new(move || {
        append_podcast_log("radio_search_dialog.perform_search");
        let selection = choice_language_search.get_selection().unwrap_or(0) as usize;
        let keyword = keyword_ctrl_search.get_value();
        if keyword.trim().is_empty() {
            show_message_subdialog(
                &dialog_search,
                "Radio",
                if ui_language_search == "it" {
                    "Inserisci un testo da cercare."
                } else {
                    "Enter text to search."
                },
            );
            return;
        }
        let results = gather_results_search(selection, &keyword, false);
        open_radio_results_dialog(
            &dialog_search,
            &settings_search,
            &radio_menu_state_search,
            &results,
        );
    });

    let perform_search_button = Rc::clone(&perform_search);
    button_search.on_click(move |_| {
        append_podcast_log("radio_search_dialog.search_clicked");
        perform_search_button();
    });

    let perform_search_enter = Rc::clone(&perform_search);
    keyword_ctrl.on_text_enter(move |_| {
        append_podcast_log("radio_search_dialog.keyword_enter");
        perform_search_enter();
    });

    let dialog_close = dialog;
    button_close.on_click(move |_| {
        append_podcast_log("radio_search_dialog.close_clicked");
        dialog_close.end_modal(ID_CANCEL);
    });

    dialog.centre();
    keyword_ctrl.set_focus();
    println!("DEBUG: Radio Search Dialog v5 - Show Modal");
    append_podcast_log("radio_search_dialog.show_modal_v5");
    dialog.layout();
    dialog.fit();
    panel.layout();

    let start_time = std::time::Instant::now();
    let result_code = dialog.show_modal();
    let elapsed = start_time.elapsed();

    println!(
        "DEBUG: Radio Search Dialog v5 - Returned code={} in {:?}",
        result_code, elapsed
    );
    append_podcast_log(&format!(
        "radio_search_dialog.modal_returned v5 code={} elapsed={:?}",
        result_code, elapsed
    ));

    dialog.destroy();

    // Logica di auto-retry suggerita dall'utente:
    // Se è la prima volta che apriamo il dialogo E si è chiuso in meno di 300ms, riproviamo una volta sola.
    let should_retry = {
        let mut state = radio_menu_state.lock().unwrap();
        let is_first_time = !state.search_ever_opened;
        state.search_ever_opened = true;
        is_first_time && elapsed.as_millis() < 300
    };

    if should_retry {
        println!("DEBUG: First attempt failed (instant close), retrying once...");
        append_podcast_log("radio_search_dialog.auto_retry_triggered");
        // Piccolo ritardo per far respirare il sistema
        std::thread::sleep(std::time::Duration::from_millis(50));
        open_radio_search_dialog(parent, settings, radio_menu_state);
    }
}

fn open_radio_results_dialog(
    parent: &Dialog,
    settings: &Arc<Mutex<Settings>>,
    radio_menu_state: &Arc<Mutex<RadioMenuState>>,
    results: &[RadioFavorite],
) {
    let ui_language = Settings::load().ui_language;
    let mut results = results.to_vec();
    results.sort_by(|left, right| {
        canonical_radio_name(&left.name)
            .cmp(&canonical_radio_name(&right.name))
            .then_with(|| left.name.cmp(&right.name))
            .then_with(|| left.stream_url.cmp(&right.stream_url))
    });
    results.dedup_by(|left, right| {
        canonical_radio_name(&left.name) == canonical_radio_name(&right.name)
            || left.stream_url == right.stream_url
    });

    append_podcast_log(&format!("radio_results_dialog count={}", results.len()));
    for (index, result) in results.iter().take(20).enumerate() {
        append_podcast_log(&format!(
            "radio_results_dialog[{index}] label={} canonical={} url={}",
            radio_label(result),
            canonical_radio_name(&result.name),
            result.stream_url
        ));
    }

    if results.is_empty() {
        show_message_subdialog(
            parent,
            "Radio",
            if ui_language == "it" {
                "Nessuna radio trovata."
            } else {
                "No radio stations found."
            },
        );
        return;
    }

    append_podcast_log("radio_results_dialog.enter");
    let dialog = Dialog::builder(parent, "Risultati radio")
        .with_style(DialogStyle::DefaultDialogStyle | DialogStyle::ResizeBorder)
        .with_size(700, 190)
        .build();
    let panel = Panel::builder(&dialog).build();
    let root = BoxSizer::builder(Orientation::Vertical).build();

    let row = BoxSizer::builder(Orientation::Horizontal).build();
    row.add(
        &StaticText::builder(&panel).with_label("Radio").build(),
        0,
        SizerFlag::AlignCenterVertical | SizerFlag::All,
        5,
    );
    let choice_result = Choice::builder(&panel).build();
    row.add(&choice_result, 1, SizerFlag::Expand | SizerFlag::All, 5);
    root.add_sizer(&row, 0, SizerFlag::Expand, 0);

    let page_row = BoxSizer::builder(Orientation::Horizontal).build();
    let previous_button = Button::builder(&panel)
        .with_label(if ui_language == "it" {
            "Precedenti"
        } else {
            "Previous"
        })
        .build();
    let next_button = Button::builder(&panel)
        .with_label(if ui_language == "it" {
            "Successivi"
        } else {
            "Next"
        })
        .build();
    let page_label = StaticText::builder(&panel).with_label("").build();
    page_row.add(&previous_button, 0, SizerFlag::All, 5);
    page_row.add(
        &page_label,
        1,
        SizerFlag::AlignCenterVertical | SizerFlag::All,
        5,
    );
    page_row.add(&next_button, 0, SizerFlag::All, 5);
    root.add_sizer(&page_row, 0, SizerFlag::Expand, 0);

    let buttons = BoxSizer::builder(Orientation::Horizontal).build();
    let open_button = Button::builder(&panel)
        .with_label(if ui_language == "it" { "Apri" } else { "Open" })
        .build();
    let favorite_button = Button::builder(&panel)
        .with_label(if ui_language == "it" {
            "Aggiungi ai preferiti"
        } else {
            "Add to favorites"
        })
        .build();
    let close_button = Button::builder(&panel)
        .with_id(ID_CANCEL)
        .with_label(if ui_language == "it" {
            "Chiudi"
        } else {
            "Close"
        })
        .build();
    buttons.add_spacer(1);
    buttons.add(&open_button, 0, SizerFlag::All, 10);
    buttons.add(&favorite_button, 0, SizerFlag::All, 10);
    buttons.add(&close_button, 0, SizerFlag::All, 10);
    root.add_sizer(&buttons, 0, SizerFlag::Expand, 0);
    panel.set_sizer(root, true);

    let all_results = Rc::new(results);
    let current_page = Rc::new(RefCell::new(0usize));
    let visible_results = Rc::new(RefCell::new(Vec::<RadioFavorite>::new()));
    let update_results_page = Rc::new({
        let all_results = Rc::clone(&all_results);
        let visible_results = Rc::clone(&visible_results);
        let ui_language = ui_language.clone();
        move |page: usize| {
            let total_pages = all_results.len().div_ceil(RADIO_RESULTS_PAGE_SIZE);
            let current = page.min(total_pages.saturating_sub(1));
            let start = current * RADIO_RESULTS_PAGE_SIZE;
            let end = (start + RADIO_RESULTS_PAGE_SIZE).min(all_results.len());
            let page_results = all_results[start..end].to_vec();
            *visible_results.borrow_mut() = page_results.clone();

            choice_result.clear();
            for result in &page_results {
                choice_result.append(&radio_label(result));
            }
            if !page_results.is_empty() {
                choice_result.set_selection(0);
            }

            page_label.set_label(&if ui_language == "it" {
                format!("Pagina {} di {}", current + 1, total_pages.max(1))
            } else {
                format!("Page {} of {}", current + 1, total_pages.max(1))
            });
            previous_button.enable(current > 0);
            next_button.enable(current + 1 < total_pages);
        }
    });
    update_results_page(0);

    let current_page_previous = Rc::clone(&current_page);
    let update_results_page_previous = Rc::clone(&update_results_page);
    previous_button.on_click(move |_| {
        let next_page = current_page_previous.borrow().saturating_sub(1);
        *current_page_previous.borrow_mut() = next_page;
        update_results_page_previous(next_page);
    });

    let current_page_next = Rc::clone(&current_page);
    let update_results_page_next = Rc::clone(&update_results_page);
    let all_results_next = Rc::clone(&all_results);
    next_button.on_click(move |_| {
        let total_pages = all_results_next.len().div_ceil(RADIO_RESULTS_PAGE_SIZE);
        let next_page = (*current_page_next.borrow() + 1).min(total_pages.saturating_sub(1));
        *current_page_next.borrow_mut() = next_page;
        update_results_page_next(next_page);
    });

    let visible_results_open = Rc::clone(&visible_results);
    let choice_result_open = choice_result;
    let dialog_open = dialog;
    open_button.on_click(move |_| {
        let Some(selection) = choice_result_open.get_selection() else {
            return;
        };
        let visible_results = visible_results_open.borrow();
        let Some(station) = visible_results.get(selection as usize).cloned() else {
            return;
        };
        if let Err(err) = open_radio_station(&dialog_open, &station.name, &station.stream_url) {
            show_message_subdialog(&dialog_open, "Radio", &err);
        }
    });

    let visible_results_favorite = Rc::clone(&visible_results);
    let choice_result_favorite = choice_result;
    let settings_favorite = Arc::clone(settings);
    let radio_menu_state_favorite = Arc::clone(radio_menu_state);
    let dialog_favorite = dialog;
    let ui_language_favorite = ui_language.clone();
    favorite_button.on_click(move |_| {
        let Some(selection) = choice_result_favorite.get_selection() else {
            return;
        };
        let visible_results = visible_results_favorite.borrow();
        let Some(station) = visible_results.get(selection as usize).cloned() else {
            return;
        };
        let station_name = station.name.clone();
        let mut settings = settings_favorite.lock().unwrap();
        if !settings
            .radio_favorites
            .iter()
            .any(|favorite| favorite.stream_url == station.stream_url)
        {
            settings.radio_favorites.push(station);
            normalize_settings_data(&mut settings);
            settings.save();
            drop(settings);
            radio_menu_state_favorite.lock().unwrap().dirty = true;
            let message = if ui_language_favorite == "it" {
                format!("{station_name} aggiunta ai preferiti.")
            } else {
                format!("{station_name} added to favorites.")
            };
            show_message_subdialog(&dialog_favorite, "Radio", &message);
        } else {
            drop(settings);
            show_message_subdialog(
                &dialog_favorite,
                "Radio",
                if ui_language_favorite == "it" {
                    "La radio selezionata è già nei preferiti."
                } else {
                    "The selected radio is already in favorites."
                },
            );
        }
    });

    let dialog_close = dialog;
    close_button.on_click(move |_| {
        dialog_close.end_modal(ID_CANCEL);
    });

    append_podcast_log("radio_results_dialog.show_modal");
    let result_code = dialog.show_modal();
    append_podcast_log(&format!(
        "radio_results_dialog.modal_returned code={result_code}"
    ));
    dialog.destroy();
}

#[cfg(target_os = "macos")]
fn macos_update_build_flavor() -> &'static str {
    option_env!("SONARPAD_MACOS_BUILD_FLAVOR").unwrap_or(if cfg!(target_arch = "aarch64") {
        "apple-silicon"
    } else {
        "intel"
    })
}

#[cfg(target_os = "macos")]
fn expected_macos_release_zip_name() -> &'static str {
    match macos_update_build_flavor() {
        "apple-silicon" => "Sonarpad-macOS-AppleSilicon.zip",
        "intel-catalina" => "Sonarpad-macOS-Intel-Catalina.zip",
        _ => "Sonarpad-macOS-Intel.zip",
    }
}

#[cfg(target_os = "macos")]
fn matching_macos_release_asset(release: &GithubReleaseInfo) -> Option<GithubReleaseAsset> {
    let expected_name = expected_macos_release_zip_name();
    release
        .assets
        .iter()
        .find(|asset| asset.name == expected_name)
        .cloned()
}

#[cfg(target_os = "macos")]
fn prepare_macos_update_install(
    asset: &GithubReleaseAsset,
) -> Result<PendingMacUpdateInstall, String> {
    let work_dir = std::env::temp_dir().join(format!("sonarpad-update-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&work_dir)
        .map_err(|err| format!("creazione cartella update fallita: {err}"))?;
    let zip_path = work_dir.join(&asset.name);

    let client = reqwest::blocking::Client::builder()
        .user_agent("SonarpadMinimalUpdater")
        .build()
        .map_err(|err| format!("creazione client aggiornamenti fallita: {}", err))?;
    let mut response = client
        .get(&asset.browser_download_url)
        .send()
        .and_then(|response| response.error_for_status())
        .map_err(|err| format!("download aggiornamento fallito: {}", err))?;
    let mut file = std::fs::File::create(&zip_path)
        .map_err(|err| format!("creazione archivio update fallita: {err}"))?;
    std::io::copy(&mut response, &mut file)
        .map_err(|err| format!("salvataggio archivio update fallito: {err}"))?;

    let extract_dir = work_dir.join("extracted");
    std::fs::create_dir_all(&extract_dir)
        .map_err(|err| format!("creazione cartella estrazione fallita: {err}"))?;
    let status = Command::new("/usr/bin/ditto")
        .args(["-x", "-k"])
        .arg(&zip_path)
        .arg(&extract_dir)
        .status()
        .map_err(|err| format!("estrazione aggiornamento fallita: {err}"))?;
    if !status.success() {
        return Err(format!(
            "estrazione aggiornamento fallita con codice {:?}",
            status.code()
        ));
    }

    let extracted_app_path = std::fs::read_dir(&extract_dir)
        .map_err(|err| format!("lettura cartella update fallita: {err}"))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| path.extension().and_then(|ext| ext.to_str()) == Some("app"))
        .ok_or_else(|| "app aggiornata non trovata nell'archivio".to_string())?;

    Ok(PendingMacUpdateInstall {
        work_dir,
        extracted_app_path,
    })
}

#[cfg(target_os = "macos")]
fn launch_pending_macos_update_install(
    pending_update: &Arc<Mutex<Option<PendingMacUpdateInstall>>>,
) -> Result<(), String> {
    let Some(pending) = pending_update.lock().unwrap().take() else {
        return Ok(());
    };

    let target_app_path = current_macos_app_bundle_path()?;
    let pid = std::process::id();
    let script_path = pending.work_dir.join("install_update.sh");
    let script = format!(
        "#!/bin/sh\nset -eu\nPID='{pid}'\nTARGET_APP='{target}'\nSOURCE_APP='{source}'\nBACKUP_APP=\"${{TARGET_APP}}.old\"\nfor _ in $(seq 1 300); do\n  if ! kill -0 \"$PID\" 2>/dev/null; then\n    break\n  fi\n  sleep 1\ndone\nrm -rf \"$BACKUP_APP\"\nif [ -d \"$TARGET_APP\" ]; then\n  mv \"$TARGET_APP\" \"$BACKUP_APP\"\nfi\nmv \"$SOURCE_APP\" \"$TARGET_APP\"\nopen \"$TARGET_APP\"\nrm -rf \"$BACKUP_APP\"\n",
        target = target_app_path.display(),
        source = pending.extracted_app_path.display()
    );
    std::fs::write(&script_path, script)
        .map_err(|err| format!("scrittura script aggiornamento fallita: {err}"))?;

    let mut permissions = std::fs::metadata(&script_path)
        .map_err(|err| format!("lettura permessi script fallita: {err}"))?
        .permissions();
    use std::os::unix::fs::PermissionsExt;
    permissions.set_mode(0o755);
    std::fs::set_permissions(&script_path, permissions)
        .map_err(|err| format!("impostazione permessi script fallita: {err}"))?;

    Command::new("/bin/sh")
        .arg(&script_path)
        .spawn()
        .map_err(|err| format!("avvio installazione aggiornamento fallito: {err}"))?;
    Ok(())
}

fn check_for_updates(
    parent: &Frame,
    #[cfg(target_os = "macos")] pending_update: &Arc<Mutex<Option<PendingMacUpdateInstall>>>,
) {
    let ui = current_ui_strings();
    let current_version = env!("CARGO_PKG_VERSION");
    match fetch_latest_release_info() {
        Ok(release) => {
            let latest_version = normalize_version_tag(&release.tag_name);
            if is_newer_version(&latest_version, current_version) {
                let message = if Settings::load().ui_language == "it" {
                    format!(
                        "È disponibile la versione {}.\n\nVuoi scaricarla ora?",
                        latest_version
                    )
                } else {
                    format!(
                        "Version {} is available.\n\nDo you want to download it now?",
                        latest_version
                    )
                };
                let dialog = MessageDialog::builder(parent, &message, &ui.updates_title)
                    .with_style(MessageDialogStyle::YesNo | MessageDialogStyle::IconQuestion)
                    .build();
                localize_standard_dialog_buttons(&dialog);
                if dialog.show_modal() == ID_YES {
                    #[cfg(target_os = "macos")]
                    {
                        match matching_macos_release_asset(&release)
                            .ok_or_else(|| {
                                format!(
                                    "asset aggiornamento non trovato: {}",
                                    expected_macos_release_zip_name()
                                )
                            })
                            .and_then(|asset| prepare_macos_update_install(&asset))
                        {
                            Ok(prepared_update) => {
                                *pending_update.lock().unwrap() = Some(prepared_update);
                                let install_message = if Settings::load().ui_language == "it" {
                                    format!(
                                        "L'aggiornamento {} è pronto.\n\nSonarpad verrà chiuso per completare l'installazione.",
                                        latest_version
                                    )
                                } else {
                                    format!(
                                        "Update {} is ready.\n\nSonarpad will close to complete installation.",
                                        latest_version
                                    )
                                };
                                show_message_dialog(parent, &ui.updates_title, &install_message);
                                parent.close(true);
                            }
                            Err(err) => {
                                show_message_dialog(
                                    parent,
                                    &ui.updates_title,
                                    &if Settings::load().ui_language == "it" {
                                        format!(
                                            "È disponibile la versione {} ma non sono riuscito a preparare l'aggiornamento.\n\n{}",
                                            latest_version, err
                                        )
                                    } else {
                                        format!(
                                            "Version {} is available but I could not prepare the update.\n\n{}",
                                            latest_version, err
                                        )
                                    },
                                );
                            }
                        }
                    }

                    #[cfg(not(target_os = "macos"))]
                    if let Err(err) = open_url_in_browser(SONARPAD_MINIMAL_RELEASES_URL) {
                        show_message_dialog(
                            parent,
                            &ui.updates_title,
                            &if Settings::load().ui_language == "it" {
                                format!(
                                    "È disponibile la versione {} ma non sono riuscito ad aprire il link.\n\n{}",
                                    latest_version, err
                                )
                            } else {
                                format!(
                                    "Version {} is available but I could not open the link.\n\n{}",
                                    latest_version, err
                                )
                            },
                        );
                    }
                }
            } else {
                show_message_dialog(
                    parent,
                    &ui.updates_title,
                    &if Settings::load().ui_language == "it" {
                        format!(
                            "Hai già l'ultima versione installata.\n\nVersione attuale: {}\nUltima versione: {}",
                            current_version, latest_version
                        )
                    } else {
                        format!(
                            "You already have the latest version installed.\n\nCurrent version: {}\nLatest version: {}",
                            current_version, latest_version
                        )
                    },
                );
            }
        }
        Err(err) => {
            show_message_dialog(
                parent,
                &ui.updates_title,
                &if Settings::load().ui_language == "it" {
                    format!(
                        "Controllo aggiornamenti non riuscito.\n\nVersione attuale: {}\nErrore: {}",
                        current_version, err
                    )
                } else {
                    format!(
                        "Update check failed.\n\nCurrent version: {}\nError: {}",
                        current_version, err
                    )
                },
            );
        }
    }
}

fn set_progress_cancel_label(progress: &ProgressDialog) {
    if let Some(button) = progress.find_window_by_id(ID_CANCEL) {
        button.set_label(&current_ui_strings().cancel);
    }
    if let Some(button) = progress.find_window_by_id(ID_OK) {
        button.set_label(&current_ui_strings().cancel);
    }
}

fn set_progress_close_label(progress: &ProgressDialog) {
    if let Some(button) = progress.find_window_by_id(ID_CANCEL) {
        button.set_label(&current_ui_strings().close);
    }
    if let Some(button) = progress.find_window_by_id(ID_OK) {
        button.set_label(&current_ui_strings().close);
    }
}

fn localize_standard_dialog_buttons(dialog: &impl WxWidget) {
    let ui = current_ui_strings();

    if let Some(button) = dialog.find_window_by_id(ID_OK) {
        button.set_label(&ui.ok);
    }
    if let Some(button) = dialog.find_window_by_id(ID_CANCEL) {
        button.set_label(&ui.close);
    }
    if let Some(button) = dialog.find_window_by_id(ID_NO) {
        button.set_label(&ui.close);
    }
    if let Some(button) = dialog.find_window_by_id(ID_YES) {
        button.set_label(&ui.yes);
    }
}

fn percent_encode(input: &str) -> String {
    url::form_urlencoded::byte_serialize(input.as_bytes()).collect()
}

fn build_google_news_rss_url(keyword: &str) -> String {
    let query = percent_encode(keyword.trim());
    format!("https://news.google.com/rss/search?q={query}&hl=it&gl=IT&ceid=IT:it")
}

fn sanitize_filename(name: &str) -> String {
    sanitize_filename_candidate(name).unwrap_or_else(|| "podcast".to_string())
}

fn sanitize_filename_candidate(name: &str) -> Option<String> {
    let invalid_chars = ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
    let cleaned = name
        .chars()
        .map(|ch| {
            if ch.is_control() || invalid_chars.contains(&ch) {
                '_'
            } else {
                ch
            }
        })
        .collect::<String>();
    let trimmed = cleaned.trim().trim_matches('.').trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn first_line_filename_candidate(text: &str) -> Option<String> {
    text.lines().next().and_then(sanitize_filename_candidate)
}

fn format_google_news_source_title(keyword: &str) -> String {
    keyword
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            if let Some(first) = chars.next() {
                let mut out = String::new();
                out.extend(first.to_uppercase());
                for ch in chars {
                    out.extend(ch.to_lowercase());
                }
                out
            } else {
                String::new()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn looks_like_article_source_url(input: &str) -> bool {
    let trimmed = input.trim();
    trimmed.starts_with("http://")
        || trimmed.starts_with("https://")
        || trimmed.starts_with("www.")
        || trimmed.starts_with("//")
        || trimmed.contains('/')
        || trimmed.contains('.')
}

fn articles_source_menu_id(source_index: usize) -> i32 {
    ID_ARTICLES_SOURCE_BASE + source_index as i32
}

fn article_folder_dialog_menu_id(folder_index: usize) -> i32 {
    ID_ARTICLE_FOLDER_DIALOG_BASE + folder_index as i32
}

fn decode_article_folder_dialog_menu_id(menu_id: i32) -> Option<usize> {
    if !(ID_ARTICLE_FOLDER_DIALOG_BASE..ID_ARTICLE_SOURCE_DIALOG_BASE).contains(&menu_id) {
        return None;
    }
    Some((menu_id - ID_ARTICLE_FOLDER_DIALOG_BASE) as usize)
}

fn article_source_dialog_menu_id(source_index: usize) -> i32 {
    ID_ARTICLE_SOURCE_DIALOG_BASE + source_index as i32
}

fn decode_article_source_dialog_menu_id(menu_id: i32) -> Option<usize> {
    if !(ID_ARTICLE_SOURCE_DIALOG_BASE..ID_ARTICLES_ARTICLE_BASE).contains(&menu_id) {
        return None;
    }
    Some((menu_id - ID_ARTICLE_SOURCE_DIALOG_BASE) as usize)
}

fn articles_article_menu_id(source_index: usize, item_index: usize) -> i32 {
    ID_ARTICLES_ARTICLE_BASE
        + (source_index as i32 * MAX_MENU_ARTICLES_PER_SOURCE as i32)
        + item_index as i32
}

fn decode_article_menu_id(menu_id: i32) -> Option<(usize, usize)> {
    if menu_id < ID_ARTICLES_ARTICLE_BASE {
        return None;
    }
    let offset = (menu_id - ID_ARTICLES_ARTICLE_BASE) as usize;
    let source_index = offset / MAX_MENU_ARTICLES_PER_SOURCE;
    let item_index = offset % MAX_MENU_ARTICLES_PER_SOURCE;
    Some((source_index, item_index))
}

fn show_article_item(
    item: &articles::ArticleItem,
    rt: &Arc<Runtime>,
    text_ctrl: &TextCtrl,
    podcast_playback: &Rc<RefCell<PodcastPlaybackState>>,
) {
    append_podcast_log(&format!(
        "article_menu.show_item.begin title={} link={}",
        item.title, item.link
    ));
    match rt.block_on(articles::fetch_article_text(item)) {
        Ok(text) if !text.trim().is_empty() => {
            podcast_playback.borrow_mut().selected_episode = None;
            text_ctrl.set_value(&text);
            append_podcast_log(&format!(
                "article_menu.show_item.applied title={} chars={}",
                item.title,
                text.chars().count()
            ));
        }
        Ok(_) | Err(_) => {
            append_podcast_log(&format!(
                "article_menu.keep_existing_text title={} link={}",
                item.title, item.link
            ));
        }
    }
}

fn podcasts_source_menu_id(source_index: usize) -> i32 {
    ID_PODCASTS_SOURCE_BASE + source_index as i32
}

fn podcasts_episode_menu_id(source_index: usize, episode_index: usize) -> i32 {
    ID_PODCASTS_EPISODE_BASE
        + (source_index as i32 * MAX_MENU_PODCAST_EPISODES_PER_SOURCE as i32)
        + episode_index as i32
}

fn decode_podcast_episode_menu_id(menu_id: i32) -> Option<(usize, usize)> {
    if menu_id < ID_PODCASTS_EPISODE_BASE {
        return None;
    }
    let offset = (menu_id - ID_PODCASTS_EPISODE_BASE) as usize;
    let source_index = offset / MAX_MENU_PODCAST_EPISODES_PER_SOURCE;
    let episode_index = offset % MAX_MENU_PODCAST_EPISODES_PER_SOURCE;
    Some((source_index, episode_index))
}

fn podcasts_category_podcast_menu_id(category_index: usize, result_index: usize) -> i32 {
    ID_PODCASTS_CATEGORY_PODCAST_BASE + (category_index as i32 * 100) + result_index as i32
}

fn decode_podcast_category_podcast_menu_id(menu_id: i32) -> Option<(usize, usize)> {
    let max_menu_id = ID_PODCASTS_CATEGORY_PODCAST_BASE
        + (podcasts::apple_categories(&Settings::load().ui_language).len() as i32 * 100);
    if menu_id < ID_PODCASTS_CATEGORY_PODCAST_BASE || menu_id >= max_menu_id {
        return None;
    }
    let offset = (menu_id - ID_PODCASTS_CATEGORY_PODCAST_BASE) as usize;
    Some((offset / 100, offset % 100))
}

fn app_storage_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME").map(|home| {
            PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join(APP_STORAGE_DIR_NAME)
        })
    }

    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

fn app_storage_path(file_name: &str) -> PathBuf {
    app_storage_dir()
        .map(|dir| dir.join(file_name))
        .unwrap_or_else(|| PathBuf::from(file_name))
}

fn read_app_storage_text(file_name: &str) -> Option<String> {
    let storage_path = app_storage_path(file_name);
    if let Ok(data) = std::fs::read_to_string(&storage_path) {
        return Some(data);
    }

    let legacy_path = PathBuf::from(file_name);
    if legacy_path != storage_path {
        return std::fs::read_to_string(legacy_path).ok();
    }

    None
}

fn write_app_storage_text(file_name: &str, data: &str) -> Result<(), String> {
    let storage_path = app_storage_path(file_name);
    if let Some(parent) = storage_path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("creazione cartella {} fallita: {}", parent.display(), err))?;
    }

    std::fs::write(&storage_path, data)
        .map_err(|err| format!("scrittura file {} fallita: {}", storage_path.display(), err))
}

#[cfg(any(target_os = "macos", windows))]
fn primary_podcast_log_path() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        return app_storage_dir().map(|dir| dir.join("log.txt"));
    }

    #[cfg(windows)]
    {
        std::env::var_os("USERPROFILE").map(|home| {
            PathBuf::from(home)
                .join("Documents")
                .join("Sonarpad")
                .join("log.txt")
        })
    }
}

#[cfg(any(target_os = "macos", windows))]
fn append_podcast_log(message: &str) {
    let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
    let line = format!("[{timestamp}] {message}\n");
    let Some(primary_path) = primary_podcast_log_path() else {
        println!("ERROR: Cartella documenti non disponibile per il log podcast");
        return;
    };
    let fallback_path = PathBuf::from("log.txt");

    let primary_failure_reason = if let Some(parent) = primary_path.parent()
        && !parent.as_os_str().is_empty()
    {
        if let Err(err) = std::fs::create_dir_all(parent) {
            println!(
                "ERROR: Creazione cartella log podcast {} fallita: {}",
                parent.display(),
                err
            );
            Some(format!(
                "path={} create_dir_all_failed={}",
                primary_path.display(),
                err
            ))
        } else {
            None
        }
    } else {
        None
    };

    let primary_failure_reason = if let Some(reason) = primary_failure_reason {
        Some(reason)
    } else {
        match std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&primary_path)
        {
            Ok(mut file) => {
                use std::io::Write;

                if let Err(err) = file.write_all(line.as_bytes()) {
                    println!(
                        "ERROR: Scrittura log podcast {} fallita: {}",
                        primary_path.display(),
                        err
                    );
                    Some(format!(
                        "path={} write_failed={}",
                        primary_path.display(),
                        err
                    ))
                } else {
                    return;
                }
            }
            Err(err) => {
                println!(
                    "ERROR: Apertura log podcast {} fallita: {}",
                    primary_path.display(),
                    err
                );
                Some(format!(
                    "path={} open_failed={}",
                    primary_path.display(),
                    err
                ))
            }
        }
    };

    let Some(primary_failure_reason) = primary_failure_reason else {
        return;
    };

    if let Some(parent) = fallback_path.parent()
        && !parent.as_os_str().is_empty()
        && let Err(err) = std::fs::create_dir_all(parent)
    {
        println!(
            "ERROR: Creazione cartella log podcast {} fallita: {}",
            parent.display(),
            err
        );
        println!("ERROR: Nessun log podcast scritto: {message}");
        return;
    }

    match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&fallback_path)
    {
        Ok(mut file) => {
            use std::io::Write;

            if let Err(err) = file.write_all(line.as_bytes()) {
                println!(
                    "ERROR: Scrittura log podcast {} fallita: {}",
                    fallback_path.display(),
                    err
                );
                println!("ERROR: Nessun log podcast scritto: {message}");
                return;
            }
            let diagnostic_line =
                format!("[{timestamp}] primary_log_path_failed {primary_failure_reason}\n");
            if let Err(err) = file.write_all(diagnostic_line.as_bytes()) {
                println!(
                    "ERROR: Scrittura log diagnostico {} fallita: {}",
                    fallback_path.display(),
                    err
                );
            }
        }
        Err(err) => {
            println!(
                "ERROR: Apertura log podcast {} fallita: {}",
                fallback_path.display(),
                err
            );
            println!("ERROR: Nessun log podcast scritto: {message}");
        }
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
fn append_podcast_log(_message: &str) {}

fn log_podcast_player_snapshot(
    player: &podcast_player::PodcastPlayer,
    context: &str,
    audio_url: &str,
) {
    match player.debug_snapshot() {
        Ok(snapshot) => append_podcast_log(&format!("{context} audio_url={audio_url} {snapshot}")),
        Err(err) => append_podcast_log(&format!(
            "{context} audio_url={audio_url} snapshot_error={err}"
        )),
    }
}

fn wait_for_podcast_ready(
    parent: &Frame,
    player: &podcast_player::PodcastPlayer,
    audio_url: &str,
) -> bool {
    let ui = current_ui_strings();
    let progress = ProgressDialog::builder(
        parent,
        &ui.podcast_loading_title,
        &ui.podcast_download_start,
        100,
    )
    .with_style(ProgressDialogStyle::CanAbort | ProgressDialogStyle::Smooth)
    .build();
    set_progress_cancel_label(&progress);

    for step in 0..=40 {
        let percent = (step * 100) / 40;
        let message = format!("{} {}%", ui.loading_podcasts, percent);
        let continue_running = progress.update(percent, Some(&message));
        set_progress_cancel_label(&progress);
        if !continue_running {
            append_podcast_log(&format!("podcast_ready.cancelled audio_url={audio_url}"));
            return false;
        }

        match player.is_ready_for_playback() {
            Ok(true) => {
                log_podcast_player_snapshot(player, "podcast_ready.success", audio_url);
                progress.update(100, Some(&ui.podcast_ready));
                set_progress_close_label(&progress);
                return true;
            }
            Ok(false) => {
                log_podcast_player_snapshot(player, "podcast_ready.waiting", audio_url);
            }
            Err(err) => {
                append_podcast_log(&format!(
                    "podcast_ready.snapshot_error audio_url={} error={}",
                    audio_url, err
                ));
                return false;
            }
        }

        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    log_podcast_player_snapshot(player, "podcast_ready.timeout", audio_url);
    false
}

#[cfg(any(target_os = "macos", windows))]
fn podcast_external_open_dir() -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join("Sonarpad");
    std::fs::create_dir_all(&dir)
        .map_err(|err| format!("creazione cartella download podcast fallita: {}", err))?;
    Ok(dir)
}

#[cfg(any(target_os = "macos", windows))]
fn podcast_extension_from_url(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    let last_segment = parsed
        .path_segments()
        .and_then(|mut segments| segments.rfind(|segment| !segment.is_empty()))?;
    let extension = Path::new(last_segment).extension()?.to_str()?.trim();
    if extension.is_empty() {
        None
    } else {
        Some(extension.to_ascii_lowercase())
    }
}

#[cfg(any(target_os = "macos", windows))]
fn podcast_extension_from_content_type(content_type: Option<&str>) -> &'static str {
    match content_type
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "audio/mp4" | "audio/x-m4a" | "audio/m4a" => "m4a",
        "audio/mpeg" | "audio/mp3" => "mp3",
        "audio/aac" | "audio/aacp" => "aac",
        "audio/wav" | "audio/x-wav" | "audio/wave" => "wav",
        "audio/ogg" | "application/ogg" => "ogg",
        "audio/flac" | "audio/x-flac" => "flac",
        _ => "mp3",
    }
}

#[cfg(any(target_os = "macos", windows))]
#[derive(Clone, Default)]
struct PodcastExternalDownloadState {
    downloaded_bytes: u64,
    total_bytes: Option<u64>,
    abort_requested: bool,
    result: Option<Result<PathBuf, String>>,
}

#[cfg(any(target_os = "macos", windows))]
fn open_podcast_download_response(
    client: &reqwest::blocking::Client,
    url: &str,
    downloaded_bytes: u64,
) -> Result<reqwest::blocking::Response, String> {
    let mut request = client.get(url).header(
        reqwest::header::USER_AGENT,
        "Mozilla/5.0 (Macintosh; Intel Mac OS X 13_0) AppleWebKit/605.1.15 (KHTML, like Gecko)",
    );
    if downloaded_bytes > 0 {
        request = request.header(reqwest::header::RANGE, format!("bytes={downloaded_bytes}-"));
    }

    let response = request
        .send()
        .map_err(|err| format!("download podcast fallito: {}", err))?;
    let status = response.status();
    if !status.is_success() && status != reqwest::StatusCode::PARTIAL_CONTENT {
        return Err(format!(
            "download podcast fallito: HTTP {}",
            status.as_u16()
        ));
    }
    if downloaded_bytes > 0 && status != reqwest::StatusCode::PARTIAL_CONTENT {
        return Err("il server non supporta la ripresa del download podcast".to_string());
    }
    Ok(response)
}

#[cfg(any(target_os = "macos", windows))]
fn download_podcast_episode_for_external_open(
    url: &str,
    state: &Arc<Mutex<PodcastExternalDownloadState>>,
) {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        let mut locked = state.lock().unwrap();
        locked.result = Some(Err("URL episodio podcast vuoto".to_string()));
        return;
    }

    let outcome = (|| -> Result<PathBuf, String> {
        let client = reqwest::blocking::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(30))
            .timeout(std::time::Duration::from_secs(900))
            .build()
            .map_err(|err| format!("inizializzazione download podcast fallita: {}", err))?;

        let mut response = open_podcast_download_response(&client, trimmed, 0)?;
        let total_bytes = response.content_length();
        state.lock().unwrap().total_bytes = total_bytes;
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok());
        let extension = podcast_extension_from_url(response.url().as_str())
            .or_else(|| podcast_extension_from_url(trimmed))
            .unwrap_or_else(|| podcast_extension_from_content_type(content_type).to_string());
        let file_path = podcast_external_open_dir()?.join(format!(
            "podcast-{}.{}",
            Uuid::new_v4().simple(),
            extension
        ));

        let mut file = std::fs::File::create(&file_path)
            .map_err(|err| format!("creazione file podcast fallita: {}", err))?;
        let mut downloaded_bytes = 0_u64;
        let mut buffer = [0_u8; 64 * 1024];
        let mut resume_attempts = 0_u8;

        loop {
            if state.lock().unwrap().abort_requested {
                if let Err(err) = std::fs::remove_file(&file_path) {
                    append_podcast_log(&format!(
                        "external_download.cleanup_error path={} error={}",
                        file_path.display(),
                        err
                    ));
                }
                return Err("scaricamento podcast annullato".to_string());
            }

            let read = match response.read(&mut buffer) {
                Ok(read) => {
                    resume_attempts = 0;
                    read
                }
                Err(err) if downloaded_bytes > 0 && resume_attempts < 15 => {
                    resume_attempts += 1;
                    append_podcast_log(&format!(
                        "external_download.resume_attempt url={} bytes={} attempt={} error={}",
                        trimmed, downloaded_bytes, resume_attempts, err
                    ));
                    response = open_podcast_download_response(&client, trimmed, downloaded_bytes)?;
                    if let Some(remaining_bytes) = response.content_length() {
                        state.lock().unwrap().total_bytes =
                            Some(downloaded_bytes + remaining_bytes);
                    }
                    continue;
                }
                Err(err) => return Err(format!("lettura download podcast fallita: {}", err)),
            };
            if read == 0 {
                break;
            }

            file.write_all(&buffer[..read])
                .map_err(|err| format!("scrittura file podcast fallita: {}", err))?;
            downloaded_bytes += read as u64;

            state.lock().unwrap().downloaded_bytes = downloaded_bytes;
        }

        file.flush()
            .map_err(|err| format!("finalizzazione file podcast fallita: {}", err))?;
        append_podcast_log(&format!(
            "external_download.success url={} path={} bytes={}",
            trimmed,
            file_path.display(),
            downloaded_bytes
        ));
        Ok(file_path)
    })();

    state.lock().unwrap().result = Some(outcome);
}

#[cfg(any(target_os = "macos", windows))]
fn open_podcast_episode_externally(
    parent: &Frame,
    url: &str,
    suggested_name: &str,
) -> Result<(), String> {
    let file_path = download_podcast_episode_with_progress(parent, url, "external_open")?;

    match prompt_downloaded_podcast_action(parent) {
        PodcastDownloadAction::Open => open_downloaded_podcast_file(&file_path),
        PodcastDownloadAction::SaveAs => {
            save_downloaded_podcast_file(parent, &file_path, suggested_name)
        }
        PodcastDownloadAction::Close => Ok(()),
    }
}

#[cfg(any(target_os = "macos", windows))]
fn save_podcast_episode(parent: &Frame, url: &str, suggested_name: &str) -> Result<(), String> {
    let file_path = download_podcast_episode_with_progress(parent, url, "podcast_save")?;
    save_downloaded_podcast_file(parent, &file_path, suggested_name)
}

#[cfg(any(target_os = "macos", windows))]
fn download_podcast_episode_with_progress(
    parent: &Frame,
    url: &str,
    log_prefix: &str,
) -> Result<PathBuf, String> {
    let ui = current_ui_strings();
    append_podcast_log(&format!("{log_prefix}.begin url={}", url.trim()));
    let progress = ProgressDialog::builder(
        parent,
        &ui.podcast_download_title,
        &ui.podcast_download_start,
        100,
    )
    .with_style(ProgressDialogStyle::CanAbort | ProgressDialogStyle::Smooth)
    .build();
    set_progress_cancel_label(&progress);

    let state = Arc::new(Mutex::new(PodcastExternalDownloadState::default()));
    let state_thread = Arc::clone(&state);
    let url_owned = url.trim().to_string();
    append_podcast_log(&format!("{log_prefix}.spawn_download url={url_owned}"));
    std::thread::spawn(move || {
        download_podcast_episode_for_external_open(&url_owned, &state_thread);
    });

    let mut fallback_percent = 0_i32;
    let mut last_logged_percent = -1_i32;
    let file_path = loop {
        std::thread::sleep(std::time::Duration::from_millis(100));

        let snapshot = state.lock().unwrap().clone();
        if let Some(result) = snapshot.result {
            let file_path = result?;
            append_podcast_log(&format!(
                "{log_prefix}.download_completed path={}",
                file_path.display()
            ));
            progress.destroy();
            break file_path;
        }

        let (percent, message) =
            if let Some(total_bytes) = snapshot.total_bytes.filter(|size| *size > 0) {
                let percent =
                    ((snapshot.downloaded_bytes.saturating_mul(100)) / total_bytes).min(99) as i32;
                let downloaded_mb = snapshot.downloaded_bytes as f64 / (1024.0 * 1024.0);
                let total_mb = total_bytes as f64 / (1024.0 * 1024.0);
                (
                    percent,
                    format!(
                        "Scaricamento podcast... {:.1}/{:.1} MB",
                        downloaded_mb, total_mb
                    ),
                )
            } else {
                fallback_percent = (fallback_percent + 2).min(99);
                let downloaded_mb = snapshot.downloaded_bytes as f64 / (1024.0 * 1024.0);
                (
                    fallback_percent,
                    format!("{} {:.1} MB", ui.loading_podcasts, downloaded_mb),
                )
            };

        if percent / 10 > last_logged_percent / 10 {
            last_logged_percent = percent;
            append_podcast_log(&format!(
                "{log_prefix}.progress percent={} downloaded_bytes={} total_bytes={:?}",
                percent, snapshot.downloaded_bytes, snapshot.total_bytes
            ));
        }

        let continue_running = progress.update(percent, Some(&message));
        set_progress_cancel_label(&progress);
        if !continue_running {
            append_podcast_log(&format!("{log_prefix}.cancelled_by_user"));
            state.lock().unwrap().abort_requested = true;
            return Err("scaricamento podcast annullato".to_string());
        }
    };

    Ok(file_path)
}

#[cfg(any(target_os = "macos", windows))]
fn open_downloaded_podcast_file(file_path: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        return open_podcast_file_with_mpv(file_path);
    }

    #[cfg(windows)]
    {
        open_podcast_file_with_default_app(file_path)
    }
}

#[cfg(any(target_os = "macos", windows))]
fn open_podcast_file_with_default_app(file_path: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let status = std::process::Command::new("/usr/bin/open")
        .arg(file_path)
        .status()
        .map_err(|err| format!("avvio app predefinita fallito: {}", err))?;

    #[cfg(windows)]
    let file_path_string = file_path.display().to_string();

    #[cfg(windows)]
    let status = std::process::Command::new("cmd")
        .args(["/C", "start", "", &file_path_string])
        .status()
        .map_err(|err| format!("avvio app predefinita fallito: {}", err))?;

    if status.success() {
        append_podcast_log(&format!(
            "external_open.success path={}",
            file_path.display()
        ));
        Ok(())
    } else {
        Err(format!(
            "apertura file podcast fallita con codice {:?}",
            status.code()
        ))
    }
}

#[cfg(target_os = "macos")]
fn open_podcast_file_with_mpv(file_path: &Path) -> Result<(), String> {
    let mpv_executable =
        podcast_player::bundled_mpv_executable_path().unwrap_or_else(|| PathBuf::from("mpv"));
    let mut command = std::process::Command::new(&mpv_executable);
    if let Some(parent_dir) = mpv_executable.parent()
        && !parent_dir.as_os_str().is_empty()
    {
        command.current_dir(parent_dir);
    }

    let status = command
        .arg(file_path)
        .arg("--no-config")
        .arg("--no-video")
        .arg("--force-window=yes")
        .arg("--osc=yes")
        .arg("--input-default-bindings=yes")
        .arg("--title=Sonarpad podcast")
        .status()
        .map_err(|err| format!("avvio mpv podcast fallito: {}", err))?;

    if status.success() {
        append_podcast_log(&format!(
            "external_open.success path={}",
            file_path.display()
        ));
        Ok(())
    } else {
        Err(format!(
            "apertura file podcast con mpv fallita con codice {:?}",
            status.code()
        ))
    }
}

fn load_cached_voices() -> Option<Vec<edge_tts::VoiceInfo>> {
    let data = read_app_storage_text("voices_cache.json")?;
    serde_json::from_str(&data).ok()
}

fn save_cached_voices(voices: &[edge_tts::VoiceInfo]) {
    if let Ok(data) = serde_json::to_string_pretty(voices)
        && let Err(err) = write_app_storage_text("voices_cache.json", &data)
    {
        println!("ERROR: Salvataggio cache voci fallito: {}", err);
    }
}

fn build_language_list(voices: &[edge_tts::VoiceInfo], ui_language: &str) -> Vec<(String, String)> {
    let mut l_map = BTreeMap::new();
    for voice in voices {
        l_map.insert(
            if ui_language == "en" {
                get_language_name_en(&voice.locale)
            } else {
                get_language_name_it(&voice.locale)
            },
            voice.locale.clone(),
        );
    }
    l_map.into_iter().collect()
}

fn normalize_settings_data(settings: &mut Settings) {
    if settings.article_sources.is_empty() {
        settings.article_sources = articles::default_sources_for_ui_language(&settings.ui_language);
    }
    for source in &mut settings.article_sources {
        source.url = articles::normalize_url(&source.url);
        source.folder_path = normalize_article_folder_path(&source.folder_path);
        if source.title.trim().is_empty() {
            source.title = source.url.clone();
        }
    }
    settings.article_folders =
        normalized_article_folders(&settings.article_folders, &settings.article_sources);
    settings
        .article_sources
        .retain(|source| !is_removed_default_article_source(&source.url));
    for source in &mut settings.podcast_sources {
        source.url = podcasts::normalize_url(&source.url);
        if source.title.trim().is_empty() {
            source.title = source.url.clone();
        }
    }
    for favorite in &mut settings.radio_favorites {
        favorite.language_code = parse_language_code(&favorite.language_code)
            .unwrap_or_else(|| favorite.language_code.trim().to_lowercase());
        favorite.name = favorite
            .name
            .replace('&', "")
            .replace('\t', " ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        favorite.stream_url = normalize_radio_stream_url(&favorite.name, &favorite.stream_url);
    }
    settings.radio_favorites.retain(|favorite| {
        !favorite.name.trim().is_empty() && !favorite.stream_url.trim().is_empty()
    });
    let mut seen_stream_urls = HashSet::new();
    settings
        .radio_favorites
        .retain(|favorite| seen_stream_urls.insert(favorite.stream_url.clone()));
}

fn is_removed_default_article_source(url: &str) -> bool {
    matches!(
        articles::normalize_url(url).as_str(),
        "https://www.ilpost.it/feed/"
            | "https://www.fanpage.it/feed/"
            | "https://www.internazionale.it/rss"
            | "https://www.affaritaliani.it/static/rss/rssGadget.aspx?idchannel=1"
            | "https://www.hwupgrade.it/rss/news.xml"
            | "https://www.startmag.it/feed/"
    )
}

fn normalize_article_folder_path(folder_path: &str) -> String {
    folder_path
        .split('/')
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join("/")
}

fn article_folder_segments(folder_path: &str) -> Vec<&str> {
    folder_path
        .split('/')
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .collect()
}

fn article_folder_display_name(ui: &UiStrings, folder_path: &str) -> String {
    if folder_path.trim().is_empty() {
        ui.root_folder_name.clone()
    } else {
        folder_path
            .rsplit('/')
            .next()
            .unwrap_or(folder_path)
            .trim()
            .to_string()
    }
}

fn article_parent_folder(folder_path: &str) -> Option<String> {
    let folder_path = normalize_article_folder_path(folder_path);
    if folder_path.is_empty() {
        None
    } else if let Some((parent, _)) = folder_path.rsplit_once('/') {
        Some(parent.to_string())
    } else {
        Some(String::new())
    }
}

fn push_article_folder_path(all_folders: &mut Vec<String>, folder_path: &str) {
    let normalized = normalize_article_folder_path(folder_path);
    if normalized.is_empty() {
        return;
    }

    let mut current = String::new();
    for segment in article_folder_segments(&normalized) {
        if current.is_empty() {
            current = segment.to_string();
        } else {
            current.push('/');
            current.push_str(segment);
        }
        if !all_folders.iter().any(|existing| existing == &current) {
            all_folders.push(current.clone());
        }
    }
}

fn normalized_article_folders(
    explicit_folders: &[String],
    sources: &[articles::ArticleSource],
) -> Vec<String> {
    let mut all_folders = Vec::new();
    for folder in explicit_folders {
        push_article_folder_path(&mut all_folders, folder);
    }
    for source in sources {
        push_article_folder_path(&mut all_folders, &source.folder_path);
    }
    all_folders
}

fn current_article_folder_children(
    current_folder: &str,
    folders: &[String],
    sources: &[articles::ArticleSource],
) -> Vec<String> {
    let current_folder = normalize_article_folder_path(current_folder);
    let mut child_folders = normalized_article_folders(folders, sources)
        .into_iter()
        .filter(|folder| match article_parent_folder(folder) {
            Some(parent) => parent == current_folder,
            None => current_folder.is_empty(),
        })
        .collect::<Vec<_>>();
    child_folders.sort_by_key(|folder| normalized_sort_key(folder));
    child_folders
}

fn sanitize_dynamic_menu_label(label: &str, fallback: &str) -> String {
    let normalized = label.split_whitespace().collect::<Vec<_>>().join(" ");
    let escaped = normalized.replace('&', "&&");
    if escaped.replace('&', "").trim().is_empty() {
        fallback.replace('&', "&&")
    } else {
        escaped
    }
}

#[derive(Clone)]
struct ImportedArticleSource {
    title: String,
    url: String,
    folder_path: String,
}

enum ArticleFolderDialogEntry {
    Folder(String),
    Source(usize),
}

fn build_article_source_submenu(
    source_index: usize,
    source: &articles::ArticleSource,
    loading_urls: &HashSet<String>,
    ui: &UiStrings,
) -> Menu {
    let submenu = Menu::builder().build();
    if source.items.is_empty() {
        let placeholder_id = articles_source_menu_id(source_index);
        let placeholder_label = if loading_urls.contains(&source.url) {
            &ui.loading_articles
        } else {
            &ui.no_articles_available
        };
        let placeholder_help = if loading_urls.contains(&source.url) {
            &ui.wait_loading_articles
        } else {
            &ui.refresh_source_for_articles
        };
        let _ = submenu.append(
            placeholder_id,
            placeholder_label,
            placeholder_help,
            ItemKind::Normal,
        );
        let _ = submenu.enable_item(placeholder_id, false);
    } else {
        for (item_index, item) in source
            .items
            .iter()
            .take(MAX_MENU_ARTICLES_PER_SOURCE)
            .enumerate()
        {
            let item_label = sanitize_dynamic_menu_label(&item.title, &item.link);
            let _ = submenu.append(
                articles_article_menu_id(source_index, item_index),
                &item_label,
                &item.link,
                ItemKind::Normal,
            );
        }
    }
    submenu
}

fn article_folder_dialog_entries(
    current_folder: &str,
    folders: &[String],
    sources: &[articles::ArticleSource],
    ui: &UiStrings,
) -> Vec<(String, ArticleFolderDialogEntry)> {
    let mut entries = current_article_folder_children(current_folder, folders, sources)
        .into_iter()
        .map(|folder_path| {
            (
                article_folder_display_name(ui, &folder_path),
                ArticleFolderDialogEntry::Folder(folder_path),
            )
        })
        .collect::<Vec<_>>();

    entries.extend(
        sources
            .iter()
            .enumerate()
            .filter(|(_, source)| {
                normalize_article_folder_path(&source.folder_path) == current_folder
            })
            .map(|(source_index, source)| {
                (
                    article_source_label(source),
                    ArticleFolderDialogEntry::Source(source_index),
                )
            }),
    );

    entries
}

fn article_folder_catalog(folders: &[String], sources: &[articles::ArticleSource]) -> Vec<String> {
    let mut all_folders = normalized_article_folders(folders, sources);
    all_folders.sort_by_key(|folder| normalized_sort_key(folder));
    all_folders
}

fn build_article_folder_submenu(
    folder_path: &str,
    folders: &[String],
    sources: &[articles::ArticleSource],
    ui: &UiStrings,
) -> Menu {
    let submenu = Menu::builder().build();
    let folder_catalog = article_folder_catalog(folders, sources);

    for child_folder in current_article_folder_children(folder_path, folders, sources) {
        if let Some(folder_index) = folder_catalog
            .iter()
            .position(|folder| folder == &child_folder)
        {
            let label = sanitize_dynamic_menu_label(
                &article_folder_display_name(ui, &child_folder),
                &ui.folder_label,
            );
            let _ = submenu.append(
                article_folder_dialog_menu_id(folder_index),
                &label,
                &child_folder,
                ItemKind::Normal,
            );
        }
    }

    for (source_index, source) in sources
        .iter()
        .enumerate()
        .filter(|(_, source)| normalize_article_folder_path(&source.folder_path) == folder_path)
    {
        let label = sanitize_dynamic_menu_label(&article_source_label(source), &source.url);
        let _ = submenu.append(
            article_source_dialog_menu_id(source_index),
            &label,
            &source.url,
            ItemKind::Normal,
        );
    }

    if submenu.get_menu_items().is_empty() {
        let _ = submenu.append(
            ID_ARTICLES_SOURCE_BASE - 1,
            &ui.folder_empty,
            &ui.folder_empty,
            ItemKind::Normal,
        );
        let _ = submenu.enable_item(ID_ARTICLES_SOURCE_BASE - 1, false);
    }

    submenu
}

fn open_article_source_items_dialog(
    parent: &Frame,
    source: &articles::ArticleSource,
    source_index: usize,
    loading_urls: &HashSet<String>,
) -> Option<articles::ArticleItem> {
    let ui = current_ui_strings();
    if source.items.is_empty() {
        let message = if loading_urls.contains(&source.url) {
            ui.wait_loading_articles.clone()
        } else {
            ui.no_articles_available.clone()
        };
        show_message_dialog(parent, &article_source_label(source), &message);
        return None;
    }

    let dialog = Dialog::builder(parent, &article_source_label(source))
        .with_style(DialogStyle::DefaultDialogStyle | DialogStyle::ResizeBorder)
        .with_size(620, 180)
        .build();
    let panel = Panel::builder(&dialog).build();
    let root = BoxSizer::builder(Orientation::Vertical).build();

    let row = BoxSizer::builder(Orientation::Horizontal).build();
    row.add(
        &StaticText::builder(&panel)
            .with_label(&ui.menu_articles)
            .build(),
        0,
        SizerFlag::AlignCenterVertical | SizerFlag::All,
        5,
    );
    let choice = Choice::builder(&panel).build();
    for item in source.items.iter().take(MAX_MENU_ARTICLES_PER_SOURCE) {
        let label = sanitize_dynamic_menu_label(&item.title, &item.link);
        choice.append(&label);
    }
    choice.set_selection(0);
    row.add(&choice, 1, SizerFlag::Expand | SizerFlag::All, 5);
    root.add_sizer(&row, 0, SizerFlag::Expand, 0);

    let buttons = BoxSizer::builder(Orientation::Horizontal).build();
    let ok_button = Button::builder(&panel)
        .with_id(ID_OK)
        .with_label(&ui.ok)
        .build();
    buttons.add_spacer(1);
    buttons.add(&ok_button, 0, SizerFlag::All, 10);
    root.add_sizer(&buttons, 0, SizerFlag::Expand, 0);
    panel.set_sizer(root, true);

    dialog.set_affirmative_id(ID_OK);
    dialog.set_escape_id(ID_CANCEL);
    let dialog_ok = dialog;
    ok_button.on_click(move |_| {
        dialog_ok.end_modal(ID_OK);
    });

    let result = if dialog.show_modal() == ID_OK {
        choice
            .get_selection()
            .and_then(|selection| source.items.get(selection as usize).cloned())
    } else {
        None
    };

    dialog.destroy();
    let _ = source_index;
    result
}

fn open_article_folder_contents_dialog(
    parent: &Frame,
    settings: &Arc<Mutex<Settings>>,
    loading_urls: &HashSet<String>,
    folder_path: &str,
) -> Option<articles::ArticleItem> {
    let ui = current_ui_strings();
    let (sources, folders) = {
        let locked = settings.lock().unwrap();
        (
            locked.article_sources.clone(),
            locked.article_folders.clone(),
        )
    };
    let folder_path = normalize_article_folder_path(folder_path);
    let entries = article_folder_dialog_entries(&folder_path, &folders, &sources, ui);
    if entries.is_empty() {
        show_message_dialog(
            parent,
            &article_folder_display_name(ui, &folder_path),
            &ui.folder_empty,
        );
        return None;
    }

    let dialog = Dialog::builder(parent, &article_folder_display_name(ui, &folder_path))
        .with_style(DialogStyle::DefaultDialogStyle | DialogStyle::ResizeBorder)
        .with_size(620, 180)
        .build();
    let panel = Panel::builder(&dialog).build();
    let root = BoxSizer::builder(Orientation::Vertical).build();

    let row = BoxSizer::builder(Orientation::Horizontal).build();
    row.add(
        &StaticText::builder(&panel)
            .with_label(&ui.folder_label)
            .build(),
        0,
        SizerFlag::AlignCenterVertical | SizerFlag::All,
        5,
    );
    let choice = Choice::builder(&panel).build();
    for (label, _) in &entries {
        choice.append(label);
    }
    choice.set_selection(0);
    row.add(&choice, 1, SizerFlag::Expand | SizerFlag::All, 5);
    root.add_sizer(&row, 0, SizerFlag::Expand, 0);

    let buttons = BoxSizer::builder(Orientation::Horizontal).build();
    let ok_button = Button::builder(&panel)
        .with_id(ID_OK)
        .with_label(&ui.ok)
        .build();
    buttons.add_spacer(1);
    buttons.add(&ok_button, 0, SizerFlag::All, 10);
    root.add_sizer(&buttons, 0, SizerFlag::Expand, 0);
    panel.set_sizer(root, true);

    dialog.set_affirmative_id(ID_OK);
    dialog.set_escape_id(ID_CANCEL);
    let dialog_ok = dialog;
    ok_button.on_click(move |_| {
        dialog_ok.end_modal(ID_OK);
    });

    let result = if dialog.show_modal() == ID_OK {
        choice
            .get_selection()
            .and_then(|selection| entries.get(selection as usize))
            .and_then(|(_, entry)| match entry {
                ArticleFolderDialogEntry::Folder(folder) => {
                    open_article_folder_contents_dialog(parent, settings, loading_urls, folder)
                }
                ArticleFolderDialogEntry::Source(source_index) => {
                    sources.get(*source_index).and_then(|source| {
                        open_article_source_items_dialog(
                            parent,
                            source,
                            *source_index,
                            loading_urls,
                        )
                    })
                }
            })
    } else {
        None
    };

    dialog.destroy();
    result
}

fn rebuild_articles_menu(
    articles_menu: &Menu,
    settings: &Arc<Mutex<Settings>>,
    loading_urls: &HashSet<String>,
) {
    let ui_language = settings.lock().unwrap().ui_language.clone();
    let ui = ui_strings(&ui_language);
    for item in articles_menu.get_menu_items().into_iter().rev() {
        let _ = articles_menu.delete_item(&item);
    }

    let _ = articles_menu.append(
        ID_ARTICLES_ADD_SOURCE,
        &format!("{}...", ui.add_source),
        &ui.add_source,
        ItemKind::Normal,
    );
    let _ = articles_menu.append(
        ID_ARTICLES_EDIT_SOURCE,
        &format!("{}...", ui.edit_source),
        &ui.edit_source,
        ItemKind::Normal,
    );
    let _ = articles_menu.append(
        ID_ARTICLES_DELETE_SOURCE,
        &format!("{}...", ui.delete_source),
        &ui.delete_source,
        ItemKind::Normal,
    );
    let _ = articles_menu.append(
        ID_ARTICLES_REORDER_SOURCES,
        &format!("{}...", ui.reorder_sources),
        &ui.reorder_sources,
        ItemKind::Normal,
    );
    let _ = articles_menu.append(
        ID_ARTICLES_SORT_SOURCES_ALPHABETICALLY,
        &ui.sorted_articles_title,
        &ui.sorted_articles_message,
        ItemKind::Normal,
    );
    let _ = articles_menu.append(
        ID_ARTICLES_IMPORT_SOURCES,
        &format!("{}...", ui.import_sources),
        &ui.import_sources,
        ItemKind::Normal,
    );
    let _ = articles_menu.append(
        ID_ARTICLES_EXPORT_SOURCES,
        &format!("{}...", ui.export_sources),
        &ui.export_sources,
        ItemKind::Normal,
    );
    articles_menu.append_separator();

    let (sources, folders) = {
        let locked = settings.lock().unwrap();
        (
            locked.article_sources.clone(),
            locked.article_folders.clone(),
        )
    };

    let root_folders = current_article_folder_children("", &folders, &sources);
    for (folder_index, folder_path) in root_folders.iter().enumerate() {
        let folder_label = sanitize_dynamic_menu_label(
            &article_folder_display_name(ui, folder_path),
            &ui.folder_label,
        );
        let submenu = build_article_folder_submenu(folder_path, &folders, &sources, ui);
        let _ = articles_menu.append_submenu(submenu, &folder_label, folder_path);
        let _ = folder_index;
    }

    for (source_index, source) in sources
        .iter()
        .enumerate()
        .filter(|(_, source)| normalize_article_folder_path(&source.folder_path).is_empty())
    {
        let submenu = build_article_source_submenu(source_index, source, loading_urls, ui);
        let label = sanitize_dynamic_menu_label(&article_source_label(source), &source.url);
        let _ = articles_menu.append_submenu(submenu, &label, &source.url);
    }
}

fn rebuild_podcasts_menu(
    podcasts_menu: &Menu,
    settings: &Arc<Mutex<Settings>>,
    loading_urls: &HashSet<String>,
    category_results: &HashMap<u32, Vec<podcasts::PodcastSearchResult>>,
    category_loading: &HashSet<u32>,
) {
    let ui_language = settings.lock().unwrap().ui_language.clone();
    let categories = podcasts::apple_categories(&ui_language);
    let ui = ui_strings(&ui_language);
    for item in podcasts_menu.get_menu_items().into_iter().rev() {
        let _ = podcasts_menu.delete_item(&item);
    }

    let _ = podcasts_menu.append(
        ID_PODCASTS_ADD,
        &format!("{}...", ui.add_podcast),
        &ui.add_podcast,
        ItemKind::Normal,
    );
    let categories_menu = Menu::builder().build();
    for (index, category) in categories.iter().enumerate() {
        let category_submenu = Menu::builder().build();
        if category_loading.contains(&category.id) {
            let placeholder_id = ID_PODCASTS_CATEGORY_BASE + index as i32;
            let _ = category_submenu.append(
                placeholder_id,
                &ui.loading_podcasts,
                &ui.wait_loading_category_podcasts,
                ItemKind::Normal,
            );
            let _ = category_submenu.enable_item(placeholder_id, false);
        } else if let Some(results) = category_results.get(&category.id) {
            if results.is_empty() {
                let placeholder_id = ID_PODCASTS_CATEGORY_BASE + index as i32;
                let _ = category_submenu.append(
                    placeholder_id,
                    &ui.no_podcasts_available,
                    &ui.no_podcasts_for_category,
                    ItemKind::Normal,
                );
                let _ = category_submenu.enable_item(placeholder_id, false);
            } else {
                for (result_index, result) in results.iter().take(30).enumerate() {
                    let label = if result.artist.trim().is_empty() {
                        result.title.clone()
                    } else {
                        format!("{} - {}", result.title, result.artist)
                    };
                    let _ = category_submenu.append(
                        podcasts_category_podcast_menu_id(index, result_index),
                        &label,
                        &ui.add_this_podcast,
                        ItemKind::Normal,
                    );
                }
            }
        } else {
            let placeholder_id = ID_PODCASTS_CATEGORY_BASE + index as i32;
            let _ = category_submenu.append(
                placeholder_id,
                &ui.loading_podcasts,
                &ui.wait_loading_category_podcasts,
                ItemKind::Normal,
            );
            let _ = category_submenu.enable_item(placeholder_id, false);
        }
        let _ = categories_menu.append_submenu(
            category_submenu,
            &category.name,
            "Sfoglia i podcast della categoria",
        );
    }
    let _ = podcasts_menu.append_submenu(
        categories_menu,
        "Sfoglia per categorie",
        "Sfoglia podcast per categoria",
    );
    let _ = podcasts_menu.append(
        ID_PODCASTS_DELETE,
        &format!("{}...", ui.delete_podcast),
        &ui.delete_podcast,
        ItemKind::Normal,
    );
    let _ = podcasts_menu.append(
        ID_PODCASTS_REORDER_SOURCES,
        &format!("{}...", ui.reorder_podcasts),
        &ui.reorder_podcasts,
        ItemKind::Normal,
    );
    let _ = podcasts_menu.append(
        ID_PODCASTS_SORT_SOURCES_ALPHABETICALLY,
        &ui.sorted_podcasts_title,
        &ui.sorted_podcasts_message,
        ItemKind::Normal,
    );
    podcasts_menu.append_separator();

    let sources = settings.lock().unwrap().podcast_sources.clone();
    for (source_index, source) in sources.iter().enumerate() {
        let submenu = Menu::builder().build();
        if source.episodes.is_empty() {
            let placeholder_id = podcasts_source_menu_id(source_index);
            let is_loading = loading_urls.contains(&source.url);
            let _ = submenu.append(
                placeholder_id,
                if is_loading {
                    &ui.loading_episodes
                } else {
                    &ui.no_episodes_available
                },
                if is_loading {
                    &ui.wait_loading_episodes
                } else {
                    &ui.refresh_podcast_for_episodes
                },
                ItemKind::Normal,
            );
            let _ = submenu.enable_item(placeholder_id, false);
        } else {
            for (episode_index, episode) in source
                .episodes
                .iter()
                .take(MAX_MENU_PODCAST_EPISODES_PER_SOURCE)
                .enumerate()
            {
                let _ = submenu.append(
                    podcasts_episode_menu_id(source_index, episode_index),
                    &episode.title,
                    &episode.link,
                    ItemKind::Normal,
                );
            }
        }
        let _ = podcasts_menu.append_submenu(submenu, &source.title, &source.url);
    }
}

fn rebuild_radio_menu(
    radio_menu: &Menu,
    settings: &Arc<Mutex<Settings>>,
    radio_menu_state: &Arc<Mutex<RadioMenuState>>,
) {
    let ui_language = settings.lock().unwrap().ui_language.clone();
    let ui = ui_strings(&ui_language);
    let favorites = settings.lock().unwrap().radio_favorites.clone();

    for item in radio_menu.get_menu_items().into_iter().rev() {
        let _ = radio_menu.delete_item(&item);
    }

    let _ = radio_menu.append(ID_RADIO_SEARCH, "Cerca...", "Cerca radio", ItemKind::Normal);
    let _ = radio_menu.append(
        ID_RADIO_ADD,
        &format!("{}...", ui.add_radio),
        &ui.add_radio,
        ItemKind::Normal,
    );
    let _ = radio_menu.append(
        ID_RADIO_EDIT_FAVORITE,
        &format!("{}...", ui.edit_radio),
        &ui.edit_radio,
        ItemKind::Normal,
    );
    let _ = radio_menu.append(
        ID_RADIO_REORDER_FAVORITES,
        &format!("{}...", ui.reorder_radios),
        &ui.reorder_radios,
        ItemKind::Normal,
    );

    let favorites_menu = Menu::builder().build();
    let mut station_ids = HashMap::new();
    if favorites.is_empty() {
        let _ = favorites_menu.append(
            ID_RADIO_FAVORITE_BASE,
            &ui.no_radios_available,
            &ui.no_radios_available,
            ItemKind::Normal,
        );
        let _ = favorites_menu.enable_item(ID_RADIO_FAVORITE_BASE, false);
    } else {
        for (index, favorite) in favorites.iter().enumerate() {
            let menu_id = radio_favorite_menu_id(index);
            let label = radio_label(favorite);
            let _ = favorites_menu.append(menu_id, &label, &favorite.stream_url, ItemKind::Normal);
            station_ids.insert(menu_id, favorite.clone());
        }
    }
    let _ = radio_menu.append_submenu(favorites_menu, &ui.radio_favorites, &ui.radio_favorites);
    let _ = radio_menu.append(
        ID_RADIO_DELETE_FAVORITE,
        &format!("{}...", ui.delete_radio_favorite),
        &ui.delete_radio_favorite,
        ItemKind::Normal,
    );

    let mut state = radio_menu_state.lock().unwrap();
    state.station_ids = station_ids;
}

fn refresh_all_article_sources(
    rt: &Arc<Runtime>,
    settings: &Arc<Mutex<Settings>>,
    article_menu_state: &Arc<Mutex<ArticleMenuState>>,
) {
    let rt_refresh = Arc::clone(rt);
    let settings_refresh = Arc::clone(settings);
    let menu_state_refresh = Arc::clone(article_menu_state);
    std::thread::spawn(move || {
        let sources = settings_refresh.lock().unwrap().article_sources.clone();
        let mut updated_sources = Vec::with_capacity(sources.len());
        let mut changed = false;
        for source in sources {
            match rt_refresh.block_on(articles::fetch_source(&source)) {
                Ok(updated) => {
                    let should_preserve_existing_items =
                        updated.items.is_empty() && !source.items.is_empty();
                    let effective_source = if should_preserve_existing_items {
                        append_podcast_log(&format!(
                            "articles_refresh.keep_existing_items url={} previous_items={}",
                            source.url,
                            source.items.len()
                        ));
                        source.clone()
                    } else {
                        updated
                    };
                    if effective_source.items != source.items
                        || effective_source.title != source.title
                    {
                        changed = true;
                    }
                    updated_sources.push(effective_source);
                }
                Err(err) => {
                    println!(
                        "ERROR: Aggiornamento articoli fallito per {}: {}",
                        source.title, err
                    );
                    updated_sources.push(source);
                }
            }
        }

        if changed {
            let mut locked = settings_refresh.lock().unwrap();
            locked.article_sources = updated_sources;
            locked.save();
            menu_state_refresh.lock().unwrap().dirty = true;
        }
    });
}

fn refresh_single_article_source(
    source_url: String,
    rt: &Arc<Runtime>,
    settings: &Arc<Mutex<Settings>>,
    article_menu_state: &Arc<Mutex<ArticleMenuState>>,
) {
    {
        let mut state = article_menu_state.lock().unwrap();
        state.loading_urls.insert(source_url.clone());
        state.dirty = true;
    }

    let rt_refresh = Arc::clone(rt);
    let settings_refresh = Arc::clone(settings);
    let menu_state_refresh = Arc::clone(article_menu_state);
    std::thread::spawn(move || {
        let source = {
            settings_refresh
                .lock()
                .unwrap()
                .article_sources
                .iter()
                .find(|source| source.url.eq_ignore_ascii_case(&source_url))
                .cloned()
        };

        if let Some(source) = source {
            match rt_refresh.block_on(articles::fetch_source(&source)) {
                Ok(updated) => {
                    let mut locked = settings_refresh.lock().unwrap();
                    if let Some(existing) = locked
                        .article_sources
                        .iter_mut()
                        .find(|existing| existing.url.eq_ignore_ascii_case(&source_url))
                    {
                        if updated.items.is_empty() && !existing.items.is_empty() {
                            append_podcast_log(&format!(
                                "article_refresh.keep_existing_items url={} previous_items={}",
                                source_url,
                                existing.items.len()
                            ));
                        } else {
                            *existing = updated;
                            locked.save();
                        }
                    }
                }
                Err(err) => {
                    println!(
                        "ERROR: Aggiornamento articoli fallito per {}: {}",
                        source.title, err
                    );
                }
            }
        }

        let mut state = menu_state_refresh.lock().unwrap();
        state.loading_urls.remove(&source_url);
        state.dirty = true;
    });
}

fn refresh_single_podcast_source(
    source_url: String,
    rt: &Arc<Runtime>,
    settings: &Arc<Mutex<Settings>>,
    podcast_menu_state: &Arc<Mutex<PodcastMenuState>>,
) {
    {
        let mut state = podcast_menu_state.lock().unwrap();
        state.loading_urls.insert(source_url.clone());
        state.dirty = true;
    }

    let rt_refresh = Arc::clone(rt);
    let settings_refresh = Arc::clone(settings);
    let menu_state_refresh = Arc::clone(podcast_menu_state);
    std::thread::spawn(move || {
        let source = {
            settings_refresh
                .lock()
                .unwrap()
                .podcast_sources
                .iter()
                .find(|source| source.url.eq_ignore_ascii_case(&source_url))
                .cloned()
        };

        if let Some(source) = source {
            match rt_refresh.block_on(podcasts::fetch_source(&source)) {
                Ok(updated) => {
                    let mut locked = settings_refresh.lock().unwrap();
                    if let Some(existing) = locked
                        .podcast_sources
                        .iter_mut()
                        .find(|existing| existing.url.eq_ignore_ascii_case(&source_url))
                    {
                        *existing = updated;
                        locked.save();
                    }
                }
                Err(err) => {
                    println!(
                        "ERROR: Aggiornamento podcast fallito per {}: {}",
                        source.title, err
                    );
                }
            }
        }

        let mut state = menu_state_refresh.lock().unwrap();
        state.loading_urls.remove(&source_url);
        state.dirty = true;
    });
}

fn refresh_all_podcast_sources(
    rt: &Arc<Runtime>,
    settings: &Arc<Mutex<Settings>>,
    podcast_menu_state: &Arc<Mutex<PodcastMenuState>>,
) {
    let source_urls = {
        settings
            .lock()
            .unwrap()
            .podcast_sources
            .iter()
            .map(|source| source.url.clone())
            .collect::<Vec<String>>()
    };

    for source_url in source_urls {
        refresh_single_podcast_source(source_url, rt, settings, podcast_menu_state);
    }
}

fn refresh_all_podcast_categories(
    rt: &Arc<Runtime>,
    podcast_menu_state: &Arc<Mutex<PodcastMenuState>>,
) {
    for category in podcasts::apple_categories(&Settings::load().ui_language) {
        {
            let mut state = podcast_menu_state.lock().unwrap();
            state.category_loading.insert(category.id);
            state.dirty = true;
        }

        let rt_refresh = Arc::clone(rt);
        let menu_state_refresh = Arc::clone(podcast_menu_state);
        std::thread::spawn(move || {
            let results = rt_refresh
                .block_on(podcasts::search_itunes_category(category.id))
                .unwrap_or_else(|err| {
                    println!(
                        "ERROR: Caricamento categoria podcast fallito per {}: {}",
                        category.name, err
                    );
                    Vec::new()
                });

            let mut state = menu_state_refresh.lock().unwrap();
            state.category_results.insert(category.id, results);
            state.category_loading.remove(&category.id);
            state.dirty = true;
        });
    }
}

fn refresh_all_radio_languages(radio_menu_state: &Arc<Mutex<RadioMenuState>>) {
    let languages = radio_menu_languages();
    {
        let mut state = radio_menu_state.lock().unwrap();
        state.loading_languages = languages
            .iter()
            .map(|(code, _)| code.clone())
            .collect::<HashSet<String>>();
        state.failed_languages.clear();
        state.dirty = true;
    }

    for (language_code, _) in languages {
        let menu_state_refresh = Arc::clone(radio_menu_state);
        std::thread::spawn(move || {
            let result = fetch_radio_browser_stations(&language_code);
            let mut state = menu_state_refresh.lock().unwrap();
            state.loading_languages.remove(&language_code);
            match result {
                Ok(stations) => {
                    state.failed_languages.remove(&language_code);
                    let merged_stations = if let Some(local_stations) =
                        state.stations_by_language.get(&language_code)
                    {
                        merge_radio_stations_preserving_local(local_stations, stations)
                    } else {
                        stations
                    };
                    state
                        .stations_by_language
                        .insert(language_code.clone(), merged_stations);
                }
                Err(err) => {
                    println!(
                        "ERROR: Caricamento radio fallito per lingua {}: {}",
                        language_code, err
                    );
                    state.failed_languages.insert(language_code.clone());
                }
            }
            state.dirty = true;
        });
    }
}

fn add_article_source(
    title: String,
    url: String,
    settings: &Arc<Mutex<Settings>>,
    article_menu_state: &Arc<Mutex<ArticleMenuState>>,
    rt: &Arc<Runtime>,
) {
    let Some((normalized_url, resolved_title)) = resolve_article_source_input(&title, &url) else {
        return;
    };

    {
        let mut locked = settings.lock().unwrap();
        if locked
            .article_sources
            .iter()
            .any(|source| source.url.eq_ignore_ascii_case(&normalized_url))
        {
            return;
        }
        locked.article_sources.push(articles::ArticleSource {
            title: resolved_title,
            url: normalized_url.clone(),
            folder_path: String::new(),
            items: Vec::new(),
        });
        locked.save();
    }
    refresh_single_article_source(normalized_url, rt, settings, article_menu_state);
}

fn resolve_article_source_input(title: &str, url: &str) -> Option<(String, String)> {
    let trimmed_input = url.trim();
    if trimmed_input.is_empty() {
        return None;
    }

    let (normalized_url, resolved_title) = if looks_like_article_source_url(trimmed_input) {
        let normalized_url = articles::normalize_url(trimmed_input);
        let resolved_title = if title.trim().is_empty() {
            normalized_url.clone()
        } else {
            title.trim().to_string()
        };
        (normalized_url, resolved_title)
    } else {
        let resolved_title = if title.trim().is_empty() {
            format_google_news_source_title(trimmed_input)
        } else {
            title.trim().to_string()
        };
        (build_google_news_rss_url(trimmed_input), resolved_title)
    };

    if normalized_url.is_empty() {
        None
    } else {
        Some((normalized_url, resolved_title))
    }
}

fn parse_opml_sources(text: &str) -> Vec<ImportedArticleSource> {
    let mut reader = Reader::from_str(text);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut out = Vec::new();
    let mut folder_stack = Vec::<String>::new();
    let mut pushed_folder_stack = Vec::<bool>::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(element)) => {
                if !element.name().as_ref().eq_ignore_ascii_case(b"outline") {
                    buf.clear();
                    continue;
                }
                let mut title = String::new();
                let mut url = String::new();
                for attr in element.attributes().flatten() {
                    let key = attr.key.as_ref();
                    let value = attr
                        .decode_and_unescape_value(reader.decoder())
                        .unwrap_or_default()
                        .to_string();
                    if key.eq_ignore_ascii_case(b"xmlUrl") {
                        url = value;
                    } else if title.is_empty()
                        && (key.eq_ignore_ascii_case(b"title") || key.eq_ignore_ascii_case(b"text"))
                    {
                        title = value;
                    }
                }
                if !url.trim().is_empty() {
                    out.push(ImportedArticleSource {
                        title,
                        url,
                        folder_path: folder_stack.join("/"),
                    });
                    pushed_folder_stack.push(false);
                } else {
                    let folder_name = normalize_article_folder_path(&title);
                    let pushed = !folder_name.is_empty();
                    if pushed {
                        folder_stack.push(folder_name);
                    }
                    pushed_folder_stack.push(pushed);
                }
            }
            Ok(Event::Empty(element)) => {
                if !element.name().as_ref().eq_ignore_ascii_case(b"outline") {
                    buf.clear();
                    continue;
                }
                let mut title = String::new();
                let mut url = String::new();
                for attr in element.attributes().flatten() {
                    let key = attr.key.as_ref();
                    let value = attr
                        .decode_and_unescape_value(reader.decoder())
                        .unwrap_or_default()
                        .to_string();
                    if key.eq_ignore_ascii_case(b"xmlUrl") {
                        url = value;
                    } else if title.is_empty()
                        && (key.eq_ignore_ascii_case(b"title") || key.eq_ignore_ascii_case(b"text"))
                    {
                        title = value;
                    }
                }
                if !url.trim().is_empty() {
                    out.push(ImportedArticleSource {
                        title,
                        url,
                        folder_path: folder_stack.join("/"),
                    });
                }
            }
            Ok(Event::End(element)) => {
                if element.name().as_ref().eq_ignore_ascii_case(b"outline")
                    && pushed_folder_stack.pop().unwrap_or(false)
                {
                    folder_stack.pop();
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    out
}

fn parse_article_sources_text(text: &str) -> Vec<ImportedArticleSource> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| !line.starts_with('#'))
        .map(|line| {
            if let Some((title, url)) = line.split_once('|') {
                ImportedArticleSource {
                    title: title.trim().to_string(),
                    url: url.trim().to_string(),
                    folder_path: String::new(),
                }
            } else {
                ImportedArticleSource {
                    title: String::new(),
                    url: line.to_string(),
                    folder_path: String::new(),
                }
            }
        })
        .collect()
}

#[cfg(target_os = "macos")]
fn mac_file_dialog_path_candidate_is_usable(path: &Path, must_exist: bool) -> bool {
    if path.as_os_str().is_empty() {
        return false;
    }

    if must_exist {
        return path.is_file();
    }

    path.parent()
        .is_some_and(|parent| !parent.as_os_str().is_empty() && parent.exists())
}

fn resolve_file_dialog_path(dialog: &FileDialog, must_exist: bool) -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        for candidate in dialog.get_paths().into_iter().map(PathBuf::from) {
            if mac_file_dialog_path_candidate_is_usable(&candidate, must_exist) {
                return Some(candidate);
            }
        }

        if let Some(candidate) = dialog.get_path().map(PathBuf::from)
            && mac_file_dialog_path_candidate_is_usable(&candidate, must_exist)
        {
            return Some(candidate);
        }

        if let (Some(directory), Some(filename)) = (dialog.get_directory(), dialog.get_filename()) {
            let candidate = PathBuf::from(directory).join(filename);
            if mac_file_dialog_path_candidate_is_usable(&candidate, must_exist) {
                return Some(candidate);
            }
        }

        None
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = must_exist;
        dialog.get_path().map(PathBuf::from)
    }
}

fn open_article_sources_import_dialog(parent: &Frame) -> Option<PathBuf> {
    let ui = current_ui_strings();
    let dialog = FileDialog::builder(parent)
        .with_message(&ui.import_sources)
        .with_wildcard("OPML o TXT|*.opml;*.txt|Tutti|*.*")
        .build();

    #[cfg(target_os = "macos")]
    set_mac_native_file_dialog_open(true);
    let dialog_result = dialog.show_modal();
    #[cfg(target_os = "macos")]
    set_mac_native_file_dialog_open(false);

    if dialog_result != ID_OK {
        return None;
    }

    resolve_file_dialog_path(&dialog, true)
}

fn open_article_sources_export_dialog(parent: &Frame) -> Option<PathBuf> {
    let ui = current_ui_strings();
    let dialog = FileDialog::builder(parent)
        .with_message(&ui.export_sources)
        .with_default_file("sonarpad-articles.opml")
        .with_wildcard("OPML|*.opml|Tutti|*.*")
        .with_style(FileDialogStyle::Save | FileDialogStyle::OverwritePrompt)
        .build();

    #[cfg(target_os = "macos")]
    set_mac_native_file_dialog_open(true);
    let dialog_result = dialog.show_modal();
    #[cfg(target_os = "macos")]
    set_mac_native_file_dialog_open(false);

    if dialog_result != ID_OK {
        return None;
    }

    resolve_file_dialog_path(&dialog, false)
}

fn escape_opml_attr(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn write_article_opml_folder(
    file: &mut std::fs::File,
    current_folder: &str,
    folders: &[String],
    sources: &[articles::ArticleSource],
    depth: usize,
) -> Result<(), String> {
    let indent = "  ".repeat(depth);
    let child_indent = "  ".repeat(depth + 1);

    for folder_path in current_article_folder_children(current_folder, folders, sources) {
        let label = article_folder_segments(&folder_path)
            .last()
            .copied()
            .unwrap_or(folder_path.as_str());
        writeln!(
            file,
            "{indent}<outline text=\"{}\" title=\"{}\">",
            escape_opml_attr(label),
            escape_opml_attr(label)
        )
        .map_err(|err| err.to_string())?;
        write_article_opml_folder(file, &folder_path, folders, sources, depth + 1)?;
        writeln!(file, "{indent}</outline>").map_err(|err| err.to_string())?;
    }

    for source in sources
        .iter()
        .filter(|source| normalize_article_folder_path(&source.folder_path) == current_folder)
    {
        let title = article_source_label(source);
        writeln!(
            file,
            "{child_indent}<outline text=\"{}\" title=\"{}\" xmlUrl=\"{}\" />",
            escape_opml_attr(&title),
            escape_opml_attr(&title),
            escape_opml_attr(&source.url)
        )
        .map_err(|err| err.to_string())?;
    }

    Ok(())
}

fn export_article_sources_to_opml(
    path: &Path,
    settings: &Arc<Mutex<Settings>>,
) -> Result<usize, String> {
    let (sources, folders) = {
        let locked = settings.lock().unwrap();
        (
            locked.article_sources.clone(),
            locked.article_folders.clone(),
        )
    };
    let mut file = std::fs::File::create(path).map_err(|err| err.to_string())?;
    writeln!(
        file,
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<opml version=\"1.0\">\n<head>\n<title>Sonarpad Articles</title>\n</head>\n<body>"
    )
    .map_err(|err| err.to_string())?;
    write_article_opml_folder(&mut file, "", &folders, &sources, 1)?;
    writeln!(file, "</body>\n</opml>").map_err(|err| err.to_string())?;
    Ok(sources.len())
}

fn import_article_sources_from_file(
    path: &Path,
    settings: &Arc<Mutex<Settings>>,
    article_menu_state: &Arc<Mutex<ArticleMenuState>>,
    rt: &Arc<Runtime>,
) -> Result<usize, String> {
    let bytes = std::fs::read(path).map_err(|err| err.to_string())?;
    let text = String::from_utf8_lossy(&bytes);
    let is_opml = path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("opml"))
        || text.to_ascii_lowercase().contains("<opml");
    let imported_sources = if is_opml {
        parse_opml_sources(&text)
    } else {
        parse_article_sources_text(&text)
    };

    if imported_sources.is_empty() {
        return Ok(0);
    }

    let mut added_urls = Vec::new();
    {
        let mut locked = settings.lock().unwrap();
        let mut existing_urls: HashSet<String> = locked
            .article_sources
            .iter()
            .map(|source| source.url.to_ascii_lowercase())
            .collect();
        for imported_source in imported_sources {
            let Some((normalized_url, resolved_title)) =
                resolve_article_source_input(&imported_source.title, &imported_source.url)
            else {
                continue;
            };
            let key = normalized_url.to_ascii_lowercase();
            if existing_urls.contains(&key) {
                continue;
            }
            existing_urls.insert(key);
            locked.article_sources.push(articles::ArticleSource {
                title: resolved_title,
                url: normalized_url.clone(),
                folder_path: normalize_article_folder_path(&imported_source.folder_path),
                items: Vec::new(),
            });
            added_urls.push(normalized_url);
        }
    }
    if added_urls.is_empty() {
        return Ok(0);
    }
    settings.lock().unwrap().save();

    let added_count = added_urls.len();
    article_menu_state.lock().unwrap().dirty = true;
    for url in added_urls {
        refresh_single_article_source(url, rt, settings, article_menu_state);
    }

    Ok(added_count)
}

fn edit_article_source(
    source_index: usize,
    title: String,
    url: String,
    settings: &Arc<Mutex<Settings>>,
    article_menu_state: &Arc<Mutex<ArticleMenuState>>,
    rt: &Arc<Runtime>,
) {
    let Some((normalized_url, resolved_title)) = resolve_article_source_input(&title, &url) else {
        return;
    };

    {
        let mut locked = settings.lock().unwrap();
        if source_index >= locked.article_sources.len() {
            return;
        }
        if locked
            .article_sources
            .iter()
            .enumerate()
            .any(|(index, source)| {
                index != source_index && source.url.eq_ignore_ascii_case(&normalized_url)
            })
        {
            return;
        }
        let source = &mut locked.article_sources[source_index];
        source.title = resolved_title;
        source.url = normalized_url.clone();
        source.items.clear();
        locked.save();
    }

    refresh_single_article_source(normalized_url, rt, settings, article_menu_state);
}

fn delete_article_source(
    source_index: usize,
    settings: &Arc<Mutex<Settings>>,
    article_menu_state: &Arc<Mutex<ArticleMenuState>>,
) {
    let mut locked = settings.lock().unwrap();
    if source_index >= locked.article_sources.len() {
        return;
    }
    locked.article_sources.remove(source_index);
    locked.save();
    article_menu_state.lock().unwrap().dirty = true;
}

fn save_reordered_article_sources(
    reordered_sources: Vec<articles::ArticleSource>,
    article_folders: Vec<String>,
    settings: &Arc<Mutex<Settings>>,
    article_menu_state: &Arc<Mutex<ArticleMenuState>>,
) {
    let mut locked = settings.lock().unwrap();
    locked.article_sources = reordered_sources;
    locked.article_folders = normalized_article_folders(&article_folders, &locked.article_sources);
    locked.save();
    article_menu_state.lock().unwrap().dirty = true;
}

fn save_reordered_podcast_sources(
    reordered_sources: Vec<podcasts::PodcastSource>,
    settings: &Arc<Mutex<Settings>>,
    podcast_menu_state: &Arc<Mutex<PodcastMenuState>>,
) {
    let mut locked = settings.lock().unwrap();
    locked.podcast_sources = reordered_sources;
    locked.save();
    podcast_menu_state.lock().unwrap().dirty = true;
}

fn save_reordered_radio_favorites(
    reordered_favorites: Vec<RadioFavorite>,
    settings: &Arc<Mutex<Settings>>,
    radio_menu_state: &Arc<Mutex<RadioMenuState>>,
) {
    let mut locked = settings.lock().unwrap();
    locked.radio_favorites = reordered_favorites;
    normalize_settings_data(&mut locked);
    locked.save();
    radio_menu_state.lock().unwrap().dirty = true;
}

fn sort_article_sources_alphabetically(
    settings: &Arc<Mutex<Settings>>,
    article_menu_state: &Arc<Mutex<ArticleMenuState>>,
) {
    let mut locked = settings.lock().unwrap();
    locked.article_sources.sort_by(|left, right| {
        let left_label = article_source_label(left);
        let right_label = article_source_label(right);
        normalized_sort_key(&left_label)
            .cmp(&normalized_sort_key(&right_label))
            .then_with(|| left.url.cmp(&right.url))
    });
    locked.save();
    article_menu_state.lock().unwrap().dirty = true;
}

fn article_source_label(source: &articles::ArticleSource) -> String {
    if source.title.trim().is_empty() {
        source.url.clone()
    } else {
        source.title.clone()
    }
}

fn podcast_source_label(source: &podcasts::PodcastSource) -> String {
    if source.title.trim().is_empty() {
        source.url.clone()
    } else {
        source.title.clone()
    }
}

fn normalized_sort_key(label: &str) -> String {
    label
        .trim()
        .trim_start_matches(|ch: char| !ch.is_alphanumeric())
        .to_lowercase()
}

fn add_podcast_source(
    result: podcasts::PodcastSearchResult,
    settings: &Arc<Mutex<Settings>>,
    podcast_menu_state: &Arc<Mutex<PodcastMenuState>>,
    rt: &Arc<Runtime>,
) {
    let normalized_url = podcasts::normalize_url(&result.feed_url);
    if normalized_url.is_empty() {
        return;
    }

    {
        let mut locked = settings.lock().unwrap();
        if locked
            .podcast_sources
            .iter()
            .any(|source| source.url.eq_ignore_ascii_case(&normalized_url))
        {
            return;
        }
        let title = if result.artist.trim().is_empty() {
            result.title
        } else {
            format!("{} - {}", result.title, result.artist)
        };
        locked.podcast_sources.push(podcasts::PodcastSource {
            title,
            url: normalized_url.clone(),
            episodes: Vec::new(),
        });
        locked.save();
    }

    refresh_single_podcast_source(normalized_url, rt, settings, podcast_menu_state);
}

fn sort_podcast_sources_alphabetically(
    settings: &Arc<Mutex<Settings>>,
    podcast_menu_state: &Arc<Mutex<PodcastMenuState>>,
) {
    let mut locked = settings.lock().unwrap();
    locked.podcast_sources.sort_by(|left, right| {
        let left_label = podcast_source_label(left);
        let right_label = podcast_source_label(right);
        normalized_sort_key(&left_label)
            .cmp(&normalized_sort_key(&right_label))
            .then_with(|| left.url.cmp(&right.url))
    });
    locked.save();
    podcast_menu_state.lock().unwrap().dirty = true;
}

fn delete_podcast_source(
    source_index: usize,
    settings: &Arc<Mutex<Settings>>,
    podcast_menu_state: &Arc<Mutex<PodcastMenuState>>,
) {
    let mut locked = settings.lock().unwrap();
    if source_index >= locked.podcast_sources.len() {
        return;
    }
    locked.podcast_sources.remove(source_index);
    locked.save();
    podcast_menu_state.lock().unwrap().dirty = true;
}

fn open_add_podcast_dialog(
    parent: &Frame,
    rt: &Arc<Runtime>,
) -> Option<podcasts::PodcastSearchResult> {
    let ui = current_ui_strings();
    let dialog = Dialog::builder(parent, &ui.add_podcast)
        .with_style(DialogStyle::DefaultDialogStyle | DialogStyle::ResizeBorder)
        .with_size(560, 180)
        .build();
    let panel = Panel::builder(&dialog).build();
    let root = BoxSizer::builder(Orientation::Vertical).build();

    let keyword_row = BoxSizer::builder(Orientation::Horizontal).build();
    keyword_row.add(
        &StaticText::builder(&panel).with_label(&ui.keyword).build(),
        0,
        SizerFlag::AlignCenterVertical | SizerFlag::All,
        5,
    );
    let keyword_ctrl = TextCtrl::builder(&panel).build();
    keyword_row.add(&keyword_ctrl, 1, SizerFlag::Expand | SizerFlag::All, 5);
    root.add_sizer(&keyword_row, 0, SizerFlag::Expand, 0);

    let buttons = BoxSizer::builder(Orientation::Horizontal).build();
    let ok_button = Button::builder(&panel)
        .with_id(ID_OK)
        .with_label(&ui.ok)
        .build();
    buttons.add_spacer(1);
    buttons.add(&ok_button, 0, SizerFlag::All, 10);
    root.add_sizer(&buttons, 0, SizerFlag::Expand, 0);
    panel.set_sizer(root, true);

    dialog.set_affirmative_id(ID_OK);
    let dialog_ok = dialog;
    ok_button.on_click(move |_| {
        dialog_ok.end_modal(ID_OK);
    });

    let result = if dialog.show_modal() == ID_OK {
        let keyword = keyword_ctrl.get_value();
        if keyword.trim().is_empty() {
            None
        } else {
            open_podcast_search_results_dialog(parent, rt, &keyword)
        }
    } else {
        None
    };

    dialog.destroy();
    result
}

fn open_podcast_search_results_dialog(
    parent: &Frame,
    rt: &Arc<Runtime>,
    keyword: &str,
) -> Option<podcasts::PodcastSearchResult> {
    let results = rt.block_on(podcasts::search_podcasts(keyword)).ok()?;
    open_podcast_results_dialog(parent, &current_ui_strings().add_podcast, &results)
}

fn open_podcast_results_dialog(
    parent: &Frame,
    title: &str,
    results: &[podcasts::PodcastSearchResult],
) -> Option<podcasts::PodcastSearchResult> {
    let ui = current_ui_strings();
    if results.is_empty() {
        return None;
    }

    let dialog = Dialog::builder(parent, title)
        .with_style(DialogStyle::DefaultDialogStyle | DialogStyle::ResizeBorder)
        .with_size(620, 180)
        .build();
    let panel = Panel::builder(&dialog).build();
    let root = BoxSizer::builder(Orientation::Vertical).build();

    let result_row = BoxSizer::builder(Orientation::Horizontal).build();
    result_row.add(
        &StaticText::builder(&panel)
            .with_label(&ui.podcast_label)
            .build(),
        0,
        SizerFlag::AlignCenterVertical | SizerFlag::All,
        5,
    );
    let choice_result = Choice::builder(&panel).build();
    for result in results {
        let label = if result.artist.trim().is_empty() {
            result.title.clone()
        } else {
            format!("{} - {}", result.title, result.artist)
        };
        choice_result.append(&label);
    }
    choice_result.set_selection(0);
    result_row.add(&choice_result, 1, SizerFlag::Expand | SizerFlag::All, 5);
    root.add_sizer(&result_row, 0, SizerFlag::Expand, 0);

    let buttons = BoxSizer::builder(Orientation::Horizontal).build();
    let ok_button = Button::builder(&panel)
        .with_id(ID_OK)
        .with_label(&ui.ok)
        .build();
    buttons.add_spacer(1);
    buttons.add(&ok_button, 0, SizerFlag::All, 10);
    root.add_sizer(&buttons, 0, SizerFlag::Expand, 0);
    panel.set_sizer(root, true);

    dialog.set_affirmative_id(ID_OK);
    let dialog_ok = dialog;
    ok_button.on_click(move |_| {
        dialog_ok.end_modal(ID_OK);
    });

    let result = if dialog.show_modal() == ID_OK {
        choice_result
            .get_selection()
            .and_then(|selection| results.get(selection as usize).cloned())
    } else {
        None
    };

    dialog.destroy();
    result
}

fn open_delete_podcast_dialog(parent: &Frame, settings: &Arc<Mutex<Settings>>) -> Option<usize> {
    let ui = current_ui_strings();
    let sources = settings.lock().unwrap().podcast_sources.clone();
    if sources.is_empty() {
        return None;
    }

    let dialog = Dialog::builder(parent, &ui.delete_podcast)
        .with_style(DialogStyle::DefaultDialogStyle | DialogStyle::ResizeBorder)
        .with_size(520, 160)
        .build();
    let panel = Panel::builder(&dialog).build();
    let root = BoxSizer::builder(Orientation::Vertical).build();

    let row = BoxSizer::builder(Orientation::Horizontal).build();
    row.add(
        &StaticText::builder(&panel)
            .with_label(&ui.podcast_label)
            .build(),
        0,
        SizerFlag::AlignCenterVertical | SizerFlag::All,
        5,
    );
    let choice_source = Choice::builder(&panel).build();
    for source in &sources {
        choice_source.append(&source.title);
    }
    choice_source.set_selection(0);
    row.add(&choice_source, 1, SizerFlag::Expand | SizerFlag::All, 5);
    root.add_sizer(&row, 0, SizerFlag::Expand, 0);

    let selected_index = Rc::new(RefCell::new(0usize));
    let choice_source_evt = choice_source;
    let selected_index_evt = Rc::clone(&selected_index);
    choice_source.on_selection_changed(move |_| {
        if let Some(selection) = choice_source_evt.get_selection() {
            *selected_index_evt.borrow_mut() = selection as usize;
        }
    });

    let buttons = BoxSizer::builder(Orientation::Horizontal).build();
    let ok_button = Button::builder(&panel)
        .with_id(ID_OK)
        .with_label(&ui.ok)
        .build();
    buttons.add_spacer(1);
    buttons.add(&ok_button, 0, SizerFlag::All, 10);
    root.add_sizer(&buttons, 0, SizerFlag::Expand, 0);
    panel.set_sizer(root, true);

    dialog.set_affirmative_id(ID_OK);
    let dialog_ok = dialog;
    ok_button.on_click(move |_| {
        dialog_ok.end_modal(ID_OK);
    });

    let result = if dialog.show_modal() == ID_OK {
        Some(*selected_index.borrow())
    } else {
        None
    };

    dialog.destroy();
    result
}

fn open_add_article_source_dialog(parent: &Frame) -> Option<(String, String)> {
    let ui = current_ui_strings();
    let dialog = Dialog::builder(parent, &ui.add_source)
        .with_style(DialogStyle::DefaultDialogStyle | DialogStyle::ResizeBorder)
        .with_size(520, 180)
        .build();
    let panel = Panel::builder(&dialog).build();
    let root = BoxSizer::builder(Orientation::Vertical).build();

    let title_row = BoxSizer::builder(Orientation::Horizontal).build();
    title_row.add(
        &StaticText::builder(&panel)
            .with_label(&ui.title_label)
            .build(),
        0,
        SizerFlag::AlignCenterVertical | SizerFlag::All,
        5,
    );
    let title_ctrl = TextCtrl::builder(&panel).build();
    title_row.add(&title_ctrl, 1, SizerFlag::Expand | SizerFlag::All, 5);
    root.add_sizer(&title_row, 0, SizerFlag::Expand, 0);

    let url_row = BoxSizer::builder(Orientation::Horizontal).build();
    url_row.add(
        &StaticText::builder(&panel)
            .with_label(&ui.url_or_source_label)
            .build(),
        0,
        SizerFlag::AlignCenterVertical | SizerFlag::All,
        5,
    );
    let url_ctrl = TextCtrl::builder(&panel).build();
    url_row.add(&url_ctrl, 1, SizerFlag::Expand | SizerFlag::All, 5);
    root.add_sizer(&url_row, 0, SizerFlag::Expand, 0);

    let buttons = BoxSizer::builder(Orientation::Horizontal).build();
    let ok_button = Button::builder(&panel)
        .with_id(ID_OK)
        .with_label(&ui.ok)
        .build();
    buttons.add_spacer(1);
    buttons.add(&ok_button, 0, SizerFlag::All, 10);
    root.add_sizer(&buttons, 0, SizerFlag::Expand, 0);
    panel.set_sizer(root, true);

    dialog.set_affirmative_id(ID_OK);
    let dialog_ok = dialog;
    ok_button.on_click(move |_| {
        dialog_ok.end_modal(ID_OK);
    });

    let result = if dialog.show_modal() == ID_OK {
        let title = title_ctrl.get_value();
        let url = url_ctrl.get_value();
        if url.trim().is_empty() {
            None
        } else {
            Some((title, url))
        }
    } else {
        None
    };

    dialog.destroy();
    result
}

fn open_edit_article_source_dialog(
    parent: &Frame,
    settings: &Arc<Mutex<Settings>>,
) -> Option<(usize, String, String)> {
    let ui = current_ui_strings();
    let sources = settings.lock().unwrap().article_sources.clone();
    if sources.is_empty() {
        return None;
    }

    let dialog = Dialog::builder(parent, &ui.edit_source)
        .with_style(DialogStyle::DefaultDialogStyle | DialogStyle::ResizeBorder)
        .with_size(560, 220)
        .build();
    let panel = Panel::builder(&dialog).build();
    let root = BoxSizer::builder(Orientation::Vertical).build();

    let source_row = BoxSizer::builder(Orientation::Horizontal).build();
    source_row.add(
        &StaticText::builder(&panel)
            .with_label(&ui.source_label)
            .build(),
        0,
        SizerFlag::AlignCenterVertical | SizerFlag::All,
        5,
    );
    let choice_source = Choice::builder(&panel).build();
    for source in &sources {
        let label = if source.title.trim().is_empty() {
            source.url.clone()
        } else {
            source.title.clone()
        };
        choice_source.append(&label);
    }
    choice_source.set_selection(0);
    source_row.add(&choice_source, 1, SizerFlag::Expand | SizerFlag::All, 5);
    root.add_sizer(&source_row, 0, SizerFlag::Expand, 0);

    let title_row = BoxSizer::builder(Orientation::Horizontal).build();
    title_row.add(
        &StaticText::builder(&panel)
            .with_label(&ui.title_label)
            .build(),
        0,
        SizerFlag::AlignCenterVertical | SizerFlag::All,
        5,
    );
    let title_ctrl = TextCtrl::builder(&panel).build();
    title_row.add(&title_ctrl, 1, SizerFlag::Expand | SizerFlag::All, 5);
    root.add_sizer(&title_row, 0, SizerFlag::Expand, 0);

    let url_row = BoxSizer::builder(Orientation::Horizontal).build();
    url_row.add(
        &StaticText::builder(&panel)
            .with_label(&ui.url_or_source_label)
            .build(),
        0,
        SizerFlag::AlignCenterVertical | SizerFlag::All,
        5,
    );
    let url_ctrl = TextCtrl::builder(&panel).build();
    url_row.add(&url_ctrl, 1, SizerFlag::Expand | SizerFlag::All, 5);
    root.add_sizer(&url_row, 0, SizerFlag::Expand, 0);

    let selected_index = Rc::new(RefCell::new(0usize));
    if let Some(source) = sources.first() {
        title_ctrl.set_value(&source.title);
        url_ctrl.set_value(&source.url);
    }

    let title_ctrl_evt = title_ctrl;
    let url_ctrl_evt = url_ctrl;
    let choice_source_evt = choice_source;
    let sources_evt = sources.clone();
    let selected_index_evt = Rc::clone(&selected_index);
    choice_source.on_selection_changed(move |_| {
        if let Some(selection) = choice_source_evt.get_selection() {
            let selection = selection as usize;
            *selected_index_evt.borrow_mut() = selection;
            if let Some(source) = sources_evt.get(selection) {
                title_ctrl_evt.set_value(&source.title);
                url_ctrl_evt.set_value(&source.url);
            }
        }
    });

    let buttons = BoxSizer::builder(Orientation::Horizontal).build();
    let ok_button = Button::builder(&panel)
        .with_id(ID_OK)
        .with_label("OK")
        .build();
    buttons.add_spacer(1);
    buttons.add(&ok_button, 0, SizerFlag::All, 10);
    root.add_sizer(&buttons, 0, SizerFlag::Expand, 0);
    panel.set_sizer(root, true);

    dialog.set_affirmative_id(ID_OK);
    let dialog_ok = dialog;
    ok_button.on_click(move |_| {
        dialog_ok.end_modal(ID_OK);
    });

    let result = if dialog.show_modal() == ID_OK {
        let url = url_ctrl.get_value();
        if url.trim().is_empty() {
            None
        } else {
            Some((*selected_index.borrow(), title_ctrl.get_value(), url))
        }
    } else {
        None
    };

    dialog.destroy();
    result
}

fn open_delete_article_source_dialog(
    parent: &Frame,
    settings: &Arc<Mutex<Settings>>,
) -> Option<usize> {
    let ui = current_ui_strings();
    let sources = settings.lock().unwrap().article_sources.clone();
    if sources.is_empty() {
        return None;
    }

    let dialog = Dialog::builder(parent, &ui.delete_source)
        .with_style(DialogStyle::DefaultDialogStyle | DialogStyle::ResizeBorder)
        .with_size(520, 160)
        .build();
    let panel = Panel::builder(&dialog).build();
    let root = BoxSizer::builder(Orientation::Vertical).build();

    let source_row = BoxSizer::builder(Orientation::Horizontal).build();
    source_row.add(
        &StaticText::builder(&panel)
            .with_label(&ui.source_label)
            .build(),
        0,
        SizerFlag::AlignCenterVertical | SizerFlag::All,
        5,
    );
    let choice_source = Choice::builder(&panel).build();
    for source in &sources {
        let label = if source.title.trim().is_empty() {
            source.url.clone()
        } else {
            source.title.clone()
        };
        choice_source.append(&label);
    }
    choice_source.set_selection(0);
    source_row.add(&choice_source, 1, SizerFlag::Expand | SizerFlag::All, 5);
    root.add_sizer(&source_row, 0, SizerFlag::Expand, 0);

    let selected_index = Rc::new(RefCell::new(0usize));
    let choice_source_evt = choice_source;
    let selected_index_evt = Rc::clone(&selected_index);
    choice_source.on_selection_changed(move |_| {
        if let Some(selection) = choice_source_evt.get_selection() {
            *selected_index_evt.borrow_mut() = selection as usize;
        }
    });

    let buttons = BoxSizer::builder(Orientation::Horizontal).build();
    let ok_button = Button::builder(&panel)
        .with_id(ID_OK)
        .with_label("OK")
        .build();
    buttons.add_spacer(1);
    buttons.add(&ok_button, 0, SizerFlag::All, 10);
    root.add_sizer(&buttons, 0, SizerFlag::Expand, 0);
    panel.set_sizer(root, true);

    dialog.set_affirmative_id(ID_OK);
    let dialog_ok = dialog;
    ok_button.on_click(move |_| {
        dialog_ok.end_modal(ID_OK);
    });

    let result = if dialog.show_modal() == ID_OK {
        Some(*selected_index.borrow())
    } else {
        None
    };

    dialog.destroy();
    result
}

fn open_article_folder_name_dialog(parent: &Dialog, title: &str, ui: &UiStrings) -> Option<String> {
    let dialog = Dialog::builder(parent, title)
        .with_style(DialogStyle::DefaultDialogStyle | DialogStyle::ResizeBorder)
        .with_size(420, 160)
        .build();
    let panel = Panel::builder(&dialog).build();
    let root = BoxSizer::builder(Orientation::Vertical).build();

    let row = BoxSizer::builder(Orientation::Horizontal).build();
    row.add(
        &StaticText::builder(&panel)
            .with_label(&ui.folder_name_label)
            .build(),
        0,
        SizerFlag::AlignCenterVertical | SizerFlag::All,
        5,
    );
    let text_ctrl = TextCtrl::builder(&panel).build();
    row.add(&text_ctrl, 1, SizerFlag::Expand | SizerFlag::All, 5);
    root.add_sizer(&row, 0, SizerFlag::Expand, 0);

    let buttons = BoxSizer::builder(Orientation::Horizontal).build();
    let ok_button = Button::builder(&panel)
        .with_id(ID_OK)
        .with_label(&ui.ok)
        .build();
    buttons.add_spacer(1);
    buttons.add(&ok_button, 0, SizerFlag::All, 10);
    root.add_sizer(&buttons, 0, SizerFlag::Expand, 0);
    panel.set_sizer(root, true);

    dialog.set_affirmative_id(ID_OK);
    let dialog_ok = dialog;
    ok_button.on_click(move |_| {
        dialog_ok.end_modal(ID_OK);
    });

    let result = if dialog.show_modal() == ID_OK {
        let value = text_ctrl.get_value();
        let value = normalize_article_folder_path(&value);
        if value.is_empty() { None } else { Some(value) }
    } else {
        None
    };

    dialog.destroy();
    result
}

fn open_article_folder_picker_dialog(
    parent: &Dialog,
    title: &str,
    ui: &UiStrings,
    folders: &[String],
) -> Option<String> {
    if folders.is_empty() {
        show_message_subdialog(parent, title, &ui.no_folders_available);
        return None;
    }

    let dialog = Dialog::builder(parent, title)
        .with_style(DialogStyle::DefaultDialogStyle | DialogStyle::ResizeBorder)
        .with_size(520, 180)
        .build();
    let panel = Panel::builder(&dialog).build();
    let root = BoxSizer::builder(Orientation::Vertical).build();

    let row = BoxSizer::builder(Orientation::Horizontal).build();
    row.add(
        &StaticText::builder(&panel)
            .with_label(&ui.folder_label)
            .build(),
        0,
        SizerFlag::AlignCenterVertical | SizerFlag::All,
        5,
    );
    let choice = Choice::builder(&panel).build();
    for folder in folders {
        choice.append(folder);
    }
    choice.set_selection(0);
    row.add(&choice, 1, SizerFlag::Expand | SizerFlag::All, 5);
    root.add_sizer(&row, 0, SizerFlag::Expand, 0);

    let buttons = BoxSizer::builder(Orientation::Horizontal).build();
    let ok_button = Button::builder(&panel)
        .with_id(ID_OK)
        .with_label(&ui.ok)
        .build();
    buttons.add_spacer(1);
    buttons.add(&ok_button, 0, SizerFlag::All, 10);
    root.add_sizer(&buttons, 0, SizerFlag::Expand, 0);
    panel.set_sizer(root, true);

    dialog.set_affirmative_id(ID_OK);
    let dialog_ok = dialog;
    ok_button.on_click(move |_| {
        dialog_ok.end_modal(ID_OK);
    });

    let result = if dialog.show_modal() == ID_OK {
        choice
            .get_selection()
            .and_then(|index| folders.get(index as usize))
            .cloned()
    } else {
        None
    };

    dialog.destroy();
    result
}

fn open_reorder_article_sources_dialog(
    parent: &Frame,
    settings: &Arc<Mutex<Settings>>,
) -> Option<(Vec<articles::ArticleSource>, Vec<String>)> {
    let ui = current_ui_strings();
    let (sources, folders) = {
        let locked = settings.lock().unwrap();
        (
            locked.article_sources.clone(),
            locked.article_folders.clone(),
        )
    };
    if sources.is_empty() {
        return None;
    }

    let dialog = Dialog::builder(parent, &ui.reorder_sources)
        .with_style(DialogStyle::DefaultDialogStyle | DialogStyle::ResizeBorder)
        .with_size(700, 260)
        .build();
    let panel = Panel::builder(&dialog).build();
    let root = BoxSizer::builder(Orientation::Vertical).build();

    let working_sources = Rc::new(RefCell::new(sources));
    let working_folders = Rc::new(RefCell::new(folders));
    let current_folder = Rc::new(RefCell::new(String::new()));

    let folder_display = StaticText::builder(&panel)
        .with_label(&ui.root_folder_name)
        .build();
    folder_display.hide();

    let source_row = BoxSizer::builder(Orientation::Horizontal).build();
    source_row.add(
        &StaticText::builder(&panel)
            .with_label(&ui.source_label)
            .build(),
        0,
        SizerFlag::AlignCenterVertical | SizerFlag::All,
        5,
    );
    let choice_source = Choice::builder(&panel).build();
    source_row.add(&choice_source, 1, SizerFlag::Expand | SizerFlag::All, 5);
    root.add_sizer(&source_row, 0, SizerFlag::Expand, 0);

    let action_row = BoxSizer::builder(Orientation::Horizontal).build();
    let move_up_button = Button::builder(&panel).with_label(&ui.move_up).build();
    let move_down_button = Button::builder(&panel).with_label(&ui.move_down).build();
    let open_folder_button = Button::builder(&panel).with_label(&ui.open_folder).build();
    let root_folder_button = Button::builder(&panel)
        .with_label(&ui.root_folder_name)
        .build();
    let parent_folder_button = Button::builder(&panel)
        .with_label(&ui.parent_folder)
        .build();
    let new_folder_button = Button::builder(&panel).with_label(&ui.new_folder).build();
    let move_to_folder_button = Button::builder(&panel)
        .with_label(&ui.move_to_folder)
        .build();
    let move_out_button = Button::builder(&panel)
        .with_label(&ui.move_out_of_folders)
        .build();
    action_row.add(&move_up_button, 1, SizerFlag::All, 5);
    action_row.add(&move_down_button, 1, SizerFlag::All, 5);
    action_row.add(&open_folder_button, 1, SizerFlag::All, 5);
    action_row.add(&root_folder_button, 1, SizerFlag::All, 5);
    action_row.add(&parent_folder_button, 1, SizerFlag::All, 5);
    action_row.add(&new_folder_button, 1, SizerFlag::All, 5);
    action_row.add(&move_to_folder_button, 1, SizerFlag::All, 5);
    action_row.add(&move_out_button, 1, SizerFlag::All, 5);
    root.add_sizer(&action_row, 0, SizerFlag::Expand, 0);

    let buttons = BoxSizer::builder(Orientation::Horizontal).build();
    let ok_button = Button::builder(&panel)
        .with_id(ID_OK)
        .with_label(&ui.ok)
        .build();
    buttons.add_spacer(1);
    buttons.add(&ok_button, 0, SizerFlag::All, 10);
    root.add_sizer(&buttons, 0, SizerFlag::Expand, 0);
    panel.set_sizer(root, true);

    let visible_source_indices = Rc::new(RefCell::new(Vec::<usize>::new()));
    let refresh_choice = Rc::new({
        let working_sources = Rc::clone(&working_sources);
        let working_folders = Rc::clone(&working_folders);
        let current_folder = Rc::clone(&current_folder);
        let visible_source_indices = Rc::clone(&visible_source_indices);
        move |choice: &Choice, folder_display: &StaticText, selected_index: usize| {
            choice.clear();
            let current_folder_value = current_folder.borrow().clone();
            folder_display.set_label(&article_folder_display_name(ui, &current_folder_value));
            let current_sources = working_sources.borrow();
            let current_folders = working_folders.borrow();
            let filtered_indices = current_sources
                .iter()
                .enumerate()
                .filter_map(|(index, source)| {
                    (normalize_article_folder_path(&source.folder_path) == current_folder_value)
                        .then_some(index)
                })
                .collect::<Vec<_>>();
            *visible_source_indices.borrow_mut() = filtered_indices.clone();
            for source_index in &filtered_indices {
                let label = article_source_label(&current_sources[*source_index]);
                choice.append(&label);
            }
            let max_index = filtered_indices.len().saturating_sub(1);
            if filtered_indices.is_empty() {
                choice.append(&ui.folder_empty);
                choice.set_selection(0);
                choice.enable(false);
            } else {
                choice.enable(true);
                choice.set_selection(selected_index.min(max_index) as u32);
            }
            let current_children = current_article_folder_children(
                &current_folder_value,
                &current_folders,
                &current_sources,
            );
            let _ = current_children;
        }
    });

    refresh_choice(&choice_source, &folder_display, 0);

    let selected_index = Rc::new(RefCell::new(0usize));

    let choice_source_evt = choice_source;
    let selected_index_evt = Rc::clone(&selected_index);
    choice_source.on_selection_changed(move |_| {
        if let Some(selection) = choice_source_evt.get_selection() {
            *selected_index_evt.borrow_mut() = selection as usize;
        }
    });

    let choice_source_up = choice_source;
    let folder_display_up = folder_display;
    let selected_index_up = Rc::clone(&selected_index);
    let working_sources_up = Rc::clone(&working_sources);
    let refresh_choice_up = Rc::clone(&refresh_choice);
    let visible_source_indices_up = Rc::clone(&visible_source_indices);
    let dialog_up = dialog;
    move_up_button.on_click(move |_| {
        let current_index = *selected_index_up.borrow();
        let visible_indices = visible_source_indices_up.borrow().clone();
        if current_index == 0 || visible_indices.len() < 2 {
            return;
        }
        let global_current = visible_indices[current_index];
        let global_target = visible_indices[current_index - 1];
        let (moved_label, target_label) = {
            let sources = working_sources_up.borrow();
            (
                article_source_label(&sources[global_current]),
                article_source_label(&sources[global_target]),
            )
        };
        {
            let mut sources = working_sources_up.borrow_mut();
            if !move_article_source_within_visible_list(
                &mut sources,
                &visible_indices,
                current_index,
                true,
            ) {
                return;
            }
        }
        let new_index = current_index - 1;
        *selected_index_up.borrow_mut() = new_index;
        refresh_choice_up(&choice_source_up, &folder_display_up, new_index);
        show_message_subdialog(
            &dialog_up,
            &ui.reorder_sources,
            &reorder_feedback_message(&moved_label, &target_label, true),
        );
    });

    let choice_source_down = choice_source;
    let folder_display_down = folder_display;
    let selected_index_down = Rc::clone(&selected_index);
    let working_sources_down = Rc::clone(&working_sources);
    let refresh_choice_down = Rc::clone(&refresh_choice);
    let visible_source_indices_down = Rc::clone(&visible_source_indices);
    let dialog_down = dialog;
    move_down_button.on_click(move |_| {
        let current_index = *selected_index_down.borrow();
        let visible_indices = visible_source_indices_down.borrow().clone();
        let len = visible_indices.len();
        if current_index + 1 >= len {
            return;
        }
        let global_current = visible_indices[current_index];
        let global_target = visible_indices[current_index + 1];
        let (moved_label, target_label) = {
            let sources = working_sources_down.borrow();
            (
                article_source_label(&sources[global_current]),
                article_source_label(&sources[global_target]),
            )
        };
        {
            let mut sources = working_sources_down.borrow_mut();
            if !move_article_source_within_visible_list(
                &mut sources,
                &visible_indices,
                current_index,
                false,
            ) {
                return;
            }
        }
        let new_index = current_index + 1;
        *selected_index_down.borrow_mut() = new_index;
        refresh_choice_down(&choice_source_down, &folder_display_down, new_index);
        show_message_subdialog(
            &dialog_down,
            &ui.reorder_sources,
            &reorder_feedback_message(&moved_label, &target_label, false),
        );
    });

    let choice_source_open = choice_source;
    let folder_display_open = folder_display;
    let selected_index_open = Rc::clone(&selected_index);
    let working_sources_open = Rc::clone(&working_sources);
    let working_folders_open = Rc::clone(&working_folders);
    let current_folder_open = Rc::clone(&current_folder);
    let refresh_choice_open = Rc::clone(&refresh_choice);
    let dialog_open = dialog;
    open_folder_button.on_click(move |_| {
        let folders = current_article_folder_children(
            &current_folder_open.borrow(),
            &working_folders_open.borrow(),
            &working_sources_open.borrow(),
        );
        let folder_labels = folders
            .iter()
            .map(|folder| article_folder_display_name(ui, folder))
            .collect::<Vec<_>>();
        if let Some(selection) =
            open_article_folder_picker_dialog(&dialog_open, &ui.open_folder, ui, &folder_labels)
            && let Some(folder_index) = folder_labels.iter().position(|label| label == &selection)
            && let Some(folder) = folders.get(folder_index)
        {
            *current_folder_open.borrow_mut() = folder.clone();
            *selected_index_open.borrow_mut() = 0;
            refresh_choice_open(&choice_source_open, &folder_display_open, 0);
        }
    });

    let choice_source_parent = choice_source;
    let folder_display_parent = folder_display;
    let selected_index_parent = Rc::clone(&selected_index);
    let current_folder_parent = Rc::clone(&current_folder);
    let refresh_choice_parent = Rc::clone(&refresh_choice);
    parent_folder_button.on_click(move |_| {
        if let Some(parent_folder) = article_parent_folder(&current_folder_parent.borrow()) {
            *current_folder_parent.borrow_mut() = parent_folder;
            *selected_index_parent.borrow_mut() = 0;
            refresh_choice_parent(&choice_source_parent, &folder_display_parent, 0);
        }
    });

    let choice_source_root = choice_source;
    let folder_display_root = folder_display;
    let selected_index_root = Rc::clone(&selected_index);
    let current_folder_root = Rc::clone(&current_folder);
    let refresh_choice_root = Rc::clone(&refresh_choice);
    root_folder_button.on_click(move |_| {
        *current_folder_root.borrow_mut() = String::new();
        *selected_index_root.borrow_mut() = 0;
        refresh_choice_root(&choice_source_root, &folder_display_root, 0);
    });

    let choice_source_new_folder = choice_source;
    let folder_display_new_folder = folder_display;
    let selected_index_new_folder = Rc::clone(&selected_index);
    let current_folder_new_folder = Rc::clone(&current_folder);
    let working_folders_new_folder = Rc::clone(&working_folders);
    let working_sources_new_folder = Rc::clone(&working_sources);
    let refresh_choice_new_folder = Rc::clone(&refresh_choice);
    let dialog_new_folder = dialog;
    new_folder_button.on_click(move |_| {
        if let Some(folder_name) =
            open_article_folder_name_dialog(&dialog_new_folder, &ui.new_folder, ui)
        {
            let new_folder_path = if current_folder_new_folder.borrow().is_empty() {
                folder_name
            } else {
                format!("{}/{}", current_folder_new_folder.borrow(), folder_name)
            };
            let mut folders = working_folders_new_folder.borrow_mut();
            if !folders.iter().any(|folder| folder == &new_folder_path) {
                folders.push(new_folder_path);
            }
            let normalized_folders =
                normalized_article_folders(&folders, &working_sources_new_folder.borrow());
            *folders = normalized_folders;
            *selected_index_new_folder.borrow_mut() = 0;
            refresh_choice_new_folder(&choice_source_new_folder, &folder_display_new_folder, 0);
        }
    });

    let choice_source_move = choice_source;
    let folder_display_move = folder_display;
    let selected_index_move = Rc::clone(&selected_index);
    let current_folder_move = Rc::clone(&current_folder);
    let working_sources_move = Rc::clone(&working_sources);
    let working_folders_move = Rc::clone(&working_folders);
    let visible_source_indices_move = Rc::clone(&visible_source_indices);
    let refresh_choice_move = Rc::clone(&refresh_choice);
    let dialog_move = dialog;
    move_to_folder_button.on_click(move |_| {
        let current_index = *selected_index_move.borrow();
        let visible_indices = visible_source_indices_move.borrow().clone();
        let Some(global_index) = visible_indices.get(current_index).copied() else {
            return;
        };
        let all_folders = normalized_article_folders(
            &working_folders_move.borrow(),
            &working_sources_move.borrow(),
        )
        .into_iter()
        .filter(|folder| folder != &normalize_article_folder_path(&current_folder_move.borrow()))
        .collect::<Vec<_>>();
        let folder_labels = all_folders.to_vec();
        if let Some(selection) =
            open_article_folder_picker_dialog(&dialog_move, &ui.move_to_folder, ui, &folder_labels)
            && let Some(folder_index) = folder_labels.iter().position(|label| label == &selection)
            && let Some(folder) = all_folders.get(folder_index)
        {
            let moved_label = article_source_label(&working_sources_move.borrow()[global_index]);
            let target_label = article_folder_display_name(ui, folder);
            working_sources_move.borrow_mut()[global_index].folder_path = folder.clone();
            *selected_index_move.borrow_mut() = 0;
            refresh_choice_move(&choice_source_move, &folder_display_move, 0);
            show_message_subdialog(
                &dialog_move,
                &ui.move_to_folder,
                &move_to_folder_feedback_message(&moved_label, &target_label),
            );
        }
    });

    let choice_source_move_out = choice_source;
    let folder_display_move_out = folder_display;
    let selected_index_move_out = Rc::clone(&selected_index);
    let working_sources_move_out = Rc::clone(&working_sources);
    let visible_source_indices_move_out = Rc::clone(&visible_source_indices);
    let refresh_choice_move_out = Rc::clone(&refresh_choice);
    let dialog_move_out = dialog;
    move_out_button.on_click(move |_| {
        let current_index = *selected_index_move_out.borrow();
        let visible_indices = visible_source_indices_move_out.borrow().clone();
        let Some(global_index) = visible_indices.get(current_index).copied() else {
            return;
        };
        let moved_label = article_source_label(&working_sources_move_out.borrow()[global_index]);
        let target_label = ui.root_folder_name.clone();
        working_sources_move_out.borrow_mut()[global_index]
            .folder_path
            .clear();
        *selected_index_move_out.borrow_mut() = 0;
        refresh_choice_move_out(&choice_source_move_out, &folder_display_move_out, 0);
        show_message_subdialog(
            &dialog_move_out,
            &ui.move_out_of_folders,
            &move_out_of_folders_feedback_message(&moved_label, &target_label),
        );
    });

    dialog.set_affirmative_id(ID_OK);
    let dialog_ok = dialog;
    ok_button.on_click(move |_| {
        dialog_ok.end_modal(ID_OK);
    });

    let result = if dialog.show_modal() == ID_OK {
        Some((
            working_sources.borrow().clone(),
            normalized_article_folders(&working_folders.borrow(), &working_sources.borrow()),
        ))
    } else {
        None
    };

    dialog.destroy();
    result
}

fn open_reorder_podcast_sources_dialog(
    parent: &Frame,
    settings: &Arc<Mutex<Settings>>,
) -> Option<Vec<podcasts::PodcastSource>> {
    let ui = current_ui_strings();
    let sources = settings.lock().unwrap().podcast_sources.clone();
    if sources.len() < 2 {
        return None;
    }

    let dialog = Dialog::builder(parent, &ui.reorder_podcasts)
        .with_style(DialogStyle::DefaultDialogStyle | DialogStyle::ResizeBorder)
        .with_size(560, 220)
        .build();
    let panel = Panel::builder(&dialog).build();
    let root = BoxSizer::builder(Orientation::Vertical).build();

    let working_sources = Rc::new(RefCell::new(sources));

    let source_row = BoxSizer::builder(Orientation::Horizontal).build();
    source_row.add(
        &StaticText::builder(&panel)
            .with_label(&ui.podcast_label)
            .build(),
        0,
        SizerFlag::AlignCenterVertical | SizerFlag::All,
        5,
    );
    let choice_source = Choice::builder(&panel).build();
    source_row.add(&choice_source, 1, SizerFlag::Expand | SizerFlag::All, 5);
    root.add_sizer(&source_row, 0, SizerFlag::Expand, 0);

    let action_row = BoxSizer::builder(Orientation::Horizontal).build();
    let move_up_button = Button::builder(&panel).with_label(&ui.move_up).build();
    let move_down_button = Button::builder(&panel).with_label(&ui.move_down).build();
    action_row.add(&move_up_button, 1, SizerFlag::All, 5);
    action_row.add(&move_down_button, 1, SizerFlag::All, 5);
    root.add_sizer(&action_row, 0, SizerFlag::Expand, 0);

    let buttons = BoxSizer::builder(Orientation::Horizontal).build();
    let ok_button = Button::builder(&panel)
        .with_id(ID_OK)
        .with_label("OK")
        .build();
    buttons.add_spacer(1);
    buttons.add(&ok_button, 0, SizerFlag::All, 10);
    root.add_sizer(&buttons, 0, SizerFlag::Expand, 0);
    panel.set_sizer(root, true);

    let refresh_choice = Rc::new({
        let working_sources = Rc::clone(&working_sources);
        move |choice: &Choice, selected_index: usize| {
            choice.clear();
            let current_sources = working_sources.borrow();
            for source in current_sources.iter() {
                choice.append(&podcast_source_label(source));
            }
            let max_index = current_sources.len().saturating_sub(1);
            choice.set_selection(selected_index.min(max_index) as u32);
        }
    });

    refresh_choice(&choice_source, 0);

    let selected_index = Rc::new(RefCell::new(0usize));

    let choice_source_evt = choice_source;
    let selected_index_evt = Rc::clone(&selected_index);
    choice_source.on_selection_changed(move |_| {
        if let Some(selection) = choice_source_evt.get_selection() {
            *selected_index_evt.borrow_mut() = selection as usize;
        }
    });

    let choice_source_up = choice_source;
    let selected_index_up = Rc::clone(&selected_index);
    let working_sources_up = Rc::clone(&working_sources);
    let refresh_choice_up = Rc::clone(&refresh_choice);
    let dialog_up = dialog;
    move_up_button.on_click(move |_| {
        let current_index = *selected_index_up.borrow();
        if current_index == 0 {
            return;
        }
        let (moved_label, target_label) = {
            let sources = working_sources_up.borrow();
            (
                podcast_source_label(&sources[current_index]),
                podcast_source_label(&sources[current_index - 1]),
            )
        };
        {
            let mut sources = working_sources_up.borrow_mut();
            sources.swap(current_index, current_index - 1);
        }
        let new_index = current_index - 1;
        *selected_index_up.borrow_mut() = new_index;
        refresh_choice_up(&choice_source_up, new_index);
        show_message_subdialog(
            &dialog_up,
            &ui.reorder_podcasts,
            &reorder_feedback_message(&moved_label, &target_label, true),
        );
    });

    let choice_source_down = choice_source;
    let selected_index_down = Rc::clone(&selected_index);
    let working_sources_down = Rc::clone(&working_sources);
    let refresh_choice_down = Rc::clone(&refresh_choice);
    let dialog_down = dialog;
    move_down_button.on_click(move |_| {
        let current_index = *selected_index_down.borrow();
        let len = working_sources_down.borrow().len();
        if current_index + 1 >= len {
            return;
        }
        let (moved_label, target_label) = {
            let sources = working_sources_down.borrow();
            (
                podcast_source_label(&sources[current_index]),
                podcast_source_label(&sources[current_index + 1]),
            )
        };
        {
            let mut sources = working_sources_down.borrow_mut();
            sources.swap(current_index, current_index + 1);
        }
        let new_index = current_index + 1;
        *selected_index_down.borrow_mut() = new_index;
        refresh_choice_down(&choice_source_down, new_index);
        show_message_subdialog(
            &dialog_down,
            &ui.reorder_podcasts,
            &reorder_feedback_message(&moved_label, &target_label, false),
        );
    });

    dialog.set_affirmative_id(ID_OK);
    let dialog_ok = dialog;
    ok_button.on_click(move |_| {
        dialog_ok.end_modal(ID_OK);
    });

    let result = if dialog.show_modal() == ID_OK {
        Some(working_sources.borrow().clone())
    } else {
        None
    };

    dialog.destroy();
    result
}

fn apply_loaded_voices(
    settings: &Arc<Mutex<Settings>>,
    voices_data: &Arc<Mutex<Vec<edge_tts::VoiceInfo>>>,
    languages: &Arc<Mutex<Vec<(String, String)>>>,
    voices: Vec<edge_tts::VoiceInfo>,
) {
    let ui_language = settings.lock().unwrap().ui_language.clone();
    let language_list = build_language_list(&voices, &ui_language);
    {
        let mut v_lock = voices_data.lock().unwrap();
        *v_lock = voices;
    }
    {
        let mut l_lock = languages.lock().unwrap();
        *l_lock = language_list.clone();
    }
    sync_settings_with_loaded_voices(settings, &voices_data.lock().unwrap(), &language_list);
}

fn refresh_playback_if_needed(playback: &Arc<Mutex<GlobalPlayback>>) {
    let mut pb = playback.lock().unwrap();
    if pb.status == PlaybackStatus::Playing {
        pb.refresh_requested = true;
        if let Some(ref sink) = pb.sink {
            sink.stop();
        }
    }
}

fn stop_tts_playback(playback: &Arc<Mutex<GlobalPlayback>>) {
    let mut pb = playback.lock().unwrap();
    if let Some(ref sink) = pb.sink {
        sink.stop();
    }
    pb.sink = None;
    pb.status = PlaybackStatus::Stopped;
    pb.refresh_requested = false;
    pb.download_finished = false;
    pb.generation = pb.generation.wrapping_add(1);
}

fn stop_podcast_playback(state: &Rc<RefCell<PodcastPlaybackState>>) {
    let mut podcast_state = state.borrow_mut();
    let current_audio_url = podcast_state.current_audio_url.clone();
    if let Some(player) = podcast_state.player.as_ref() {
        log_podcast_player_snapshot(player, "stop_podcast.before_pause", &current_audio_url);
        if let Err(err) = player.pause() {
            println!("ERROR: Pausa podcast fallita: {}", err);
            append_podcast_log(&format!(
                "stop_podcast.pause_error audio_url={} error={}",
                current_audio_url, err
            ));
        } else {
            log_podcast_player_snapshot(player, "stop_podcast.after_pause", &current_audio_url);
        }
    }
    podcast_state.player = None;
    podcast_state.status = PlaybackStatus::Stopped;
    append_podcast_log(&format!(
        "stop_podcast.completed audio_url={} status={:?}",
        current_audio_url, podcast_state.status
    ));
}

fn seek_podcast_playback(state: &Rc<RefCell<PodcastPlaybackState>>, offset_seconds: f64) {
    let podcast_state = state.borrow();
    if let Some(player) = podcast_state.player.as_ref() {
        log_podcast_player_snapshot(
            player,
            &format!("seek_podcast.before offset_seconds={offset_seconds}"),
            &podcast_state.current_audio_url,
        );
        if let Err(err) = player.seek_by_seconds(offset_seconds) {
            println!("ERROR: Seek podcast fallito: {}", err);
            append_podcast_log(&format!(
                "seek_podcast.error audio_url={} offset_seconds={} error={}",
                podcast_state.current_audio_url, offset_seconds, err
            ));
        } else {
            log_podcast_player_snapshot(
                player,
                &format!("seek_podcast.after offset_seconds={offset_seconds}"),
                &podcast_state.current_audio_url,
            );
        }
    }
}

fn seek_podcast_playback_to_ratio(state: &Rc<RefCell<PodcastPlaybackState>>, slider_value: i32) {
    let podcast_state = state.borrow();
    if let Some(player) = podcast_state.player.as_ref() {
        let Ok(Some(duration_seconds)) = player.duration_seconds() else {
            append_podcast_log(&format!(
                "seek_podcast.slider_no_duration audio_url={}",
                podcast_state.current_audio_url
            ));
            return;
        };
        let clamped_value = slider_value.clamp(0, PODCAST_SLIDER_RANGE);
        let target_seconds =
            duration_seconds * f64::from(clamped_value) / f64::from(PODCAST_SLIDER_RANGE);
        log_podcast_player_snapshot(
            player,
            &format!("seek_podcast.slider_before target_seconds={target_seconds}"),
            &podcast_state.current_audio_url,
        );
        if let Err(err) = player.seek_to_seconds(target_seconds) {
            println!("ERROR: Seek podcast da slider fallito: {}", err);
            append_podcast_log(&format!(
                "seek_podcast.slider_error audio_url={} target_seconds={} error={}",
                podcast_state.current_audio_url, target_seconds, err
            ));
        } else {
            log_podcast_player_snapshot(
                player,
                &format!("seek_podcast.slider_after target_seconds={target_seconds}"),
                &podcast_state.current_audio_url,
            );
        }
    }
}

fn podcast_slider_value(state: &PodcastPlaybackState) -> i32 {
    let Some(player) = state.player.as_ref() else {
        return 0;
    };
    let Ok(position_seconds) = player.position_seconds() else {
        return 0;
    };
    let Ok(Some(duration_seconds)) = player.duration_seconds() else {
        return 0;
    };
    if duration_seconds <= 0.0 {
        return 0;
    }
    ((position_seconds.max(0.0).min(duration_seconds) / duration_seconds)
        * f64::from(PODCAST_SLIDER_RANGE))
    .round()
    .clamp(0.0, f64::from(PODCAST_SLIDER_RANGE)) as i32
}

fn sync_settings_with_loaded_voices(
    settings: &Arc<Mutex<Settings>>,
    voices: &[edge_tts::VoiceInfo],
    languages: &[(String, String)],
) {
    if languages.is_empty() {
        return;
    }

    let mut changed = false;
    let mut s = settings.lock().unwrap();

    if !languages.iter().any(|(name, _)| name == &s.language) {
        if let Some(locale) = voices
            .iter()
            .find(|voice| voice.short_name == s.voice)
            .map(|voice| voice.locale.clone())
            && let Some((name, _)) = languages
                .iter()
                .find(|(_, item_locale)| *item_locale == locale)
        {
            s.language = name.clone();
            changed = true;
        } else if languages.iter().any(|(name, _)| name == "Italiano") {
            s.language = "Italiano".to_string();
            changed = true;
        } else if languages.iter().any(|(name, _)| name == "Italian") {
            s.language = "Italian".to_string();
            changed = true;
        } else if let Some((name, _)) = languages.first() {
            s.language = name.clone();
            changed = true;
        }
    }

    let locale = languages
        .iter()
        .find(|(name, _)| name == &s.language)
        .map(|(_, locale)| locale.clone());
    if let Some(locale) = locale {
        let available_voices: Vec<_> = voices.iter().filter(|v| v.locale == locale).collect();
        if !available_voices
            .iter()
            .any(|voice| voice.short_name == s.voice)
            && let Some(voice) = available_voices.first()
        {
            s.voice = voice.short_name.clone();
            changed = true;
        }
    }

    if changed {
        s.save();
    }
}

fn open_settings_dialog(
    parent: &Frame,
    settings: &Arc<Mutex<Settings>>,
    voices_data: &Arc<Mutex<Vec<edge_tts::VoiceInfo>>>,
    languages: &Arc<Mutex<Vec<(String, String)>>>,
    playback: &Arc<Mutex<GlobalPlayback>>,
) {
    let settings_before = settings.lock().unwrap().clone();
    let ui = ui_strings(&settings_before.ui_language);
    let voices_snapshot = voices_data.lock().unwrap().clone();
    let languages_snapshot = if voices_snapshot.is_empty() {
        languages.lock().unwrap().clone()
    } else {
        build_language_list(&voices_snapshot, &settings_before.ui_language)
    };
    let interface_languages = [("Italiano", "it"), ("English", "en")];

    let dialog = Dialog::builder(parent, &ui.settings_title)
        .with_style(DialogStyle::DefaultDialogStyle | DialogStyle::ResizeBorder)
        .with_size(560, if cfg!(target_os = "macos") { 380 } else { 320 })
        .build();
    let panel = Panel::builder(&dialog).build();
    let root = BoxSizer::builder(Orientation::Vertical).build();

    let ui_lang_row = BoxSizer::builder(Orientation::Horizontal).build();
    ui_lang_row.add(
        &StaticText::builder(&panel)
            .with_label(&ui.interface_language_label)
            .build(),
        0,
        SizerFlag::AlignCenterVertical | SizerFlag::All,
        5,
    );
    let choice_ui_lang = Choice::builder(&panel).build();
    ui_lang_row.add(&choice_ui_lang, 1, SizerFlag::Expand | SizerFlag::All, 5);
    root.add_sizer(&ui_lang_row, 0, SizerFlag::Expand, 0);

    let lang_row = BoxSizer::builder(Orientation::Horizontal).build();
    lang_row.add(
        &StaticText::builder(&panel)
            .with_label(&ui.voice_language_label)
            .build(),
        0,
        SizerFlag::AlignCenterVertical | SizerFlag::All,
        5,
    );
    let choice_lang = Choice::builder(&panel).build();
    lang_row.add(&choice_lang, 1, SizerFlag::Expand | SizerFlag::All, 5);
    root.add_sizer(&lang_row, 0, SizerFlag::Expand, 0);

    let voice_row = BoxSizer::builder(Orientation::Horizontal).build();
    voice_row.add(
        &StaticText::builder(&panel)
            .with_label(&ui.voice_label)
            .build(),
        0,
        SizerFlag::AlignCenterVertical | SizerFlag::All,
        5,
    );
    let choice_voices = Choice::builder(&panel).build();
    voice_row.add(&choice_voices, 1, SizerFlag::Expand | SizerFlag::All, 5);
    root.add_sizer(&voice_row, 0, SizerFlag::Expand, 0);

    let rate_row = BoxSizer::builder(Orientation::Horizontal).build();
    rate_row.add(
        &StaticText::builder(&panel)
            .with_label(&ui.rate_label)
            .build(),
        0,
        SizerFlag::AlignCenterVertical | SizerFlag::All,
        5,
    );
    let choice_rate = Choice::builder(&panel).build();
    for (label, _) in RATE_PRESETS {
        choice_rate.append(label);
    }
    choice_rate.set_selection(nearest_preset_index(&RATE_PRESETS, settings_before.rate) as u32);
    rate_row.add(&choice_rate, 1, SizerFlag::Expand | SizerFlag::All, 5);
    root.add_sizer(&rate_row, 0, SizerFlag::Expand, 0);

    let pitch_row = BoxSizer::builder(Orientation::Horizontal).build();
    pitch_row.add(
        &StaticText::builder(&panel)
            .with_label(&ui.pitch_label)
            .build(),
        0,
        SizerFlag::AlignCenterVertical | SizerFlag::All,
        5,
    );
    let choice_pitch = Choice::builder(&panel).build();
    for (label, _) in PITCH_PRESETS {
        choice_pitch.append(label);
    }
    choice_pitch.set_selection(nearest_preset_index(&PITCH_PRESETS, settings_before.pitch) as u32);
    pitch_row.add(&choice_pitch, 1, SizerFlag::Expand | SizerFlag::All, 5);
    root.add_sizer(&pitch_row, 0, SizerFlag::Expand, 0);

    let volume_row = BoxSizer::builder(Orientation::Horizontal).build();
    volume_row.add(
        &StaticText::builder(&panel)
            .with_label(&ui.volume_label)
            .build(),
        0,
        SizerFlag::AlignCenterVertical | SizerFlag::All,
        5,
    );
    let choice_volume = Choice::builder(&panel).build();
    for (label, _) in VOLUME_PRESETS {
        choice_volume.append(label);
    }
    choice_volume
        .set_selection(nearest_preset_index(&VOLUME_PRESETS, settings_before.volume) as u32);
    volume_row.add(&choice_volume, 1, SizerFlag::Expand | SizerFlag::All, 5);
    root.add_sizer(&volume_row, 0, SizerFlag::Expand, 0);

    #[cfg(target_os = "macos")]
    {
        let file_assoc_row = BoxSizer::builder(Orientation::Horizontal).build();
        file_assoc_row.add(
            &StaticText::builder(&panel)
                .with_label(&ui.file_associations_label)
                .build(),
            1,
            SizerFlag::AlignCenterVertical | SizerFlag::All,
            5,
        );
        let btn_file_associations = Button::builder(&panel)
            .with_label(&ui.file_associations_button)
            .build();
        file_assoc_row.add(&btn_file_associations, 0, SizerFlag::All, 5);
        root.add_sizer(&file_assoc_row, 0, SizerFlag::Expand, 0);

        let dialog_file_associations = dialog;
        let success_title = ui.settings_title.clone();
        let success_message = ui.file_associations_success_message.clone();
        let error_template = ui.file_associations_error_message.clone();
        btn_file_associations.on_click(move |_| match set_macos_default_file_associations() {
            Ok(()) => {
                show_message_subdialog(&dialog_file_associations, &success_title, &success_message)
            }
            Err(err) => show_message_subdialog(
                &dialog_file_associations,
                &success_title,
                &error_template.replace("{err}", &err),
            ),
        });
    }

    let button_row = BoxSizer::builder(Orientation::Horizontal).build();
    let btn_ok = Button::builder(&panel)
        .with_id(ID_OK)
        .with_label(&ui.ok)
        .build();
    button_row.add_spacer(1);
    button_row.add(&btn_ok, 0, SizerFlag::All, 10);
    root.add_sizer(&button_row, 0, SizerFlag::Expand, 0);

    panel.set_sizer(root, true);

    for (label, _) in interface_languages {
        choice_ui_lang.append(label);
    }
    if let Some(pos) = interface_languages
        .iter()
        .position(|(_, value)| *value == settings_before.ui_language)
    {
        choice_ui_lang.set_selection(pos as u32);
    } else {
        choice_ui_lang.set_selection(0);
    }

    for (name, _) in &languages_snapshot {
        choice_lang.append(name);
    }
    if let Some(pos) = languages_snapshot
        .iter()
        .position(|(name, _)| name == &settings_before.language)
    {
        choice_lang.set_selection(pos as u32);
    } else if let Some(locale) = voices_snapshot
        .iter()
        .find(|voice| voice.short_name == settings_before.voice)
        .map(|voice| voice.locale.clone())
        && let Some(pos) = languages_snapshot
            .iter()
            .position(|(_, item_locale)| *item_locale == locale)
    {
        choice_lang.set_selection(pos as u32);
    } else if let Some(pos) = languages_snapshot
        .iter()
        .position(|(name, _)| name == "Italiano")
    {
        choice_lang.set_selection(pos as u32);
    } else if !languages_snapshot.is_empty() {
        choice_lang.set_selection(0);
    }

    let selected_voice = Rc::new(RefCell::new(settings_before.voice.clone()));
    let filtered_voices = Rc::new(RefCell::new(Vec::<edge_tts::VoiceInfo>::new()));
    let filtered_voices_init = Rc::clone(&filtered_voices);
    let selected_voice_init = Rc::clone(&selected_voice);
    let choice_voices_fill = choice_voices;
    let choice_voices_evt = choice_voices;
    let choice_lang_evt = choice_lang;

    let populate_voices = Rc::new(move |lang_sel: u32| {
        let locale = languages_snapshot
            .get(lang_sel as usize)
            .map(|(_, locale)| locale.clone())
            .unwrap_or_default();
        let voice_list: Vec<_> = voices_snapshot
            .iter()
            .filter(|voice| voice.locale == locale)
            .cloned()
            .collect();
        choice_voices_fill.clear();
        for voice in &voice_list {
            choice_voices_fill.append(&voice.friendly_name);
        }

        let preferred = selected_voice_init.borrow().clone();
        if let Some(pos) = voice_list
            .iter()
            .position(|voice| voice.short_name == preferred)
        {
            choice_voices_fill.set_selection(pos as u32);
        } else if let Some(first) = voice_list.first() {
            choice_voices_fill.set_selection(0);
            *selected_voice_init.borrow_mut() = first.short_name.clone();
        } else {
            selected_voice_init.borrow_mut().clear();
        }
        *filtered_voices_init.borrow_mut() = voice_list;
    });

    if let Some(sel) = choice_lang.get_selection() {
        populate_voices(sel);
    }

    let populate_voices_evt = Rc::clone(&populate_voices);
    choice_lang.on_selection_changed(move |_| {
        if let Some(sel) = choice_lang_evt.get_selection() {
            populate_voices_evt(sel);
        }
    });

    let filtered_voices_choice = Rc::clone(&filtered_voices);
    let selected_voice_choice = Rc::clone(&selected_voice);
    choice_voices.on_selection_changed(move |_| {
        if let Some(sel) = choice_voices_evt.get_selection()
            && let Some(voice) = filtered_voices_choice.borrow().get(sel as usize)
        {
            *selected_voice_choice.borrow_mut() = voice.short_name.clone();
        }
    });

    dialog.set_affirmative_id(ID_OK);
    let dialog_ok = dialog;
    btn_ok.on_click(move |_| {
        dialog_ok.end_modal(ID_OK);
    });

    if dialog.show_modal() == ID_OK {
        let mut updated = settings_before.clone();
        if let Some(sel) = choice_ui_lang.get_selection()
            && let Some((_, value)) = interface_languages.get(sel as usize)
        {
            updated.ui_language = (*value).to_string();
        }
        if let Some(sel) = choice_lang.get_selection()
            && let Some((name, _)) = languages.lock().unwrap().get(sel as usize)
        {
            updated.language = name.clone();
        }
        let chosen_voice = selected_voice.borrow().clone();
        if !chosen_voice.is_empty() {
            updated.voice = chosen_voice;
        }
        if let Some(sel) = choice_rate.get_selection() {
            updated.rate = RATE_PRESETS[sel as usize].1;
        }
        if let Some(sel) = choice_pitch.get_selection() {
            updated.pitch = PITCH_PRESETS[sel as usize].1;
        }
        if let Some(sel) = choice_volume.get_selection() {
            updated.volume = VOLUME_PRESETS[sel as usize].1;
        }

        let refresh_needed = settings_before.voice != updated.voice
            || settings_before.rate != updated.rate
            || settings_before.pitch != updated.pitch
            || settings_before.volume != updated.volume;
        let changed = settings_before.ui_language != updated.ui_language
            || settings_before.language != updated.language
            || refresh_needed;

        if changed {
            let mut locked = settings.lock().unwrap();
            *locked = updated;
            locked.save();
        }
        if refresh_needed {
            refresh_playback_if_needed(playback);
        }
    }

    dialog.destroy();
}

fn main() {
    #[cfg(windows)]
    SystemOptions::set_option_by_int("msw.no-manifest-check", 1);

    append_podcast_log("app.start");

    #[cfg(target_os = "macos")]
    {
        let bundled_curl_libraries = articles::bundled_curl_impersonate_libraries();
        if bundled_curl_libraries.is_empty() {
            println!("INFO: Nessuna libreria curl-impersonate rilevata nel bundle macOS");
        } else {
            for library in bundled_curl_libraries {
                println!(
                    "INFO: Libreria curl-impersonate rilevata nel bundle macOS: {}",
                    library.display()
                );
            }
        }
    }

    let rt = Arc::new(Runtime::new().unwrap());
    let voices_data = Arc::new(Mutex::new(Vec::<edge_tts::VoiceInfo>::new()));
    let languages = Arc::new(Mutex::new(Vec::<(String, String)>::new()));
    let settings = Arc::new(Mutex::new(Settings::load()));
    #[cfg(target_os = "macos")]
    let pending_mac_update = Arc::new(Mutex::new(None::<PendingMacUpdateInstall>));
    let initial_radio_stations = embedded_radio_stations();
    let article_menu_state = Arc::new(Mutex::new(ArticleMenuState {
        dirty: true,
        loading_urls: HashSet::new(),
        pending_dialog: None,
    }));
    let podcast_menu_state = Arc::new(Mutex::new(PodcastMenuState {
        dirty: true,
        loading_urls: HashSet::new(),
        category_results: HashMap::new(),
        category_loading: HashSet::new(),
    }));
    let radio_menu_state = Arc::new(Mutex::new(RadioMenuState {
        dirty: true,
        loading_languages: HashSet::new(),
        failed_languages: HashSet::new(),
        stations_by_language: initial_radio_stations,
        station_ids: HashMap::new(),
        open_search_requested: false,
        search_ever_opened: false,
    }));
    let podcast_playback = Rc::new(RefCell::new(PodcastPlaybackState {
        player: None,
        selected_episode: None,
        current_audio_url: String::new(),
        status: PlaybackStatus::Stopped,
    }));

    let playback = Arc::new(Mutex::new(GlobalPlayback {
        sink: None,
        status: PlaybackStatus::Stopped,
        download_finished: false,
        refresh_requested: false,
        generation: 0,
        cached_tts: None,
    }));

    if let Some(cached_voices) = load_cached_voices() {
        apply_loaded_voices(&settings, &voices_data, &languages, cached_voices);
    }

    let rt_refresh = Arc::clone(&rt);
    let settings_refresh = Arc::clone(&settings);
    let voices_refresh = Arc::clone(&voices_data);
    let languages_refresh = Arc::clone(&languages);
    std::thread::spawn(
        move || match rt_refresh.block_on(edge_tts::get_edge_voices()) {
            Ok(voices) => {
                save_cached_voices(&voices);
                apply_loaded_voices(
                    &settings_refresh,
                    &voices_refresh,
                    &languages_refresh,
                    voices,
                );
            }
            Err(err) => {
                println!("ERROR: Aggiornamento voci fallito: {}", err);
            }
        },
    );

    refresh_all_article_sources(&rt, &settings, &article_menu_state);
    refresh_all_podcast_sources(&rt, &settings, &podcast_menu_state);
    refresh_all_podcast_categories(&rt, &podcast_menu_state);
    refresh_all_radio_languages(&radio_menu_state);
    let initial_open_path = initial_open_path_from_args();
    let pending_open_files = Arc::new(Mutex::new(Vec::<PathBuf>::new()));
    let current_document = Arc::new(Mutex::new(CurrentDocumentState::default()));

    let _ = wxdragon::main(move |_app| {
        #[cfg(target_os = "macos")]
        {
            let pending_open_files_app = Arc::clone(&pending_open_files);
            _app.on_open_files(move |files| {
                let mut pending = pending_open_files_app.lock().unwrap();
                for file in files {
                    let path = PathBuf::from(file);
                    if path.is_file() {
                        pending.push(path);
                    }
                }
            });
        }

        let ui = current_ui_strings();
        let frame = Frame::builder()
            .with_title("Sonarpad")
            .with_size(Size::new(800, 700))
            .build();

        let file_menu = Menu::builder().build();
        file_menu.append(ID_OPEN, &ui.menu_open, &ui.menu_open_help, ItemKind::Normal);
        #[cfg(target_os = "macos")]
        let save_text_menu_item = file_menu.append(
            ID_SAVE_TEXT,
            &ui.menu_save_text,
            &ui.menu_save_text_help,
            ItemKind::Normal,
        );
        #[cfg(target_os = "macos")]
        let save_text_as_menu_item = file_menu.append(
            ID_SAVE_TEXT_AS,
            &ui.menu_save_text_as,
            &ui.menu_save_text_as_help,
            ItemKind::Normal,
        );
        #[cfg(not(target_os = "macos"))]
        file_menu.append(
            ID_SAVE_TEXT,
            &ui.menu_save_text,
            &ui.menu_save_text_help,
            ItemKind::Normal,
        );
        #[cfg(not(target_os = "macos"))]
        file_menu.append(
            ID_SAVE_TEXT_AS,
            &ui.menu_save_text_as,
            &ui.menu_save_text_as_help,
            ItemKind::Normal,
        );
        file_menu.append_separator();
        #[cfg(target_os = "macos")]
        let start_menu_item = file_menu.append(
            ID_START_PLAYBACK,
            &ui.menu_start,
            &ui.menu_start_help,
            ItemKind::Normal,
        );
        #[cfg(target_os = "macos")]
        let play_menu_item = file_menu.append(
            ID_PLAY_PAUSE,
            &ui.menu_play_pause,
            &ui.menu_play_pause_help,
            ItemKind::Normal,
        );
        #[cfg(target_os = "macos")]
        let stop_menu_item =
            file_menu.append(ID_STOP, &ui.menu_stop, &ui.menu_stop_help, ItemKind::Normal);
        #[cfg(target_os = "macos")]
        let save_menu_item =
            file_menu.append(ID_SAVE, &ui.menu_save, &ui.menu_save_help, ItemKind::Normal);
        #[cfg(target_os = "macos")]
        let settings_menu_item = file_menu.append(
            ID_SETTINGS,
            &settings_menu_label(&ui.menu_settings),
            &ui.menu_settings_help,
            ItemKind::Normal,
        );
        #[cfg(target_os = "macos")]
        file_menu.append_separator();
        file_menu.append(ID_EXIT, &ui.menu_exit, &ui.menu_exit_help, ItemKind::Normal);
        let help_menu = Menu::builder().build();
        help_menu.append(
            ID_ABOUT,
            &ui.menu_about,
            &ui.menu_about_help,
            ItemKind::Normal,
        );
        help_menu.append(
            ID_DONATIONS,
            &ui.menu_donations,
            &ui.menu_donations_help,
            ItemKind::Normal,
        );
        help_menu.append(
            ID_CHANGELOG,
            &ui.menu_changelog,
            &ui.menu_changelog_help,
            ItemKind::Normal,
        );
        help_menu.append(
            ID_CHECK_UPDATES,
            &ui.menu_updates,
            &ui.menu_updates_help,
            ItemKind::Normal,
        );

        let articles_menu = Menu::builder().build();
        rebuild_articles_menu(&articles_menu, &settings, &HashSet::new());
        let articles_menu_timer = Menu::from(articles_menu.as_const_ptr());
        let articles_menu_settings = Menu::from(articles_menu.as_const_ptr());
        let podcasts_menu = Menu::builder().build();
        rebuild_podcasts_menu(
            &podcasts_menu,
            &settings,
            &HashSet::new(),
            &HashMap::new(),
            &HashSet::new(),
        );
        let podcasts_menu_timer = Menu::from(podcasts_menu.as_const_ptr());
        let podcasts_menu_settings = Menu::from(podcasts_menu.as_const_ptr());
        let radio_menu = Menu::builder().build();
        rebuild_radio_menu(&radio_menu, &settings, &radio_menu_state);
        let radio_menu_timer = Menu::from(radio_menu.as_const_ptr());
        let radio_menu_settings = Menu::from(radio_menu.as_const_ptr());

        #[cfg(target_os = "macos")]
        let menubar = MenuBar::builder()
            .append(file_menu, &ui.menu_file)
            .append(articles_menu, &ui.menu_articles)
            .append(podcasts_menu, &ui.menu_podcasts)
            .append(radio_menu, &ui.menu_radio)
            .append(help_menu, &ui.menu_help)
            .build();
        #[cfg(not(target_os = "macos"))]
        let menubar = MenuBar::builder()
            .append(file_menu, &ui.menu_file)
            .append(articles_menu, &ui.menu_articles)
            .append(podcasts_menu, &ui.menu_podcasts)
            .append(radio_menu, &ui.menu_radio)
            .append(help_menu, &ui.menu_help)
            .build();
        frame.set_menu_bar(menubar);

        #[cfg(target_os = "macos")]
        frame.track_menu_lifecycle(|_, is_opening| {
            set_mac_menu_bar_active(is_opening);
        });

        let panel = Panel::builder(&frame).build();
        let main_sizer = BoxSizer::builder(Orientation::Vertical).build();

        let text_ctrl = TextCtrl::builder(&panel)
            .with_style(TextCtrlStyle::MultiLine)
            .build();
        main_sizer.add(&text_ctrl, 1, SizerFlag::Expand | SizerFlag::All, 5);

        let btn_sizer = BoxSizer::builder(Orientation::Horizontal).build();
        let btn_start = Button::builder(&panel)
            .with_id(ID_START_PLAYBACK)
            .with_label(&start_button_label(false))
            .build();
        btn_sizer.add(&btn_start, 1, SizerFlag::All, 10);
        let btn_play = Button::builder(&panel)
            .with_id(ID_PLAY_PAUSE)
            .with_label(&play_button_label(PlaybackStatus::Stopped, false))
            .build();
        btn_sizer.add(&btn_play, 1, SizerFlag::All, 10);
        let btn_stop = Button::builder(&panel)
            .with_id(ID_STOP)
            .with_label(&stop_button_label(false))
            .build();
        btn_sizer.add(&btn_stop, 1, SizerFlag::All, 10);
        let btn_podcast_back = Button::builder(&panel)
            .with_id(ID_PODCAST_BACKWARD)
            .with_label(&format!("{} ({}+Left)", ui.button_back_30, MOD_CMD))
            .build();
        btn_podcast_back.show(false);
        btn_sizer.add(&btn_podcast_back, 1, SizerFlag::All, 10);
        let btn_podcast_forward = Button::builder(&panel)
            .with_id(ID_PODCAST_FORWARD)
            .with_label(&format!("{} ({}+Right)", ui.button_forward_30, MOD_CMD))
            .build();
        btn_podcast_forward.show(false);
        btn_sizer.add(&btn_podcast_forward, 1, SizerFlag::All, 10);
        let btn_save = Button::builder(&panel)
            .with_id(ID_SAVE)
            .with_label(&save_button_label())
            .build();
        btn_sizer.add(&btn_save, 1, SizerFlag::All, 10);
        let btn_settings = Button::builder(&panel)
            .with_id(ID_SETTINGS)
            .with_label(&settings_button_label())
            .build();
        btn_sizer.add(&btn_settings, 1, SizerFlag::All, 10);

        main_sizer.add_sizer(&btn_sizer, 0, SizerFlag::Expand, 0);
        let podcast_position_slider = Slider::builder(&panel)
            .with_min_value(0)
            .with_max_value(PODCAST_SLIDER_RANGE)
            .with_value(0)
            .build();
        podcast_position_slider.show(false);
        main_sizer.add(
            &podcast_position_slider,
            0,
            SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Bottom,
            10,
        );
        panel.set_sizer(main_sizer, true);

        // --- Timer per aggiornamento UI ---
        let timer = Rc::new(Timer::new(&frame));
        let pb_timer = Arc::clone(&playback);
        let btn_start_timer = btn_start;
        let btn_play_timer = btn_play;
        let btn_stop_timer = btn_stop;
        let btn_podcast_back_timer = btn_podcast_back;
        let btn_podcast_forward_timer = btn_podcast_forward;
        let podcast_position_slider_timer = podcast_position_slider;
        let panel_timer = panel;
        let settings_timer = Arc::clone(&settings);
        let article_menu_state_timer = Arc::clone(&article_menu_state);
        let podcast_menu_state_timer = Arc::clone(&podcast_menu_state);
        let radio_menu_state_timer = Arc::clone(&radio_menu_state);
        let podcast_playback_timer = Rc::clone(&podcast_playback);
        let rt_articles_timer = Arc::clone(&rt);
        let tc_articles_timer = text_ctrl;
        let pending_open_files_timer = Arc::clone(&pending_open_files);
        let current_document_timer = Arc::clone(&current_document);
        let timer_tick = Rc::clone(&timer);
        let frame_timer = frame;

        timer_tick.on_tick(move |_| {
            let tts_status = pb_timer.lock().unwrap().status;
            let (podcast_status, podcast_mode) = {
                let podcast_state = podcast_playback_timer.borrow();
                (
                    podcast_state.status,
                    podcast_state.selected_episode.is_some(),
                )
            };
            let start_label = start_button_label(podcast_mode);
            if btn_start_timer.get_label() != start_label {
                btn_start_timer.set_label(&start_label);
            }
            let label = play_button_label(
                if podcast_status != PlaybackStatus::Stopped {
                    podcast_status
                } else {
                    tts_status
                },
                podcast_mode,
            );
            if btn_play_timer.get_label() != label {
                btn_play_timer.set_label(&label);
            }
            let stop_label = stop_button_label(podcast_mode);
            if btn_stop_timer.get_label() != stop_label {
                btn_stop_timer.set_label(&stop_label);
            }
            #[cfg(target_os = "macos")]
            let seek_visible = false;
            #[cfg(not(target_os = "macos"))]
            let seek_visible = podcast_mode;
            btn_podcast_back_timer.show(seek_visible);
            btn_podcast_forward_timer.show(seek_visible);
            podcast_position_slider_timer.show(seek_visible);
            if seek_visible {
                let slider_value = {
                    let podcast_state = podcast_playback_timer.borrow();
                    podcast_slider_value(&podcast_state)
                };
                if podcast_position_slider_timer.get_value() != slider_value {
                    podcast_position_slider_timer.set_value(slider_value);
                }
            } else if podcast_position_slider_timer.get_value() != 0 {
                podcast_position_slider_timer.set_value(0);
            }
            panel_timer.layout();
            #[cfg(target_os = "macos")]
            if mac_should_defer_menu_rebuilds() {
                return;
            }

            let (article_loading_urls, pending_article_dialog) = {
                let mut article_state = article_menu_state_timer.lock().unwrap();
                let loading_urls = if article_state.dirty {
                    article_state.dirty = false;
                    Some(article_state.loading_urls.clone())
                } else {
                    None
                };
                let pending_dialog = article_state.pending_dialog.take();
                (loading_urls, pending_dialog)
            };
            if let Some(loading_urls) = article_loading_urls {
                rebuild_articles_menu(&articles_menu_timer, &settings_timer, &loading_urls);
            }
            if let Some(pending_dialog) = pending_article_dialog {
                append_podcast_log("article_menu.pending_dialog.open");
                let loading_urls = article_menu_state_timer
                    .lock()
                    .unwrap()
                    .loading_urls
                    .clone();
                let selected_item = match pending_dialog {
                    PendingArticleMenuDialog::Folder(folder_path) => {
                        append_podcast_log(&format!(
                            "article_menu.pending_dialog.folder path={folder_path}"
                        ));
                        open_article_folder_contents_dialog(
                            &frame_timer,
                            &settings_timer,
                            &loading_urls,
                            &folder_path,
                        )
                    }
                    PendingArticleMenuDialog::Source(source_index) => settings_timer
                        .lock()
                        .unwrap()
                        .article_sources
                        .get(source_index)
                        .cloned()
                        .and_then(|source| {
                            append_podcast_log(&format!(
                                "article_menu.pending_dialog.source index={} title={}",
                                source_index,
                                article_source_label(&source)
                            ));
                            open_article_source_items_dialog(
                                &frame_timer,
                                &source,
                                source_index,
                                &loading_urls,
                            )
                        }),
                };
                if let Some(item) = selected_item {
                    append_podcast_log(&format!(
                        "article_menu.pending_dialog.selected title={} link={}",
                        item.title, item.link
                    ));
                    show_article_item(
                        &item,
                        &rt_articles_timer,
                        &tc_articles_timer,
                        &podcast_playback_timer,
                    );
                } else {
                    append_podcast_log("article_menu.pending_dialog.no_selection");
                }
            }

            let podcast_menu_snapshot = {
                let mut podcast_state = podcast_menu_state_timer.lock().unwrap();
                if podcast_state.dirty {
                    podcast_state.dirty = false;
                    Some((
                        podcast_state.loading_urls.clone(),
                        podcast_state.category_results.clone(),
                        podcast_state.category_loading.clone(),
                    ))
                } else {
                    None
                }
            };
            if let Some((loading_urls, category_results, category_loading)) = podcast_menu_snapshot
            {
                rebuild_podcasts_menu(
                    &podcasts_menu_timer,
                    &settings_timer,
                    &loading_urls,
                    &category_results,
                    &category_loading,
                );
            }

            let radio_menu_dirty = {
                let mut radio_state = radio_menu_state_timer.lock().unwrap();
                if radio_state.dirty {
                    radio_state.dirty = false;
                    true
                } else {
                    false
                }
            };
            if radio_menu_dirty {
                rebuild_radio_menu(&radio_menu_timer, &settings_timer, &radio_menu_state_timer);
            }

            let open_search = {
                let mut state = radio_menu_state_timer.lock().unwrap();
                if state.open_search_requested {
                    state.open_search_requested = false;
                    true
                } else {
                    false
                }
            };
            if open_search {
                println!("DEBUG: Opening Radio Search Dialog from Timer");
                open_radio_search_dialog(&frame_timer, &settings_timer, &radio_menu_state_timer);
            }

            let pending_paths = {
                let mut pending = pending_open_files_timer.lock().unwrap();
                std::mem::take(&mut *pending)
            };
            for path in pending_paths {
                append_podcast_log(&format!(
                    "app.open_files_event.begin path={}",
                    path.display()
                ));
                match load_file_for_display(&frame_timer, &path) {
                    Ok(content) => {
                        podcast_playback_timer.borrow_mut().selected_episode = None;
                        tc_articles_timer.set_value(&content);
                        tc_articles_timer.set_modified(false);
                        set_current_document_state(&current_document_timer, Some(path.clone()));
                        append_podcast_log(&format!(
                            "app.open_files_event.loaded path={} length={}",
                            path.display(),
                            content.len()
                        ));
                    }
                    Err(err) => {
                        append_podcast_log(&format!(
                            "app.open_files_event.failed path={} err={}",
                            path.display(),
                            err
                        ));
                        let ui = current_ui_strings();
                        show_message_dialog(&frame_timer, &ui.open_document_title, &err);
                    }
                }
            }
        });
        timer.start(200, false);

        let timer_close = Rc::clone(&timer);
        let tc_close = text_ctrl;
        let settings_close = Arc::clone(&settings);
        let current_document_close = Arc::clone(&current_document);
        #[cfg(target_os = "macos")]
        let pending_mac_update_close = Arc::clone(&pending_mac_update);
        let frame_close = frame;
        frame.on_close(move |event| {
            if tc_close.is_modified() {
                let ui = current_ui_strings();
                let dialog = MessageDialog::builder(
                    &frame_close,
                    &ui.unsaved_changes_message,
                    &ui.unsaved_changes_title,
                )
                .with_style(
                    MessageDialogStyle::YesNo
                        | MessageDialogStyle::Cancel
                        | MessageDialogStyle::IconQuestion,
                )
                .build();
                localize_standard_dialog_buttons(&dialog);
                match dialog.show_modal() {
                    ID_YES => {
                        if !save_current_document(
                            &frame_close,
                            &settings_close,
                            &tc_close,
                            &current_document_close,
                        ) {
                            event.skip(false);
                            return;
                        }
                    }
                    ID_CANCEL => {
                        event.skip(false);
                        return;
                    }
                    _ => {}
                }
            }
            #[cfg(target_os = "macos")]
            if let Err(err) = launch_pending_macos_update_install(&pending_mac_update_close) {
                let ui = current_ui_strings();
                show_message_dialog(&frame_close, &ui.updates_title, &err);
                event.skip(false);
                return;
            }
            #[cfg(target_os = "macos")]
            stop_all_active_mac_radio_sessions();
            timer_close.stop();
            event.skip(true);
        });

        let timer_destroy = Rc::clone(&timer);
        frame.on_destroy(move |event| {
            timer_destroy.stop();
            event.skip(true);
        });

        // --- Menu ---
        let f_menu = frame;
        let tc_menu = text_ctrl;
        let settings_menu = Arc::clone(&settings);
        let current_document_menu = Arc::clone(&current_document);
        let article_menu_state_menu = Arc::clone(&article_menu_state);
        let podcast_menu_state_menu = Arc::clone(&podcast_menu_state);
        let radio_menu_state_menu = Arc::clone(&radio_menu_state);
        let rt_articles_menu = Arc::clone(&rt);
        let podcast_selection_menu = Rc::clone(&podcast_playback);
        frame.on_menu(move |event| {
            let ui = current_ui_strings();
            if event.get_id() == ID_OPEN {
                let dialog = FileDialog::builder(&f_menu).with_message(&ui.open).with_wildcard("Supportati|*.txt;*.doc;*.docx;*.pdf;*.epub;*.rtf;*.xlsx;*.xls;*.ods;*.html;*.htm;*.png;*.jpg;*.jpeg;*.gif;*.bmp;*.tif;*.tiff;*.webp;*.heic|Tutti|*.*").build();
                #[cfg(target_os = "macos")]
                set_mac_native_file_dialog_open(true);
                let dialog_result = dialog.show_modal();
                #[cfg(target_os = "macos")]
                set_mac_native_file_dialog_open(false);
                if dialog_result == ID_OK
                    && let Some(path) = dialog.get_path()
                {
                    let path = Path::new(&path);
                    let content = load_file_for_display(&f_menu, path);
                    if let Ok(c) = content {
                        podcast_selection_menu.borrow_mut().selected_episode = None;
                        tc_menu.set_value(&c);
                        tc_menu.set_modified(false);
                        set_current_document_state(&current_document_menu, Some(path.to_path_buf()));
                    }
                }
            } else if event.get_id() == ID_SAVE_TEXT {
                let _ = save_current_document(
                    &f_menu,
                    &settings_menu,
                    &tc_menu,
                    &current_document_menu,
                );
            } else if event.get_id() == ID_SAVE_TEXT_AS {
                let _ = save_current_document_as(
                    &f_menu,
                    &settings_menu,
                    &tc_menu,
                    &current_document_menu,
                );
            } else if event.get_id() == ID_EXIT {
                f_menu.close(true);
            } else if event.get_id() == ID_ABOUT {
                let dialog = MessageDialog::builder(&f_menu, &about_message(), about_title())
                    .with_style(MessageDialogStyle::OK | MessageDialogStyle::IconInformation)
                    .build();
                localize_standard_dialog_buttons(&dialog);
                dialog.show_modal();
            } else if event.get_id() == ID_DONATIONS {
                open_donations_dialog(&f_menu);
            } else if event.get_id() == ID_CHANGELOG {
                open_changelog_dialog(&f_menu);
            } else if event.get_id() == ID_CHECK_UPDATES {
                check_for_updates(
                    &f_menu,
                    #[cfg(target_os = "macos")]
                    &pending_mac_update,
                );
            } else if event.get_id() == ID_ARTICLES_ADD_SOURCE {
                if let Some((title, url)) = open_add_article_source_dialog(&f_menu) {
                    add_article_source(
                        title,
                        url,
                        &settings_menu,
                        &article_menu_state_menu,
                        &rt_articles_menu,
                    );
                }
            } else if event.get_id() == ID_ARTICLES_EDIT_SOURCE {
                if let Some((source_index, title, url)) =
                    open_edit_article_source_dialog(&f_menu, &settings_menu)
                {
                    edit_article_source(
                        source_index,
                        title,
                        url,
                        &settings_menu,
                        &article_menu_state_menu,
                        &rt_articles_menu,
                    );
                }
            } else if event.get_id() == ID_ARTICLES_DELETE_SOURCE {
                if let Some(source_index) =
                    open_delete_article_source_dialog(&f_menu, &settings_menu)
                    && confirm_delete_dialog(
                        &f_menu,
                        &ui.confirm_delete_title,
                        &ui.confirm_delete_rss_message,
                    )
                {
                    delete_article_source(
                        source_index,
                        &settings_menu,
                        &article_menu_state_menu,
                    );
                }
            } else if event.get_id() == ID_ARTICLES_REORDER_SOURCES {
                if let Some((reordered_sources, article_folders)) =
                    open_reorder_article_sources_dialog(&f_menu, &settings_menu)
                {
                    save_reordered_article_sources(
                        reordered_sources,
                        article_folders,
                        &settings_menu,
                        &article_menu_state_menu,
                    );
                }
            } else if event.get_id() == ID_ARTICLES_SORT_SOURCES_ALPHABETICALLY {
                sort_article_sources_alphabetically(&settings_menu, &article_menu_state_menu);
                show_message_dialog(
                    &f_menu,
                    &ui.sorted_articles_title,
                    &ui.sorted_articles_message,
                );
            } else if event.get_id() == ID_ARTICLES_IMPORT_SOURCES {
                if let Some(path) = open_article_sources_import_dialog(&f_menu) {
                    match import_article_sources_from_file(
                        &path,
                        &settings_menu,
                        &article_menu_state_menu,
                        &rt_articles_menu,
                    ) {
                        Ok(imported_count) => {
                            show_message_dialog(
                                &f_menu,
                                &ui.imported_articles_title,
                                &format!("{} {}", ui.imported_articles_message, imported_count),
                            );
                        }
                        Err(err) => {
                            show_message_dialog(
                                &f_menu,
                                &ui.import_articles_error_title,
                                &err,
                            );
                        }
                    }
                }
            } else if let Some(folder_index) = decode_article_folder_dialog_menu_id(event.get_id()) {
                let (sources, folders) = {
                    let locked = settings_menu.lock().unwrap();
                    (
                        locked.article_sources.clone(),
                        locked.article_folders.clone(),
                    )
                };
                let folder_catalog = article_folder_catalog(&folders, &sources);
                if let Some(folder_path) = folder_catalog.get(folder_index) {
                    article_menu_state_menu.lock().unwrap().pending_dialog =
                        Some(PendingArticleMenuDialog::Folder(folder_path.clone()));
                }
            } else if let Some(source_index) = decode_article_source_dialog_menu_id(event.get_id()) {
                article_menu_state_menu.lock().unwrap().pending_dialog =
                    Some(PendingArticleMenuDialog::Source(source_index));
            } else if event.get_id() == ID_ARTICLES_EXPORT_SOURCES {
                if let Some(path) = open_article_sources_export_dialog(&f_menu) {
                    match export_article_sources_to_opml(&path, &settings_menu) {
                        Ok(exported_count) => {
                            show_message_dialog(
                                &f_menu,
                                &ui.exported_articles_title,
                                &format!("{} {}", ui.exported_articles_message, exported_count),
                            );
                        }
                        Err(err) => {
                            show_message_dialog(
                                &f_menu,
                                &ui.export_articles_error_title,
                                &err,
                            );
                        }
                    }
                }
            } else if event.get_id() == ID_PODCASTS_ADD {
                if let Some(result) = open_add_podcast_dialog(&f_menu, &rt_articles_menu) {
                    add_podcast_source(
                        result,
                        &settings_menu,
                        &podcast_menu_state_menu,
                        &rt_articles_menu,
                    );
                }
            } else if event.get_id() == ID_PODCASTS_DELETE {
                if let Some(source_index) = open_delete_podcast_dialog(&f_menu, &settings_menu)
                    && confirm_delete_dialog(
                        &f_menu,
                        &ui.confirm_delete_title,
                        &ui.confirm_delete_podcast_message,
                    )
                {
                    delete_podcast_source(source_index, &settings_menu, &podcast_menu_state_menu);
                }
            } else if event.get_id() == ID_PODCASTS_REORDER_SOURCES {
                if let Some(reordered_sources) =
                    open_reorder_podcast_sources_dialog(&f_menu, &settings_menu)
                {
                    save_reordered_podcast_sources(
                        reordered_sources,
                        &settings_menu,
                        &podcast_menu_state_menu,
                    );
                }
            } else if event.get_id() == ID_PODCASTS_SORT_SOURCES_ALPHABETICALLY {
                sort_podcast_sources_alphabetically(&settings_menu, &podcast_menu_state_menu);
                show_message_dialog(
                    &f_menu,
                    &ui.sorted_podcasts_title,
                    &ui.sorted_podcasts_message,
                );
            } else if event.get_id() == ID_RADIO_SEARCH {
                println!("DEBUG: Menu Cerca radio cliccato - setting request flag");
                append_podcast_log("menu.radio_search.clicked_set_flag");
                radio_menu_state_menu.lock().unwrap().open_search_requested = true;
            } else if event.get_id() == ID_RADIO_ADD {
                if let Some((title, url)) = open_add_radio_dialog(&f_menu) {
                    let mut settings = settings_menu.lock().unwrap();
                    let favorite = RadioFavorite {
                        name: title,
                        stream_url: url,
                        language_code: "custom".to_string(), // Possiamo usare un codice custom o it di default
                    };
                    if !settings.radio_favorites.iter().any(|f| f.stream_url == favorite.stream_url) {
                        settings.radio_favorites.push(favorite);
                        normalize_settings_data(&mut settings);
                        settings.save();
                        drop(settings);
                        radio_menu_state_menu.lock().unwrap().dirty = true;
                    }
                }
            } else if event.get_id() == ID_RADIO_EDIT_FAVORITE {
                if let Some((index, title, url)) =
                    open_edit_radio_favorite_dialog(&f_menu, &settings_menu)
                {
                    let updated = {
                        let mut settings = settings_menu.lock().unwrap();
                        if index < settings.radio_favorites.len() {
                            let language_code = settings.radio_favorites[index].language_code.clone();
                            settings.radio_favorites[index] = RadioFavorite {
                                language_code,
                                name: title,
                                stream_url: url,
                            };
                            normalize_settings_data(&mut settings);
                            settings.save();
                            true
                        } else {
                            false
                        }
                    };
                    if updated {
                        radio_menu_state_menu.lock().unwrap().dirty = true;
                    }
                }
            } else if event.get_id() == ID_RADIO_REORDER_FAVORITES {
                if let Some(reordered_favorites) =
                    open_reorder_radio_favorites_dialog(&f_menu, &settings_menu)
                {
                    save_reordered_radio_favorites(
                        reordered_favorites,
                        &settings_menu,
                        &radio_menu_state_menu,
                    );
                }
            } else if event.get_id() == ID_RADIO_DELETE_FAVORITE {
                if let Some(index) = open_delete_radio_favorite_dialog(&f_menu, &settings_menu) {
                    let removed = {
                        let mut settings = settings_menu.lock().unwrap();
                        if index < settings.radio_favorites.len() {
                            let removed = settings.radio_favorites.remove(index);
                            normalize_settings_data(&mut settings);
                            settings.save();
                            Some(removed)
                        } else {
                            None
                        }
                    };
                    if let Some(removed) = removed {
                        radio_menu_state_menu.lock().unwrap().dirty = true;
                        show_message_dialog(
                            &f_menu,
                            &ui.menu_radio.replace('&', ""),
                            &if Settings::load().ui_language == "it" {
                                format!("{} rimossa dai preferiti.", removed.name)
                            } else {
                                format!("{} removed from favorites.", removed.name)
                            },
                        );
                    }
                }
            } else if let Some((category_index, result_index)) =
                decode_podcast_category_podcast_menu_id(event.get_id())
            {
                let categories = podcasts::apple_categories(&settings_menu.lock().unwrap().ui_language);
                if let Some(category) = categories.get(category_index) {
                    let result = {
                        let state = podcast_menu_state_menu.lock().unwrap();
                        state
                            .category_results
                            .get(&category.id)
                            .and_then(|results| results.get(result_index))
                            .cloned()
                    };
                    if let Some(result) = result {
                        add_podcast_source(
                            result,
                            &settings_menu,
                            &podcast_menu_state_menu,
                            &rt_articles_menu,
                        );
                    }
                }
            } else if let Some((source_index, episode_index)) =
                decode_podcast_episode_menu_id(event.get_id())
            {
                append_podcast_log(&format!(
                    "podcast_menu.select source_index={} episode_index={} event_id={}",
                    source_index,
                    episode_index,
                    event.get_id()
                ));
                let episode = settings_menu
                    .lock()
                    .unwrap()
                    .podcast_sources
                    .get(source_index)
                    .and_then(|source| source.episodes.get(episode_index))
                    .cloned();
                if let Some(episode) = episode {
                    let description = crate::reader::collapse_blank_lines(
                        &crate::reader::clean_text(&episode.description),
                    );
                    tc_menu.set_value(&format!("{}\n\n{}", episode.title.trim(), description.trim()));

                    if episode.audio_url.trim().is_empty() {
                        append_podcast_log(&format!(
                            "podcast_menu.no_audio_url title={} link={}",
                            episode.title, episode.link
                        ));
                        let dialog = MessageDialog::builder(
                            &f_menu,
                            "Questo episodio non espone un URL audio diretto nel feed RSS.\n\nNon posso scaricare la pagina web al posto dell'audio.",
                            "Audio podcast non disponibile",
                        )
                        .with_style(MessageDialogStyle::OK | MessageDialogStyle::IconError)
                        .build();
                        localize_standard_dialog_buttons(&dialog);
                        dialog.show_modal();
                        return;
                    }

                    append_podcast_log(&format!(
                        "podcast_menu.episode_resolved title={} audio_url={} link={}",
                        episode.title, episode.audio_url, episode.link
                    ));

                    #[cfg(any(target_os = "macos", windows))]
                    {
                        let external_url = episode.audio_url.as_str();
                        let mut playback_state = podcast_selection_menu.borrow_mut();
                        if let Some(player) = playback_state.player.as_ref()
                            && let Err(err) = player.pause()
                        {
                            println!("ERROR: Pausa podcast fallita: {}", err);
                            append_podcast_log(&format!(
                                "podcast_menu.previous_pause_error audio_url={} error={}",
                                playback_state.current_audio_url, err
                            ));
                        }
                        playback_state.player = None;
                        playback_state.selected_episode = None;
                        playback_state.current_audio_url.clear();
                        playback_state.status = PlaybackStatus::Stopped;
                        drop(playback_state);
                        append_podcast_log("podcast_menu.external_open_call");

                        if let Err(err) =
                            open_podcast_episode_externally(&f_menu, external_url, &episode.title)
                        {
                            append_podcast_log(&format!(
                                "podcast_menu.external_open_error error={}",
                                err
                            ));
                            println!("ERROR: Apertura esterna podcast fallita: {}", err);
                            let dialog = MessageDialog::builder(
                                &f_menu,
                                &if Settings::load().ui_language == "it" {
                                    format!("Impossibile aprire il podcast.\n\n{err}")
                                } else {
                                    format!("Unable to open the podcast.\n\n{err}")
                                },
                                &current_ui_strings().podcast_error_title,
                            )
                            .with_style(MessageDialogStyle::OK | MessageDialogStyle::IconError)
                            .build();
                            localize_standard_dialog_buttons(&dialog);
                            dialog.show_modal();
                        } else {
                            append_podcast_log("podcast_menu.external_open_ok");
                        }
                    }

                    #[cfg(not(any(target_os = "macos", windows)))]
                    {
                        podcast_selection_menu.borrow_mut().selected_episode = Some(episode.clone());
                    }
                }
            } else if let Some(station) = {
                let state = radio_menu_state_menu.lock().unwrap();
                state.station_ids.get(&event.get_id()).cloned()
            } {
                append_podcast_log(&format!(
                    "menu.radio.favorite.open name={} url={}",
                    station.name, station.stream_url
                ));
                if let Err(err) = open_radio_station(&f_menu, &station.name, &station.stream_url) {
                    show_message_dialog(
                        &f_menu,
                        &ui.menu_radio,
                        &ui.radio_open_failed.replace("{err}", &err),
                    );
                }
            } else if let Some((source_index, item_index)) = decode_article_menu_id(event.get_id()) {
                let item = settings_menu
                    .lock()
                    .unwrap()
                    .article_sources
                    .get(source_index)
                    .and_then(|source| source.items.get(item_index))
                    .cloned();
                if let Some(item) = item {
                    show_article_item(&item, &rt_articles_menu, &tc_menu, &podcast_selection_menu);
                }
            }
        });

        // --- Play / Pausa / Stop ---
        let pb_p = Arc::clone(&playback);
        let b_p_label = btn_play;
        let f_play = frame;
        let podcast_playback_play = Rc::clone(&podcast_playback);
        let play_action: Rc<dyn Fn()> = Rc::new(move || {
            let selected_episode = podcast_playback_play.borrow().selected_episode.clone();
            if let Some(episode) = selected_episode
                && !episode.audio_url.trim().is_empty()
            {
                stop_tts_playback(&pb_p);
                append_podcast_log(&format!(
                    "play_action.selected_episode title={} audio_url={} previous_status={:?}",
                    episode.title,
                    episode.audio_url,
                    podcast_playback_play.borrow().status
                ));

                let mut podcast_state = podcast_playback_play.borrow_mut();
                let needs_new_player = podcast_state.player.is_none()
                    || !podcast_state
                        .current_audio_url
                        .eq_ignore_ascii_case(&episode.audio_url);

                if needs_new_player {
                    match podcast_player::PodcastPlayer::new(&episode.audio_url) {
                        Ok(player) => {
                            log_podcast_player_snapshot(
                                &player,
                                "play_action.new_player",
                                &episode.audio_url,
                            );
                            podcast_state.player = Some(player);
                            podcast_state.current_audio_url = episode.audio_url.clone();
                        }
                        Err(err) => {
                            println!("ERROR: Avvio player podcast fallito: {}", err);
                            append_podcast_log(&format!(
                                "play_action.new_player_error audio_url={} error={}",
                                episode.audio_url, err
                            ));
                            podcast_state.status = PlaybackStatus::Stopped;
                            return;
                        }
                    }
                }

                match podcast_state.status {
                    PlaybackStatus::Playing => {
                        if let Some(player) = podcast_state.player.as_ref() {
                            log_podcast_player_snapshot(
                                player,
                                "play_action.pause.before",
                                &episode.audio_url,
                            );
                            if let Err(err) = player.pause() {
                                println!("ERROR: Pausa podcast fallita: {}", err);
                                append_podcast_log(&format!(
                                    "play_action.pause.error audio_url={} error={}",
                                    episode.audio_url, err
                                ));
                                podcast_state.status = PlaybackStatus::Stopped;
                                return;
                            }
                            log_podcast_player_snapshot(
                                player,
                                "play_action.pause.after",
                                &episode.audio_url,
                            );
                        }
                        podcast_state.status = PlaybackStatus::Paused;
                        b_p_label.set_label(&play_button_label(PlaybackStatus::Paused, true));
                    }
                    PlaybackStatus::Paused => {
                        if let Some(player) = podcast_state.player.as_ref() {
                            log_podcast_player_snapshot(
                                player,
                                "play_action.resume.before",
                                &episode.audio_url,
                            );
                            if let Err(err) = player.play() {
                                println!("ERROR: Ripresa podcast fallita: {}", err);
                                append_podcast_log(&format!(
                                    "play_action.resume.error audio_url={} error={}",
                                    episode.audio_url, err
                                ));
                                podcast_state.status = PlaybackStatus::Stopped;
                                return;
                            }
                            log_podcast_player_snapshot(
                                player,
                                "play_action.resume.after",
                                &episode.audio_url,
                            );
                            if needs_new_player
                                && !wait_for_podcast_ready(&f_play, player, &episode.audio_url)
                            {
                                if let Err(err) = player.pause() {
                                    println!("ERROR: Pausa podcast dopo timeout fallita: {}", err);
                                    append_podcast_log(&format!(
                                        "play_action.resume.cleanup_error audio_url={} error={}",
                                        episode.audio_url, err
                                    ));
                                }
                                podcast_state.status = PlaybackStatus::Stopped;
                                return;
                            }
                        }
                        podcast_state.status = PlaybackStatus::Playing;
                        b_p_label.set_label(&play_button_label(PlaybackStatus::Playing, true));
                    }
                    PlaybackStatus::Stopped => {}
                }
                append_podcast_log(&format!(
                    "play_action.completed audio_url={} new_status={:?}",
                    episode.audio_url, podcast_state.status
                ));
                return;
            }

            stop_podcast_playback(&podcast_playback_play);
            let mut pb = pb_p.lock().unwrap();
            match pb.status {
                PlaybackStatus::Playing => {
                    if let Some(ref s) = pb.sink {
                        s.pause();
                        pb.status = PlaybackStatus::Paused;
                        b_p_label.set_label(&play_button_label(PlaybackStatus::Paused, false));
                    }
                }
                PlaybackStatus::Paused => {
                    if let Some(ref s) = pb.sink {
                        s.play();
                        pb.status = PlaybackStatus::Playing;
                        b_p_label.set_label(&play_button_label(PlaybackStatus::Playing, false));
                    }
                }
                PlaybackStatus::Stopped => {}
            }
        });

        let rt_p_start = Arc::clone(&rt);
        let pb_p_start = Arc::clone(&playback);
        let tc_p_start = text_ctrl;
        let f_play_start = frame;
        let s_play_start = Arc::clone(&settings);
        let podcast_playback_start = Rc::clone(&podcast_playback);
        let start_action: Rc<dyn Fn()> = Rc::new(move || {
            let selected_episode = podcast_playback_start.borrow().selected_episode.clone();
            if let Some(episode) = selected_episode
                && !episode.audio_url.trim().is_empty()
            {
                stop_tts_playback(&pb_p_start);
                append_podcast_log(&format!(
                    "start_action.selected_episode title={} audio_url={} previous_status={:?}",
                    episode.title,
                    episode.audio_url,
                    podcast_playback_start.borrow().status
                ));

                let mut podcast_state = podcast_playback_start.borrow_mut();
                if podcast_state.status != PlaybackStatus::Stopped {
                    return;
                }

                let needs_new_player = podcast_state.player.is_none()
                    || !podcast_state
                        .current_audio_url
                        .eq_ignore_ascii_case(&episode.audio_url);

                if needs_new_player {
                    match podcast_player::PodcastPlayer::new(&episode.audio_url) {
                        Ok(player) => {
                            log_podcast_player_snapshot(
                                &player,
                                "start_action.new_player",
                                &episode.audio_url,
                            );
                            podcast_state.player = Some(player);
                            podcast_state.current_audio_url = episode.audio_url.clone();
                        }
                        Err(err) => {
                            println!("ERROR: Avvio player podcast fallito: {}", err);
                            append_podcast_log(&format!(
                                "start_action.new_player_error audio_url={} error={}",
                                episode.audio_url, err
                            ));
                            podcast_state.status = PlaybackStatus::Stopped;
                            return;
                        }
                    }
                }

                if let Some(player) = podcast_state.player.as_ref() {
                    log_podcast_player_snapshot(
                        player,
                        "start_action.play.before",
                        &episode.audio_url,
                    );
                    if let Err(err) = player.play() {
                        println!("ERROR: Riproduzione podcast fallita: {}", err);
                        append_podcast_log(&format!(
                            "start_action.play.error audio_url={} error={}",
                            episode.audio_url, err
                        ));
                        podcast_state.status = PlaybackStatus::Stopped;
                        return;
                    }
                    log_podcast_player_snapshot(
                        player,
                        "start_action.play.after",
                        &episode.audio_url,
                    );
                    if !wait_for_podcast_ready(&f_play_start, player, &episode.audio_url) {
                        if let Err(err) = player.pause() {
                            println!("ERROR: Pausa podcast dopo timeout fallita: {}", err);
                            append_podcast_log(&format!(
                                "start_action.play.cleanup_error audio_url={} error={}",
                                episode.audio_url, err
                            ));
                        }
                        podcast_state.status = PlaybackStatus::Stopped;
                        return;
                    }
                }

                podcast_state.current_audio_url = episode.audio_url.clone();
                podcast_state.status = PlaybackStatus::Playing;
                append_podcast_log(&format!(
                    "start_action.completed audio_url={} new_status={:?}",
                    episode.audio_url, podcast_state.status
                ));
                return;
            }

            stop_podcast_playback(&podcast_playback_start);
            let previous_status = {
                let pb = pb_p_start.lock().unwrap();
                pb.status
            };
            if previous_status != PlaybackStatus::Stopped {
                append_podcast_log(&format!(
                    "start_action.tts_restart previous_status={:?}",
                    previous_status
                ));
                stop_tts_playback(&pb_p_start);
            }
            let text = tc_p_start.get_value();
            if text.trim().is_empty() {
                append_podcast_log("start_action.text_empty");
                return;
            }
            let (voice, rate, pitch, volume) = {
                let s = s_play_start.lock().unwrap();
                (s.voice.clone(), s.rate, s.pitch, s.volume)
            };
            let mut pb = pb_p_start.lock().unwrap();
            append_podcast_log(&format!(
                "start_action.tts_begin chars={} trimmed_chars={}",
                text.len(),
                text.trim().len()
            ));

            pb.status = PlaybackStatus::Playing;
            pb.download_finished = false;
            pb.refresh_requested = false;
            pb.generation = pb.generation.wrapping_add(1);
            let playback_generation = pb.generation;
            let cached_tts = pb.cached_tts.clone();
            drop(pb);

            let pb_thread = Arc::clone(&pb_p_start);
            if let Some(cached) = cached_tts.filter(|cached| {
                cached.text == text
                    && cached.voice == voice
                    && cached.rate == rate
                    && cached.pitch == pitch
                    && cached.volume == volume
            }) {
                std::thread::spawn(move || {
                    append_podcast_log("start_action.tts_cache_hit");
                    let Ok((_stream, handle)) = OutputStream::try_default() else {
                        let mut pb_lock = pb_thread.lock().unwrap();
                        if pb_lock.generation == playback_generation {
                            append_podcast_log("start_action.audio_output_failed");
                            pb_lock.status = PlaybackStatus::Stopped;
                            pb_lock.sink = None;
                        }
                        return;
                    };
                    let Ok(sink) = Sink::try_new(&handle) else {
                        let mut pb_lock = pb_thread.lock().unwrap();
                        if pb_lock.generation == playback_generation {
                            append_podcast_log("start_action.audio_sink_failed");
                            pb_lock.status = PlaybackStatus::Stopped;
                            pb_lock.sink = None;
                        }
                        return;
                    };

                    let sink_arc = Arc::new(sink);
                    {
                        let mut pb_lock = pb_thread.lock().unwrap();
                        if pb_lock.generation != playback_generation {
                            return;
                        }
                        pb_lock.sink = Some(Arc::clone(&sink_arc));
                        pb_lock.download_finished = true;
                    }

                    for (chunk_index, data) in cached.chunks.into_iter().enumerate() {
                        {
                            let pb_lock = pb_thread.lock().unwrap();
                            if pb_lock.generation != playback_generation
                                || pb_lock.status == PlaybackStatus::Stopped
                            {
                                return;
                            }
                        }
                        if let Ok(source) = Decoder::new(Cursor::new(data)) {
                            sink_arc.append(source);
                        } else {
                            append_podcast_log(&format!(
                                "start_action.decoder_failed index={}",
                                chunk_index
                            ));
                        }
                    }

                    append_podcast_log("start_action.tts_download_finished");
                    loop {
                        std::thread::sleep(std::time::Duration::from_millis(200));
                        let mut pb_lock = pb_thread.lock().unwrap();
                        if pb_lock.generation != playback_generation {
                            break;
                        }
                        if pb_lock.status == PlaybackStatus::Stopped {
                            append_podcast_log("start_action.tts_loop_stopped");
                            break;
                        }
                        if sink_arc.empty() && pb_lock.download_finished {
                            pb_lock.status = PlaybackStatus::Stopped;
                            pb_lock.sink = None;
                            append_podcast_log("start_action.tts_completed");
                            break;
                        }
                    }
                });
                return;
            }

            let rt_thread = Arc::clone(&rt_p_start);

            std::thread::spawn(move || {
                append_podcast_log("start_action.tts_thread_started");
                let Ok((_stream, handle)) = OutputStream::try_default() else {
                    let mut pb_lock = pb_thread.lock().unwrap();
                    if pb_lock.generation == playback_generation {
                        append_podcast_log("start_action.audio_output_failed");
                        pb_lock.status = PlaybackStatus::Stopped;
                        pb_lock.sink = None;
                    }
                    return;
                };
                let Ok(sink) = Sink::try_new(&handle) else {
                    let mut pb_lock = pb_thread.lock().unwrap();
                    if pb_lock.generation == playback_generation {
                        append_podcast_log("start_action.audio_sink_failed");
                        pb_lock.status = PlaybackStatus::Stopped;
                        pb_lock.sink = None;
                    }
                    return;
                };

                let mut sink_arc = Arc::new(sink);
                {
                    let mut pb_lock = pb_thread.lock().unwrap();
                    pb_lock.sink = Some(Arc::clone(&sink_arc));
                }

                let chunks: Vec<String> = edge_tts::split_text_realtime_lazy(&text).collect();
                let mut cached_chunks = Vec::with_capacity(chunks.len());
                let (audio_tx, mut audio_rx) =
                    tokio::sync::mpsc::channel::<Result<(usize, Vec<u8>), String>>(10);
                append_podcast_log(&format!("start_action.tts_chunks total={}", chunks.len()));

                rt_thread.spawn({
                    let pb_download = Arc::clone(&pb_thread);
                    let voice_download = voice.clone();
                    async move {
                        let mut edge_session = None;
                        for (chunk_index, chunk) in chunks.into_iter().enumerate() {
                            {
                                let pb_lock = pb_download.lock().unwrap();
                                if pb_lock.generation != playback_generation
                                    || pb_lock.status == PlaybackStatus::Stopped
                                {
                                    break;
                                }
                            }

                            append_podcast_log(&format!(
                                "start_action.tts_chunk_request index={} voice={} rate={} pitch={} volume={}",
                                chunk_index, voice_download, rate, pitch, volume
                            ));
                            match edge_tts::synthesize_realtime_chunk_with_retry(
                                edge_session,
                                &chunk,
                                &voice_download,
                                rate,
                                pitch,
                                volume,
                                40,
                            )
                            .await
                            {
                                Ok((data, session)) => {
                                    edge_session = session;
                                    if data.is_empty() {
                                        append_podcast_log(&format!(
                                            "start_action.tts_chunk_empty index={chunk_index}"
                                        ));
                                        continue;
                                    }
                                    append_podcast_log(&format!(
                                        "start_action.tts_chunk_ok index={} bytes={}",
                                        chunk_index,
                                        data.len()
                                    ));
                                    if audio_tx.send(Ok((chunk_index, data))).await.is_err() {
                                        break;
                                    }
                                }
                                Err(err) => {
                                    append_podcast_log(&format!(
                                        "start_action.tts_chunk_error index={} error={}",
                                        chunk_index, err
                                    ));
                                    let _ = audio_tx.send(Err(err.to_string())).await;
                                    break;
                                }
                            }
                        }
                    }
                });

                while let Some(packet) = rt_thread.block_on(audio_rx.recv()) {
                    loop {
                        {
                            let mut pb_lock = pb_thread.lock().unwrap();
                            if pb_lock.generation != playback_generation {
                                break;
                            }
                            if pb_lock.status == PlaybackStatus::Stopped {
                                break;
                            }
                            if pb_lock.refresh_requested {
                                pb_lock.refresh_requested = false;
                                if let Ok(new_sink) = Sink::try_new(&handle) {
                                    sink_arc = Arc::new(new_sink);
                                    pb_lock.sink = Some(Arc::clone(&sink_arc));
                                }
                            }
                        }
                        if sink_arc.len() < 10 {
                            break;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(60));
                    }

                    {
                        let pb_lock = pb_thread.lock().unwrap();
                        if pb_lock.generation != playback_generation {
                            break;
                        }
                        if pb_lock.status == PlaybackStatus::Stopped {
                            break;
                        }
                    }

                    let (chunk_index, data) = match packet {
                        Ok(data) => data,
                        Err(err) => {
                            let mut pb_lock = pb_thread.lock().unwrap();
                            if pb_lock.generation == playback_generation {
                                println!("ERROR: Sintesi realtime fallita: {}", err);
                                pb_lock.status = PlaybackStatus::Stopped;
                                pb_lock.sink = None;
                            }
                            break;
                        }
                    };

                    cached_chunks.push(data.clone());
                    if let Ok(source) = Decoder::new(Cursor::new(data)) {
                        sink_arc.append(source);
                    } else {
                        append_podcast_log(&format!(
                            "start_action.decoder_failed index={}",
                            chunk_index
                        ));
                    }
                }

                {
                    let mut pb_lock = pb_thread.lock().unwrap();
                    if pb_lock.generation == playback_generation {
                        pb_lock.download_finished = true;
                        pb_lock.cached_tts = Some(TtsPlaybackCache {
                            text,
                            voice,
                            rate,
                            pitch,
                            volume,
                            chunks: cached_chunks,
                        });
                    } else {
                        return;
                    }
                }
                append_podcast_log("start_action.tts_download_finished");

                loop {
                    std::thread::sleep(std::time::Duration::from_millis(200));
                    let mut pb_lock = pb_thread.lock().unwrap();
                    if pb_lock.generation != playback_generation {
                        break;
                    }
                    if pb_lock.status == PlaybackStatus::Stopped {
                        append_podcast_log("start_action.tts_loop_stopped");
                        break;
                    }
                    if sink_arc.empty() && pb_lock.download_finished {
                        pb_lock.status = PlaybackStatus::Stopped;
                        pb_lock.sink = None;
                        append_podcast_log("start_action.tts_completed");
                        break;
                    }
                }
            });
        });

        let start_action_click = Rc::clone(&start_action);
        btn_start.on_click(move |_| {
            start_action_click();
        });
        #[cfg(target_os = "macos")]
        if let Some(item) = start_menu_item {
            let start_action_menu = Rc::clone(&start_action);
            item.on_click(move |_| {
                start_action_menu();
            });
        }

        let play_action_click = Rc::clone(&play_action);
        btn_play.on_click(move |_| {
            play_action_click();
        });
        #[cfg(target_os = "macos")]
        if let Some(item) = play_menu_item {
            let play_action_menu = Rc::clone(&play_action);
            item.on_click(move |_| {
                play_action_menu();
            });
        }

        let podcast_seek_back = Rc::clone(&podcast_playback);
        btn_podcast_back.on_click(move |_| {
            seek_podcast_playback(&podcast_seek_back, -PODCAST_SEEK_SECONDS);
        });

        let podcast_slider_seek = Rc::clone(&podcast_playback);
        podcast_position_slider.on_slider(move |_| {
            seek_podcast_playback_to_ratio(
                &podcast_slider_seek,
                podcast_position_slider.get_value(),
            );
        });

        let podcast_seek_forward = Rc::clone(&podcast_playback);
        btn_podcast_forward.on_click(move |_| {
            seek_podcast_playback(&podcast_seek_forward, PODCAST_SEEK_SECONDS);
        });

        let pb_stop = Arc::clone(&playback);
        let b_p_reset = btn_play;
        let podcast_playback_stop = Rc::clone(&podcast_playback);
        let stop_action: Rc<dyn Fn()> = Rc::new(move || {
            stop_podcast_playback(&podcast_playback_stop);
            let mut pb = pb_stop.lock().unwrap();
            if let Some(ref s) = pb.sink {
                s.stop();
            }
            pb.sink = None;
            pb.status = PlaybackStatus::Stopped;
            pb.refresh_requested = false;
            let podcast_mode = podcast_playback_stop.borrow().selected_episode.is_some();
            b_p_reset.set_label(&play_button_label(PlaybackStatus::Stopped, podcast_mode));
        });

        let stop_action_click = Rc::clone(&stop_action);
        btn_stop.on_click(move |_| {
            stop_action_click();
        });
        #[cfg(target_os = "macos")]
        if let Some(item) = stop_menu_item {
            let stop_action_menu = Rc::clone(&stop_action);
            item.on_click(move |_| {
                stop_action_menu();
            });
        }

        // --- Salva con Progress Bar (Non Bloccante) ---
        let rt_s = Arc::clone(&rt);
        let tc_s = text_ctrl;
        let f_save = frame;
        let s_save = Arc::clone(&settings);
        let podcast_playback_save = Rc::clone(&podcast_playback);
        #[cfg(target_os = "macos")]
        let tc_save_text = text_ctrl;
        #[cfg(target_os = "macos")]
        let f_save_text = frame;
        #[cfg(target_os = "macos")]
        let s_save_text = Arc::clone(&settings);
        #[cfg(target_os = "macos")]
        let current_document_save_text = Arc::clone(&current_document);
        #[cfg(target_os = "macos")]
        let save_text_action: Rc<dyn Fn()> = Rc::new(move || {
            let _ = save_current_document(
                &f_save_text,
                &s_save_text,
                &tc_save_text,
                &current_document_save_text,
            );
        });
        #[cfg(target_os = "macos")]
        let tc_save_text_as = text_ctrl;
        #[cfg(target_os = "macos")]
        let f_save_text_as = frame;
        #[cfg(target_os = "macos")]
        let s_save_text_as = Arc::clone(&settings);
        #[cfg(target_os = "macos")]
        let current_document_save_text_as = Arc::clone(&current_document);
        #[cfg(target_os = "macos")]
        let save_text_as_action: Rc<dyn Fn()> = Rc::new(move || {
            let _ = save_current_document_as(
                &f_save_text_as,
                &s_save_text_as,
                &tc_save_text_as,
                &current_document_save_text_as,
            );
        });
        let save_action: Rc<dyn Fn()> = Rc::new(move || {
            if let Some(episode) = podcast_playback_save.borrow().selected_episode.clone()
                && !episode.audio_url.trim().is_empty()
            {
                append_podcast_log(&format!(
                    "save_action.podcast_episode title={} audio_url={}",
                    episode.title, episode.audio_url
                ));
                if let Err(err) = save_podcast_episode(&f_save, &episode.audio_url, &episode.title)
                {
                    append_podcast_log(&format!(
                        "save_action.podcast_episode_error audio_url={} error={}",
                        episode.audio_url, err
                    ));
                    let ui = current_ui_strings();
                    let dialog = MessageDialog::builder(
                        &f_save,
                        &if Settings::load().ui_language == "it" {
                            format!("Impossibile salvare il podcast.\n\n{err}")
                        } else {
                            format!("Unable to save the podcast.\n\n{err}")
                        },
                        &ui.podcast_error_title,
                    )
                    .with_style(MessageDialogStyle::OK | MessageDialogStyle::IconError)
                    .build();
                    localize_standard_dialog_buttons(&dialog);
                    dialog.show_modal();
                }
                return;
            }

            let ui = current_ui_strings();
            let text = tc_s.get_value();
            if text.trim().is_empty() {
                return;
            }

            let (voice, rate, pitch, volume) = {
                let s = s_save.lock().unwrap();
                (s.voice.clone(), s.rate, s.pitch, s.volume)
            };
            let audiobook_file_not_saved = ui.audiobook_file_not_saved.clone();
            let audiobook_conversion_failed = ui.audiobook_conversion_failed.clone();
            let audiobook_ffmpeg_missing = ui.audiobook_ffmpeg_missing.clone();
            let audiobook_m4b_conversion_failed = ui.audiobook_m4b_conversion_failed.clone();
            let audiobook_m4a_conversion_failed = ui.audiobook_m4a_conversion_failed.clone();
            let audiobook_wav_conversion_failed = ui.audiobook_wav_conversion_failed.clone();

            if let Some(path_buf) = prompt_audiobook_save_path(&f_save, &s_save) {
                let path = path_buf.to_string_lossy().into_owned();
                append_podcast_log(&format!("audiobook_save.begin path={path}"));
                let chunks: Vec<String> = edge_tts::split_text_lazy(&text).collect();
                let total = chunks.len();
                append_podcast_log(&format!("audiobook_save.chunks total={total}"));

                let progress_dialog = Dialog::builder(&f_save, &ui.create_audiobook_title)
                    .with_style(
                        DialogStyle::Caption
                            | DialogStyle::SystemMenu
                            | DialogStyle::CloseBox
                            | DialogStyle::StayOnTop,
                    )
                    .with_size(420, 160)
                    .build();
                let progress_panel = Panel::builder(&progress_dialog).build();
                let progress_root = BoxSizer::builder(Orientation::Vertical).build();
                let progress_label = StaticText::builder(&progress_panel)
                    .with_label(&ui.initializing)
                    .build();
                progress_root.add(
                    &progress_label,
                    0,
                    SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Top,
                    12,
                );
                let progress_gauge = Gauge::builder(&progress_panel)
                    .with_range(total.max(1) as i32)
                    .build();
                progress_root.add(
                    &progress_gauge,
                    0,
                    SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Top,
                    12,
                );
                let progress_buttons = BoxSizer::builder(Orientation::Horizontal).build();
                let progress_cancel = Button::builder(&progress_panel)
                    .with_id(ID_AUDIOBOOK_DIALOG_CANCEL)
                    .with_label(&ui.cancel)
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
                progress_dialog.set_escape_id(ID_AUDIOBOOK_DIALOG_CANCEL);
                progress_dialog.show(true);

                let rt_save = Arc::clone(&rt_s);
                let abort_requested = Arc::new(AtomicBool::new(false));
                let abort_requested_thread = Arc::clone(&abort_requested);
                let save_state = Arc::new(Mutex::new(SaveAudiobookState {
                    completed_chunks: 0,
                    completed: false,
                    cancelled: false,
                    error_message: None,
                }));
                let save_state_thread = Arc::clone(&save_state);
                let chunks = Arc::new(chunks);
                std::thread::spawn(move || {
                    let next_index = Arc::new(Mutex::new(0usize));
                    let results = Arc::new(Mutex::new(vec![None; chunks.len()]));
                    let worker_count = chunks.len().clamp(1, AUDIOBOOK_SAVE_THREADS);
                    let mut workers = Vec::with_capacity(worker_count);

                    for _ in 0..worker_count {
                        let rt_worker = Arc::clone(&rt_save);
                        let chunks_worker = Arc::clone(&chunks);
                        let next_index_worker = Arc::clone(&next_index);
                        let results_worker = Arc::clone(&results);
                        let save_state_worker = Arc::clone(&save_state_thread);
                        let abort_worker = Arc::clone(&abort_requested_thread);
                        let voice_worker = voice.clone();
                        let audiobook_conversion_failed_worker =
                            audiobook_conversion_failed.clone();
                        workers.push(std::thread::spawn(move || {
                            loop {
                                if abort_worker.load(Ordering::Relaxed) {
                                    return;
                                }

                                let index = {
                                    let mut next = next_index_worker.lock().unwrap();
                                    if *next >= chunks_worker.len() {
                                        return;
                                    }
                                    let index = *next;
                                    *next += 1;
                                    index
                                };

                                let chunk = chunks_worker[index].clone();
                                match rt_worker.block_on(edge_tts::synthesize_text_with_retry(
                                    &chunk,
                                    &voice_worker,
                                    rate,
                                    pitch,
                                    volume,
                                    3,
                                )) {
                                    Ok(data) => {
                                        results_worker.lock().unwrap()[index] = Some(data);
                                        save_state_worker.lock().unwrap().completed_chunks += 1;
                                    }
                                    Err(err) => {
                                        append_podcast_log(&format!(
                                            "audiobook_save.chunk_error index={} chars={} error={}",
                                            index,
                                            chunk.chars().count(),
                                            err
                                        ));
                                        abort_worker.store(true, Ordering::Relaxed);
                                        save_state_worker.lock().unwrap().error_message =
                                            Some(audiobook_conversion_failed_worker.clone());
                                        return;
                                    }
                                }
                            }
                        }));
                    }

                    for worker in workers {
                        if worker.join().is_err() {
                            abort_requested_thread.store(true, Ordering::Relaxed);
                            save_state_thread.lock().unwrap().error_message =
                                Some(audiobook_conversion_failed.clone());
                            append_podcast_log("audiobook_save.worker_join_failed");
                            return;
                        }
                    }

                    if abort_requested_thread.load(Ordering::Relaxed) {
                        save_state_thread.lock().unwrap().cancelled = true;
                        append_podcast_log("audiobook_save.cancelled");
                        return;
                    }

                    let mut full_audio = Vec::new();
                    for maybe_data in results.lock().unwrap().iter_mut() {
                        let Some(data) = maybe_data.take() else {
                            append_podcast_log("audiobook_save.missing_chunk_data");
                            save_state_thread.lock().unwrap().error_message =
                                Some(audiobook_conversion_failed.clone());
                            return;
                        };
                        full_audio.extend(data);
                    }

                    let extension = path_buf
                        .extension()
                        .and_then(|ext| ext.to_str())
                        .map(|ext| ext.to_ascii_lowercase())
                        .unwrap_or_else(|| "mp3".to_string());

                    if extension == "m4b" || extension == "m4a" || extension == "wav" {
                        append_podcast_log(&format!(
                            "audiobook_save.transcode_start format={} path={}",
                            extension,
                            path_buf.display()
                        ));
                        let temp_mp3 = std::env::temp_dir()
                            .join(format!("sonarpad-minimal-audiobook-{}.mp3", Uuid::new_v4()));
                        if std::fs::write(&temp_mp3, &full_audio).is_err() {
                            save_state_thread.lock().unwrap().error_message =
                                Some(audiobook_file_not_saved.clone());
                            append_podcast_log("audiobook_save.temp_mp3_write_failed");
                            return;
                        }
                        let convert_result = match extension.as_str() {
                            "m4b" => convert_mp3_to_m4b(&temp_mp3, &path_buf, 128),
                            "m4a" => convert_mp3_to_m4a(&temp_mp3, &path_buf, 128),
                            "wav" => convert_mp3_to_wav(&temp_mp3, &path_buf),
                            _ => Ok(()),
                        };
                        if let Err(remove_err) = std::fs::remove_file(&temp_mp3) {
                            append_podcast_log(&format!(
                                "audiobook_save.temp_mp3_cleanup_failed error={remove_err}"
                            ));
                        }
                        if let Err(err) = convert_result {
                            let base_message = match extension.as_str() {
                                "m4b" => audiobook_m4b_conversion_failed.clone(),
                                "m4a" => audiobook_m4a_conversion_failed.clone(),
                                "wav" => audiobook_wav_conversion_failed.clone(),
                                _ => audiobook_conversion_failed.clone(),
                            };
                            let user_message = if err == "__FFMPEG_MISSING__" {
                                audiobook_ffmpeg_missing.clone()
                            } else {
                                format!("{base_message} ({err})")
                            };
                            save_state_thread.lock().unwrap().error_message = Some(user_message);
                            append_podcast_log(&format!(
                                "audiobook_save.transcode_failed format={} error={err}",
                                extension
                            ));
                            let _ = std::fs::remove_file(&path_buf);
                            return;
                        }
                        append_podcast_log(&format!(
                            "audiobook_save.transcode_completed format={} path={}",
                            extension,
                            path_buf.display()
                        ));
                    } else if std::fs::write(&path_buf, full_audio).is_err() {
                        save_state_thread.lock().unwrap().error_message =
                            Some(audiobook_file_not_saved.clone());
                        append_podcast_log("audiobook_save.write_failed");
                        return;
                    }

                    save_state_thread.lock().unwrap().completed = true;
                    append_podcast_log("audiobook_save.completed");
                });

                let progress_timer = Rc::new(Timer::new(&f_save));
                let progress_timer_tick = Rc::clone(&progress_timer);
                let progress_timer_handle = Rc::clone(&progress_timer);
                let pending_dialog = Rc::new(RefCell::new(None::<PendingSaveDialog>));
                let pending_dialog_tick = Rc::clone(&pending_dialog);
                let progress_dialog_handle = progress_dialog;
                let progress_dialog_close = progress_dialog;
                let progress_dialog_destroy = progress_dialog;
                let progress_label_tick = progress_label;
                let progress_label_cancel = progress_label;
                let progress_label_close = progress_label;
                let progress_gauge_tick = progress_gauge;
                let progress_cancel_close = progress_cancel;
                let abort_close = Arc::clone(&abort_requested);
                let save_state_tick = Arc::clone(&save_state);
                let save_state_close = Arc::clone(&save_state);
                let cancel_pending = Rc::new(RefCell::new(false));
                let cancel_pending_tick = Rc::clone(&cancel_pending);
                let cancel_pending_close = Rc::clone(&cancel_pending);
                let finalizing = Rc::new(RefCell::new(false));
                let finalizing_tick = Rc::clone(&finalizing);
                progress_cancel.on_click(move |_| {
                    if !*cancel_pending.borrow() {
                        append_podcast_log("audiobook_save.cancel_requested_button");
                        abort_requested.store(true, Ordering::Relaxed);
                        *cancel_pending.borrow_mut() = true;
                        progress_cancel.enable(false);
                        progress_label_cancel.set_label(&ui.cancelling_audiobook);
                    }
                });
                progress_dialog_close.on_close(move |event| {
                    append_podcast_log("audiobook_save.progress_dialog.on_close");
                    let state = save_state_close.lock().unwrap();
                    let finished =
                        state.completed || state.cancelled || state.error_message.is_some();
                    drop(state);

                    if finished {
                        append_podcast_log("audiobook_save.progress_dialog.on_close.finished");
                        event.skip(true);
                        return;
                    }

                    if !*cancel_pending_close.borrow() {
                        append_podcast_log("audiobook_save.cancel_requested_close");
                        abort_close.store(true, Ordering::Relaxed);
                        *cancel_pending_close.borrow_mut() = true;
                        progress_cancel_close.enable(false);
                        progress_label_close.set_label(&ui.cancelling_audiobook);
                    }

                    event.skip(false);
                });
                let timer_destroy = Rc::clone(&progress_timer);
                progress_dialog_destroy.on_destroy(move |event| {
                    append_podcast_log("audiobook_save.progress_dialog.on_destroy");
                    timer_destroy.stop();
                    event.skip(true);
                });
                progress_timer_tick.on_tick(move |_| {
                    if *finalizing_tick.borrow() {
                        return;
                    }

                    let state = save_state_tick.lock().unwrap();
                    if let Some(error_message) = state.error_message.as_ref() {
                        *finalizing_tick.borrow_mut() = true;
                        append_podcast_log(&format!(
                            "audiobook_save.tick.error completed_chunks={} message={error_message}",
                            state.completed_chunks
                        ));
                        progress_timer_handle.stop();
                        progress_label_tick.set_label(&ui.audiobook_conversion_error);
                        progress_gauge_tick.set_value(state.completed_chunks as i32);
                        *pending_dialog_tick.borrow_mut() =
                            Some(PendingSaveDialog::Error(error_message.clone()));
                        append_podcast_log("audiobook_save.tick.error.destroy_progress");
                        progress_dialog_handle.destroy();
                        let Some(dialog) = pending_dialog_tick.borrow_mut().take() else {
                            return;
                        };
                        match dialog {
                            PendingSaveDialog::Success => {}
                            PendingSaveDialog::Error(error_message) => {
                                append_podcast_log(&format!(
                                    "audiobook_save.show_error message={error_message}"
                                ));
                                show_modeless_message_dialog(
                                    &f_save,
                                    &ui.conversion_error_title,
                                    &error_message,
                                );
                                append_podcast_log("audiobook_save.error_closed");
                            }
                        }
                        return;
                    }

                    if state.cancelled {
                        *finalizing_tick.borrow_mut() = true;
                        append_podcast_log(&format!(
                            "audiobook_save.tick.cancelled completed_chunks={}",
                            state.completed_chunks
                        ));
                        progress_timer_handle.stop();
                        progress_dialog_handle.destroy();
                        return;
                    }

                    if state.completed {
                        *finalizing_tick.borrow_mut() = true;
                        append_podcast_log(&format!(
                            "audiobook_save.tick.completed completed_chunks={}",
                            state.completed_chunks
                        ));
                        progress_label_tick.set_label(&ui.audiobook_saved_ok);
                        progress_gauge_tick.set_value(total.max(1) as i32);
                        progress_timer_handle.stop();
                        *pending_dialog_tick.borrow_mut() = Some(PendingSaveDialog::Success);
                        append_podcast_log("audiobook_save.tick.completed.destroy_progress");
                        progress_dialog_handle.destroy();
                        let Some(dialog) = pending_dialog_tick.borrow_mut().take() else {
                            return;
                        };
                        match dialog {
                            PendingSaveDialog::Success => {
                                append_podcast_log("audiobook_save.show_success");
                                show_modeless_message_dialog(
                                    &f_save,
                                    &ui.save_completed_title,
                                    &ui.audiobook_saved_ok,
                                );
                                append_podcast_log("audiobook_save.success_closed");
                            }
                            PendingSaveDialog::Error(_) => {}
                        }
                        return;
                    }

                    let current = state.completed_chunks as i32;
                    drop(state);

                    if *cancel_pending_tick.borrow() {
                        append_podcast_log(&format!(
                            "audiobook_save.tick.cancelling completed_chunks={current}"
                        ));
                        progress_label_tick.set_label(&ui.cancelling_audiobook);
                        progress_gauge_tick.set_value(current);
                        return;
                    }

                    let current_display = current.min(total.max(1) as i32);
                    let msg = format!("Sintesi blocco {} di {}...", current, total);
                    progress_label_tick.set_label(&msg);
                    progress_gauge_tick.set_value(current_display);
                });
                progress_timer.start(100, false);
            }
        });

        let save_action_click = Rc::clone(&save_action);
        btn_save.on_click(move |_| {
            save_action_click();
        });
        #[cfg(target_os = "macos")]
        if let Some(item) = save_text_menu_item {
            let save_text_action_menu = Rc::clone(&save_text_action);
            item.on_click(move |_| {
                save_text_action_menu();
            });
        }
        #[cfg(target_os = "macos")]
        if let Some(item) = save_text_as_menu_item {
            let save_text_as_action_menu = Rc::clone(&save_text_as_action);
            item.on_click(move |_| {
                save_text_as_action_menu();
            });
        }
        #[cfg(target_os = "macos")]
        if let Some(item) = save_menu_item {
            let save_action_menu = Rc::clone(&save_action);
            item.on_click(move |_| {
                save_action_menu();
            });
        }

        let frame_settings = frame;
        let settings_state = Arc::clone(&settings);
        let voices_state = Arc::clone(&voices_data);
        let languages_state = Arc::clone(&languages);
        let playback_state = Arc::clone(&playback);
        let article_menu_state_settings = Arc::clone(&article_menu_state);
        let podcast_menu_state_settings = Arc::clone(&podcast_menu_state);
        let radio_menu_state_settings = Arc::clone(&radio_menu_state);
        let btn_save_settings = btn_save;
        let btn_settings_settings = btn_settings;
        let btn_podcast_back_settings = btn_podcast_back;
        let btn_podcast_forward_settings = btn_podcast_forward;
        let settings_action: Rc<dyn Fn()> = Rc::new(move || {
            append_podcast_log("settings_dialog.open");
            let previous_ui_language = settings_state.lock().unwrap().ui_language.clone();
            open_settings_dialog(
                &frame_settings,
                &settings_state,
                &voices_state,
                &languages_state,
                &playback_state,
            );
            let updated_ui_language = settings_state.lock().unwrap().ui_language.clone();
            if previous_ui_language != updated_ui_language {
                refresh_localized_main_ui(
                    &frame_settings,
                    &settings_state,
                    (
                        &articles_menu_settings,
                        &podcasts_menu_settings,
                        &radio_menu_settings,
                    ),
                    (
                        &article_menu_state_settings,
                        &podcast_menu_state_settings,
                        &radio_menu_state_settings,
                    ),
                    (
                        &btn_save_settings,
                        &btn_settings_settings,
                        &btn_podcast_back_settings,
                        &btn_podcast_forward_settings,
                    ),
                );
            }
        });

        let settings_action_click = Rc::clone(&settings_action);
        btn_settings.on_click(move |_| {
            settings_action_click();
        });
        #[cfg(target_os = "macos")]
        if let Some(item) = settings_menu_item {
            let settings_action_menu = Rc::clone(&settings_action);
            item.on_click(move |_| {
                settings_action_menu();
            });
        }

        #[cfg(target_os = "macos")]
        {
            let start_action_menu = Rc::clone(&start_action);
            let play_action_menu = Rc::clone(&play_action);
            let stop_action_menu = Rc::clone(&stop_action);
            let save_action_menu = Rc::clone(&save_action);
            let save_text_action_menu = Rc::clone(&save_text_action);
            let save_text_as_action_menu = Rc::clone(&save_text_as_action);
            let settings_action_menu = Rc::clone(&settings_action);
            frame.on_menu(move |event| match event.get_id() {
                ID_START_PLAYBACK => start_action_menu(),
                ID_PLAY_PAUSE => play_action_menu(),
                ID_STOP => stop_action_menu(),
                ID_SAVE => save_action_menu(),
                ID_SAVE_TEXT => save_text_action_menu(),
                ID_SAVE_TEXT_AS => save_text_as_action_menu(),
                ID_SETTINGS => settings_action_menu(),
                _ => {}
            });
        }

        #[cfg(target_os = "macos")]
        {
            let shortcut_actions = ShortcutActions {
                start: Rc::clone(&start_action),
                play_pause: Rc::clone(&play_action),
                stop: Rc::clone(&stop_action),
                save: Rc::clone(&save_action),
                settings: Rc::clone(&settings_action),
            };
            let podcast_seek_back_shortcut = Rc::clone(&podcast_playback);
            let podcast_seek_forward_shortcut = Rc::clone(&podcast_playback);
            frame.on_key_down(move |event| {
                handle_shortcut_event(
                    event,
                    &shortcut_actions,
                    &podcast_seek_back_shortcut,
                    &podcast_seek_forward_shortcut,
                );
            });
        }

        #[cfg(target_os = "macos")]
        {
            let shortcut_actions = ShortcutActions {
                start: Rc::clone(&start_action),
                play_pause: Rc::clone(&play_action),
                stop: Rc::clone(&stop_action),
                save: Rc::clone(&save_action),
                settings: Rc::clone(&settings_action),
            };
            let podcast_seek_back_shortcut = Rc::clone(&podcast_playback);
            let podcast_seek_forward_shortcut = Rc::clone(&podcast_playback);
            panel.on_key_down(move |event| {
                handle_shortcut_event(
                    event,
                    &shortcut_actions,
                    &podcast_seek_back_shortcut,
                    &podcast_seek_forward_shortcut,
                );
            });
        }

        #[cfg(target_os = "macos")]
        {
            let shortcut_actions = ShortcutActions {
                start: Rc::clone(&start_action),
                play_pause: Rc::clone(&play_action),
                stop: Rc::clone(&stop_action),
                save: Rc::clone(&save_action),
                settings: Rc::clone(&settings_action),
            };
            let podcast_seek_back_shortcut = Rc::clone(&podcast_playback);
            let podcast_seek_forward_shortcut = Rc::clone(&podcast_playback);
            btn_start.on_key_down(move |event| {
                handle_shortcut_event(
                    event,
                    &shortcut_actions,
                    &podcast_seek_back_shortcut,
                    &podcast_seek_forward_shortcut,
                );
            });
        }

        #[cfg(target_os = "macos")]
        {
            let shortcut_actions = ShortcutActions {
                start: Rc::clone(&start_action),
                play_pause: Rc::clone(&play_action),
                stop: Rc::clone(&stop_action),
                save: Rc::clone(&save_action),
                settings: Rc::clone(&settings_action),
            };
            let podcast_seek_back_shortcut = Rc::clone(&podcast_playback);
            let podcast_seek_forward_shortcut = Rc::clone(&podcast_playback);
            btn_play.on_key_down(move |event| {
                handle_shortcut_event(
                    event,
                    &shortcut_actions,
                    &podcast_seek_back_shortcut,
                    &podcast_seek_forward_shortcut,
                );
            });
        }

        #[cfg(target_os = "macos")]
        {
            let shortcut_actions = ShortcutActions {
                start: Rc::clone(&start_action),
                play_pause: Rc::clone(&play_action),
                stop: Rc::clone(&stop_action),
                save: Rc::clone(&save_action),
                settings: Rc::clone(&settings_action),
            };
            let podcast_seek_back_shortcut = Rc::clone(&podcast_playback);
            let podcast_seek_forward_shortcut = Rc::clone(&podcast_playback);
            btn_stop.on_key_down(move |event| {
                handle_shortcut_event(
                    event,
                    &shortcut_actions,
                    &podcast_seek_back_shortcut,
                    &podcast_seek_forward_shortcut,
                );
            });
        }

        #[cfg(target_os = "macos")]
        {
            let shortcut_actions = ShortcutActions {
                start: Rc::clone(&start_action),
                play_pause: Rc::clone(&play_action),
                stop: Rc::clone(&stop_action),
                save: Rc::clone(&save_action),
                settings: Rc::clone(&settings_action),
            };
            let podcast_seek_back_shortcut = Rc::clone(&podcast_playback);
            let podcast_seek_forward_shortcut = Rc::clone(&podcast_playback);
            btn_save.on_key_down(move |event| {
                handle_shortcut_event(
                    event,
                    &shortcut_actions,
                    &podcast_seek_back_shortcut,
                    &podcast_seek_forward_shortcut,
                );
            });
        }

        #[cfg(target_os = "macos")]
        {
            let shortcut_actions = ShortcutActions {
                start: Rc::clone(&start_action),
                play_pause: Rc::clone(&play_action),
                stop: Rc::clone(&stop_action),
                save: Rc::clone(&save_action),
                settings: Rc::clone(&settings_action),
            };
            let podcast_seek_back_shortcut = Rc::clone(&podcast_playback);
            let podcast_seek_forward_shortcut = Rc::clone(&podcast_playback);
            btn_settings.on_key_down(move |event| {
                handle_shortcut_event(
                    event,
                    &shortcut_actions,
                    &podcast_seek_back_shortcut,
                    &podcast_seek_forward_shortcut,
                );
            });
        }

        #[cfg(target_os = "macos")]
        {
            let shortcut_actions = ShortcutActions {
                start: Rc::clone(&start_action),
                play_pause: Rc::clone(&play_action),
                stop: Rc::clone(&stop_action),
                save: Rc::clone(&save_action),
                settings: Rc::clone(&settings_action),
            };
            let podcast_seek_back_shortcut = Rc::clone(&podcast_playback);
            let podcast_seek_forward_shortcut = Rc::clone(&podcast_playback);
            btn_podcast_back.on_key_down(move |event| {
                handle_shortcut_event(
                    event,
                    &shortcut_actions,
                    &podcast_seek_back_shortcut,
                    &podcast_seek_forward_shortcut,
                );
            });
        }

        #[cfg(target_os = "macos")]
        {
            let shortcut_actions = ShortcutActions {
                start: Rc::clone(&start_action),
                play_pause: Rc::clone(&play_action),
                stop: Rc::clone(&stop_action),
                save: Rc::clone(&save_action),
                settings: Rc::clone(&settings_action),
            };
            let podcast_seek_back_shortcut = Rc::clone(&podcast_playback);
            let podcast_seek_forward_shortcut = Rc::clone(&podcast_playback);
            btn_podcast_forward.on_key_down(move |event| {
                handle_shortcut_event(
                    event,
                    &shortcut_actions,
                    &podcast_seek_back_shortcut,
                    &podcast_seek_forward_shortcut,
                );
            });
        }

        #[cfg(not(target_os = "macos"))]
        {
            let shortcut_actions = ShortcutActions {
                start: Rc::clone(&start_action),
                play_pause: Rc::clone(&play_action),
                stop: Rc::clone(&stop_action),
                save: Rc::clone(&save_action),
                settings: Rc::clone(&settings_action),
            };
            let podcast_seek_back_shortcut = Rc::clone(&podcast_playback);
            let podcast_seek_forward_shortcut = Rc::clone(&podcast_playback);
            text_ctrl.on_key_down(move |event| {
                handle_shortcut_event(
                    event,
                    &shortcut_actions,
                    &podcast_seek_back_shortcut,
                    &podcast_seek_forward_shortcut,
                );
            });
        }

        #[cfg(target_os = "macos")]
        {
            let shortcut_actions = ShortcutActions {
                start: Rc::clone(&start_action),
                play_pause: Rc::clone(&play_action),
                stop: Rc::clone(&stop_action),
                save: Rc::clone(&save_action),
                settings: Rc::clone(&settings_action),
            };
            let podcast_seek_back_shortcut = Rc::clone(&podcast_playback);
            let podcast_seek_forward_shortcut = Rc::clone(&podcast_playback);
            text_ctrl.on_key_down(move |event| {
                handle_shortcut_event(
                    event,
                    &shortcut_actions,
                    &podcast_seek_back_shortcut,
                    &podcast_seek_forward_shortcut,
                );
            });
        }

        if let Some(path) = initial_open_path.as_ref() {
            append_podcast_log(&format!("app.initial_open.begin path={}", path.display()));
            match load_file_for_display(&frame, path) {
                Ok(content) => {
                    podcast_playback.borrow_mut().selected_episode = None;
                    text_ctrl.set_value(&content);
                    text_ctrl.set_modified(false);
                    set_current_document_state(&current_document, Some(path.clone()));
                    append_podcast_log(&format!(
                        "app.initial_open.loaded path={} length={}",
                        path.display(),
                        content.len()
                    ));
                }
                Err(err) => {
                    append_podcast_log(&format!(
                        "app.initial_open.failed path={} err={}",
                        path.display(),
                        err
                    ));
                    let ui = current_ui_strings();
                    show_message_dialog(&frame, &ui.open_document_title, &err);
                }
            }
        }

        frame.show(true);
        frame.centre();
    });
}

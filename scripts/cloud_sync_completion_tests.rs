#![allow(dead_code)]
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

// The production completion and snapshot functions are appended by the harness.
// Doubles isolate UI, disk failures and normalization from macOS dependencies.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Settings {
    ui_language: String,
    news_language: String,
    article_sources: Vec<Value>,
    media_seek_seconds: u64,
}
impl Settings { fn remember_storage_snapshot(&self) {} }
#[derive(Default)]
struct Storage { text: Option<String>, fail_write: bool, corrupt_readback: bool,
    finished: bool, logs: Vec<String> }
static STORAGE: Mutex<Storage> = Mutex::new(Storage {
    text: None, fail_write: false, corrupt_readback: false, finished: false, logs: Vec::new(),
});
fn append_podcast_log(message: &str) { STORAGE.lock().unwrap().logs.push(message.into()); }
fn read_app_storage_text(_: &str) -> Option<String> { STORAGE.lock().unwrap().text.clone() }
fn write_app_storage_text(_: &str, data: &str) -> Result<(), String> {
    let mut storage = STORAGE.lock().unwrap();
    if storage.fail_write { return Err("settings_permission_denied".into()); }
    let mut value: Value = serde_json::from_str(data).unwrap();
    if storage.corrupt_readback { value["media_seek_seconds"] = json!(20); }
    storage.text = Some(value.to_string());
    Ok(())
}
fn normalize_ui_language(value: &str) -> String { value.trim().to_lowercase() }
fn normalize_news_language(value: &str) -> String { value.trim().to_lowercase() }
fn normalize_settings_data(settings: &mut Settings) {
    settings.media_seek_seconds = settings.media_seek_seconds.clamp(1, 60);
}
fn replace_default_article_sources_for_news_language(s: &[Value], _: &str) -> Vec<Value> { s.to_vec() }
mod articles {
    pub struct Source { pub url: String }
    pub fn default_sources_for_news_language(_: &str) -> Vec<Source> { vec![] }
}
struct Session { meta: Value }
impl Session {
    fn select(&self, value: &Value) -> Value { value.clone() }
    fn merge_response(&mut self, remote: &Value, _: &Value) -> Result<Value, String> { Ok(remote.clone()) }
    fn finish(&mut self, _: Value) -> Result<(), String> { STORAGE.lock().unwrap().finished = true; Ok(()) }
}
type Outcome = (Session, Option<Value>, Option<String>);
mod cloud_sync {
    use super::*;
    pub fn export(value: &Value, _: bool) -> Value {
        let mut value = value.clone(); value.as_object_mut().unwrap().remove("_cloud_default_urls"); value
    }
    pub fn language(value: &Value, _: bool) -> String { value["news_language"].as_str().unwrap().into() }
    pub fn apply(current: &mut Value, incoming: &Value, _: bool) -> Result<(), String> { *current = incoming.clone(); Ok(()) }
    pub fn reconcile_news_language(_: &Session, _: &mut Value, _: &mut Value, _: bool) -> Result<(), String> { Ok(()) }
    pub fn log_collection_reductions(_: &Value, _: &Value, _: &str, _: fn(&str)) {}
}
#[test]
fn normalization_and_failure_paths_use_production_completion() {
    let original = Settings { ui_language: "it".into(), news_language: "it".into(),
        article_sources: vec![], media_seek_seconds: 10 };
    let settings = Arc::new(Mutex::new(original.clone()));
    let remote = json!({"ui_language":" IT ","news_language":" IT ",
        "article_sources":[],"media_seek_seconds":999});
    let outcome = || (Session { meta: json!({}) }, Some(remote.clone()), None);
    *STORAGE.lock().unwrap() = Storage { text: Some(serde_json::to_string(&original).unwrap()), ..Default::default() };
    assert!(finish(&settings, outcome()).is_ok());
    assert!(STORAGE.lock().unwrap().finished);
    assert_eq!(settings.lock().unwrap().media_seek_seconds, 60);
    assert_eq!(settings.lock().unwrap().ui_language, "it");
    assert_eq!(snapshot_for_sync().unwrap(), snapshot(&settings.lock().unwrap()).unwrap());

    for (write_failure, expected) in [(true, "settings_permission_denied"), (false, "settings_verification_mismatch")] {
        *STORAGE.lock().unwrap() = Storage { text: Some(serde_json::to_string(&original).unwrap()),
            fail_write: write_failure, corrupt_readback: !write_failure, ..Default::default() };
        *settings.lock().unwrap() = original.clone();
        assert_eq!(finish(&settings, outcome()).err().unwrap(), expected);
        let storage = STORAGE.lock().unwrap();
        assert!(!storage.finished, "Never acknowledge a failed write/verification");
        assert!(storage.logs.iter().any(|l| l.contains(expected)));
        assert_eq!(settings.lock().unwrap().media_seek_seconds, 10);
    }
    *STORAGE.lock().unwrap() = Storage::default();
    assert_eq!(snapshot_for_sync().unwrap_err(), "settings_read_error");
    STORAGE.lock().unwrap().text = Some("invalid JSON".into());
    assert_eq!(snapshot_for_sync().unwrap_err(), "settings_parse_error");
}

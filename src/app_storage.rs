use serde_json::{Value, json};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

/// Each Settings clone keeps its own last saved values. Never serialize these
/// bookkeeping values into settings.json or send them to the cloud.
#[derive(Default)]
pub struct SettingsSnapshot(std::sync::Mutex<Value>);

impl Clone for SettingsSnapshot {
    fn clone(&self) -> Self {
        Self(std::sync::Mutex::new(
            self.0
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .clone(),
        ))
    }
}

impl SettingsSnapshot {
    pub fn read(&self) -> Result<Value, String> {
        self.0
            .lock()
            .map(|value| value.clone())
            .map_err(|_| "settings_lock_error".into())
    }

    pub fn remember(&self, value: Value) -> Result<(), String> {
        *self.0.lock().map_err(|_| "settings_lock_error")? = value;
        Ok(())
    }
}

/// Apply only this window's edits to the latest on-disk settings. Reuse the
/// cloud's identity-based merge for collections, including explicit removals.
pub fn merge_settings(current: &Value, before: &Value, edited: &Value) -> Value {
    let mut merged = current.clone();
    let Some(fields) = edited.as_object() else {
        return merged;
    };
    for (key, value) in fields {
        if current.get(key).is_none() {
            merged[key] = value.clone();
            continue;
        }
        if before.get(key) == Some(value) {
            continue;
        }
        let identity = match key.as_str() {
            "youtube_favorites" | "podcast_sources" | "article_sources" => "url",
            "tv_favorites" => "name",
            "radio_favorites" => "stream_url",
            "sonarmusic_favorites" => "id",
            "article_folders" => "",
            _ => {
                merged[key] = value.clone();
                continue;
            }
        };
        let rows = |values: &Value| -> Value {
            Value::Array(values.as_array().into_iter().flatten().map(|item| {
                let id = if identity.is_empty() {
                    item.clone()
                } else if key == "tv_favorites" {
                    json!(item["name"].as_str().unwrap_or("").trim().to_lowercase())
                } else if key == "sonarmusic_favorites" {
                    json!([item["kind"], item[identity]])
                } else {
                    item[identity].clone()
                };
                json!({"id":id,"data":if item.is_object() {item.clone()} else {json!({"value":item})}})
            }).collect())
        };
        let collections = crate::cloud_sync::merge(
            &json!({key:rows(&current[key])}),
            &json!({key:rows(&before[key])}),
            &json!({key:rows(value)}),
        );
        merged[key] = Value::Array(
            collections[key]
                .as_array()
                .into_iter()
                .flatten()
                .map(|row| {
                    if identity.is_empty() {
                        row["data"]["value"].clone()
                    } else {
                        row["data"].clone()
                    }
                })
                .collect(),
        );
    }
    merged
}

pub fn test_directory() -> PathBuf {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        let path = std::env::temp_dir().join(format!(
            "sonarpad-mac-tests-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&path).expect("isolated test directory");
        path
    })
    .clone()
}

pub fn write_atomically(path: &Path, data: &[u8]) -> std::io::Result<()> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let temporary = path.with_extension(format!(
        "{}.{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)?;
            file.write_all(data)?;
            file.sync_all()?;
        }
        // Same-volume rename replaces the complete file on macOS without truncating it.
        std::fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _cleanup = std::fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stale_preference_save_keeps_all_collections_and_newer_preferences() {
        let before = json!({"youtube_favorites":[],"radio_favorites":[],"tv_favorites":[],
            "podcast_sources":[],"article_sources":[],"article_folders":[],"sonarmusic_favorites":[],
            "volume":100,"sonartube_search_type":0});
        let mut current = before.clone();
        for key in [
            "youtube_favorites",
            "radio_favorites",
            "tv_favorites",
            "podcast_sources",
            "article_sources",
            "article_folders",
            "sonarmusic_favorites",
        ] {
            current[key] = json!([{"id":"added-elsewhere"}]);
        }
        current["sonartube_search_type"] = json!(3);
        current["future_setting"] = json!(true);
        let mut edited = before.clone();
        edited["volume"] = json!(80);
        let merged = merge_settings(&current, &before, &edited);
        current["volume"] = json!(80);
        assert_eq!(merged, current);
        assert_eq!(merge_settings(&merged, &edited, &edited), current);
    }

    #[test]
    fn stale_collection_edits_keep_new_items_and_honor_specific_removals() {
        for (key, identity) in [
            ("youtube_favorites", "url"),
            ("tv_favorites", "name"),
            ("radio_favorites", "stream_url"),
            ("podcast_sources", "url"),
            ("article_sources", "url"),
            ("sonarmusic_favorites", "id"),
        ] {
            let item = |id: &str| json!({identity:id,"kind":"channel","title":id});
            let before = json!({key:[item("a"),item("b")]});
            let current = json!({key:[item("a"),item("b"),item("remote")]});
            let edited = json!({key:[item("b"),item("local")]});
            assert_eq!(
                merge_settings(&current, &before, &edited),
                json!({key:[item("b"),item("local"),item("remote")]})
            );
            // An unchanged old row cannot reappear after removal on disk.
            assert_eq!(
                merge_settings(
                    &json!({key:[item("b")]}),
                    &before,
                    &json!({key:[item("a"),item("b"),item("local")]})
                ),
                json!({key:[item("b"),item("local")]})
            );
        }
    }

    #[test]
    fn tv_name_only_favorites_survive_save_reload_and_individual_removal() {
        let before = json!({"tv_favorites":[{"name":"Rai 1","url":""},{"name":"Rai 2","url":""}]});
        let current = json!({"tv_favorites":[{"name":"Rai 1","url":""},{"name":"Rai 2","url":""},{"name":"Rai 3","url":""}]});
        let edited = json!({"tv_favorites":[{"name":"Rai 2","url":""},{"name":"Rai 4","url":""}]});
        let merged = merge_settings(&current, &before, &edited);
        assert_eq!(merged, json!({"tv_favorites":[{"name":"Rai 2","url":""},{"name":"Rai 4","url":""},{"name":"Rai 3","url":""}]}));
        let path = test_directory().join("tv-roundtrip.json");
        write_atomically(&path, &serde_json::to_vec(&merged).unwrap()).unwrap();
        let reloaded: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(merge_settings(&reloaded, &edited, &edited), merged);
        let resolved = json!({"tv_favorites":[{"name":" RAI 2 ","url":"https://example.org/live"},{"name":"Rai 4","url":""}]});
        let updated = merge_settings(&reloaded, &edited, &resolved);
        assert_eq!(updated["tv_favorites"].as_array().unwrap().len(), 3);
        assert_eq!(updated["tv_favorites"][0]["url"], "https://example.org/live");
    }

    #[test]
    fn folder_reorder_and_metadata_edits_keep_concurrent_changes() {
        let before = json!({"article_folders":["a","b"],"youtube_favorites":[{"url":"channel","title":"Old"}]});
        let current = json!({"article_folders":["a","b","new"],"youtube_favorites":[{"url":"channel","title":"Renamed"}]});
        let edited = json!({"article_folders":["b","a"],"youtube_favorites":[{"url":"channel","title":"Old"},{"url":"other","title":"Other"}]});
        let merged = merge_settings(&current, &before, &edited);
        assert_eq!(merged["article_folders"], json!(["b", "a", "new"]));
        assert_eq!(merged["youtube_favorites"][0]["title"], "Renamed");
    }

    #[test]
    fn first_save_writes_defaults_and_snapshot_clones_remain_independent() {
        let defaults = json!({"volume":100,"youtube_favorites":[]});
        assert_eq!(merge_settings(&json!({}), &defaults, &defaults), defaults);
        let snapshot = SettingsSnapshot::default();
        snapshot.remember(defaults.clone()).unwrap();
        let older_window = snapshot.clone();
        snapshot.remember(json!({"volume":80})).unwrap();
        assert_eq!(older_window.read().unwrap(), defaults);
        assert_ne!(snapshot.read().unwrap(), defaults);
    }
    #[test]
    fn concurrent_readers_never_see_truncated_settings() {
        let path = test_directory().join("concurrent.json");
        let old = vec![b'a'; 16384];
        let new = vec![b'b'; 32768];
        write_atomically(&path, &old).unwrap();
        std::thread::scope(|scope| {
            scope.spawn(|| {
                for _ in 0..200 {
                    let _result = write_atomically(&path, &new);
                    let _result = write_atomically(&path, &old);
                }
            });
            for _ in 0..1000 {
                let bytes = std::fs::read(&path).unwrap();
                assert!(bytes == old || bytes == new);
            }
        });
    }
    #[test]
    fn failed_replacement_preserves_destination() {
        let path = test_directory().join("protected-directory");
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(path.join("existing"), b"preserved").unwrap();
        assert!(write_atomically(&path, b"replacement").is_err());
        assert_eq!(std::fs::read(path.join("existing")).unwrap(), b"preserved");
    }
    #[test]
    fn test_directory_is_stable_and_temporary() {
        assert_eq!(test_directory(), test_directory());
        assert!(test_directory().starts_with(std::env::temp_dir()));
    }
}

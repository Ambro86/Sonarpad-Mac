use serde::Deserialize;
use std::path::Path;

const SONARPAD_AUDIO_DESCRIPTIONS_API_URL: &str =
    "https://www.nicofranca.it/index.php?api=sonarpad";
const API_RESULT_LIMIT: usize = 1000;

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct CatalogItem {
    #[serde(rename = "type", default)]
    pub(crate) item_type: String,
    #[serde(default)]
    pub(crate) title: String,
    #[serde(default)]
    pub(crate) path: String,
    #[serde(default)]
    pub(crate) modified_at: Option<String>,
    #[serde(default)]
    pub(crate) modified_timestamp: i64,
    #[serde(default)]
    pub(crate) filename: String,
    #[serde(default)]
    pub(crate) download_filename: String,
    #[serde(default)]
    pub(crate) plot: String,
    #[serde(default)]
    pub(crate) download_url: String,
    #[serde(default)]
    pub(crate) stream_url: Option<String>,
}

impl CatalogItem {
    pub(crate) fn is_folder(&self) -> bool {
        self.item_type.eq_ignore_ascii_case("folder")
    }

    pub(crate) fn is_playable_file(&self) -> bool {
        self.item_type.eq_ignore_ascii_case("file")
            && self
                .stream_url
                .as_deref()
                .is_some_and(|url| !url.trim().is_empty())
    }

    pub(crate) fn is_catalog_entry(&self) -> bool {
        self.is_folder() || self.is_playable_file()
    }

    pub(crate) fn date_label(&self) -> String {
        if let Some(value) = self
            .modified_at
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            && let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(value)
        {
            return parsed.format("%d/%m/%Y").to_string();
        }

        if self.modified_timestamp > 0
            && let Some(parsed) = chrono::DateTime::<chrono::Utc>::from_timestamp(self.modified_timestamp, 0)
        {
            return parsed.format("%d/%m/%Y").to_string();
        }

        String::new()
    }

    pub(crate) fn suggested_download_name(&self) -> String {
        for candidate in [&self.download_filename, &self.filename] {
            let trimmed = candidate.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }

        let title = self.title.trim();
        if title.is_empty() {
            return "audiodescrizione_sonarpad.mp3".to_string();
        }

        let mut safe = title
            .chars()
            .map(|ch| match ch {
                '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
                _ => ch,
            })
            .collect::<String>();
        safe = safe.trim().trim_matches('.').to_string();
        if safe.is_empty() {
            "audiodescrizione_sonarpad.mp3".to_string()
        } else if Path::new(&safe).extension().is_some() {
            safe
        } else {
            format!("{safe}.mp3")
        }
    }
}

#[derive(Debug, Deserialize)]
struct ApiResponse {
    #[serde(default)]
    ok: bool,
    #[serde(default)]
    message: String,
    #[serde(default)]
    items: Vec<CatalogItem>,
}

pub(crate) fn load_recent_catalog() -> Result<Vec<CatalogItem>, String> {
    request_catalog("recent", None, None, Some("recent"), true).map(|items| {
        items
            .into_iter()
            .filter(CatalogItem::is_catalog_entry)
            .collect()
    })
}

pub(crate) fn search_catalog(query: &str) -> Result<Vec<CatalogItem>, String> {
    let query = query.trim();
    if query.is_empty() {
        return Err("Inserisci un testo da cercare nelle audiodescrizioni Sonarpad.".to_string());
    }
    request_catalog("search", Some(query), None, Some("alpha"), false).map(|items| {
        items
            .into_iter()
            .filter(CatalogItem::is_playable_file)
            .collect()
    })
}

pub(crate) fn load_folder_catalog(folder: &str) -> Result<Vec<CatalogItem>, String> {
    let folder = folder.trim_matches('/').trim();
    let action = if folder.is_empty() { "catalog" } else { "folder" };
    request_catalog(
        action,
        None,
        (!folder.is_empty()).then_some(folder),
        Some("alpha"),
        false,
    )
    .map(|items| {
        items
            .into_iter()
            .filter(CatalogItem::is_catalog_entry)
            .collect()
    })
}

fn request_catalog(
    action: &str,
    query: Option<&str>,
    folder: Option<&str>,
    sort: Option<&str>,
    group_recent_folders: bool,
) -> Result<Vec<CatalogItem>, String> {
    let code = crate::load_saved_rai_luce_code()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            "Codice Sonarpad mancante: inseriscilo nelle impostazioni prima di aprire le audiodescrizioni."
                .to_string()
        })?;

    let mut body = serde_json::json!({
        "action": action,
        "show_branding": false,
        "limit": API_RESULT_LIMIT,
        "offset": 0
    });
    if let Some(query) = query.map(str::trim).filter(|value| !value.is_empty()) {
        body["q"] = serde_json::Value::String(query.to_string());
    }
    if let Some(folder) = folder.map(str::trim).filter(|value| !value.is_empty()) {
        body["folder"] = serde_json::Value::String(folder.to_string());
    }
    if let Some(sort) = sort.map(str::trim).filter(|value| !value.is_empty()) {
        body["sort"] = serde_json::Value::String(sort.to_string());
    }
    if group_recent_folders {
        body["group_recent_folders"] = serde_json::Value::Bool(true);
    }

    let authorization = format!("X-Sonarpad-Password: {code}");
    let headers = [
        "Content-Type: application/json",
        "Accept: application/json",
        authorization.as_str(),
    ];
    let bytes = crate::curl_client::CurlClient::post_form_impersonated(
        SONARPAD_AUDIO_DESCRIPTIONS_API_URL,
        &body.to_string(),
        &headers,
    )
    .map_err(|err| format!("Impossibile contattare il catalogo Sonarpad: {err}"))?;
    let response: ApiResponse = serde_json::from_slice(&bytes)
        .map_err(|err| format!("Risposta non valida dal catalogo Sonarpad: {err}"))?;

    if !response.ok {
        let message = response.message.trim();
        return Err(if message.is_empty() {
            "Il catalogo Sonarpad ha rifiutato la richiesta.".to_string()
        } else {
            message.to_string()
        });
    }

    Ok(response.items)
}

#[cfg(test)]
mod tests {
    use super::CatalogItem;

    #[test]
    fn catalog_item_deserializes_plot() {
        let item: CatalogItem = serde_json::from_value(serde_json::json!({
            "type": "file",
            "title": "Titolo",
            "plot": "Una trama di prova."
        }))
        .expect("catalog item");

        assert_eq!(item.plot, "Una trama di prova.");
    }

    #[test]
    fn suggested_download_name_prefers_server_filename() {
        let item = CatalogItem {
            item_type: "file".to_string(),
            title: "Titolo".to_string(),
            path: String::new(),
            modified_at: None,
            modified_timestamp: 0,
            filename: "origine.mp4".to_string(),
            download_filename: "film.mp4".to_string(),
            plot: String::new(),
            download_url: String::new(),
            stream_url: Some("https://example.invalid/stream".to_string()),
        };
        assert_eq!(item.suggested_download_name(), "film.mp4");
    }
}

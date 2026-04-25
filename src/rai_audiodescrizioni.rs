use base64::Engine;
use serde::Deserialize;
use std::collections::BTreeMap;

const CATALOG_URL_B64: &str = "aHR0cHM6Ly9naXRodWIuY29tL0FtYnJvODYvU29uYXJwYWQtVG9vbHMvcmVsZWFzZXMvZG93bmxvYWQvbHVjZS1jYXRhbG9ndWUvbHVjZS1jYXRhbG9ndWUuanNvbg==";

#[derive(Clone, Debug)]
pub struct CatalogItem {
    pub title: String,
    pub date: String,
    pub audio_url: String,
}

#[derive(Clone, Debug)]
pub struct CatalogGroup {
    pub title: String,
    pub items: Vec<CatalogItem>,
}

#[derive(Deserialize)]
struct RawCatalogItem {
    #[serde(default)]
    title: String,
    #[serde(default)]
    program: String,
    #[serde(default)]
    date: String,
    #[serde(default, alias = "audioUrl", alias = "audio_url", alias = "url")]
    audio_url: String,
}

pub fn is_luce_key_missing_error(err: &str) -> bool {
    let lower = err.to_lowercase();
    lower.contains("chiave luce mancante") || lower.contains("codice") && lower.contains("manc")
}

pub fn load_grouped_catalog() -> Result<Vec<CatalogGroup>, String> {
    let catalog_url = catalog_url()?;
    let bytes = crate::curl_client::CurlClient::fetch_url_impersonated(&catalog_url)
        .map_err(|err| format!("Impossibile caricare il catalogo Rai audiodescrizioni: {err}"))?;
    let mut raw_items = parse_catalog(&bytes)?;
    raw_items.retain(|item| !item.audio_url.trim().is_empty());

    let mut grouped: BTreeMap<String, Vec<CatalogItem>> = BTreeMap::new();
    for raw in raw_items {
        let group_title = first_non_empty(&[raw.program.as_str(), raw.title.as_str(), "Rai audiodescrizioni"]);
        let item_title = first_non_empty(&[raw.title.as_str(), raw.program.as_str(), "Audiodescrizione Rai"]);
        grouped.entry(group_title).or_default().push(CatalogItem {
            title: item_title,
            date: raw.date.trim().to_string(),
            audio_url: raw.audio_url.trim().to_string(),
        });
    }

    let mut groups: Vec<CatalogGroup> = grouped.into_iter().map(|(title, mut items)| {
        items.sort_by(|a, b| b.date.cmp(&a.date).then_with(|| a.title.cmp(&b.title)));
        CatalogGroup { title, items }
    }).collect();
    groups.sort_by(|a, b| a.title.cmp(&b.title));
    Ok(groups)
}

pub fn resolve_audio_url(url: &str) -> Result<String, String> {
    let trimmed = url.trim();
    if trimmed.is_empty() { return Err("URL audiodescrizione vuoto.".to_string()); }
    Ok(trimmed.to_string())
}

pub fn resolve_audio_url_for_clipboard(url: &str) -> Result<String, String> { resolve_audio_url(url) }

fn parse_catalog(bytes: &[u8]) -> Result<Vec<RawCatalogItem>, String> {
    if let Ok(items) = serde_json::from_slice::<Vec<RawCatalogItem>>(bytes) { return Ok(items); }
    let value: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|err| format!("Catalogo Rai audiodescrizioni non valido: {err}"))?;
    for key in ["items", "catalog", "data", "results"] {
        if let Some(array) = value.get(key).and_then(serde_json::Value::as_array) {
            return serde_json::from_value::<Vec<RawCatalogItem>>(serde_json::Value::Array(array.clone()))
                .map_err(|err| format!("Elementi Rai audiodescrizioni non validi: {err}"));
        }
    }
    Err("Catalogo Rai audiodescrizioni senza elementi leggibili.".to_string())
}

fn catalog_url() -> Result<String, String> {
    let _key = crate::load_saved_rai_luce_code()
        .ok_or_else(|| "Chiave Luce mancante: inserisci il codice nelle impostazioni.".to_string())?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(CATALOG_URL_B64)
        .map_err(|err| format!("URL catalogo Rai non valido: {err}"))?;
    String::from_utf8(bytes).map_err(|err| format!("URL catalogo Rai non UTF-8: {err}"))
}

fn first_non_empty(values: &[&str]) -> String {
    values.iter().map(|value| value.trim()).find(|value| !value.is_empty()).unwrap_or("").to_string()
}

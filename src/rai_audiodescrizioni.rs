use base64::Engine;
use chrono::{TimeZone, Utc};
use flate2::read::GzDecoder;
use serde::Deserialize;
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::io::Read;

const RAI_AUDIODESCRIZIONI_URL_KEY_A: &[u8] = b"rai-";
const RAI_AUDIODESCRIZIONI_URL_KEY_B: &[u8] = b"audio";
const RAI_AUDIODESCRIZIONI_LIST_URL_B64: &str = "GhUdXRJPS0YdExZHSggBDBwNBxIMXwIaCh0KHBVHTg4YSygCEBMGFVdaNwYBExMZTAVYMAYAHhJGXwQTF0YHFwANXk4YBQABXQYMQwQHBR0KFk4FWAIQSQUGARVHSA8WSgMcHQ8=";
const RAI_AUDIODESCRIZIONI_CATALOGUE_URL_B64: &str = "GhUdXRJPS0YdExZHSggBDBwNBxIMXwIaCh0KHBVHTg4YSygCEBMGFVdaNwYBExMZTAVYMAYAHhJGXwQTF0YHFwANXk4YBQABXQYMQwQHBR0KFk4FWAIQSQoOBgAFQgYAAUcKHAJHRxIaCg==";
const LUCE_PAYLOAD_STATIC_KEY_PARTS: &[&[u8]] = &[b"sonar", b"pad-", b"SonarSecure-"];

#[derive(Clone, Debug)]
pub struct CatalogItem {
    pub title: String,
    pub date: String,
    pub audio_url: String,
    source_order: i64,
}

#[derive(Clone, Debug)]
pub struct CatalogGroup {
    pub title: String,
    pub items: Vec<CatalogItem>,
}

#[derive(Debug, Deserialize)]
struct ExternalItem {
    title: String,
    #[serde(default)]
    #[serde(rename = "partOf")]
    part_of: String,
    #[serde(default)]
    added: Option<i64>,
    #[serde(default)]
    url: String,
}

#[derive(Debug, Deserialize)]
struct ExternalCatalogueGroup {
    title: String,
    #[serde(default)]
    data: Vec<ExternalCatalogueItem>,
}

#[derive(Debug, Deserialize)]
struct ExternalCatalogueItem {
    title: String,
    #[serde(default)]
    url: String,
}

#[derive(Debug, Deserialize)]
struct EncryptedPayload {
    algorithm: String,
    #[serde(rename = "payload_b64")]
    payload_b64: String,
}

pub fn is_luce_key_missing_error(err: &str) -> bool {
    err.starts_with("Chiave Luce mancante:")
}

pub fn load_catalog() -> Result<Vec<CatalogItem>, String> {
    let source_url =
        decode_obfuscated_url(RAI_AUDIODESCRIZIONI_LIST_URL_B64, &obfuscated_url_key())?;
    let raw = fetch_and_decode_luce_payload(&source_url)?;
    let entries: Vec<ExternalItem> =
        serde_json::from_str(&raw).map_err(|err| format!("Catalogo Rai non valido: {err}"))?;
    let mut items_with_added = Vec::new();

    for (item_index, item) in entries.into_iter().enumerate() {
        let title = normalize_item_title(&item.title);
        let audio_url = item.url.trim().to_string();
        if title.is_empty() || audio_url.is_empty() {
            continue;
        }

        let _set_name = item.part_of.trim().to_string();
        let source_order = item_index as i64;
        let (date, _iso_date, _gen_date) = format_added_date(item.added);
        items_with_added.push((
            item.added.unwrap_or(i64::MIN),
            CatalogItem {
                title,
                date,
                audio_url,
                source_order,
            },
        ));
    }

    items_with_added.sort_by(|(left_added, left_item), (right_added, right_item)| {
        right_added
            .cmp(left_added)
            .then_with(|| left_item.source_order.cmp(&right_item.source_order))
    });

    Ok(items_with_added
        .into_iter()
        .map(|(_, item)| item)
        .collect::<Vec<_>>())
}

pub fn load_grouped_catalog() -> Result<Vec<CatalogGroup>, String> {
    let source_url = decode_obfuscated_url(
        RAI_AUDIODESCRIZIONI_CATALOGUE_URL_B64,
        &obfuscated_url_key(),
    )?;
    let raw = fetch_and_decode_luce_payload(&source_url)?;
    let groups: Vec<ExternalCatalogueGroup> = serde_json::from_str(&raw)
        .map_err(|err| format!("Catalogo Rai completo non valido: {err}"))?;
    let mut parsed_groups = Vec::new();

    for group in groups {
        let title = group.title.trim().to_string();
        if title.is_empty() {
            continue;
        }
        let mut items = Vec::new();
        for (item_index, item) in group.data.into_iter().enumerate() {
            let item_title = normalize_item_title(&item.title);
            let audio_url = item.url.trim().to_string();
            if item_title.is_empty() || audio_url.is_empty() {
                continue;
            }
            items.push(CatalogItem {
                title: item_title,
                date: String::new(),
                audio_url,
                source_order: item_index as i64,
            });
        }

        if !items.is_empty() {
            parsed_groups.push(CatalogGroup { title, items });
        }
    }

    Ok(normalize_grouped_catalog(parsed_groups))
}

pub fn search_catalog(query: &str) -> Result<Vec<CatalogItem>, String> {
    let normalized_query = normalize_search_query(query)?;
    let groups = load_grouped_catalog()?;
    let mut matches = Vec::new();

    for group in groups {
        for item in group.items {
            if normalize_search_key(&item.title).contains(&normalized_query) {
                matches.push(item);
            }
        }
    }

    Ok(matches)
}

pub fn resolve_audio_url(audio_url: &str) -> Result<String, String> {
    let audio_url = audio_url.trim();
    if audio_url.is_empty() {
        return Err("L'audiodescrizione selezionata non ha un URL audio disponibile.".to_string());
    }
    Ok(audio_url.to_string())
}

pub fn resolve_audio_url_for_clipboard(audio_url: &str) -> Result<String, String> {
    let audio_url = resolve_audio_url(audio_url)?;
    if !audio_url.contains("/relinker/relinkerServlet") {
        return Ok(audio_url);
    }

    crate::curl_client::CurlClient::resolve_final_url_iphone_impersonated(&audio_url)
        .map_err(|err| format!("Impossibile risolvere l'URL audio Rai: {err}"))
}

fn fetch_text_blocking(url: &str) -> Result<String, String> {
    let bytes = crate::curl_client::CurlClient::fetch_url_impersonated(url)
        .map_err(|err| format!("Impossibile scaricare il catalogo Rai: {err}"))?;
    String::from_utf8(bytes)
        .map_err(|err| format!("Catalogo Rai non decodificabile come UTF-8: {err}"))
}

fn fetch_and_decode_luce_payload(url: &str) -> Result<String, String> {
    let raw = fetch_text_blocking(url)?;
    let payload: EncryptedPayload =
        serde_json::from_str(&raw).map_err(|err| format!("Payload Luce non valido: {err}"))?;
    if payload.algorithm != "gzip-xor-base64-v1" {
        return Err(format!(
            "Algoritmo payload Luce non supportato: {}",
            payload.algorithm
        ));
    }

    let secret_key = resolve_luce_secret_key()?;
    let encrypted = base64::engine::general_purpose::STANDARD
        .decode(payload.payload_b64)
        .map_err(|err| format!("Payload Luce base64 non valido: {err}"))?;
    let decrypted = xor_with_luce_key(&encrypted, &secret_key, LUCE_PAYLOAD_STATIC_KEY_PARTS)?;
    let mut decoder = GzDecoder::new(decrypted.as_slice());
    let mut decoded = String::new();
    decoder
        .read_to_string(&mut decoded)
        .map_err(|err| format!("Payload Luce gzip non valido: {err}"))?;
    Ok(decoded)
}

fn xor_with_luce_key(
    payload: &[u8],
    secret_key: &str,
    static_key_parts: &[&[u8]],
) -> Result<Vec<u8>, String> {
    let mut key = Vec::new();
    for part in static_key_parts {
        key.extend_from_slice(part);
    }
    key.extend_from_slice(secret_key.as_bytes());
    if key.is_empty() {
        return Err("Chiave payload Luce non valida.".to_string());
    }
    Ok(payload
        .iter()
        .enumerate()
        .map(|(index, byte)| byte ^ key[index % key.len()])
        .collect())
}

fn resolve_luce_secret_key() -> Result<String, String> {
    if let Some(secret_key) = crate::load_saved_rai_luce_code() {
        let trimmed = secret_key.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }

    Err("Chiave Luce mancante: inserisci il codice nelle impostazioni RSS/Podcast.".to_string())
}

fn decode_obfuscated_url(encoded: &str, key: &[u8]) -> Result<String, String> {
    if key.is_empty() {
        return Err("Chiave URL Rai non valida.".to_string());
    }

    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|err| format!("URL Rai offuscato non valido: {err}"))?;
    let decoded: Vec<u8> = bytes
        .into_iter()
        .enumerate()
        .map(|(index, byte)| byte ^ key[index % key.len()])
        .collect();
    String::from_utf8(decoded).map_err(|err| format!("URL Rai decodificato non valido: {err}"))
}

fn obfuscated_url_key() -> Vec<u8> {
    [
        RAI_AUDIODESCRIZIONI_URL_KEY_A,
        RAI_AUDIODESCRIZIONI_URL_KEY_B,
    ]
    .concat()
}

fn normalize_grouped_catalog(groups: Vec<CatalogGroup>) -> Vec<CatalogGroup> {
    let mut merged = BTreeMap::<String, Vec<CatalogItem>>::new();

    for group in groups {
        let normalized_title = normalize_group_title(&group.title);
        merged
            .entry(normalized_title)
            .or_default()
            .extend(group.items);
    }

    let mut normalized_groups = merged
        .into_iter()
        .map(|(title, mut items)| {
            items.sort_by(|left, right| {
                compare_natural_labels(
                    &normalize_sort_key(&left.title),
                    &normalize_sort_key(&right.title),
                )
                .then_with(|| compare_natural_labels(&left.title, &right.title))
                .then_with(|| left.source_order.cmp(&right.source_order))
            });
            items.dedup_by(|left, right| dedupe_key(&left.title) == dedupe_key(&right.title));
            CatalogGroup { title, items }
        })
        .collect::<Vec<_>>();

    normalized_groups.sort_by(|left, right| {
        compare_natural_labels(&left.title, &right.title)
            .then_with(|| sortable_label(&left.title).cmp(&sortable_label(&right.title)))
    });
    normalized_groups
}

fn normalize_group_title(title: &str) -> String {
    let trimmed = title.trim();
    let lower = trimmed.to_lowercase();
    if (trimmed.starts_with("Film (") && trimmed.ends_with(')'))
        || lower == "film - audiodescrizioni"
    {
        "Film".to_string()
    } else if lower == "miniserie tv - audiodescrizioni" {
        "Miniserie Tv".to_string()
    } else {
        trimmed.to_string()
    }
}

fn sortable_label(input: &str) -> String {
    input.trim().to_lowercase()
}

fn normalize_sort_key(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn normalize_item_title(input: &str) -> String {
    let trimmed = input.trim();
    let lower = trimmed.to_lowercase();
    if let Some(prefix) = lower.strip_suffix(" - audiodescrizione") {
        let prefix_len = prefix.len();
        trimmed[..prefix_len].trim().to_string()
    } else {
        trimmed.to_string()
    }
}

fn dedupe_key(input: &str) -> String {
    sortable_label(&normalize_sort_key(&normalize_item_title(input)))
}

fn normalize_search_query(query: &str) -> Result<String, String> {
    let normalized = normalize_search_key(query);
    if normalized.is_empty() {
        Err("Inserisci un testo da cercare nelle audiodescrizioni Rai.".to_string())
    } else {
        Ok(normalized)
    }
}

fn normalize_search_key(input: &str) -> String {
    normalize_sort_key(&normalize_item_title(input)).to_lowercase()
}

fn compare_natural_labels(left: &str, right: &str) -> Ordering {
    let left_chars = left.trim().to_lowercase().chars().collect::<Vec<_>>();
    let right_chars = right.trim().to_lowercase().chars().collect::<Vec<_>>();
    let mut left_index = 0usize;
    let mut right_index = 0usize;

    while left_index < left_chars.len() && right_index < right_chars.len() {
        let left_char = left_chars[left_index];
        let right_char = right_chars[right_index];
        if left_char.is_ascii_digit() && right_char.is_ascii_digit() {
            let left_start = left_index;
            while left_index < left_chars.len() && left_chars[left_index].is_ascii_digit() {
                left_index += 1;
            }
            let right_start = right_index;
            while right_index < right_chars.len() && right_chars[right_index].is_ascii_digit() {
                right_index += 1;
            }

            let left_number = left_chars[left_start..left_index]
                .iter()
                .collect::<String>()
                .parse::<u64>()
                .unwrap_or(0);
            let right_number = right_chars[right_start..right_index]
                .iter()
                .collect::<String>()
                .parse::<u64>()
                .unwrap_or(0);
            match left_number.cmp(&right_number) {
                Ordering::Equal => continue,
                non_equal => return non_equal,
            }
        }

        match left_char.cmp(&right_char) {
            Ordering::Equal => {
                left_index += 1;
                right_index += 1;
            }
            non_equal => return non_equal,
        }
    }

    left_chars.len().cmp(&right_chars.len())
}

fn format_added_date(added: Option<i64>) -> (String, Option<String>, Option<String>) {
    let Some(timestamp) = added else {
        return (String::new(), None, None);
    };
    let Some(datetime) = Utc.timestamp_opt(timestamp, 0).single() else {
        return (String::new(), None, None);
    };
    (
        datetime.format("%d/%m/%Y").to_string(),
        Some(datetime.format("%Y-%m-%d").to_string()),
        Some(datetime.format("%d/%m/%Y %H:%M:%S").to_string()),
    )
}

use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::Local;
use flate2::read::GzDecoder;
use serde::Deserialize;
use std::collections::HashMap;
use std::io::Read;

const TV_PAYLOAD_STATIC_KEY_PARTS: &[&[u8]] = &[b"sonar", b"pad-", b"SonarSecure-"];
const LA7_STREAM_URL: &str = "https://d1chghleocc9sm.cloudfront.net/v1/master/3722c60a815c199d9c0ef36c5b73da68a62b09d1/cc-evfku205gqrtf/Live.m3u8";
const OGGI_IN_TV_TIMELINE_URL_PAYLOAD_JSON: &str = r#"{"payload_b64":"csAxIXZQMnhMMuhZfR1S+OWXPRn4oJR5K4nkpYbgWGup/jgB+m6jPWForBe9oLtOwaBOreEeoqetOYbKLTxeLIC4fDkh4S9vy3U4I3E=","algorithm":"gzip-xor-base64-v1"}"#;
const OGGI_IN_TV_GUIDE_URL_PAYLOAD_JSON: &str = r#"{"payload_b64":"csAxIXZQMnhMMiawFTr6bjtEskCkzkNJJ+Zweyc6I0xoq5wAQq2me+nsGOl55vyuggHwBZyk/4KnTrP2iV7rNEEN7i90j4pqQXbXPAgPICMLN0By","algorithm":"gzip-xor-base64-v1"}"#;
const TV_CHANNELS_PAYLOAD_JSON: &str = r#"{"payload_b64":"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","algorithm":"gzip-xor-base64-v1"}"#;
const REGIONAL_TV_CHANNELS_PAYLOAD_JSON: &str = r#"{"payload_b64":"csAxIXZQMnhMMo4nOUegdie+WUmPr85UP2lOC6zV85CqYfPI9UYbz/SJtM5/JQLqurLlENeMVFhVVksN8JN5yUmAdYLumBrJ1Q2fB7ospk0UyD0ZMxWW1GAHYij/w/s9oTk23N536VinN9bLcLiM7jP6bX0npXLmJ6Uf1Df4tWqepV949CrgySuvGsnwuRKOMwr29+XktV9Kpw91dRv1woiB6UJTHb0NW3AnHvgGAv0YOaQ1L5c4fToWQMBubd+1x9urhTHXhu9D6V9ape3C76kEEv18Zc4B9fqsRz/V4ONlUjYbXx/pDc8aeQjUHJ7PSQ9QHxJ2vGARArCFTCdY723R63ouUUcve50hFgC8d2gdjLvKJkpYRXCFOU2CRdNtBX9fkW/mUkMNRUrFpIMcIRtQqXy3I/h1jp1eC3hGBI+aIVyv5QVp3o0/Bxi4NirVK0vG/h2uCAd3sqM0NCYePNipU0KE5FYPBKxKGqW86TF0/dld6VJ2WdSLGrtwihbA3Ph9c4322xZKhes5rB2IuJgpkOT23Y4gmT+chjhu5bu6qGD33GLQy9J0/I9b6WQR2oxef062oD+mSQtHuPSNRS4xgW5tcOLEa4KLuVmqvwbyu2aHgY3HkzdnBI2n12Cu/Og2VL9S0xMRZpdgMZOLGvNW6ya+N13/SJoDfI9nmJaXrpEpD5sXHL08btYpkZE/Iqt/9b24pRDfxKxedsdG5yfIL1Yz5A9giNsGC+jzdgOdDME/5mEbFuTC5k3H3Uj3Da19v7IRJVv8FjGhfTEzOraK++dKu8DszdnyDbqbyKKYbrPZOa2P/LhNco/YU3mUhrSido1gOIyYBqCe1nUPY2j2n0RZqeyjRCR4mbnZS+0e1pgNS06era5pQa+KJuFur1pT2rqN1w5MaYaG47Y5OOrSja2EPqox3tI1CpjdUJh4icYSKus8DatyJKSv23wv7MbuE56iaiLK9yN4owMoDZ5jWNNOxBXluHTzgRIvPYt6JrOkqASIGiG2syPzr5XdIXOobhVEvrYdxnDynFkAH31w8jmMKGCqkWuQG11bmzBGHDe9YTJxiArkhDUMFFTHC+jiUa2GbK1LjRJqx/VQzrV4WlmjswVWrdEQCmg8Jxo9yPC2JlaFniypbwrruufgj4CAMjFi5ExHfkxOvBY40/LAKZ1jBIqLqNzZopRtIDFU3xGRY7+3N8ud8YdSaXCFXn3LqvuwBxV+TakLFVs7YDSXtoawvg8oyMfA/SceCSqP9wbE/BNu3DX5LCAwBE5fFPum1BTfpheVlv/J/NDHwxdSnyps0be7mMzIRLYA1rNJG0vY/0FxnQrBVN1GCVRb1VBbktqQye9tqYvwk8LL/qqnuuVUJcqzmsMXv9lzwtS5zc/unYX+QRPIi21Rn0NiecqkvTpHKgBfip4+cYj1BEx17Ae5La6qO8N0GsJgBwERp2RsvombgGRcITjve9FoyojrKrM+xGHnIeNYtzOlNwKq13X90M7c2s615lNRUn6uhbpNTFhxHbZ0TXxc","algorithm":"gzip-xor-base64-v1"}"#;

#[derive(Clone, Debug)]
pub(crate) struct TvChannel {
    pub(crate) name: String,
    pub(crate) url: String,
    pub(crate) category: TvChannelCategory,
    pub(crate) current_program: Option<String>,
    pub(crate) programs: Vec<TvProgram>,
    pub(crate) guide_channel: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TvChannelCategory {
    Rai,
    Mediaset,
    Other,
    Regional,
}

#[derive(Clone, Debug)]
pub(crate) struct TvProgram {
    pub(crate) channel: String,
    pub(crate) hour: String,
    pub(crate) title: String,
    pub(crate) start_time: i64,
    pub(crate) end_time: i64,
}

impl TvChannel {
    pub(crate) fn display_name(&self) -> String {
        match self.current_program.as_deref() {
            Some(current_program) if !current_program.trim().is_empty() => {
                format!("{}. Ora in onda: {}", self.name, current_program.trim())
            }
            _ => self.name.clone(),
        }
    }
}

#[derive(Deserialize)]
struct TvPayload {
    channels: Vec<TvChannelPayload>,
}

#[derive(Deserialize)]
struct TvChannelPayload {
    name: String,
    url: String,
}

#[derive(Deserialize)]
struct EncryptedPayload {
    algorithm: String,
    payload_b64: String,
}

#[derive(Deserialize)]
struct OggiInTvProgram {
    ch: String,
    hour: String,
    title: String,
    #[serde(rename = "startTime")]
    start_time: i64,
    #[serde(rename = "endTime")]
    end_time: i64,
}

pub(crate) fn load_channels() -> Result<Vec<TvChannel>, String> {
    let raw = decode_tv_payload()?;
    let payload: TvPayload =
        serde_json::from_str(&raw).map_err(|err| format!("Catalogo TV non valido: {err}"))?;
    let mut channels = payload
        .channels
        .into_iter()
        .filter_map(|channel| {
            let name = channel.name.trim().to_string();
            let url = if name == "La7" {
                LA7_STREAM_URL.to_string()
            } else {
                channel.url.trim().to_string()
            };
            if name.is_empty() || url.is_empty() {
                None
            } else {
                Some(TvChannel {
                    category: tv_channel_category(&name),
                    name,
                    url,
                    current_program: None,
                    programs: Vec::new(),
                    guide_channel: None,
                })
            }
        })
        .collect::<Vec<_>>();
    channels.extend(load_regional_channels()?);
    append_current_programs(&mut channels);
    Ok(channels)
}

fn decode_tv_payload() -> Result<String, String> {
    decode_encrypted_payload(TV_CHANNELS_PAYLOAD_JSON, "TV")
}

fn load_regional_channels() -> Result<Vec<TvChannel>, String> {
    let raw = decode_encrypted_payload(REGIONAL_TV_CHANNELS_PAYLOAD_JSON, "TV regionali")?;
    let payload: TvPayload = serde_json::from_str(&raw)
        .map_err(|err| format!("Catalogo TV regionali non valido: {err}"))?;
    Ok(payload
        .channels
        .into_iter()
        .filter_map(|channel| {
            let name = channel.name.trim().to_string();
            let url = channel.url.trim().to_string();
            if name.is_empty() || url.is_empty() {
                None
            } else {
                Some(TvChannel {
                    name,
                    url,
                    category: TvChannelCategory::Regional,
                    current_program: None,
                    programs: Vec::new(),
                    guide_channel: None,
                })
            }
        })
        .collect())
}

fn decode_oggi_in_tv_timeline_url() -> Result<String, String> {
    decode_encrypted_payload(OGGI_IN_TV_TIMELINE_URL_PAYLOAD_JSON, "Oggi in TV")
}

fn decode_oggi_in_tv_guide_url() -> Result<String, String> {
    decode_encrypted_payload(OGGI_IN_TV_GUIDE_URL_PAYLOAD_JSON, "Oggi in TV")
}

fn decode_encrypted_payload(payload_json: &str, label: &str) -> Result<String, String> {
    let payload: EncryptedPayload = serde_json::from_str(payload_json)
        .map_err(|err| format!("Payload {label} non valido: {err}"))?;
    if payload.algorithm != "gzip-xor-base64-v1" {
        return Err(format!(
            "Algoritmo payload {label} non supportato: {}",
            payload.algorithm
        ));
    }
    let secret_key = resolve_tv_secret_key()?;
    let encrypted = STANDARD
        .decode(payload.payload_b64)
        .map_err(|err| format!("Payload {label} base64 non valido: {err}"))?;
    let decrypted = xor_with_tv_key(&encrypted, &secret_key, TV_PAYLOAD_STATIC_KEY_PARTS)?;
    let mut decoder = GzDecoder::new(decrypted.as_slice());
    let mut decoded = String::new();
    decoder
        .read_to_string(&mut decoded)
        .map_err(|err| format!("Payload {label} gzip non valido: {err}"))?;
    Ok(decoded)
}

fn append_current_programs(channels: &mut [TvChannel]) {
    let Ok(programs_by_channel) = fetch_tv_programs() else {
        return;
    };
    let now = Local::now().timestamp();
    for channel in channels {
        let key = normalize_oggi_in_tv_channel_name(&channel.name);
        if let Some(programs) = programs_by_channel.get(&key) {
            channel.programs = programs.clone();
            channel.guide_channel = programs.first().map(|program| program.channel.clone());
            if let Some(program) = programs
                .iter()
                .find(|program| program.start_time <= now && now < program.end_time)
                .or_else(|| {
                    programs
                        .iter()
                        .filter(|program| program.start_time <= now)
                        .max_by_key(|program| program.start_time)
                        .filter(|program| now.saturating_sub(program.start_time) <= 6 * 60 * 60)
                })
            {
                channel.current_program = Some(program.title.clone());
            }
        }
    }
}

fn fetch_tv_programs() -> Result<HashMap<String, Vec<TvProgram>>, String> {
    let template = decode_oggi_in_tv_timeline_url()?;
    let date = Local::now().format("%Y-%m-%d").to_string();
    let url = template.replace("{date}", &date);
    let response = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .user_agent("Sonarpad TV/1.0")
        .build()
        .map_err(|err| err.to_string())?
        .get(&url)
        .send()
        .map_err(|err| err.to_string())?
        .error_for_status()
        .map_err(|err| err.to_string())?
        .text()
        .map_err(|err| err.to_string())?;
    let timeline: Vec<Vec<OggiInTvProgram>> =
        serde_json::from_str(&response).map_err(|err| err.to_string())?;
    let mut programs_by_channel: HashMap<String, Vec<TvProgram>> = HashMap::new();
    for group in timeline {
        for program in group {
            let title = program.title.trim();
            if !title.is_empty() {
                programs_by_channel
                    .entry(normalize_oggi_in_tv_channel_name(&program.ch))
                    .or_default()
                    .push(TvProgram {
                        channel: program.ch.trim().to_string(),
                        hour: program.hour.trim().to_string(),
                        title: title.to_string(),
                        start_time: program.start_time,
                        end_time: program.end_time,
                    });
            }
        }
    }
    Ok(programs_by_channel)
}

pub(crate) fn load_channel_guide(channel: &str, day_offset: i64) -> Result<Vec<TvProgram>, String> {
    let template = decode_oggi_in_tv_guide_url()?;
    let date = (Local::now().date_naive() + chrono::Duration::days(day_offset))
        .format("%Y-%m-%d")
        .to_string();
    let url = template
        .replace("{channel}", &url_encode_component(channel))
        .replace("{date}", &date);
    let response = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .user_agent("Sonarpad TV/1.0")
        .build()
        .map_err(|err| err.to_string())?
        .get(&url)
        .send()
        .map_err(|err| err.to_string())?
        .error_for_status()
        .map_err(|err| err.to_string())?
        .text()
        .map_err(|err| err.to_string())?;
    let guide: Vec<OggiInTvProgram> =
        serde_json::from_str(&response).map_err(|err| err.to_string())?;
    Ok(guide
        .into_iter()
        .filter_map(|program| {
            let title = program.title.trim();
            if title.is_empty() {
                None
            } else {
                Some(TvProgram {
                    channel: program.ch.trim().to_string(),
                    hour: program.hour.trim().to_string(),
                    title: title.to_string(),
                    start_time: program.start_time,
                    end_time: program.end_time,
                })
            }
        })
        .collect())
}

fn url_encode_component(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

fn normalize_oggi_in_tv_channel_name(name: &str) -> String {
    let mut normalized = name
        .to_ascii_lowercase()
        .replace("(dtt)", "")
        .replace(" dtt", "")
        .replace(" hd", "")
        .replace("twenty seven", "27")
        .replace("twentyseven", "27");
    normalized.retain(|ch| ch.is_ascii_alphanumeric());
    if let Some(stripped) = normalized.strip_suffix("hd") {
        normalized = stripped.to_string();
    }
    match normalized.as_str() {
        "la7dtt" => "la7".to_string(),
        "mediaset20" | "20mediaset" => "20".to_string(),
        "mediaset27" | "27mediaset" => "27".to_string(),
        "retequattro" | "rete4mediaset" | "mediasetrete4" => "rete4".to_string(),
        "canale5mediaset" | "mediasetcanale5" => "canale5".to_string(),
        "italia1mediaset" | "mediasetitalia1" => "italia1".to_string(),
        "italia2mediaset" | "mediasetitalia2" => "italia2".to_string(),
        "sportitalialive24" => "sportitalia".to_string(),
        "virginradio" => "virginradiotv".to_string(),
        _ if normalized.contains("rete4") || normalized.contains("retequattro") => {
            "rete4".to_string()
        }
        _ => normalized,
    }
}

fn tv_channel_category(name: &str) -> TvChannelCategory {
    let normalized = normalize_oggi_in_tv_channel_name(name);
    if normalized.starts_with("rai") {
        return TvChannelCategory::Rai;
    }
    if matches!(
        normalized.as_str(),
        "rete4"
            | "canale5"
            | "italia1"
            | "mediaset20"
            | "20"
            | "iris"
            | "27"
            | "twentyseven"
            | "la5"
            | "mediasetextra"
            | "cine34"
            | "focus"
            | "italia2"
            | "boing"
            | "topcrime"
            | "cartoonito"
            | "tgcom24"
    ) {
        TvChannelCategory::Mediaset
    } else {
        TvChannelCategory::Other
    }
}

fn xor_with_tv_key(
    payload: &[u8],
    secret_key: &str,
    static_key_parts: &[&[u8]],
) -> Result<Vec<u8>, String> {
    let mut key = secret_key.as_bytes().to_vec();
    for part in static_key_parts {
        key.extend_from_slice(part);
    }
    if key.is_empty() {
        return Err("Chiave payload TV non valida.".to_string());
    }
    Ok(payload
        .iter()
        .enumerate()
        .map(|(index, byte)| byte ^ key[index % key.len()])
        .collect())
}

fn resolve_tv_secret_key() -> Result<String, String> {
    if let Some(secret_key) = crate::load_saved_rai_luce_code() {
        let trimmed = secret_key.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }
    Err("Chiave Luce mancante: inserisci il codice nelle impostazioni RSS/Podcast.".to_string())
}

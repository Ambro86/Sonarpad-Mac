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

#[derive(Clone, Debug)]
pub(crate) struct TvChannel {
    pub(crate) name: String,
    pub(crate) url: String,
    pub(crate) current_program: Option<String>,
    pub(crate) programs: Vec<TvProgram>,
    pub(crate) guide_channel: Option<String>,
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
                    name,
                    url,
                    current_program: None,
                    programs: Vec::new(),
                    guide_channel: None,
                })
            }
        })
        .collect::<Vec<_>>();
    append_current_programs(&mut channels);
    Ok(channels)
}

fn decode_tv_payload() -> Result<String, String> {
    decode_encrypted_payload(TV_CHANNELS_PAYLOAD_JSON, "TV")
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
        .replace("twenty seven", "27");
    normalized.retain(|ch| ch.is_ascii_alphanumeric());
    match normalized.as_str() {
        "la7dtt" => "la7".to_string(),
        "mediaset20" | "20mediaset" => "20".to_string(),
        "sportitalialive24" => "sportitalia".to_string(),
        "virginradio" => "virginradiotv".to_string(),
        _ => normalized,
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

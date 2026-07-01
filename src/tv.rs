use crate::{SONARPAD_ROUTE_CLIENT_TOKEN, append_podcast_log};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::Local;
use flate2::read::GzDecoder;
use serde::Deserialize;
use std::collections::HashMap;
use std::io::Read;

const TV_PAYLOAD_STATIC_KEY_PARTS: &[&[u8]] = &[b"sonar", b"pad-", b"SonarSecure-"];
const LA7_STREAM_URL: &str = "https://d1chghleocc9sm.cloudfront.net/v1/master/3722c60a815c199d9c0ef36c5b73da68a62b09d1/cc-evfku205gqrtf/Live.m3u8";
const LA7_CINEMA_DASH_URL: &str = "https://d15umi5iaezxgx.cloudfront.net/HBBTV/LA7D/DASH/Live.mpd";
const SONARPAD_TV_TOKEN: &str = "";
const OGGI_IN_TV_TIMELINE_URL_PAYLOAD_JSON: &str = r#"{"payload_b64":"csAxIXZQMnhMMuhZfR1S+OWXPRn4oJR5K4nkpYbgWGup/jgB+m6jPWForBe9oLtOwaBOreEeoqetOYbKLTxeLIC4fDkh4S9vy3U4I3E=","algorithm":"gzip-xor-base64-v1"}"#;
const OGGI_IN_TV_GUIDE_URL_PAYLOAD_JSON: &str = r#"{"payload_b64":"csAxIXZQMnhMMiawFTr6bjtEskCkzkNJJ+Zweyc6I0xoq5wAQq2me+nsGOl55vyuggHwBZyk/4KnTrP2iV7rNEEN7i90j4pqQXbXPAgPICMLN0By","algorithm":"gzip-xor-base64-v1"}"#;
const TV_CHANNELS_REMOTE_URL_PAYLOAD_JSON: &str = r#"{"payload_b64":"csAxIXZQMnhOxyawhT16bj9FstrDzDx0LnYYFeIHCjUg0J4I2IRoLCC6WbXJFAJY66itPMER8CXxsWb8uE7xHG1LOQ==","algorithm":"gzip-xor-base64-v1"}"#;
#[allow(dead_code)]
const TV_CHANNELS_PAYLOAD_JSON: &str = r#"{"payload_b64":"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","algorithm":"gzip-xor-base64-v1"}"#;
#[allow(dead_code)]
const REGIONAL_TV_CHANNELS_PAYLOAD_JSON: &str = r#"{"payload_b64":"csAxIXZQMnhMMo4nOUegdie+WUmPr85Uj4uCpYb+eOdb7HZeI0qNCD7T7Xlq+7Z34/VojdnssHYJrF8MsIN5yUmAdYLumBrJ3Y1fD74xbk4UyD4bsx3g1GAH6rS84/ksoTk2E5536VjBOx7IYFz//hI2DPNEaUMkNk3znFwDXVNEy6+Gs3OSPhzVG0YQg3Bp3eSj7+Wkpd4LYzxx8+P6woCB2UIxKrXej36b/GQGUDa4cAtnmqla69uoehBDRXsxpp4RqRyyEb2tNmQ3RJW5O0OAP5bLMLL7atRd6lJbLxaATCMxAitjohs7CnWm8GxiW/cpCuMxbLffbLf1U/FkojcCWoVMxjem+MWbDOpyuO3x5muELmrcBfSFOTGK+6DNBm7b8XfmwsQz3QmoQRHIYcfwje/j6a/9frT35X3LSJAB0+UwSVGeMvCSSyeQcTJY3NtQUPd59Qt0svtvFwdksay9OjgKmvZiPQ5k+4hsR/DPixpoUFDgJboB3rR9+kw2i1mzYuAt7dEazKqaDh9C4uZTFVroYKOYNLxcfPoDvLE4NM0LJo6uRuYotBkMQS6GKep2TCJciQi9d+cbBLcfoVRw3Yajra9G/ZLbihg22u50J3iLTuUkSfsUHWepqHC90J//JfXmqFYzNKCZe5KO5vtS6z6+F1P51ZkPdA1En4a3jokxcqsPFN38rlbpsXG4IuphfOeNxneAac0Cjxlm1p/E7BdAnZkWb0g/i8gTcTmVjP5/4OICFuz5mn3DHcnXD61/vpIBJVvcgr8JnPx3OuYCfkoBgt3XSOoVjd2rqOKYfZyj64RqZrXV2olVFmKSBjU+9aVCugpQaYSeOgOcaO3qhFh7lqmb1blGphnTTe0eV4EtT068jcyJlzeGZ9n9I7ZqIVodQZXSJwJnRFNQLXnn6d3g86kkH5uyX2+/GiEWGGMlNIHm4g/wnzFkRgr9R/OSo2Fn9slLaF5vcpsYVy7GQK+O7p3wN0eSRb3Y39Qbbz2LB58SAEU432sE7yI6wA4cHgit33noxPQWM2kGNWfSFvJILtsEpWMAJYJnmwxGTIQgLwJRjGnZ8nnBdaUpLNf7ap9EHcW3PdAjH2Hjs+EG9SGj6f6D/x9RGzuowx7Am1t4odnr+KqBC34Z7jhe3ixIhkEE5pqfiPnqheU6X5gG/OZMKymJwo2+uOSPdWsu7A2nUaHJyUuv2sKbPjxELlIfjs8cWK21XEvL5fk9wfo3KdGjQkDrRKo9dpJbEzDKsexZqrT9c+A3ELAhv6gIMI0TRLCApOyH3E9/hFv7uzzcd4W10GEzWaOz1BZ+pVZy0+NCEyrQeFJDS8FxorM/2VPCGIPgejoFJ/UcaJeYyEILf1ZwcHb0G/SmyQTBY7F4eLnhwbvnVehk6clOMa09b9Lk+vxkvSAGuAbTnN+ZQGRWuk77GmFgTyho/0tmf+5y0XUG7zm8FXEjH0yiGtHoO7jrCkXH+pRGTPi9gA3RKZkhlfOiwa2bVrk/6/VtUloaCv0hrS0mvPojNLr6SPFEQDayGWwEluc+TWqkJO9VdvTNXcVIDDBvPnm6WcvsyUkcPX4PnbDVm+w5mzvNd4e8B7jHWHJM","algorithm":"gzip-xor-base64-v1"}"#;

#[derive(Clone, Debug)]
pub(crate) struct TvChannel {
    pub(crate) name: String,
    pub(crate) url: String,
    pub(crate) category: String,
    pub(crate) has_guide: bool,
    pub(crate) current_program: Option<String>,
    pub(crate) programs: Vec<TvProgram>,
    pub(crate) guide_channel: Option<String>,
    pub(crate) guide_name: Option<String>,
    pub(crate) tvg_id: Option<String>,
    pub(crate) stream_resolver: Option<String>,
    pub(crate) resolver_endpoint: Option<String>,
    pub(crate) resolver_realm: Option<String>,
    pub(crate) resolver_channel_id: Option<String>,
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
#[allow(dead_code)]
struct TvPayload {
    channels: Vec<TvChannelPayload>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct TvChannelPayload {
    name: String,
    url: String,
}

#[derive(Deserialize)]
struct RemoteTvPayload {
    channels: Vec<RemoteTvChannelPayload>,
}

#[derive(Deserialize)]
struct RemoteTvChannelPayload {
    name: String,
    url: String,
    tvg_name: Option<String>,
    tvg_id: Option<String>,
    group_title: Option<String>,
    has_guide: Option<bool>,
    stream_resolver: Option<String>,
    resolver_endpoint: Option<String>,
    resolver_realm: Option<String>,
    resolver_channel_id: Option<String>,
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
    let mut channels = fetch_remote_channels().map_err(|err| {
        format!(
            "Impossibile scaricare i canali TV dal catalogo Sonarpad: {err}"
        )
    })?;
    append_current_programs(&mut channels);
    Ok(channels)
}

#[allow(dead_code)]
fn load_local_channels() -> Result<Vec<TvChannel>, String> {
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
                let has_guide = !is_dash_stream_url(&url);
                Some(TvChannel {
                    category: tv_channel_category(&name),
                    name,
                    url,
                    has_guide,
                    current_program: None,
                    programs: Vec::new(),
                    guide_channel: None,
                    guide_name: None,
                    tvg_id: None,
                    stream_resolver: None,
                    resolver_endpoint: None,
                    resolver_realm: None,
                    resolver_channel_id: None,
                })
            }
        })
        .collect::<Vec<_>>();
    channels.extend(load_regional_channels()?);
    append_current_programs(&mut channels);
    Ok(channels)
}

fn fetch_remote_channels() -> Result<Vec<TvChannel>, String> {
    let remote_url = decode_encrypted_payload(
        TV_CHANNELS_REMOTE_URL_PAYLOAD_JSON,
        "TV Channels Remote URL",
    )?;
    let route_token_present = !SONARPAD_ROUTE_CLIENT_TOKEN.trim().is_empty();
    append_podcast_log(&format!(
        "tv.remote.request begin url={} tv_token_mode=mobile_static route_token_present={} route_token_len={}",
        remote_url,
        route_token_present,
        SONARPAD_ROUTE_CLIENT_TOKEN.len()
    ));

    let response = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("Sonarpad TV/1.0")
        .build()
        .map_err(|err| {
            append_podcast_log(&format!("tv.remote.client_build_error err={}", err));
            err.to_string()
        })?
        .get(&remote_url)
        .header("Accept", "application/json")
        .header("X-Sonarpad-TV-Token", SONARPAD_TV_TOKEN)
        .header("X-Sonarpad-Route-Token", SONARPAD_ROUTE_CLIENT_TOKEN)
        .send()
        .map_err(|err| {
            append_podcast_log(&format!("tv.remote.request_error err={}", err));
            err.to_string()
        })?;

    let status = response.status();
    let body = response.text().map_err(|err| {
        append_podcast_log(&format!("tv.remote.read_body_error status={} err={}", status, err));
        err.to_string()
    })?;
    append_podcast_log(&format!(
        "tv.remote.response status={} success={} body_len={}",
        status,
        status.is_success(),
        body.len()
    ));
    if !status.is_success() {
        let snippet: String = body.chars().take(500).collect();
        append_podcast_log(&format!(
            "tv.remote.response_error status={} body_snippet={}",
            status,
            snippet.replace('\n', " ").replace('\r', " ")
        ));
        return Err(format!("HTTP {} dal catalogo TV Sonarpad", status));
    }

    let payload: RemoteTvPayload = serde_json::from_str(&body).map_err(|err| {
        let snippet: String = body.chars().take(500).collect();
        append_podcast_log(&format!(
            "tv.remote.json_error err={} body_snippet={}",
            err,
            snippet.replace('\n', " ").replace('\r', " ")
        ));
        err.to_string()
    })?;
    append_podcast_log(&format!(
        "tv.remote.parsed channels={}",
        payload.channels.len()
    ));

    let channels = payload
        .channels
        .into_iter()
        .filter_map(|channel| {
            let name = channel.name.trim().trim_start_matches(|c: char| c == '[' || c.is_ascii_digit() || c == ']' || c.is_whitespace()).trim().to_string();
            let mut url = channel.url.trim().to_string();
            if name == "La7" {
                url = LA7_STREAM_URL.to_string();
            }
            if matches!(name.as_str(), "La7 Cinema" | "La7D" | "LA7D") {
                url = LA7_CINEMA_DASH_URL.to_string();
            }
            if name.is_empty() || url.is_empty() {
                return None;
            }
            let has_guide = channel.has_guide.unwrap_or(true) && !is_dash_stream_url(&url);
            Some(TvChannel {
                category: tv_channel_category_from_group(
                    channel.group_title.as_deref().unwrap_or_default(),
                    &name,
                ),
                name,
                url,
                has_guide,
                current_program: None,
                programs: Vec::new(),
                guide_channel: None,
                guide_name: channel
                    .tvg_name
                    .map(|name| name.trim().to_string())
                    .filter(|name| !name.is_empty()),
                tvg_id: channel
                    .tvg_id
                    .map(|id| id.trim().to_string())
                    .filter(|id| !id.is_empty()),
                stream_resolver: channel.stream_resolver,
                resolver_endpoint: channel.resolver_endpoint,
                resolver_realm: channel.resolver_realm,
                resolver_channel_id: channel.resolver_channel_id,
            })
        })
        .collect::<Vec<_>>();
    if channels.is_empty() {
        Err("Catalogo TV remoto vuoto".to_string())
    } else {
        Ok(channels)
    }
}

#[allow(dead_code)]
fn decode_tv_payload() -> Result<String, String> {
    decode_encrypted_payload(TV_CHANNELS_PAYLOAD_JSON, "TV")
}

#[allow(dead_code)]
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
                    category: "Regionali".to_string(),
                    has_guide: false,
                    current_program: None,
                    programs: Vec::new(),
                    guide_channel: None,
                    guide_name: None,
                    tvg_id: None,
                    stream_resolver: None,
                    resolver_endpoint: None,
                    resolver_realm: None,
                    resolver_channel_id: None,
                })
            }
        })
        .collect())
}

fn is_dash_stream_url(url: &str) -> bool {
    url.split('?')
        .next()
        .unwrap_or(url)
        .to_ascii_lowercase()
        .ends_with(".mpd")
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
        if !channel.has_guide {
            continue;
        }
        let mut lookup_keys = Vec::new();
        for value in [
            Some(channel.name.as_str()),
            channel.guide_name.as_deref(),
            channel.tvg_id.as_deref(),
            channel
                .tvg_id
                .as_deref()
                .and_then(|id| id.strip_suffix(".it")),
        ]
        .into_iter()
        .flatten()
        {
            let key = normalize_oggi_in_tv_channel_name(value);
            if !key.is_empty() && !lookup_keys.contains(&key) {
                lookup_keys.push(key);
            }
        }
        if let Some(programs) = lookup_keys.iter().find_map(|key| programs_by_channel.get(key)) {
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

fn tv_channel_category(name: &str) -> String {
    let normalized = normalize_oggi_in_tv_channel_name(name);
    if normalized.starts_with("rai") {
        return "Rai".to_string();
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
        "Mediaset".to_string()
    } else {
        "Altri".to_string()
    }
}

fn tv_channel_category_from_group(group: &str, name: &str) -> String {
    let trimmed = group.trim();
    match trimmed.to_ascii_lowercase().as_str() {
        "rai" => "Rai".to_string(),
        "mediaset" => "Mediaset".to_string(),
        "regionali" => "Regionali".to_string(),
        _ if trimmed.starts_with("Regionali - ") => trimmed.to_string(),
        _ if !trimmed.is_empty() => trimmed.to_string(),
        _ => tv_channel_category(name),
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

use serde::Serialize;
use serde_json::json;

#[derive(Serialize)]
struct AuroraDeviceInfo {
    #[serde(rename = "adBlocker")]
    ad_blocker: bool,
    #[serde(rename = "drmSupported")]
    drm_supported: bool,
    #[serde(rename = "hdrCapabilities")]
    hdr_capabilities: Vec<&'static str>,
    #[serde(rename = "hwDecodingCapabilities")]
    hw_decoding_capabilities: Vec<&'static str>,
    #[serde(rename = "soundCapabilities")]
    sound_capabilities: Vec<&'static str>,
}

#[derive(Serialize)]
struct AuroraDevice {
    browser: serde_json::Value,
    #[serde(rename = "type")]
    device_type: &'static str,
}

#[derive(Serialize)]
struct AuroraWisteria {
    device: AuroraDevice,
    platform: &'static str,
}

#[derive(Serialize)]
struct AuroraPayload<'a> {
    #[serde(rename = "channelId")]
    channel_id: &'a str,
    #[serde(rename = "deviceInfo")]
    device_info: AuroraDeviceInfo,
    #[serde(rename = "wisteriaProperties")]
    wisteria_properties: AuroraWisteria,
}

pub(crate) fn resolve_tv_channel_url(channel: &TvChannel) -> Result<String, String> {
    
    let mut eff_resolver = channel.stream_resolver.as_deref();
    let mut eff_channel_id = channel.resolver_channel_id.as_deref();
    
    let norm_name = channel.name.to_lowercase().replace(" ", "");
    match norm_name.as_str() {
        "realtime" => { eff_resolver = Some("aurora_channel"); eff_channel_id = Some("2"); },
        "nove" | "la9" | "9" => { eff_resolver = Some("aurora_channel"); eff_channel_id = Some("3"); },
        "dmax" => { eff_resolver = Some("aurora_channel"); eff_channel_id = Some("4"); },
        "foodnetwork" => { eff_resolver = Some("aurora_channel"); eff_channel_id = Some("6"); },
        "motortrend" => { eff_resolver = Some("aurora_channel"); eff_channel_id = Some("11"); },
        "discoverychannel" => { eff_resolver = Some("aurora_channel"); eff_channel_id = Some("12"); },
        "hgtv" => { eff_resolver = Some("aurora_channel"); eff_channel_id = Some("13"); },
        _ => {}
    }

    if let Some(resolver) = eff_resolver
        && resolver == "aurora_channel" {

            let endpoint = channel
                .resolver_endpoint
                .as_deref()
                .unwrap_or("https://public.aurora.enhanced.live");
            let realm = channel.resolver_realm.as_deref().unwrap_or("it");
            let channel_id = eff_channel_id.ok_or("Missing channel_id")?;

            let client = reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .map_err(|e| e.to_string())?;

            let token_url = format!("{}/token?realm={}", endpoint, realm);
            let token_resp: serde_json::Value = client.get(&token_url)
                .header("Accept", "application/json,text/plain,*/*")
                .header("Origin", "https://nove.tv")
                .header("Referer", &channel.url)
                .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/136.0.0.0 Safari/537.36")
                .header("X-disco-client", "WEB:UNKNOWN:wbdatv:2.1.9")
                .header("X-disco-params", format!("realm={}", realm))
                .header("X-Device-Info", "STONEJS/1 (Unknown/Unknown; Windows/10; Unknown)")
                .send()
                .map_err(|e| format!("Network error: {}", e))?
                .json()
                .map_err(|e| format!("JSON error: {}", e))?;

            let token = token_resp["data"]["attributes"]["token"]
                .as_str()
                .ok_or("Aurora token not found in response")?;

            let payload = AuroraPayload {
                channel_id,
                device_info: AuroraDeviceInfo {
                    ad_blocker: false,
                    drm_supported: true,
                    hdr_capabilities: vec!["SDR"],
                    hw_decoding_capabilities: vec![],
                    sound_capabilities: vec!["STEREO"],
                },
                wisteria_properties: AuroraWisteria {
                    device: AuroraDevice {
                        browser: json!({ "name": "chrome", "version": "136" }),
                        device_type: "desktop",
                    },
                    platform: "desktop",
                },
            };

            let playback_url = format!("{}/playback/v3/channelPlaybackInfo", endpoint);
            let pb_resp: serde_json::Value = client.post(&playback_url)
                .header("Accept", "application/json,text/plain,*/*")
                .header("Content-Type", "application/json")
                .header("Origin", "https://nove.tv")
                .header("Referer", &channel.url)
                .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/136.0.0.0 Safari/537.36")
                .header("X-disco-client", "WEB:UNKNOWN:wbdatv:2.1.9")
                .header("X-disco-params", format!("realm={}", realm))
                .header("X-Device-Info", "STONEJS/1 (Unknown/Unknown; Windows/10; Unknown)")
                .header("Authorization", format!("Bearer {}", token))
                .json(&payload)
                .send()
                .map_err(|e| format!("Network error pb: {}", e))?
                .json()
                .map_err(|e| format!("JSON pb error: {}", e))?;

            // recursive search for .m3u8 url
            fn find_m3u8(val: &serde_json::Value) -> Option<String> {
                if let Some(s) = val.as_str() {
                    if s.contains(".m3u8") && s.starts_with("http") {
                        return Some(s.to_string());
                    }
                } else if let Some(arr) = val.as_array() {
                    for v in arr {
                        if let Some(url) = find_m3u8(v) {
                            return Some(url);
                        }
                    }
                } else if let Some(obj) = val.as_object() {
                    for v in obj.values() {
                        if let Some(url) = find_m3u8(v) {
                            return Some(url);
                        }
                    }
                }
                None
            }

            return find_m3u8(&pb_resp).ok_or("Aurora stream url not found".to_string());
        }

    Ok(channel.url.clone())
}


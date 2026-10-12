use anyhow::{Result, anyhow};
use chrono::{DateTime, Utc};
use futures_util::{SinkExt, StreamExt};
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::VecDeque;
use std::fmt;
use std::future::Future;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{Duration, Instant};
use tokio::net::TcpStream;
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async,
    tungstenite::{self, client::IntoClientRequest, http::HeaderValue, protocol::Message},
};
use uuid::Uuid;

pub const TRUSTED_CLIENT_TOKEN: &str = "6A5AA1D4EAFF4E9FB37E23D68491D6F4";
pub const WSS_URL_BASE: &str =
    "wss://speech.platform.bing.com/consumer/speech/synthesize/readaloud/edge/v1";
pub const VOICE_LIST_URL: &str =
    "https://speech.platform.bing.com/consumer/speech/synthesize/readaloud/voices/list";
pub const EDGE_TTS_MAX_BYTES: usize = 1800;
pub const EDGE_TTS_REALTIME_MAX_BYTES: usize = 900;
const EDGE_TTS_REALTIME_FIRST_CHUNK_MAX_BYTES: usize = 280;
const EDGE_TTS_CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
const EDGE_TTS_SEND_TIMEOUT: Duration = Duration::from_secs(5);
const EDGE_TTS_READ_TIMEOUT: Duration = Duration::from_secs(20);
const EDGE_TTS_TURN_TIMEOUT: Duration = Duration::from_secs(60);
const EDGE_TTS_CANCEL_POLL: Duration = Duration::from_millis(100);
const EDGE_TTS_MIN_AUDIO_BYTES: usize = 1000;
const EDGE_TTS_MAX_AUDIO_BYTES: usize = 16 * 1024 * 1024;
// Edge 155 is the primary read-aloud service. Sonarpad Windows confirmed
// the v1-compatible signature by receiving complete MP3 audio from it.
// Keep the earlier endpoint as a working fallback, not as another pipeline.
const EDGE_TTS_MODERN_WSS_URL: &str =
    "wss://api.msedgeservices.com/tts/cognitiveservices/websocket/v1";
const EDGE_TTS_MODERN_CLIENT_VERSION: &str = "1-155.0.4283.45";
const EDGE_TTS_LEGACY_CLIENT_VERSION: &str = "1-132.0.2917.39";
const EDGE_TTS_MODERN_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/155.0.0.0 Safari/537.36 Edg/155.0.0.0";
const EDGE_TTS_LEGACY_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/132.0.0.0 Safari/537.36 Edg/132.0.0.0";
// The old voices/list endpoint is retained to avoid unrelated behavior changes.
const EDGE_TTS_USER_AGENT: &str = EDGE_TTS_LEGACY_USER_AGENT;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EdgeEndpoint {
    Modern,
    Legacy,
}

impl EdgeEndpoint {
    fn label(self) -> &'static str {
        match self {
            Self::Modern => "Edge 155 (api.msedgeservices.com)",
            Self::Legacy => "legacy (speech.platform.bing.com)",
        }
    }

    fn url(self) -> &'static str {
        match self {
            Self::Modern => EDGE_TTS_MODERN_WSS_URL,
            Self::Legacy => WSS_URL_BASE,
        }
    }

    fn query_key(self) -> &'static str {
        match self {
            Self::Modern => "Ocp-Apim-Subscription-Key",
            Self::Legacy => "TrustedClientToken",
        }
    }

    fn client_version(self) -> &'static str {
        match self {
            Self::Modern => EDGE_TTS_MODERN_CLIENT_VERSION,
            Self::Legacy => EDGE_TTS_LEGACY_CLIENT_VERSION,
        }
    }

    fn user_agent(self) -> &'static str {
        match self {
            Self::Modern => EDGE_TTS_MODERN_USER_AGENT,
            Self::Legacy => EDGE_TTS_LEGACY_USER_AGENT,
        }
    }
}


// The service's authenticated Date header can correct a local clock skew.
// Reconnect normally with a new request; never modify a caller's system clock.
static EDGE_TTS_CLOCK_OFFSET_SECONDS: AtomicI64 = AtomicI64::new(0);

type EdgeSocket = WebSocketStream<MaybeTlsStream<TcpStream>>;
type EdgeSocketWriter = futures_util::stream::SplitSink<EdgeSocket, Message>;
type EdgeSocketReader = futures_util::stream::SplitStream<EdgeSocket>;

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct VoiceInfo {
    #[serde(rename = "ShortName")]
    pub short_name: String,
    #[serde(rename = "FriendlyName")]
    pub friendly_name: String,
    #[serde(rename = "Locale")]
    pub locale: String,
    #[serde(rename = "SuggestedCodec")]
    pub suggested_codec: String,
}

#[derive(Debug)]
struct EdgeHttpFailure {
    status: u16,
    retry_after: Option<Duration>,
    endpoint: EdgeEndpoint,
}

impl fmt::Display for EdgeHttpFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let status = tungstenite::http::StatusCode::from_u16(self.status)
            .ok()
            .and_then(|status| status.canonical_reason())
            .unwrap_or("HTTP error");
        write!(
            f,
            "Edge TTS HTTP {} {} endpoint={}",
            self.status, status, self.endpoint.label()
        )
    }
}

impl std::error::Error for EdgeHttpFailure {}

pub fn is_edge_text_usable(text: &str) -> bool {
    normalize_for_tts(text).chars().any(char::is_alphanumeric)
}

fn is_permanent_edge_message(message: &str) -> bool {
    let message = message.to_lowercase();
    [
        "audio_description_cancelled",
        "edge_tts_cancelled",
        "401 unauthorized",
        "400 bad request",
        "404 not found",
        "invalid certificate",
        "certificate verify failed",
        "unknown issuer",
        "unknown voice",
        "invalid voice",
        "voce non selezionata",
        "nome della voce non valido",
        "input exceeds the supported text byte limit",
    ]
    .iter()
    .any(|needle| message.contains(needle))
}

pub fn is_transient_edge_message(message: &str) -> bool {
    if is_permanent_edge_message(message) {
        return false;
    }
    let message = message.to_lowercase();
    [
        "audio vuoto",
        "empty audio",
        "empty payload",
        "no audio sent",
        "file audio troppo piccolo",
        "audio too small",
        "invalid audio",
        "decode failed",
        "truncated audio",
        "timeout",
        "timed out",
        "temporar",
        "connection reset",
        "connection refused",
        "connection aborted",
        "connection closed",
        "failed to connect",
        "failed to lookup address information",
        "name or service not known",
        "no address associated with hostname",
        "nodename nor servname",
        "forcibly closed",
        "os error 10054",
        "websocket",
        "web socket",
        "edge ws:",
        "network",
        "broken pipe",
        "unexpected eof",
        "closed without",
        "too many requests",
        "403 forbidden",
        "http status code: 403",
        "http 403",
        "429",
        "500 internal server error",
        "502 bad gateway",
        "503 service unavailable",
        "504 gateway timeout",
    ]
    .iter()
    .any(|needle| message.contains(needle))
}

pub fn is_transient_edge_error(error: &anyhow::Error) -> bool {
    // A generic WebSocket/I/O wrapper must not turn a nested permanent error
    // into an endless retry, especially an invalid voice or TLS certificate.
    if error
        .chain()
        .any(|cause| is_permanent_edge_message(&cause.to_string()))
    {
        return false;
    }
    for cause in error.chain() {
        if let Some(error) = cause.downcast_ref::<EdgeHttpFailure>() {
            return is_transient_http_status(error.status);
        }
        if let Some(error) = cause.downcast_ref::<tungstenite::Error>() {
            match error {
                tungstenite::Error::Http(response) => {
                    return is_transient_http_status(response.status().as_u16());
                }
                tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed => {
                    return true;
                }
                tungstenite::Error::Io(error) => return is_transient_io_error(error),
                // Malformed/truncated messages and remote socket resets may leave
                // an unusable session, so the next attempt opens a fresh socket.
                tungstenite::Error::Protocol(_) | tungstenite::Error::Utf8(_) => return true,
                _ => {}
            }
        }
        if let Some(error) = cause.downcast_ref::<std::io::Error>() {
            return is_transient_io_error(error);
        }
    }
    error
        .chain()
        .any(|cause| is_transient_edge_message(&cause.to_string()))
}

fn is_transient_http_status(status: u16) -> bool {
    matches!(status, 403 | 408 | 425 | 429 | 500 | 502 | 503 | 504)
}

fn is_transient_io_error(error: &std::io::Error) -> bool {
    use std::io::ErrorKind;
    matches!(
        error.kind(),
        ErrorKind::ConnectionRefused
            | ErrorKind::ConnectionReset
            | ErrorKind::ConnectionAborted
            | ErrorKind::NotConnected
            | ErrorKind::BrokenPipe
            | ErrorKind::TimedOut
            | ErrorKind::WouldBlock
            | ErrorKind::Interrupted
            | ErrorKind::UnexpectedEof
            | ErrorKind::NetworkUnreachable
            | ErrorKind::HostUnreachable
            | ErrorKind::Other
    ) || is_transient_edge_message(&error.to_string())
}

// Normal connection errors retain the existing quick retry. HTTP 429 uses
// a slower 5–60 second backoff; Retry-After is always a minimum wait.
pub fn edge_retry_delay(error: &anyhow::Error, retry: usize) -> Duration {
    let message = format!("{error:#}").to_lowercase();
    if is_edge_rate_limited(error) {
        // Respect throttling instead of hammering Microsoft: 5, 10, ... 60s.
        let delay = Duration::from_secs((retry.min(12) as u64 * 5).min(60));
        return with_server_retry_after(error, delay);
    }
    let fast = [
        "403",
        "os error 10054",
        "connection reset",
        "forcibly closed by the remote host",
        "timeout",
    ]
    .iter()
    .any(|needle| message.contains(needle));
    let delay =
        Duration::from_millis((retry.min(8) as u64 * if fast { 250 } else { 400 }).min(2_000));
    with_server_retry_after(error, delay)
}

fn bounded_edge_retry_delay(error: &anyhow::Error, retry: usize) -> Duration {
    // Preview/export keeps its previous short waits unless the server
    // explicitly rate-limits us (HTTP 429).
    if is_edge_rate_limited(error) {
        return edge_retry_delay(error, retry);
    }
    let delay = Duration::from_millis((retry.min(5) as u64 * 400).min(2_000));
    with_server_retry_after(error, delay)
}

fn is_edge_rate_limited(error: &anyhow::Error) -> bool {
    for cause in error.chain() {
        if let Some(failure) = cause.downcast_ref::<EdgeHttpFailure>() {
            return failure.status == 429;
        }
        if let Some(tungstenite::Error::Http(response)) = cause.downcast_ref::<tungstenite::Error>() {
            return response.status().as_u16() == 429;
        }
    }
    let message = format!("{error:#}").to_lowercase();
    message.contains("429") || message.contains("too many requests")
}

fn with_server_retry_after(error: &anyhow::Error, delay: Duration) -> Duration {
    error
        .chain()
        .filter_map(|cause| cause.downcast_ref::<EdgeHttpFailure>())
        .filter_map(|failure| failure.retry_after)
        .fold(delay, Duration::max)
}

pub async fn get_edge_voices() -> Result<Vec<VoiceInfo>> {
    let client = reqwest::Client::builder()
        .user_agent(EDGE_TTS_USER_AGENT)
        .connect_timeout(EDGE_TTS_CONNECT_TIMEOUT)
        .timeout(EDGE_TTS_TURN_TIMEOUT)
        .build()?;
    let url = format!(
        "{}?TrustedClientToken={}",
        VOICE_LIST_URL, TRUSTED_CLIENT_TOKEN
    );
    Ok(client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json::<Vec<VoiceInfo>>()
        .await?)
}

// Compatibility path for short operations without a cancellation token.
// max_retries has always denoted the total attempt budget in the Mac code.
#[allow(dead_code)]
pub async fn synthesize_text_with_retry(
    text: &str,
    voice: &str,
    rate: i32,
    pitch: i32,
    volume: i32,
    max_retries: usize,
) -> Result<Vec<u8>> {
    synthesize_text_with_retry_cancellable(
        text,
        voice,
        rate,
        pitch,
        volume,
        Some(max_retries),
        || true,
    )
    .await?
    .ok_or_else(|| anyhow!("EDGE_TTS_CANCELLED"))
}

/// None for max_attempts recovers transient errors until the caller stops.
/// Ok(None) means cancelled, including a late result from an old session.
/// Only this text chunk is retried; completed chunks remain with the caller.
pub async fn synthesize_text_with_retry_cancellable<F>(
    text: &str,
    voice: &str,
    rate: i32,
    pitch: i32,
    volume: i32,
    max_attempts: Option<usize>,
    is_active: F,
) -> Result<Option<Vec<u8>>>
where
    F: Fn() -> bool,
{
    if !is_active() {
        return Ok(None);
    }
    let parts = split_text_edge_with_limit(text, EDGE_TTS_MAX_BYTES);
    let mut complete_audio = Vec::new();
    for part in parts {
        let result = retry_edge_operation(
            || synthesize_text_chunk(&part, voice, rate, pitch, volume),
            max_attempts,
            &is_active,
            "chunk",
            if max_attempts.is_some() {
                bounded_edge_retry_delay
            } else {
                edge_retry_delay
            },
        )
        .await?;
        let Some(audio) = result else {
            return Ok(None);
        };
        complete_audio.extend_from_slice(&audio);
    }
    Ok(is_active().then_some(complete_audio))
}

async fn retry_edge_operation<T, G, Fut, F>(
    mut generate: G,
    max_attempts: Option<usize>,
    is_active: &F,
    operation: &str,
    delay_for_retry: fn(&anyhow::Error, usize) -> Duration,
) -> Result<Option<T>>
where
    G: FnMut() -> Fut,
    Fut: Future<Output = Result<T>>,
    F: Fn() -> bool,
{
    let mut attempt = 1usize;
    loop {
        if !is_active() {
            return Ok(None);
        }
        match await_while_active(generate(), is_active).await {
            Ok(result) => return Ok(result),
            Err(error) => {
                if !is_active() {
                    return Ok(None);
                }
                let retryable = is_transient_edge_error(&error);
                let exhausted = max_attempts.is_some_and(|limit| attempt >= limit.max(1));
                if !retryable || exhausted {
                    log_edge_failure(operation, attempt, None, &error);
                    return Err(error);
                }
                let delay = delay_for_retry(&error, attempt);
                log_edge_failure(operation, attempt, Some(delay), &error);
                if !wait_before_retry(delay, is_active).await {
                    return Ok(None);
                }
                attempt = attempt.saturating_add(1);
            }
        }
    }
}

// Dropping the pinned future also drops an in-flight connect/receive and its
// socket. Polling is independent of network timeouts, so Stop stays responsive.
async fn await_while_active<T, Fut, F>(future: Fut, is_active: &F) -> Result<Option<T>>
where
    Fut: Future<Output = Result<T>>,
    F: Fn() -> bool,
{
    if !is_active() {
        return Ok(None);
    }
    tokio::pin!(future);
    loop {
        tokio::select! {
            result = &mut future => {
                return if is_active() { result.map(Some) } else { Ok(None) };
            }
            _ = tokio::time::sleep(EDGE_TTS_CANCEL_POLL) => {
                if !is_active() {
                    return Ok(None);
                }
            }
        }
    }
}

async fn wait_before_retry<F>(mut delay: Duration, is_active: &F) -> bool
where
    F: Fn() -> bool,
{
    while !delay.is_zero() {
        if !is_active() {
            return false;
        }
        let slice = delay.min(EDGE_TTS_CANCEL_POLL);
        tokio::time::sleep(slice).await;
        delay = delay.saturating_sub(slice);
    }
    is_active()
}

fn log_edge_failure(
    operation: &str,
    attempt: usize,
    delay: Option<Duration>,
    error: &anyhow::Error,
) {
    crate::append_podcast_log(&format!(
        "edge_tts.{} attempt={} retry={} delay_ms={} err={}",
        operation,
        attempt,
        delay.is_some(),
        delay.map_or(0, |delay| delay.as_millis()),
        error
    ));
}

pub struct EdgeRealtimeSession {
    write: EdgeSocketWriter,
    read: EdgeSocketReader,
    healthy: bool,
    // A 403 in a reused reading session switches the remaining reading to
    // fresh connections, matching Windows without resending completed chunks.
    fresh_for_each_chunk: bool,
    endpoint: EdgeEndpoint,
}

fn format_rate(rate: i32) -> String {
    format!("{:+}%", rate)
}

fn format_pitch(pitch: i32) -> String {
    format!("{:+}Hz", pitch)
}

fn format_volume(volume: i32) -> String {
    let delta = volume.saturating_sub(100);
    format!("{:+}%", delta)
}

// Only the endpoint name/status are logged; never expose a GEC, subscription
// key, cookie, connection id or a full WebSocket URL in diagnostics.
fn edge_socket_request(
    endpoint: EdgeEndpoint,
    connection_id: &str,
) -> Result<tungstenite::http::Request<()>> {
    let url = format!(
        "{}?{}={}&ConnectionId={}&Sec-MS-GEC={}&Sec-MS-GEC-Version={}",
        endpoint.url(),
        endpoint.query_key(),
        TRUSTED_CLIENT_TOKEN,
        connection_id,
        generate_sec_ms_gec(),
        endpoint.client_version()
    );
    let mut request = url.as_str().into_client_request()?;
    let headers = request.headers_mut();
    headers.insert("Pragma", HeaderValue::from_static("no-cache"));
    headers.insert("Cache-Control", HeaderValue::from_static("no-cache"));
    headers.insert(
        "Origin",
        HeaderValue::from_static("chrome-extension://jdiccldimpdaibmpdkjnbmckianbfold"),
    );
    headers.insert("User-Agent", HeaderValue::from_static(endpoint.user_agent()));
    headers.insert("Accept-Encoding", HeaderValue::from_static("gzip, deflate, br"));
    headers.insert("Accept-Language", HeaderValue::from_static("en-US,en;q=0.9"));
    headers.insert(
        "Cookie",
        HeaderValue::from_str(&format!("muid={};", generate_muid()))?,
    );
    Ok(request)
}

async fn connect_edge_socket_route(endpoint: EdgeEndpoint) -> Result<EdgeSocket> {
    let connection_id = Uuid::new_v4().simple().to_string().to_uppercase();
    let request = edge_socket_request(endpoint, &connection_id)?;
    match tokio::time::timeout(EDGE_TTS_CONNECT_TIMEOUT, connect_async(request)).await {
        Ok(Ok((socket, _response))) => Ok(socket),
        Ok(Err(error)) => Err(map_connect_error_for_endpoint(error, endpoint)),
        Err(_) => Err(anyhow!("Edge WebSocket connect timeout endpoint={}", endpoint.label())),
    }
}

async fn connect_edge_socket() -> Result<(EdgeSocket, EdgeEndpoint)> {
    match connect_edge_socket_route(EdgeEndpoint::Modern).await {
        Ok(socket) => {
            crate::append_podcast_log(&format!(
                "Edge TTS: WebSocket connected (HTTP 101), endpoint={} [PRIMARY]",
                EdgeEndpoint::Modern.label()
            ));
            Ok((socket, EdgeEndpoint::Modern))
        }
        Err(primary_error) => {
            crate::append_podcast_log(&format!(
                "Edge TTS: primary endpoint={} failed: {}; trying fallback={}",
                EdgeEndpoint::Modern.label(), primary_error, EdgeEndpoint::Legacy.label()
            ));
            match connect_edge_socket_route(EdgeEndpoint::Legacy).await {
                Ok(socket) => {
                    crate::append_podcast_log(&format!(
                        "Edge TTS: WebSocket connected (HTTP 101), endpoint={} [FALLBACK]",
                        EdgeEndpoint::Legacy.label()
                    ));
                    Ok((socket, EdgeEndpoint::Legacy))
                }
                Err(fallback_error) => {
                    crate::append_podcast_log(&format!(
                        "Edge TTS: both endpoints failed: primary={} error={}; fallback={} error={}",
                        EdgeEndpoint::Modern.label(), primary_error,
                        EdgeEndpoint::Legacy.label(), fallback_error
                    ));
                    // Preserve structured Retry-After and error classification.
                    // In particular, do not lose a rate limit from the primary
                    // endpoint just because the legacy handshake also failed.
                    if is_edge_rate_limited(&primary_error)
                        && !is_edge_rate_limited(&fallback_error)
                        && is_transient_edge_error(&fallback_error)
                    {
                        Err(primary_error)
                    } else if is_transient_edge_error(&fallback_error) {
                        Err(fallback_error)
                    } else if is_transient_edge_error(&primary_error) {
                        Err(primary_error)
                    } else {
                        Err(fallback_error)
                    }
                }
            }
        }
    }
}

fn map_connect_error_for_endpoint(
    error: tungstenite::Error,
    endpoint: EdgeEndpoint,
) -> anyhow::Error {
    if let tungstenite::Error::Http(response) = error {
        let status = response.status().as_u16();
        if status == 403
            && let Some(server_time) = response
                .headers()
                .get("Date")
                .and_then(|value| value.to_str().ok())
                .and_then(parse_http_date)
        {
            let offset = server_time.saturating_sub(Utc::now().timestamp());
            let previous = EDGE_TTS_CLOCK_OFFSET_SECONDS.swap(offset, Ordering::Relaxed);
            crate::append_podcast_log(&format!(
                "edge_tts.clock_sync status=403 offset_seconds={} changed={} endpoint={}",
                offset, previous != offset, endpoint.label()
            ));
        }
        let retry_after = response
            .headers()
            .get("Retry-After")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| parse_retry_after(value, Utc::now().timestamp()));
        return EdgeHttpFailure {
            status,
            retry_after,
            endpoint,
        }
        .into();
    }
    // Never serialize the request, auth URL or response body into the log.
    error.into()
}

#[cfg(test)]
fn map_connect_error(error: tungstenite::Error) -> anyhow::Error {
    map_connect_error_for_endpoint(error, EdgeEndpoint::Legacy)
}

fn parse_http_date(value: &str) -> Option<i64> {
    DateTime::parse_from_rfc2822(value.trim())
        .ok()
        .map(|value| value.timestamp())
}

fn parse_retry_after(value: &str, now: i64) -> Option<Duration> {
    if let Ok(seconds) = value.trim().parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }
    parse_http_date(value)
        .map(|timestamp| Duration::from_secs(timestamp.saturating_sub(now).max(0) as u64))
}

fn speech_config_message() -> Message {
    Message::Text(format!(
        "X-Timestamp:{}\r\nContent-Type:application/json; charset=utf-8\r\nPath:speech.config\r\n\r\n{{\"context\":{{\"synthesis\":{{\"audio\":{{\"metadataoptions\":{{\"sentenceBoundaryEnabled\":\"false\",\"wordBoundaryEnabled\":\"false\"}},\"outputFormat\":\"audio-24khz-48kbitrate-mono-mp3\"}}}}}}}}",
        get_date_string()
    ).into())
}

fn ssml_message(text: &str, voice: &str, rate: i32, pitch: i32, volume: i32) -> Message {
    let lang = voice.split('-').take(2).collect::<Vec<_>>().join("-");
    let lang = if lang.contains('-') {
        lang.as_str()
    } else {
        "en-US"
    };
    let ssml = format!(
        "<speak version='1.0' xmlns='http://www.w3.org/2001/10/synthesis' xml:lang='{}'><voice name='{}'><prosody pitch='{}' rate='{}' volume='{}'>{}</prosody></voice></speak>",
        escape_xml(lang),
        escape_xml(voice),
        format_pitch(pitch),
        format_rate(rate),
        format_volume(volume),
        escape_xml(text)
    );
    Message::Text(format!(
        "X-RequestId:{}\r\nContent-Type:application/ssml+xml\r\nX-Timestamp:{}Z\r\nPath:ssml\r\n\r\n{}",
        Uuid::new_v4().simple(),
        get_date_string(),
        ssml
    ).into())
}

pub async fn synthesize_text_chunk(
    text: &str,
    voice: &str,
    rate: i32,
    pitch: i32,
    volume: i32,
) -> Result<Vec<u8>> {
    let normalized_text = normalize_for_tts(text);
    if !normalized_text.chars().any(char::is_alphanumeric) {
        return Ok(Vec::new());
    }
    validate_voice(voice)?;
    let mut session = EdgeRealtimeSession::connect().await?;
    session
        .synthesize_chunk(&normalized_text, voice, rate, pitch, volume)
        .await
}

fn validate_voice(voice: &str) -> Result<()> {
    if voice.trim().is_empty() {
        return Err(anyhow!("Edge TTS: voce non selezionata"));
    }
    if voice.chars().any(char::is_control) {
        return Err(anyhow!("Edge TTS: nome della voce non valido"));
    }
    Ok(())
}

impl EdgeRealtimeSession {
    pub async fn connect() -> Result<Self> {
        let started = Instant::now();
        let (socket, endpoint) = connect_edge_socket().await?;
        let (mut write, read) = socket.split::<Message>();
        tokio::time::timeout(EDGE_TTS_SEND_TIMEOUT, write.send(speech_config_message()))
            .await
            .map_err(|_| anyhow!("Edge speech.config send timeout"))??;
        crate::append_podcast_log(&format!(
            "edge_tts.realtime.connect_ok elapsed_ms={}",
            started.elapsed().as_millis()
        ));
        Ok(Self {
            write,
            read,
            healthy: true,
            fresh_for_each_chunk: false,
            endpoint,
        })
    }

    pub fn invalidate(&mut self) {
        self.healthy = false;
    }

    pub async fn synthesize_chunk(
        &mut self,
        text: &str,
        voice: &str,
        rate: i32,
        pitch: i32,
        volume: i32,
    ) -> Result<Vec<u8>> {
        let started = Instant::now();
        let normalized_text = normalize_for_tts(text);
        if !normalized_text.chars().any(char::is_alphanumeric) {
            return Ok(Vec::new());
        }
        validate_voice(voice)?;
        if edge_wire_text_len(&normalized_text) > EDGE_TTS_MAX_BYTES {
            return Err(anyhow!(
                "Edge TTS input exceeds the supported text byte limit"
            ));
        }
        if !self.healthy {
            return Err(anyhow!(
                "Edge WebSocket session needs reconnect after interrupted turn"
            ));
        }
        self.healthy = false;
        tokio::time::timeout(
            EDGE_TTS_SEND_TIMEOUT,
            self.write
                .send(ssml_message(&normalized_text, voice, rate, pitch, volume)),
        )
        .await
        .map_err(|_| anyhow!("Edge SSML send timeout"))??;

        let audio_data = receive_complete_audio(
            &mut self.write,
            &mut self.read,
            EDGE_TTS_READ_TIMEOUT,
            EDGE_TTS_TURN_TIMEOUT,
        )
        .await?;
        // Only an audio turn terminated by turn.end is reported as successful.
        crate::append_podcast_log(&format!(
            "Edge TTS: SYNTHESIS SUCCEEDED, endpoint={} audio_bytes={} path=single_chunk",
            self.endpoint.label(),
            audio_data.len()
        ));
        self.healthy = true;
        if audio_data.len() < 4096 && normalized_text.encode_utf16().count() > 600 {
            crate::append_podcast_log(&format!(
                "edge_tts.suspicious_audio utf16_len={} bytes={}",
                normalized_text.encode_utf16().count(),
                audio_data.len()
            ));
        }
        crate::append_podcast_log(&format!(
            "edge_tts.realtime.chunk_done chars={} bytes={} elapsed_ms={}",
            normalized_text.chars().count(),
            audio_data.len(),
            started.elapsed().as_millis()
        ));
        Ok(audio_data)
    }
}

// Complete each turn before publishing its bytes. EOF/Close is never a
// substitute for turn.end: the partial buffer is discarded on every failure.
async fn receive_complete_audio(
    write: &mut EdgeSocketWriter,
    read: &mut EdgeSocketReader,
    read_timeout: Duration,
    turn_timeout: Duration,
) -> Result<Vec<u8>> {
    let deadline = tokio::time::Instant::now() + turn_timeout;
    let mut audio_data = Vec::new();
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return Err(anyhow!("Edge audio turn timeout"));
        }
        let message = tokio::time::timeout(remaining.min(read_timeout), read.next())
            .await
            .map_err(|_| anyhow!("Edge audio read timeout"))?
            .ok_or_else(|| anyhow!("Edge connection closed without turn.end; truncated audio"))??;
        match message {
            Message::Text(text) if edge_message_path(&text) == Some("turn.end") => {
                validate_complete_audio(&audio_data)?;
                return Ok(audio_data);
            }
            Message::Binary(data) => {
                if let Some(audio) = parse_edge_binary_audio_payload(&data)? {
                    if audio_data.len().saturating_add(audio.len()) > EDGE_TTS_MAX_AUDIO_BYTES {
                        return Err(anyhow!(
                            "Edge WS: audio response exceeds the chunk size limit"
                        ));
                    }
                    audio_data.extend_from_slice(&audio);
                }
            }
            Message::Close(frame) => {
                return Err(anyhow!(
                    "Edge connection closed without turn.end; truncated audio close_code={}",
                    frame.map_or_else(
                        || "none".to_string(),
                        |frame| u16::from(frame.code).to_string()
                    )
                ));
            }
            Message::Ping(payload) => {
                let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
                tokio::time::timeout(
                    remaining.min(EDGE_TTS_SEND_TIMEOUT),
                    write.send(Message::Pong(payload)),
                )
                .await
                .map_err(|_| anyhow!("Edge WebSocket pong send timeout"))??;
            }
            _ => {}
        }
    }
}

fn edge_message_header<'a>(message: &'a str, header: &str) -> Option<&'a str> {
    for line in message.split("\r\n") {
        if line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':')
            && name.trim().eq_ignore_ascii_case(header)
        {
            return Some(value.trim());
        }
    }
    None
}

fn edge_message_path(message: &str) -> Option<&str> {
    edge_message_header(message, "Path")
}

fn validate_complete_audio(audio: &[u8]) -> Result<()> {
    if audio.is_empty() {
        return Err(anyhow!("Edge TTS empty audio after turn.end"));
    }
    if audio.len() < EDGE_TTS_MIN_AUDIO_BYTES {
        return Err(anyhow!(
            "Edge TTS audio too small after turn.end: {} bytes (minimum {})",
            audio.len(),
            EDGE_TTS_MIN_AUDIO_BYTES
        ));
    }
    Ok(())
}

#[allow(dead_code)]
pub async fn synthesize_realtime_chunk_with_retry(
    session: Option<EdgeRealtimeSession>,
    text: &str,
    voice: &str,
    rate: i32,
    pitch: i32,
    volume: i32,
    max_retries: usize,
) -> Result<(Vec<u8>, Option<EdgeRealtimeSession>)> {
    synthesize_realtime_chunk_with_retry_cancellable(
        session,
        text,
        voice,
        rate,
        pitch,
        volume,
        Some(max_retries),
        || true,
    )
    .await?
    .ok_or_else(|| anyhow!("EDGE_TTS_CANCELLED"))
}

// Keep the existing voice/prosody call shape while adding explicit cancellation.
#[allow(clippy::too_many_arguments)]
pub async fn synthesize_realtime_chunk_with_retry_cancellable<F>(
    session: Option<EdgeRealtimeSession>,
    text: &str,
    voice: &str,
    rate: i32,
    pitch: i32,
    volume: i32,
    max_attempts: Option<usize>,
    is_active: F,
) -> Result<Option<(Vec<u8>, Option<EdgeRealtimeSession>)>>
where
    F: Fn() -> bool,
{
    synthesize_realtime_with_connector(
        session,
        text,
        voice,
        rate,
        pitch,
        volume,
        max_attempts,
        is_active,
        EdgeRealtimeSession::connect,
        if max_attempts.is_some() {
            bounded_edge_retry_delay
        } else {
            edge_retry_delay
        },
    )
    .await
}

// The connector and backoff are injectable so the real recovery state machine
// can be tested against local WebSockets, without contacting the Edge service.
#[allow(clippy::too_many_arguments)]
async fn synthesize_realtime_with_connector<F, C, CFut>(
    session: Option<EdgeRealtimeSession>,
    text: &str,
    voice: &str,
    rate: i32,
    pitch: i32,
    volume: i32,
    max_attempts: Option<usize>,
    is_active: F,
    mut connect: C,
    delay_for_retry: fn(&anyhow::Error, usize) -> Duration,
) -> Result<Option<(Vec<u8>, Option<EdgeRealtimeSession>)>>
where
    F: Fn() -> bool,
    C: FnMut() -> CFut,
    CFut: Future<Output = Result<EdgeRealtimeSession>>,
{
    if !is_active() {
        return Ok(None);
    }
    if !is_edge_text_usable(text) {
        return Ok(Some((Vec::new(), session)));
    }
    validate_voice(voice)?;
    let mut fresh_for_each_chunk = session
        .as_ref()
        .is_some_and(|session| session.fresh_for_each_chunk);
    let mut session = session.filter(|session| session.healthy && !session.fresh_for_each_chunk);
    let mut attempt = 1usize;
    loop {
        if !is_active() {
            return Ok(None);
        }
        let result = await_while_active(
            async {
                if session.is_none() {
                    let mut connected = connect().await?;
                    connected.fresh_for_each_chunk = fresh_for_each_chunk;
                    session = Some(connected);
                }
                session
                    .as_mut()
                    .ok_or_else(|| anyhow!("Edge WebSocket session unavailable"))?
                    .synthesize_chunk(text, voice, rate, pitch, volume)
                    .await
            },
            &is_active,
        )
        .await;
        match result {
            Ok(Some(audio)) => return Ok(Some((audio, session))),
            Ok(None) => return Ok(None),
            Err(error) => {
                // Drop the failed session and its partial audio; a retry must
                // never consume late frames from the previous failed request.
                session = None;
                if is_forbidden_edge_error(&error) {
                    if !fresh_for_each_chunk {
                        crate::append_podcast_log("edge_tts.reading.fresh_sessions_after_403");
                    }
                    fresh_for_each_chunk = true;
                }
                if !is_active() {
                    return Ok(None);
                }
                let exhausted = max_attempts.is_some_and(|limit| attempt >= limit.max(1));
                if !is_transient_edge_error(&error) || exhausted {
                    log_edge_failure("realtime", attempt, None, &error);
                    return Err(error);
                }
                let delay = delay_for_retry(&error, attempt);
                log_edge_failure("realtime", attempt, Some(delay), &error);
                if !wait_before_retry(delay, &is_active).await {
                    return Ok(None);
                }
                attempt = attempt.saturating_add(1);
            }
        }
    }
}

fn is_forbidden_edge_error(error: &anyhow::Error) -> bool {
    for cause in error.chain() {
        if let Some(error) = cause.downcast_ref::<EdgeHttpFailure>() {
            return error.status == 403;
        }
        if let Some(tungstenite::Error::Http(response)) = cause.downcast_ref::<tungstenite::Error>()
        {
            return response.status().as_u16() == 403;
        }
    }
    let message = format!("{error:#}").to_lowercase();
    ["403 forbidden", "http 403", "http status code: 403"]
        .iter()
        .any(|needle| message.contains(needle))
}

fn generate_sec_ms_gec() -> String {
    generate_sec_ms_gec_at(
        Utc::now()
            .timestamp()
            .saturating_add(EDGE_TTS_CLOCK_OFFSET_SECONDS.load(Ordering::Relaxed)),
    )
}

fn generate_sec_ms_gec_at(unix_seconds: i64) -> String {
    let win_epoch = 11_644_473_600i64;
    let seconds = unix_seconds.saturating_add(win_epoch);
    let ticks = (seconds - seconds.rem_euclid(300)) as i128 * 10_000_000;
    let str_to_hash = format!("{}{}", ticks, TRUSTED_CLIENT_TOKEN);
    let mut hasher = Sha256::new();
    hasher.update(str_to_hash);
    hex::encode(hasher.finalize()).to_uppercase()
}

fn generate_muid() -> String {
    let mut rng = rand::thread_rng();
    let mut bytes = [0u8; 16];
    rng.fill(&mut bytes);
    hex::encode(bytes).to_uppercase()
}

fn get_date_string() -> String {
    Utc::now()
        .format("%a %b %d %Y %H:%M:%S GMT+0000 (Coordinated Universal Time)")
        .to_string()
}

pub fn escape_xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn normalize_for_tts(text: &str) -> String {
    let normalized = sanitize_symbols_for_tts(&decode_html_entities_for_tts(text))
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    let mut out = String::with_capacity(normalized.len());
    let mut chars = normalized.chars().peekable();
    let mut pending_space = false;

    while let Some(ch) = chars.next() {
        if ch == '\n' {
            let mut newline_count = 1usize;
            while chars.peek().is_some_and(|next| *next == '\n') {
                newline_count += 1;
                chars.next();
            }
            if newline_count >= 2 {
                while out.ends_with(' ') {
                    out.pop();
                }
                if !out.is_empty() && !out.ends_with("\n\n") {
                    out.push('\n');
                    out.push('\n');
                }
                pending_space = false;
            } else if !out.is_empty() {
                pending_space = true;
            }
            continue;
        }

        if ch.is_whitespace() {
            if !out.is_empty() {
                pending_space = true;
            }
            continue;
        }

        if pending_space
            && ch.is_uppercase()
            && out.ends_with('.')
            && !out.ends_with("..")
            && !out.ends_with(". ")
        {
            out.push(' ');
            pending_space = false;
        }

        if pending_space && !out.ends_with([' ', '\n']) {
            out.push(' ');
        }
        pending_space = false;
        out.push(ch);
    }

    out.trim().to_string()
}

fn sanitize_symbols_for_tts(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut dot_run = 0usize;
    let mut bang_run = 0usize;
    let mut question_run = 0usize;

    for ch in text.chars() {
        match ch {
            '.' => {
                dot_run += 1;
                continue;
            }
            '!' => {
                bang_run += 1;
                continue;
            }
            '?' => {
                question_run += 1;
                continue;
            }
            _ => {
                flush_tts_punctuation_runs(&mut out, &mut dot_run, &mut bang_run, &mut question_run)
            }
        }

        match ch {
            '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{2060}' | '\u{FEFF}' => {}
            '\u{00A0}' | '\u{1680}' | '\u{2000}' | '\u{2001}' | '\u{2002}' | '\u{2003}'
            | '\u{2004}' | '\u{2005}' | '\u{2006}' | '\u{2007}' | '\u{2008}' | '\u{2009}'
            | '\u{200A}' | '\u{202F}' | '\u{205F}' | '\u{3000}' | '\u{2420}' | '\u{2423}'
            | '\u{2409}' => out.push(' '),
            '\u{240A}' | '\u{240D}' | '\u{2424}' => out.push('\n'),
            '`' | '´' | 'ˋ' | 'ˊ' | '|' | '¦' | '‖' | '⁄' | '∕' => out.push(' '),
            '«' | '»' | '“' | '”' | '„' | '‟' | '〝' | '〞' => out.push(' '),
            '‘' | '’' | '‚' | '‛' | '‹' | '›' => out.push('\''),
            '–' | '—' | '―' | '−' => out.push('-'),
            '…' => out.push('.'),
            _ if is_unwanted_control_char(ch) => out.push(' '),
            _ => out.push(ch),
        }
    }
    flush_tts_punctuation_runs(&mut out, &mut dot_run, &mut bang_run, &mut question_run);
    normalize_tts_terminal_punctuation(&out)
}

fn flush_tts_punctuation_runs(
    out: &mut String,
    dot_run: &mut usize,
    bang_run: &mut usize,
    question_run: &mut usize,
) {
    if *dot_run >= 3 {
        trim_trailing_tts_spaces(out);
        out.push('.');
    } else if *dot_run > 0 {
        trim_trailing_tts_spaces(out);
        for _ in 0..*dot_run {
            out.push('.');
        }
    }
    if *bang_run >= 3 {
        trim_trailing_tts_spaces(out);
        out.push('!');
    } else if *bang_run > 0 {
        trim_trailing_tts_spaces(out);
        for _ in 0..*bang_run {
            out.push('!');
        }
    }
    if *question_run >= 3 {
        trim_trailing_tts_spaces(out);
        out.push('?');
    } else if *question_run > 0 {
        trim_trailing_tts_spaces(out);
        for _ in 0..*question_run {
            out.push('?');
        }
    }
    *dot_run = 0;
    *bang_run = 0;
    *question_run = 0;
}

fn trim_trailing_tts_spaces(text: &mut String) {
    // Tighten punctuation without consuming the existing paragraph pauses.
    while text
        .chars()
        .last()
        .is_some_and(|ch| ch.is_whitespace() && ch != '\n' && ch != '\r')
    {
        text.pop();
    }
}

fn is_unwanted_control_char(ch: char) -> bool {
    let code = ch as u32;
    (0..=8).contains(&code)
        || (11..=12).contains(&code)
        || (14..=31).contains(&code)
        || code == 0x7f
}

fn normalize_tts_terminal_punctuation(text: &str) -> String {
    text.replace(".\":", ". ")
        .replace(".':", ". ")
        .replace(".”:", ". ")
        .replace(".’:", ". ")
}

fn decode_html_entities_for_tts(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '&' {
            out.push(ch);
            continue;
        }

        let mut entity = String::new();
        let mut ended_with_semicolon = false;
        while let Some(&next) = chars.peek() {
            chars.next();
            if next == ';' {
                ended_with_semicolon = true;
                break;
            }
            if entity.len() >= 16 {
                entity.push(next);
                break;
            }
            entity.push(next);
        }

        let decoded = if entity.starts_with("#x") || entity.starts_with("#X") {
            u32::from_str_radix(&entity[2..], 16)
                .ok()
                .and_then(char::from_u32)
        } else if let Some(num) = entity.strip_prefix('#') {
            num.parse::<u32>().ok().and_then(char::from_u32)
        } else {
            match entity.as_str() {
                "nbsp" => Some(' '),
                "amp" => Some('&'),
                "quot" | "quote" => Some('"'),
                "apos" => Some('\''),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "laquo" => Some('«'),
                "raquo" => Some('»'),
                "hellip" => Some('…'),
                "ndash" => Some('–'),
                "mdash" => Some('—'),
                "rsquo" => Some('’'),
                "lsquo" => Some('‘'),
                "rdquo" => Some('”'),
                "ldquo" => Some('“'),
                "agrave" => Some('à'),
                "egrave" => Some('è'),
                "eacute" => Some('é'),
                "igrave" => Some('ì'),
                "ograve" => Some('ò'),
                "ugrave" => Some('ù'),
                _ => None,
            }
        };

        if let Some(decoded) = decoded {
            out.push(decoded);
        } else {
            out.push('&');
            out.push_str(&entity);
            if ended_with_semicolon {
                out.push(';');
            }
        }
    }
    out
}

fn parse_edge_binary_audio_payload(data: &[u8]) -> Result<Option<Vec<u8>>> {
    if data.len() < 2 {
        return Err(anyhow!("Edge WS: binary frame missing header length"));
    }
    let be_len = u16::from_be_bytes([data[0], data[1]]) as usize;
    let le_len = u16::from_le_bytes([data[0], data[1]]) as usize;
    let header_len = if be_len > 0 && data.len() >= be_len + 2 {
        be_len
    } else if le_len > 0 && data.len() >= le_len + 2 {
        // Compatibility with the existing Windows/mobile binary-frame parser.
        le_len
    } else {
        return Err(anyhow!("Edge WS: invalid binary header length"));
    };
    let header = std::str::from_utf8(&data[2..2 + header_len])
        .map_err(|_| anyhow!("Edge WS: invalid binary header text"))?;
    let payload = &data[2 + header_len..];
    if !edge_message_path(header).is_some_and(|path| path.eq_ignore_ascii_case("audio")) {
        return Err(anyhow!("Edge WS: binary frame path is not audio"));
    }
    match edge_message_header(header, "Content-Type") {
        Some(kind) if kind.eq_ignore_ascii_case("audio/mpeg") => {
            if payload.is_empty() {
                Err(anyhow!("Edge WS: audio frame has empty payload"))
            } else {
                Ok(Some(payload.to_vec()))
            }
        }
        Some(_) => Err(anyhow!("Edge WS: unexpected binary content type")),
        // Edge's empty terminator is valid only when it has no Content-Type.
        None if payload.is_empty() => Ok(None),
        None => Err(anyhow!(
            "Edge WS: missing content type with non-empty payload"
        )),
    }
}

// --- Logica di segmentazione (da Sonarpad) ---

pub fn split_sentences_lazy(text: &str) -> impl Iterator<Item = &str> {
    let mut start = 0;
    let mut iter = text.char_indices().peekable();

    std::iter::from_fn(move || {
        if start >= text.len() {
            return None;
        }
        while let Some((idx, ch)) = iter.next() {
            if matches!(ch, '.' | '!' | '?' | ';' | ':') {
                let next_ch = iter.peek().map(|(_, c)| *c);
                if ch == '.' && matches!(next_ch, Some('.')) {
                    continue;
                }
                let next_is_space = iter.peek().map(|(_, c)| c.is_whitespace()).unwrap_or(true);
                if next_is_space {
                    let end = idx + ch.len_utf8();
                    if end > start {
                        let sentence = &text[start..end];
                        if sentence.chars().any(|c| c.is_alphanumeric()) {
                            start = end;
                            return Some(sentence);
                        }
                        start = end;
                    }
                }
            }
        }
        let sentence = &text[start..];
        start = text.len();
        if sentence.chars().any(|c| c.is_alphanumeric()) {
            Some(sentence)
        } else {
            None
        }
    })
}

fn split_long_text_by_whitespace(text: &str, max_bytes: usize) -> VecDeque<String> {
    let mut out = VecDeque::new();
    let mut current = String::new();

    for word in text.split_whitespace() {
        if current.is_empty() {
            if word.len() > max_bytes {
                out.push_back(word.to_string());
            } else {
                current.push_str(word);
            }
            continue;
        }

        if current.len() + 1 + word.len() <= max_bytes {
            current.push(' ');
            current.push_str(word);
        } else {
            out.push_back(std::mem::take(&mut current));
            if word.len() > max_bytes {
                out.push_back(word.to_string());
            } else {
                current.push_str(word);
            }
        }
    }

    if !current.is_empty() {
        out.push_back(current);
    }

    out
}

fn split_text_lazy_with_limit<'a>(
    text: &'a str,
    max_bytes: usize,
) -> impl Iterator<Item = String> + 'a {
    split_text_lazy_with_limits(text, max_bytes, max_bytes)
}

fn split_text_lazy_with_limits<'a>(
    text: &'a str,
    first_max_bytes: usize,
    max_bytes: usize,
) -> impl Iterator<Item = String> + 'a {
    let normalized = normalize_for_tts(text);
    let mut sentences: VecDeque<String> = split_sentences_lazy(&normalized)
        .map(str::trim)
        .filter(|sentence| !sentence.is_empty())
        .map(ToString::to_string)
        .collect();
    let mut current = String::new();
    let mut pending = VecDeque::<String>::new();
    let mut emitted = false;

    std::iter::from_fn(move || {
        if let Some(part) = pending.pop_front() {
            emitted = true;
            return Some(part);
        }

        while let Some(sentence) = sentences.pop_front() {
            let active_max_bytes = if emitted { max_bytes } else { first_max_bytes };
            if sentence.len() > active_max_bytes {
                if !current.is_empty() {
                    let res = std::mem::take(&mut current);
                    pending.extend(split_long_text_by_whitespace(&sentence, max_bytes));
                    emitted = true;
                    return Some(res);
                }

                pending.extend(split_long_text_by_whitespace(&sentence, max_bytes));
                if let Some(part) = pending.pop_front() {
                    emitted = true;
                    return Some(part);
                }
                continue;
            }

            if current.is_empty() {
                current.push_str(&sentence);
            } else if current.len() + 1 + sentence.len() > active_max_bytes {
                let res = std::mem::take(&mut current);
                current.push_str(&sentence);
                emitted = true;
                return Some(res);
            } else {
                current.push(' ');
                current.push_str(&sentence);
            }
        }

        if !current.is_empty() {
            emitted = true;
            return Some(std::mem::take(&mut current));
        }
        None
    })
}

pub fn split_text_lazy<'a>(text: &'a str) -> impl Iterator<Item = String> + 'a {
    split_text_lazy_with_limit(text, EDGE_TTS_MAX_BYTES)
}

pub fn split_text_realtime_lazy<'a>(text: &'a str) -> impl Iterator<Item = String> + 'a {
    split_edge_text_with_limits(
        text,
        EDGE_TTS_REALTIME_FIRST_CHUNK_MAX_BYTES,
        EDGE_TTS_REALTIME_MAX_BYTES,
    )
    .into_iter()
}

fn escaped_char_bytes(ch: char) -> usize {
    match ch {
        '&' => 5,
        '<' | '>' => 4,
        '\'' | '"' => 6,
        _ => ch.len_utf8(),
    }
}

fn edge_wire_text_len(text: &str) -> usize {
    text.chars().map(escaped_char_bytes).sum()
}

fn edge_safe_split_index(text: &str, max_bytes: usize) -> usize {
    let mut bytes = 0usize;
    let mut end = 0usize;
    let mut space = 0usize;
    for (idx, ch) in text.char_indices() {
        let next = bytes.saturating_add(escaped_char_bytes(ch));
        if next > max_bytes {
            break;
        }
        bytes = next;
        end = idx + ch.len_utf8();
        if ch.is_whitespace() {
            space = end;
        }
    }
    if end == text.len() {
        end
    } else if space > 0 {
        space
    } else if end > 0 {
        end
    } else {
        // Make progress even for an artificially tiny limit below one scalar.
        text.chars().next().map_or(0, char::len_utf8)
    }
}

/// Bound the text after XML escaping, preserving UTF-8 scalars and paragraph
/// boundaries. A long word may require a hard split; it must never bypass the
/// Edge byte limit or leave an infinite loop on a non-ASCII character.
pub fn split_text_edge_with_limit(text: &str, max_bytes: usize) -> Vec<String> {
    split_edge_text_with_limits(text, max_bytes, max_bytes)
}

fn split_edge_text_with_limits(
    text: &str,
    first_max_bytes: usize,
    max_bytes: usize,
) -> Vec<String> {
    let normalized = normalize_for_tts(text);
    let mut out = Vec::new();
    let mut current = String::new();
    for paragraph in normalized.split("\n\n") {
        for sentence in split_sentences_lazy(paragraph) {
            let mut remaining = sentence.trim();
            // Count a long sentence once. Recounting its whole tail after every
            // hard split makes a very long word unnecessarily quadratic.
            let mut remaining_bytes = edge_wire_text_len(remaining);
            while !remaining.is_empty() {
                let limit = if out.is_empty() {
                    first_max_bytes
                } else {
                    max_bytes
                };
                let added_bytes = remaining_bytes;
                if !current.is_empty()
                    && edge_wire_text_len(&current)
                        .saturating_add(1)
                        .saturating_add(added_bytes)
                        > limit
                {
                    out.push(std::mem::take(&mut current));
                    continue;
                }
                if added_bytes <= limit {
                    if !current.is_empty() {
                        current.push(' ');
                    }
                    current.push_str(remaining);
                    break;
                }
                let split = edge_safe_split_index(remaining, limit);
                if split == 0 {
                    break;
                }
                let part = remaining[..split].trim();
                if part.chars().any(char::is_alphanumeric) {
                    out.push(part.to_string());
                }
                let tail = remaining[split..].trim_start();
                let consumed = remaining.len() - tail.len();
                remaining_bytes =
                    remaining_bytes.saturating_sub(edge_wire_text_len(&remaining[..consumed]));
                remaining = tail;
            }
        }
        if !current.is_empty() {
            out.push(std::mem::take(&mut current));
        }
    }
    out
}

pub fn normalize_edge_audiobook_text(text: &str) -> String {
    let normalized = normalize_for_tts(text);
    let strict: String = normalized
        .chars()
        .map(|ch| {
            let safe_punctuation = matches!(
                ch,
                '.' | ','
                    | ';'
                    | ':'
                    | '!'
                    | '?'
                    | '"'
                    | '\''
                    | '-'
                    | '('
                    | ')'
                    | '['
                    | ']'
                    | '{'
                    | '}'
                    | '/'
                    | '\\'
                    | '@'
                    | '#'
                    | '%'
                    | '&'
                    | '+'
                    | '='
                    | '*'
                    | '_'
                    | '<'
                    | '>'
                    | '|'
                    | '~'
            );
            if ch.is_alphanumeric() || ch.is_whitespace() || safe_punctuation {
                ch
            } else {
                ' '
            }
        })
        .collect();
    normalize_for_tts(&strict)
}

pub fn split_text_edge_audiobook(text: &str, voice: &str) -> Vec<String> {
    let limit = if voice.to_ascii_lowercase().starts_with("lt-") {
        900
    } else {
        EDGE_TTS_MAX_BYTES
    };
    split_text_edge_with_limit(&normalize_edge_audiobook_text(text), limit)
}

pub fn merge_short_tts_chunks(
    chunks: impl IntoIterator<Item = String>,
    min_chars: usize,
    max_bytes: usize,
) -> Vec<String> {
    let mut merged: Vec<String> = Vec::new();

    for chunk in chunks {
        let chunk = chunk.trim().to_string();
        if chunk.is_empty() {
            continue;
        }

        if let Some(last) = merged.last_mut()
            && last.chars().count() < min_chars
            && last.len() + 1 + chunk.len() <= max_bytes
        {
            last.push(' ');
            last.push_str(&chunk);
            continue;
        }

        merged.push(chunk);
    }

    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize};
    use tokio::net::TcpListener;
    use tokio_tungstenite::{accept_async, accept_hdr_async};

    fn no_delay(_: &anyhow::Error, _: usize) -> Duration {
        Duration::ZERO
    }

    fn audio_frame(payload: &[u8]) -> Message {
        let header = b"X-RequestId:test\r\nContent-Type:audio/mpeg\r\nPath:audio\r\n";
        let mut data = Vec::from((header.len() as u16).to_be_bytes());
        data.extend_from_slice(header);
        data.extend_from_slice(payload);
        Message::Binary(data.into())
    }

    async fn local_session(address: std::net::SocketAddr) -> EdgeRealtimeSession {
        let (socket, _) = connect_async(format!("ws://{address}/")).await.unwrap();
        let (write, read) = socket.split();
        EdgeRealtimeSession {
            write,
            read,
            healthy: true,
            fresh_for_each_chunk: false,
            endpoint: EdgeEndpoint::Legacy,
        }
    }

    #[test]
    fn separators_are_skipped_but_unicode_speech_is_kept() {
        for text in ["", "  ", "|", "...?!", "— | •", "😀", "&nbsp;"] {
            assert!(!is_edge_text_usable(text), "{text}");
        }
        for text in ["ciao.", "8:26", "è", "中文", "العربية", "Русский", "１２"]
        {
            assert!(is_edge_text_usable(text), "{text}");
        }
    }

    #[test]
    fn transient_classification_preserves_permanent_errors() {
        for message in [
            "HTTP error: 403 Forbidden",
            "WebSocketException: HTTP status code: 403",
            "os error 10054",
            "Edge TTS empty audio after turn.end",
            "Edge TTS audio too small after turn.end: 400 bytes",
            "connection closed without turn.end; truncated audio",
            "invalid audio: decode failed",
            "503 Service Unavailable",
        ] {
            assert!(
                is_transient_edge_error(&anyhow!(message.to_string())),
                "{message}"
            );
        }
        for message in [
            "voce non selezionata",
            "AUDIO_DESCRIPTION_CANCELLED",
            "Edge WebSocket error: 401 Unauthorized",
            "Edge WebSocket error: 400 Bad Request",
            "invalid certificate",
        ] {
            assert!(
                !is_transient_edge_error(&anyhow!(message.to_string())),
                "{message}"
            );
        }
        for permanent in [
            "invalid certificate",
            "unknown issuer",
            "Unknown voice name",
        ] {
            let nested = anyhow!(std::io::Error::other(permanent))
                .context("Edge WebSocket connection failed");
            assert!(!is_transient_edge_error(&nested), "{nested:#}");
        }
        for status in [400, 401, 404] {
            let error = anyhow!(EdgeHttpFailure {
                status,
                retry_after: None,
                endpoint: EdgeEndpoint::Legacy,
            });
            assert!(!is_transient_edge_error(&error));
        }
        for status in [403, 408, 429, 500, 502, 503, 504] {
            let error = anyhow!(EdgeHttpFailure {
                status,
                retry_after: None,
                endpoint: EdgeEndpoint::Legacy,
            });
            assert!(is_transient_edge_error(&error));
        }
    }

    #[test]
    fn retry_backoff_matches_mobile_and_respects_retry_after() {
        let fast = anyhow!("403 Forbidden");
        assert_eq!(edge_retry_delay(&fast, 1), Duration::from_millis(250));
        assert_eq!(edge_retry_delay(&fast, 2), Duration::from_millis(500));
        assert_eq!(edge_retry_delay(&fast, usize::MAX), Duration::from_secs(2));
        assert_eq!(
            bounded_edge_retry_delay(&fast, 1),
            Duration::from_millis(400)
        );
        assert_eq!(
            bounded_edge_retry_delay(&fast, 2),
            Duration::from_millis(800)
        );
        assert_eq!(
            bounded_edge_retry_delay(&fast, usize::MAX),
            Duration::from_secs(2)
        );
        let normal = anyhow!("empty audio");
        assert_eq!(edge_retry_delay(&normal, 1), Duration::from_millis(400));
        assert_eq!(
            edge_retry_delay(&normal, usize::MAX),
            Duration::from_secs(2)
        );
        let server = anyhow!(EdgeHttpFailure {
            status: 429,
            retry_after: Some(Duration::from_secs(30)),
            endpoint: EdgeEndpoint::Modern,
        });
        assert_eq!(edge_retry_delay(&server, 1), Duration::from_secs(30));
        let limited = anyhow!(EdgeHttpFailure {
            status: 429,
            retry_after: None,
            endpoint: EdgeEndpoint::Modern,
        });
        assert_eq!(edge_retry_delay(&limited, 1), Duration::from_secs(5));
        assert_eq!(edge_retry_delay(&limited, 2), Duration::from_secs(10));
        assert_eq!(edge_retry_delay(&limited, 13), Duration::from_secs(60));
        assert_eq!(bounded_edge_retry_delay(&limited, 2), Duration::from_secs(10));
    }

    #[test]
    fn protocol_clock_uses_utc_windows_epoch_and_five_minute_windows() {
        assert_eq!(
            generate_sec_ms_gec_at(0),
            "7ECB79D14E3AA576D2D79E6D487A1388156D91E614B1BE11C64226A29BC8DD8C"
        );
        assert_eq!(generate_sec_ms_gec_at(0), generate_sec_ms_gec_at(299));
        assert_ne!(generate_sec_ms_gec_at(299), generate_sec_ms_gec_at(300));
        let date = "Mon, 12 Oct 2026 09:00:00 GMT";
        let now = parse_http_date(date).unwrap();
        assert_eq!(parse_retry_after("30", now), Some(Duration::from_secs(30)));
        assert_eq!(
            parse_retry_after(date, now - 20),
            Some(Duration::from_secs(20))
        );
        assert_eq!(parse_retry_after(date, now + 20), Some(Duration::ZERO));
        assert_eq!(parse_retry_after("invalid", now), None);
    }

    #[test]
    fn read_aloud_endpoints_keep_distinct_authentication_and_fallback() {
        let modern = edge_socket_request(EdgeEndpoint::Modern, "TESTCONNECTION").unwrap();
        let legacy = edge_socket_request(EdgeEndpoint::Legacy, "TESTCONNECTION").unwrap();
        let modern_uri = modern.uri().to_string();
        let legacy_uri = legacy.uri().to_string();
        assert!(modern_uri.starts_with(EDGE_TTS_MODERN_WSS_URL));
        assert!(modern_uri.contains("Ocp-Apim-Subscription-Key="));
        assert!(modern_uri.contains("Sec-MS-GEC-Version=1-155.0.4283.45"));
        assert!(!modern_uri.contains("TrustedClientToken="));
        assert!(legacy_uri.starts_with(WSS_URL_BASE));
        assert!(legacy_uri.contains("TrustedClientToken="));
        assert!(legacy_uri.contains("Sec-MS-GEC-Version=1-132.0.2917.39"));
        assert!(!legacy_uri.contains("Ocp-Apim-Subscription-Key="));
        assert!(modern.headers().get("User-Agent").unwrap().to_str().unwrap().contains("155"));
        assert!(legacy.headers().get("User-Agent").unwrap().to_str().unwrap().contains("132"));
    }

    #[test]
    fn handshake_error_keeps_status_without_exposing_response_secrets() {
        let response = tungstenite::http::Response::builder()
            .status(403)
            .header("Retry-After", "10")
            .header("Set-Cookie", "secret-cookie")
            .body(Some(b"secret-response-body".to_vec()))
            .unwrap();
        let error = map_connect_error(tungstenite::Error::Http(Box::new(response)));
        let message = error.to_string();
        assert!(message.contains("403 Forbidden"));
        assert!(message.contains(WSS_URL_BASE));
        assert!(!message.contains("secret"));
        assert!(!message.contains(TRUSTED_CLIENT_TOKEN));
        assert!(!message.contains("ConnectionId"));
        assert_eq!(edge_retry_delay(&error, 1), Duration::from_secs(10));
    }

    #[test]
    fn binary_audio_parser_requires_valid_headers_and_exact_path() {
        let Message::Binary(frame) = audio_frame(b"sample") else {
            unreachable!()
        };
        assert_eq!(
            parse_edge_binary_audio_payload(&frame).unwrap(),
            Some(b"sample".to_vec())
        );
        assert!(parse_edge_binary_audio_payload(&[0]).is_err());
        assert!(parse_edge_binary_audio_payload(&[0, 40, 1]).is_err());
        assert!(parse_edge_binary_audio_payload(&[0, 0]).is_err());
        let header = b"pAtH:AuDiO\r\ncOnTeNt-TyPe:AuDiO/MpEg\r\n";
        let mut frame = Vec::from((header.len() as u16).to_be_bytes());
        frame.extend_from_slice(header);
        frame.extend_from_slice(b"ok");
        assert_eq!(
            parse_edge_binary_audio_payload(&frame).unwrap(),
            Some(b"ok".to_vec())
        );
        assert_eq!(
            edge_message_path("Path:audio.metadata\r\n\r\nPath:turn.end"),
            Some("audio.metadata")
        );
        assert_eq!(
            edge_message_path("Path:response\r\n\r\nPath:turn.end"),
            Some("response")
        );
    }

    #[test]
    fn binary_audio_parser_accepts_end_marker_and_both_byte_orders() {
        fn frame(header: &[u8], payload: &[u8], little_endian: bool) -> Vec<u8> {
            let length = header.len() as u16;
            let mut frame = Vec::from(if little_endian {
                length.to_le_bytes()
            } else {
                length.to_be_bytes()
            });
            frame.extend_from_slice(header);
            frame.extend_from_slice(payload);
            frame
        }
        for little_endian in [false, true] {
            let valid = frame(
                b"Path:audio\r\nContent-Type:audio/mpeg\r\n",
                b"sample",
                little_endian,
            );
            assert_eq!(
                parse_edge_binary_audio_payload(&valid).unwrap(),
                Some(b"sample".to_vec())
            );
            let end_marker = frame(b"Path:audio\r\n", b"", little_endian);
            assert_eq!(parse_edge_binary_audio_payload(&end_marker).unwrap(), None);
        }
        for (header, payload) in [
            (
                b"Path:audio\r\nContent-Type:audio/mpeg\r\n".as_slice(),
                b"".as_slice(),
            ),
            (
                b"Path:audio\r\nContent-Type:text/html\r\n".as_slice(),
                b"server error".as_slice(),
            ),
            (
                b"Path:response\r\nContent-Type:audio/mpeg\r\n".as_slice(),
                b"sample".as_slice(),
            ),
            (b"Path:audio\r\n".as_slice(), b"sample".as_slice()),
            (b"Path:audio\r\n\xff".as_slice(), b"sample".as_slice()),
        ] {
            let error =
                parse_edge_binary_audio_payload(&frame(header, payload, false)).unwrap_err();
            assert!(is_transient_edge_error(&error), "{error}");
        }
    }

    #[test]
    fn edge_splits_bound_escaped_xml_and_preserve_unicode_and_word_order() {
        let texts = [
            "漢字".repeat(1500),
            "ąžuolas&<>\"'中文🙂".repeat(250),
            "L'acqua & il pane <3. Ancora: العربية Русский 日本語. ".repeat(120),
            "ParolaSenzaSpazi".repeat(400),
        ];
        for text in texts {
            let normalized = normalize_for_tts(&text);
            let expected: String = normalized
                .chars()
                .filter(|ch| !ch.is_whitespace())
                .collect();
            for limit in [280, 320, 900, 1800] {
                let chunks = split_text_edge_with_limit(&text, limit);
                assert!(!chunks.is_empty());
                for chunk in &chunks {
                    assert!(
                        escape_xml(chunk).len() <= limit,
                        "limit={limit} bytes={}",
                        escape_xml(chunk).len()
                    );
                    assert!(is_edge_text_usable(chunk));
                }
                let actual: String = chunks
                    .join("")
                    .chars()
                    .filter(|ch| !ch.is_whitespace())
                    .collect();
                assert_eq!(actual, expected, "limit={limit}");
            }
        }
    }

    #[test]
    fn realtime_first_chunk_is_small_and_paragraph_boundaries_survive() {
        let text = "a".repeat(3200);
        let chunks: Vec<_> = split_text_realtime_lazy(&text).collect();
        assert_eq!(chunks[0].len(), EDGE_TTS_REALTIME_FIRST_CHUNK_MAX_BYTES);
        assert!(
            chunks
                .iter()
                .skip(1)
                .all(|chunk| escape_xml(chunk).len() <= EDGE_TTS_REALTIME_MAX_BYTES)
        );
        assert_eq!(chunks.join(""), text);
        let paragraphs: Vec<_> =
            split_text_realtime_lazy("Prima frase.\n\nSeconda frase.\n\n..?!\n\nTerza.").collect();
        assert_eq!(paragraphs, ["Prima frase.", "Seconda frase.", "Terza."]);
    }

    #[test]
    fn audiobook_strict_normalization_and_lithuanian_limits_are_conditional() {
        let chunks = split_text_edge_audiobook(
            "  Prima😀\u{200b} frase.\n\n… | 😀\n\n中文 ≤ 3. ",
            "it-IT-ElsaNeural",
        );
        assert_eq!(chunks, ["Prima frase.", "中文 3."]);
        assert!(split_text_edge_audiobook("| … 😀 ", "it-IT-ElsaNeural").is_empty());
        let text = "ą".repeat(1200);
        let lithuanian = split_text_edge_audiobook(&text, "LT-lt-OnaNeural");
        assert!(
            lithuanian
                .iter()
                .all(|chunk| escape_xml(chunk).len() <= 900)
        );
        let italian = split_text_edge_audiobook(&text, "it-IT-ElsaNeural");
        assert!(italian.iter().any(|chunk| escape_xml(chunk).len() > 900));
    }

    #[tokio::test]
    async fn empty_input_never_opens_a_realtime_connection() {
        let connections = AtomicUsize::new(0);
        let result = synthesize_realtime_with_connector(
            None,
            "| … 😀 ",
            "it-IT-ElsaNeural",
            0,
            0,
            100,
            None,
            || true,
            || {
                connections.fetch_add(1, Ordering::SeqCst);
                std::future::ready(Err(anyhow!("connector must not run")))
            },
            no_delay,
        )
        .await
        .unwrap()
        .unwrap();
        assert!(result.0.is_empty());
        assert!(result.1.is_none());
        assert_eq!(connections.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    #[allow(clippy::result_large_err)]
    async fn reading_keeps_fresh_sessions_after_403_and_preserves_chunk_order() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let rejected = accept_hdr_async(
                stream,
                |_: &tungstenite::handshake::server::Request,
                 _: tungstenite::handshake::server::Response| {
                    Err(tungstenite::http::Response::builder()
                        .status(403)
                        .body(Some("Forbidden".to_string()))
                        .unwrap())
                },
            )
            .await;
            assert!(rejected.is_err());
            for (index, text) in ["prima", "seconda", "terza"].into_iter().enumerate() {
                let (stream, _) = listener.accept().await.unwrap();
                let mut socket = accept_async(stream).await.unwrap();
                let request = socket.next().await.unwrap().unwrap().into_text().unwrap();
                assert!(request.contains(text));
                socket
                    .send(audio_frame(&vec![index as u8 + 1; 1200]))
                    .await
                    .unwrap();
                socket
                    .send(Message::Text("Path:turn.end\r\n\r\n".into()))
                    .await
                    .unwrap();
            }
        });
        let connections = AtomicUsize::new(0);
        let mut session = None;
        let mut output = Vec::new();
        for (index, text) in ["prima", "seconda", "terza"].into_iter().enumerate() {
            let (audio, mut next_session) = synthesize_realtime_with_connector(
                session,
                text,
                "it-IT-ElsaNeural",
                0,
                0,
                100,
                None,
                || true,
                || {
                    connections.fetch_add(1, Ordering::SeqCst);
                    async {
                        let (socket, _) = connect_async(format!("ws://{address}/"))
                            .await
                            .map_err(map_connect_error)?;
                        let (write, read) = socket.split();
                        Ok(EdgeRealtimeSession {
                            write,
                            read,
                            healthy: true,
                            fresh_for_each_chunk: false,
                            endpoint: EdgeEndpoint::Legacy,
                        })
                    }
                },
                no_delay,
            )
            .await
            .unwrap()
            .unwrap();
            assert!(next_session.as_ref().unwrap().fresh_for_each_chunk);
            // A decoder rejection must invalidate the socket without forgetting
            // the policy selected earlier by the 403 recovery.
            if index == 1 {
                next_session.as_mut().unwrap().invalidate();
            }
            assert_eq!(audio, vec![index as u8 + 1; 1200]);
            output.extend(audio);
            session = next_session;
        }
        drop(session);
        server.await.unwrap();
        assert_eq!(connections.load(Ordering::SeqCst), 4);
        assert_eq!(
            output,
            [vec![1; 1200], vec![2; 1200], vec![3; 1200]].concat()
        );
    }

    #[tokio::test]
    async fn reading_reuses_healthy_sessions_without_a_403() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = accept_async(stream).await.unwrap();
            for text in ["prima", "seconda", "terza"] {
                let request = socket.next().await.unwrap().unwrap().into_text().unwrap();
                assert!(request.contains(text));
                socket.send(audio_frame(&vec![7; 1200])).await.unwrap();
                socket
                    .send(Message::Text("Path:turn.end\r\n\r\n".into()))
                    .await
                    .unwrap();
            }
        });
        let connections = AtomicUsize::new(0);
        let mut session = None;
        for text in ["prima", "seconda", "terza"] {
            let (_, next_session) = synthesize_realtime_with_connector(
                session,
                text,
                "it-IT-ElsaNeural",
                0,
                0,
                100,
                None,
                || true,
                || {
                    connections.fetch_add(1, Ordering::SeqCst);
                    async { Ok(local_session(address).await) }
                },
                no_delay,
            )
            .await
            .unwrap()
            .unwrap();
            session = next_session;
        }
        drop(session);
        server.await.unwrap();
        assert_eq!(connections.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn persistent_retry_survives_fifty_failures_without_replaying_previous_chunks() {
        let attempts = AtomicUsize::new(0);
        let requests = std::sync::Mutex::new(Vec::new());
        let mut queue = Vec::new();
        for chunk in [1, 2, 3] {
            let result = retry_edge_operation(
                || {
                    requests.lock().unwrap().push(chunk);
                    let fail = chunk == 2 && attempts.fetch_add(1, Ordering::SeqCst) < 50;
                    async move {
                        if fail {
                            Err(anyhow!("HTTP error: 403 Forbidden"))
                        } else {
                            Ok(chunk)
                        }
                    }
                },
                None,
                &|| true,
                "test",
                no_delay,
            )
            .await
            .unwrap();
            queue.push(result.unwrap());
        }
        assert_eq!(queue, [1, 2, 3]);
        let requested = requests.lock().unwrap();
        assert_eq!(requested.iter().filter(|&&chunk| chunk == 1).count(), 1);
        assert_eq!(requested.iter().filter(|&&chunk| chunk == 2).count(), 51);
        assert_eq!(requested.iter().filter(|&&chunk| chunk == 3).count(), 1);
    }

    #[tokio::test]
    async fn bounded_retries_stop_and_permanent_errors_fail_immediately() {
        for (message, budget, expected) in [
            ("403 Forbidden", Some(6), 6),
            ("403 Forbidden", Some(0), 1),
            ("voce non selezionata", None, 1),
            ("AUDIO_DESCRIPTION_CANCELLED", None, 1),
        ] {
            let attempts = AtomicUsize::new(0);
            let result = retry_edge_operation(
                || {
                    attempts.fetch_add(1, Ordering::SeqCst);
                    async { Err::<(), _>(anyhow!(message.to_string())) }
                },
                budget,
                &|| true,
                "test",
                no_delay,
            )
            .await;
            assert!(result.is_err());
            assert_eq!(attempts.load(Ordering::SeqCst), expected);
        }
    }

    #[tokio::test]
    async fn cancellation_discards_late_result_and_prevents_further_attempts() {
        let active = AtomicBool::new(true);
        let result = retry_edge_operation(
            || async {
                active.store(false, Ordering::SeqCst);
                Ok(42)
            },
            None,
            &|| active.load(Ordering::SeqCst),
            "test",
            no_delay,
        )
        .await
        .unwrap();
        assert_eq!(result, None);
        let attempts = AtomicUsize::new(0);
        let result = retry_edge_operation(
            || {
                attempts.fetch_add(1, Ordering::SeqCst);
                async { Ok::<(), anyhow::Error>(()) }
            },
            None,
            &|| false,
            "test",
            no_delay,
        )
        .await
        .unwrap();
        assert_eq!(result, None);
        assert_eq!(attempts.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn stop_interrupts_retry_after_instead_of_waiting_for_server_delay() {
        let active = Arc::new(AtomicBool::new(true));
        let cancel = active.clone();
        let task = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            cancel.store(false, Ordering::SeqCst);
        });
        let attempts = AtomicUsize::new(0);
        let started = Instant::now();
        let result = retry_edge_operation(
            || {
                attempts.fetch_add(1, Ordering::SeqCst);
                async {
                    Err::<(), _>(anyhow!(EdgeHttpFailure {
                        status: 429,
                        retry_after: Some(Duration::from_secs(60)),
                        endpoint: EdgeEndpoint::Modern,
                    }))
                }
            },
            None,
            &|| active.load(Ordering::SeqCst),
            "test",
            edge_retry_delay,
        )
        .await
        .unwrap();
        task.await.unwrap();
        assert_eq!(result, None);
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[tokio::test]
    #[allow(clippy::result_large_err)] // tungstenite's handshake callback requires ErrorResponse.
    async fn local_http_403_reconnects_and_completes_the_same_chunk() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let rejected = accept_hdr_async(
                stream,
                |_: &tungstenite::handshake::server::Request,
                 _: tungstenite::handshake::server::Response| {
                    Err(tungstenite::http::Response::builder()
                        .status(403)
                        .header("Retry-After", "0")
                        .body(Some("Forbidden".to_string()))
                        .unwrap())
                },
            )
            .await;
            assert!(rejected.is_err());

            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = accept_async(stream).await.unwrap();
            let request = socket.next().await.unwrap().unwrap().into_text().unwrap();
            assert!(request.contains("frase recuperata"));
            socket.send(audio_frame(&vec![7; 1200])).await.unwrap();
            socket
                .send(Message::Text("Path:turn.end\r\n\r\n".into()))
                .await
                .unwrap();
        });
        let attempts = AtomicUsize::new(0);
        let audio = retry_edge_operation(
            || {
                attempts.fetch_add(1, Ordering::SeqCst);
                async {
                    let (socket, _) = connect_async(format!("ws://{address}/"))
                        .await
                        .map_err(map_connect_error)?;
                    let (write, read) = socket.split();
                    let mut session = EdgeRealtimeSession {
                        write,
                        read,
                        healthy: true,
                        fresh_for_each_chunk: false,
                        endpoint: EdgeEndpoint::Legacy,
                    };
                    session
                        .synthesize_chunk("frase recuperata", "it-IT-ElsaNeural", 0, 0, 100)
                        .await
                }
            },
            Some(2),
            &|| true,
            "test",
            no_delay,
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
        assert_eq!(audio, vec![7; 1200]);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn local_socket_retries_truncated_turn_without_leaking_partial_audio() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            for attempt in 0..2 {
                let (stream, _) = listener.accept().await.unwrap();
                let mut socket = accept_async(stream).await.unwrap();
                let request = socket.next().await.unwrap().unwrap();
                assert!(request.into_text().unwrap().contains("stessa frase"));
                socket
                    .send(audio_frame(&vec![attempt as u8; 1200]))
                    .await
                    .unwrap();
                if attempt == 0 {
                    socket.close(None).await.unwrap();
                } else {
                    socket
                        .send(Message::Text("Path:turn.end\r\n\r\n".into()))
                        .await
                        .unwrap();
                }
            }
        });
        let result = retry_edge_operation(
            || async {
                let mut session = local_session(address).await;
                session
                    .synthesize_chunk("stessa frase", "it-IT-ElsaNeural", 0, 0, 100)
                    .await
            },
            Some(2),
            &|| true,
            "test",
            no_delay,
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(result, vec![1; 1200]);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn local_socket_requires_audio_even_with_successful_turn_end() {
        for payload in [vec![], vec![1; 999]] {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let server = tokio::spawn(async move {
                let (stream, _) = listener.accept().await.unwrap();
                let mut socket = accept_async(stream).await.unwrap();
                socket.next().await.unwrap().unwrap();
                socket.send(audio_frame(&payload)).await.unwrap();
                socket
                    .send(Message::Text("Path:turn.end\r\n\r\n".into()))
                    .await
                    .unwrap();
            });
            let mut session = local_session(address).await;
            let error = session
                .synthesize_chunk("ciao", "it-IT-ElsaNeural", 0, 0, 100)
                .await
                .unwrap_err();
            assert!(is_transient_edge_error(&error));
            assert!(!session.healthy);
            server.await.unwrap();
        }
    }

    #[tokio::test]
    async fn local_socket_read_timeout_rejects_incomplete_turn() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = accept_async(stream).await.unwrap();
            socket.send(audio_frame(&vec![1; 1200])).await.unwrap();
            let _ = socket.next().await;
        });
        let mut session = local_session(address).await;
        let result = receive_complete_audio(
            &mut session.write,
            &mut session.read,
            Duration::from_millis(20),
            Duration::from_secs(1),
        )
        .await;
        assert!(result.unwrap_err().to_string().contains("timeout"));
        drop(session);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn stop_drops_in_flight_socket_without_waiting_for_network_timeout() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let active = Arc::new(AtomicBool::new(true));
        let cancel = active.clone();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = accept_async(stream).await.unwrap();
            socket.next().await.unwrap().unwrap();
            cancel.store(false, Ordering::SeqCst);
            let close = tokio::time::timeout(Duration::from_secs(1), socket.next()).await;
            assert!(close.is_ok(), "cancel must close the underlying socket");
        });
        let started = Instant::now();
        let result = retry_edge_operation(
            || async {
                let mut session = local_session(address).await;
                session
                    .synthesize_chunk("ciao", "it-IT-ElsaNeural", 0, 0, 100)
                    .await
            },
            None,
            &|| active.load(Ordering::SeqCst),
            "test",
            no_delay,
        )
        .await
        .unwrap();
        assert_eq!(result, None);
        assert!(started.elapsed() < Duration::from_secs(1));
        server.await.unwrap();
    }

    #[test]
    fn merge_short_tts_chunks_combines_short_title_with_body() {
        let chunks = vec![
            "Titolo:".to_string(),
            "Questo e' il corpo dell'articolo con abbastanza testo da non lasciare il titolo isolato.".to_string(),
            "Secondo blocco.".to_string(),
        ];

        let merged = merge_short_tts_chunks(chunks, 120, 300);

        assert_eq!(merged.len(), 1);
        assert!(merged[0].starts_with("Titolo: Questo e' il corpo"));
        assert!(merged[0].contains("Secondo blocco."));
    }

    #[test]
    fn merge_short_tts_chunks_respects_max_bytes() {
        let chunks = vec!["Breve:".to_string(), "x".repeat(20)];

        assert_eq!(merge_short_tts_chunks(chunks, 120, 10).len(), 2);
    }
}

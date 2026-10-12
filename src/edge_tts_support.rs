//! Call-site safeguards for playable Edge audio and resumable audiobook jobs.

use crate::edge_tts::{self, EdgeRealtimeSession};
use anyhow::{Result, anyhow};
use rodio::Decoder;
use sha2::{Digest, Sha256};
use std::future::Future;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

const EDGE_PREVIEW_ATTEMPTS: usize = 6;

fn validate_audio(audio: &[u8]) -> Result<()> {
    validate_audio_cancellable(audio, &|| true).map(|_| ())
}

fn validate_audio_cancellable<F: Fn() -> bool>(audio: &[u8], is_active: &F) -> Result<bool> {
    if !is_active() {
        return Ok(false);
    }
    if audio.len() < 1000 {
        return Err(anyhow!("invalid audio: empty or undersized Edge output"));
    }
    let decoder = Decoder::new(Cursor::new(audio.to_vec()))
        .map_err(|error| anyhow!("invalid audio: decode failed: {error}"))?;
    // Decode the complete stream before it enters a playback queue or cache.
    // Opening a decoder and reading its first sample is not a complete check.
    let mut has_samples = false;
    for (index, _) in decoder.enumerate() {
        if index % 4096 == 0 && !is_active() {
            return Ok(false);
        }
        has_samples = true;
    }
    if !has_samples {
        return Err(anyhow!("invalid audio: no decoded samples"));
    }
    Ok(is_active())
}

async fn wait_while_active<F: Fn() -> bool>(delay: Duration, is_active: &F) -> bool {
    let mut remaining = delay;
    while !remaining.is_zero() {
        if !is_active() {
            return false;
        }
        let slice = remaining.min(Duration::from_millis(100));
        tokio::time::sleep(slice).await;
        remaining = remaining.saturating_sub(slice);
    }
    is_active()
}

async fn retry_playable_audio<G, Fut, F, D>(
    mut generate: G,
    is_active: &F,
    max_attempts: Option<usize>,
    retry_delay: D,
    label: &str,
) -> Result<Option<Vec<u8>>>
where
    G: FnMut() -> Fut,
    Fut: Future<Output = Result<Option<Vec<u8>>>>,
    F: Fn() -> bool,
    D: Fn(&anyhow::Error, usize) -> Duration,
{
    let mut attempt = 1usize;
    loop {
        if !is_active() {
            return Ok(None);
        }
        let result = match generate().await {
            Ok(None) => return Ok(None),
            Ok(Some(audio)) => match validate_audio_cancellable(&audio, is_active) {
                Ok(true) => Ok(audio),
                Ok(false) => return Ok(None),
                Err(error) => Err(error),
            },
            Err(error) => Err(error),
        };
        if !is_active() {
            return Ok(None);
        }
        let error = match result {
            Ok(audio) => return Ok(Some(audio)),
            Err(error) => error,
        };
        let exhausted = max_attempts.is_some_and(|limit| attempt >= limit.max(1));
        if !edge_tts::is_transient_edge_error(&error) || exhausted {
            return Err(error);
        }
        let delay = retry_delay(&error, attempt);
        let budget = max_attempts.map_or_else(
            || "until_cancelled".to_string(),
            |limit| limit.max(1).to_string(),
        );
        crate::append_podcast_log(&format!(
            "edge_tts.playable.retry label={label} attempt={attempt} max_attempts={budget} delay_ms={} error={error}",
            delay.as_millis()
        ));
        if !wait_while_active(delay, is_active).await {
            return Ok(None);
        }
        attempt = attempt.saturating_add(1);
    }
}

#[allow(clippy::too_many_arguments)]
async fn synthesize_playable_text<F: Fn() -> bool>(
    text: &str,
    voice: &str,
    rate: i32,
    pitch: i32,
    volume: i32,
    max_attempts: Option<usize>,
    label: &str,
    is_active: F,
) -> Result<Option<Vec<u8>>> {
    if !is_active() {
        return Ok(None);
    }
    if !edge_tts::is_edge_text_usable(text) {
        return Ok(Some(Vec::new()));
    }
    retry_playable_audio(
        || {
            // One transport attempt here: the enclosing loop owns the complete
            // budget, including empty or undecodable audio, without multiplying it.
            edge_tts::synthesize_text_with_retry_cancellable(
                text,
                voice,
                rate,
                pitch,
                volume,
                Some(1),
                &is_active,
            )
        },
        &is_active,
        max_attempts,
        |error, failed_attempt| {
            let delay = edge_tts::edge_retry_delay(error, failed_attempt);
            if max_attempts.is_some() {
                delay.max(Duration::from_millis(
                    (failed_attempt.min(5) as u64 * 400).min(2000),
                ))
            } else {
                delay
            }
        },
        label,
    )
    .await
}

pub async fn synthesize_preview(
    text: &str,
    voice: &str,
    rate: i32,
    pitch: i32,
    volume: i32,
) -> Result<Vec<u8>> {
    synthesize_preview_cancellable(text, voice, rate, pitch, volume, || true)
        .await?
        .ok_or_else(|| anyhow!("Edge voice preview cancelled"))
}

pub async fn synthesize_preview_cancellable<F: Fn() -> bool>(
    text: &str,
    voice: &str,
    rate: i32,
    pitch: i32,
    volume: i32,
    is_active: F,
) -> Result<Option<Vec<u8>>> {
    synthesize_playable_text(
        text,
        voice,
        rate,
        pitch,
        volume,
        Some(EDGE_PREVIEW_ATTEMPTS),
        "preview",
        is_active,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn synthesize_audiobook_chunk<F: Fn() -> bool>(
    text: &str,
    voice: &str,
    rate: i32,
    pitch: i32,
    volume: i32,
    chunk_index: usize,
    is_active: F,
) -> Result<Option<Vec<u8>>> {
    let label = format!("audiobook_chunk_{chunk_index}");
    synthesize_audiobook_parts(text, voice, &is_active, &mut |part: String| {
        let is_active = &is_active;
        let label = &label;
        async move {
            // The Windows export policy retries transient transport and
            // decoding failures until Stop; permanent errors remain fatal.
            synthesize_playable_text(&part, voice, rate, pitch, volume, None, label, is_active)
                .await
        }
    })
    .await
}

fn is_lithuanian_audio_suspicious(text: &str, audio_len: usize) -> bool {
    let text_len = text.encode_utf16().count();
    text_len >= 120 && audio_len < text_len.saturating_mul(120)
}

fn split_lithuanian_text_in_two(text: &str) -> Option<(String, String)> {
    if text.len() < 8 {
        return None;
    }
    let middle = text.len() / 2;
    let mut boundary = 0;
    let mut whitespace = 0;
    for (index, ch) in text
        .char_indices()
        .take_while(|(index, _)| *index <= middle)
    {
        boundary = index;
        if ch.is_whitespace() {
            whitespace = index;
        }
    }
    let boundary = if whitespace > 0 { whitespace } else { boundary };
    if boundary == 0 || boundary >= text.len() {
        return None;
    }
    let (left, right) = text.split_at(boundary);
    let (left, right) = (left.trim(), right.trim());
    if !edge_tts::is_edge_text_usable(left) || !edge_tts::is_edge_text_usable(right) {
        return None;
    }
    Some((left.to_string(), right.to_string()))
}

async fn adaptive_lithuanian_audio<G, Fut, F>(
    text: &str,
    depth: usize,
    generate: &mut G,
    is_active: &F,
) -> Result<Option<Vec<u8>>>
where
    G: FnMut(String) -> Fut,
    Fut: Future<Output = Result<Option<Vec<u8>>>>,
    F: Fn() -> bool,
{
    if !is_active() {
        return Ok(None);
    }
    let Some(audio) = generate(text.to_string()).await? else {
        return Ok(None);
    };
    if !is_active() {
        return Ok(None);
    }
    if !is_lithuanian_audio_suspicious(text, audio.len()) || depth >= 3 {
        return Ok(Some(audio));
    }
    let Some((left, right)) = split_lithuanian_text_in_two(text) else {
        return Ok(Some(audio));
    };
    crate::append_podcast_log(&format!(
        "edge_tts.audiobook.lt_subdivide depth={depth} utf16_len={} bytes={}",
        text.encode_utf16().count(),
        audio.len()
    ));
    // Publish only the completed replacement. The suspicious parent audio is
    // discarded, and successful left parts remain intact while the right retries.
    let Some(mut merged) = Box::pin(adaptive_lithuanian_audio(
        &left,
        depth + 1,
        generate,
        is_active,
    ))
    .await?
    else {
        return Ok(None);
    };
    let Some(right_audio) = Box::pin(adaptive_lithuanian_audio(
        &right,
        depth + 1,
        generate,
        is_active,
    ))
    .await?
    else {
        return Ok(None);
    };
    if !is_active() {
        return Ok(None);
    }
    merged.extend_from_slice(&right_audio);
    Ok(Some(merged))
}

async fn synthesize_audiobook_parts<G, Fut, F>(
    text: &str,
    voice: &str,
    is_active: &F,
    generate: &mut G,
) -> Result<Option<Vec<u8>>>
where
    G: FnMut(String) -> Fut,
    Fut: Future<Output = Result<Option<Vec<u8>>>>,
    F: Fn() -> bool,
{
    if !is_active() {
        return Ok(None);
    }
    let lithuanian = voice.to_ascii_lowercase().starts_with("lt-");
    let mut output = Vec::new();
    for part in edge_tts::split_text_edge_audiobook(text, voice) {
        if !is_active() {
            return Ok(None);
        }
        let result = if lithuanian {
            adaptive_lithuanian_audio(&part, 0, generate, is_active).await?
        } else {
            generate(part.clone()).await?
        };
        let Some(mut audio) = result else {
            return Ok(None);
        };
        if !is_active() {
            return Ok(None);
        }
        if lithuanian && is_lithuanian_audio_suspicious(&part, audio.len()) {
            let strict_parts = edge_tts::split_text_edge_with_limit(&part, 320);
            let mut strict_audio = Vec::new();
            for strict_part in strict_parts {
                if !is_active() {
                    return Ok(None);
                }
                let Some(generated) = generate(strict_part).await? else {
                    return Ok(None);
                };
                if !is_active() {
                    return Ok(None);
                }
                strict_audio.extend_from_slice(&generated);
            }
            if strict_audio.len() > audio.len() {
                audio = strict_audio;
            }
            crate::append_podcast_log(&format!(
                "edge_tts.audiobook.lt_strict utf16_len={} bytes={} still_suspicious={}",
                part.encode_utf16().count(),
                audio.len(),
                is_lithuanian_audio_suspicious(&part, audio.len())
            ));
        }
        output.extend_from_slice(&audio);
    }
    Ok(is_active().then_some(output))
}

#[allow(clippy::too_many_arguments)]
pub async fn synthesize_reading_chunk<F: Fn() -> bool>(
    mut session: Option<EdgeRealtimeSession>,
    text: &str,
    voice: &str,
    rate: i32,
    pitch: i32,
    volume: i32,
    chunk_index: usize,
    is_active: F,
) -> Result<Option<(Vec<u8>, Option<EdgeRealtimeSession>)>> {
    let mut decode_retries = 0usize;
    loop {
        let result = edge_tts::synthesize_realtime_chunk_with_retry_cancellable(
            session, text, voice, rate, pitch, volume, None, &is_active,
        )
        .await?;
        let Some((audio, next_session)) = result else {
            return Ok(None);
        };
        if !is_active() {
            return Ok(None);
        }
        if audio.is_empty() && !edge_tts::is_edge_text_usable(text) {
            return Ok(Some((audio, next_session)));
        }
        match validate_audio_cancellable(&audio, &is_active) {
            Ok(true) => return Ok(Some((audio, next_session))),
            Ok(false) => return Ok(None),
            Err(error) => {
                // The failed audio never reaches the playback queue. Reconnect
                // and regenerate just this chunk, preserving every earlier one.
                session = next_session;
                if let Some(session) = session.as_mut() {
                    session.invalidate();
                }
                decode_retries = decode_retries.saturating_add(1);
                let delay = edge_tts::edge_retry_delay(&error, decode_retries);
                crate::append_podcast_log(&format!(
                    "edge_tts.reading.decode_retry index={chunk_index} retry={decode_retries} delay_ms={} error={error}",
                    delay.as_millis()
                ));
                if !wait_while_active(delay, &is_active).await {
                    return Ok(None);
                }
            }
        }
    }
}

/// The key includes the exact chunk boundaries and all voice parameters. A
/// resumed export can reuse completed audio only when its synthesis is identical.
#[derive(Clone)]
pub struct AudiobookCache {
    directory: PathBuf,
}

impl AudiobookCache {
    pub fn open(
        root: &Path,
        chunks: &[String],
        voice: &str,
        rate: i32,
        pitch: i32,
        volume: i32,
    ) -> std::io::Result<Self> {
        let mut hash = Sha256::new();
        hash.update(b"sonarpad-edge-audiobook-v2\0");
        for part in std::iter::once(voice).chain(chunks.iter().map(String::as_str)) {
            hash.update((part.len() as u64).to_le_bytes());
            hash.update(part.as_bytes());
        }
        hash.update(rate.to_le_bytes());
        hash.update(pitch.to_le_bytes());
        hash.update(volume.to_le_bytes());
        let directory = root.join(hex::encode(hash.finalize()));
        std::fs::create_dir_all(&directory)?;
        cleanup_old_jobs(root, &directory);
        Ok(Self { directory })
    }

    pub fn load(&self, index: usize) -> Option<Vec<u8>> {
        let path = self.chunk_path(index);
        let audio = std::fs::read(&path).ok()?;
        if validate_audio(&audio).is_ok() {
            Some(audio)
        } else {
            crate::append_podcast_log(&format!(
                "audiobook_save.cache_invalid index={index} path={}",
                path.display()
            ));
            let _ = std::fs::remove_file(path);
            None
        }
    }

    pub fn store(&self, index: usize, audio: &[u8]) -> std::io::Result<()> {
        validate_audio(audio).map_err(|error| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, error.to_string())
        })?;
        let destination = self.chunk_path(index);
        let temporary = self
            .directory
            .join(format!("chunk-{index:05}-{}.partial", uuid::Uuid::new_v4()));
        let result = std::fs::write(&temporary, audio)
            .and_then(|()| std::fs::rename(&temporary, &destination));
        if result.is_err() {
            let _ = std::fs::remove_file(&temporary);
        }
        result
    }

    pub fn finish(&self) {
        if let Err(error) = std::fs::remove_dir_all(&self.directory)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            crate::append_podcast_log(&format!(
                "audiobook_save.cache_cleanup_failed error={error}"
            ));
        }
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    fn chunk_path(&self, index: usize) -> PathBuf {
        self.directory.join(format!("chunk-{index:05}.mp3"))
    }
}

fn cleanup_old_jobs(root: &Path, current: &Path) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        // This directory belongs exclusively to the cache. Still restrict
        // cleanup to our own hash-named directories and never follow symlinks.
        if name.len() != 64
            || !name.bytes().all(|byte| byte.is_ascii_hexdigit())
            || entry.path() == current
            || !entry.file_type().is_ok_and(|kind| kind.is_dir())
        {
            continue;
        }
        let expired = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|modified| SystemTime::now().duration_since(modified).ok())
            .is_some_and(|age| age >= Duration::from_secs(3 * 24 * 60 * 60));
        if expired {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

/// Publish a completed export only after conversion succeeds. A failed write or
/// cancellation removes its staging file and preserves an existing destination.
pub struct AudiobookOutput {
    staging: PathBuf,
}

impl AudiobookOutput {
    pub fn new(destination: &Path) -> Self {
        let extension = destination
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("mp3");
        let staging = destination.with_file_name(format!(
            "sonarpad-audiobook-{}.partial.{extension}",
            uuid::Uuid::new_v4()
        ));
        Self { staging }
    }

    pub fn path(&self) -> &Path {
        &self.staging
    }

    pub fn commit(self, destination: &Path) -> std::io::Result<()> {
        if std::fs::metadata(&self.staging)?.len() < 1024 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Audiobook output is empty or too small",
            ));
        }
        std::fs::rename(&self.staging, destination)
    }
}

impl Drop for AudiobookOutput {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.staging);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn playable_audio() -> Vec<u8> {
        // A synthetic 440 Hz tone, 24 kHz / 48 kbps MP3, exercises the actual
        // decoder without a live Edge service or an audio output device.
        include_bytes!("../tests/fixtures/edge_tts_sample.mp3").to_vec()
    }

    #[tokio::test]
    async fn bounded_retry_recovers_403_and_bad_audio_without_replaying_success() {
        let calls = Cell::new(0usize);
        let expected = playable_audio();
        let result = retry_playable_audio(
            || {
                calls.set(calls.get() + 1);
                std::future::ready(match calls.get() {
                    1 => Err(anyhow!("HTTP error: 403 Forbidden")),
                    2 => Ok(Some(vec![0; 1500])),
                    _ => Ok(Some(expected.clone())),
                })
            },
            &|| true,
            Some(6),
            |_, _| Duration::ZERO,
            "test",
        )
        .await
        .unwrap();
        assert_eq!(result, Some(expected));
        assert_eq!(calls.get(), 3);
    }

    #[tokio::test]
    async fn permanent_error_does_not_consume_the_retry_budget() {
        let calls = Cell::new(0usize);
        let error = retry_playable_audio(
            || {
                calls.set(calls.get() + 1);
                std::future::ready(Err(anyhow!("Unknown voice name")))
            },
            &|| true,
            Some(10),
            |_, _| Duration::ZERO,
            "test",
        )
        .await
        .unwrap_err();
        assert!(error.to_string().contains("Unknown voice"));
        assert_eq!(calls.get(), 1);
    }

    #[tokio::test]
    async fn cancelled_generation_discards_audio_and_prevents_a_new_attempt() {
        let active = Cell::new(true);
        let calls = Cell::new(0usize);
        let result = retry_playable_audio(
            || {
                calls.set(calls.get() + 1);
                active.set(false);
                std::future::ready(Ok(Some(playable_audio())))
            },
            &|| active.get(),
            Some(10),
            |_, _| Duration::ZERO,
            "test",
        )
        .await
        .unwrap();
        assert!(result.is_none());
        assert_eq!(calls.get(), 1);
    }

    #[tokio::test]
    async fn cancellation_interrupts_a_long_audiobook_retry_delay() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};
        let active = Arc::new(AtomicBool::new(true));
        let stop = Arc::clone(&active);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            stop.store(false, Ordering::Relaxed);
        });
        let calls = Cell::new(0usize);
        let started = std::time::Instant::now();
        let result = retry_playable_audio(
            || {
                calls.set(calls.get() + 1);
                std::future::ready(Err(anyhow!("HTTP error: 403 Forbidden")))
            },
            &|| active.load(Ordering::Relaxed),
            None,
            |_, _| Duration::from_secs(60),
            "test",
        )
        .await
        .unwrap();
        assert!(result.is_none());
        assert_eq!(calls.get(), 1);
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[tokio::test]
    async fn repeated_transient_errors_stop_at_the_bounded_attempt_limit() {
        let calls = Cell::new(0usize);
        let error = retry_playable_audio(
            || {
                calls.set(calls.get() + 1);
                std::future::ready(Err(anyhow!("HTTP error: 403 Forbidden")))
            },
            &|| true,
            Some(EDGE_PREVIEW_ATTEMPTS),
            |_, _| Duration::ZERO,
            "test",
        )
        .await
        .unwrap_err();
        assert_eq!(calls.get(), EDGE_PREVIEW_ATTEMPTS);
        assert!(error.to_string().contains("403"));
    }

    #[tokio::test]
    async fn audiobook_retry_survives_forty_failures_and_keeps_completed_chunks() {
        let mut output = Vec::new();
        let requests = std::cell::RefCell::new(Vec::new());
        let failures = Cell::new(0usize);
        for chunk in [1, 2, 3] {
            let result = retry_playable_audio(
                || {
                    requests.borrow_mut().push(chunk);
                    if chunk == 2 && failures.get() < 40 {
                        failures.set(failures.get() + 1);
                        std::future::ready(Err(anyhow!("HTTP error: 403 Forbidden")))
                    } else {
                        std::future::ready(Ok(Some(playable_audio())))
                    }
                },
                &|| true,
                None,
                |_, _| Duration::ZERO,
                "persistent_audiobook",
            )
            .await
            .unwrap()
            .unwrap();
            output.push((chunk, result));
        }
        assert_eq!(
            output.iter().map(|(index, _)| *index).collect::<Vec<_>>(),
            [1, 2, 3]
        );
        let requests = requests.borrow();
        assert_eq!(requests.iter().filter(|&&index| index == 1).count(), 1);
        assert_eq!(requests.iter().filter(|&&index| index == 2).count(), 41);
        assert_eq!(requests.iter().filter(|&&index| index == 3).count(), 1);
    }

    #[test]
    fn lithuanian_suspicion_uses_windows_utf16_thresholds() {
        assert!(!is_lithuanian_audio_suspicious(&"ą".repeat(119), 1000));
        assert!(is_lithuanian_audio_suspicious(&"ą".repeat(120), 14_399));
        assert!(!is_lithuanian_audio_suspicious(&"ą".repeat(120), 14_400));
        assert!(is_lithuanian_audio_suspicious(&"🙂".repeat(60), 14_399));
        let text = format!("{} {}", "ą".repeat(75), "字".repeat(75));
        let (left, right) = split_lithuanian_text_in_two(&text).unwrap();
        assert_eq!(
            [left, right].join("").replace(' ', ""),
            text.replace(' ', "")
        );
    }

    #[tokio::test]
    async fn lithuanian_replacement_preserves_order_without_parent_audio() {
        let text = format!("{} {}", "a".repeat(140), "b".repeat(140));
        let calls = std::cell::RefCell::new(Vec::new());
        let result =
            synthesize_audiobook_parts(&text, "lt-LT-OnaNeural", &|| true, &mut |part: String| {
                calls.borrow_mut().push(part.clone());
                let audio = if part == text {
                    vec![1; 1500]
                } else if part.starts_with('a') {
                    vec![2; 20_000]
                } else {
                    vec![3; 20_000]
                };
                std::future::ready(Ok(Some(audio)))
            })
            .await
            .unwrap()
            .unwrap();
        assert_eq!(calls.into_inner(), [text, "a".repeat(140), "b".repeat(140)]);
        assert_eq!(result, [vec![2; 20_000], vec![3; 20_000]].concat());
    }

    #[tokio::test]
    async fn lithuanian_strict_pass_replaces_only_with_larger_audio_and_has_bounded_depth() {
        for improve in [false, true] {
            let calls = Cell::new(0usize);
            let result = synthesize_audiobook_parts(
                &"a".repeat(640),
                "lt-LT-OnaNeural",
                &|| true,
                &mut |part: String| {
                    let call = calls.get() + 1;
                    calls.set(call);
                    let strict = call > 15;
                    if strict {
                        assert!(part.len() <= 320);
                    }
                    let audio = if strict && improve {
                        vec![2; 9000]
                    } else {
                        vec![1; 1000]
                    };
                    std::future::ready(Ok(Some(audio)))
                },
            )
            .await
            .unwrap()
            .unwrap();
            assert_eq!(calls.get(), 17);
            assert_eq!(
                result,
                if improve {
                    vec![2; 18_000]
                } else {
                    vec![1; 8000]
                }
            );
        }
    }

    #[tokio::test]
    async fn cancellation_during_lithuanian_replacement_discards_partial_result() {
        let active = Cell::new(true);
        let calls = Cell::new(0usize);
        let result = synthesize_audiobook_parts(
            &format!("{} {}", "a".repeat(140), "b".repeat(140)),
            "lt-LT-OnaNeural",
            &|| active.get(),
            &mut |_: String| {
                calls.set(calls.get() + 1);
                if calls.get() == 2 {
                    active.set(false);
                }
                std::future::ready(Ok(Some(vec![1; 1500])))
            },
        )
        .await
        .unwrap();
        assert!(result.is_none());
        assert_eq!(calls.get(), 2);
    }

    #[tokio::test]
    async fn empty_input_skips_generation_and_other_voices_skip_lithuanian_heuristics() {
        let calls = Cell::new(0usize);
        let mut generate = |_: String| {
            calls.set(calls.get() + 1);
            std::future::ready(Ok(Some(vec![1; 1500])))
        };
        let skipped =
            synthesize_audiobook_parts(" | … 😀 ", "lt-LT-OnaNeural", &|| true, &mut generate)
                .await
                .unwrap();
        assert_eq!(skipped, Some(Vec::new()));
        assert_eq!(calls.get(), 0);
        let normal = synthesize_audiobook_parts(
            &"a".repeat(300),
            "it-IT-ElsaNeural",
            &|| true,
            &mut generate,
        )
        .await
        .unwrap();
        assert_eq!(normal, Some(vec![1; 1500]));
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn realtime_future_remains_send_for_the_existing_tokio_worker() {
        fn assert_send<T: Send>(_: T) {}
        let active = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        assert_send(synthesize_reading_chunk(
            None,
            "frase",
            "it-IT-ElsaNeural",
            0,
            0,
            100,
            0,
            move || active.load(std::sync::atomic::Ordering::Relaxed),
        ));
    }

    #[test]
    fn cancellation_interrupts_full_audio_validation() {
        let polls = Cell::new(0usize);
        let active = || {
            polls.set(polls.get() + 1);
            polls.get() < 3
        };
        assert!(!validate_audio_cancellable(&playable_audio(), &active).unwrap());
        assert_eq!(polls.get(), 3);
    }

    #[test]
    fn full_audio_validation_reads_concatenated_mp3_parts() {
        let audio = playable_audio();
        let single_samples = Decoder::new(Cursor::new(audio.clone())).unwrap().count();
        let combined = [audio.clone(), audio.clone(), audio].concat();
        let combined_samples = Decoder::new(Cursor::new(combined.clone())).unwrap().count();
        assert!(combined_samples >= single_samples * 2);
        validate_audio(&combined).unwrap();
        assert!(validate_audio(&vec![0; 4000]).is_err());
    }

    #[test]
    fn cache_resumes_only_valid_chunks_with_matching_voice_and_boundaries() {
        let root =
            std::env::temp_dir().join(format!("sonarpad-edge-cache-test-{}", uuid::Uuid::new_v4()));
        let chunks = vec!["prima frase".to_string(), "seconda frase".to_string()];
        let cache = AudiobookCache::open(&root, &chunks, "it-IT-ElsaNeural", 0, 0, 100).unwrap();
        let audio = playable_audio();
        cache.store(0, &audio).unwrap();
        std::fs::write(cache.chunk_path(1), vec![0; 1500]).unwrap();

        let resumed = AudiobookCache::open(&root, &chunks, "it-IT-ElsaNeural", 0, 0, 100).unwrap();
        assert_eq!(resumed.load(0), Some(audio));
        assert!(resumed.load(1).is_none());
        assert!(!resumed.chunk_path(1).exists());

        let other_voice =
            AudiobookCache::open(&root, &chunks, "it-IT-DiegoNeural", 0, 0, 100).unwrap();
        assert!(other_voice.load(0).is_none());
        let other_rate =
            AudiobookCache::open(&root, &chunks, "it-IT-ElsaNeural", 10, 0, 100).unwrap();
        assert!(other_rate.load(0).is_none());
        let other_chunks = vec!["prima frase seconda frase".to_string()];
        let other_boundaries =
            AudiobookCache::open(&root, &other_chunks, "it-IT-ElsaNeural", 0, 0, 100).unwrap();
        assert!(other_boundaries.load(0).is_none());
        assert!(cache.store(1, &[]).is_err());
        cache.finish();
        assert!(!cache.directory().exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_or_abandoned_export_preserves_an_existing_destination() {
        let root = std::env::temp_dir().join(format!(
            "sonarpad-edge-output-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let destination = root.join("audiolibro.mp3");
        let original = playable_audio();
        std::fs::write(&destination, &original).unwrap();

        let failed = AudiobookOutput::new(&destination);
        let failed_path = failed.path().to_owned();
        std::fs::write(failed.path(), []).unwrap();
        assert!(failed.commit(&destination).is_err());
        assert_eq!(std::fs::read(&destination).unwrap(), original);
        assert!(!failed_path.exists());

        let cancelled = AudiobookOutput::new(&destination);
        let cancelled_path = cancelled.path().to_owned();
        std::fs::write(cancelled.path(), &original).unwrap();
        drop(cancelled);
        assert_eq!(std::fs::read(&destination).unwrap(), original);
        assert!(!cancelled_path.exists());
        std::fs::remove_dir_all(root).unwrap();
    }
}

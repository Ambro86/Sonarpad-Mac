//! Retry one audio-description cue without restarting completed cues.
//!
//! The renderer classifies transport/output failures before entering this
//! policy. Local file, configuration and voice errors remain terminal.

use std::thread;
use std::time::Duration;

const RETRY_DELAY: Duration = Duration::from_secs(5);
const CANCEL_POLL: Duration = Duration::from_millis(75);

pub(super) enum AttemptError {
    Retryable {
        stage: &'static str,
        error: String,
        delay: Duration,
    },
    Permanent(String),
}

pub(super) fn has_audible_signal(samples: &[i16]) -> bool {
    // Match the mobile max-volume check (> -70 dBFS). A nonzero PCM sample
    // alone can be inaudible encoding noise; 11/32768 is above that threshold.
    samples.iter().any(|sample| i32::from(*sample).abs() > 10)
}

pub(super) fn is_invalid_output(error: &str) -> bool {
    let message = error.to_lowercase();
    if [
        "cancelled",
        "canceled",
        "permission denied",
        "operation not permitted",
        "access is denied",
        "read-only file system",
        "no space left",
        "disk full",
        "no such file or directory",
        "not found in path",
        "command not found",
        "unknown encoder",
        "encoder not found",
    ]
    .iter()
    .any(|part| message.contains(part))
    {
        return false;
    }

    [
        "tts audio vuoto",
        "tts wav vuoto",
        "audio vuoto",
        "nessun campione pcm",
        "tts_empty_output",
        "tts_silent_output",
        "tts_duration_invalid",
        "empty audio",
        "empty payload",
        "no audio sent",
        "file audio troppo piccolo",
        "invalid audio",
        "decode failed",
        "invalid data found when processing input",
        "failed to read frame size",
        "invalid frame size",
        "header missing",
        "could not find codec parameters",
        "tts wav non leggibile",
        "lettura tts wav fallita",
    ]
    .iter()
    .any(|part| message.contains(part))
}

pub(super) fn run<T, G, A, R>(
    generate: G,
    is_active: A,
    on_retry: R,
    max_attempts: Option<usize>,
) -> Result<T, String>
where
    G: FnMut() -> Result<T, AttemptError>,
    A: Fn() -> bool,
    R: FnMut(usize, &'static str, &str, Duration),
{
    run_with_wait_limit(
        generate,
        is_active,
        on_retry,
        max_attempts,
        |active, delay| wait_for_retry(active, delay),
    )
}

fn wait_for_retry<A: Fn() -> bool>(is_active: &A, mut remaining: Duration) -> bool {
    while !remaining.is_zero() {
        if !is_active() {
            return false;
        }
        let slice = remaining.min(CANCEL_POLL);
        thread::sleep(slice);
        remaining = remaining.saturating_sub(slice);
    }
    is_active()
}

fn run_with_wait_limit<T, G, A, R, W>(
    mut generate: G,
    is_active: A,
    mut on_retry: R,
    max_attempts: Option<usize>,
    mut wait: W,
) -> Result<T, String>
where
    G: FnMut() -> Result<T, AttemptError>,
    A: Fn() -> bool,
    R: FnMut(usize, &'static str, &str, Duration),
    W: FnMut(&A, Duration) -> bool,
{
    let mut retries = 0_usize;
    loop {
        if !is_active() {
            return Err("cancelled".to_string());
        }
        let result = generate();
        // Stop or a failed sibling invalidates even a successful late result.
        if !is_active() {
            return Err("cancelled".to_string());
        }
        match result {
            Ok(value) => return Ok(value),
            Err(AttemptError::Permanent(error)) => return Err(error),
            Err(AttemptError::Retryable {
                stage,
                error,
                delay,
            }) => {
                if max_attempts.is_some_and(|limit| retries.saturating_add(1) >= limit.max(1)) {
                    return Err(error);
                }
                retries = retries.saturating_add(1);
                let delay = RETRY_DELAY.max(delay);
                on_retry(retries, stage, &error, delay);
                if !wait(&is_active, delay) {
                    return Err("cancelled".to_string());
                }
            }
        }
    }
}

#[cfg(test)]
fn run_with_wait<T, G, A, R, W>(
    generate: G,
    is_active: A,
    on_retry: R,
    wait: W,
) -> Result<T, String>
where
    G: FnMut() -> Result<T, AttemptError>,
    A: Fn() -> bool,
    R: FnMut(usize, &'static str, &str, Duration),
    W: FnMut(&A, Duration) -> bool,
{
    run_with_wait_limit(generate, is_active, on_retry, None, wait)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Instant;

    fn forbidden() -> AttemptError {
        AttemptError::Retryable {
            stage: "renderer",
            error: "HTTP error: 403 Forbidden".to_string(),
            delay: Duration::ZERO,
        }
    }

    #[test]
    fn forbidden_retries_only_the_failed_cue_and_keeps_completed_audio() {
        let requested = RefCell::new(Vec::new());
        let attempts = [Cell::new(0), Cell::new(0), Cell::new(0)];
        let mut completed = Vec::new();
        for (cue, attempt) in attempts.iter().enumerate() {
            let value = run_with_wait(
                || {
                    requested.borrow_mut().push(cue);
                    attempt.set(attempt.get() + 1);
                    if cue == 1 && attempt.get() <= 2 {
                        Err(forbidden())
                    } else {
                        Ok(vec![cue as i16, 100])
                    }
                },
                || true,
                |_, _, _, _| {},
                |active, _| active(),
            )
            .unwrap();
            completed.push(value);
        }
        assert_eq!(*requested.borrow(), vec![0, 1, 1, 1, 2]);
        assert_eq!(completed, vec![vec![0, 100], vec![1, 100], vec![2, 100]]);
    }

    #[test]
    fn transient_failures_do_not_exhaust_the_old_attempt_limit() {
        let calls = Cell::new(0);
        let mut retries = Vec::new();
        let result = run_with_wait(
            || {
                calls.set(calls.get() + 1);
                if calls.get() <= 51 {
                    Err(forbidden())
                } else {
                    Ok(42)
                }
            },
            || true,
            |retry, _, _, _| retries.push(retry),
            |active, _| active(),
        );
        assert_eq!(result.unwrap(), 42);
        assert_eq!(calls.get(), 52);
        assert_eq!(retries, (1..=51).collect::<Vec<_>>());
    }

    #[test]
    fn permanent_voice_or_file_error_is_returned_without_retry() {
        for message in [
            "invalid voice",
            "Permission denied writing TTS temporary file",
        ] {
            let calls = Cell::new(0);
            let result = run_with_wait::<(), _, _, _, _>(
                || {
                    calls.set(calls.get() + 1);
                    Err(AttemptError::Permanent(message.to_string()))
                },
                || true,
                |_, _, _, _| panic!("a permanent failure must not retry"),
                |_, _| panic!("a permanent failure must not wait"),
            );
            assert_eq!(result.unwrap_err(), message);
            assert_eq!(calls.get(), 1);
        }
    }

    #[test]
    fn cancellation_during_backoff_prevents_another_request() {
        let active = Cell::new(true);
        let calls = Cell::new(0);
        let result = run_with_wait::<(), _, _, _, _>(
            || {
                calls.set(calls.get() + 1);
                Err(forbidden())
            },
            || active.get(),
            |_, _, _, _| active.set(false),
            |is_active, _| is_active(),
        );
        assert_eq!(result.unwrap_err(), "cancelled");
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn a_success_after_stop_is_discarded() {
        let active = Cell::new(true);
        let result = run_with_wait(
            || {
                active.set(false);
                Ok(vec![123_i16])
            },
            || active.get(),
            |_, _, _, _| panic!("no retry after stop"),
            |is_active, _| is_active(),
        );
        assert_eq!(result.unwrap_err(), "cancelled");
    }

    #[test]
    fn real_five_second_wait_is_interruptible() {
        let active = Arc::new(AtomicBool::new(true));
        let cancel = active.clone();
        let started = Instant::now();
        let stopper = thread::spawn(move || {
            thread::sleep(Duration::from_millis(25));
            cancel.store(false, Ordering::Relaxed);
        });
        let result = wait_for_retry(&|| active.load(Ordering::Relaxed), RETRY_DELAY);
        stopper.join().unwrap();
        assert!(!result);
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn a_longer_server_retry_after_is_respected() {
        let calls = Cell::new(0);
        let mut waits = Vec::new();
        let result = run_with_wait(
            || {
                calls.set(calls.get() + 1);
                if calls.get() == 1 {
                    Err(AttemptError::Retryable {
                        stage: "renderer",
                        error: "HTTP 429 Too Many Requests".to_string(),
                        delay: Duration::from_secs(30),
                    })
                } else {
                    Ok(123)
                }
            },
            || true,
            |_, _, _, _| {},
            |active, delay| {
                waits.push(delay);
                active()
            },
        );
        assert_eq!(result.unwrap(), 123);
        assert_eq!(waits, vec![Duration::from_secs(30)]);
    }

    #[test]
    fn synchronous_editor_checks_have_a_finite_attempt_budget() {
        let calls = Cell::new(0);
        let result = run_with_wait_limit::<(), _, _, _, _>(
            || {
                calls.set(calls.get() + 1);
                Err(forbidden())
            },
            || true,
            |_, _, _, _| {},
            Some(6),
            |active, _| active(),
        );
        assert_eq!(result.unwrap_err(), "HTTP error: 403 Forbidden");
        assert_eq!(calls.get(), 6);
    }

    #[test]
    fn output_recovery_does_not_hide_local_configuration_errors() {
        for error in [
            "TTS audio vuoto: payload MP3 senza dati.",
            "FFmpeg: Invalid data found when processing input",
            "TTS WAV non leggibile: invalid RIFF header",
            "TTS_SILENT_OUTPUT",
        ] {
            assert!(is_invalid_output(error), "{error}");
        }
        for error in [
            "TTS WAV non leggibile: Permission denied",
            "Lettura TTS WAV fallita: No such file or directory",
            "FFmpeg not found in PATH",
            "Scrittura TTS temporaneo fallita: No space left on device",
            "cancelled",
        ] {
            assert!(!is_invalid_output(error), "{error}");
        }
    }

    #[test]
    fn audible_signal_check_rejects_silence_and_encoding_noise() {
        assert!(!has_audible_signal(&[]));
        assert!(!has_audible_signal(&[0, 0, 0]));
        assert!(!has_audible_signal(&[-10, 0, 10]));
        assert!(has_audible_signal(&[-11, 0, 11]));
        assert!(has_audible_signal(&[i16::MIN]));
    }
}

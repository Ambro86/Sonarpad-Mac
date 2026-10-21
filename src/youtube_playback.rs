//! Playback routing for YouTube and SonarMusic. The internal resolver is the
//! existing Rust Windows resolver in sonarmusic; platform player code stays in main.
use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

pub fn direct_watch_url(input: &str, has_credentials: bool) -> Option<String> {
    if has_credentials {
        return None;
    }
    let u = reqwest::Url::parse(input).ok()?;
    if !u.username().is_empty() || u.password().is_some() {
        return None;
    }
    let id = match u.host_str()? {
        "youtu.be" => u.path().trim_matches('/').to_owned(),
        "youtube.com" | "www.youtube.com" | "m.youtube.com" | "music.youtube.com" => {
            if u.path() == "/watch" {
                u.query_pairs().find(|(k, _)| k == "v")?.1.into_owned()
            } else {
                let mut parts = u.path().trim_matches('/').split('/');
                if !matches!(parts.next(), Some("shorts" | "live" | "embed")) {
                    return None;
                }
                parts.next()?.to_owned()
            }
        }
        _ => return None,
    };
    (id.len() == 11
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-'))
    .then(|| format!("https://www.youtube.com/watch?v={id}"))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Internal,
    Php,
    Ytdlp,
}

pub fn open_with_fallback(
    url: &str,
    cancel: &AtomicBool,
    cancelled: &str,
    mut resolve: impl FnMut(Source, &str) -> Result<String, String>,
    mut open: impl FnMut(Source, Option<&str>) -> Result<(), String>,
) -> Result<(), String> {
    let canonical = direct_watch_url(url, false);
    let mut last_error = String::new();
    for source in [Source::Internal, Source::Php, Source::Ytdlp] {
        if cancel.load(Ordering::SeqCst) {
            return Err(cancelled.into());
        }
        let direct = if source == Source::Ytdlp {
            None
        } else {
            let Some(watch) = canonical.as_deref() else {
                continue;
            };
            match resolve(source, watch) {
                Ok(url) => Some(url),
                Err(error) => {
                    last_error = error;
                    continue;
                }
            }
        };
        if cancel.load(Ordering::SeqCst) {
            return Err(cancelled.into());
        }
        match open(source, direct.as_deref()) {
            Ok(()) => return Ok(()),
            Err(error) => last_error = error,
        }
    }
    if cancel.load(Ordering::SeqCst) {
        Err(cancelled.into())
    } else {
        Err(last_error)
    }
}

pub fn resolve_php(watch_url: &str, token: &str) -> Result<String, String> {
    if token.is_empty() {
        return Err("PHP route token unavailable".into());
    }
    let canonical = direct_watch_url(watch_url, false).ok_or("Unsupported YouTube URL")?;
    let data: Value = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(20))
        .user_agent("Sonarpad/1.0")
        .build()
        .map_err(|_| "PHP client unavailable")?
        .get("https://sonarpad.com/api/youtube_resolve.php")
        .query(&[
            ("url", canonical.as_str()),
            ("format", "json"),
            ("quality", "360p"),
            ("prefer", "progressive"),
        ])
        .header("X-Sonarpad-Route-Token", token)
        .send()
        .map_err(|_| "PHP resolver connection failed")?
        .error_for_status()
        .map_err(|_| "PHP resolver HTTP failure")?
        .json()
        .map_err(|_| "PHP resolver invalid JSON")?;
    php_stream(&data)
}
fn php_stream(data: &Value) -> Result<String, String> {
    if data["ok"] != true || !matches!(data["stream_type"].as_str(), Some("progressive" | "hls")) {
        return Err("PHP resolver has no combined audio/video stream".into());
    }
    let raw = data["stream"].as_str().ok_or("PHP stream missing")?;
    let url = reqwest::Url::parse(raw).map_err(|_| "Invalid PHP stream")?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || !url
            .host_str()
            .is_some_and(|h| h.ends_with(".googlevideo.com"))
    {
        return Err("Untrusted PHP media URL".into());
    }
    Ok(raw.into())
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::cell::RefCell;
    const URL: &str = "https://www.youtube.com/watch?v=Jpz9AC_QJig";
    #[test]
    fn internal_success_stops_chain() {
        let calls = RefCell::new(Vec::new());
        open_with_fallback(
            URL,
            &AtomicBool::new(false),
            "cancel",
            |s, _| {
                calls.borrow_mut().push(s);
                Ok("media".into())
            },
            |s, _| {
                assert_eq!(s, Source::Internal);
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(*calls.borrow(), [Source::Internal]);
    }
    #[test]
    fn internal_player_failure_uses_php_then_ytdlp() {
        let opened = RefCell::new(Vec::new());
        open_with_fallback(
            URL,
            &AtomicBool::new(false),
            "cancel",
            |_, _| Ok("media".into()),
            |s, url| {
                opened.borrow_mut().push(s);
                if s == Source::Ytdlp {
                    assert!(url.is_none());
                    Ok(())
                } else {
                    Err("load failed".into())
                }
            },
        )
        .unwrap();
        assert_eq!(
            *opened.borrow(),
            [Source::Internal, Source::Php, Source::Ytdlp]
        );
    }
    #[test]
    fn resolver_failure_uses_php() {
        open_with_fallback(
            URL,
            &AtomicBool::new(false),
            "cancel",
            |s, _| {
                if s == Source::Internal {
                    Err("offline".into())
                } else {
                    Ok("media".into())
                }
            },
            |s, _| {
                assert_eq!(s, Source::Php);
                Ok(())
            },
        )
        .unwrap();
    }
    #[test]
    fn cancel_after_resolve_does_not_open_player() {
        let cancel = AtomicBool::new(false);
        let r = open_with_fallback(
            URL,
            &cancel,
            "cancel",
            |_, _| {
                cancel.store(true, Ordering::SeqCst);
                Ok("media".into())
            },
            |_, _| panic!(),
        );
        assert_eq!(r, Err("cancel".into()));
    }
    #[test]
    fn cancel_after_player_failure_does_not_fallback() {
        let cancel = AtomicBool::new(false);
        assert_eq!(
            open_with_fallback(
                URL,
                &cancel,
                "cancel",
                |_, _| Ok("media".into()),
                |_, _| {
                    cancel.store(true, Ordering::SeqCst);
                    Err("failed".into())
                }
            ),
            Err("cancel".into())
        );
    }
    #[test]
    fn external_url_keeps_original_extractor() {
        open_with_fallback(
            "https://vimeo.com/123",
            &AtomicBool::new(false),
            "cancel",
            |_, _| panic!(),
            |s, d| {
                assert_eq!(s, Source::Ytdlp);
                assert!(d.is_none());
                Ok(())
            },
        )
        .unwrap();
    }
    #[test]
    fn embedded_credentials_bypass_direct() {
        assert!(
            direct_watch_url(
                "https://user:secret@www.youtube.com/watch?v=Jpz9AC_QJig",
                false
            )
            .is_none()
        );
    }
    #[test]
    fn explicit_credentials_bypass_direct() {
        assert!(direct_watch_url(URL, true).is_none());
    }
    #[test]
    fn short_url_normalizes() {
        assert_eq!(
            direct_watch_url("https://youtu.be/Jpz9AC_QJig", false).as_deref(),
            Some(URL)
        );
    }
    #[test]
    fn php_rejects_video_only() {
        assert!(php_stream(&json!({"ok":true,"stream_type":"video","stream":"https://rr1.googlevideo.com/video"})).is_err());
    }
    #[test]
    fn php_rejects_failed_response() {
        assert!(php_stream(&json!({"ok":false})).is_err());
    }
    #[test]
    fn php_accepts_combined_stream() {
        assert!(php_stream(&json!({"ok":true,"stream_type":"progressive","stream":"https://rr1.googlevideo.com/video"})).is_ok());
    }
    #[test]
    fn php_accepts_hls() {
        assert!(php_stream(&json!({"ok":true,"stream_type":"hls","stream":"https://manifest.googlevideo.com/list"})).is_ok());
    }
    #[test]
    fn php_rejects_spoofed_host() {
        assert!(php_stream(&json!({"ok":true,"stream_type":"hls","stream":"https://googlevideo.com.evil.test/list"})).is_err());
    }
    #[test]
    fn all_fail_returns_final_error() {
        assert_eq!(
            open_with_fallback(
                URL,
                &AtomicBool::new(false),
                "cancel",
                |_, _| Err("resolve failed".into()),
                |_, _| Err("player failed".into())
            ),
            Err("player failed".into())
        );
    }
}

#[cfg(test)]
mod live_tests {
    #[test]
    #[ignore = "Requires network and SONARMUSIC_LIVE_TOKEN"]
    fn internal_and_php_resolve_real_videos() {
        let token = std::env::var("SONARMUSIC_LIVE_TOKEN").expect("live token required");
        for id in ["Jpz9AC_QJig", "4K6cvc27_CE", "JPl_XV3OuBU"] {
            let watch = format!("https://www.youtube.com/watch?v={id}");
            let direct = crate::sonarmusic::resolve_playback(&watch).expect("internal resolver");
            assert!(direct.starts_with("https://"));
            let php = super::resolve_php(&watch, &token).expect("PHP resolver");
            assert!(php.starts_with("https://"));
            println!("{id}: internal and PHP resolution passed");
        }
    }
}

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::time::Duration;

const ENDPOINT: &str = "https://sonarpad.com/api/logdeveloper.php";
const MAX_LOG_BYTES: u64 = 2 * 1024 * 1024;

pub fn valid_email(value: &str) -> bool {
    let value = value.trim();
    if value.is_empty() {
        return true;
    }
    if value.len() > 254 || !value.is_ascii() {
        return false;
    }
    let Some((local, domain)) = value.split_once('@') else {
        return false;
    };
    !local.is_empty()
        && local.len() <= 64
        && !local.starts_with('.')
        && !local.ends_with('.')
        && !local.contains("..")
        && local
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b".!#$%&'*+/=?^_`{|}~-".contains(&c))
        && domain.contains('.')
        && domain.split('.').all(|part| {
            !part.is_empty()
                && part.len() <= 63
                && !part.starts_with('-')
                && !part.ends_with('-')
                && part.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
        })
}

fn sanitize(content: &str) -> Result<String, String> {
    let mut text = content.to_owned();
    for pattern in [
        r"/Users/[^/\s]+",
        r"(?i)C:\\Users\\[^\\\r\n]+",
        r"AIza[\w-]{20,}",
        r"\b(?:sp_|sst_)[\w-]{12,}",
        r"(?i)Bearer\s+[\w.+/=-]+",
        r#"(?i)(?:api[_-]?key|token|password|secret|access[_-]?code)["']?\s*[=:]\s*["']?[^\s&"',;}]+"#,
        r"(?i)https?://[^/\s@]+:[^/\s@]+@",
    ] {
        let regex = regex::Regex::new(pattern).map_err(|_| "redaction_failed".to_string())?;
        text = regex.replace_all(&text, "[REDACTED]").into_owned();
    }
    Ok(text)
}

fn read_log(path: &Path) -> Result<Option<String>, String> {
    let mut file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(format!("log_open: {}", err.kind())),
    };
    let length = file.metadata().map_err(|_| "log_metadata")?.len();
    let offset = length.saturating_sub(MAX_LOG_BYTES);
    file.seek(SeekFrom::Start(offset)).map_err(|_| "log_seek")?;
    let mut bytes = Vec::new();
    file.take(MAX_LOG_BYTES)
        .read_to_end(&mut bytes)
        .map_err(|_| "log_read")?;
    let text = String::from_utf8_lossy(&bytes);
    // Avoid sending a partial first line (which could cut a secret's identifying prefix).
    let text = if offset > 0 {
        text.split_once('\n').map(|(_, tail)| tail).unwrap_or("")
    } else {
        &text
    };
    sanitize(text).map(Some)
}

fn collect_log(path: &Path) -> Result<String, String> {
    let mut text = format!(
        "Sonarpad Mac {}\nArchitecture: {}\n",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::ARCH
    );
    let backup = path.with_file_name(format!(
        "{}.1",
        path.file_name().unwrap_or_default().to_string_lossy()
    ));
    let mut found = false;
    for (label, file) in [("previous log", backup.as_path()), ("current log", path)] {
        if let Some(log) = read_log(file)? {
            found = true;
            text.push_str(&format!("\n--- {label} ---\n{log}"));
        }
    }
    if !found {
        return Err("log_missing".into());
    }
    Ok(text)
}

pub fn send(path: &Path, name: &str, email: &str, report: &str) -> Result<(), String> {
    if report.trim().is_empty()
        || report.chars().count() > 8000
        || name.chars().count() > 200
        || !valid_email(email)
    {
        return Err("invalid_report".into());
    }
    let log = collect_log(path)?;
    send_to(ENDPOINT, name, email, report, &log)
}

fn send_to(endpoint: &str, name: &str, email: &str, report: &str, log: &str) -> Result<(), String> {
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(60))
        .redirect(reqwest::redirect::Policy::none())
        .user_agent("Sonarpad-Mac")
        .build()
        .map_err(|_| "http_client")?;
    let response = client
        .post(endpoint)
        .json(&serde_json::json!({
            "name": name.trim(), "email": email.trim(), "report": report.trim(),
            "log": log, "platform": "macos"
        }))
        .send()
        .map_err(|err| {
            if err.is_timeout() {
                "http_timeout"
            } else {
                "http_connection"
            }
        })?;
    if !response.status().is_success() {
        return Err(format!("http_status_{}", response.status().as_u16()));
    }
    let mut bytes = Vec::new();
    response
        .take(65537)
        .read_to_end(&mut bytes)
        .map_err(|_| "http_response")?;
    if bytes.len() > 65536 {
        return Err("http_response_too_large".into());
    }
    let json: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| "http_response_json")?;
    if json.get("ok").and_then(|v| v.as_bool()) != Some(true) {
        return Err("report_rejected".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn optional_email_validation() {
        for email in ["", " ", " user+test@example.com "] {
            assert!(valid_email(email));
        }
        for email in [
            "missing",
            "x@@example.com",
            "x@y..com",
            "a\nb@example.com",
            "x@-bad.it",
        ] {
            assert!(!valid_email(email));
        }
    }
    #[test]
    fn log_secrets_are_removed() {
        let log = "/Users/Mario/Library/log.txt token=private123 {\"api_key\":\"secretvalue\"} Bearer abc123\nHTTP 403";
        let cleaned = sanitize(log).unwrap();
        for secret in ["Mario", "private123", "secretvalue", "abc123"] {
            assert!(!cleaned.contains(secret));
        }
        assert!(cleaned.contains("HTTP 403"));
    }
    #[test]
    fn collection_handles_rotation_truncation_and_missing_files() {
        let directory = std::env::temp_dir().join(format!(
            "sonarpad-report-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join("log.txt");
        assert!(collect_log(&path).is_err());
        std::fs::write(directory.join("log.txt.1"), "previous error\n").unwrap();
        let long = format!(
            "token={}\ncurrent error 日本語\n",
            "x".repeat(MAX_LOG_BYTES as usize + 100)
        );
        std::fs::write(&path, long).unwrap();
        let log = collect_log(&path).unwrap();
        assert!(log.contains("previous error"));
        assert!(log.contains("current error 日本語"));
        assert!(!log.contains("xxx"));
        std::fs::remove_file(&path).unwrap();
        std::fs::remove_file(directory.join("log.txt.1")).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }
    #[test]
    fn http_payload_and_acknowledgement() {
        use std::io::{BufRead, BufReader, Write};
        for (email, reply, expected) in [
            ("", "{\"ok\":true}", true),
            (" person@example.com ", "{\"ok\":true}", true),
            ("", "{\"ok\":false}", false),
            ("", "invalid", false),
        ] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let endpoint = format!("http://{}", listener.local_addr().unwrap());
            let worker = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut reader = BufReader::new(&mut stream);
                let mut length = 0;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" {
                        break;
                    }
                    if let Some(value) = line.to_lowercase().strip_prefix("content-length:") {
                        length = value.trim().parse::<usize>().unwrap();
                    }
                }
                let mut bytes = vec![0; length];
                reader.read_exact(&mut bytes).unwrap();
                let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                assert_eq!(payload["email"], email.trim());
                assert_eq!(payload["name"], "Name");
                assert_eq!(payload["report"], "Report");
                assert_eq!(payload["log"], "test log");
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    reply.len(),
                    reply
                )
                .unwrap();
            });
            assert_eq!(
                send_to(&endpoint, " Name ", email, " Report ", "test log").is_ok(),
                expected
            );
            worker.join().unwrap();
        }
    }
}

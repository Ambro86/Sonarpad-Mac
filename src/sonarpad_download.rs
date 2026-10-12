use curl::easy::{Easy, List};
use std::cell::{Cell, RefCell};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const LOW_SPEED_TIMEOUT: Duration = Duration::from_secs(60);
const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

/// Blocking transport for the download worker. Progress counts bytes written
/// to the temporary file, with a total only when the decoded size is known.
/// The destination is replaced only after a complete, validated transfer.
pub(crate) fn download(
    url: &str,
    destination: &Path,
    cancelled: &AtomicBool,
    mut progress: impl FnMut(u64, Option<u64>),
) -> Result<bool, String> {
    if cancelled.load(Ordering::Relaxed) {
        return Ok(false);
    }
    let parsed_url = url::Url::parse(url.trim())
        .map_err(|err| format!("URL di download non valido: {err}"))?;
    if !matches!(parsed_url.scheme(), "http" | "https") {
        return Err("Il download richiede un indirizzo HTTP o HTTPS.".to_string());
    }
    let mut easy = configured_client(parsed_url.as_str())?;
    let mut temporary = TemporaryDownload::create(destination)?;
    let response_headers = RefCell::new(ResponseHeaders::default());
    let callback_error = RefCell::new(None::<String>);
    let written = Cell::new(0_u64);
    let mut last_report = Instant::now();
    let mut last_progress = (0_u64, None);
    progress(0, None);

    let result = {
        let file = temporary
            .file
            .as_mut()
            .ok_or("File temporaneo di download non disponibile.")?;
        let mut transfer = easy.transfer();
        transfer
            .header_function(|data| {
                if cancelled.load(Ordering::Relaxed) {
                    return false;
                }
                if let Err(err) = response_headers.borrow_mut().receive(data) {
                    *callback_error.borrow_mut() = Some(err);
                    return false;
                }
                true
            })
            .map_err(|err| err.to_string())?;
        transfer
            .write_function(|data| {
                if cancelled.load(Ordering::Relaxed) {
                    return Ok(0);
                }
                {
                    let headers = response_headers.borrow();
                    // Redirect and authentication responses must never become
                    // part of the selected media file.
                    if !(200..300).contains(&headers.status) {
                        return Ok(data.len());
                    }
                    if let Err(err) = headers.validate_media_response() {
                        *callback_error.borrow_mut() = Some(err);
                        return Ok(0);
                    }
                }
                if let Err(err) = file.write_all(data) {
                    *callback_error.borrow_mut() =
                        Some(format!("Impossibile scrivere il file scaricato: {err}"));
                    return Ok(0);
                }
                written.set(written.get().saturating_add(data.len() as u64));
                Ok(data.len())
            })
            .map_err(|err| err.to_string())?;
        transfer
            .progress_function(|_, _, _, _| {
                if cancelled.load(Ordering::Relaxed) {
                    return false;
                }
                let current = (written.get(), response_headers.borrow().decoded_total());
                if current != last_progress
                    && (last_report.elapsed() >= PROGRESS_INTERVAL
                        || current.1 != last_progress.1)
                {
                    progress(current.0, current.1);
                    last_progress = current;
                    last_report = Instant::now();
                }
                !cancelled.load(Ordering::Relaxed)
            })
            .map_err(|err| err.to_string())?;
        transfer.perform()
    };

    if cancelled.load(Ordering::Relaxed) {
        return Ok(false);
    }
    if let Some(err) = callback_error.into_inner() {
        return Err(err);
    }
    let status = easy.response_code().map_err(|err| err.to_string())?;
    if !(200..300).contains(&status) {
        if status == 0 {
            return Err(result.err().map_or_else(
                || "Il server non ha restituito una risposta HTTP valida.".to_string(),
                |err| format!("Download audiodescrizione Sonarpad fallito: {err}"),
            ));
        }
        return Err(format!("Il server ha rifiutato il download: HTTP {status}."));
    }
    result.map_err(|err| format!("Download audiodescrizione Sonarpad interrotto: {err}"))?;
    let headers = response_headers.into_inner();
    headers.validate_complete(written.get())?;
    progress(written.get(), headers.decoded_total());
    temporary.commit(destination, cancelled)
}

fn configured_client(url: &str) -> Result<Easy, String> {
    let mut easy = Easy::new();
    easy.url(url).map_err(|err| err.to_string())?;
    easy.follow_location(true).map_err(|err| err.to_string())?;
    easy.max_redirections(10).map_err(|err| err.to_string())?;
    easy.connect_timeout(CONNECT_TIMEOUT)
        .map_err(|err| err.to_string())?;
    // Large films can take hours on a slow connection. Only connection setup
    // and a stalled transfer are bounded; there is no whole-download timeout.
    easy.low_speed_limit(1).map_err(|err| err.to_string())?;
    easy.low_speed_time(LOW_SPEED_TIMEOUT)
        .map_err(|err| err.to_string())?;
    easy.accept_encoding("gzip, deflate, br")
        .map_err(|err| err.to_string())?;
    easy.cookie_file("").map_err(|err| err.to_string())?;
    easy.pipewait(true).map_err(|err| err.to_string())?;
    easy.fail_on_error(true).map_err(|err| err.to_string())?;
    easy.ssl_verify_peer(true).map_err(|err| err.to_string())?;
    easy.ssl_verify_host(true).map_err(|err| err.to_string())?;
    easy.progress(true).map_err(|err| err.to_string())?;
    easy.signal(false).map_err(|err| err.to_string())?;

    // Keep the same desktop profile as CurlClient::fetch_with_profile.
    let mut headers = List::new();
    for header in [
        "User-Agent: Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
        "Accept: text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8",
        "Accept-Language: it-IT,it;q=0.9,en-US;q=0.8",
        "Cache-Control: no-cache",
        "Pragma: no-cache",
        "Upgrade-Insecure-Requests: 1",
        "Connection: keep-alive",
    ] {
        headers.append(header).map_err(|err| err.to_string())?;
    }
    easy.http_headers(headers).map_err(|err| err.to_string())?;
    Ok(easy)
}

#[derive(Default)]
struct ResponseHeaders {
    status: u32,
    complete: bool,
    content_length: Option<u64>,
    compressed: bool,
    chunked: bool,
    content_type: String,
    content_range: Option<(u64, u64, u64)>,
}

impl ResponseHeaders {
    fn receive(&mut self, data: &[u8]) -> Result<(), String> {
        let line = String::from_utf8_lossy(data);
        let line = line.trim();
        if line.starts_with("HTTP/") {
            // Curl reports every response's headers, including redirects and
            // proxy CONNECT. Only the final response describes the media.
            *self = Self::default();
            self.status = line
                .split_whitespace()
                .nth(1)
                .and_then(|value| value.parse::<u32>().ok())
                .filter(|value| (100..600).contains(value))
                .ok_or("Codice della risposta HTTP non valido.")?;
            return Ok(());
        }
        if line.is_empty() {
            self.complete = true;
            return Ok(());
        }
        if self.complete {
            // HTTP trailers must not change the size or encoding of the body
            // that was already downloaded and reported to the progress UI.
            return Ok(());
        }
        let Some((name, value)) = line.split_once(':') else {
            return Ok(());
        };
        let value = value.trim();
        if name.eq_ignore_ascii_case("Content-Length") {
            let length = value
                .parse::<u64>()
                .map_err(|_| "Dimensione del download non valida nella risposta HTTP.")?;
            if self.content_length.is_some_and(|previous| previous != length) {
                return Err("Il server ha indicato dimensioni discordanti per il download.".to_string());
            }
            self.content_length = Some(length);
        } else if name.eq_ignore_ascii_case("Content-Encoding") {
            self.compressed |= value
                .split(',')
                .any(|encoding| !encoding.trim().eq_ignore_ascii_case("identity"));
        } else if name.eq_ignore_ascii_case("Transfer-Encoding") {
            self.chunked |= value
                .split(',')
                .any(|encoding| encoding.trim().eq_ignore_ascii_case("chunked"));
        } else if name.eq_ignore_ascii_case("Content-Type") {
            self.content_type = value.split(';').next().unwrap_or_default().trim().to_ascii_lowercase();
        } else if name.eq_ignore_ascii_case("Content-Range") {
            self.content_range = parse_content_range(value);
        }
        Ok(())
    }

    fn decoded_total(&self) -> Option<u64> {
        if !self.complete || !(200..300).contains(&self.status) || self.compressed || self.chunked {
            return None;
        }
        self.content_length.filter(|length| *length > 0)
    }

    fn validate_media_response(&self) -> Result<(), String> {
        if matches!(
            self.content_type.as_str(),
            "text/html" | "application/xhtml+xml" | "application/json" | "application/problem+json"
        ) {
            return Err("Il server ha restituito una pagina o un messaggio di errore al posto del file multimediale.".to_string());
        }
        if self.status == 206
            && !self.content_range.is_some_and(|(start, end, total)| {
                start == 0 && end.checked_add(1) == Some(total)
            })
        {
            return Err("Il server ha restituito soltanto una parte del file (HTTP 206).".to_string());
        }
        Ok(())
    }

    fn validate_complete(&self, written: u64) -> Result<(), String> {
        if !self.complete {
            return Err("La risposta HTTP del download è incompleta.".to_string());
        }
        self.validate_media_response()?;
        if written == 0 {
            return Err("Il server ha restituito un file vuoto.".to_string());
        }
        // Curl validates transfer framing, including compressed and chunked
        // transfers. Content-Length describes compressed bytes when decoding
        // is enabled, so compare it with the file size only for plain bodies.
        if !self.compressed && !self.chunked {
            if let Some(expected) = self.content_length
                && written != expected
            {
                return Err(format!(
                    "Download incompleto: ricevuti {written} byte su {expected}."
                ));
            }
        }
        if !self.compressed
            && self.status == 206
            && let Some((_, _, expected)) = self.content_range
            && written != expected
        {
            return Err(format!(
                "Download parziale: ricevuti {written} byte su {expected}."
            ));
        }
        Ok(())
    }
}

fn parse_content_range(value: &str) -> Option<(u64, u64, u64)> {
    let (unit, range) = value.split_once(' ')?;
    if !unit.eq_ignore_ascii_case("bytes") {
        return None;
    }
    let (interval, total) = range.trim().split_once('/')?;
    let (start, end) = interval.split_once('-')?;
    let start = start.parse::<u64>().ok()?;
    let end = end.parse::<u64>().ok()?;
    let total = total.parse::<u64>().ok()?;
    (start <= end && end < total).then_some((start, end, total))
}

struct TemporaryDownload {
    path: PathBuf,
    file: Option<File>,
    committed: bool,
}

impl TemporaryDownload {
    fn create(destination: &Path) -> Result<Self, String> {
        if destination.file_name().is_none() || destination.is_dir() {
            return Err("Scegli un file di destinazione valido per il download.".to_string());
        }
        let parent = destination
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        for _ in 0..8 {
            let path = parent.join(format!(".sonarpad-download-{}.part", uuid::Uuid::new_v4()));
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            match options.open(&path) {
                Ok(file) => return Ok(Self { path, file: Some(file), committed: false }),
                Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(err) => return Err(format!("Impossibile creare il file per il download: {err}")),
            }
        }
        Err("Impossibile creare un file temporaneo univoco per il download.".to_string())
    }

    fn commit(mut self, destination: &Path, cancelled: &AtomicBool) -> Result<bool, String> {
        if cancelled.load(Ordering::Relaxed) {
            return Ok(false);
        }
        let file = self.file.as_mut().ok_or("File temporaneo di download non disponibile.")?;
        file.flush().map_err(|err| format!("Salvataggio del download fallito: {err}"))?;
        file.sync_all().map_err(|err| format!("Salvataggio del download fallito: {err}"))?;
        self.file.take();
        if cancelled.load(Ordering::Relaxed) {
            return Ok(false);
        }
        // The sibling temporary file is on the same filesystem. On macOS,
        // rename atomically replaces an existing file after successful download.
        fs::rename(&self.path, destination)
            .map_err(|err| format!("Impossibile salvare il download nella destinazione scelta: {err}"))?;
        self.committed = true;
        Ok(true)
    }
}

impl Drop for TemporaryDownload {
    fn drop(&mut self) {
        self.file.take();
        if !self.committed {
            let _ = fs::remove_file(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread::{self, JoinHandle};

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("sonarpad-download-test-{}", uuid::Uuid::new_v4()));
            fs::create_dir(&path).unwrap();
            Self(path)
        }

        fn destination(&self) -> PathBuf {
            self.0.join("audiodescrizione.mp3")
        }

        fn assert_no_partials(&self) {
            assert!(fs::read_dir(&self.0).unwrap().all(|entry| {
                !entry.unwrap().file_name().to_string_lossy().ends_with(".part")
            }));
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    // All transport fixtures are served on loopback; tests never contact the
    // real catalog or require credentials. Accept/read timeouts bound failures.
    fn serve(responses: Vec<Vec<u8>>) -> (String, JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}/file", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            let mut requests = Vec::new();
            for response in responses {
                let deadline = Instant::now() + Duration::from_secs(5);
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                            assert!(Instant::now() < deadline, "Download did not connect to fixture");
                            thread::sleep(Duration::from_millis(5));
                        }
                        Err(err) => panic!("Fixture accept failed: {err}"),
                    }
                };
                stream.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
                stream.set_write_timeout(Some(Duration::from_secs(2))).unwrap();
                let mut request = Vec::new();
                let mut buffer = [0_u8; 1024];
                while !request.windows(4).any(|part| part == b"\r\n\r\n") {
                    let count = stream.read(&mut buffer).unwrap();
                    assert!(count > 0 && request.len() < 64 * 1024);
                    request.extend_from_slice(&buffer[..count]);
                }
                requests.push(String::from_utf8_lossy(&request).into_owned());
                // An error or cancellation can close the connection before all
                // fixture bytes are accepted; that is expected for those tests.
                let _ = stream.write_all(&response);
            }
            requests
        });
        (url, server)
    }

    fn plain_response(body: &[u8]) -> Vec<u8> {
        let mut response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: audio/mpeg\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        ).into_bytes();
        response.extend_from_slice(body);
        response
    }

    #[test]
    fn complete_download_writes_the_exact_body_and_reports_real_size() {
        let directory = TestDirectory::new();
        let destination = directory.destination();
        let body = b"complete media payload";
        let (url, server) = serve(vec![plain_response(body)]);
        let mut updates = Vec::new();
        let result = download(&url, &destination, &AtomicBool::new(false), |done, total| {
            updates.push((done, total));
        });
        let requests = server.join().unwrap();
        assert_eq!(result, Ok(true));
        assert_eq!(fs::read(&destination).unwrap(), body);
        assert_eq!(updates.last().copied(), Some((body.len() as u64, Some(body.len() as u64))));
        assert!(updates.iter().all(|(done, _)| *done <= body.len() as u64));
        assert!(requests[0].contains("Chrome/131.0.0.0"));
        assert!(requests[0].contains("Accept-Language: it-IT,it;q=0.9,en-US;q=0.8"));
        directory.assert_no_partials();
    }

    #[test]
    fn redirects_keep_cookies_and_ignore_intermediate_body_and_headers() {
        let directory = TestDirectory::new();
        let redirect = b"HTTP/1.1 302 Found\r\nLocation: /media\r\nSet-Cookie: session=test; Path=/\r\nContent-Type: text/html\r\nContent-Length: 4\r\nConnection: close\r\n\r\nskip".to_vec();
        let (url, server) = serve(vec![redirect, plain_response(b"audio")]);
        let result = download(&url, &directory.destination(), &AtomicBool::new(false), |_, _| {});
        let requests = server.join().unwrap();
        assert_eq!(result, Ok(true));
        assert_eq!(fs::read(directory.destination()).unwrap(), b"audio");
        assert!(requests[1].starts_with("GET /media "));
        assert!(requests[1].to_ascii_lowercase().contains("cookie: session=test"));
        directory.assert_no_partials();
    }

    #[test]
    fn http_errors_empty_truncated_and_html_responses_preserve_existing_destination() {
        for response in [
            b"HTTP/1.1 404 Not Found\r\nContent-Length: 4\r\nConnection: close\r\n\r\nnope".to_vec(),
            b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
            b"HTTP/1.1 200 OK\r\nContent-Length: 20\r\nConnection: close\r\n\r\nshort".to_vec(),
            b"HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: 5\r\nConnection: close\r\n\r\nlogin".to_vec(),
            b"HTTP/1.1 206 Partial Content\r\nContent-Range: bytes 0-19/20\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n5\r\nshort\r\n0\r\n\r\n".to_vec(),
        ] {
            let directory = TestDirectory::new();
            let destination = directory.destination();
            fs::write(&destination, b"original").unwrap();
            let (url, server) = serve(vec![response]);
            let result = download(&url, &destination, &AtomicBool::new(false), |_, _| {});
            server.join().unwrap();
            assert!(result.is_err());
            assert_eq!(fs::read(&destination).unwrap(), b"original");
            directory.assert_no_partials();
        }
    }

    #[test]
    fn cancellation_after_receiving_bytes_preserves_existing_file_and_removes_partial() {
        let directory = TestDirectory::new();
        let destination = directory.destination();
        fs::write(&destination, b"original").unwrap();
        let cancelled = AtomicBool::new(false);
        let (url, server) = serve(vec![plain_response(b"replacement")]);
        let result = download(&url, &destination, &cancelled, |written, _| {
            if written > 0 {
                cancelled.store(true, Ordering::Relaxed);
            }
        });
        server.join().unwrap();
        assert_eq!(result, Ok(false));
        assert_eq!(fs::read(&destination).unwrap(), b"original");
        directory.assert_no_partials();
    }

    #[test]
    fn compressed_and_chunked_downloads_report_bytes_without_a_fabricated_total() {
        let body = b"hello world";
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(body).unwrap();
        let compressed = encoder.finish().unwrap();
        let mut gzip_response = format!(
            "HTTP/1.1 200 OK\r\nContent-Encoding: gzip\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            compressed.len()
        ).into_bytes();
        gzip_response.extend_from_slice(&compressed);
        let chunked_response = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n".to_vec();
        for response in [gzip_response, chunked_response] {
            let directory = TestDirectory::new();
            let (url, server) = serve(vec![response]);
            let mut updates = Vec::new();
            let result = download(&url, &directory.destination(), &AtomicBool::new(false), |done, total| {
                updates.push((done, total));
            });
            server.join().unwrap();
            assert_eq!(result, Ok(true));
            assert_eq!(fs::read(directory.destination()).unwrap(), body);
            assert_eq!(updates.last().copied(), Some((body.len() as u64, None)));
            assert!(updates.iter().all(|(_, total)| total.is_none()));
            directory.assert_no_partials();
        }
    }

    #[test]
    fn partial_http_ranges_are_rejected_and_full_ranges_are_validated() {
        let mut headers = ResponseHeaders::default();
        for line in [
            "HTTP/1.1 206 Partial Content\r\n",
            "Content-Range: bytes 10-19/20\r\n",
            "Content-Length: 10\r\n",
            "\r\n",
        ] {
            headers.receive(line.as_bytes()).unwrap();
        }
        assert!(headers.validate_complete(10).is_err());
        for line in [
            "HTTP/1.1 206 Partial Content\r\n",
            "Content-Range: bytes 0-19/20\r\n",
            "Transfer-Encoding: chunked\r\n",
            "\r\n",
        ] {
            headers.receive(line.as_bytes()).unwrap();
        }
        assert!(headers.validate_complete(20).is_ok());
        assert!(headers.validate_complete(10).is_err());
        headers.receive(b"Content-Encoding: gzip\r\n").unwrap();
        assert!(!headers.compressed, "Trailers must not change body metadata");
        assert!(parse_content_range("bytes 0-9/*").is_none());
        assert!(parse_content_range("bytes 10-5/20").is_none());
    }

    #[test]
    fn cancelled_start_and_failed_rename_leave_no_partial_file() {
        let directory = TestDirectory::new();
        let destination = directory.destination();
        assert_eq!(download("https://example.test/media", &destination, &AtomicBool::new(true), |_, _| {}), Ok(false));
        assert!(!destination.exists());
        directory.assert_no_partials();
        let mut temporary = TemporaryDownload::create(&destination).unwrap();
        temporary.file.as_mut().unwrap().write_all(b"download").unwrap();
        fs::create_dir(&destination).unwrap();
        fs::write(destination.join("keep.txt"), b"keep").unwrap();
        assert!(temporary.commit(&destination, &AtomicBool::new(false)).is_err());
        assert_eq!(fs::read(destination.join("keep.txt")).unwrap(), b"keep");
        directory.assert_no_partials();
    }

    #[cfg(unix)]
    #[test]
    fn successful_commit_replaces_existing_file_only_at_the_end() {
        let directory = TestDirectory::new();
        let destination = directory.destination();
        fs::write(&destination, b"original").unwrap();
        let mut temporary = TemporaryDownload::create(&destination).unwrap();
        temporary.file.as_mut().unwrap().write_all(b"replacement").unwrap();
        assert_eq!(fs::read(&destination).unwrap(), b"original");
        assert_eq!(temporary.commit(&destination, &AtomicBool::new(false)), Ok(true));
        assert_eq!(fs::read(&destination).unwrap(), b"replacement");
        directory.assert_no_partials();
    }
}

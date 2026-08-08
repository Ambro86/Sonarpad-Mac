use crate::audio_description_bridge::BridgeWebRequest;
use reqwest::blocking::Client;
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::fs;
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};
use tokio_tungstenite::tungstenite::{
    self, Message, WebSocket, connect, stream::MaybeTlsStream,
};

const GEMINI_URL: &str = "https://gemini.google.com/app";
const STARTUP_TIMEOUT: Duration = Duration::from_secs(30);
const LOGIN_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const GENERATION_TIMEOUT: Duration = Duration::from_secs(20 * 60);
const SUBMISSION_JS_ACK_TIMEOUT: Duration = Duration::from_secs(5);
const SUBMISSION_MOUSE_ACK_TIMEOUT: Duration = Duration::from_secs(12);

#[derive(Clone, Copy)]
enum BrowserRuntime {
    Chrome,
    Edge,
}

impl BrowserRuntime {
    fn environment_variable(self) -> &'static str {
        match self {
            Self::Chrome => "CHROME_PATH",
            Self::Edge => "EDGE_PATH",
        }
    }

    fn display_name(self) -> &'static str {
        match self {
            Self::Chrome => "Google Chrome",
            Self::Edge => "Microsoft Edge",
        }
    }
}

struct BrowserExecutable {
    runtime: BrowserRuntime,
    path: PathBuf,
}

pub struct GeminiWebSession {
    browser: Option<Child>,
    browser_name: &'static str,
    socket: WebSocket<MaybeTlsStream<TcpStream>>,
    next_message_id: u64,
    pending_events: VecDeque<Value>,
    current_attachment: Option<PathBuf>,
}

fn profile_dir() -> PathBuf {
    let base = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("Library")
        .join("Application Support")
        .join("Sonarpad");
    base.join("gemini_web").join("browser_profile")
}

fn browser_candidates(runtime: BrowserRuntime) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(path) = std::env::var_os(runtime.environment_variable()) {
        candidates.push(PathBuf::from(path));
    }
    match runtime {
        BrowserRuntime::Chrome => {
            candidates.push(PathBuf::from("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"));
            if let Some(home) = std::env::var_os("HOME") {
                candidates.push(PathBuf::from(home).join("Applications/Google Chrome.app/Contents/MacOS/Google Chrome"));
            }
        }
        BrowserRuntime::Edge => {
            candidates.push(PathBuf::from("/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge"));
            if let Some(home) = std::env::var_os("HOME") {
                candidates.push(PathBuf::from(home).join("Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge"));
            }
        }
    }
    candidates
}

fn find_browser() -> Option<BrowserExecutable> {
    for runtime in [BrowserRuntime::Chrome, BrowserRuntime::Edge] {
        if let Some(path) = browser_candidates(runtime)
            .into_iter()
            .find(|path| path.is_file())
        {
            return Some(BrowserExecutable { runtime, path });
        }
    }
    None
}

fn read_devtools_port(profile: &Path) -> Option<u16> {
    let content = fs::read_to_string(profile.join("DevToolsActivePort")).ok()?;
    content.lines().next()?.trim().parse::<u16>().ok()
}

fn page_targets(debug_port: u16) -> Result<Vec<Value>, String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .map_err(|error| error.to_string())?;
    client
        .get(format!("http://127.0.0.1:{debug_port}/json/list"))
        .send()
        .map_err(|error| error.to_string())?
        .json::<Vec<Value>>()
        .map_err(|error| error.to_string())
}

fn browser_websocket(debug_port: u16) -> Result<String, String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .map_err(|error| error.to_string())?;
    let value = client
        .get(format!("http://127.0.0.1:{debug_port}/json/version"))
        .send()
        .map_err(|error| error.to_string())?
        .json::<Value>()
        .map_err(|error| error.to_string())?;
    value
        .get("webSocketDebuggerUrl")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| "Chrome DevTools did not expose a browser WebSocket.".to_string())
}

fn close_stale_automation(profile: &Path) {
    let Some(port) = read_devtools_port(profile) else {
        return;
    };

    if let Ok(url) = browser_websocket(port)
        && let Ok((mut socket, _)) = connect(url.as_str())
    {
        let request = json!({"id": 1, "method": "Browser.close", "params": {}});
        if let Err(error) = socket.send(Message::Text(request.to_string().into())) {
            crate::append_podcast_log(&format!(
                "Gemini Web: could not ask stale automated browser to close: {error}"
            ));
        }
    }

    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline && page_targets(port).is_ok() {
        thread::sleep(Duration::from_millis(100));
    }

    let port_file = profile.join("DevToolsActivePort");
    if let Err(error) = fs::remove_file(port_file)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        crate::append_podcast_log(&format!(
            "Gemini Web: could not remove stale DevToolsActivePort file: {error}"
        ));
    }
}

fn page_websocket(debug_port: u16) -> Result<String, String> {
    for target in page_targets(debug_port)? {
        if target.get("type").and_then(Value::as_str) != Some("page") {
            continue;
        }
        let page_url = target.get("url").and_then(Value::as_str).unwrap_or_default();
        if !page_url.contains("gemini.google.com") && !page_url.contains("accounts.google.com") {
            continue;
        }
        if let Some(url) = target.get("webSocketDebuggerUrl").and_then(Value::as_str) {
            return Ok(url.to_string());
        }
    }
    Err("No Gemini Web page is open in the browser.".to_string())
}

fn wait_for_debug_port(
    child: &mut Child,
    browser_name: &str,
    profile: &Path,
    cancel: &Arc<AtomicBool>,
) -> Result<u16, String> {
    let deadline = Instant::now() + STARTUP_TIMEOUT;
    while Instant::now() < deadline {
        if cancel.load(Ordering::Relaxed) {
            if let Err(error) = child.kill() {
                crate::append_podcast_log(&format!(
                    "Gemini Web: failed to stop browser after cancellation: {error}"
                ));
            }
            return Err("cancelled".to_string());
        }
        if let Some(port) = read_devtools_port(profile)
            && page_targets(port).is_ok()
        {
            return Ok(port);
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                return Err(format!(
                    "{browser_name} exited before Gemini Web automation opened: {status}. If the Gemini Web setup/login window is still open, close it completely and retry."
                ));
            }
            Ok(None) => {}
            Err(error) => return Err(error.to_string()),
        }
        thread::sleep(Duration::from_millis(100));
    }
    Err(format!("Timed out waiting for {browser_name} DevTools."))
}

fn spawn_browser(
    browser: &BrowserExecutable,
    profile: &Path,
) -> Result<Child, String> {
    fs::create_dir_all(profile)
        .map_err(|error| format!("Could not create Gemini Web browser profile: {error}"))?;
    let args = vec![
        "--remote-debugging-port=0".to_string(),
        "--remote-allow-origins=*".to_string(),
        format!("--user-data-dir={}", profile.display()),
        "--no-first-run".to_string(),
        "--no-default-browser-check".to_string(),
        "--disable-translate".to_string(),
        "--disable-features=Translate,TranslateUI".to_string(),
        "--new-window".to_string(),
        GEMINI_URL.to_string(),
    ];
    Command::new(&browser.path)
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("Could not start {}: {error}", browser.runtime.display_name()))
}

fn spawn_setup_browser(
    browser: &BrowserExecutable,
    profile: &Path,
) -> Result<Child, String> {
    fs::create_dir_all(profile)
        .map_err(|error| format!("Could not create Gemini Web browser profile: {error}"))?;

    // Google blocks account sign-in from browsers controlled through software
    // automation. Setup therefore uses an ordinary Chrome/Edge process with the
    // same dedicated profile, but without a DevTools remote-debugging port. Once
    // the user has signed in and closed this setup window, the normal automated
    // session can reopen the same profile and reuse its cookies/session.
    let args = vec![
        format!("--user-data-dir={}", profile.display()),
        "--no-first-run".to_string(),
        "--no-default-browser-check".to_string(),
        "--new-window".to_string(),
        GEMINI_URL.to_string(),
    ];
    Command::new(&browser.path)
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("Could not start {}: {error}", browser.runtime.display_name()))
}

fn ensure_browser(
    cancel: &Arc<AtomicBool>,
) -> Result<(Option<Child>, &'static str, u16), String> {
    let profile = profile_dir();
    fs::create_dir_all(&profile)
        .map_err(|error| format!("Could not create Gemini Web profile: {error}"))?;

    // A DevToolsActivePort left by an earlier failed/cancelled run means an
    // automation browser can still be alive in the background. Reusing it is
    // unsafe: it may point at an old Gemini tab or a half-closed page and make
    // Sonarpad appear stuck while waiting for the prompt editor. Each new
    // audio-description run therefore starts a fresh automated browser.
    close_stale_automation(&profile);

    let browser = find_browser().ok_or_else(|| {
        "Gemini Web requires Google Chrome or Microsoft Edge.".to_string()
    })?;
    let browser_name = browser.runtime.display_name();
    let mut child = spawn_browser(&browser, &profile)?;
    let port = wait_for_debug_port(&mut child, browser_name, &profile, cancel)?;
    Ok((Some(child), browser_name, port))
}

pub fn open_setup_browser() -> Result<(), String> {
    let profile = profile_dir();
    fs::create_dir_all(&profile)
        .map_err(|error| format!("Could not create Gemini Web profile: {error}"))?;
    let browser = find_browser().ok_or_else(|| {
        "Gemini Web requires Google Chrome or Microsoft Edge.".to_string()
    })?;

    // Sign-in must happen in a normal, non-automated browser. Google can block
    // account authentication when Chrome/Edge is running with remote debugging.
    // The same dedicated profile is reused later by the automated session.
    let _browser_process = spawn_setup_browser(&browser, &profile)?;
    Ok(())
}

impl Drop for GeminiWebSession {
    fn drop(&mut self) {
        // Close the dedicated automation browser when a run completes, fails or
        // is cancelled. std::process::Child does not kill a child on Drop, so
        // without this an invisible/stale Chrome instance can survive and be
        // mistaken for the next Gemini Web session.
        let request = json!({
            "id": self.next_message_id,
            "method": "Browser.close",
            "params": {}
        });
        if let Err(error) = self
            .socket
            .send(Message::Text(request.to_string().into()))
        {
            crate::append_podcast_log(&format!(
                "Gemini Web: graceful browser shutdown request failed: {error}"
            ));
        }

        if let Some(child) = self.browser.as_mut() {
            let deadline = Instant::now() + Duration::from_secs(2);
            loop {
                match child.try_wait() {
                    Ok(Some(_)) => break,
                    Ok(None) if Instant::now() < deadline => {
                        thread::sleep(Duration::from_millis(100));
                    }
                    Ok(None) => {
                        if let Err(error) = child.kill() {
                            crate::append_podcast_log(&format!(
                                "Gemini Web: could not stop automated browser: {error}"
                            ));
                        }
                        if let Err(error) = child.wait() {
                            crate::append_podcast_log(&format!(
                                "Gemini Web: could not wait for automated browser shutdown: {error}"
                            ));
                        }
                        break;
                    }
                    Err(error) => {
                        crate::append_podcast_log(&format!(
                            "Gemini Web: could not inspect automated browser process: {error}"
                        ));
                        break;
                    }
                }
            }
        }

        let port_file = profile_dir().join("DevToolsActivePort");
        if let Err(error) = fs::remove_file(port_file)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            crate::append_podcast_log(&format!(
                "Gemini Web: could not remove DevToolsActivePort during shutdown: {error}"
            ));
        }
    }
}

impl GeminiWebSession {
    pub fn start(cancel: &Arc<AtomicBool>) -> Result<Self, String> {
        let (browser, browser_name, debug_port) = ensure_browser(cancel)?;
        let deadline = Instant::now() + STARTUP_TIMEOUT;
        let websocket_url = loop {
            if cancel.load(Ordering::Relaxed) {
                return Err("cancelled".to_string());
            }
            match page_websocket(debug_port) {
                Ok(url) => break url,
                Err(error) if Instant::now() < deadline => {
                    crate::append_podcast_log(&format!(
                        "Gemini Web: waiting for page target: {error}"
                    ));
                    thread::sleep(Duration::from_millis(150));
                }
                Err(error) => return Err(error),
            }
        };
        let (mut socket, _) = connect(websocket_url.as_str())
            .map_err(|error| format!("Gemini Web DevTools connection failed: {error}"))?;
        if let MaybeTlsStream::Plain(stream) = socket.get_mut()
            && let Err(error) = stream.set_read_timeout(Some(Duration::from_millis(100)))
        {
            crate::append_podcast_log(&format!(
                "Gemini Web: could not set DevTools read timeout: {error}"
            ));
        }
        let mut session = Self {
            browser,
            browser_name,
            socket,
            next_message_id: 1,
            pending_events: VecDeque::new(),
            current_attachment: None,
        };
        session.cdp_request(
            "Runtime.enable",
            json!({}),
            Duration::from_secs(10),
            cancel,
        )?;
        session.cdp_request(
            "Page.enable",
            json!({}),
            Duration::from_secs(10),
            cancel,
        )?;
        session.wait_for_prompt_editor(cancel)?;
        Ok(session)
    }

    fn cdp_request(
        &mut self,
        method: &str,
        params: Value,
        timeout: Duration,
        cancel: &Arc<AtomicBool>,
    ) -> Result<Value, String> {
        let id = self.next_message_id;
        self.next_message_id = self.next_message_id.saturating_add(1);
        let request = json!({"id": id, "method": method, "params": params});
        self.socket
            .send(Message::Text(request.to_string().into()))
            .map_err(|error| format!("Gemini Web DevTools send failed: {error}"))?;

        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if cancel.load(Ordering::Relaxed) {
                return Err("cancelled".to_string());
            }
            match self.socket.read() {
                Ok(Message::Text(text)) => {
                    let message: Value = serde_json::from_str(text.as_str())
                        .map_err(|error| format!("Invalid Gemini Web DevTools message: {error}"))?;
                    if message.get("id").and_then(Value::as_u64) != Some(id) {
                        if message.get("method").is_some() {
                            if self.pending_events.len() >= 64 {
                                self.pending_events.pop_front();
                            }
                            self.pending_events.push_back(message);
                        }
                        continue;
                    }
                    if let Some(error) = message.get("error") {
                        return Err(format!("Gemini Web CDP error for {method}: {error}"));
                    }
                    if let Some(details) = message.pointer("/result/exceptionDetails") {
                        return Err(format!("Gemini Web JavaScript error: {details}"));
                    }
                    return Ok(message);
                }
                Ok(Message::Close(_)) => {
                    return Err(format!(
                        "{} closed the Gemini Web DevTools connection.",
                        self.browser_name
                    ));
                }
                Ok(_) => {}
                Err(tungstenite::Error::Io(error))
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) => {}
                Err(error) => {
                    return Err(format!("Gemini Web DevTools read failed: {error}"));
                }
            }
        }
        Err(format!("Gemini Web timed out waiting for {method}."))
    }

    fn evaluate(
        &mut self,
        expression: &str,
        cancel: &Arc<AtomicBool>,
    ) -> Result<Value, String> {
        self.cdp_request(
            "Runtime.evaluate",
            json!({
                "expression": expression,
                "returnByValue": true,
                "awaitPromise": true
            }),
            Duration::from_secs(15),
            cancel,
        )
    }

    fn evaluate_with_user_gesture(
        &mut self,
        expression: &str,
        cancel: &Arc<AtomicBool>,
    ) -> Result<Value, String> {
        self.cdp_request(
            "Runtime.evaluate",
            json!({
                "expression": expression,
                "returnByValue": true,
                "awaitPromise": true,
                "userGesture": true
            }),
            Duration::from_secs(15),
            cancel,
        )
    }

    fn wait_for_event(
        &mut self,
        method: &str,
        timeout: Duration,
        cancel: &Arc<AtomicBool>,
    ) -> Result<Value, String> {
        if let Some(index) = self
            .pending_events
            .iter()
            .position(|message| message.get("method").and_then(Value::as_str) == Some(method))
        {
            return self
                .pending_events
                .remove(index)
                .ok_or_else(|| format!("Gemini Web lost pending CDP event {method}."));
        }

        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if cancel.load(Ordering::Relaxed) {
                return Err("cancelled".to_string());
            }
            match self.socket.read() {
                Ok(Message::Text(text)) => {
                    let message: Value = serde_json::from_str(text.as_str())
                        .map_err(|error| format!("Invalid Gemini Web DevTools message: {error}"))?;
                    if message.get("method").and_then(Value::as_str) == Some(method) {
                        return Ok(message);
                    }
                    if message.get("method").is_some() {
                        if self.pending_events.len() >= 64 {
                            self.pending_events.pop_front();
                        }
                        self.pending_events.push_back(message);
                    }
                }
                Ok(Message::Close(_)) => {
                    return Err(format!(
                        "{} closed the Gemini Web DevTools connection.",
                        self.browser_name
                    ));
                }
                Ok(_) => {}
                Err(tungstenite::Error::Io(error))
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) => {}
                Err(error) => {
                    return Err(format!("Gemini Web DevTools read failed: {error}"));
                }
            }
        }
        Err(format!("Gemini Web timed out waiting for {method}."))
    }

    fn evaluate_value(
        &mut self,
        expression: &str,
        cancel: &Arc<AtomicBool>,
    ) -> Result<Value, String> {
        self.evaluate(expression, cancel)?
            .pointer("/result/result/value")
            .cloned()
            .ok_or_else(|| "Gemini Web JavaScript returned no value.".to_string())
    }

    fn start_fresh_chat(&mut self, cancel: &Arc<AtomicBool>) -> Result<(), String> {
        crate::append_podcast_log("Gemini Web: starting a fresh chat for the next physical video chunk.");
        self.pending_events.clear();
        let response = self.cdp_request(
            "Page.navigate",
            json!({"url": GEMINI_URL}),
            STARTUP_TIMEOUT,
            cancel,
        )?;
        if let Some(error_text) = response
            .pointer("/result/errorText")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
        {
            return Err(format!("Gemini Web could not open a fresh chat: {error_text}"));
        }

        // A full navigation is intentional here: it removes every previously
        // attached clip from Gemini's conversation context. Recovery/repair
        // requests for the same physical chunk do not call this method. Give
        // the old document a moment to detach, then wait on the new composer
        // itself rather than a load event (Gemini sometimes uses SPA routing).
        thread::sleep(Duration::from_millis(250));
        self.wait_for_prompt_editor_for(STARTUP_TIMEOUT, cancel)?;

        let deadline = Instant::now() + STARTUP_TIMEOUT;
        while Instant::now() < deadline {
            if cancel.load(Ordering::Relaxed) {
                return Err("cancelled".to_string());
            }
            let state = self.submission_state(cancel)?;
            let responses = state
                .get("responses")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let user_queries = state
                .get("userQueries")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            if responses == 0 && user_queries == 0 {
                crate::append_podcast_log("Gemini Web: fresh chat ready.");
                return Ok(());
            }
            thread::sleep(Duration::from_millis(250));
        }
        Err("Gemini Web opened a new page but the previous conversation was still present.".to_string())
    }

    fn wait_for_prompt_editor_for(
        &mut self,
        timeout: Duration,
        cancel: &Arc<AtomicBool>,
    ) -> Result<(), String> {
        let expression = r#"
(() => {
  const selectors = [
    '.ql-editor.textarea[contenteditable="true"]',
    '.ql-editor[contenteditable="true"]',
    'rich-textarea [contenteditable="true"]',
    'div[contenteditable="true"][aria-label*="prompt" i]'
  ];
  return selectors.some(selector => document.querySelector(selector));
})()
"#;
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if cancel.load(Ordering::Relaxed) {
                return Err("cancelled".to_string());
            }
            if self
                .evaluate_value(expression, cancel)
                .ok()
                .and_then(|value| value.as_bool())
                == Some(true)
            {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(500));
        }
        Err(
            "Gemini Web is not ready. Sign in to Gemini in the browser window, choose the desired model, then try again."
                .to_string(),
        )
    }

    fn wait_for_prompt_editor(&mut self, cancel: &Arc<AtomicBool>) -> Result<(), String> {
        self.wait_for_prompt_editor_for(LOGIN_TIMEOUT, cancel)
    }

    fn find_file_input_object(
        &mut self,
        cancel: &Arc<AtomicBool>,
    ) -> Result<Option<String>, String> {
        let expression = r#"
(() => {
  const inputs = Array.from(document.querySelectorAll('input[type="file"]'));
  if (!inputs.length) return null;
  return inputs.find(input => {
    const accept = String(input.accept || '').toLowerCase();
    return !input.disabled && (!accept || accept.includes('video') || accept.includes('*/*'));
  }) || inputs[inputs.length - 1];
})()
"#;
        let response = self.cdp_request(
            "Runtime.evaluate",
            json!({
                "expression": expression,
                "returnByValue": false,
                "awaitPromise": true
            }),
            Duration::from_secs(10),
            cancel,
        )?;
        Ok(response
            .pointer("/result/result/objectId")
            .and_then(Value::as_str)
            .map(str::to_string))
    }

    fn click_visible_upload_menu_item(
        &mut self,
        cancel: &Arc<AtomicBool>,
    ) -> Result<bool, String> {
        let expression = r#"
(() => {
  const text = element => [
    element.getAttribute && element.getAttribute('aria-label'),
    element.getAttribute && element.getAttribute('title'),
    element.getAttribute && element.getAttribute('data-tooltip'),
    element.innerText,
    element.textContent
  ].filter(Boolean).join(' ').toLowerCase();

  const visible = element => {
    const style = window.getComputedStyle(element);
    const rect = element.getBoundingClientRect();
    return style.visibility !== 'hidden' && style.display !== 'none' && rect.width > 0 && rect.height > 0;
  };

  const items = Array.from(document.querySelectorAll(
    '[role="menuitem"],[role="option"],button,[role="button"],mat-option,[aria-label]'
  )).filter(visible);
  const upload = items.find(element => {
    const value = text(element);
    return (
      value.includes('upload files') ||
      value.includes('upload file') ||
      value.includes('upload from computer') ||
      value.includes('upload from device') ||
      value.includes('choose file') ||
      value.includes('select file') ||
      value.includes('carica file') ||
      value.includes('carica dal computer') ||
      value.includes('scegli file') ||
      value.includes('seleziona file') ||
      value.includes('dal dispositivo') ||
      value.includes('dal computer') ||
      value.includes('from device') ||
      value.includes('from computer')
    );
  });
  if (upload) {
    upload.click();
    return true;
  }
  return false;
})()
"#;
        Ok(self
            .evaluate_with_user_gesture(expression, cancel)?
            .pointer("/result/result/value")
            .and_then(Value::as_bool)
            .unwrap_or(false))
    }

    fn expose_file_input(&mut self, cancel: &Arc<AtomicBool>) -> Result<bool, String> {
        // If the attachment menu is already open, do not click the + button
        // again because that would close the menu.
        if self.click_visible_upload_menu_item(cancel)? {
            return Ok(true);
        }

        // Prefer controls that live in the same composer as the prompt editor.
        // Gemini has changed the accessible label/icon of the attachment button
        // several times, while its geometric relationship with the editor is
        // much more stable.
        let expression = r#"
(() => {
  const visible = element => {
    if (!element || !element.getBoundingClientRect) return false;
    const style = window.getComputedStyle(element);
    const rect = element.getBoundingClientRect();
    return style.visibility !== 'hidden' && style.display !== 'none' && rect.width > 0 && rect.height > 0;
  };

  const editors = [
    '.ql-editor.textarea[contenteditable="true"]',
    '.ql-editor[contenteditable="true"]',
    'rich-textarea [contenteditable="true"]',
    'div[contenteditable="true"][aria-label*="prompt" i]',
    '[role="textbox"][contenteditable="true"]',
    'textarea'
  ];
  const editor = editors.map(selector => document.querySelector(selector)).find(visible);
  if (!editor) return false;

  const normalizedText = element => [
    element.getAttribute && element.getAttribute('aria-label'),
    element.getAttribute && element.getAttribute('title'),
    element.getAttribute && element.getAttribute('data-tooltip'),
    element.getAttribute && element.getAttribute('data-tooltip-text'),
    element.getAttribute && element.getAttribute('mattooltip'),
    element.innerText,
    element.textContent
  ].filter(Boolean).join(' ').replace(/\s+/g, ' ').trim().toLowerCase();

  const keywords = [
    'add files', 'add file', 'upload file', 'upload menu', 'attach file',
    'upload & tools', 'upload and tools', 'files & tools', 'files and tools',
    'aggiungi file', 'carica file', 'allega file', 'menu di caricamento',
    'menu per caricare', 'caricamento e strumenti', 'aggiungi', 'allega'
  ];

  // First use semantic labels anywhere in the document.
  const allControls = Array.from(document.querySelectorAll(
    'button,[role="button"],[aria-label],[title],[aria-haspopup="menu"],mat-icon'
  )).filter(visible);
  const semantic = allControls.find(element => {
    const value = normalizedText(element);
    return keywords.some(keyword => value.includes(keyword));
  });
  if (semantic) {
    const target = semantic.closest('button,[role="button"]') || semantic;
    target.click();
    return true;
  }

  // Then inspect only controls geometrically close to the prompt. This catches
  // icon-only Gemini buttons whose accessible name is absent from the DOM dump.
  const editorRect = editor.getBoundingClientRect();
  const candidates = allControls
    .map(element => {
      const target = element.closest('button,[role="button"]') || element;
      if (!visible(target)) return null;
      const rect = target.getBoundingClientRect();
      const cy = rect.top + rect.height / 2;
      const editorCy = editorRect.top + editorRect.height / 2;
      const verticalDistance = Math.abs(cy - editorCy);
      const nearComposer = verticalDistance <= Math.max(90, editorRect.height + 30);
      const notFarLeft = rect.right >= editorRect.left - 260;
      const notFarRight = rect.left <= editorRect.right + 260;
      if (!nearComposer || !notFarLeft || !notFarRight) return null;

      const value = normalizedText(target);
      let score = 0;
      if (target.getAttribute && target.getAttribute('aria-haspopup') === 'menu') score += 7;
      if (keywords.some(keyword => value.includes(keyword))) score += 20;
      if (/\b(add|add_2|add_circle|attach_file|upload|upload_file)\b/.test(value)) score += 15;
      if (rect.right <= editorRect.left + 90) score += 4;
      if (rect.width <= 72 && rect.height <= 72) score += 2;
      return {target, score, rect};
    })
    .filter(Boolean)
    .sort((a, b) => b.score - a.score);

  if (candidates.length && candidates[0].score >= 6) {
    candidates[0].target.click();
    return true;
  }
  return false;
})()
"#;
        let opened = self
            .evaluate_with_user_gesture(expression, cancel)?
            .pointer("/result/result/value")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if !opened {
            return Ok(false);
        }

        crate::append_podcast_log("Gemini Web: opened the composer upload/tools control.");

        // The attachment control can either open a menu or directly open the
        // chooser. Give a menu a moment to appear and select its local-file item.
        thread::sleep(Duration::from_millis(650));
        let menu_clicked = self.click_visible_upload_menu_item(cancel)?;
        if menu_clicked {
            crate::append_podcast_log("Gemini Web: selected the local-file item from the upload/tools menu.");
        }
        Ok(true)
    }

    fn prompt_drop_point(
        &mut self,
        cancel: &Arc<AtomicBool>,
    ) -> Result<Option<(f64, f64)>, String> {
        let expression = r#"
(() => {
  const visible = element => {
    if (!element || !element.getBoundingClientRect) return false;
    const style = window.getComputedStyle(element);
    const rect = element.getBoundingClientRect();
    return style.visibility !== 'hidden' && style.display !== 'none' && rect.width > 0 && rect.height > 0;
  };
  const selectors = [
    '.ql-editor.textarea[contenteditable="true"]',
    '.ql-editor[contenteditable="true"]',
    'rich-textarea [contenteditable="true"]',
    'div[contenteditable="true"][aria-label*="prompt" i]',
    '[role="textbox"][contenteditable="true"]',
    'textarea'
  ];
  const editor = selectors.map(selector => document.querySelector(selector)).find(visible);
  if (!editor) return null;
  const rect = editor.getBoundingClientRect();
  return {
    x: Math.max(1, Math.min(window.innerWidth - 2, rect.left + rect.width / 2)),
    y: Math.max(1, Math.min(window.innerHeight - 2, rect.top + rect.height / 2))
  };
})()
"#;
        let value = self.evaluate_value(expression, cancel)?;
        if value.is_null() {
            return Ok(None);
        }
        let x = value.get("x").and_then(Value::as_f64);
        let y = value.get("y").and_then(Value::as_f64);
        Ok(x.zip(y))
    }

    fn deliver_file_via_drag(
        &mut self,
        path: &Path,
        cancel: &Arc<AtomicBool>,
    ) -> Result<bool, String> {
        let Some((x, y)) = self.prompt_drop_point(cancel)? else {
            return Ok(false);
        };
        let data = json!({
            "items": [],
            "files": [path.to_string_lossy().to_string()],
            "dragOperationsMask": 1
        });
        for event_type in ["dragEnter", "dragOver", "drop"] {
            self.cdp_request(
                "Input.dispatchDragEvent",
                json!({
                    "type": event_type,
                    "x": x,
                    "y": y,
                    "data": data.clone()
                }),
                Duration::from_secs(10),
                cancel,
            )?;
            thread::sleep(Duration::from_millis(150));
        }
        Ok(true)
    }

    fn attachment_acknowledged(
        &mut self,
        path: &Path,
        cancel: &Arc<AtomicBool>,
    ) -> Result<bool, String> {
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_lowercase();
        let needle = serde_json::to_string(&file_name).map_err(|error| error.to_string())?;
        let expression = format!(
            r#"
(() => {{
  const needle = {needle};
  if (!needle) return false;
  const inputs = Array.from(document.querySelectorAll('input[type="file"]'));
  if (inputs.some(input => Array.from(input.files || []).some(file => String(file.name || '').toLowerCase() === needle))) {{
    return true;
  }}
  const elements = Array.from(document.querySelectorAll(
    '[aria-label],[title],[data-tooltip],button,[role="button"],mat-chip,.mat-mdc-chip,file-chip,upload-chip'
  ));
  if (elements.some(element => {{
    const value = [
      element.getAttribute && element.getAttribute('aria-label'),
      element.getAttribute && element.getAttribute('title'),
      element.getAttribute && element.getAttribute('data-tooltip'),
      element.textContent
    ].filter(Boolean).join(' ').toLowerCase();
    return value.includes(needle);
  }})) {{
    return true;
  }}
  return String(document.body && document.body.innerText || '').toLowerCase().includes(needle);
}})()
"#
        );
        Ok(self
            .evaluate_value(&expression, cancel)?
            .as_bool()
            .unwrap_or(false))
    }

    fn attachment_rejection(
        &mut self,
        cancel: &Arc<AtomicBool>,
    ) -> Result<Option<String>, String> {
        let expression = r#"
(() => {
  const visible = element => {
    if (!element || !element.getBoundingClientRect) return false;
    const style = window.getComputedStyle(element);
    const rect = element.getBoundingClientRect();
    return style.visibility !== 'hidden' && style.display !== 'none' && rect.width > 0 && rect.height > 0;
  };
  const candidates = Array.from(document.querySelectorAll(
    '[role="alert"],[aria-live="assertive"],[aria-live="polite"],mat-snack-bar-container,.mat-mdc-snack-bar-container'
  )).filter(visible);
  for (const element of candidates) {
    const text = String(element.innerText || element.textContent || '').replace(/\s+/g, ' ').trim();
    const value = text.toLowerCase();
    if (
      value.includes('100 mb') ||
      value.includes('file is too large') ||
      value.includes('file too large') ||
      value.includes('troppo grande') ||
      value.includes('dimensione massima') ||
      value.includes('maximum file size') ||
      value.includes('max file size')
    ) {
      return text;
    }
  }
  return null;
})()
"#;
        let value = self.evaluate_value(expression, cancel)?;
        Ok(value.as_str().map(str::to_string))
    }

    fn wait_for_attachment_ack(
        &mut self,
        path: &Path,
        timeout: Duration,
        cancel: &Arc<AtomicBool>,
    ) -> Result<bool, String> {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if cancel.load(Ordering::Relaxed) {
                return Err("cancelled".to_string());
            }
            if let Some(message) = self.attachment_rejection(cancel)? {
                return Err(format!("Gemini Web rejected the video upload: {message}"));
            }
            if self.attachment_acknowledged(path, cancel)? {
                return Ok(true);
            }
            thread::sleep(Duration::from_millis(300));
        }
        Ok(false)
    }

    fn composer_snapshot(&mut self, cancel: &Arc<AtomicBool>) -> String {
        let expression = r#"
(() => {
  const visible = element => {
    if (!element || !element.getBoundingClientRect) return false;
    const style = window.getComputedStyle(element);
    const rect = element.getBoundingClientRect();
    return style.visibility !== 'hidden' && style.display !== 'none' && rect.width > 0 && rect.height > 0;
  };
  const selectors = [
    '.ql-editor.textarea[contenteditable="true"]',
    '.ql-editor[contenteditable="true"]',
    'rich-textarea [contenteditable="true"]',
    'div[contenteditable="true"][aria-label*="prompt" i]',
    '[role="textbox"][contenteditable="true"]',
    'textarea'
  ];
  const editor = selectors.map(selector => document.querySelector(selector)).find(visible);
  if (!editor) return ['prompt-editor-not-found'];
  const er = editor.getBoundingClientRect();
  const elements = Array.from(document.querySelectorAll(
    'button,[role="button"],[aria-label],[title],[aria-haspopup],mat-icon,.google-symbols'
  )).filter(element => {
    if (!visible(element)) return false;
    const r = element.getBoundingClientRect();
    const cy = r.top + r.height / 2;
    const ecy = er.top + er.height / 2;
    return Math.abs(cy - ecy) < 140 && r.right >= er.left - 320 && r.left <= er.right + 320;
  });
  return elements.slice(0, 40).map(element => {
    const r = element.getBoundingClientRect();
    const label = [
      element.tagName,
      element.getAttribute && element.getAttribute('aria-label'),
      element.getAttribute && element.getAttribute('title'),
      element.getAttribute && element.getAttribute('aria-haspopup'),
      element.getAttribute && element.getAttribute('class'),
      element.textContent
    ].filter(Boolean).join(' ').replace(/\s+/g, ' ').trim();
    return `${label} @${Math.round(r.left)},${Math.round(r.top)},${Math.round(r.width)}x${Math.round(r.height)}`;
  });
})()
"#;
        self.evaluate_value(expression, cancel)
            .ok()
            .and_then(|value| value.as_array().cloned())
            .map(|values| {
                values
                    .into_iter()
                    .filter_map(|value| value.as_str().map(str::to_string))
                    .collect::<Vec<_>>()
                    .join(" | ")
            })
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "unavailable".to_string())
    }

    fn visible_controls_snapshot(&mut self, cancel: &Arc<AtomicBool>) -> String {
        let expression = r#"
(() => Array.from(document.querySelectorAll('button,[role="button"],[role="menuitem"],[aria-label]'))
  .filter(element => {
    const style = window.getComputedStyle(element);
    const rect = element.getBoundingClientRect();
    return style.visibility !== 'hidden' && style.display !== 'none' && rect.width > 0 && rect.height > 0;
  })
  .map(element => [
    element.getAttribute && element.getAttribute('aria-label'),
    element.getAttribute && element.getAttribute('title'),
    element.innerText,
    element.textContent
  ].filter(Boolean).join(' ').replace(/\s+/g, ' ').trim())
  .filter(Boolean)
  .slice(0, 30))()
"#;
        self.evaluate_value(expression, cancel)
            .ok()
            .and_then(|value| value.as_array().cloned())
            .map(|values| {
                values
                    .into_iter()
                    .filter_map(|value| value.as_str().map(str::to_string))
                    .collect::<Vec<_>>()
                    .join(" | ")
            })
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "unavailable".to_string())
    }

    fn set_file_input_object(
        &mut self,
        object_id: String,
        path: &Path,
        cancel: &Arc<AtomicBool>,
    ) -> Result<(), String> {
        self.cdp_request(
            "DOM.setFileInputFiles",
            json!({
                "objectId": object_id,
                "files": [path.to_string_lossy().to_string()]
            }),
            Duration::from_secs(15),
            cancel,
        )?;
        Ok(())
    }

    fn attach_file(&mut self, path: &Path, cancel: &Arc<AtomicBool>) -> Result<(), String> {
        if !path.is_file() {
            return Err(format!("Gemini Web attachment does not exist: {}", path.display()));
        }
        if let Ok(metadata) = fs::metadata(path) {
            crate::append_podcast_log(&format!(
                "Gemini Web: attaching {} ({:.1} MB).",
                path.file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("<unknown>"),
                metadata.len() as f64 / (1024.0 * 1024.0)
            ));
        }

        // Fast path for Gemini versions that keep a file input in the DOM.
        if let Some(object_id) = self.find_file_input_object(cancel)? {
            self.set_file_input_object(object_id, path, cancel)?;
            if self.wait_for_attachment_ack(path, Duration::from_secs(15), cancel)? {
                return Ok(());
            }
            crate::append_podcast_log(
                "Gemini Web: file input accepted the path but no attachment chip was detected; trying alternate upload paths."
            );
        }

        // Prefer Gemini's visible upload/tools menu. On the current Gemini Web UI
        // this is the reliable path, while a synthetic composer drag can take 15
        // seconds to prove that no attachment was created. Keep drag-and-drop only
        // as a last-resort compatibility fallback for future/alternate layouts.
        // Current Gemini builds can create the <input type=file> only while the
        // native chooser is being opened, so intercept it before clicking the menu.
        self.cdp_request(
            "Page.setInterceptFileChooserDialog",
            json!({"enabled": true}),
            Duration::from_secs(10),
            cancel,
        )?;

        let result = (|| {
            let deadline = Instant::now() + Duration::from_secs(20);
            while Instant::now() < deadline {
                if cancel.load(Ordering::Relaxed) {
                    return Err("cancelled".to_string());
                }

                if let Some(object_id) = self.find_file_input_object(cancel)? {
                    self.set_file_input_object(object_id, path, cancel)?;
                    if self.wait_for_attachment_ack(path, Duration::from_secs(15), cancel)? {
                        crate::append_podcast_log(
                            "Gemini Web: attachment acknowledged through the upload/tools path."
                        );
                        return Ok(());
                    }
                }

                let clicked = self.expose_file_input(cancel)?;
                if clicked {
                    match self.wait_for_event(
                        "Page.fileChooserOpened",
                        Duration::from_secs(3),
                        cancel,
                    ) {
                        Ok(event) => {
                            let backend_node_id = event
                                .pointer("/params/backendNodeId")
                                .and_then(Value::as_u64)
                                .ok_or_else(|| {
                                    "Gemini Web opened a file chooser without an input node."
                                        .to_string()
                                })?;
                            self.cdp_request(
                                "DOM.setFileInputFiles",
                                json!({
                                    "backendNodeId": backend_node_id,
                                    "files": [path.to_string_lossy().to_string()]
                                }),
                                Duration::from_secs(15),
                                cancel,
                            )?;
                            if self.wait_for_attachment_ack(
                                path,
                                Duration::from_secs(15),
                                cancel,
                            )? {
                                crate::append_podcast_log(
                                    "Gemini Web: attachment acknowledged through the upload/tools path."
                                );
                                return Ok(());
                            }
                        }
                        Err(error) if error.contains("timed out waiting") => {}
                        Err(error) => return Err(error),
                    }
                }

                thread::sleep(Duration::from_millis(350));
            }

            let controls = self.visible_controls_snapshot(cancel);
            let composer = self.composer_snapshot(cancel);
            Err(format!(
                "Gemini Web could not attach the video. The page layout may have changed. Composer controls: {composer}. Visible controls: {controls}"
            ))
        })();

        if let Err(error) = self.cdp_request(
            "Page.setInterceptFileChooserDialog",
            json!({"enabled": false}),
            Duration::from_secs(10),
            cancel,
        ) {
            crate::append_podcast_log(&format!(
                "Gemini Web: could not disable file chooser interception: {error}"
            ));
        }

        match result {
            Ok(()) => Ok(()),
            Err(menu_error) => {
                crate::append_podcast_log(
                    "Gemini Web: upload/tools menu path did not attach the video; trying composer file drag fallback."
                );
                if self.deliver_file_via_drag(path, cancel)?
                    && self.wait_for_attachment_ack(path, Duration::from_secs(15), cancel)?
                {
                    crate::append_podcast_log(
                        "Gemini Web: attached prepared video by dropping it on the prompt composer."
                    );
                    return Ok(());
                }
                Err(menu_error)
            }
        }
    }

    fn fill_prompt(&mut self, prompt: &str, cancel: &Arc<AtomicBool>) -> Result<(), String> {
        let focus_expression = r#"
(() => {
  const selectors = [
    '.ql-editor.textarea[contenteditable="true"]',
    '.ql-editor[contenteditable="true"]',
    'rich-textarea [contenteditable="true"]',
    'div[contenteditable="true"][aria-label*="prompt" i]'
  ];
  const editor = selectors.map(selector => document.querySelector(selector)).find(Boolean);
  if (!editor) return false;
  editor.focus();
  const selection = window.getSelection();
  const range = document.createRange();
  range.selectNodeContents(editor);
  selection.removeAllRanges();
  selection.addRange(range);
  document.execCommand('delete');
  editor.focus();
  return true;
})()
"#;
        let focused = self
            .evaluate_value(focus_expression, cancel)?
            .as_bool()
            .unwrap_or(false);
        if !focused {
            return Err(
                "Gemini Web prompt box was not found. The Gemini page may have changed."
                    .to_string(),
            );
        }
        self.cdp_request(
            "Input.insertText",
            json!({"text": prompt}),
            Duration::from_secs(30),
            cancel,
        )?;
        Ok(())
    }

    fn response_state(&mut self, cancel: &Arc<AtomicBool>) -> Result<Value, String> {
        let expression = r#"
(() => {
  const responses = Array.from(document.querySelectorAll('model-response'));
  const last = responses.length ? responses[responses.length - 1] : null;
  let text = '';
  if (last) {
    const content = (
      last.querySelector('div.response-content message-content div.markdown.markdown-main-panel') ||
      last.querySelector('message-content.model-response-text div.markdown.markdown-main-panel') ||
      last.querySelector('message-content.model-response-text') ||
      last.querySelector('message-content') ||
      last.querySelector('div.markdown.markdown-main-panel') ||
      last
    );
    text = String(content.innerText || content.textContent || '').trim();
  }

  const controls = Array.from(document.querySelectorAll('button,[role="button"]'));
  const busy = controls.some(element => {
    const value = [
      element.getAttribute && element.getAttribute('aria-label'),
      element.getAttribute && element.getAttribute('title'),
      element.innerText,
      element.textContent
    ].filter(Boolean).join(' ').toLowerCase();
    return (
      value.includes('stop response') ||
      value.includes('stop generating') ||
      value.includes('interrompi risposta') ||
      value.includes('interrompi generazione') ||
      value === 'stop' ||
      value === 'interrompi'
    );
  });

  return {count: responses.length, text, busy};
})()
"#;
        self.evaluate_value(expression, cancel)
    }

    fn click_send_when_ready(&mut self, cancel: &Arc<AtomicBool>) -> Result<(), String> {
        let expression = r#"
(() => {
  const direct = document.querySelector(
    'button.send-button:not([disabled]), button[aria-label*="Send" i]:not([disabled]), button[aria-label*="Invia" i]:not([disabled])'
  );
  if (direct) {
    direct.click();
    return true;
  }

  const buttons = Array.from(document.querySelectorAll('button:not([disabled])'));
  const send = buttons.find(element => {
    const value = [
      element.getAttribute('aria-label'),
      element.getAttribute('title'),
      element.innerText,
      element.textContent
    ].filter(Boolean).join(' ').toLowerCase();
    return value.includes('send message') || value.includes('invia messaggio');
  });
  if (send) {
    send.click();
    return true;
  }
  return false;
})()
"#;
        let deadline = Instant::now() + Duration::from_secs(3 * 60);
        while Instant::now() < deadline {
            if cancel.load(Ordering::Relaxed) {
                return Err("cancelled".to_string());
            }
            if self
                .evaluate_with_user_gesture(expression, cancel)?
                .pointer("/result/result/value")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(400));
        }
        Err(
            "Gemini Web send button did not become available. Check that the video finished uploading and that Gemini is ready."
                .to_string(),
        )
    }

    fn submission_state(&mut self, cancel: &Arc<AtomicBool>) -> Result<Value, String> {
        let expression = r#"
(() => {
  const selectors = [
    '.ql-editor.textarea[contenteditable="true"]',
    '.ql-editor[contenteditable="true"]',
    'rich-textarea [contenteditable="true"]',
    'div[contenteditable="true"][aria-label*="prompt" i]'
  ];
  const editor = selectors.map(selector => document.querySelector(selector)).find(Boolean);
  const promptText = editor
    ? String(editor.innerText || editor.textContent || '').trim()
    : '';

  const responses = document.querySelectorAll('model-response').length;
  const userQueries = document.querySelectorAll('user-query').length;
  const controls = Array.from(document.querySelectorAll('button,[role="button"]'));
  const busy = controls.some(element => {
    const value = [
      element.getAttribute && element.getAttribute('aria-label'),
      element.getAttribute && element.getAttribute('title'),
      element.innerText,
      element.textContent
    ].filter(Boolean).join(' ').toLowerCase();
    return (
      value.includes('stop response') ||
      value.includes('stop generating') ||
      value.includes('interrompi risposta') ||
      value.includes('interrompi generazione') ||
      value === 'stop' ||
      value === 'interrompi'
    );
  });

  return {responses, userQueries, promptText, busy};
})()
"#;
        self.evaluate_value(expression, cancel)
    }

    fn wait_for_submission_ack(
        &mut self,
        previous_response_count: u64,
        previous_user_query_count: u64,
        prompt_was_present: bool,
        timeout: Duration,
        cancel: &Arc<AtomicBool>,
    ) -> Result<bool, String> {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if cancel.load(Ordering::Relaxed) {
                return Err("cancelled".to_string());
            }

            let state = self.submission_state(cancel)?;
            let response_count = state
                .get("responses")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let user_query_count = state
                .get("userQueries")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let prompt_text = state
                .get("promptText")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim();
            let busy = state.get("busy").and_then(Value::as_bool).unwrap_or(false);

            if response_count > previous_response_count
                || user_query_count > previous_user_query_count
                || busy
                || (prompt_was_present && prompt_text.is_empty())
            {
                return Ok(true);
            }

            thread::sleep(Duration::from_millis(250));
        }
        Ok(false)
    }

    fn send_button_point(
        &mut self,
        cancel: &Arc<AtomicBool>,
    ) -> Result<Option<(f64, f64)>, String> {
        let expression = r#"
(() => {
  const visible = element => {
    if (!element || !element.getBoundingClientRect) return false;
    const style = window.getComputedStyle(element);
    const rect = element.getBoundingClientRect();
    return style.visibility !== 'hidden' && style.display !== 'none' && rect.width > 0 && rect.height > 0;
  };
  const text = element => [
    element.getAttribute && element.getAttribute('aria-label'),
    element.getAttribute && element.getAttribute('title'),
    element.innerText,
    element.textContent
  ].filter(Boolean).join(' ').toLowerCase();

  const buttons = Array.from(document.querySelectorAll('button:not([disabled])')).filter(visible);
  const send = buttons.find(element => {
    if (element.matches('button.send-button')) return true;
    const value = text(element);
    return value.includes('send message') || value.includes('invia messaggio');
  });
  if (!send) return null;
  const rect = send.getBoundingClientRect();
  return {
    x: Math.max(1, Math.min(window.innerWidth - 2, rect.left + rect.width / 2)),
    y: Math.max(1, Math.min(window.innerHeight - 2, rect.top + rect.height / 2))
  };
})()
"#;
        let value = self.evaluate_value(expression, cancel)?;
        if value.is_null() {
            return Ok(None);
        }
        let x = value.get("x").and_then(Value::as_f64);
        let y = value.get("y").and_then(Value::as_f64);
        Ok(x.zip(y))
    }

    fn click_send_with_mouse(&mut self, cancel: &Arc<AtomicBool>) -> Result<bool, String> {
        let Some((x, y)) = self.send_button_point(cancel)? else {
            return Ok(false);
        };
        self.cdp_request(
            "Input.dispatchMouseEvent",
            json!({
                "type": "mousePressed",
                "x": x,
                "y": y,
                "button": "left",
                "clickCount": 1
            }),
            Duration::from_secs(10),
            cancel,
        )?;
        self.cdp_request(
            "Input.dispatchMouseEvent",
            json!({
                "type": "mouseReleased",
                "x": x,
                "y": y,
                "button": "left",
                "clickCount": 1
            }),
            Duration::from_secs(10),
            cancel,
        )?;
        Ok(true)
    }

    fn submit_prompt(
        &mut self,
        previous_response_count: u64,
        cancel: &Arc<AtomicBool>,
    ) -> Result<(), String> {
        let before = self.submission_state(cancel)?;
        let previous_user_query_count = before
            .get("userQueries")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let prompt_was_present = before
            .get("promptText")
            .and_then(Value::as_str)
            .is_some_and(|text| !text.trim().is_empty());

        self.click_send_when_ready(cancel)?;
        if self.wait_for_submission_ack(
            previous_response_count,
            previous_user_query_count,
            prompt_was_present,
            SUBMISSION_JS_ACK_TIMEOUT,
            cancel,
        )? {
            crate::append_podcast_log("Gemini Web: send acknowledged by the page.");
            return Ok(());
        }

        crate::append_podcast_log(
            "Gemini Web: send click was not acknowledged; retrying once with a real mouse click."
        );
        if !self.click_send_with_mouse(cancel)? {
            return Err(
                "Gemini Web did not acknowledge the send action and the Send button could not be located for a retry."
                    .to_string(),
            );
        }

        if self.wait_for_submission_ack(
            previous_response_count,
            previous_user_query_count,
            prompt_was_present,
            SUBMISSION_MOUSE_ACK_TIMEOUT,
            cancel,
        )? {
            crate::append_podcast_log("Gemini Web: send acknowledged after mouse-click retry.");
            return Ok(());
        }

        let composer = self.composer_snapshot(cancel);
        Err(format!(
            "Gemini Web did not accept the message after two send attempts. The request was not left waiting indefinitely. Composer controls: {composer}"
        ))
    }

    fn wait_for_response(
        &mut self,
        previous_count: u64,
        cancel: &Arc<AtomicBool>,
    ) -> Result<String, String> {
        let deadline = Instant::now() + GENERATION_TIMEOUT;
        let mut last_text = String::new();
        let mut stable_since: Option<Instant> = None;

        while Instant::now() < deadline {
            if cancel.load(Ordering::Relaxed) {
                return Err("cancelled".to_string());
            }
            let state = self.response_state(cancel)?;
            let count = state.get("count").and_then(Value::as_u64).unwrap_or(0);
            let text = state
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_string();
            let busy = state.get("busy").and_then(Value::as_bool).unwrap_or(false);

            if count > previous_count && !text.is_empty() {
                if text != last_text {
                    last_text = text;
                    stable_since = Some(Instant::now());
                } else if !busy
                    && stable_since
                        .is_some_and(|started| started.elapsed() >= Duration::from_secs(5))
                {
                    return Ok(last_text);
                }
            }
            thread::sleep(Duration::from_millis(500));
        }

        Err(
            "Gemini Web did not finish the response within 20 minutes.".to_string(),
        )
    }

    pub fn generate(
        &mut self,
        request: &BridgeWebRequest,
        cancel: &Arc<AtomicBool>,
    ) -> Result<String, String> {
        self.wait_for_prompt_editor(cancel)?;

        let attachment = request
            .attachment_path
            .as_deref()
            .map(str::trim)
            .filter(|path| !path.is_empty())
            .map(PathBuf::from);
        let attachment_changed = attachment
            .as_ref()
            .is_some_and(|path| self.current_attachment.as_ref() != Some(path));
        if request.fresh_chat || attachment_changed {
            self.start_fresh_chat(cancel)?;
            self.current_attachment = attachment.clone();
        }

        if let Some(path) = attachment.as_deref() {
            self.attach_file(path, cancel)?;
        }

        self.fill_prompt(&request.prompt, cancel)?;
        let previous_count = self
            .response_state(cancel)?
            .get("count")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        self.submit_prompt(previous_count, cancel)?;
        self.wait_for_response(previous_count, cancel)
    }
}

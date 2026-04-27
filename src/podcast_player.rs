#[cfg(target_os = "macos")]
mod imp {
    use serde_json::{Value, json};
    use std::cell::{Cell, RefCell};
    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream;
    use std::path::{Path, PathBuf};
    use std::process::{Child, Command};
    use std::time::Duration;
    use uuid::Uuid;

    const MPV_CONNECT_ATTEMPTS: usize = 150;
    const MPV_CONNECT_DELAY: Duration = Duration::from_millis(100);
    const MPV_IPC_TIMEOUT: Duration = Duration::from_secs(2);

    pub struct PodcastPlayer {
        ipc_path: PathBuf,
        process_id: u32,
        stream_url: String,
        ipc: RefCell<UnixStream>,
        child: RefCell<Option<Child>>,
        next_request_id: Cell<u64>,
    }

    impl PodcastPlayer {
        pub fn new(url: &str) -> Result<Self, String> {
            let stream_url = url.trim();
            if stream_url.is_empty() {
                return Err("URL podcast vuoto".to_string());
            }

            let mpv_executable =
                bundled_mpv_executable_path().unwrap_or_else(|| PathBuf::from("mpv"));
            let mpv_input_conf = bundled_mpv_input_conf_path();
            let enable_bookmarks = crate::media_bookmarks_enabled();
            let mpv_config_dir = crate::prepare_mpv_runtime_dirs(enable_bookmarks)?;
            let ipc_path = podcast_ipc_socket_path();
            remove_stale_socket(&ipc_path, "podcast.mpv.socket_prep_failed")?;

            let mut command = Command::new(&mpv_executable);
            if let Some(parent_dir) = mpv_executable.parent()
                && !parent_dir.as_os_str().is_empty()
            {
                command.current_dir(parent_dir);
            }
            command
                .arg(stream_url)
                .arg(format!("--config-dir={}", mpv_config_dir.display()))
                .arg(format!("--input-conf={}", mpv_input_conf.display()))
                .arg("--pause=yes")
                .arg("--no-video")
                .arg("--force-window=yes")
                .arg("--idle=no")
                .arg("--no-terminal")
                .arg("--osc=yes")
                .arg("--input-default-bindings=yes")
                .arg("--volume-max=300")
                .arg(format!("--input-ipc-server={}", ipc_path.display()))
                .arg("--title=Sonarpad podcast");
            if enable_bookmarks {
                command
                    .arg(format!(
                        "--watch-later-dir={}",
                        crate::mpv_watch_later_dir().display()
                    ))
                    .arg("--save-position-on-quit")
                    .arg("--resume-playback=yes")
                    .arg("--watch-later-options=start");
            } else {
                command.arg("--resume-playback=no");
            }

            let mut child = command
                .spawn()
                .map_err(|err| format!("avvio mpv podcast fallito: {err}"))?;
            activate_mpv_application();

            for attempt in 0..MPV_CONNECT_ATTEMPTS {
                if let Ok(mut ipc) = open_mpv_ipc(&ipc_path) {
                    let handshake = send_mpv_command_with_stream(
                        &mut ipc,
                        &ipc_path,
                        1,
                        json!(["get_property", "pause"]),
                    );
                    if let Err(err) = &handshake {
                        crate::append_podcast_log(&format!(
                            "podcast.mpv.handshake_pending attempt={} path={} err={err}",
                            attempt + 1,
                            ipc_path.display()
                        ));
                    }

                    let process_id = child.id();
                    crate::append_podcast_log(&format!(
                        "podcast.mpv.started pid={process_id} path={} url={} handshake_ok={}",
                        ipc_path.display(),
                        stream_url,
                        handshake.is_ok()
                    ));
                    return Ok(Self {
                        ipc_path,
                        process_id,
                        stream_url: stream_url.to_string(),
                        ipc: RefCell::new(ipc),
                        child: RefCell::new(Some(child)),
                        next_request_id: Cell::new(2),
                    });
                }
                std::thread::sleep(MPV_CONNECT_DELAY);
            }

            crate::append_podcast_log(&format!(
                "podcast.mpv.start_timeout path={} url={stream_url}",
                ipc_path.display()
            ));
            cleanup_failed_child(&mut child, &ipc_path);
            Err("inizializzazione player podcast mpv fallita".to_string())
        }

        pub fn play(&self) -> Result<(), String> {
            self.send_command(json!(["set_property", "pause", false]))?;
            Ok(())
        }

        pub fn debug_snapshot(&self) -> Result<String, String> {
            let pause = self.get_property("pause").unwrap_or(Value::Null);
            let idle_active = self.get_property("idle-active").unwrap_or(Value::Null);
            let core_idle = self.get_property("core-idle").unwrap_or(Value::Null);
            let time_pos = self.get_property_f64("time-pos").ok();
            let duration = self.get_property_f64("duration").ok();
            let eof_reached = self.get_property("eof-reached").unwrap_or(Value::Null);
            Ok(format!(
                "mpv pid={} pause={pause} idle_active={idle_active} core_idle={core_idle} time_pos={time_pos:?} duration={duration:?} eof_reached={eof_reached}",
                self.process_id
            ))
        }

        pub fn is_ready_for_playback(&self) -> Result<bool, String> {
            let pause = self.get_property_bool("pause").unwrap_or(false);
            let idle_active = self.get_property_bool("idle-active").unwrap_or(false);
            let core_idle = self.get_property_bool("core-idle").unwrap_or(false);
            Ok(!pause && !idle_active && !core_idle)
        }

        pub fn pause(&self) -> Result<(), String> {
            self.send_command(json!(["set_property", "pause", true]))?;
            Ok(())
        }

        pub fn seek_by_seconds(&self, offset_seconds: f64) -> Result<(), String> {
            self.send_command(json!(["seek", offset_seconds, "relative", "exact"]))?;
            Ok(())
        }

        pub fn seek_to_seconds(&self, position_seconds: f64) -> Result<(), String> {
            self.send_command(json!([
                "seek",
                position_seconds.max(0.0),
                "absolute",
                "exact"
            ]))?;
            Ok(())
        }

        pub fn duration_seconds(&self) -> Result<Option<f64>, String> {
            match self.get_property("duration")? {
                Value::Number(number) => Ok(number
                    .as_f64()
                    .filter(|value| value.is_finite() && *value > 0.0)),
                Value::Null => Ok(None),
                _ => Ok(None),
            }
        }

        fn send_command(&self, command: Value) -> Result<Value, String> {
            let request_id = self.next_request_id.get();
            self.next_request_id.set(request_id.wrapping_add(1).max(2));
            let mut ipc = self.ipc.borrow_mut();
            send_mpv_command_with_stream(&mut ipc, &self.ipc_path, request_id, command)
        }

        fn get_property(&self, property: &str) -> Result<Value, String> {
            let response = self.send_command(json!(["get_property", property]))?;
            Ok(response.get("data").cloned().unwrap_or(Value::Null))
        }

        fn get_property_bool(&self, property: &str) -> Result<bool, String> {
            match self.get_property(property)? {
                Value::Bool(value) => Ok(value),
                _ => Err(format!("proprietà mpv non booleana: {property}")),
            }
        }

        fn get_property_f64(&self, property: &str) -> Result<f64, String> {
            match self.get_property(property)? {
                Value::Number(number) => number
                    .as_f64()
                    .filter(|value| value.is_finite())
                    .ok_or_else(|| format!("proprietà mpv numerica non valida: {property}")),
                Value::Null => Err(format!("proprietà mpv non disponibile: {property}")),
                _ => Err(format!("proprietà mpv non numerica: {property}")),
            }
        }
    }

    impl Drop for PodcastPlayer {
        fn drop(&mut self) {
            let quit_result = {
                let mut ipc = self.ipc.borrow_mut();
                send_mpv_command_with_stream(&mut ipc, &self.ipc_path, 1, json!(["quit"]))
            };

            if let Some(child) = self.child.borrow_mut().as_mut() {
                if quit_result.is_err()
                    && let Err(err) = child.kill()
                {
                    crate::append_podcast_log(&format!(
                        "podcast.mpv.kill_failed pid={} err={err}",
                        self.process_id
                    ));
                }
                if let Err(err) = child.wait() {
                    crate::append_podcast_log(&format!(
                        "podcast.mpv.wait_failed pid={} err={err}",
                        self.process_id
                    ));
                }
            }
            self.child.borrow_mut().take();

            if let Err(err) = std::fs::remove_file(&self.ipc_path)
                && err.kind() != std::io::ErrorKind::NotFound
            {
                crate::append_podcast_log(&format!(
                    "podcast.mpv.socket_cleanup_failed path={} err={err}",
                    self.ipc_path.display()
                ));
            }
            crate::append_podcast_log(&format!(
                "podcast.mpv.stopped pid={} url={}",
                self.process_id, self.stream_url
            ));
        }
    }

    pub fn bundled_mpv_executable_path() -> Option<PathBuf> {
        let current_exe = std::env::current_exe().ok()?;
        let macos_dir = current_exe.parent()?;
        let contents_dir = macos_dir.parent()?;
        let candidate = contents_dir
            .join("Resources")
            .join("mpv.app")
            .join("Contents")
            .join("MacOS")
            .join("mpv");
        candidate.exists().then_some(candidate)
    }

    fn bundled_mpv_input_conf_path() -> PathBuf {
        std::env::current_exe()
            .ok()
            .and_then(|path| path.parent().map(Path::to_path_buf))
            .and_then(|macos_dir| macos_dir.parent().map(Path::to_path_buf))
            .map(|contents_dir| contents_dir.join("Resources").join("mpv-input.conf"))
            .unwrap_or_else(|| PathBuf::from("mpv-input.conf"))
    }

    fn podcast_ipc_socket_path() -> PathBuf {
        Path::new("/tmp").join(format!("spd-podcast-{}.sock", Uuid::new_v4().simple()))
    }

    fn remove_stale_socket(ipc_path: &Path, log_context: &str) -> Result<(), String> {
        if let Err(err) = std::fs::remove_file(ipc_path)
            && err.kind() != std::io::ErrorKind::NotFound
        {
            crate::append_podcast_log(&format!(
                "{log_context} path={} err={err}",
                ipc_path.display()
            ));
            return Err(format!("preparazione socket mpv podcast fallita: {err}"));
        }
        Ok(())
    }

    fn open_mpv_ipc(ipc_path: &Path) -> Result<UnixStream, String> {
        let stream = UnixStream::connect(ipc_path)
            .map_err(|err| format!("apertura canale mpv podcast fallita: {err}"))?;
        stream
            .set_read_timeout(Some(MPV_IPC_TIMEOUT))
            .map_err(|err| format!("configurazione lettura mpv podcast fallita: {err}"))?;
        stream
            .set_write_timeout(Some(MPV_IPC_TIMEOUT))
            .map_err(|err| format!("configurazione scrittura mpv podcast fallita: {err}"))?;
        Ok(stream)
    }

    fn send_mpv_command_with_stream(
        stream: &mut UnixStream,
        ipc_path: &Path,
        request_id: u64,
        command: Value,
    ) -> Result<Value, String> {
        let message = json!({
            "command": command,
            "request_id": request_id,
        });
        let serialized = serde_json::to_string(&message)
            .map_err(|err| format!("comando mpv podcast non valido: {err}"))?;
        stream
            .write_all(serialized.as_bytes())
            .map_err(|err| format!("invio comando mpv podcast fallito: {err}"))?;
        stream
            .write_all(b"\n")
            .map_err(|err| format!("invio comando mpv podcast fallito: {err}"))?;
        stream
            .flush()
            .map_err(|err| format!("invio comando mpv podcast fallito: {err}"))?;
        read_mpv_response(ipc_path, stream, request_id)
    }

    fn read_mpv_response(
        ipc_path: &Path,
        stream: &mut UnixStream,
        request_id: u64,
    ) -> Result<Value, String> {
        let mut line = Vec::new();
        let mut byte = [0_u8; 1];
        loop {
            line.clear();
            loop {
                let read = stream
                    .read(&mut byte)
                    .map_err(|err| format!("lettura risposta mpv podcast fallita: {err}"))?;
                if read == 0 {
                    return Err("canale mpv podcast chiuso".to_string());
                }
                if byte[0] == b'\n' {
                    break;
                }
                line.push(byte[0]);
            }

            let trimmed = String::from_utf8_lossy(&line);
            let value: Value = serde_json::from_slice(&line)
                .map_err(|err| format!("risposta mpv podcast non valida: {err}"))?;
            if value.get("request_id").and_then(Value::as_u64) == Some(request_id) {
                if value.get("error").and_then(Value::as_str) == Some("success") {
                    return Ok(value);
                }
                return Err(format!("mpv podcast error: {value}"));
            }
            crate::append_podcast_log(&format!(
                "podcast.mpv.ipc.skip path={} expected_request_id={} response={trimmed}",
                ipc_path.display(),
                request_id
            ));
        }
    }

    fn cleanup_failed_child(child: &mut Child, ipc_path: &Path) {
        if let Err(err) = child.kill() {
            crate::append_podcast_log(&format!("podcast.mpv.launch_cleanup_kill_failed err={err}"));
        }
        if let Err(err) = child.wait() {
            crate::append_podcast_log(&format!("podcast.mpv.launch_cleanup_wait_failed err={err}"));
        }
        if let Err(err) = std::fs::remove_file(ipc_path)
            && err.kind() != std::io::ErrorKind::NotFound
        {
            crate::append_podcast_log(&format!(
                "podcast.mpv.launch_cleanup_socket_failed path={} err={err}",
                ipc_path.display()
            ));
        }
    }

    fn activate_mpv_application() {
        let result = Command::new("osascript")
            .args(["-e", "tell application \"mpv\" to activate"])
            .output();
        match result {
            Ok(output) if output.status.success() => {
                crate::append_podcast_log("podcast.mpv.activate.ok");
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                crate::append_podcast_log(&format!(
                    "podcast.mpv.activate.failed code={:?} stdout={} stderr={}",
                    output.status.code(),
                    stdout,
                    stderr
                ));
            }
            Err(err) => {
                crate::append_podcast_log(&format!("podcast.mpv.activate.error err={err}"));
            }
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use std::path::PathBuf;

    pub struct PodcastPlayer;

    impl PodcastPlayer {
        pub fn new(_url: &str) -> Result<Self, String> {
            Err("Player podcast interno disponibile solo su macOS".to_string())
        }

        pub fn play(&self) -> Result<(), String> {
            Err("Player podcast interno disponibile solo su macOS".to_string())
        }

        pub fn debug_snapshot(&self) -> Result<String, String> {
            Err("Player podcast interno disponibile solo su macOS".to_string())
        }

        pub fn is_ready_for_playback(&self) -> Result<bool, String> {
            Err("Player podcast interno disponibile solo su macOS".to_string())
        }

        pub fn pause(&self) -> Result<(), String> {
            Err("Player podcast interno disponibile solo su macOS".to_string())
        }

        pub fn seek_by_seconds(&self, _offset_seconds: f64) -> Result<(), String> {
            Err("Player podcast interno disponibile solo su macOS".to_string())
        }

        pub fn seek_to_seconds(&self, _position_seconds: f64) -> Result<(), String> {
            Err("Player podcast interno disponibile solo su macOS".to_string())
        }

        pub fn duration_seconds(&self) -> Result<Option<f64>, String> {
            Err("Player podcast interno disponibile solo su macOS".to_string())
        }
    }

    pub fn bundled_mpv_executable_path() -> Option<PathBuf> {
        None
    }
}

pub use imp::PodcastPlayer;
pub use imp::bundled_mpv_executable_path;

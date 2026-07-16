use chrono::{Datelike, Local, NaiveDateTime, Timelike};
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use uuid::Uuid;

const RUN_ARGUMENT: &str = "--run-scheduled-tv-job";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ScheduledTvChannel {
    name: String,
    url: String,
    category: String,
    has_guide: bool,
    current_program: Option<String>,
    guide_channel: Option<String>,
    guide_name: Option<String>,
    tvg_id: Option<String>,
    stream_resolver: Option<String>,
    resolver_endpoint: Option<String>,
    resolver_realm: Option<String>,
    resolver_channel_id: Option<String>,
    http_user_agent: Option<String>,
}

impl ScheduledTvChannel {
    fn from_channel(channel: &crate::tv::TvChannel) -> Self {
        Self {
            name: channel.name.clone(),
            url: channel.url.clone(),
            category: channel.category.clone(),
            has_guide: channel.has_guide,
            current_program: channel.current_program.clone(),
            guide_channel: channel.guide_channel.clone(),
            guide_name: channel.guide_name.clone(),
            tvg_id: channel.tvg_id.clone(),
            stream_resolver: channel.stream_resolver.clone(),
            resolver_endpoint: channel.resolver_endpoint.clone(),
            resolver_realm: channel.resolver_realm.clone(),
            resolver_channel_id: channel.resolver_channel_id.clone(),
            http_user_agent: channel.http_user_agent.clone(),
        }
    }

    fn into_channel(self) -> crate::tv::TvChannel {
        crate::tv::TvChannel {
            name: self.name,
            url: self.url,
            category: self.category,
            has_guide: self.has_guide,
            current_program: self.current_program,
            programs: Vec::new(),
            guide_channel: self.guide_channel,
            guide_name: self.guide_name,
            tvg_id: self.tvg_id,
            stream_resolver: self.stream_resolver,
            resolver_endpoint: self.resolver_endpoint,
            resolver_realm: self.resolver_realm,
            resolver_channel_id: self.resolver_channel_id,
            http_user_agent: self.http_user_agent,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ScheduledTvJob {
    id: String,
    channel: ScheduledTvChannel,
    start: String,
    duration_minutes: u32,
    launch_agent_path: String,
    #[serde(default)]
    status: crate::ScheduledRecordingStatus,
}

pub(crate) fn handle_command_line() -> bool {
    let mut arguments = std::env::args_os();
    let _program = arguments.next();
    let Some(argument) = arguments.next() else {
        return false;
    };
    if argument != RUN_ARGUMENT {
        return false;
    }
    let Some(job_path) = arguments.next() else {
        eprintln!("Scheduled TV job path missing");
        return true;
    };
    let path = PathBuf::from(job_path);
    if let Err(error) = run_job_file(&path) {
        eprintln!("Scheduled TV recording failed: {error}");
        crate::append_podcast_log(&format!(
            "tv.schedule.run_failed job={} err={error}",
            path.display()
        ));
    }
    true
}

pub(crate) fn schedule(
    channel: &crate::tv::TvChannel,
    start: NaiveDateTime,
    duration_minutes: u32,
) -> Result<PathBuf, String> {
    if start <= Local::now().naive_local() {
        return Err("L'orario selezionato è già trascorso.".to_string());
    }
    if duration_minutes == 0 {
        return Err("La durata deve essere maggiore di zero.".to_string());
    }
    let id = Uuid::new_v4().simple().to_string();
    let jobs_dir = crate::app_storage_path("scheduled-tv");
    std::fs::create_dir_all(&jobs_dir).map_err(|error| {
        format!("creazione cartella registrazioni programmate fallita: {error}")
    })?;
    let job_path = jobs_dir.join(format!("{id}.json"));
    let home =
        std::env::var_os("HOME").ok_or_else(|| "Cartella Home non disponibile.".to_string())?;
    let launch_agents = PathBuf::from(home).join("Library").join("LaunchAgents");
    std::fs::create_dir_all(&launch_agents)
        .map_err(|error| format!("creazione cartella LaunchAgents fallita: {error}"))?;
    let plist_path = launch_agents.join(format!("com.sonarpad.tv.{id}.plist"));
    let job = ScheduledTvJob {
        id: id.clone(),
        channel: ScheduledTvChannel::from_channel(channel),
        start: start.format("%Y-%m-%d %H:%M:%S").to_string(),
        duration_minutes,
        launch_agent_path: plist_path.to_string_lossy().to_string(),
        status: crate::ScheduledRecordingStatus::Scheduled,
    };
    write_job(&job_path, &job)?;

    let executable = std::env::current_exe()
        .map_err(|error| format!("percorso eseguibile Sonarpad non disponibile: {error}"))?;
    let log_path = jobs_dir.join(format!("{id}.log"));
    let plist = launch_agent_plist(&id, &executable, &job_path, &log_path, start);
    std::fs::write(&plist_path, plist)
        .map_err(|error| format!("scrittura LaunchAgent fallita: {error}"))?;

    let status = Command::new("/bin/launchctl")
        .arg("load")
        .arg(&plist_path)
        .status()
        .map_err(|error| format!("avvio LaunchAgent fallito: {error}"))?;
    if !status.success() {
        if let Err(error) = std::fs::remove_file(&job_path) {
            crate::append_podcast_log(&format!(
                "tv.schedule.cleanup_job_failed path={} err={error}",
                job_path.display()
            ));
        }
        if let Err(error) = std::fs::remove_file(&plist_path) {
            crate::append_podcast_log(&format!(
                "tv.schedule.cleanup_plist_failed path={} err={error}",
                plist_path.display()
            ));
        }
        return Err(format!("launchctl ha restituito lo stato {status}"));
    }
    crate::append_podcast_log(&format!(
        "tv.schedule.created id={} channel={} start={} duration_minutes={} job={} plist={}",
        id,
        channel.name,
        start,
        duration_minutes,
        job_path.display(),
        plist_path.display()
    ));
    Ok(job_path)
}

fn launch_agent_plist(
    id: &str,
    executable: &Path,
    job_path: &Path,
    log_path: &Path,
    start: NaiveDateTime,
) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key><string>com.sonarpad.tv.{id}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{executable}</string>
        <string>{argument}</string>
        <string>{job}</string>
    </array>
    <key>StartCalendarInterval</key>
    <dict>
        <key>Year</key><integer>{year}</integer>
        <key>Month</key><integer>{month}</integer>
        <key>Day</key><integer>{day}</integer>
        <key>Hour</key><integer>{hour}</integer>
        <key>Minute</key><integer>{minute}</integer>
    </dict>
    <key>ProcessType</key><string>Background</string>
    <key>StandardOutPath</key><string>{log}</string>
    <key>StandardErrorPath</key><string>{log}</string>
</dict>
</plist>
"#,
        id = xml_escape(id),
        executable = xml_escape(&executable.to_string_lossy()),
        argument = RUN_ARGUMENT,
        job = xml_escape(&job_path.to_string_lossy()),
        year = start.year(),
        month = start.month(),
        day = start.day(),
        hour = start.hour(),
        minute = start.minute(),
        log = xml_escape(&log_path.to_string_lossy()),
    )
}

fn run_job_file(job_path: &Path) -> Result<(), String> {
    let data = std::fs::read_to_string(job_path)
        .map_err(|error| format!("lettura job {} fallita: {error}", job_path.display()))?;
    let mut job = serde_json::from_str::<ScheduledTvJob>(&data)
        .map_err(|error| format!("job non valido: {error}"))?;
    let launch_agent_path = PathBuf::from(&job.launch_agent_path);
    job.status = crate::ScheduledRecordingStatus::Recording;
    write_job(job_path, &job)?;
    let result = run_job(&job);
    if result.is_ok() {
        remove_file_logged(job_path, "tv.schedule.remove_job_failed");
    } else {
        job.status = crate::ScheduledRecordingStatus::Failed;
        if let Err(error) = write_job(job_path, &job) {
            crate::append_podcast_log(&format!(
                "tv.schedule.write_failed_status_failed job={} err={error}",
                job_path.display()
            ));
        }
    }
    remove_file_logged(&launch_agent_path, "tv.schedule.remove_plist_failed");
    result
}

fn write_job(path: &Path, job: &ScheduledTvJob) -> Result<(), String> {
    let json = serde_json::to_string_pretty(job)
        .map_err(|error| format!("serializzazione registrazione programmata fallita: {error}"))?;
    let temporary_path = path.with_extension("json.tmp");
    std::fs::write(&temporary_path, json)
        .map_err(|error| format!("salvataggio registrazione programmata fallita: {error}"))?;
    std::fs::rename(&temporary_path, path)
        .map_err(|error| format!("aggiornamento registrazione programmata fallita: {error}"))
}

pub(crate) fn entries() -> Vec<crate::ScheduledRecordingEntry> {
    let jobs_dir = crate::app_storage_path("scheduled-tv");
    let Ok(read_dir) = std::fs::read_dir(jobs_dir) else {
        return Vec::new();
    };
    read_dir
        .flatten()
        .filter_map(|item| {
            let path = item.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                return None;
            }
            let data = std::fs::read_to_string(&path).ok()?;
            let job = serde_json::from_str::<ScheduledTvJob>(&data).ok()?;
            let status = effective_status(job.status, &job.start, job.duration_minutes);
            Some(crate::ScheduledRecordingEntry {
                title: job.channel.name,
                kind: "tv".to_string(),
                start: job.start,
                duration_minutes: job.duration_minutes,
                status,
                job_path: path,
            })
        })
        .collect()
}

fn effective_status(
    status: crate::ScheduledRecordingStatus,
    start: &str,
    duration_minutes: u32,
) -> crate::ScheduledRecordingStatus {
    let Ok(start) = NaiveDateTime::parse_from_str(start, "%Y-%m-%d %H:%M:%S") else {
        return crate::ScheduledRecordingStatus::Failed;
    };
    let deadline = start + chrono::Duration::minutes(i64::from(duration_minutes) + 1);
    if status != crate::ScheduledRecordingStatus::Failed && Local::now().naive_local() > deadline {
        crate::ScheduledRecordingStatus::Failed
    } else {
        status
    }
}

pub(crate) fn cancel(job_path: &Path) -> Result<(), String> {
    let data = std::fs::read_to_string(job_path)
        .map_err(|error| format!("lettura registrazione programmata fallita: {error}"))?;
    let job = serde_json::from_str::<ScheduledTvJob>(&data)
        .map_err(|error| format!("registrazione programmata non valida: {error}"))?;
    let plist_path = PathBuf::from(job.launch_agent_path);
    let _ = Command::new("/bin/launchctl")
        .arg("unload")
        .arg(&plist_path)
        .status();
    if plist_path.is_file() {
        std::fs::remove_file(&plist_path)
            .map_err(|error| format!("rimozione LaunchAgent fallita: {error}"))?;
    }
    std::fs::remove_file(job_path)
        .map_err(|error| format!("rimozione registrazione programmata fallita: {error}"))
}

fn scheduled_mpv_executable_path() -> Option<PathBuf> {
    let podcast_player_path = crate::podcast_player::bundled_mpv_executable_path();
    #[cfg(target_os = "macos")]
    {
        podcast_player_path.or_else(crate::bundled_mpv_executable_path)
    }
    #[cfg(not(target_os = "macos"))]
    {
        podcast_player_path
    }
}

fn run_job(job: &ScheduledTvJob) -> Result<(), String> {
    let channel = job.channel.clone().into_channel();
    let resolved_url = crate::tv::resolve_tv_channel_url(&channel)
        .map_err(|error| format!("risoluzione canale fallita: {error}"))?;
    let recordings_dir = crate::default_recordings_dir();
    std::fs::create_dir_all(&recordings_dir)
        .map_err(|error| format!("creazione cartella registrazioni fallita: {error}"))?;
    let safe_title = crate::sanitize_filename(&channel.name);
    let timestamp = Local::now().format("%Y-%m-%d %H-%M-%S");
    let ts_path = recordings_dir.join(format!("{safe_title} - {timestamp}.ts"));
    let mp4_path = ts_path.with_extension("mp4");
    let mpv = scheduled_mpv_executable_path().unwrap_or_else(|| PathBuf::from("mpv"));
    let duration_seconds = job.duration_minutes.saturating_mul(60);
    let mut command = Command::new(&mpv);
    command
        .arg("--no-config")
        .arg("--no-terminal")
        .arg("--force-window=no")
        .arg("--vo=null")
        .arg("--ao=null")
        .arg(format!("--stream-record={}", ts_path.display()))
        .arg(format!("--length={duration_seconds}"));
    let user_agent = channel.playback_user_agent().trim();
    if !user_agent.is_empty() {
        command.arg(format!("--user-agent={user_agent}"));
    }
    command.arg(&resolved_url);
    crate::append_podcast_log(&format!(
        "tv.schedule.record_start id={} channel={} scheduled_start={} duration_seconds={} output={} mpv={}",
        job.id,
        channel.name,
        job.start,
        duration_seconds,
        ts_path.display(),
        mpv.display()
    ));
    let status = command
        .status()
        .map_err(|error| format!("avvio mpv programmato fallito: {error}"))?;
    if !status.success() && !ts_path.is_file() {
        return Err(format!("mpv ha restituito lo stato {status}"));
    }
    if !ts_path.is_file() {
        return Err("La registrazione programmata non ha creato alcun file.".to_string());
    }

    let final_path = match convert_to_mp4(&channel, &ts_path, &mp4_path) {
        Ok(()) => {
            if let Err(error) = std::fs::remove_file(&ts_path) {
                crate::append_podcast_log(&format!(
                    "tv.schedule.remove_ts_failed path={} err={error}",
                    ts_path.display()
                ));
            }
            mp4_path
        }
        Err(error) => {
            crate::append_podcast_log(&format!(
                "tv.schedule.convert_failed source={} err={error}",
                ts_path.display()
            ));
            ts_path
        }
    };
    append_manifest(&final_path, &channel.name)?;
    crate::append_podcast_log(&format!(
        "tv.schedule.record_saved id={} channel={} output={}",
        job.id,
        channel.name,
        final_path.display()
    ));
    Ok(())
}

fn convert_to_mp4(
    channel: &crate::tv::TvChannel,
    source: &Path,
    destination: &Path,
) -> Result<(), String> {
    let ffmpeg = crate::ffmpeg_executable_path().unwrap_or_else(|| PathBuf::from("ffmpeg"));
    let mut command = Command::new(&ffmpeg);
    command
        .arg("-nostdin")
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("warning")
        .arg("-y")
        .arg("-i")
        .arg(source);
    if matches!(channel.name.as_str(), "Rai 1" | "Rai 2" | "Rai 3") {
        command
            .arg("-map")
            .arg("0:v:0?")
            .arg("-map")
            .arg("0:a:2?")
            .arg("-map")
            .arg("0:a:0?")
            .arg("-disposition:a:0")
            .arg("default");
    } else {
        command.arg("-map").arg("0:v:0?").arg("-map").arg("0:a?");
    }
    let output = command
        .arg("-c")
        .arg("copy")
        .arg(destination)
        .output()
        .map_err(|error| format!("avvio ffmpeg fallito: {error}"))?;
    if output.status.success() && destination.is_file() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(if stderr.is_empty() {
            format!("ffmpeg ha restituito lo stato {}", output.status)
        } else {
            stderr
        })
    }
}

fn append_manifest(path: &Path, title: &str) -> Result<(), String> {
    let manifest = crate::recordings_manifest_path();
    if let Some(parent) = manifest.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("creazione cartella manifest fallita: {error}"))?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&manifest)
        .map_err(|error| format!("apertura manifest registrazioni fallita: {error}"))?;
    let saved_at = Local::now().format("%Y-%m-%d %H:%M:%S");
    let clean_title = title.replace(['\t', '\n', '\r'], " ");
    writeln!(
        file,
        "{}\t{}\ttv\t{}",
        path.to_string_lossy(),
        clean_title,
        saved_at
    )
    .map_err(|error| format!("scrittura manifest registrazioni fallita: {error}"))
}

fn remove_file_logged(path: &Path, context: &str) {
    if let Err(error) = std::fs::remove_file(path) {
        crate::append_podcast_log(&format!("{context} path={} err={error}", path.display()));
    }
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

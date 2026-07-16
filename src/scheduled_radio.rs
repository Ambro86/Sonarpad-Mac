use chrono::{Datelike, Local, NaiveDateTime, Timelike};
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use uuid::Uuid;

const RUN_ARGUMENT: &str = "--run-scheduled-radio-job";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ScheduledRadioJob {
    id: String,
    station_name: String,
    stream_url: String,
    start: String,
    duration_minutes: u32,
    launch_agent_path: String,
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
        eprintln!("Scheduled radio job path missing");
        return true;
    };
    let path = PathBuf::from(job_path);
    if let Err(error) = run_job_file(&path) {
        eprintln!("Scheduled radio recording failed: {error}");
        crate::append_podcast_log(&format!(
            "radio.schedule.run_failed job={} err={error}",
            path.display()
        ));
    }
    true
}

pub(crate) fn schedule(
    station_name: &str,
    stream_url: &str,
    start: NaiveDateTime,
    duration_minutes: u32,
) -> Result<PathBuf, String> {
    if start <= Local::now().naive_local() {
        return Err("L'orario selezionato è già trascorso.".to_string());
    }
    if duration_minutes == 0 {
        return Err("La durata deve essere maggiore di zero.".to_string());
    }
    if station_name.trim().is_empty() || stream_url.trim().is_empty() {
        return Err("Nome o indirizzo della radio non valido.".to_string());
    }

    let id = Uuid::new_v4().simple().to_string();
    let jobs_dir = crate::app_storage_path("scheduled-radio");
    std::fs::create_dir_all(&jobs_dir).map_err(|error| {
        format!("creazione cartella registrazioni programmate fallita: {error}")
    })?;
    let job_path = jobs_dir.join(format!("{id}.json"));
    let home =
        std::env::var_os("HOME").ok_or_else(|| "Cartella Home non disponibile.".to_string())?;
    let launch_agents = PathBuf::from(home).join("Library").join("LaunchAgents");
    std::fs::create_dir_all(&launch_agents)
        .map_err(|error| format!("creazione cartella LaunchAgents fallita: {error}"))?;
    let plist_path = launch_agents.join(format!("com.sonarpad.radio.{id}.plist"));
    let job = ScheduledRadioJob {
        id: id.clone(),
        station_name: station_name.trim().to_string(),
        stream_url: stream_url.trim().to_string(),
        start: start.format("%Y-%m-%d %H:%M:%S").to_string(),
        duration_minutes,
        launch_agent_path: plist_path.to_string_lossy().to_string(),
    };
    let json = serde_json::to_string_pretty(&job)
        .map_err(|error| format!("serializzazione registrazione programmata fallita: {error}"))?;
    std::fs::write(&job_path, json)
        .map_err(|error| format!("salvataggio registrazione programmata fallito: {error}"))?;

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
                "radio.schedule.cleanup_job_failed path={} err={error}",
                job_path.display()
            ));
        }
        if let Err(error) = std::fs::remove_file(&plist_path) {
            crate::append_podcast_log(&format!(
                "radio.schedule.cleanup_plist_failed path={} err={error}",
                plist_path.display()
            ));
        }
        return Err(format!("launchctl ha restituito lo stato {status}"));
    }

    crate::append_podcast_log(&format!(
        "radio.schedule.created id={} station={} start={} duration_minutes={} job={} plist={}",
        id,
        station_name,
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
    <key>Label</key><string>com.sonarpad.radio.{id}</string>
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
    let job = serde_json::from_str::<ScheduledRadioJob>(&data)
        .map_err(|error| format!("job non valido: {error}"))?;
    let launch_agent_path = PathBuf::from(&job.launch_agent_path);
    let result = run_job(&job);
    cleanup_job(job_path, &launch_agent_path);
    result
}

fn run_job(job: &ScheduledRadioJob) -> Result<(), String> {
    let recordings_dir = crate::default_recordings_dir();
    std::fs::create_dir_all(&recordings_dir)
        .map_err(|error| format!("creazione cartella registrazioni fallita: {error}"))?;
    let safe_title = crate::sanitize_filename(&job.station_name);
    let timestamp = Local::now().format("%Y-%m-%d %H-%M-%S");
    let output_path = recordings_dir.join(format!("{safe_title} - {timestamp}.mp3"));
    let ffmpeg = crate::ffmpeg_executable_path().unwrap_or_else(|| PathBuf::from("ffmpeg"));
    let duration_seconds = job.duration_minutes.saturating_mul(60);

    crate::append_podcast_log(&format!(
        "radio.schedule.record_start id={} station={} scheduled_start={} duration_seconds={} output={} ffmpeg={}",
        job.id,
        job.station_name,
        job.start,
        duration_seconds,
        output_path.display(),
        ffmpeg.display()
    ));

    let output = Command::new(&ffmpeg)
        .arg("-nostdin")
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("warning")
        .arg("-y")
        .arg("-i")
        .arg(&job.stream_url)
        .arg("-t")
        .arg(duration_seconds.to_string())
        .arg("-vn")
        .arg("-c:a")
        .arg("libmp3lame")
        .arg("-b:a")
        .arg("160k")
        .arg("-f")
        .arg("mp3")
        .arg(&output_path)
        .output()
        .map_err(|error| format!("avvio ffmpeg programmato fallito: {error}"))?;

    let output_is_usable = std::fs::metadata(&output_path)
        .map(|metadata| metadata.len() > 0)
        .unwrap_or(false);
    if !output_is_usable {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            format!("ffmpeg ha restituito lo stato {}", output.status)
        } else {
            stderr
        });
    }
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr)
            .trim()
            .replace(['\n', '\r'], " ");
        crate::append_podcast_log(&format!(
            "radio.schedule.ffmpeg_nonzero_with_output id={} status={} stderr={} output={}",
            job.id,
            output.status,
            stderr,
            output_path.display()
        ));
    }

    append_manifest(&output_path, &job.station_name)?;
    crate::append_podcast_log(&format!(
        "radio.schedule.record_saved id={} station={} output={}",
        job.id,
        job.station_name,
        output_path.display()
    ));
    Ok(())
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
        "{}\t{}\tradio\t{}",
        path.to_string_lossy(),
        clean_title,
        saved_at
    )
    .map_err(|error| format!("scrittura manifest registrazioni fallita: {error}"))
}

fn cleanup_job(job_path: &Path, plist_path: &Path) {
    if let Err(error) = Command::new("/bin/launchctl")
        .arg("unload")
        .arg(plist_path)
        .status()
    {
        crate::append_podcast_log(&format!(
            "radio.schedule.unload_failed plist={} err={error}",
            plist_path.display()
        ));
    }
    if let Err(error) = std::fs::remove_file(job_path) {
        crate::append_podcast_log(&format!(
            "radio.schedule.remove_job_failed path={} err={error}",
            job_path.display()
        ));
    }
    if let Err(error) = std::fs::remove_file(plist_path) {
        crate::append_podcast_log(&format!(
            "radio.schedule.remove_plist_failed path={} err={error}",
            plist_path.display()
        ));
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

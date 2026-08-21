from pathlib import Path

ROOT = Path(__file__).resolve().parent
MAIN = (ROOT / "src" / "main.rs").read_text(encoding="utf-8")
SCHEDULED = (ROOT / "src" / "scheduled_tv.rs").read_text(encoding="utf-8")


def _between(text: str, start: str, end: str) -> str:
    return text.split(start, 1)[1].split(end, 1)[0]


def test_manual_tv_recording_uses_same_mpv_instance_stream_record():
    start_recording = _between(MAIN, "local function start_recording()", "local function stop_recording(announce)")
    assert 'mp.set_property("stream-record", current_recording_path)' in start_recording
    assert "recording.stream_record_started backend=mpv" in start_recording
    assert "ffmpeg_recording_command(current_recording_path)" not in start_recording
    assert "run_shell_detached(command)" not in start_recording


def test_tv_stream_record_uses_transport_stream_container():
    tv_config = _between(MAIN, "    fn tv(url: &str, title: &str) -> Self {", "    fn rai_tv_audio_description")
    rai_config = _between(MAIN, "    fn rai_tv_audio_description", "    fn rai_separate_audio_description")
    assert 'extension: ".ts"' in tv_config
    assert 'extension: ".ts"' in rai_config


def test_manual_tv_recording_is_validated_before_manifest_and_audio_description_queue():
    assert "local function media_validation_command(path)" in MAIN
    assert '" -sseof -3 -i "' in MAIN
    assert "sonarpad_recording_validated" in MAIN
    assert "sonarpad_recording_source_invalid" in MAIN
    assert "sonarpad_recording_source_validated_fallback" in MAIN


def test_manual_stop_flushes_mpv_before_validation_and_remux():
    stop_recording = _between(MAIN, "local function stop_recording(announce)", "local last_recording_toggle_time = 0")
    assert 'mp.set_property("stream-record", "")' in stop_recording
    assert 'local command = "sleep 0.4; " .. finalize_recording_command(saved_path)' in stop_recording
    assert "recording.stream_record_stop_done backend=mpv" in stop_recording


def test_scheduled_tv_recording_uses_mpv_and_validates_output():
    assert "fn scheduled_mpv_executable_path()" in SCHEDULED
    assert "backend=mpv" in SCHEDULED
    assert 'format!("{safe_title} - {timestamp}.ts")' in SCHEDULED
    assert 'format!("--stream-record={}", ts_path.display())' in SCHEDULED
    assert 'format!("--length={duration_seconds}")' in SCHEDULED
    assert "validate_media_file(&ts_path)" in SCHEDULED
    assert "validate_media_file(&final_path)" in SCHEDULED
    assert "tv.schedule.mpv_end" in SCHEDULED


def test_convert_media_logs_spawn_and_exit_diagnostics():
    assert "convert_media.ffmpeg.spawn_failed" in MAIN
    assert "convert_media.ffmpeg.end success=" in MAIN
    assert "status_code={:?}" in MAIN
    assert "stderr_tail={}" in MAIN
    assert "convert_media.ffmpeg.wait_failed" in MAIN

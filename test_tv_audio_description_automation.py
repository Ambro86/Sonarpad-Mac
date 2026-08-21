from pathlib import Path

ROOT = Path(__file__).resolve().parent
MAIN = (ROOT / "src" / "main.rs").read_text(encoding="utf-8")
SCHEDULED = (ROOT / "src" / "scheduled_tv.rs").read_text(encoding="utf-8")
AUDIO_DESCRIPTION = (ROOT / "src" / "audio_description.rs").read_text(encoding="utf-8")
MACOS_BUILD = (ROOT / ".github" / "workflows" / "macos-app-dmg.yml").read_text(encoding="utf-8")
CATALINA_BUILD = (ROOT / ".github" / "workflows" / "macos-app-dmg-catalina.yml").read_text(encoding="utf-8")


def test_setting_is_persistent_and_only_shown_in_italian_settings():
    assert "auto_audio_describe_tv_recordings: bool" in MAIN
    assert 'with_label("Apri Crea audiodescrizione al termine delle registrazioni TV")' in MAIN
    assert 'if settings_before.ui_language == "it"' in MAIN
    assert "auto_audio_describe_tv_recordings_checkbox.show(false);" in MAIN


def test_manual_tv_recordings_enqueue_only_after_a_real_saved_file():
    assert 'if auto_audio_describe_tv_recordings and effective_kind == "tv" then' in MAIN
    assert 'if [ -s " .. shell_quote(path)' in MAIN
    assert "sonarpad_tv_audio_description_queued" in MAIN
    assert 'append_recording_manifest_command_for(mp4_path, recording_title, recording_kind)' in MAIN


def test_scheduled_tv_recordings_enqueue_final_file_and_reopen_sonarpad():
    assert "crate::enqueue_tv_audio_description(&final_path)" in SCHEDULED
    assert "crate::launch_main_app_for_pending_tv_audio_description()" in SCHEDULED
    assert "tv.schedule.audio_description_queued" in SCHEDULED


def test_main_ui_consumes_queue_and_prefills_create_audio_description():
    assert "dequeue_tv_audio_description()" in MAIN
    assert "audio_description_context_timer.open_with_input(&frame_timer, path);" in MAIN
    assert 'app_storage_path("pending-tv-audio-description")' in MAIN


def test_previous_voice_dictionary_and_video_format_fixes_are_preserved():
    assert "crate::apply_voice_dictionary_to_text(text)" in AUDIO_DESCRIPTION
    assert "probe_media_duration_from_packets" in AUDIO_DESCRIPTION
    for workflow in (MACOS_BUILD, CATALINA_BUILD):
        assert "--enable-demuxer=avi" in workflow
        assert "--enable-demuxer=mpeg" in workflow
        assert "--enable-demuxer=flv" in workflow
        assert "--enable-demuxer=asf" in workflow

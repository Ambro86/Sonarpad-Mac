from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SOURCE = (ROOT / 'src' / 'main.rs').read_text(encoding='utf-8')


def section(start: str, end: str) -> str:
    a = SOURCE.index(start)
    b = SOURCE.index(end, a)
    return SOURCE[a:b]


def test_youtube_open_has_no_separate_ytdlp_preflight():
    block = section('fn configure_youtube_mpv_command', 'fn find_youtube_temp_download')
    assert '--skip-download' not in block
    assert 'probe_youtube_stream_playable' not in block
    assert 'command.output()' not in block
    assert 'ytdl_hook-ytdl_path' in block
    assert '.spawn()' in block


def test_youtube_open_progress_waits_for_real_file_loaded():
    block = section('fn wait_for_youtube_mpv_file_loaded', 'fn find_youtube_temp_download')
    assert '--input-ipc-server=' in SOURCE
    assert 'Some("file-loaded")' in block
    assert 'youtube.mpv.file_loaded' in block
    assert 'track-list' in SOURCE


def test_youtube_open_cancel_kills_mpv_and_suppresses_fake_error():
    assert 'Some(Arc::clone(&cancel_requested))' in SOURCE
    assert 'cancel_requested.store(true, Ordering::SeqCst)' in SOURCE
    assert 'child.kill()' in SOURCE
    assert 'err != YOUTUBE_OPEN_CANCELLED' in SOURCE


def test_drm_detection_is_specific_not_generic():
    block = section('fn is_youtube_drm_error', 'fn youtube_drm_message')
    assert 'known to use drm protection' in block
    assert 'drm protected' in block
    assert '[drm]' in block
    assert 'widevine' in block
    assert 'lower.contains("license")' not in block
    assert 'lower.contains("licence")' not in block
    assert 'lower.contains("encrypted")' not in block
    assert 'lower.contains("protected")' not in block


def test_audio_description_download_path_still_exists():
    # Regression guard: this YouTube change must not remove the existing AD flow.
    assert 'youtube_pending_audio_description' in SOURCE
    assert 'audio_context_timer.open_with_input(&dialog_timer, path)' in SOURCE

from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SOURCE = (ROOT / 'src' / 'main.rs').read_text(encoding='utf-8')


def section(start: str, end: str) -> str:
    a = SOURCE.index(start)
    b = SOURCE.index(end, a)
    return SOURCE[a:b]


def test_youtube_open_matches_stable_windows_two_stage_flow():
    preflight = section('fn probe_youtube_stream_playable', 'fn configure_youtube_mpv_command')
    opener = section('fn open_youtube_with_windows_flow', 'fn find_youtube_temp_download')
    assert '--skip-download' in preflight
    assert '.arg("--print")' in preflight
    assert '.arg("id")' in preflight
    assert 'probe_youtube_stream_playable(&ytdlp, url)' in opener
    assert 'open_youtube_with_mpv(url, title)' in opener


def test_youtube_mpv_uses_normal_url_and_stable_windows_format():
    b = section('fn configure_youtube_mpv_command', 'fn spawn_youtube_mpv')
    assert '.arg(url)' in b
    assert 'ytdl_hook-ytdl_path={}' in b
    assert 'best[height<=360][ext=mp4]/18/best[height<=480]/best' in SOURCE
    assert 'ytdl://' not in b
    assert '--no-video' not in b


def test_mac_opening_progress_is_preserved():
    b = section('let choice_open = choice;', 'let choice_save = choice;')
    assert 'open_youtube_open_progress_dialog' in b
    assert 'Some(Arc::clone(&cancel_requested))' in b
    assert 'open_youtube_with_windows_flow' in b
    assert 'err != YOUTUBE_OPEN_CANCELLED' in SOURCE


def test_save_does_not_use_experimental_client_profiles():
    b = section('fn save_youtube_mp3_with_ffmpeg', 'type YoutubeResultsPayload')
    assert 'player_client=' not in b
    assert 'youtube_save_client_profile_count' not in b
    assert '--force-overwrites' not in b


def test_audio_description_download_path_still_exists():
    assert 'youtube_pending_audio_description' in SOURCE
    assert 'audio_context_timer.open_with_input(&dialog_timer, path)' in SOURCE

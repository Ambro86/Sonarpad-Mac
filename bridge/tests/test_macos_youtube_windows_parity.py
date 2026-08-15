import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
MAIN = (ROOT / 'src' / 'main.rs').read_text(encoding='utf-8')


def block(start: str, end: str) -> str:
    a = MAIN.index(start)
    b = MAIN.index(end, a)
    return MAIN[a:b]


class MacYoutubeWindowsParityTests(unittest.TestCase):
    def test_ytdlp_command_matches_windows_encoding_and_stdin(self):
        b = block('fn ytdlp_command(ytdlp: &Path)', '#[cfg(all(target_os = "macos", target_arch = "x86_64"))]')
        self.assertIn('.arg("--encoding").arg("utf-8")', b)
        self.assertIn('command.stdin(Stdio::null())', b)

    def test_open_uses_exact_windows_lightweight_preflight(self):
        b = block('fn probe_youtube_stream_playable', 'fn configure_youtube_mpv_command')
        for expected in ('--no-playlist', '--no-warnings', '--skip-download', '--print', '.arg("id")', '.arg("--")'):
            self.assertIn(expected, b)
        self.assertNotIn('.arg("-g")', b)
        self.assertNotIn('player_client=', b)

    def test_open_passes_original_url_to_mpv_with_windows_format(self):
        b = block('fn configure_youtube_mpv_command', 'fn find_youtube_temp_download')
        self.assertIn('.arg(url)', b)
        self.assertIn('ytdl_hook-ytdl_path={}', b)
        self.assertIn('best[height<=360][ext=mp4]/18/best[height<=480]/best', MAIN)
        self.assertIn('--aid=auto', b)
        self.assertIn('--audio-channels=stereo', b)
        self.assertNotIn('--no-video', b)
        self.assertNotIn('ytdl://', b)
        self.assertNotIn('try_ytdl_first', b)

    def test_preflight_failure_is_logged_but_does_not_block_mpv_like_windows(self):
        b = block('fn open_youtube_with_windows_flow', 'fn find_youtube_temp_download')
        self.assertIn('if let Err(error) = probe_youtube_stream_playable(&ytdlp, url)', b)
        self.assertIn('YouTube preflight failed', b)
        self.assertIn('open_youtube_with_mpv(url, title)', b)

    def test_mac_keeps_opening_progress_and_cancel_only(self):
        b = block('let choice_open = choice;', 'let choice_save = choice;')
        self.assertIn('open_youtube_open_progress_dialog', b)
        self.assertIn('Some(Arc::clone(&cancel_requested))', b)
        self.assertIn('open_youtube_with_windows_flow', b)
        opener = block('fn open_youtube_with_windows_flow', 'fn find_youtube_temp_download')
        self.assertNotIn('file-loaded', opener)
        self.assertNotIn('input-ipc-server', opener)

    def test_save_has_no_mac_only_client_profiles(self):
        b = block('fn save_youtube_mp3_with_ffmpeg', 'type YoutubeResultsPayload')
        self.assertNotIn('youtube_save_client_profile_count', b)
        self.assertNotIn('configure_youtube_save_client_profile', b)
        self.assertNotIn('player_client=', b)
        self.assertNotIn('--force-overwrites', b)

    def test_save_uses_windows_selectors(self):
        b = block('fn save_youtube_mp3_with_ffmpeg', 'type YoutubeResultsPayload')
        self.assertIn('bestaudio[ext=mp3]/bestaudio/best', b)
        self.assertIn('best[ext=mp4]/best', b)
        self.assertIn('best[ext=mp4][height<=720]/best[height<=720]/best', b)
        self.assertIn('--merge-output-format', b)
        self.assertIn('--progress-template', b)

    def test_audio_description_still_uses_same_save_wrapper(self):
        b = block('let choice_audio_description = choice;', 'let choice_favorite = choice;')
        self.assertIn('open_youtube_save_progress_dialog', b)
        self.assertIn('save_youtube_to_path(&url, "mp4", &quality, output_path)', b)


if __name__ == '__main__':
    unittest.main()

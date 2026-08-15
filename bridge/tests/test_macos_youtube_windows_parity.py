import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
MAIN = (ROOT / 'src' / 'main.rs').read_text(encoding='utf-8')


class MacYoutubeWindowsParityTests(unittest.TestCase):
    def test_drm_detection_is_narrow_and_does_not_use_generic_words(self):
        start = MAIN.index('fn is_youtube_drm_error')
        end = MAIN.index('fn youtube_cancelled_message', start)
        block = MAIN[start:end]
        self.assertIn('known to use drm protection', block)
        self.assertIn('uses drm protection', block)
        self.assertIn('[drm]', block)
        self.assertNotIn('lower.contains("protected")', block)
        self.assertNotIn('lower.contains("license")', block)
        self.assertNotIn('lower.contains("encrypted")', block)

    def test_open_uses_windows_style_lightweight_preflight(self):
        start = MAIN.index('fn probe_youtube_stream_playable_fast')
        end = MAIN.index('fn open_youtube_with_mpv_controlled', start)
        block = MAIN[start:end]
        self.assertIn('.arg("--skip-download")', block)
        self.assertIn('.arg("--print")', block)
        self.assertIn('.arg("id")', block)
        self.assertNotIn('.arg("-g")', block)
        self.assertNotIn('bestaudio/best', block)
        self.assertNotIn('--fragment-retries', block)
        self.assertIn('youtube_save_client_profile_count()', block)
        self.assertIn('if profile > 0', block)

    def test_open_passes_original_youtube_url_to_mpv(self):
        start = MAIN.index('fn open_youtube_with_mpv_controlled')
        end = MAIN.index('fn find_youtube_temp_download', start)
        block = MAIN[start:end]
        self.assertIn('probe_youtube_stream_playable_fast(url', block)
        self.assertIn('open_stream_with_mpv(url, title, None, true)', block)
        self.assertNotIn('playback_url', block)

    def test_mpv_receives_windows_ytdlp_hook_and_format_for_youtube(self):
        self.assertIn(
            '"best[height<=360][ext=mp4]/18/best[height<=480]/best"', MAIN
        )
        start = MAIN.index('fn open_stream_with_mpv_recordable')
        end = MAIN.index('match command.spawn()', start)
        block = MAIN[start:end]
        self.assertIn('if is_youtube_url(url)', block)
        self.assertIn('--script-opts=ytdl_hook-ytdl_path={}', block)
        self.assertIn('--ytdl-format={YOUTUBE_MPV_STREAM_FORMAT}', block)

    def test_open_progress_has_cancel_and_worker_receives_cancel_flag(self):
        progress_start = MAIN.index('fn open_youtube_open_progress_dialog')
        progress_end = MAIN.index('fn open_youtube_results_dialog', progress_start)
        progress_block = MAIN[progress_start:progress_end]
        self.assertIn('true,', progress_block)
        self.assertIn('cancel_button', progress_block)
        self.assertIn('event.skip(false)', progress_block)

        click_start = MAIN.index('let choice_open = choice;')
        click_end = MAIN.index('let choice_save = choice;', click_start)
        click_block = MAIN[click_start:click_end]
        self.assertIn('let cancel_flag = progress_dialog.cancel_flag();', click_block)
        self.assertIn('open_youtube_with_mpv_controlled', click_block)
        self.assertIn('Some(cancel_flag)', click_block)

    def test_normal_save_uses_real_ytdlp_progress_and_cancel(self):
        save_start = MAIN.index('fn save_youtube_to_path_with_control')
        save_end = MAIN.index('type YoutubeResultsPayload', save_start)
        save_block = MAIN[save_start:save_end]
        self.assertIn('--progress-template', save_block)
        self.assertIn('ytdlp_download_progress_template()', save_block)
        self.assertIn('run_ytdlp_cancellable', save_block)
        self.assertIn('--merge-output-format', save_block)
        self.assertIn('bestvideo[ext=mp4]+bestaudio[ext=m4a]', save_block)

        click_start = MAIN.index('let choice_save = choice;')
        click_end = MAIN.index('let choice_audio_description = choice;', click_start)
        click_block = MAIN[click_start:click_end]
        self.assertIn('open_youtube_standard_save_progress_dialog', click_block)
        self.assertIn('actual_progress_flag()', click_block)
        self.assertIn('save_youtube_to_path_with_control', click_block)

    def test_audio_description_download_flow_remains_on_existing_wrapper(self):
        start = MAIN.index('let choice_audio_description = choice;')
        end = MAIN.index('let choice_favorite = choice;', start)
        block = MAIN[start:end]
        self.assertIn('open_youtube_save_progress_dialog', block)
        self.assertIn('save_youtube_to_path(&url, "mp4", &quality, output_path)', block)
        self.assertNotIn('save_youtube_to_path_with_control', block)


if __name__ == '__main__':
    unittest.main()

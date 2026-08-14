import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
MAIN = (ROOT / "src" / "main.rs").read_text(encoding="utf-8")
AUDIO = (ROOT / "src" / "audio_description.rs").read_text(encoding="utf-8")


class MacStreamAudioDescriptionLaunchTests(unittest.TestCase):
    def test_youtube_button_is_localized_and_sits_after_save(self):
        start = MAIN.index("fn open_youtube_results_dialog")
        end = MAIN.index("fn open_youtube_dialog(", start)
        block = MAIN[start:end]
        self.assertIn("create_audio_description_button", block)
        self.assertIn("audio_description::menu_label()", block)
        self.assertLess(
            block.index("buttons.add(&save_button"),
            block.index("buttons.add(&create_audio_description_button"),
        )

    def test_youtube_audio_description_always_downloads_video_with_progress(self):
        start = MAIN.index("let choice_audio_description = choice;")
        end = MAIN.index("let choice_favorite = choice;", start)
        block = MAIN[start:end]
        self.assertIn('prompt_youtube_save_path(&dialog_audio_description, &result.title, "mp4")', block)
        self.assertIn('save_youtube_to_path(&url, "mp4", &quality, output_path)', block)
        self.assertIn("open_youtube_save_progress_dialog", block)

    def test_youtube_success_message_precedes_prefilled_audio_description_dialog(self):
        start = MAIN.index("let audio_description_result = youtube_pending_audio_description_timer")
        end = MAIN.index("youtube_result_timer.start", start)
        block = MAIN[start:end]
        self.assertLess(block.index("show_message_subdialog"), block.index("open_with_input"))

    def test_raiplay_and_la7_buttons_use_existing_ffmpeg_progress_and_open_prefilled_dialog(self):
        for function_name, helper_name in [
            ("fn open_raiplay_items_modal", "save_raiplay_video_for_audio_description"),
            ("fn open_la7_play_items_modal", "save_la7_video_for_audio_description"),
        ]:
            start = MAIN.index(function_name)
            next_fn = MAIN.find("\nfn ", start + len(function_name))
            block = MAIN[start:next_fn]
            self.assertIn("create_audio_description_button", block)
            self.assertIn(helper_name, block)
            self.assertLess(block.index("show_message_subdialog"), block.index("open_with_input"))
        self.assertIn("run_ffmpeg_save(parent", MAIN[MAIN.index("fn save_la7_video_for_audio_description"):])
        rai_helper = MAIN[MAIN.index("fn save_raiplay_video_for_audio_description"):]
        self.assertIn("run_ffmpeg_save(", rai_helper)

    def test_prefilled_create_dialog_sets_both_source_and_destination(self):
        start = AUDIO.index("pub fn open_create_dialog_with_input")
        end = AUDIO.index("fn format_mmss", start)
        block = AUDIO[start:end]
        self.assertIn("input.set_value", block)
        self.assertIn("_audiodescritto.mp3", block)
        self.assertIn("output.set_value", block)


if __name__ == "__main__":
    unittest.main()

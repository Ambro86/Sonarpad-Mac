from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]
MAIN = (ROOT / "src" / "main.rs").read_text(encoding="utf-8")


class MacConvertProgressViewTests(unittest.TestCase):
    def test_single_conversion_shows_only_percentage_and_stop_while_running(self):
        start = MAIN.index("fn open_convert_media_dialog")
        end = MAIN.index("fn convert_media_supported_input", start)
        block = MAIN[start:end]
        self.assertIn('status_text_convert.set_label("0%")', block)
        self.assertIn('status_text_timer.set_label(&format!("{}%", snapshot.0.clamp(0, 99)))', block)
        self.assertIn("cancel_button.show(true)", block)
        self.assertIn("cancel_button.set_focus()", block)
        for token in (
            "input_label.show(false)",
            "output_label.show(false)",
            "image_label.show(false)",
            "format_label.show(false)",
            "bitrate_label.show(false)",
            "ogg_label.show(false)",
            "flac_label.show(false)",
            "wav_label.show(false)",
            "convert_button.show(false)",
            "close_button.show(false)",
        ):
            self.assertIn(token, block)

    def test_folder_conversion_uses_overall_percentage_and_stop_only_view(self):
        start = MAIN.index("fn open_convert_media_folder_dialog")
        end = MAIN.index("fn italian_directories_code_available", start)
        block = MAIN[start:end]
        self.assertIn('status_text_convert.set_label("0%")', block)
        self.assertIn("let overall =", block)
        self.assertIn('status_text_timer.set_label(&format!("{overall}%"))', block)
        self.assertIn("cancel_button.show(true)", block)
        self.assertIn("cancel_button.set_focus()", block)
        self.assertIn("current_file_percent.store(0, Ordering::SeqCst)", MAIN)


if __name__ == "__main__":
    unittest.main()

import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
MAIN = (ROOT / "src" / "main.rs").read_text(encoding="utf-8")
WORKFLOW = (ROOT / ".github" / "workflows" / "macos-app-dmg.yml").read_text(encoding="utf-8")
CATALINA = (ROOT / ".github" / "workflows" / "macos-app-dmg-catalina.yml").read_text(encoding="utf-8")


def block(start: str, end: str) -> str:
    a = MAIN.index(start)
    b = MAIN.index(end, a)
    return MAIN[a:b]


class MacYoutubeSaveFormatsTests(unittest.TestCase):
    def test_format_menu_exposes_supported_audio_and_video_targets(self):
        self.assertIn(
            '''const YOUTUBE_SAVE_FORMATS: &[&str] = &[
    "mp3", "mp4", "avi", "mov", "mkv", "wav", "aiff", "m4a", "m4b", "flac",
];''',
            MAIN,
        )
        dialog = block("fn open_youtube_results_dialog", "struct NewCalendarReminder")
        self.assertIn("for format in YOUTUBE_SAVE_FORMATS", dialog)
        self.assertIn("YOUTUBE_SAVE_FORMATS\n                .get(format_index)", dialog)

    def test_save_dialog_uses_selected_extension(self):
        prompt = block("fn prompt_youtube_save_path", "fn youtube_save_completed_message")
        self.assertIn("YOUTUBE_SAVE_FORMATS", prompt)
        self.assertIn('format!("{}.{}", sanitize_filename(title), extension)', prompt)

    def test_non_mp4_formats_are_converted_by_bundled_ffmpeg(self):
        converted = block("fn save_youtube_converted_with_ffmpeg", "fn save_youtube_to_path")
        for expected in (
            '"wav" =>',
            '"aiff" =>',
            '"m4a" | "m4b" =>',
            '"flac" =>',
            '"avi" =>',
            '"mov" =>',
            '"mkv" =>',
            '"pcm_s16le"',
            '"aac"',
            '"flac"',
            '"mpeg4"',
            '"libmp3lame"',
            '"matroska"',
        ):
            self.assertIn(expected, converted)
        save = block("fn save_youtube_to_path", "type YoutubeResultsPayload")
        self.assertIn('if format != "mp4"', save)
        self.assertIn("save_youtube_converted_with_ffmpeg", save)

    def test_both_macos_ffmpeg_workflows_guarantee_required_outputs(self):
        for workflow in (WORKFLOW, CATALINA):
            for expected in (
                "--enable-muxer=mp3",
                "--enable-muxer=mp4",
                "--enable-muxer=avi",
                "--enable-muxer=mov",
                "--enable-muxer=matroska",
                "--enable-muxer=wav",
                "--enable-muxer=ipod",
                "--enable-muxer=flac",
                "--enable-muxer=aiff",
                "--enable-encoder=libmp3lame",
                "--enable-encoder=aac",
                "--enable-encoder=mpeg4",
                "--enable-encoder=pcm_s16le",
                "--enable-encoder=pcm_s16be",
                "--enable-encoder=flac",
            ):
                self.assertIn(expected, workflow)
            for runtime_check in (
                "[[:space:]]mov[[:space:]]",
                "[[:space:]]ipod[[:space:]]",
                "[[:space:]]flac[[:space:]]",
                "[[:space:]]aac[[:space:]]",
                "[[:space:]]libmp3lame[[:space:]]",
            ):
                self.assertIn(runtime_check, workflow)


if __name__ == "__main__":
    unittest.main()

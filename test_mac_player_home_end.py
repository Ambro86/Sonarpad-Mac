from pathlib import Path
import unittest

MAIN = (Path(__file__).parent / "src" / "main.rs").read_text(encoding="utf-8")


class MacPlayerHomeEndTests(unittest.TestCase):
    def test_home_seeks_to_absolute_start(self):
        self.assertIn('local function seek_to_start_with_speech()', MAIN)
        self.assertIn('mp.commandv("seek", "0", "absolute+exact")', MAIN)
        self.assertIn('mp.add_forced_key_binding("HOME", "sonarpad-seek-start-speech", seek_to_start_with_speech)', MAIN)

    def test_end_seeks_five_seconds_before_duration(self):
        self.assertIn('local target = math.max(0, duration - 5)', MAIN)
        self.assertIn('mp.commandv("seek", tostring(target), "absolute+exact")', MAIN)
        self.assertIn('mp.add_forced_key_binding("END", "sonarpad-seek-end-speech", seek_to_end_with_speech)', MAIN)

    def test_non_seekable_media_is_not_forced(self):
        self.assertGreaterEqual(MAIN.count('if not seekable'), 2)
        self.assertIn('speak(msg_live_stream)', MAIN)

    def test_existing_relative_arrow_seek_is_unchanged(self):
        self.assertIn('mp.add_forced_key_binding("RIGHT", "sonarpad-seek-forward-speech", function() seek_with_speech(seek_step_seconds) end, {repeatable = true})', MAIN)
        self.assertIn('mp.add_forced_key_binding("LEFT", "sonarpad-seek-backward-speech", function() seek_with_speech(-seek_step_seconds) end, {repeatable = true})', MAIN)


if __name__ == "__main__":
    unittest.main()

from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[2]
MAIN = (ROOT / "src" / "main.rs").read_text(encoding="utf-8")


class MacOSMainEditorAccessibilityTests(unittest.TestCase):
    def test_main_editor_hides_native_vertical_scrollbar_from_voiceover(self):
        pattern = re.compile(
            r"let\s+text_ctrl\s*=\s*TextCtrl::builder\(&panel\)"
            r".*?\.with_style\(\s*TextCtrlStyle::MultiLine\s*\|\s*TextCtrlStyle::NoVScroll\s*\)"
            r".*?text_ctrl\.set_accessibility_label\(&ui\.editor_text_label\);",
            re.S,
        )
        self.assertRegex(MAIN, pattern)

    def test_only_main_editor_uses_no_vscroll(self):
        self.assertEqual(MAIN.count("TextCtrlStyle::NoVScroll"), 1)


if __name__ == "__main__":
    unittest.main()

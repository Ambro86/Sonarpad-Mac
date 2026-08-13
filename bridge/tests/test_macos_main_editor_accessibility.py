from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[2]
MAIN = (ROOT / "src" / "main.rs").read_text(encoding="utf-8")


class MacOSMainEditorAccessibilityTests(unittest.TestCase):
    def test_main_editor_restores_plain_multiline_voiceover_behavior(self):
        pattern = re.compile(
            r"let\s+text_ctrl\s*=\s*TextCtrl::builder\(&panel\)"
            r".*?\.with_style\(TextCtrlStyle::MultiLine\)"
            r".*?\.build\(\);",
            re.S,
        )
        self.assertRegex(MAIN, pattern)
        anchor = "let main_sizer = BoxSizer::builder(Orientation::Vertical).build();"
        start = MAIN.index("let text_ctrl = TextCtrl::builder(&panel)", MAIN.index(anchor))
        end = MAIN.index("let cursor_moved_by_user", start)
        editor_block = MAIN[start:end]
        self.assertNotIn("set_accessibility_label", editor_block)

    def test_main_editor_does_not_use_no_vscroll(self):
        self.assertNotIn("TextCtrlStyle::NoVScroll", MAIN)
        self.assertNotIn("editor_text_label", MAIN)


if __name__ == "__main__":
    unittest.main()

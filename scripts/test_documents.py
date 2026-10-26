"""Local document lifecycle tests; no macOS GUI or network required."""
import json
from pathlib import Path
import subprocess
import sys
import unittest

ROOT = Path(__file__).resolve().parents[1]


class DocumentTests(unittest.TestCase):
    def test_document_navigation_is_in_file_menu(self):
        source = (ROOT / "src/main.rs").read_text(encoding="utf-8")
        file_menu = source[source.index("let file_menu = Menu::builder()"):]
        file_menu = file_menu[:file_menu.index("let help_menu = Menu::builder()")]
        for command in ("NEXT", "PREVIOUS"):
            self.assertIn(f"file_menu.append_command(\n            documents::ID_{command}_DOCUMENT,", file_menu)

    def test_production_document_controller(self):
        executable = ROOT / "target" / (
            "document_lifecycle_tests.exe" if sys.platform == "win32" else "document_lifecycle_tests"
        )
        executable.parent.mkdir(exist_ok=True)
        subprocess.run([
            "rustc", "--edition", "2024", "--test",
            str(ROOT / "scripts/document_lifecycle_tests.rs"), "-o", str(executable),
        ], check=True)
        subprocess.run([str(executable)], check=True)

    def test_labels_and_release_notes_in_every_language(self):
        for language in ("it", "en", "fr", "es", "pt", "cs", "pl"):
            with self.subTest(language=language):
                labels = json.loads((ROOT / f"i18n/ui_{language}.json").read_text(encoding="utf-8"))
                for key in ("new", "close", "next", "previous", "untitled"):
                    self.assertTrue(labels[f"document_{key}"].strip())
                latest = (ROOT / f"changelog_{language}.md").read_text(encoding="utf-8").split("0.5.2")[0]
                self.assertIn("Command+<", latest)
                self.assertIn("Command+>", latest)


if __name__ == "__main__":
    unittest.main()

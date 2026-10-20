"""Update download checks runnable without macOS or a real app update."""
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class UpdateProgressTests(unittest.TestCase):
    def test_download_progress_and_failures(self):
        with tempfile.TemporaryDirectory(prefix="sonarpad-update-test-") as folder:
            binary = Path(folder) / ("tests.exe" if sys.platform == "win32" else "tests")
            subprocess.run([
                "rustc", "--edition", "2024", "--test",
                str(ROOT / "src/update_progress.rs"), "-o", str(binary),
            ], check=True)
            subprocess.run([str(binary)], check=True)

    def test_update_window_uses_modal_event_loop_and_measured_progress(self):
        source = (ROOT / "src/main.rs").read_text(encoding="utf-8")
        window = source.split("fn prepare_macos_update_install_with_progress(", 1)[1]
        window = window.split("fn launch_pending_macos_update_install(", 1)[0]
        self.assertIn("Gauge::builder", window)
        self.assertIn("timer.on_tick", window)
        self.assertIn("dialog.show_modal()", window)
        self.assertIn("timer.stop()", window)
        self.assertIn("value.percent()", window)
        self.assertIn("gauge.set_accessibility_label(&message)", window)
        self.assertNotIn("std::thread::sleep", window)
        self.assertNotIn("progress_value +=", window)
        download = source.split("fn prepare_macos_update_install(", 1)[1].split(
            "fn prepare_macos_update_install_with_progress(", 1
        )[0]
        self.assertLess(download.index("copy_download"), download.index("extracting = true"))
        self.assertLess(download.index("extracting = true"), download.index('Command::new("/usr/bin/ditto")'))

    def test_every_ui_language_has_update_phases(self):
        for path in (ROOT / "i18n").glob("ui_*.json"):
            with self.subTest(language=path.name):
                labels = json.loads(path.read_text(encoding="utf-8"))
                for key in ("update_downloading", "update_extracting"):
                    self.assertTrue(labels[key].strip())
                    self.assertNotIn("\ufffd", labels[key])


if __name__ == "__main__":
    unittest.main()

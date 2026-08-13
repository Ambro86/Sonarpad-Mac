from __future__ import annotations

import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
MAIN = ROOT / "src" / "main.rs"


def text(path: Path) -> str:
    return path.read_text(encoding="utf-8")


class MacOsLogRetentionTests(unittest.TestCase):
    def test_main_log_rotates_at_one_megabyte_with_single_backup(self):
        source = text(MAIN)
        self.assertIn("const SONARPAD_LOG_MAX_BYTES: u64 = 1024 * 1024;", source)
        self.assertIn('file_name.push(".1");', source)
        self.assertIn("rotate_log_if_needed(path, line.len())?", source)
        self.assertIn("std::fs::rename(path, &backup_path)", source)

    def test_main_log_writer_is_serialized_inside_sonarpad(self):
        source = text(MAIN)
        self.assertIn("static LOG_WRITE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();", source)
        self.assertIn("unwrap_or_else(|poisoned| poisoned.into_inner())", source)

    def test_rotation_records_marker_in_new_log(self):
        source = text(MAIN)
        self.assertIn("log.rotated previous={backup_name} max_bytes={SONARPAD_LOG_MAX_BYTES}", source)

    def test_mpv_lua_writer_uses_same_one_megabyte_rotation(self):
        source = text(MAIN)
        self.assertIn("local sonarpad_log_max_bytes = 1048576", source)
        self.assertIn('local sonarpad_log_backup_file = sonarpad_log_file .. \\".1\\"', source)
        self.assertIn("local function rotate_sonarpad_log_if_needed(incoming_bytes)", source)
        self.assertIn("os.rename(sonarpad_log_file, sonarpad_log_backup_file)", source)

    def test_only_ten_mpv_diagnostic_logs_are_retained(self):
        source = text(MAIN)
        self.assertIn("const MPV_DIAGNOSTIC_LOG_KEEP_TOTAL: usize = 10;", source)
        self.assertIn("logs.into_iter().skip(keep_count)", source)
        self.assertIn("std::fs::remove_file(path)", source)
        self.assertIn(
            "cleanup_mpv_diagnostic_logs(MPV_DIAGNOSTIC_LOG_KEEP_TOTAL.saturating_sub(1));",
            source,
        )

    def test_mpv_logs_are_also_pruned_on_app_start(self):
        source = text(MAIN)
        main_start = source.index("fn main()")
        app_start = source.index('append_podcast_log("app.start");', main_start)
        cleanup = source.index(
            "cleanup_mpv_diagnostic_logs(MPV_DIAGNOSTIC_LOG_KEEP_TOTAL);", main_start
        )
        self.assertLess(cleanup, app_start)


if __name__ == "__main__":
    unittest.main()

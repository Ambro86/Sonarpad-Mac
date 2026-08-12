from __future__ import annotations

import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
MAIN = ROOT / "src" / "main.rs"
LA7 = ROOT / "src" / "la7_play.rs"
CARGO = ROOT / "Cargo.toml"
LOCK = ROOT / "Cargo.lock"


def text(path: Path) -> str:
    return path.read_text(encoding="utf-8")


class MacOsLa7PlayTests(unittest.TestCase):
    def test_windows_la7_core_is_ported_as_native_rust_module(self):
        source = text(LA7)
        self.assertIn('const RIVEDI: &str = "https://www.la7.it/rivedila7/0/la7";', source)
        self.assertIn('const PROGRAMMI: &str = "https://www.la7.it/programmi";', source)
        self.assertIn('const TUTTI_PROGRAMMI: &str = "https://www.la7.it/tutti-i-programmi";', source)
        self.assertIn('pub(crate) fn root_page()', source)
        self.assertIn('pub(crate) fn load_page(', source)
        self.assertIn('pub(crate) fn search(', source)
        self.assertIn('pub(crate) fn resolve_vod(', source)
        self.assertIn('program_search_clips', source)
        self.assertIn('program_episodes', source)
        self.assertIn('widevine', source.lower())

    def test_la7_is_exposed_only_in_italian_tools_section(self):
        source = text(MAIN)
        self.assertIn('mod la7_play;', source)
        self.assertIn('const ID_LA7_PLAY: i32 = 2378;', source)
        italian_start = source.index('if Settings::load().ui_language == "it" {', source.index('let tools_menu'))
        menubar = source.index('let menubar', italian_start)
        italian_tools = source[italian_start:menubar]
        self.assertIn('ID_LA7_PLAY', italian_tools)
        self.assertIn('la7_play::menu_label()', italian_tools)

    def test_la7_uses_same_rai_luce_code_gate(self):
        source = text(MAIN)
        start = source.index('fn la7_play_code_available')
        end = source.index('fn open_la7_play_page_dialog', start)
        gate = source[start:end]
        self.assertIn('Settings::load().ui_language != "it"', gate)
        self.assertIn('load_saved_rai_luce_code().is_some()', gate)
        self.assertIn('handle_rai_missing_code(', gate)
        self.assertIn('Chiave Luce mancante:', gate)

    def test_la7_dialog_mirrors_raiplay_search_open_save_close_ui(self):
        source = text(MAIN)
        start = source.index('fn open_la7_play_items_modal')
        end = source.index('fn open_la7_live_with_mpv', start)
        dialog = source[start:end]
        self.assertIn('TextCtrlStyle::ProcessEnter', dialog)
        self.assertIn('Button::builder(&panel).with_label(&ui.search)', dialog)
        self.assertIn('let choice = Choice::builder(&panel).build();', dialog)
        self.assertIn('with_label(&ui.open)', dialog)
        self.assertIn('with_label(&ui.rai_save_content)', dialog)
        self.assertIn('with_label(&ui.close)', dialog)
        self.assertIn('update_choice_button_visibility', dialog)
        self.assertIn('item.kind == la7_play::ItemKind::Media', dialog)

    def test_la7_vod_download_reuses_raiplay_ffmpeg_progress_path(self):
        source = text(MAIN)
        start = source.index('fn save_la7_target_dialog')
        end = source.index('fn open_raiplay_search_dialog', start)
        saving = source[start:end]
        self.assertIn('choice.append("MP3")', saving)
        self.assertIn('choice.append("MP4")', saving)
        self.assertIn('run_ffmpeg_save(', saving)
        self.assertIn('"libmp3lame"', saving)
        self.assertIn('&["-y", "-i", url, "-c", "copy"]', saving)
        # run_ffmpeg_save is the exact RaiPlay Mac progress helper.
        self.assertEqual(source.count('fn run_ffmpeg_save(parent: &Dialog'), 1)

    def test_la7_live_reuses_mac_tv_player(self):
        source = text(MAIN)
        start = source.index('fn open_la7_live_with_mpv')
        end = source.index('fn save_la7_target_dialog', start)
        live = source[start:end]
        self.assertIn('tv::load_channels()?', live)
        self.assertIn('open_tv_stream_with_mpv(&channel)', live)
        self.assertIn('la7d', live)

    def test_regex_is_direct_dependency_without_new_native_runtime(self):
        cargo = text(CARGO)
        lock = text(LOCK)
        self.assertRegex(cargo, r'(?m)^regex = "1\.12"$')
        package = lock[lock.index('name = "sonarpad-minimal"'):]
        package = package[:package.index('[[package]]', 1)]
        self.assertIn(' "regex",', package)
        self.assertIn('name = "regex"', lock)


if __name__ == "__main__":
    unittest.main()

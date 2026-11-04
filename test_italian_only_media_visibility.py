"""Regression guard: Italian-only multimedia menu entries must stay hidden for de/en/etc."""
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parent
SOURCE = (ROOT / 'src/main.rs').read_text(encoding='utf-8')
ITALIAN_ONLY = (
    'ID_SONARPAD_AUDIO_DESCRIPTIONS',
    'ID_RAI_AUDIO_DESCRIPTIONS',
    'ID_RAIPLAY_BROWSE',
    'ID_LA7_PLAY',
    'ID_RAIPLAY_SOUND',
    'ID_TV',
)


def section(start: str, end: str) -> str:
    first = SOURCE.index(start)
    last = SOURCE.index(end, first + len(start))
    return SOURCE[first:last]


class ItalianOnlyMultimediaTests(unittest.TestCase):
    def test_categorized_menu_restricts_all_italian_media(self):
        code = section('fn append_tools_multimedia(', 'fn append_tools_utilities(')
        italian_part = code.index('if settings.ui_language == "it" {')
        for item in ITALIAN_ONLY:
            with self.subTest(item=item):
                self.assertNotIn(item, code[:italian_part])
                self.assertIn(item, code[italian_part:])

    def test_flat_menu_restricts_all_italian_media(self):
        code = section('fn rebuild_tools_menu(', '\nfn main()')
        italian_part = code.index('if settings.ui_language == "it" {')
        for item in ITALIAN_ONLY:
            with self.subTest(item=item):
                self.assertNotIn(item, code[:italian_part])
                self.assertIn(item, code[italian_part:])

    def test_sonarpad_catalog_requires_configured_code_in_both_menus(self):
        for code in (
            section('fn append_tools_multimedia(', 'fn append_tools_utilities('),
            section('fn rebuild_tools_menu(', '\nfn main()'),
        ):
            self.assertRegex(code, r'if load_saved_rai_luce_code\(\)\.is_some\(\)\s*\{\s*let _ = (?:menu|tools_menu)\.append_command\(\s*ID_SONARPAD_AUDIO_DESCRIPTIONS')
        self.assertIn('if Settings::load().ui_language != "it" || load_saved_rai_luce_code().is_none()',
                      section('fn open_sonarpad_audio_descriptions_dialog(', 'fn open_sonarpad_audio_recent_dialog('))

    def test_dispatch_denies_italian_media_in_other_languages(self):
        code = section('frame.on_menu(move |event| {\n            let ui = current_ui_strings();',
                       'if matches!(event.get_id(), documents::ID_NEW_DOCUMENT')
        self.assertIn('settings_menu.lock().unwrap().ui_language != "it"', code)
        self.assertIn('load_saved_rai_luce_code().is_none()', code)
        for item in ITALIAN_ONLY + ('ID_RAIPLAY_SEARCH',):
            with self.subTest(item=item):
                self.assertIn(item, code)

    def test_general_ai_audio_description_stays_available_globally(self):
        code = section('fn append_tools_multimedia(', 'fn append_tools_utilities(')
        italian_part = code.index('if settings.ui_language == "it" {')
        self.assertIn('ID_TOOLS_AUDIO_DESCRIPTION', code[:italian_part])
        self.assertNotIn('ID_TOOLS_AUDIO_DESCRIPTION', section(
            'frame.on_menu(move |event| {\n            let ui = current_ui_strings();',
            'if matches!(event.get_id(), documents::ID_NEW_DOCUMENT'))

    def test_de_is_distinct_from_it(self):
        code = section('fn normalize_ui_language(value: &str) -> String {', 'fn system_language_code()')
        self.assertRegex(code, r'"de" \| "german"[\s\S]*?=> "de"\.to_string\(\)')

if __name__ == '__main__':
    unittest.main()

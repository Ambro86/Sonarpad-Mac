from pathlib import Path
import re

ROOT = Path(__file__).resolve().parent
MAIN = (ROOT / 'src' / 'main.rs').read_text(encoding='utf-8')


def _function_body(name: str) -> str:
    start = MAIN.index(f'fn {name}(')
    brace = MAIN.index('{', start)
    depth = 0
    for i in range(brace, len(MAIN)):
        if MAIN[i] == '{':
            depth += 1
        elif MAIN[i] == '}':
            depth -= 1
            if depth == 0:
                return MAIN[brace + 1:i]
    raise AssertionError(f'Function {name} not closed')


def _labels(name: str):
    body = _function_body(name)
    return dict(re.findall(r'"(it|en|fr|es|pt|cs|pl)"\s*=>\s*"([^"]+)"\.to_string\(\)', body))


def test_interface_language_choices_use_current_ui_language():
    body = _function_body('interface_language_options')
    assert 'get_language_name_for_ui(code, ui_language)' in body
    assert '["it", "en", "fr", "es", "pt", "cs", "pl"]' in body
    settings_body = _function_body('open_settings_dialog')
    assert 'interface_language_options(&settings_before.ui_language)' in settings_body
    assert '("Italiano", "it")' not in settings_body
    assert '("English", "en")' not in settings_body
    assert '("Français", "fr")' not in settings_body


def test_expected_italian_interface_language_names():
    labels = _labels('get_language_name_it')
    assert labels['it'] == 'Italiano'
    assert labels['en'] == 'Inglese'
    assert labels['fr'] == 'Francese'


def test_expected_english_interface_language_names():
    labels = _labels('get_language_name_en')
    assert labels['it'] == 'Italian'
    assert labels['en'] == 'English'
    assert labels['fr'] == 'French'


def test_expected_french_interface_language_names():
    labels = _labels('get_language_name_fr')
    assert labels['it'] == 'Italien'
    assert labels['en'] == 'Anglais'
    assert labels['fr'] == 'Français'


def test_voice_language_list_uses_same_localizer_including_french():
    body = _function_body('build_language_list')
    assert 'get_language_name_for_ui(&voice.locale, ui_language)' in body

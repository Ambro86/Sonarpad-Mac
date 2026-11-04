"""Phase 1 German source-localization regression tests (no Mac needed)."""
import gettext
import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parent


def _placeholders(value):
    return sorted(re.findall(r"\{[a-zA-Z_][a-zA-Z_0-9]*\}", value))


def test_complete_main_catalog():
    english = json.loads((ROOT / "i18n/ui_en.json").read_text(encoding="utf-8"))
    german = json.loads((ROOT / "i18n/ui_de.json").read_text(encoding="utf-8"))
    assert len(english) == len(german) == 529
    assert set(english) == set(german)
    for key, value in english.items():
        translated = german[key]
        assert type(translated) is type(value), key
        if isinstance(value, str):
            assert translated.strip(), key
            assert _placeholders(translated) == _placeholders(value), key
            if "\t" in value:
                assert translated.split("\t", 1)[1] == value.split("\t", 1)[1], key
        else:
            assert len(translated) == len(value), key
    assert german["menu_file"] == "&Datei"
    assert german["menu_window"] == "Fenster"
    assert german["document_untitled"] == "Unbenannt"
    assert german["youtube_search_types"] == ["Alle", "Videos", "Playlists", "Kanäle", "Kurse", "Podcasts"]


def test_german_available_in_program():
    source = (ROOT / "src/main.rs").read_text(encoding="utf-8")
    assert '"de" => UI_DE.get_or_init' in source
    assert '"de" => get_language_name_de(locale)' in source
    assert '"de" => Some(include_bytes!("../i18n/wxstd_de.mo"))' in source
    assert '"de" | "german" | "tedesco" | "deutsch"' in source
    assert '"de" => include_str!("../donations_de.txt")' in source
    assert '"de" => "Nein"' in source
    assert '"de" => "de"' in source
    assert 'de_DE' in source


def test_german_calendar_labels_and_dates():
    source = (ROOT / "src/calendar.rs").read_text(encoding="utf-8")
    assert '"de" => CalendarLabels {' in source
    assert 'menu: "Kalender"' in source
    assert 'title: "Kalender"' in source
    assert '"Montag", "Dienstag", "Mittwoch"' in source
    assert '"Januar", "Februar", "März"' in source
    assert '"Tag der Deutschen Einheit"' in source
    assert '"{}, {}. {} {}"' in source and 'if language == "de"' in source
    assert 'language == "de"' in source


def test_transcription_and_native_macos_menu():
    english = json.loads((ROOT / "i18n/media_transcription_en.json").read_text(encoding="utf-8"))
    german = json.loads((ROOT / "i18n/media_transcription_de.json").read_text(encoding="utf-8"))
    assert english.keys() == german.keys()
    for k in english:
        assert _placeholders(english[k]) == _placeholders(german[k]), k
    transcriptions = (ROOT / "src/media_transcription.rs").read_text(encoding="utf-8")
    assert '"de" => (&DE, include_str!("../i18n/media_transcription_de.json"))' in transcriptions
    with (ROOT / "i18n/wxstd_de.mo").open('rb') as stream:
        translated = gettext.GNUTranslations(stream)
    assert translated.gettext('Window') == 'Fenster'
    assert translated.gettext('Help') == 'Hilfe'
    for workflow in ('macos-app-dmg.yml', 'macos-app-dmg-catalina.yml'):
        w = (ROOT / '.github/workflows' / workflow).read_text(encoding='utf-8')
        assert 'CFBundleLocalizations:7 string de' in w
        assert 'DE_LPROJ_DIR' in w
        assert '<string>de</string>' in w

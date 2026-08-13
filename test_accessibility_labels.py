from pathlib import Path
import json
import re

ROOT = Path(__file__).resolve().parent
LANGS = ("it", "en", "fr", "es", "pt", "cs", "pl")
NEW_UI_KEYS = {
    "editor_text_label",
    "weather_results_label",
    "cinema_details_label",
    "italian_directories_location_label",
    "youtube_search_label",
    "tv_search_label",
}


def _active_textctrls():
    controls = []
    for path in sorted((ROOT / "src").glob("*.rs")):
        if path.name == "main1.rs":
            continue
        lines = path.read_text(encoding="utf-8").splitlines()
        for index, line in enumerate(lines):
            if "TextCtrl::builder" not in line:
                continue
            match = re.search(r"let\s+(\w+)\s*=\s*TextCtrl::builder", line)
            assert match, f"Cannot identify TextCtrl variable at {path}:{index + 1}"
            variable = match.group(1)
            end = index
            while end < len(lines) and ".build();" not in lines[end]:
                end += 1
            assert end < len(lines), f"No build() end for {variable} at {path}:{index + 1}"
            controls.append((path, lines, index, end, variable))
    return controls


def test_textctrls_use_visible_context_or_an_explicit_accessibility_label():
    controls = _active_textctrls()
    assert len(controls) == 68, f"Expected 68 active TextCtrl controls, found {len(controls)}"

    for path, lines, index, end, variable in controls:
        following = "\n".join(lines[end + 1 : end + 7])
        explicit = f"{variable}.set_accessibility_label(" in following
        if explicit:
            continue

        # On macOS a nearby visible StaticText is intentionally the single spoken label.
        # Repeating the same wording through NSAccessibility makes VoiceOver announce it twice
        # while navigating with VO+Right.
        context = "\n".join(lines[max(0, index - 24) : min(len(lines), end + 30)])
        assert "StaticText::builder" in context, (
            f"{variable} at {path}:{index + 1} has neither visible label context "
            "nor an explicit accessibility label"
        )


def test_explicit_textctrl_labels_are_only_for_controls_without_visible_duplicate():
    for path, lines, index, end, variable in _active_textctrls():
        following_lines = lines[end + 1 : end + 7]
        label_line = next(
            (line.strip() for line in following_lines if f"{variable}.set_accessibility_label(" in line),
            None,
        )
        if label_line is None:
            continue

        arg = label_line.split(".set_accessibility_label(", 1)[1].rsplit(");", 1)[0].strip()
        context = "\n".join(lines[max(0, index - 24) : min(len(lines), end + 30)])
        assert f".with_label({arg})" not in context, (
            f"Duplicate VoiceOver label for {variable} at {path}:{index + 1}: {arg}"
        )


def test_settings_choices_do_not_repeat_visible_labels():
    source = (ROOT / "src" / "main.rs").read_text(encoding="utf-8")
    duplicated_choices = (
        "choice_ui_lang",
        "choice_news_lang",
        "choice_voice_engine",
        "choice_lang",
        "choice_voices",
        "choice_rate",
        "choice_pitch",
        "choice_volume",
        "choice_media_seek",
        "format_choice",
    )
    for variable in duplicated_choices:
        assert f"set_localized_accessible_label(\n        &{variable}," not in source
        assert f"set_localized_accessible_label(&{variable}," not in source

    # This hidden control has no visible StaticText companion, so an explicit label is useful.
    assert "set_localized_accessible_label(\n            &podcast_seek_choice," in source


def test_audio_description_fields_do_not_repeat_visible_labels():
    source = (ROOT / "src" / "audio_description.rs").read_text(encoding="utf-8")
    repeated = (
        'input.set_accessibility_label(&tr("audio_description.input"));',
        'output.set_accessibility_label(&tr("audio_description.output"));',
        'catalog_name.set_accessibility_label(&tr("audio_description.character_catalog.new_name_label"));',
        'api.set_accessibility_label(&tr("audio_description.gemini_api_key"));',
        'text.set_accessibility_label(&tr("audio_description.project.text"));',
        'search.set_accessibility_label(&tr("audio_description.project.search"));',
    )
    for line in repeated:
        assert line not in source

    # The completion details box has no visible label before it.
    assert 'completion_details.set_accessibility_label(&tr("audio_description.completion.details"));' in source


def test_new_accessibility_strings_exist_in_every_language():
    for lang in LANGS:
        ui = json.loads((ROOT / "i18n" / f"ui_{lang}.json").read_text(encoding="utf-8"))
        for key in NEW_UI_KEYS:
            assert key in ui and str(ui[key]).strip(), f"{lang}: missing {key}"
        audio = json.loads((ROOT / "i18n" / f"audio_description_{lang}.json").read_text(encoding="utf-8"))
        key = "audio_description.completion.details"
        assert key in audio and str(audio[key]).strip(), f"{lang}: missing {key}"


def test_old_italian_only_helper_is_replaced():
    source = (ROOT / "src" / "main.rs").read_text(encoding="utf-8")
    assert "set_italian_accessible_name" not in source
    assert "fn set_localized_accessible_label" in source
    assert "widget.set_accessibility_label(label);" in source

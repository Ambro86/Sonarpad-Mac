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


def test_all_active_textctrls_have_accessibility_labels():
    total = 0
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
            total += 1
            end = index
            while end < len(lines) and ".build();" not in lines[end]:
                end += 1
            assert end < len(lines), f"No build() end for {variable} at {path}:{index + 1}"
            following = "\n".join(lines[end + 1 : end + 5])
            assert f"{variable}.set_accessibility_label(" in following, (
                f"Missing accessibility label for {variable} at {path}:{index + 1}"
            )
    assert total == 68, f"Expected 68 active TextCtrl controls, found {total}"


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

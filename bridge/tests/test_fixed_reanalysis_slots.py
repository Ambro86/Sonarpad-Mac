import importlib.util
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("ad_bridge", ROOT / "audio_description_bridge.py")
bridge = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(bridge)


def test_fixed_slots_are_normalized_without_retiming():
    slots = bridge._normalise_fixed_reanalysis_slots([
        {"id": "saved-7", "start_sec": 6.353, "end_sec": 11.343,
         "visual_reference_sec": 9.0, "previous_text": " Vecchio   testo ",
         "extended_pause": False},
    ], 178.83)
    assert slots == [{
        "id": "saved-7", "start": 6.353, "end": 11.343,
        "visual_reference": 9.0, "previous_text": "Vecchio testo",
        "extended_pause": False,
    }]


def test_fixed_response_uses_slot_ids_and_never_needs_timestamps():
    slots = [{"id": "saved-7"}, {"id": "saved-9"}]
    raw = '{"slot_descriptions":[{"slot_id":"saved-7","description_text":"Nuovo testo."},{"slot_id":"saved-9","description_text":"Altra descrizione."}]}'
    assert bridge._parse_fixed_reanalysis_json(raw, slots) == {
        "saved-7": "Nuovo testo.",
        "saved-9": "Altra descrizione.",
    }


def test_unknown_or_duplicate_ids_are_ignored():
    slots = [{"id": "saved-7"}]
    raw = '{"slot_descriptions":[{"slot_id":"invented","description_text":"No"},{"slot_id":"saved-7","description_text":"Sì"},{"slot_id":"saved-7","description_text":"Duplicato"}]}'
    assert bridge._parse_fixed_reanalysis_json(raw, slots) == {"saved-7": "Sì"}

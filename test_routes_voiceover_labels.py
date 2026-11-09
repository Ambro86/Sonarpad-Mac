from pathlib import Path

ROUTES = (Path(__file__).resolve().parent / "src" / "routes.rs").read_text(encoding="utf-8")


def test_route_static_labels_are_excluded_from_voiceover_navigation():
    # Text remains visible. Its accessible name belongs to the associated input.
    assert "control.set_accessibility_label(&label.get_label());" in ROUTES
    assert "view.setAccessibilityElement(false);" in ROUTES
    assert "view.setAccessibilityHidden(true);" in ROUTES
    assert "label.show(" in ROUTES or "departure_label.show(" in ROUTES
    for name, label in (
        ("from_ctrl", "from_label"), ("to_ctrl", "to_label"),
        ("profile_choice", "profile_label"), ("pref_choice", "pref_label"),
        ("departure_choice", "departure_label"), ("hour_choice", "hour_label"),
        ("minute_choice", "minute_label"), ("zone_ctrl", "zone_label"),
    ):
        assert f"label_route_control(&{name}, &{label});" in ROUTES

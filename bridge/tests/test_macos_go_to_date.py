import json
import pathlib
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]


class MacGoToDateTests(unittest.TestCase):
    def test_podcast_feed_persists_publication_date(self):
        source = (ROOT / "src" / "podcasts.rs").read_text(encoding="utf-8")
        self.assertIn("pub published_date: String", source)
        self.assertRegex(source, r"entry\s*\.published")
        self.assertRegex(source, r"\.or\(entry\.updated\)")
        self.assertIn('format("%Y-%m-%d")', source)

    def test_podcast_menu_shows_go_to_date_before_episode_entries(self):
        source = (ROOT / "src" / "main.rs").read_text(encoding="utf-8")
        menu_start = source.index("fn rebuild_podcasts_menu")
        menu_end = source.index("fn rebuild_radio_menu", menu_start)
        menu = source[menu_start:menu_end]
        self.assertIn("podcasts_date_menu_id(source_index)", menu)
        self.assertLess(menu.index("ui.go_to_date"), menu.index("MAX_MENU_PODCAST_EPISODES_PER_SOURCE"))

    def test_date_dialogs_do_not_add_redundant_static_labels(self):
        source = (ROOT / "src" / "main.rs").read_text(encoding="utf-8")
        podcast_start = source.index("fn open_podcast_date_episode_dialog")
        podcast_end = source.index("fn activate_podcast_episode", podcast_start)
        podcast_dialog = source[podcast_start:podcast_end]
        self.assertNotIn("StaticText::builder", podcast_dialog)
        self.assertIn("choice_focus.set_focus()", podcast_dialog)

        rai_start = source.index("fn open_raiplaysound_date_selector")
        rai_end = source.index("fn open_raiplaysound_items_modal", rai_start)
        rai_dialog = source[rai_start:rai_end]
        self.assertNotIn("StaticText::builder", rai_dialog)
        self.assertIn("choice_focus.set_focus()", rai_dialog)

    def test_raiplaysound_uses_mobile_date_fields_and_context_button(self):
        source = (ROOT / "src" / "raiplaysound.rs").read_text(encoding="utf-8")
        for key in (
            "publication_date_iso",
            "publication_date",
            "create_date",
            "literal_publication_date",
        ):
            self.assertIn(f'"{key}"', source)

        main = (ROOT / "src" / "main.rs").read_text(encoding="utf-8")
        self.assertIn("let date_button = Button::builder(&panel).with_label(&ui.go_to_date).build();", main)
        self.assertIn("!raiplaysound_available_dates(items_rc.as_ref()).is_empty()", main)
        modal_start = main.index("fn open_raiplaysound_items_modal")
        modal_end = main.index("fn open_rai_stream_with_mpv", modal_start)
        modal = main[modal_start:modal_end]
        self.assertLess(modal.index("root.add(&date_button"), modal.index("root.add(&choice"))

    def test_all_ui_languages_have_date_strings(self):
        for path in sorted((ROOT / "i18n").glob("ui_*.json")):
            data = json.loads(path.read_text(encoding="utf-8"))
            self.assertTrue(data["go_to_date"].strip(), path.name)
            self.assertTrue(data["no_dates_available"].strip(), path.name)
            self.assertIn("{date}", data["episodes_on_date"], path.name)


if __name__ == "__main__":
    unittest.main()

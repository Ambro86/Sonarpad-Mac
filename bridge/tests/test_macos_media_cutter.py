from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]
MAIN = (ROOT / "src" / "main.rs").read_text(encoding="utf-8")
CUTTER = (ROOT / "src" / "media_cutter.rs").read_text(encoding="utf-8")


class MacMediaCutterTests(unittest.TestCase):
    def test_media_cutter_is_kept_but_hidden_from_macos_menus(self):
        self.assertIn("mod media_cutter;", MAIN)
        self.assertIn("ID_TOOLS_MEDIA_CUTTER", MAIN)
        self.assertIn('#[cfg(target_os = "macos")]\nconst SHOW_MEDIA_CUTTER_IN_MENUS: bool = false;', MAIN)
        self.assertEqual(MAIN.count("if SHOW_MEDIA_CUTTER_IN_MENUS {"), 2)
        self.assertIn("media_cutter::menu_label()", MAIN)
        self.assertIn("media_cutter::open_dialog(&f_menu)", MAIN)

    def test_guided_and_advanced_workflows_are_present(self):
        for token in [
            '"Taglio guidato"',
            '"Taglio avanzato"',
            '"Inizio taglio"',
            '"Fine taglio"',
            '"Applica taglio"',
            '"Ascolta taglio"',
            '"Dividi qui"',
            '"Ripristina parte eliminata"',
        ]:
            self.assertIn(token, CUTTER)
        self.assertIn("delete_range(&mut", CUTTER)
        self.assertIn("split_part_at(&mut", CUTTER)

    def test_precision_matches_mobile_port(self):
        for token in ["1 => 0.5", "2 => 0.25", "3 => 0.10", "[\"1 s\", \"0,5 s\", \"0,25 s\", \"0,10 s\"]"]:
            self.assertIn(token, CUTTER)

    def test_irrelevant_advanced_and_video_controls_are_hidden(self):
        for token in [
            "split_here.show(false)",
            "listen_part.show(false)",
            "modify_part.show(false)",
            "delete_part.show(false)",
            "restore_part.show(false)",
            "parts_choice.show(false)",
            "rotation_label.show(false)",
            "rotation_choice.show(false)",
            "video_preview_button.show(false)",
        ]:
            self.assertIn(token, CUTTER)

    def test_audio_cover_art_is_not_treated_as_real_video(self):
        self.assertIn("stream_disposition=attached_pic", CUTTER)
        self.assertIn('get("attached_pic")', CUTTER)
        self.assertIn("if !attached_picture", CUTTER)

    def test_export_is_cancellable_and_published_only_at_end(self):
        self.assertIn("child.kill()", CUTTER)
        self.assertIn("__CANCELLED__", CUTTER)
        self.assertIn("sonarpad-pending", CUTTER)
        self.assertIn("std::fs::rename(&pending, &snapshot.output)", CUTTER)
        self.assertNotIn("remove_file(&snapshot.output)", CUTTER)

    def test_pending_guided_cut_cannot_be_silently_lost_on_save(self):
        self.assertIn("guided_start_save.get().is_some()", CUTTER)
        self.assertIn("labels.pending_cut_save", CUTTER)
        self.assertIn("labels.discard_pending_cut", CUTTER)

    def test_voiceover_gets_start_announcement(self):
        self.assertIn("announce_voiceover_message(labels.processing_started)", CUTTER)
        self.assertIn('"Elaborazione iniziata"', CUTTER)

    def test_initial_focus_and_redundant_empty_status_are_accessible(self):
        self.assertIn("file_status.show(false)", CUTTER)
        self.assertIn("position_text.show(false)", CUTTER)
        self.assertIn("open_focus.set_focus()", CUTTER)
        self.assertIn("focus_timer.start(80, true)", CUTTER)


    def test_probe_falls_back_to_bundled_ffmpeg_when_ffprobe_is_missing(self):
        self.assertIn("fn probe_media_with_ffmpeg", CUTTER)
        self.assertIn("ffprobe_unavailable", CUTTER)
        self.assertIn("using_ffmpeg", CUTTER)
        self.assertIn('.args(["-hide_banner", "-i"])', CUTTER)
        self.assertIn("attached pic", CUTTER)

    def test_close_is_always_reachable_and_focus_moves_to_cut_mode_after_load(self):
        self.assertIn("let top_buttons = BoxSizer::builder", CUTTER)
        self.assertIn("with_id(ID_CANCEL)", CUTTER)
        self.assertIn("top_buttons.add(&open_button", CUTTER)
        self.assertIn("top_buttons.add(&close_button", CUTTER)
        self.assertIn("mode_choice.set_focus()", CUTTER)

    def test_add_track_and_rotation_are_secondary_features(self):
        self.assertIn('"Aggiungi nuova traccia…"', CUTTER)
        self.assertIn('"Rotazione video"', CUTTER)
        self.assertIn("amix=inputs=2", CUTTER)
        self.assertIn('Some("transpose=1")', CUTTER)
        self.assertIn('Some("transpose=2")', CUTTER)


if __name__ == "__main__":
    unittest.main()

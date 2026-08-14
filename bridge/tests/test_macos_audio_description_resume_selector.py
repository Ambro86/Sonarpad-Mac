from __future__ import annotations

import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
AUDIO = ROOT / "src" / "audio_description.rs"
MAIN = ROOT / "src" / "main.rs"


class MacAudioDescriptionResumeSelectorTests(unittest.TestCase):
    def test_continue_uses_project_selector_instead_of_direct_file_dialog(self):
        audio = AUDIO.read_text(encoding="utf-8")
        self.assertIn("fn discover_resume_candidates()", audio)
        self.assertIn("fn choose_resume_checkpoint(parent: &Dialog)", audio)
        self.assertIn('tr("audio_description.resume.choose_label")', audio)
        self.assertIn("Choice::builder(&panel)", audio)

    def test_selector_lists_only_valid_partial_checkpoints_and_has_browse_fallback(self):
        audio = AUDIO.read_text(encoding="utf-8")
        self.assertIn('const CHECKPOINT_SUFFIX: &str = ".sonarpad-ad.partial.json";', audio)
        self.assertIn("load_resume_settings(path).ok()?", audio)
        self.assertIn("discover_resume_candidates()", audio)
        self.assertIn('tr("audio_description.resume.browse_other")', audio)
        start = audio.index("fn browse_resume_checkpoint")
        end = audio.index("fn choose_resume_checkpoint", start)
        browse = audio[start:end]
        self.assertIn("*.sonarpad-ad.partial.json", browse)
        self.assertNotIn("*.*", browse)
        self.assertNotIn("*.json", browse.replace("*.sonarpad-ad.partial.json", ""))

    def test_recent_project_folders_are_persisted_and_used_for_discovery(self):
        audio = AUDIO.read_text(encoding="utf-8")
        main = MAIN.read_text(encoding="utf-8")
        self.assertIn("audio_description_recent_project_folders", main)
        self.assertIn("audio_description_recent_project_folders", audio)
        self.assertIn("truncate(MAX_RECENT_PROJECT_FOLDERS)", audio)
        self.assertIn("remember_audio_description_project_folder", audio)
        self.assertIn("remember_audio_description_project_folder(&mut settings, &path);", audio)
        self.assertIn("remember_audio_description_project_folder(&mut st, &job.output_path);", audio)

    def test_empty_selector_disables_continue_and_focuses_browse(self):
        audio = AUDIO.read_text(encoding="utf-8")
        self.assertIn("choice.enable(has_candidates);", audio)
        self.assertIn("continue_button.enable(has_candidates);", audio)
        self.assertIn("ID_AUDIO_DESCRIPTION_RESUME_BROWSE", audio)
        self.assertIn("browse_button.set_focus();", audio)
        self.assertIn('choice.set_accessibility_label(&tr("audio_description.resume.choose_label"));', audio)

    def test_cancelling_resume_selector_closes_creation_dialog(self):
        audio = AUDIO.read_text(encoding="utf-8")
        start = audio.index("continue_interrupted.on_click")
        end = audio.index("d.set_escape_id", start)
        resume = audio[start:end]
        self.assertIn("resume_selector_cancelled_closing_creation_window", resume)
        self.assertIn("d_resume.end_modal(ID_AUDIO_DESCRIPTION_CLOSE);", resume)

    def test_resume_selector_labels_are_localized_in_all_macos_locales(self):
        locales = ["it", "en", "fr", "es", "pt", "cs", "pl"]
        keys = [
            "audio_description.resume.choose_label",
            "audio_description.resume.choose_hint",
            "audio_description.resume.none_found",
            "audio_description.resume.browse_other",
        ]
        for locale in locales:
            text = (ROOT / "i18n" / f"audio_description_{locale}.json").read_text(
                encoding="utf-8-sig"
            )
            for key in keys:
                self.assertIn(f'"{key}"', text, f"{locale}: {key}")


if __name__ == "__main__":
    unittest.main()

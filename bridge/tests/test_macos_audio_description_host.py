from __future__ import annotations

import json
import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
MAIN = (ROOT / "src" / "main.rs").read_text(encoding="utf-8")
AUDIO = (ROOT / "src" / "audio_description.rs").read_text(encoding="utf-8")
BRIDGE = (ROOT / "src" / "audio_description_bridge.rs").read_text(encoding="utf-8")
WORKFLOW = (ROOT / ".github" / "workflows" / "macos-app-dmg.yml").read_text(encoding="utf-8")
CATALINA = (ROOT / ".github" / "workflows" / "macos-app-dmg-catalina.yml").read_text(encoding="utf-8")
RELEASE = (ROOT / ".github" / "workflows" / "crea-release-macos.yml").read_text(encoding="utf-8")
MAC_SPEC = (ROOT / "bridge" / "audio_description_bridge_macos.spec").read_text(encoding="utf-8")


class MacAudioDescriptionHostTests(unittest.TestCase):
    def test_tools_menu_opens_native_audio_description_dialog(self):
        self.assertIn("ID_TOOLS_AUDIO_DESCRIPTION", MAIN)
        self.assertIn("audio_description::menu_label()", MAIN)
        self.assertIn("audio_description::open_create_dialog", MAIN)
        self.assertIn("mod audio_description;", MAIN)
        self.assertIn("mod audio_description_bridge;", MAIN)
        self.assertNotIn("mod gemini_web;", MAIN)

    def test_audio_description_preferences_include_api_and_dedicated_tts(self):
        required = (
            "audio_description_gemini_api_key",
            "audio_description_gemini_model",
            "audio_description_language",
            "audio_description_verbosity",
            "audio_description_extended_pauses",
            "audio_description_recognize_characters",
            "audio_description_save_project",
            "audio_description_keep_character_catalog",
            "audio_description_character_catalog",
            "audio_description_tts_engine",
            "audio_description_tts_voice",
        )
        for field in required:
            with self.subTest(field=field):
                self.assertIn(field, MAIN)

    def test_mac_pipeline_uses_bundled_ffmpeg_and_measures_segment_durations(self):
        self.assertIn("ffmpeg_executable_path", AUDIO)
        self.assertIn('"-f".into()', AUDIO)
        self.assertIn('"segment".into()', AUDIO)
        self.assertIn('"-segment_time"', AUDIO)
        self.assertIn("probe_media(&path)", AUDIO)
        self.assertNotIn('"gemini_web_chunk_"', AUDIO)
        self.assertIn('"gemini_chunk_"', AUDIO)
        self.assertIn('let extension = "mkv";', AUDIO)
        self.assertIn("GEMINI_MAX_CHUNK_BYTES", AUDIO)

    def test_mandatory_descriptions_are_scheduled_before_optional_ones(self):
        self.assertIn("mandatory", AUDIO)
        self.assertIn("slot_id", AUDIO)
        self.assertIn("slot_start_sec", AUDIO)
        self.assertIn("slot_end_sec", AUDIO)
        self.assertRegex(AUDIO, r"(?s)filter\(\|.*mandatory.*\).*filter\(\|.*!.*mandatory")

    def test_worker_is_bundled_and_uses_process_group_cancellation(self):
        self.assertIn("Contents", BRIDGE)
        self.assertIn("Resources", BRIDGE)
        self.assertIn("audio-description", BRIDGE)
        self.assertIn("audio_description_bridge", BRIDGE)
        self.assertIn("process_group(0)", BRIDGE)
        self.assertNotIn("WEB_REQUEST", BRIDGE)
        self.assertNotIn("BridgeWebRequest", BRIDGE)
        self.assertNotIn("http://", BRIDGE.lower())
        self.assertNotIn("https://", BRIDGE.lower())

    def test_gemini_web_integration_is_removed(self):
        self.assertFalse((ROOT / "src" / "gemini_web.rs").exists())
        self.assertNotIn("gemini_web", AUDIO)
        self.assertNotIn("WEB_REQUEST", BRIDGE)

    def test_pyinstaller_spec_bundles_runtime_model_and_google_genai(self):
        self.assertIn("pyannote-segmentation", MAC_SPEC)
        self.assertIn("collect_data_files", MAC_SPEC)
        self.assertIn("collect_submodules", MAC_SPEC)
        self.assertIn("google.genai", MAC_SPEC)
        self.assertIn("onnxruntime", MAC_SPEC)

    def test_modern_workflow_builds_bundles_and_signs_worker(self):
        self.assertIn("build_audio_description_bridge_macos.sh", WORKFLOW)
        self.assertIn("dist/audio-description-worker", WORKFLOW)
        self.assertIn("Contents/Resources/audio-description", WORKFLOW)
        self.assertIn("Mach-O", WORKFLOW)
        self.assertIn("codesign", WORKFLOW)
        self.assertIn("--enable-muxer=segment", WORKFLOW)
        self.assertIn("--enable-muxer=matroska", WORKFLOW)
        self.assertIn('ONNXRUNTIME_VERSION: "1.19.2"', WORKFLOW)

    def test_catalina_workflow_uses_compatible_worker_and_checks_macho_targets(self):
        self.assertIn("build_audio_description_bridge_macos.sh", CATALINA)
        self.assertIn('ONNXRUNTIME_VERSION: "1.15.0"', CATALINA)
        self.assertIn("10.15", CATALINA)
        self.assertIn("otool", CATALINA)
        self.assertIn("audio-description", CATALINA)
        self.assertIn("--enable-muxer=segment", CATALINA)
        self.assertIn("--enable-muxer=matroska", CATALINA)


    def test_release_workflow_rejects_archives_without_worker_or_ffmpeg(self):
        self.assertIn("Contents/Resources/audio-description/audio_description_bridge/audio_description_bridge", RELEASE)
        self.assertIn("Contents/Resources/ffmpeg/bin/ffmpeg", RELEASE)
        self.assertIn("unzip -Z1", RELEASE)


    def test_mac_audio_description_matches_windows_catalog_and_edge_cleanup(self):
        self.assertIn("merge_catalog_description", AUDIO)
        self.assertIn("catalog_description_coverage", AUDIO)
        self.assertIn("MAX_CHARACTER_DESCRIPTION_CHARS", AUDIO)
        self.assertIn("trim_edge_trailing_silence", AUDIO)
        self.assertIn("EDGE_TRAILING_WINDOW_MS", AUDIO)

    def test_audio_description_save_folder_is_configurable(self):
        self.assertIn("audio_description_save_folder", MAIN)
        self.assertIn("default_audio_description_save_folder", MAIN)
        self.assertIn("audio_description::save_folder_label()", MAIN)
        self.assertIn("DirDialog::builder", MAIN)
        self.assertIn("Settings::load().audio_description_save_folder", AUDIO)

    def test_conditional_controls_relayout_native_dialog(self):
        self.assertIn("panel_catalog.layout()", AUDIO)
        self.assertIn("panel_recognize.layout()", AUDIO)
        self.assertNotIn("panel_web.layout()", AUDIO)

    def test_all_mac_audio_description_locales_have_same_keys(self):
        files = sorted((ROOT / "i18n").glob("audio_description_*.json"))
        self.assertEqual(7, len(files))
        payloads = [json.loads(path.read_text(encoding="utf-8")) for path in files]
        expected = set(payloads[0])
        self.assertGreater(len(expected), 80)
        for path, payload in zip(files, payloads):
            with self.subTest(locale=path.name):
                self.assertEqual(expected, set(payload))


if __name__ == "__main__":
    unittest.main()

from __future__ import annotations

import json
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
BRIDGE = ROOT / "bridge" / "faster_whisper_bridge.py"
BUILD_SCRIPT = ROOT / "bridge" / "build_faster_whisper_bridge_macos.sh"
RUST_BRIDGE = ROOT / "src" / "faster_whisper_bridge.rs"
UI = ROOT / "src" / "media_transcription.rs"
MAIN = ROOT / "src" / "main.rs"
WORKFLOW = ROOT / ".github" / "workflows" / "macos-app-dmg.yml"
CATALINA_WORKFLOW = ROOT / ".github" / "workflows" / "macos-app-dmg-catalina.yml"
RELEASE_WORKFLOW = ROOT / ".github" / "workflows" / "crea-release-macos.yml"
LOCALES = tuple(ROOT.glob("i18n/media_transcription_*.json"))


def text(path: Path) -> str:
    return path.read_text(encoding="utf-8")


class MacOSMediaTranscriptionTests(unittest.TestCase):
    def test_bridge_keeps_windows_protocol_and_models(self):
        source = text(BRIDGE)
        self.assertIn("BRIDGE_PROTOCOL_VERSION = 2", source)
        self.assertIn("LONG_AUDIO_THRESHOLD_SECONDS = 30 * 60", source)
        self.assertIn('help="small | medium | large-v3"', source)
        self.assertIn('print(f"PROGRESS:{last_progress}"', source)
        self.assertIn('print("STAGE:model", flush=True)', source)
        self.assertIn('print("STAGE:transcribing", flush=True)', source)

    def test_macos_bridge_uses_same_stable_cpu_backend_on_all_architectures(self):
        source = text(BRIDGE)
        self.assertIn('{"device": "cpu", "compute_type": "int8"}', source)
        self.assertNotIn("mlx_whisper", source)
        self.assertNotIn("mlx.core", source)
        self.assertNotIn("BACKEND_FALLBACK:metal", source)
        self.assertNotIn("MlxWhisperModel", source)
        self.assertNotIn("MetalThenCpuModel", source)
        self.assertNotIn("is_apple_silicon", source)
        self.assertIn('parser.add_argument("--self-test", action="store_true")', source)
        for module in ("av", "ctranslate2", "faster_whisper", "onnxruntime"):
            self.assertIn(module, source)


    def test_short_inputs_are_predecoded_to_numpy_before_faster_whisper(self):
        source = text(BRIDGE)
        self.assertIn("def decode_audio_mono_16k(path):", source)
        self.assertIn("audio = decode_audio_mono_16k(input_path)", source)
        self.assertIn("model.transcribe(\n        audio,", source)
        self.assertNotIn("model.transcribe(\n        input_path,", source)

    def test_transcription_requires_explicit_audio_language_and_never_auto_detects(self):
        source = text(BRIDGE)
        self.assertNotIn("model.detect_language(", source)
        self.assertNotIn("def detect_source_language", source)
        self.assertNotIn('getattr(info, "language", "")', source)
        self.assertIn('"transcription language is required"', source)
        self.assertIn('task="transcribe"', source)
        self.assertNotIn('task="translate"', source)
        self.assertIn("vad_filter=False", source)
        self.assertNotIn("vad_filter=True", source)

    def test_long_transcription_forces_requested_language_for_every_chunk(self):
        source = text(BRIDGE)
        start = source.index("def transcribe_long_input")
        end = source.index("def transcribe_input", start)
        body = source[start:end]
        self.assertIn('selected_language = str(language or "").strip().lower()', body)
        self.assertNotIn("or None", body)
        self.assertIn('return {"ok": False, "error": "transcription language is required"}', body)
        self.assertIn("segments, _info = model.transcribe(", body)
        self.assertIn("language=selected_language", body)
        self.assertNotIn("language_probability", body)
        self.assertNotIn("detected_language", body)
        self.assertIn('task="transcribe"', body)
        self.assertIn('"language": selected_language', body)

    def test_short_transcription_forces_requested_language(self):
        source = text(BRIDGE)
        start = source.index("def transcribe_input")
        end = source.index("def worker_loop", start)
        body = source[start:end]
        self.assertIn('requested_language = str(language or "").strip().lower()', body)
        self.assertNotIn("or None", body)
        self.assertIn('return {"ok": False, "error": "transcription language is required"}', body)
        self.assertIn("segments, _info = model.transcribe(", body)
        self.assertIn("language=requested_language", body)
        self.assertNotIn("detect_language", body)
        self.assertNotIn("language_probability", body)
        self.assertIn('task="transcribe"', body)
        self.assertIn('"language": requested_language', body)

    def test_pyav_decoder_uses_concrete_audio_stream_not_audio_index_lookup(self):
        source = text(BRIDGE)
        self.assertIn("def first_audio_stream(container):", source)
        self.assertIn("for frame in container.decode(audio_stream):", source)
        self.assertNotIn("container.decode(audio=0)", source)
        self.assertNotIn("container.streams.audio", source)

    def test_worker_errors_are_logged_with_exception_type(self):
        bridge_source = text(BRIDGE)
        rust_source = text(RUST_BRIDGE)
        self.assertIn('f"{type(exc).__name__}: {exc}"', bridge_source)
        self.assertIn('transcription.worker failed error=', rust_source)

    def test_build_script_pins_stable_cpu_runtime_family_for_every_mac(self):
        source = text(BUILD_SCRIPT)
        self.assertIn('FASTER_WHISPER_VERSION="${FASTER_WHISPER_VERSION:-1.2.1}"', source)
        self.assertIn('CTRANSLATE2_VERSION="${CTRANSLATE2_VERSION:-4.3.1}"', source)
        self.assertIn('ONNXRUNTIME_VERSION="${ONNXRUNTIME_VERSION:-1.15.0}"', source)
        self.assertIn('PYAV_VERSION="${PYAV_VERSION:-12.3.0}"', source)
        self.assertIn('NUMPY_SPEC="${NUMPY_SPEC:-numpy>=1.24.2,<2}"', source)
        self.assertIn("--onedir", source)
        self.assertIn("--collect-data faster_whisper", source)
        self.assertIn("--self-test", source)
        self.assertNotIn("MLX_VERSION", source)
        self.assertNotIn("MLX_WHISPER_VERSION", source)
        self.assertNotIn("mlx-whisper", source)
        self.assertNotIn("mlx-metal", source)
        self.assertNotIn("--hidden-import mlx_whisper", source)
        self.assertNotIn("IS_APPLE_SILICON", source)
        self.assertNotIn("uname -m", source)


    def test_self_test_rejects_a_bundle_missing_silero_vad_asset(self):
        source = text(BRIDGE)
        self.assertIn('"assets" / "silero_vad_v6.onnx"', source)
        self.assertIn("bundled faster-whisper VAD model missing", source)
        self.assertIn("silero_vad_model.is_file()", source)

    def test_rust_bridge_is_bundled_and_models_are_cached_in_sonarpad(self):
        source = text(RUST_BRIDGE)
        self.assertIn('.join("transcription")', source)
        self.assertIn('.join("faster_whisper_bridge")', source)
        self.assertIn('.join("Application Support")', source)
        self.assertIn('.join("Sonarpad")', source)
        self.assertIn('.join("models")', source)
        self.assertIn('.join("faster-whisper")', source)
        self.assertIn(".process_group(0)", source)
        self.assertIn('.arg("--language")', source)
        self.assertIn('transcription.worker requested_language=', source)
        self.assertIn('strip_prefix("STAGE:")', source)
        self.assertIn('strip_prefix("PROGRESS:")', source)
        self.assertIn('pub backend: String', source)
        self.assertIn('pub language: String', source)
        self.assertIn('transcription.worker completed backend=', source)
        self.assertIn('compute_type={} language={}', source)
        self.assertNotIn("github.com", source.lower())

    def test_accessible_ui_has_explicit_source_destination_models_and_progress(self):
        source = text(UI)
        self.assertIn('tr("media_transcription.browse_input")', source)
        self.assertIn('tr("media_transcription.browse_output")', source)
        self.assertIn('tr("media_transcription.model.small")', source)
        self.assertIn('tr("media_transcription.model.medium")', source)
        self.assertIn('tr("media_transcription.model.large")', source)
        self.assertIn('tr("media_transcription.audio_language")', source)
        self.assertIn("let audio_language = Choice::builder(&panel).build();", source)
        self.assertIn("let saved_audio_language = Settings::load().transcription_audio_language;", source)
        self.assertIn("selected_audio_language(&audio_language)", source)
        self.assertIn("Gauge::builder(&panel).with_range(100).build()", source)
        self.assertIn("gauge_tick.set_value(snapshot.progress.clamp(0, 99))", source)
        self.assertNotIn("gauge_tick.pulse()", source)
        self.assertIn("run_with_progress", source)
        self.assertIn("FileDialogStyle::FileMustExist", source)
        self.assertIn("FileDialogStyle::OverwritePrompt", source)

        # Creation order is also keyboard focus order in wxWidgets for these controls.
        positions = [
            source.index("let input_button = Button::builder"),
            source.index("let input = TextCtrl::builder"),
            source.index("let output_button = Button::builder"),
            source.index("let output = TextCtrl::builder"),
            source.index("let model = Choice::builder"),
            source.index("let audio_language = Choice::builder"),
            source.index("let start = Button::builder"),
            source.index("let close = Button::builder"),
        ]
        self.assertEqual(positions, sorted(positions))

    def test_italian_labels_match_requested_wording(self):
        values = json.loads(text(ROOT / "i18n" / "media_transcription_it.json"))
        self.assertEqual(values["media_transcription.title"], "Trascrivi file media")
        self.assertEqual(values["media_transcription.browse_input"], "Scegli file media...")
        self.assertEqual(values["media_transcription.browse_output"], "Cambia cartella di destinazione...")
        self.assertEqual(values["media_transcription.model.small"], "Piccolo")
        self.assertEqual(values["media_transcription.audio_language"], "Lingua dell'audio:")
        self.assertEqual(values["media_transcription.model.medium"], "Medio")
        self.assertEqual(values["media_transcription.model.large"], "Grande v3")

    def test_audio_language_picker_remembers_last_choice_and_offers_common_languages(self):
        source = text(UI)
        for label, code in (
            ("Italiano", "it"),
            ("English", "en"),
            ("Español", "es"),
            ("Français", "fr"),
            ("Deutsch", "de"),
            ("Русский", "ru"),
            ("中文", "zh"),
        ):
            self.assertIn(f'(\"{label}\", \"{code}\")', source)
        self.assertIn(".position(|(_, code)| *code == saved_audio_language.as_str())", source)
        self.assertIn("audio_language.set_selection(default_language_index as u32);", source)
        self.assertIn("audio_language.on_selection_changed", source)
        self.assertIn("settings.transcription_audio_language = selected", source)
        self.assertIn("settings.save();", source)
        self.assertIn("chosen_language", source)
        self.assertIn("transcription.request model={} language={}", source)

        settings_source = text(ROOT / "src" / "main.rs")
        self.assertIn("transcription_audio_language: String", settings_source)
        self.assertIn("default_transcription_language_for_ui", settings_source)
        self.assertIn("is_supported_transcription_language", settings_source)

    def test_media_transcription_locale_key_sets_match(self):
        self.assertEqual(len(LOCALES), 7)
        locale_maps = [json.loads(text(path)) for path in LOCALES]
        keys = set(locale_maps[0])
        self.assertTrue(keys)
        for locale_map in locale_maps[1:]:
            self.assertEqual(set(locale_map), keys)

    def test_destination_button_says_folder_in_every_language(self):
        expected = {
            "it": "Cambia cartella di destinazione...",
            "en": "Change destination folder...",
            "fr": "Changer le dossier de destination...",
            "es": "Cambiar carpeta de destino...",
            "pt": "Alterar pasta de destino...",
            "cs": "Změnit cílovou složku...",
            "pl": "Zmień folder docelowy...",
        }
        for lang, value in expected.items():
            values = json.loads(text(ROOT / "i18n" / f"media_transcription_{lang}.json"))
            self.assertEqual(values["media_transcription.browse_output"], value)

    def test_tools_menu_registers_and_opens_transcription_dialog(self):
        source = text(MAIN)
        self.assertIn("mod faster_whisper_bridge;", source)
        self.assertIn("mod media_transcription;", source)
        self.assertIn("const ID_TOOLS_MEDIA_TRANSCRIPTION: i32 = 2377;", source)
        self.assertIn("media_transcription::menu_label()", source)
        self.assertIn("media_transcription::open_dialog(&f_menu);", source)

    def test_all_macos_builds_bundle_sign_and_verify_transcription_worker(self):
        expected = "Resources/transcription/faster_whisper_bridge/faster_whisper_bridge"
        for workflow in (WORKFLOW, CATALINA_WORKFLOW):
            source = text(workflow)
            self.assertIn("build_faster_whisper_bridge_macos.sh", source)
            self.assertIn('dist/faster-whisper-worker/faster_whisper_bridge', source)
            self.assertIn('TRANSCRIPTION_DIR="${RESOURCES_DIR}/transcription"', source)
            self.assertIn('TRANSCRIPTION_BIN="${TRANSCRIPTION_DIR}/faster_whisper_bridge"', source)
            self.assertIn("codesign --verify --strict --verbose=2 \"${TRANSCRIPTION_BIN}\"", source)
            self.assertIn(expected, source)

    def test_catalina_scans_every_transcription_macho_for_10_15(self):
        source = text(CATALINA_WORKFLOW)
        marker = 'TRANSCRIPTION_DIR="dist/stage/Sonarpad.app/Contents/Resources/transcription/faster_whisper_bridge"'
        self.assertIn(marker, source)
        tail = source[source.index(marker):]
        self.assertIn('find "${TRANSCRIPTION_DIR}" -type f -print0', tail)
        self.assertIn('file "${bin}" | grep -q "Mach-O"', tail)
        self.assertIn('check_max_macos_version "${bin}" "10.15"', tail)

    def test_release_refuses_archives_without_transcription_worker(self):
        source = text(RELEASE_WORKFLOW)
        self.assertIn(
            'expected_transcription_worker="Sonarpad.app/Contents/Resources/transcription/faster_whisper_bridge/faster_whisper_bridge"',
            source,
        )
        self.assertIn('grep -Fx "$expected_transcription_worker"', source)


if __name__ == "__main__":
    unittest.main()

from __future__ import annotations

import importlib.util
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

    def test_macos_bridge_prefers_mlx_metal_on_apple_silicon_with_cpu_fallback(self):
        source = text(BRIDGE)
        self.assertIn('class MlxWhisperModel:', source)
        self.assertIn('mx.metal.is_available()', source)
        self.assertIn('mx.set_default_device(mx.gpu)', source)
        self.assertIn('"small": "mlx-community/whisper-small-mlx"', source)
        self.assertIn('"medium": "mlx-community/whisper-medium-mlx"', source)
        self.assertIn('"large-v3": "mlx-community/whisper-large-v3-mlx"', source)
        self.assertIn('class MetalThenCpuModel:', source)
        self.assertIn('BACKEND_FALLBACK:metal->cpu', source)
        self.assertIn('"device": "cpu", "compute_type": "int8"', source)
        self.assertNotIn("SONARPAD_FORCE_CUDA", source)
        self.assertIn('parser.add_argument("--self-test", action="store_true")', source)
        for module in ("av", "ctranslate2", "faster_whisper", "onnxruntime", "mlx_whisper"):
            self.assertIn(module, source)


    def test_mlx_adapter_keeps_same_three_user_model_choices(self):
        source = text(BRIDGE)
        self.assertEqual(source.count('mlx-community/whisper-small-mlx'), 1)
        self.assertEqual(source.count('mlx-community/whisper-medium-mlx'), 1)
        self.assertEqual(source.count('mlx-community/whisper-large-v3-mlx'), 1)
        self.assertIn('path_or_hf_repo=self._repo', source)
        self.assertIn('condition_on_previous_text=condition_on_previous_text', source)

    def test_mlx_short_inputs_are_decoded_with_pyav_not_external_ffmpeg(self):
        source = text(BRIDGE)
        self.assertIn('def decode_audio_mono_16k(path):', source)
        self.assertIn('av.AudioResampler(format="s16", layout="mono", rate=WHISPER_SAMPLE_RATE)', source)
        self.assertIn('audio = decode_audio_mono_16k(os.fspath(audio))', source)

    def test_runtime_metal_failure_switches_once_to_cpu(self):
        spec = importlib.util.spec_from_file_location("sonarpad_fw_bridge_test", BRIDGE)
        module = importlib.util.module_from_spec(spec)
        assert spec.loader is not None
        spec.loader.exec_module(module)

        class MetalModel:
            def transcribe(self, *_args, **_kwargs):
                raise RuntimeError("simulated metal failure")

        class CpuModel:
            def __init__(self):
                self.calls = 0

            def transcribe(self, *_args, **_kwargs):
                self.calls += 1
                return ["cpu"], "info"

        created = []

        def cpu_factory():
            model = CpuModel()
            created.append(model)
            return model

        model = module.MetalThenCpuModel(MetalModel(), cpu_factory)
        self.assertEqual(model.transcribe("audio"), (["cpu"], "info"))
        self.assertEqual(model.backend, "cpu")
        self.assertEqual(model.compute_type, "int8")
        self.assertEqual(len(created), 1)
        self.assertEqual(model.transcribe("audio2"), (["cpu"], "info"))
        self.assertEqual(len(created), 1)
        self.assertEqual(created[0].calls, 2)

    def test_model_mapping_is_exact(self):
        spec = importlib.util.spec_from_file_location("sonarpad_fw_bridge_mapping_test", BRIDGE)
        module = importlib.util.module_from_spec(spec)
        assert spec.loader is not None
        spec.loader.exec_module(module)
        self.assertEqual(module.mlx_model_repo("small"), "mlx-community/whisper-small-mlx")
        self.assertEqual(module.mlx_model_repo("medium"), "mlx-community/whisper-medium-mlx")
        self.assertEqual(module.mlx_model_repo("large-v3"), "mlx-community/whisper-large-v3-mlx")
        with self.assertRaises(RuntimeError):
            module.mlx_model_repo("unknown")

    def test_build_script_pins_catalina_compatible_runtime_family(self):
        source = text(BUILD_SCRIPT)
        self.assertIn('FASTER_WHISPER_VERSION="${FASTER_WHISPER_VERSION:-1.2.1}"', source)
        self.assertIn('CTRANSLATE2_VERSION="${CTRANSLATE2_VERSION:-4.3.1}"', source)
        self.assertIn('ONNXRUNTIME_VERSION="${ONNXRUNTIME_VERSION:-1.15.0}"', source)
        self.assertIn('PYAV_VERSION="${PYAV_VERSION:-12.3.0}"', source)
        self.assertIn('NUMPY_SPEC="${NUMPY_SPEC:-numpy>=1.24.2,<2}"', source)
        self.assertIn("--onedir", source)
        self.assertIn("--self-test", source)
        self.assertIn('"mlx==${MLX_VERSION}"', source)
        self.assertIn('"mlx-metal==${MLX_VERSION}"', source)
        self.assertIn('--no-deps "mlx-whisper==${MLX_WHISPER_VERSION}"', source)
        self.assertIn('if [[ "$(uname -m)" == "arm64" ]]; then', source)
        self.assertNotIn('"torch"', source)
        self.assertIn('--hidden-import mlx_whisper', source)
        self.assertNotIn('--collect-all mlx_whisper', source)

    def test_rust_bridge_is_bundled_and_models_are_cached_in_sonarpad(self):
        source = text(RUST_BRIDGE)
        self.assertIn('.join("transcription")', source)
        self.assertIn('.join("faster_whisper_bridge")', source)
        self.assertIn('.join("Application Support")', source)
        self.assertIn('.join("Sonarpad")', source)
        self.assertIn('.join("models")', source)
        self.assertIn('.join("faster-whisper")', source)
        self.assertIn(".process_group(0)", source)
        self.assertIn('strip_prefix("STAGE:")', source)
        self.assertIn('strip_prefix("PROGRESS:")', source)
        self.assertIn('pub backend: String', source)
        self.assertIn('transcription.worker completed backend=', source)
        self.assertNotIn("github.com", source.lower())

    def test_accessible_ui_has_explicit_source_destination_models_and_progress(self):
        source = text(UI)
        self.assertIn('tr("media_transcription.browse_input")', source)
        self.assertIn('tr("media_transcription.browse_output")', source)
        self.assertIn('tr("media_transcription.model.small")', source)
        self.assertIn('tr("media_transcription.model.medium")', source)
        self.assertIn('tr("media_transcription.model.large")', source)
        self.assertIn("Gauge::builder(&panel).with_range(100).build()", source)
        self.assertIn("gauge_tick.set_value(snapshot.progress.clamp(0, 99))", source)
        self.assertNotIn("gauge_tick.pulse()", source)
        self.assertIn("run_with_progress", source)
        self.assertIn("FileDialogStyle::FileMustExist", source)
        self.assertIn("FileDialogStyle::OverwritePrompt", source)

        # Creation order is also keyboard focus order in wxWidgets for these controls.
        positions = [
            source.index("let input = TextCtrl::builder"),
            source.index("let input_button = Button::builder"),
            source.index("let output = TextCtrl::builder"),
            source.index("let output_button = Button::builder"),
            source.index("let model = Choice::builder"),
            source.index("let start = Button::builder"),
            source.index("let close = Button::builder"),
        ]
        self.assertEqual(positions, sorted(positions))

    def test_italian_labels_match_requested_wording(self):
        values = json.loads(text(ROOT / "i18n" / "media_transcription_it.json"))
        self.assertEqual(values["media_transcription.title"], "Trascrivi file media")
        self.assertEqual(values["media_transcription.browse_input"], "Scegli file media...")
        self.assertEqual(values["media_transcription.browse_output"], "Cambia file di destinazione...")
        self.assertEqual(values["media_transcription.model.small"], "Piccolo")
        self.assertEqual(values["media_transcription.model.medium"], "Medio")
        self.assertEqual(values["media_transcription.model.large"], "Grande v3")

    def test_media_transcription_locale_key_sets_match(self):
        self.assertEqual(len(LOCALES), 7)
        locale_maps = [json.loads(text(path)) for path in LOCALES]
        keys = set(locale_maps[0])
        self.assertTrue(keys)
        for locale_map in locale_maps[1:]:
            self.assertEqual(set(locale_map), keys)

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

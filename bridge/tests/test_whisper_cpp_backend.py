import contextlib
import io
import json
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch

import numpy as np
import faster_whisper_bridge as bridge
import whisper_cpp_backend as backend


class WhisperCppBackendTests(unittest.TestCase):
    def test_metal_selection_requires_native_apple_silicon_and_binary(self):
        with tempfile.TemporaryDirectory() as folder:
            cpu = Path(folder) / 'whisper-cli'
            metal = Path(folder) / 'whisper-metal'
            metal.touch()
            for system, arch, expected in [('darwin', 'arm64', metal), ('darwin', 'x86_64', None), ('win32', 'arm64', None)]:
                with patch.object(backend.sys, 'platform', system), patch.object(backend.platform, 'machine', return_value=arch):
                    self.assertEqual(backend.bundled_metal_binary(cpu), expected)
            metal.unlink()
            with patch.object(backend.sys, 'platform', 'darwin'), patch.object(backend.platform, 'machine', return_value='arm64'):
                self.assertIsNone(backend.bundled_metal_binary(cpu))

    def test_metal_failure_retries_cpu_once_and_stays_on_cpu(self):
        factory = Mock()
        model = backend.CppThenCpuModel('cpu-cli', 'model.bin', factory, 'metal-cli')
        calls = []
        def run(audio, options):
            calls.append((model.binary.name, model.use_metal))
            if model.use_metal:
                raise RuntimeError('GPU failed')
            return [SimpleNamespace(text='Recovered')]
        with patch.object(model, '_transcribe_cpp', side_effect=run), contextlib.redirect_stderr(io.StringIO()):
            for _ in range(2):
                segments, _ = model.transcribe(np.zeros(16), language='it', task='transcribe')
                self.assertEqual([s.text for s in segments], ['Recovered'])
        self.assertEqual(calls, [('metal-cli', True), ('cpu-cli', False), ('cpu-cli', False)])
        self.assertEqual(model.backend, 'whisper.cpp-cpu')
        factory.assert_not_called()

    def test_metal_and_cpp_failures_reach_existing_backend(self):
        existing = Mock()
        existing.transcribe.return_value = (iter([]), None)
        factory = Mock(return_value=existing)
        model = backend.CppThenCpuModel('cpu-cli', 'model.bin', factory, 'metal-cli')
        with patch.object(model, '_transcribe_cpp', side_effect=RuntimeError('failed')) as cpp, contextlib.redirect_stderr(io.StringIO()):
            model.transcribe(np.zeros(16), language='it', task='transcribe')
            self.assertEqual(cpp.call_count, 2)
        self.assertEqual(model.backend, 'cpu')
        factory.assert_called_once_with()

    def test_metal_cancellation_never_falls_back(self):
        for error in (KeyboardInterrupt, SystemExit):
            factory = Mock()
            model = backend.CppThenCpuModel('cpu-cli', 'model.bin', factory, 'metal-cli')
            with patch.object(model, '_transcribe_cpp', side_effect=error) as cpp:
                with self.assertRaises(error):
                    model.transcribe(np.zeros(16), language='it', task='transcribe')
                cpp.assert_called_once()
            factory.assert_not_called()
            self.assertTrue(model.use_metal)

    def test_metal_requires_backend_confirmation_and_omits_no_gpu(self):
        for confirmed in (True, False):
            model = backend.CppThenCpuModel('cpu-cli', 'model.bin', Mock(), 'metal-cli')
            def start(command, **kwargs):
                self.assertEqual(Path(command[0]).name, 'metal-cli')
                self.assertNotIn('-ng', command)
                output = Path(command[command.index('-of') + 1]).with_suffix('.json')
                output.write_text(json.dumps({'result': {'language': 'it'}, 'transcription': []}))
                log = 'whisper_backend_init_gpu: using Metal backend\n' if confirmed else 'CPU only\n'
                return Mock(stderr=io.StringIO(log), wait=Mock(return_value=0), poll=Mock(return_value=0))
            with patch.object(backend.subprocess, 'Popen', side_effect=start):
                if confirmed:
                    self.assertEqual(model._transcribe_cpp(np.zeros(16), {'language': 'it', 'task': 'transcribe'}), [])
                    self.assertEqual(model.backend, 'whisper.cpp-metal')
                else:
                    with self.assertRaisesRegex(RuntimeError, 'Metal was not activated'):
                        model._transcribe_cpp(np.zeros(16), {'language': 'it', 'task': 'transcribe'})

    def test_old_macos_keeps_existing_backend(self):
        with patch.object(backend.sys, "platform", "darwin"):
            for version, supported in [("10.15.7", False), ("12.7.6", False), ("13.2", False), ("13.3", True), ("14.0", True), ("", False)]:
                with self.subTest(version=version), patch.object(backend.platform, "mac_ver", return_value=(version, (), "")):
                    self.assertEqual(backend.supports_cpp_runtime(), supported)

    def test_incompatible_system_never_downloads_or_launches_cpp(self):
        factory = Mock(return_value="existing")
        with patch.object(backend, "supports_cpp_runtime", return_value=False), patch.object(backend, "bundled_cpp_binary") as binary:
            self.assertEqual(backend.prefer_cpp_model("small", None, factory), "existing")
            binary.assert_not_called()
            factory.assert_called_once_with()

    def test_missing_binary_and_failed_download_use_fallback(self):
        for binary in (None, Path("whisper-cli")):
            factory = Mock(return_value="existing")
            with patch.object(backend, "supports_cpp_runtime", return_value=True), patch.object(backend, "bundled_cpp_binary", return_value=binary), patch.object(backend, "validate_cpp_binary"), patch.object(backend, "download_cpp_model", side_effect=OSError("offline")), contextlib.redirect_stderr(io.StringIO()):
                self.assertEqual(backend.prefer_cpp_model("small", None, factory), "existing")
                factory.assert_called_once_with()

    def test_model_download_preserves_selection_and_uses_separate_pinned_cache(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "model.bin"
            path.write_bytes(b"lmgg-test")
            downloader = Mock(return_value=str(path))
            with patch.dict("sys.modules", {"huggingface_hub": SimpleNamespace(hf_hub_download=downloader)}):
                for model in ("small", "medium", "large-v3"):
                    backend.download_cpp_model(model, str(Path(folder) / "faster-whisper"))
                    kwargs = downloader.call_args.kwargs
                    self.assertEqual(kwargs["filename"], f"ggml-{model}.bin")
                    self.assertEqual(kwargs["revision"], backend.MODEL_REVISION)
                    self.assertEqual(Path(kwargs["cache_dir"]).name, "whisper-cpp")
                path.write_bytes(b"bad!")
                with self.assertRaises(ValueError):
                    backend.download_cpp_model("small", folder)

    def test_cpp_success_forces_cpu_language_and_reports_progress(self):
        fallback = Mock()
        model = backend.CppThenCpuModel("whisper-cli", "model.bin", fallback)
        progress = []
        model.progress_callback = progress.append
        calls = []
        def start(command, **kwargs):
            calls.append((command, kwargs))
            output = Path(command[command.index("-of") + 1]).with_suffix(".json")
            output.write_text(json.dumps({"result": {"language": "it"}, "transcription": [{"text": "Ciao", "offsets": {"from": 0, "to": 1000}}]}))
            return Mock(stderr=io.StringIO("progress = 50%\nprogress = 100%\n"), wait=Mock(return_value=0), poll=Mock(return_value=0))
        with patch.object(backend.subprocess, "Popen", side_effect=start):
            segments, info = model.transcribe(np.zeros(16000), language="it", task="transcribe", beam_size=5)
            self.assertEqual([segment.text for segment in segments], ["Ciao"])
            self.assertEqual(info.language, "it")
        command, kwargs = calls[0]
        self.assertIn("-ng", command)
        self.assertEqual(command[command.index("-l") + 1], "it")
        self.assertNotIn("start_new_session", kwargs)
        self.assertEqual(progress, [50, 100])
        self.assertEqual(model.backend, "whisper.cpp-cpu")
        fallback.assert_not_called()
        self.assertFalse(Path(command[command.index("-f") + 1]).exists())

    def test_runtime_failure_switches_once_without_duplicate_segments(self):
        existing = Mock()
        existing.transcribe.side_effect = lambda *a, **k: (iter([SimpleNamespace(text="Recovered")]), None)
        factory = Mock(return_value=existing)
        model = backend.CppThenCpuModel("whisper-cli", "model.bin", factory)
        with patch.object(model, "_transcribe_cpp", side_effect=RuntimeError("invalid result")) as cpp, contextlib.redirect_stderr(io.StringIO()):
            for _ in range(2):
                segments, _ = model.transcribe(np.zeros(16), language="it", task="transcribe")
                self.assertEqual([s.text for s in segments], ["Recovered"])
            cpp.assert_called_once()
            factory.assert_called_once_with()
        self.assertEqual(model.backend, "cpu")
        self.assertEqual(model.compute_type, "int8")

    def test_cancellation_is_not_retried(self):
        factory = Mock()
        model = backend.CppThenCpuModel("whisper-cli", "model.bin", factory)
        with patch.object(model, "_transcribe_cpp", side_effect=KeyboardInterrupt):
            with self.assertRaises(KeyboardInterrupt):
                model.transcribe(np.zeros(16), language="it", task="transcribe")
        factory.assert_not_called()

    def test_invalid_results_trigger_error_instead_of_partial_transcript(self):
        for payload in ({}, {"result": {"language": "en"}, "transcription": []},
                        {"result": {"language": "it"}, "transcription": [{"text": "Bad", "offsets": {"from": 10, "to": 0}}]}):
            with self.assertRaises((ValueError, KeyError)):
                backend.parse_segments(payload, "it", 10)
        self.assertEqual(backend.parse_segments({"result": {"language": "it"}, "transcription": []}, "it", 10), [])

    def test_long_chunk_progress_is_global_monotonic_and_never_completes_early(self):
        model = SimpleNamespace(progress_callback=None)
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            state = bridge.install_chunk_progress(model, 40, 900, 900, 1800)
            for value in (0, 50, 25, 100):
                model.progress_callback(value)
        self.assertEqual(output.getvalue().splitlines(), ["PROGRESS:50", "PROGRESS:75", "PROGRESS:99"])
        self.assertEqual(state[0], 99)

    def test_build_keeps_independent_cpu_and_arm_only_metal_with_catalina_fallback(self):
        root = Path(__file__).resolve().parents[1]
        build = (root / "build_whisper_cpp_macos.sh").read_text(encoding="utf-8")
        for flag in ("-DGGML_METAL=OFF", "-DWHISPER_COREML=OFF", "-DGGML_NATIVE=OFF", "-DBUILD_SHARED_LIBS=OFF", "-DCMAKE_OSX_DEPLOYMENT_TARGET=13.3"):
            self.assertIn(flag, build)
        self.assertIn("tail -n +2", build)
        self.assertIn('if [ "$(uname -m)" = "arm64" ]; then', build)
        self.assertIn('-DGGML_METAL=ON', build)
        self.assertIn('-DGGML_METAL_EMBED_LIBRARY=ON', build)
        self.assertIn('$OUT_DIR/whisper-metal', build)
        self.assertIn('!= "intel-catalina"', (root / "build_faster_whisper_bridge_macos.sh").read_text(encoding="utf-8"))


if __name__ == "__main__":
    unittest.main()

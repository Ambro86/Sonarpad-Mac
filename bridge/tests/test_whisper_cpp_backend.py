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

    def test_build_is_cpu_only_and_catalina_bundle_keeps_fallback(self):
        root = Path(__file__).resolve().parents[1]
        build = (root / "build_whisper_cpp_macos.sh").read_text(encoding="utf-8")
        for flag in ("-DGGML_METAL=OFF", "-DWHISPER_COREML=OFF", "-DGGML_NATIVE=OFF", "-DBUILD_SHARED_LIBS=OFF", "-DCMAKE_OSX_DEPLOYMENT_TARGET=13.3"):
            self.assertIn(flag, build)
        self.assertIn("tail -n +2", build)
        self.assertIn('!= "intel-catalina"', (root / "build_faster_whisper_bridge_macos.sh").read_text(encoding="utf-8"))


if __name__ == "__main__":
    unittest.main()

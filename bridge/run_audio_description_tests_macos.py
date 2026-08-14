"""Run Sonarpad's cross-platform audio-description tests plus macOS host checks."""
from __future__ import annotations

import sys
import unittest
from pathlib import Path

BRIDGE_DIR = Path(__file__).resolve().parent
RUNTIME_DIR = BRIDGE_DIR / "audio_description_runtime"
TESTS_DIR = BRIDGE_DIR / "tests"
for path in (BRIDGE_DIR, RUNTIME_DIR, TESTS_DIR):
    value = str(path)
    if value not in sys.path:
        sys.path.insert(0, value)

MODULES = (
    "test_audio_description_logging",
    "test_audio_description_bridge_spec",
    "test_bridge_protocol",
    "test_chunk_timestamps",
    "test_gemini_models",
    "test_gemini_retry",
    "test_language_detection",
    "test_speech_detector",
    "test_macos_audio_description_host",
    "test_macos_audio_description_resume_selector",
    "test_macos_audio_description_transport",
    "test_macos_media_transcription",
    "test_macos_la7_play",
    "test_macos_log_retention",
)


def main() -> int:
    loader = unittest.defaultTestLoader
    suite = unittest.TestSuite()
    for module_name in MODULES:
        suite.addTests(loader.loadTestsFromName(module_name))
    result = unittest.TextTestRunner(verbosity=2).run(suite)
    print(f"Sonarpad macOS audio-description tests executed: {result.testsRun}")
    return 0 if result.wasSuccessful() else 1


if __name__ == "__main__":
    raise SystemExit(main())

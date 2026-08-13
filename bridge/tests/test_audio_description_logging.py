from __future__ import annotations

import logging
import unittest
from unittest import mock

import audio_description_bridge as bridge
from audio_describer.utils import logger as bridge_logger


class AudioDescriptionLoggingTests(unittest.TestCase):
    def test_worker_logger_has_no_separate_file_handler(self):
        self.assertFalse(
            any(isinstance(handler, logging.FileHandler) for handler in bridge_logger.app_logger.handlers)
        )
        self.assertTrue(
            any(
                isinstance(handler, logging.StreamHandler)
                and not isinstance(handler, logging.FileHandler)
                for handler in bridge_logger.app_logger.handlers
            )
        )

    def test_real_job_logs_start_to_stderr_without_resetting_a_private_log(self):
        with mock.patch.object(
            bridge.sys, "argv", ["audio_description_bridge.py", "--request", "job.json"]
        ), mock.patch.object(bridge, "_read_request", return_value={"job": True}), mock.patch.object(
            bridge, "run", return_value={"ok": True}
        ), mock.patch.object(
            bridge, "_emit"
        ), mock.patch.object(
            bridge.app_logger, "info"
        ) as info:
            self.assertEqual(bridge.main(), 0)

        info.assert_any_call(
            "Starting new audio-description job; logging forwarded to Sonarpad log.txt."
        )

    def test_self_test_does_not_need_a_separate_log_file(self):
        with mock.patch.object(
            bridge.sys, "argv", ["audio_description_bridge.py", "--self-test"]
        ), mock.patch.object(
            bridge.speech_detector,
            "get_bundled_model_path",
            return_value=__file__,
        ), mock.patch.object(
            bridge.gemini_helpers,
            "validate_gemini_runtime",
            return_value={"available": True, "module_path": "test"},
        ), mock.patch.object(bridge, "_emit"):
            self.assertEqual(bridge.main(), 0)


if __name__ == "__main__":
    unittest.main()

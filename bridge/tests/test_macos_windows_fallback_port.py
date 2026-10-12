"""Behavioral coverage for the Windows fallback port, without network requests."""
from __future__ import annotations

import errno
import io
import json
import unittest
import urllib.error
from types import SimpleNamespace
from unittest import mock

import audio_description_bridge as bridge
from audio_describer.core import audio_describer, gemini_helpers, sonarpad_service


class MacWindowsFallbackPortTests(unittest.TestCase):
    def _service(self, responses):
        client = sonarpad_service.SonarpadServiceClient.__new__(
            sonarpad_service.SonarpadServiceClient
        )
        client._request_json = mock.Mock(side_effect=responses)
        client._activate = mock.Mock()
        client._uploads = {}
        return client

    def test_successful_paid_request_keeps_original_body_and_single_call(self):
        client = self._service([{"candidates": [{"finishReason": "STOP"}]}])
        with mock.patch.object(sonarpad_service.time, "sleep") as sleep:
            result = client.generate(
                ["testo originale"],
                {"temperature": 0.2, "response_mime_type": "application/json"},
            )
        self.assertEqual(result.candidates[0].finish_reason.name, "STOP")
        client._request_json.assert_called_once()
        method, endpoint, body = client._request_json.call_args.args
        self.assertEqual((method, endpoint), ("POST", "/v1/generate"))
        self.assertEqual(body, {
            "contents": [{"role": "user", "parts": [{"text": "testo originale"}]}],
            "generationConfig": {"temperature": 0.2, "responseMimeType": "application/json"},
        })
        self.assertEqual(client._request_json.call_args.kwargs["timeout"], 650)
        self.assertTrue(client._request_json.call_args.kwargs["headers"]["X-Idempotency-Key"].startswith("ad_"))
        client._activate.assert_not_called()
        sleep.assert_not_called()

    def test_paid_request_retains_key_and_body_through_session_network_and_poll_recovery(self):
        error = sonarpad_service.SonarpadServiceError
        client = self._service([
            error("invalid_session", 401),
            error("Sonarpad AI network error: connection reset", 503),
            error("temporary server error", 500),
            error("bad gateway", 502),
            error("gateway timeout", 504),
            error("generation_in_progress", 503, retry_after=7),
            error("generation_in_progress", 503, retry_after=2),
            {"text": "risposta riuscita"},
        ])
        with mock.patch.object(sonarpad_service.time, "sleep") as sleep, mock.patch.object(
            sonarpad_service.uuid, "uuid4", return_value=SimpleNamespace(hex="same-request")
        ) as new_id:
            result = client.generate(["mantieni questa richiesta"], {"temperature": 0.2})
        self.assertEqual(result.text, "risposta riuscita")
        self.assertEqual(client._request_json.call_count, 8)
        first_body = client._request_json.call_args_list[0].args[2]
        for call in client._request_json.call_args_list:
            self.assertEqual(call.args[:2], ("POST", "/v1/generate"))
            self.assertIs(call.args[2], first_body)
            self.assertEqual(call.kwargs["headers"], {"X-Idempotency-Key": "ad_same-request"})
        client._activate.assert_called_once_with()
        new_id.assert_called_once_with()
        self.assertEqual([call.args[0] for call in sleep.call_args_list], [3, 6, 9, 12, 7, 2])

    def test_generation_in_progress_is_polling_even_after_six_responses(self):
        pending = sonarpad_service.SonarpadServiceError("generation_in_progress", 503, retry_after=1)
        client = self._service([pending] * 9 + [{"text": "ready"}])
        with mock.patch.object(sonarpad_service.time, "sleep") as sleep:
            result = client.generate(["unchanged"], None)
        self.assertEqual(result.text, "ready")
        self.assertEqual(client._request_json.call_count, 10)
        keys = {call.kwargs["headers"]["X-Idempotency-Key"] for call in client._request_json.call_args_list}
        self.assertEqual(len(keys), 1)
        self.assertEqual(sleep.call_args_list, [mock.call(1)] * 9)

    def test_persistent_ambiguous_failure_has_six_same_id_retries(self):
        error = sonarpad_service.SonarpadServiceError("bad gateway", 502)
        client = self._service([error] * 7)
        with mock.patch.object(sonarpad_service.time, "sleep") as sleep:
            with self.assertRaises(sonarpad_service.SonarpadServiceError) as raised:
                client.generate(["same work"], None)
        self.assertIs(raised.exception, error)
        self.assertEqual(client._request_json.call_count, 7)
        keys = {call.kwargs["headers"]["X-Idempotency-Key"] for call in client._request_json.call_args_list}
        self.assertEqual(len(keys), 1)
        self.assertEqual([call.args[0] for call in sleep.call_args_list], [3, 6, 9, 12, 15, 18])

    def test_permanent_and_file_verification_errors_return_to_existing_caller(self):
        for status, message in (
            (400, "invalid request"),
            (401, "authentication failed"),
            (402, "insufficient balance"),
            (403, "access denied"),
            (429, "quota exceeded"),
            (503, "file_verification_failed"),
        ):
            with self.subTest(status=status, message=message):
                error = sonarpad_service.SonarpadServiceError(message, status)
                client = self._service([error])
                with mock.patch.object(sonarpad_service.time, "sleep") as sleep:
                    with self.assertRaises(sonarpad_service.SonarpadServiceError) as raised:
                        client.generate(["work"], None)
                self.assertIs(raised.exception, error)
                client._request_json.assert_called_once()
                client._activate.assert_not_called()
                sleep.assert_not_called()

    def test_invalid_session_is_reactivated_only_once_for_generation(self):
        error = sonarpad_service.SonarpadServiceError("invalid_session", 401)
        client = self._service([error, error])
        with self.assertRaises(sonarpad_service.SonarpadServiceError):
            client.generate(["same work"], None)
        self.assertEqual(client._request_json.call_count, 2)
        client._activate.assert_called_once_with()
        keys = {call.kwargs["headers"]["X-Idempotency-Key"] for call in client._request_json.call_args_list}
        self.assertEqual(len(keys), 1)

    def test_upload_cleanup_refreshes_once_and_retains_record_on_failure(self):
        error = sonarpad_service.SonarpadServiceError("invalid_session", 401)
        for second_result in (None, error):
            with self.subTest(second_result=second_result):
                client = self._service([error, second_result])
                record = sonarpad_service._UploadRecord("upload-owned", "files/owned", "video/mp4")
                client._uploads[record.file_name] = record
                files = sonarpad_service._FilesApi(client)
                if second_result is None:
                    self.assertIsNone(files.delete(record.file_name))
                    self.assertNotIn(record.file_name, client._uploads)
                else:
                    with self.assertRaises(sonarpad_service.SonarpadServiceError):
                        files.delete(record.file_name)
                    self.assertIs(client._uploads[record.file_name], record)
                client._activate.assert_called_once_with()
                self.assertEqual(client._request_json.call_count, 2)
                for call in client._request_json.call_args_list:
                    self.assertEqual(call.args, ("POST", "/v1/upload/delete", {"upload_id": "upload-owned"}))

    def test_http_poll_delay_is_bounded_and_macos_verified_context_is_preserved(self):
        cases = [("{\"retry_after\":7}", 7), ("{\"retry_after\":999999}", 5),
                 ("{\"retry_after\":0}", 5), ("{\"retry_after\":\"bad\"}", 5),
                 ("null", 5), ("[]", 5), ("bad json", 5)]
        for raw, expected in cases:
            with self.subTest(raw=raw):
                self.assertEqual(sonarpad_service._http_retry_after(raw), expected)
        client = sonarpad_service.SonarpadServiceClient.__new__(sonarpad_service.SonarpadServiceClient)
        client.base_url = "https://example.invalid"
        client.session_token = "sst_test"
        client._ssl_context = object()
        response_error = urllib.error.HTTPError(
            "https://example.invalid/v1/generate", 503, "pending", {},
            io.BytesIO(b'{"error":"generation_in_progress","retry_after":7}'),
        )
        with mock.patch.object(sonarpad_service.urllib.request, "urlopen", side_effect=response_error) as open_url:
            with self.assertRaises(sonarpad_service.SonarpadServiceError) as raised:
                client._request_json("POST", "/v1/generate", {})
        self.assertEqual(raised.exception.retry_after, 7)
        self.assertEqual(raised.exception.status_code, 503)
        self.assertIn("generation_in_progress", str(raised.exception))
        self.assertIs(open_url.call_args.kwargs["context"], client._ssl_context)

    def test_wrapped_http_403_is_recognized_from_supported_transport_fields(self):
        for field in ("code", "status_code", "response"):
            for chaining in ("__cause__", "__context__"):
                with self.subTest(field=field, chaining=chaining):
                    original = RuntimeError("remote rejection")
                    setattr(original, field, SimpleNamespace(status_code=403) if field == "response" else "403")
                    outer = RuntimeError("Gemini upload reference rejected")
                    setattr(outer, chaining, original)
                    self.assertTrue(audio_describer._is_permission_denied_error(outer))

    def test_local_permission_or_file_errors_do_not_trigger_http_fallback(self):
        for error in (
            PermissionError(errno.EACCES, "403 PERMISSION_DENIED", "/missing/film.mp4"),
            FileNotFoundError(errno.ENOENT, "403 PERMISSION_DENIED", "/missing/film.mp4"),
            IsADirectoryError(errno.EISDIR, "403 PERMISSION_DENIED"),
            RuntimeError("429 RESOURCE_EXHAUSTED"),
        ):
            with self.subTest(error=error):
                self.assertFalse(audio_describer._is_permission_denied_error(error))
        local = PermissionError(errno.EACCES, "local file cannot be opened")
        prior_remote = RuntimeError("403 PERMISSION_DENIED")
        local.__context__ = prior_remote
        self.assertFalse(audio_describer._is_permission_denied_error(local))
        for kind in (PermissionError, FileNotFoundError):
            for chaining in ("__cause__", "__context__"):
                with self.subTest(kind=kind, chaining=chaining):
                    local = kind(errno.EACCES, "cannot open", "403 PERMISSION_DENIED.mp4")
                    outer = RuntimeError(f"upload failed: {local}")
                    setattr(outer, chaining, local)
                    self.assertFalse(audio_describer._is_permission_denied_error(outer))
        cycle = RuntimeError("unrelated wrapper")
        cycle.__cause__ = cycle
        self.assertFalse(audio_describer._is_permission_denied_error(cycle))

    def test_non_object_json_enters_existing_repair_path_without_a_parser_crash(self):
        for raw in ("[]", "null", '"testo"', "true", "123"):
            with self.subTest(raw=raw), mock.patch.object(
                audio_describer.gemini, "get_response_finish_reason", return_value="STOP"
            ), mock.patch.object(
                audio_describer, "_request_json_repair_from_gemini",
                return_value=([("00:01", "00:03", "Apre la porta.")], [], True),
            ) as repair:
                descriptions, glossary = audio_describer._parse_with_optional_json_repair(
                    "client", "model", "same-video", raw, object()
                )
                self.assertEqual(descriptions, [("00:01", "00:03", "Apre la porta.")])
                self.assertEqual(glossary, [])
                repair.assert_called_once()
                self.assertEqual(repair.call_args.args[:4], ("client", "model", "same-video", raw))

    def test_valid_json_preserves_text_and_times_without_requesting_repair(self):
        raw = json.dumps({"audio_descriptions": [{
            "start_time_mmss": "01:02.300", "end_time_mmss": "01:04.800",
            "description_text": "La luce si spegne.", "visual_evidence_time_seconds": 62.3,
        }], "character_glossary": []})
        with mock.patch.object(
            audio_describer.gemini, "get_response_finish_reason", return_value="STOP"
        ), mock.patch.object(audio_describer, "_request_json_repair_from_gemini") as repair:
            descriptions, glossary = audio_describer._parse_with_optional_json_repair(
                "client", "model", "same-video", raw, object()
            )
        self.assertEqual(descriptions, [("01:02.300", "01:04.800", "La luce si spegne.")])
        self.assertEqual(glossary, [])
        repair.assert_not_called()

    def test_normal_pipe_output_is_unchanged_and_closed_pipe_exits_once(self):
        output = io.StringIO()
        with mock.patch.object(bridge.sys, "stdout", output):
            bridge._emit("STATUS", {"message": "è pronto", "progress": 4})
        self.assertEqual(output.getvalue(), 'STATUS:{"message":"\\u00e8 pronto","progress":4}\n')
        for error in (BrokenPipeError(errno.EPIPE, "closed"), OSError(errno.EINVAL, "closed")):
            with self.subTest(error=error), mock.patch("builtins.print", side_effect=error) as emit:
                with self.assertRaises(SystemExit) as stopped:
                    bridge._emit("RESULT", {"success": False})
                self.assertEqual(stopped.exception.code, 0)
                emit.assert_called_once()
        unrelated = OSError(errno.ENOSPC, "output storage full")
        with mock.patch("builtins.print", side_effect=unrelated):
            with self.assertRaises(OSError) as raised:
                bridge._emit("STATUS", "diagnostic")
        self.assertIs(raised.exception, unrelated)

    def test_overload_consent_is_scoped_to_each_request(self):
        error = RuntimeError("503 UNAVAILABLE. This model is currently experiencing high demand.")
        client = mock.Mock()
        client.models.generate_content.side_effect = [error] * 3 + ["ok1"] + [error] * 3 + ["ok2"]
        handler = mock.Mock(return_value=True)
        with mock.patch.object(gemini_helpers, "_lazy_import_gemini_sdk"), mock.patch.object(
            gemini_helpers.config_model, "get_setting", return_value=""
        ), mock.patch.object(gemini_helpers.time, "sleep"), mock.patch.object(
            gemini_helpers, "_OVERLOAD_DECISION_HANDLER", handler
        ):
            first = gemini_helpers.generate_content_with_retry(client, "same-model", [], object())
            second = gemini_helpers.generate_content_with_retry(client, "same-model", [], object())
        self.assertEqual((first, second), ("ok1", "ok2"))
        self.assertEqual(handler.call_count, 2)


if __name__ == "__main__":
    unittest.main()

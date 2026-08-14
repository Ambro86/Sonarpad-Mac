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

    def test_worker_build_pins_binary_cryptography_and_runs_packaged_self_test(self):
        build_script = (ROOT / "bridge" / "build_audio_description_bridge_macos.sh").read_text(
            encoding="utf-8"
        )
        self.assertIn('CRYPTOGRAPHY_VERSION="${CRYPTOGRAPHY_VERSION:-48.0.1}"', build_script)
        self.assertIn('GOOGLE_GENAI_VERSION="${GOOGLE_GENAI_VERSION:-2.12.1}"', build_script)
        self.assertIn('GOOGLE_API_CORE_VERSION="${GOOGLE_API_CORE_VERSION:-2.32.0}"', build_script)
        self.assertIn("--only-binary=cryptography", build_script)
        self.assertIn('audio_description_bridge" --self-test', build_script)

    def test_catalina_worker_can_force_official_pure_python_protobuf_wheel(self):
        build_script = (ROOT / "bridge" / "build_audio_description_bridge_macos.sh").read_text(
            encoding="utf-8"
        )
        self.assertIn('PROTOBUF_PURE_PYTHON="${PROTOBUF_PURE_PYTHON:-0}"', build_script)
        self.assertIn('PROTOBUF_VERSION="${PROTOBUF_VERSION:-7.35.1}"', build_script)
        self.assertIn("--platform any", build_script)
        self.assertIn("protobuf-*-py3-none-any.whl", build_script)

    def test_extended_pauses_are_disabled_by_default(self):
        self.assertIn("audio_description_extended_pauses: false", MAIN)
        self.assertNotIn("audio_description_extended_pauses: true", MAIN)

    def test_audio_description_progress_uses_the_ui_event_loop(self):
        start = AUDIO.index("fn run_with_progress")
        end = AUDIO.index("fn language_choices", start)
        progress = AUDIO[start:end]
        self.assertIn("Gauge::builder", progress)
        self.assertIn("Timer::new(&progress_dialog)", progress)
        self.assertIn("progress_cancel.on_click", progress)
        self.assertIn("ID_AUDIO_DESCRIPTION_PROGRESS_CANCEL", progress)
        self.assertNotIn("with_id(ID_CANCEL)", progress)
        self.assertIn("progress_dialog.on_close", progress)
        self.assertIn("progress_dialog.show_modal()", progress)
        self.assertNotIn("thread::sleep", progress)

    def test_completion_dialog_waits_for_an_accessible_ok(self):
        self.assertIn("ID_AUDIO_DESCRIPTION_START", AUDIO)
        self.assertIn("fn show_completion", AUDIO)
        self.assertRegex(AUDIO, r"TextCtrlStyle::MultiLine\s*\|\s*TextCtrlStyle::ReadOnly")
        self.assertIn("completion_ok.set_focus()", AUDIO)
        self.assertIn("completion_dialog.show_modal()", AUDIO)

    def test_create_dialog_close_and_macos_quit_are_not_standard_cancel_buttons(self):
        start = AUDIO.index("pub fn open_create_dialog")
        end = AUDIO.index("fn format_mmss", start)
        create_dialog = AUDIO[start:end]
        self.assertIn("ID_AUDIO_DESCRIPTION_CLOSE", create_dialog)
        self.assertIn("audio_description.create.close_requested_button", create_dialog)
        self.assertIn("audio_description.create.close_requested_window", create_dialog)
        self.assertIn("audio_description.create.quit_requested_menu", create_dialog)
        helper_start = AUDIO.index("fn execute_audio_description_job")
        helper_end = AUDIO.index("pub fn open_create_dialog", helper_start)
        helper = AUDIO[helper_start:helper_end]
        self.assertIn("audio_description.create.closed_after_cancel", helper)
        self.assertIn('if error == "cancelled"', helper)
        self.assertRegex(
            create_dialog,
            r'(?s)execute_audio_description_job\(.*d\.end_modal\(ID_AUDIO_DESCRIPTION_CLOSE\)',
        )
        self.assertIn("parent.close(false)", create_dialog)

    def test_success_ok_opens_generated_audio_and_keeps_create_dialog_open(self):
        start = AUDIO.index("fn execute_audio_description_job")
        end = AUDIO.index("pub fn open_create_dialog", start)
        helper = AUDIO[start:end]
        success_start = helper.index("Ok(out)")
        success_end = helper.index("Err(error) =>", success_start)
        success = helper[success_start:success_end]
        self.assertLess(success.index("show_completion"), success.index("open_local_media_with_mpv"))
        self.assertIn("audio_description.create.open_output_completed", success)
        self.assertNotIn("end_modal", success)

    def test_project_export_uses_event_loop_and_closes_modal_layers(self):
        progress_start = AUDIO.index("fn run_project_export_with_progress")
        progress_end = AUDIO.index("pub fn open_project_editor", progress_start)
        progress = AUDIO[progress_start:progress_end]
        self.assertIn("Gauge::builder", progress)
        self.assertIn("Timer::new(&progress_dialog)", progress)
        self.assertIn("cancel_button.on_click", progress)
        self.assertIn("progress_dialog.show_modal()", progress)
        self.assertNotIn("thread::sleep", progress)

        editor_start = AUDIO.index("pub fn open_project_editor")
        editor = AUDIO[editor_start:]
        self.assertIn("audio_description.project.editor_closed_after_export", editor)
        self.assertIn("ID_AUDIO_DESCRIPTION_PROJECT_CLOSE", editor)
        self.assertIn("audio_description.project.quit_requested_menu", editor)
        self.assertIn("parent.close(false)", editor)

        create_start = AUDIO.index("pub fn open_create_dialog")
        create_end = AUDIO.index("fn format_mmss", create_start)
        create_dialog = AUDIO[create_start:create_end]
        self.assertIn("open_project_requested", create_dialog)
        self.assertIn("audio_description.create.open_project_after_close", create_dialog)
        self.assertRegex(
            create_dialog,
            r"(?s)open_project_requested_button\.set\(true\).*d_modify\.end_modal",
        )

    def test_project_editor_exports_srt_and_vtt_after_mp3(self):
        editor_start = AUDIO.index("pub fn open_project_editor")
        editor = AUDIO[editor_start:]
        mp3 = editor.index('tr("audio_description.project.export")')
        srt = editor.index('tr("audio_description.project.export_srt")')
        vtt = editor.index('tr("audio_description.project.export_vtt")')
        close = editor.index('tr("audio_description.close")', vtt)
        self.assertLess(mp3, srt)
        self.assertLess(srt, vtt)
        self.assertLess(vtt, close)
        self.assertIn('export_project_subtitles(&dialog_export_srt, &snapshot, "srt")', editor)
        self.assertIn('export_project_subtitles(&dialog_export_vtt, &snapshot, "vtt")', editor)
        self.assertIn("fn render_project_srt", AUDIO)
        self.assertIn("fn render_project_vtt", AUDIO)
        self.assertIn('String::from("WEBVTT\\r\\n\\r\\n")', AUDIO)
        self.assertIn("description.output_start_sec", AUDIO)
        self.assertIn("description.output_end_sec", AUDIO)

    def test_modern_workflow_builds_bundles_and_signs_worker(self):
        self.assertIn("build_audio_description_bridge_macos.sh", WORKFLOW)
        self.assertIn("dist/audio-description-worker", WORKFLOW)
        self.assertIn("Contents/Resources/audio-description", WORKFLOW)
        self.assertIn("Mach-O", WORKFLOW)
        self.assertIn("codesign", WORKFLOW)
        self.assertIn("--enable-muxer=segment", WORKFLOW)
        self.assertIn("--enable-muxer=matroska", WORKFLOW)

    def test_packaged_mpv_declares_italian_for_voiceover(self):
        for workflow in (WORKFLOW, (ROOT / ".github/workflows/macos-app-dmg-catalina.yml").read_text()):
            self.assertIn("MPV_INFO_PLIST", workflow)
            self.assertIn("Set :CFBundleDevelopmentRegion it", workflow)
            self.assertIn('cp -R "mpv-config/localizations/."', workflow)
            for index, language in enumerate(("en", "it", "fr", "es", "pt", "cs", "pl")):
                self.assertIn(
                    f"Add :CFBundleLocalizations:{index} string {language}", workflow
                )
        for language in ("en", "it", "fr", "es", "pt", "cs", "pl"):
            self.assertTrue(
                (ROOT / f"mpv-config/localizations/{language}.lproj/InfoPlist.strings").is_file()
            )
        self.assertIn('ONNXRUNTIME_VERSION: "1.19.2"', WORKFLOW)

    def test_modern_workflow_pins_known_good_mpv_builds(self):
        self.assertIn('mpv_version: "0.40.0"', WORKFLOW)
        self.assertIn("mpv-arm64-0.40.0.tar.gz", WORKFLOW)
        self.assertIn("3170fb709defebaba33e9755297d70dc3562220541de54fc3d494a8309ef1260", WORKFLOW)
        self.assertIn('mpv_version: "0.39.0"', WORKFLOW)
        self.assertIn("mpv-0.39.0.tar.gz", WORKFLOW)
        self.assertIn("35ec81ad86a97b24956a8d0f4fa1ba2690b44ae7741c920e923620bcd7bd402a", WORKFLOW)
        self.assertIn("laboratory.stolendata.net/~djinn/mpv_osx", WORKFLOW)
        self.assertIn("shasum -a 256 --check", WORKFLOW)
        self.assertIn("Unexpected mpv version", WORKFLOW)
        self.assertNotIn("mpv-arm64-latest.tar.gz", WORKFLOW)
        self.assertNotIn("mpv-latest.tar.gz", WORKFLOW)
        self.assertNotIn("mpv-v0.41.0-macos-14-arm.zip", WORKFLOW)
        self.assertNotIn("mpv-v0.41.0-macos-15-intel.zip", WORKFLOW)
        self.assertIn('app_macos_min: "12.0"', WORKFLOW)
        self.assertIn("mpv-0.35.0.tar.gz", CATALINA)

    def test_catalina_workflow_uses_compatible_worker_and_checks_macho_targets(self):
        self.assertIn("build_audio_description_bridge_macos.sh", CATALINA)
        self.assertIn('ONNXRUNTIME_VERSION: "1.15.0"', CATALINA)
        self.assertIn('PROTOBUF_PURE_PYTHON: "1"', CATALINA)
        self.assertIn('PROTOBUF_VERSION: "7.35.1"', CATALINA)
        self.assertIn('_internal/google/_upb/_message.abi3.so', CATALINA)
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

    def test_create_dialog_has_unambiguous_browse_buttons(self):
        self.assertIn('tr("audio_description.browse_input")', AUDIO)
        self.assertIn('tr("audio_description.browse_output")', AUDIO)

    def test_create_dialog_browse_buttons_precede_path_fields_in_keyboard_order(self):
        start = AUDIO.index("pub fn open_create_dialog")
        end = AUDIO.index("fn format_mmss", start)
        create_dialog = AUDIO[start:end]
        positions = [
            create_dialog.index("let input_btn = Button::builder"),
            create_dialog.index("let input = TextCtrl::builder"),
            create_dialog.index("let output_btn = Button::builder"),
            create_dialog.index("let output = TextCtrl::builder"),
        ]
        self.assertEqual(positions, sorted(positions))
        self.assertLess(
            create_dialog.index("input_row.add(&input_btn"),
            create_dialog.index("input_row.add(&input,"),
        )
        self.assertLess(
            create_dialog.index("output_row.add(&output_btn"),
            create_dialog.index("output_row.add(&output,"),
        )

    def test_worker_stderr_is_streamed_into_the_single_sonarpad_log(self):
        bridge_source = BRIDGE
        logger_source = (
            ROOT / "bridge" / "audio_description_runtime" / "audio_describer" / "utils" / "logger.py"
        ).read_text(encoding="utf-8")
        self.assertIn('audio_description.worker {line}', bridge_source)
        self.assertIn("read_until(b'\\n'", bridge_source)
        self.assertNotIn("read_to_end(&mut raw)", bridge_source)
        self.assertNotIn("logging.FileHandler", logger_source)
        self.assertNotIn("sonarpad_audio_description_bridge.log", logger_source)
        self.assertIn("logging.StreamHandler(sys.stderr)", logger_source)

    def test_create_dialog_makes_new_character_catalog_explicit_before_start(self):
        start = AUDIO.index("pub fn open_create_dialog")
        end = AUDIO.index("fn format_mmss", start)
        create_dialog = AUDIO[start:end]
        self.assertIn('tr("audio_description.character_catalog.new_option")', create_dialog)
        self.assertIn('tr("audio_description.character_catalog.selection_label")', create_dialog)
        self.assertIn('tr("audio_description.character_catalog.new_name_label")', create_dialog)
        self.assertIn("let catalog_name = TextCtrl::builder", create_dialog)
        self.assertIn("catalog_choice.on_selection_changed", create_dialog)
        self.assertIn("suggested_catalog_name", create_dialog)
        self.assertIn("catalog_name.set_focus()", create_dialog)
        self.assertNotIn("ask_catalog_name", AUDIO)


    def test_project_editor_uses_modal_feedback_for_applied_edits(self):
        self.assertIn("show_project_edit_success(&dialog_apply)", AUDIO)
        self.assertRegex(
            AUDIO,
            r'(?s)duration > available \+ 0\.001.*show_project_error\(\s*&dialog_apply',
        )
        self.assertIn('audio_description.project.edit_success_title', AUDIO)

    def test_project_editor_voice_change_is_explicit_and_accessible(self):
        editor_start = AUDIO.index("pub fn open_project_editor")
        editor = AUDIO[editor_start:]
        self.assertIn('let apply = Button::builder(&panel)', editor)
        self.assertIn('let engine = Choice::builder(&panel).build();', editor)
        self.assertIn('engine.append(&tr("audio_description.engine.edge"))', editor)
        self.assertIn('engine.append(&tr("audio_description.engine.system"))', editor)
        self.assertIn('let voice = Choice::builder(&panel).build();', editor)
        self.assertIn('let change_voice = Button::builder(&panel)', editor)
        self.assertIn('audio_description.project.change_voice', editor)
        self.assertIn('change_voice.on_click', editor)
        self.assertNotIn('voice.on_selection_changed', editor)
        self.assertLess(editor.index('let text = TextCtrl::builder'), editor.index('let apply = Button::builder'))
        self.assertLess(editor.index('let apply = Button::builder'), editor.index('let engine = Choice::builder'))
        self.assertLess(editor.index('let engine = Choice::builder'), editor.index('let voice = Choice::builder'))
        self.assertLess(editor.index('let voice = Choice::builder'), editor.index('let change_voice = Button::builder'))

    def test_project_voice_change_checks_every_description_and_rebuilds_mp3(self):
        self.assertIn('fn change_project_voice(', AUDIO)
        voice_change_start = AUDIO.index('fn change_project_voice(')
        voice_change_end = AUDIO.index('fn run_project_voice_validation_with_progress', voice_change_start)
        voice_change = AUDIO[voice_change_start:voice_change_end]
        self.assertIn('for (index, description) in project.descriptions.iter().enumerate()', voice_change)
        self.assertIn('engine: tts_engine', voice_change)
        self.assertIn('voice: tts_voice', voice_change)
        self.assertIn('schedule_descriptions(', voice_change)
        self.assertIn('project.allow_extended_pauses', voice_change)
        self.assertIn('render_mix(&source, &mix, &scheduled, &cancel)', voice_change)
        self.assertIn('encode_mp3(&mix, &temporary_mp3, &cancel)', voice_change)
        self.assertIn('updated.tts_engine = tts_engine.to_string()', voice_change)
        self.assertIn('updated.tts_voice = tts_voice.to_string()', voice_change)
        self.assertIn('commit_project_pair(', voice_change)
        self.assertIn('ProjectVoiceFitError', AUDIO)
        self.assertIn('audio_description.project.voice_too_long', AUDIO)
        voice_progress_start = AUDIO.index('fn run_project_voice_validation_with_progress')
        voice_progress_end = AUDIO.index('fn project_file_dialog', voice_progress_start)
        voice_progress = AUDIO[voice_progress_start:voice_progress_end]
        self.assertIn('Timer::new(&progress_dialog)', voice_progress)
        self.assertIn('progress_dialog.show_modal()', voice_progress)
        self.assertNotIn('thread::sleep', voice_progress)

    def test_dropped_description_keeps_original_index_for_voice_fit_errors(self):
        dropped_start = AUDIO.index('struct DroppedDescription')
        dropped_end = AUDIO.index('struct ProjectInterval', dropped_start)
        dropped_struct = AUDIO[dropped_start:dropped_end]
        self.assertIn('original_index: usize', dropped_struct)
        schedule_start = AUDIO.index('fn schedule_descriptions(')
        schedule_end = AUDIO.index('fn mix_sample', schedule_start)
        schedule = AUDIO[schedule_start:schedule_end]
        self.assertRegex(
            schedule,
            r'(?s)dropped\.push\(DroppedDescription \{.*original_index: d\.original_index',
        )

    def test_temporal_grounding_prompt_matches_windows_release_behavior(self):
        core = (
            ROOT
            / "bridge"
            / "audio_description_runtime"
            / "audio_describer"
            / "core"
            / "audio_describer.py"
        ).read_text(encoding="utf-8")
        worker = (ROOT / "bridge" / "audio_description_bridge.py").read_text(encoding="utf-8")
        self.assertIn("GEMINI_FRAME_RATE_FOR_AI = 0", worker)
        self.assertIn("CHUNK_DURATION_SECONDS = 180", worker)
        self.assertIn("Temporal grounding", core)
        self.assertIn("A static or mundane description that is correct", core)
        self.assertIn("including a logo or title card", core)
        self.assertIn("outside the slot", core)

    def test_interrupted_audio_description_checkpoint_and_resume_protocol(self):
        worker = (ROOT / "bridge" / "audio_description_bridge.py").read_text(encoding="utf-8")
        core = (
            ROOT
            / "bridge"
            / "audio_description_runtime"
            / "audio_describer"
            / "core"
            / "audio_describer.py"
        ).read_text(encoding="utf-8")
        self.assertIn('"CHECKPOINT"', worker)
        self.assertIn("resume_completed_chunks=", worker)
        self.assertIn("if i < resume_completed_chunks:", core)
        extend = core.index("all_descriptions.extend(normalized_chunk)")
        callback = core.index("checkpoint_callback(", extend)
        cleanup = core.index("_cleanup_uploaded_file(client, current_chunk_file_obj", callback)
        self.assertLess(extend, callback)
        self.assertLess(callback, cleanup)
        self.assertIn('line.strip_prefix("CHECKPOINT:")', BRIDGE)
        self.assertIn("AudioDescriptionBridgeResume", BRIDGE)

    def test_continue_interrupted_button_follows_modify_project_in_tab_order(self):
        start = AUDIO.index("pub fn open_create_dialog")
        end = AUDIO.index("fn format_mmss", start)
        dialog = AUDIO[start:end]
        modify = dialog.index('with_label(&tr("audio_description.modify_project"))')
        resume = dialog.index('with_label(&tr("audio_description.resume.title"))', modify)
        create = dialog.index('with_label(&tr("audio_description.start"))', resume)
        self.assertLess(modify, resume)
        self.assertLess(resume, create)
        self.assertRegex(
            dialog,
            r'(?s)actions\.add\(&modify.*actions\.add\(&continue_interrupted.*actions\.add\(&start',
        )

    def test_resume_uses_selected_model_and_preserves_checkpoint_on_cancel(self):
        self.assertIn('set_extension("sonarpad-ad.partial.json")', AUDIO)
        self.assertIn("save_partial_checkpoint", AUDIO)
        self.assertIn("job_from_checkpoint", AUDIO)
        self.assertIn("model_value.clone()", AUDIO)
        self.assertIn("resume_checkpoint_path: Some", AUDIO)
        self.assertIn("if result.is_ok() && checkpoint_path.exists()", AUDIO)
        self.assertIn('return Err("cancelled".to_string());', BRIDGE)
        self.assertNotIn(
            "checkpoint_remove_after_cancel",
            AUDIO,
        )

    def test_resume_labels_exist_in_all_mac_locales(self):
        for path in sorted((ROOT / "i18n").glob("audio_description_*.json")):
            payload = json.loads(path.read_text(encoding="utf-8"))
            with self.subTest(locale=path.name):
                for key in (
                    "audio_description.resume.title",
                    "audio_description.resume.model",
                    "audio_description.resume.start",
                    "audio_description.resume.open_title",
                    "audio_description.resume.invalid",
                ):
                    self.assertIn(key, payload)

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

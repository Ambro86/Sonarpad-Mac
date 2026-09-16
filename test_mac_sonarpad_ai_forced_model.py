from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parent
AUDIO = (ROOT / 'src' / 'audio_description.rs').read_text(encoding='utf-8')
BRIDGE = (ROOT / 'bridge' / 'audio_description_bridge.py').read_text(encoding='utf-8')


class MacSonarpadAiForcedModelTests(unittest.TestCase):
    def test_macos_matches_windows_forced_service_model(self):
        self.assertIn(
            'const SONARPAD_AI_FORCED_GEMINI_MODEL: &str = "gemini-3.8-flash";',
            AUDIO,
        )
        self.assertGreaterEqual(
            AUDIO.count('SONARPAD_AI_FORCED_GEMINI_MODEL.to_string()'),
            3,
        )

    def test_service_mode_displays_forced_model_and_restores_personal_model(self):
        self.assertIn('let personal_gemini_model = Rc::new(RefCell::new(', AUDIO)
        self.assertIn('model.append(SONARPAD_AI_FORCED_GEMINI_MODEL);', AUDIO)
        self.assertIn('*personal_gemini_model_ai_access.borrow_mut() = current_model.clone();', AUDIO)
        self.assertIn('model.append(restored);', AUDIO)
        self.assertIn('model.enable(!service);', AUDIO)

    def test_service_mode_does_not_overwrite_personal_model_preference(self):
        start = AUDIO.index('fn execute_audio_description_job(')
        end = AUDIO.index('match run_with_progress', start)
        block = AUDIO[start:end]
        self.assertIn('if use_sonarpad_ai {', block)
        self.assertIn('st.audio_description_gemini_model = selected_model;', block)
        personal_branch = block.index('} else {')
        model_write = block.index('st.audio_description_gemini_model = selected_model;')
        self.assertGreater(model_write, personal_branch)

    def test_new_resume_and_reanalysis_jobs_force_38(self):
        self.assertIn('let resume_model = if use_sonarpad_ai {', AUDIO)
        self.assertIn('let effective_model = if use_sonarpad_ai {', AUDIO)
        project_job = AUDIO[AUDIO.index('fn audio_description_job_from_project('):]
        self.assertIn('gemini_model: if use_sonarpad_ai {', project_job)
        self.assertIn('SONARPAD_AI_FORCED_GEMINI_MODEL.to_string()', project_job)

    def test_python_bridge_forces_same_model_before_runtime_configuration(self):
        self.assertIn('SONARPAD_AI_FORCED_GEMINI_MODEL = "gemini-3.8-flash"', BRIDGE)
        configure = BRIDGE[BRIDGE.index('def _configure_omni('):BRIDGE.index('_CHUNK_RE', BRIDGE.index('def _configure_omni('))]
        self.assertIn('if use_sonarpad_ai', configure)
        self.assertIn('SONARPAD_AI_FORCED_GEMINI_MODEL', configure)
        fixed = BRIDGE[BRIDGE.index('def _run_fixed_reanalysis('):BRIDGE.index('def run(', BRIDGE.index('def _run_fixed_reanalysis('))]
        self.assertIn('SONARPAD_AI_FORCED_GEMINI_MODEL', fixed)


if __name__ == '__main__':
    unittest.main()

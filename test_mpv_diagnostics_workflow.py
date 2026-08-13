from pathlib import Path
import ast

ROOT = Path(__file__).resolve().parent
WORKFLOW = ROOT / ".github" / "workflows" / "mpv-diagnostics-macos.yml"
SCRIPT = ROOT / "scripts" / "mpv_diagnostics_macos.py"


def test_diagnostic_script_is_valid_python():
    ast.parse(SCRIPT.read_text(encoding="utf-8"))


def test_workflow_has_all_three_macos_scenarios_and_single_final_report():
    text = WORKFLOW.read_text(encoding="utf-8")
    assert "apple-silicon-macos14" in text
    assert "intel-macos15" in text
    assert "intel-catalina-compatibility" in text
    assert "mpv-v0.41.0-macos-14-arm.zip" in text
    assert "mpv-v0.41.0-macos-15-intel.zip" in text
    assert "mpv-diagnostics-combined.txt" in text
    assert "name: mpv-diagnostics-combined" in text


def test_diagnostics_compare_candidate_to_known_good_and_simulate_sonarpad_packaging():
    text = SCRIPT.read_text(encoding="utf-8")
    assert "known-good baseline" in text
    assert "SONARPAD PACKAGING SIMULATION" in text
    assert 'codesign", "--force", "--deep", "--sign", "-"' in text
    assert "launchservices_bundle" in text
    assert "cocoa_force_window" in text
    assert "local_wav" in text
    assert "la7_live" in text
    assert "la7_vod" in text


def test_catalina_is_not_falsely_claimed_as_runtime_tested():
    workflow = WORKFLOW.read_text(encoding="utf-8")
    script = SCRIPT.read_text(encoding="utf-8")
    assert 'scenario: intel-catalina-compatibility' in workflow
    assert 'compatibility_target: "10.15"' in workflow
    assert 'runtime: "no"' in workflow
    assert "does not provide a macOS 10.15 runner" in script

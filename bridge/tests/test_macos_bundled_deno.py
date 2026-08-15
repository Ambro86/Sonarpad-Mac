import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
WORKFLOWS = (
    ROOT / ".github" / "workflows" / "macos-app-dmg.yml",
    ROOT / ".github" / "workflows" / "macos-app-dmg-catalina.yml",
)


class MacBundledDenoTests(unittest.TestCase):
    def test_every_macos_package_downloads_and_bundles_deno(self):
        for workflow in WORKFLOWS:
            source = workflow.read_text(encoding="utf-8")
            with self.subTest(workflow=workflow.name):
                self.assertIn('DENO_VERSION: "2.8.1"', source)
                self.assertIn("github.com/denoland/deno/releases/download/v${DENO_VERSION}", source)
                self.assertIn('${RESOURCES_DIR}/deno', source)
                self.assertIn('${DENO_BIN}', source)

    def test_every_dmg_validation_requires_executable_deno(self):
        for workflow in WORKFLOWS:
            source = workflow.read_text(encoding="utf-8")
            with self.subTest(workflow=workflow.name):
                self.assertIn(
                    'test -x "${MOUNT_DIR}/Sonarpad.app/Contents/Resources/deno"',
                    source,
                )

    def test_timestamped_codesign_operations_are_retried(self):
        for workflow in WORKFLOWS:
            source = workflow.read_text(encoding="utf-8")
            with self.subTest(workflow=workflow.name):
                self.assertIn("codesign_with_retry()", source)
                self.assertIn("for attempt in 1 2 3 4 5", source)
                self.assertIn("/usr/bin/codesign \"$@\"", source)
                self.assertNotIn("codesign --force --options runtime --timestamp", source)


if __name__ == "__main__":
    unittest.main()

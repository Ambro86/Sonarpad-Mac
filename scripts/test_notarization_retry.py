"""Exercise the actual workflow retry blocks without Apple credentials or waits."""
import os
from pathlib import Path
import shutil
import subprocess
import textwrap
import unittest

ROOT = Path(__file__).resolve().parents[1]
BASH = (r'C:\Program Files\Git\bin\bash.exe' if os.name == 'nt' else shutil.which('bash'))


class NotarizationRetryTests(unittest.TestCase):
    def run_block(self, workflow, staple_failures=0, validate_failures=0, error=68):
        source = (ROOT / '.github/workflows' / workflow).read_text(encoding='utf-8')
        section = source.split('      - name: Staple notarization ticket\n', 1)[1]
        block = section.split('        run: |\n', 1)[1].split('\n      - name:', 1)[0]
        script = f'''
        staple_calls=0
        validate_calls=0
        xcrun() {{
          [ "$1" = stapler ] || return 99
          [ "$3" = dist/stage/Sonarpad.app ] || return 99
          if [ "$2" = staple ]; then
            staple_calls=$((staple_calls + 1))
            echo "CALL staple $staple_calls"
            if [ "$staple_calls" -le {staple_failures} ]; then return {error}; fi
          else
            validate_calls=$((validate_calls + 1))
            echo "CALL validate $validate_calls"
            if [ "$validate_calls" -le {validate_failures} ]; then return {error}; fi
          fi
          return 0
        }}
        sleep() {{ echo "WAIT $1"; }}
        '''
        return subprocess.run([BASH, '--noprofile', '--norc'],
                              input=textwrap.dedent(script) + textwrap.dedent(block),
                              text=True, capture_output=True, timeout=10)

    def test_workflows(self):
        for workflow in ['macos-app-dmg.yml', 'macos-app-dmg-catalina.yml']:
            for staple, validate in [(0, 0), (2, 0), (0, 2), (25, 25)]:
                with self.subTest(workflow=workflow, staple=staple, validate=validate):
                    result = self.run_block(workflow, staple, validate)
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertIn(f'CALL staple {staple + 1}\n', result.stdout)
                    self.assertIn(f'CALL validate {validate + 1}\n', result.stdout)
                    waits = [int(line.split()[1]) for line in result.stdout.splitlines() if line.startswith('WAIT ')]
                    self.assertEqual(waits, [min(i * 15, 300) for n in [staple, validate] for i in range(1, n + 1)])
            for staple, validate in [(1, 0), (0, 1)]:
                with self.subTest(workflow=workflow, permanent_error=(staple, validate)):
                    result = self.run_block(workflow, staple, validate, error=65)
                    self.assertEqual(result.returncode, 65, result.stderr)
                    self.assertNotIn('WAIT ', result.stdout)
                    if staple:
                        self.assertNotIn('CALL validate', result.stdout)


if __name__ == '__main__':
    unittest.main()

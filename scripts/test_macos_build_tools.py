"""Run workflow tool setup with simulated installed tools and Homebrew failures."""
import os
from pathlib import Path
import shutil
import subprocess
import textwrap
import unittest

ROOT = Path(__file__).resolve().parents[1]
BASH = r'C:\Program Files\Git\bin\bash.exe' if os.name == 'nt' else shutil.which('bash')

class BuildToolTests(unittest.TestCase):
    def run_setup(self, workflow, missing='', failures=0):
        source = (ROOT / '.github/workflows' / workflow).read_text(encoding='utf-8')
        section = source.split('      - name: Ensure build tools', 1)[1]
        block = section.split('        run: |\n', 1)[1].split('\n      - name:', 1)[0]
        stub = f'''
        missing=" {missing} "
        calls=0
        command() {{
          if [ "$1" = -v ]; then
            case "$missing" in *" $2 "*) return 1;; esac
          fi
          builtin command "$@"
        }}
        cmake() {{ echo CHECK-cmake; }}
        ninja() {{ echo CHECK-ninja; }}
        nasm() {{ echo CHECK-nasm; }}
        pkg-config() {{ echo CHECK-pkg-config; }}
        sleep() {{ :; }}
        brew() {{
          [ "$HOMEBREW_NO_AUTO_UPDATE" = 1 ] || return 90
          [ "$HOMEBREW_NO_INSTALL_UPGRADE" = 1 ] || return 91
          [ "$HOMEBREW_NO_INSTALL_CLEANUP" = 1 ] || return 92
          echo "BREW $*"
          calls=$((calls + 1))
          if [ "$calls" -le {failures} ]; then return 1; fi
          missing=" "
        }}
        '''
        return subprocess.run([BASH, '--noprofile', '--norc'], input=textwrap.dedent(stub)+textwrap.dedent(block), text=True, capture_output=True, timeout=10)

    def test_pipe_protocol_check_accepts_indentation_but_requires_pipe(self):
        source = (ROOT / '.github/workflows/macos-app-dmg.yml').read_text(encoding='utf-8')
        line = next(line.strip() for line in source.splitlines()
                    if '-protocols |' in line and 'pipe' in line)
        script = ('set -euo pipefail\n'
                  'ffmpeg_bin=fake_ffmpeg\n'
                  'fake_ffmpeg() { printf "%s\\n" "$PROTOCOL_LIST"; return "$FFMPEG_EXIT"; }\n'
                  + line + '\n')
        for listing, success in [
            ('Supported file protocols:\nInput:\n  file\n  pipe\nOutput:\n  pipe', True),
            ('pipe', True),
            ('\tpipe\t', True),
            ('  file\n  https', False),
            ('  pipeline', False),
            ('', False),
        ]:
            with self.subTest(listing=listing):
                env = {**os.environ, 'PROTOCOL_LIST': listing, 'FFMPEG_EXIT': '0'}
                result = subprocess.run([BASH, '--noprofile', '--norc'],
                    input=script, text=True, capture_output=True, env=env, timeout=10)
                self.assertEqual(result.returncode == 0, success, result.stderr)
        # A failing FFmpeg must not pass even if it printed the protocol.
        env = {**os.environ, 'PROTOCOL_LIST': '  pipe', 'FFMPEG_EXIT': '1'}
        result = subprocess.run([BASH, '--noprofile', '--norc'], input=script,
            text=True, capture_output=True, env=env, timeout=10)
        self.assertNotEqual(result.returncode, 0)

    def test_both_workflows(self):
        for name in ['macos-app-dmg.yml', 'macos-app-dmg-catalina.yml']:
            for missing in ['', 'nasm', 'cmake ninja nasm pkg-config']:
                with self.subTest(workflow=name, missing=missing):
                    result = self.run_setup(name, missing)
                    self.assertEqual(result.returncode, 0, result.stderr)
                    calls = [line for line in result.stdout.splitlines() if line.startswith('BREW ')]
                    self.assertEqual(calls, ['BREW install '+missing] if missing else [])
                    self.assertIn('CHECK-pkg-config', result.stdout)
            result = self.run_setup(name, 'nasm', failures=2)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(result.stdout.count('BREW install nasm'), 3)
            result = self.run_setup(name, 'nasm', failures=3)
            self.assertNotEqual(result.returncode, 0)
            self.assertNotIn('CHECK-cmake', result.stdout)

if __name__ == '__main__':
    unittest.main()

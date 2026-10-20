"""Check production menu accelerators without requiring a macOS GUI."""
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
MAIN = (ROOT / "src/main.rs").read_text(encoding="utf-8")
SHORTCUTS = (ROOT / "src/menu_shortcuts.rs").read_text(encoding="utf-8")
BINDINGS = set(re.findall(r"crate::((?:documents::)?ID_\w+),", SHORTCUTS))


class MenuShortcutTests(unittest.TestCase):
    def test_production_registry(self):
        constants = dict(re.findall(r"const (ID_\w+): i32 = (\d+);", MAIN))
        # wxWidgets' native macOS menu identifiers.
        constants.update(ID_EXIT="5006", ID_ABOUT="5014", ID_SETTINGS="5022")
        documents = dict(re.findall(
            r"const (ID_\w+): i32 = (\d+);",
            (ROOT / "src/documents.rs").read_text(encoding="utf-8"),
        ))
        declarations = [
            f"const {name}: i32 = {constants[name]};"
            for name in sorted(BINDINGS) if "::" not in name
        ]
        declarations.append("mod documents {")
        declarations.extend(
            f"pub const {name}: i32 = {documents[name]};"
            for name in sorted(documents)
        )
        declarations.append("}")
        module_path = (ROOT / "src/menu_shortcuts.rs").as_posix()
        declarations.append(f'#[path = "{module_path}"] mod menu_shortcuts;')
        with tempfile.TemporaryDirectory(prefix="sonarpad-shortcuts-") as folder:
            source = Path(folder) / "shortcuts.rs"
            source.write_bytes("\n".join(declarations).encode("utf-8"))
            binary = Path(folder) / ("shortcuts.exe" if sys.platform == "win32" else "shortcuts")
            subprocess.run(
                ["rustc", "--edition", "2024", "--test", str(source), "-o", str(binary)],
                check=True,
            )
            subprocess.run([str(binary)], check=True)

    def test_every_fixed_menu_command_uses_registry(self):
        calls = set(re.findall(
            r"\.append_command\(\s*((?:documents::)?ID_\w+)\s*,", MAIN
        ))
        self.assertEqual(calls, BINDINGS)
        plain = set(re.findall(r"\.append\(\s*((?:documents::)?ID_\w+)\s*,", MAIN))
        self.assertEqual(plain, {"ID_RADIO_FAVORITE_OPEN_BASE"})
        self.assertIn("self.append(id, &menu_shortcuts::label(id, title), help, kind)", MAIN)


if __name__ == "__main__":
    unittest.main()

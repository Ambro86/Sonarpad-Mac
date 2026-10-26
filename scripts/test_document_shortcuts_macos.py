"""Exercise native Cocoa key events through wxDragon's real editor/menu.

The keyboard handler and accelerators are taken from production source. This
does not simulate VoiceOver or a physical keyboard/input source.
"""
from pathlib import Path
import re
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
WORK = ROOT / "target/document-shortcuts-native"
(WORK / "src").mkdir(parents=True, exist_ok=True)
main = (ROOT / "src/main.rs").read_text(encoding="utf-8")
start = main.index("            let key_code =", main.index("fn handle_shortcut_event("))
end = main.index("            let is_standard_edit_shortcut", start)
handler = main[start:end]
registry = (ROOT / "src/menu_shortcuts.rs").read_text(encoding="utf-8")
source = (ROOT / "scripts/document_shortcuts_native.rs").read_text(encoding="utf-8")
source = source.replace("// PRODUCTION_HANDLER", handler)
source = source.replace("DOCUMENT_DECK_PATH", (ROOT / "src/document_deck.rs").as_posix())
for name in ("NEXT", "PREVIOUS"):
    accelerator = re.search(r'ID_' + name + r'_DOCUMENT, "([^"]+)"', registry).group(1)
    source = source.replace(name + "_ACCELERATOR", accelerator)
(WORK / "src/main.rs").write_text(source, encoding="utf-8")
(WORK / "Cargo.toml").write_text('''[package]
name = "document-shortcuts-native"
version = "0.0.0"
edition = "2024"
[workspace]
[dependencies]
wxdragon = "=0.9.18"
objc2 = "0.6.3"
objc2-foundation = {version = "0.3.2", features = ["NSString", "NSGeometry"]}
objc2-app-kit = {version = "0.3.2", features = ["NSApplication", "NSWindow", "NSEvent", "NSGraphicsContext"]}
''', encoding="utf-8")
shutil.copyfile(ROOT / "Cargo.lock", WORK / "Cargo.lock")
if "--prepare" not in sys.argv:
    subprocess.run(["cargo", "run", "--manifest-path", str(WORK / "Cargo.toml")], check=True, timeout=1200)

"""Exercise native Cocoa key events through wxDragon's real editor/menu.

The keyboard handler and accelerators are taken from production source. This
does not simulate VoiceOver or a physical keyboard/input source.
"""
from pathlib import Path
import re
import shutil
import subprocess
import sys
import os
import ctypes

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
native_start = main.index("fn mac_native_document_shortcut_direction()")
native_end = main.index("fn install_mac_document_shortcut_monitor(", native_start)
source = source.replace("// PRODUCTION_NATIVE_FALLBACK", main[native_start:native_end])
monitor_start = native_end
monitor_end = main.index("fn handle_shortcut_event(", monitor_start)
source = source.replace("// PRODUCTION_NATIVE_MONITOR", main[monitor_start:monitor_end])
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
block2 = "0.6.2"
objc2 = "0.6.3"
objc2-foundation = {version = "0.3.2", features = ["NSString", "NSGeometry"]}
objc2-app-kit = {version = "0.3.2", features = ["block2", "NSApplication", "NSWindow", "NSEvent", "NSGraphicsContext"]}
''', encoding="utf-8")
shutil.copyfile(ROOT / "Cargo.lock", WORK / "Cargo.lock")
if "--prepare" not in sys.argv:
    # wxWidgets translates hardware key codes using the active input source,
    # rather than trusting NSEvent.characters. Select the actual layout first.
    cf = ctypes.CDLL("/System/Library/Frameworks/CoreFoundation.framework/CoreFoundation")
    carbon = ctypes.CDLL("/System/Library/Frameworks/Carbon.framework/Carbon")
    ptr = ctypes.c_void_p
    carbon.TISCreateInputSourceList.argtypes = [ptr, ctypes.c_bool]
    carbon.TISCreateInputSourceList.restype = ptr
    carbon.TISGetInputSourceProperty.argtypes = [ptr, ptr]
    carbon.TISGetInputSourceProperty.restype = ptr
    carbon.TISCopyCurrentKeyboardInputSource.restype = ptr
    carbon.TISCopyCurrentKeyboardLayoutInputSource.restype = ptr
    carbon.LMGetKbdType.restype = ctypes.c_uint8
    carbon.UCKeyTranslate.argtypes = [ptr, ctypes.c_uint16, ctypes.c_uint16, ctypes.c_uint32,
        ctypes.c_uint32, ctypes.c_uint32, ctypes.POINTER(ctypes.c_uint32), ctypes.c_uint32,
        ctypes.POINTER(ctypes.c_uint32), ctypes.POINTER(ctypes.c_uint16)]
    carbon.UCKeyTranslate.restype = ctypes.c_int32
    carbon.TISSelectInputSource.argtypes = [ptr]
    carbon.TISSelectInputSource.restype = ctypes.c_int32
    carbon.TISEnableInputSource.argtypes = [ptr]
    cf.CFArrayGetCount.argtypes = [ptr]
    cf.CFArrayGetCount.restype = ctypes.c_long
    cf.CFArrayGetValueAtIndex.argtypes = [ptr, ctypes.c_long]
    cf.CFArrayGetValueAtIndex.restype = ptr
    cf.CFStringGetCString.argtypes = [ptr, ptr, ctypes.c_long, ctypes.c_uint32]
    cf.CFRelease.argtypes = [ptr]
    cf.CFDataGetBytePtr.argtypes = [ptr]
    cf.CFDataGetBytePtr.restype = ptr
    identifier = ptr.in_dll(carbon, "kTISPropertyInputSourceID")
    sources = carbon.TISCreateInputSourceList(None, True)
    original = carbon.TISCopyCurrentKeyboardInputSource()
    available = {}
    def shortcut_keys():
        layout_source = carbon.TISCopyCurrentKeyboardLayoutInputSource()
        try:
            data = carbon.TISGetInputSourceProperty(layout_source, ptr.in_dll(carbon, "kTISPropertyUnicodeKeyLayoutData"))
            if not data:
                raise RuntimeError("Input source has no Unicode keyboard layout")
            keyboard = cf.CFDataGetBytePtr(data)
            found = {}
            for shift in (0, 1):
                for key in range(128):
                    dead = ctypes.c_uint32(0)
                    length = ctypes.c_uint32(0)
                    chars = (ctypes.c_uint16 * 4)()
                    result = carbon.UCKeyTranslate(keyboard, key, 0, shift * 2, carbon.LMGetKbdType(),
                                                  1, ctypes.byref(dead), 4, ctypes.byref(length), chars)
                    if result == 0 and length.value == 1 and chars[0] in (60, 62):
                        found.setdefault(chr(chars[0]), (key, shift))
            if len(found) != 2:
                raise RuntimeError(f"Cannot locate < and > on active keyboard: {found}")
            print(f"Actual hardware mapping: {found}", flush=True)
            return {"SHORTCUT_TEST_" + name: str(found[char][part])
                    for char, prefix in (("<", "NEXT"), (">", "PREVIOUS"))
                    for part, suffix in ((0, "KEY"), (1, "SHIFT"))
                    for name in (prefix + "_" + suffix,)}
        finally:
            cf.CFRelease(layout_source)
    for index in range(cf.CFArrayGetCount(sources)):
        source = cf.CFArrayGetValueAtIndex(sources, index)
        value = carbon.TISGetInputSourceProperty(source, identifier)
        buffer = ctypes.create_string_buffer(512)
        if value and cf.CFStringGetCString(value, buffer, len(buffer), 0x08000100):
            available[buffer.value.decode()] = source
    try:
        for layout in ("US", "Italian", "Italian-Pro"):
            source = available.get("com.apple.keylayout." + layout)
            if not source:
                raise RuntimeError(f"Keyboard layout unavailable: {layout}")
            carbon.TISEnableInputSource(source)
            result = carbon.TISSelectInputSource(source)
            if result:
                raise RuntimeError(f"Could not select {layout}: {result}")
            print(f"Native input source: {layout}", flush=True)
            subprocess.run(["cargo", "run", "--manifest-path", str(WORK / "Cargo.toml")],
                           env={**os.environ, "SHORTCUT_TEST_LAYOUT": layout, **shortcut_keys()}, check=True, timeout=1200)
    finally:
        carbon.TISSelectInputSource(original)
        cf.CFRelease(original)
        cf.CFRelease(sources)

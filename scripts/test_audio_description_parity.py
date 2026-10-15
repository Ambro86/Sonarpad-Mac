"""Execute production resume and mixing code without the macOS GUI."""
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def item(source, marker, derive=False):
    start = source.index(marker)
    if derive:
        start = source.rfind("#[derive", 0, start)
    end = source.index("\n}", start) + 2
    return source[start:end]


def main():
    audio = (ROOT / "src/audio_description.rs").read_text(encoding="utf-8")
    bridge = (ROOT / "src/audio_description_bridge.rs").read_text(encoding="utf-8")
    pieces = ["#![allow(dead_code)]", "use serde::{Serialize, Deserialize};",
              "use std::{path::{Path, PathBuf}, sync::{Arc, atomic::{AtomicBool, Ordering}}};",
              "use hound::{WavReader, WavWriter, WavSpec, SampleFormat};",
              "fn default_true() -> bool { true }"]
    for name in ("AudioDescriptionPreparedChunk", "BridgeCharacter", "BridgeDescription",
                 "AudioDescriptionBridgeResume"):
        pieces.append(item(bridge, "pub struct " + name, True))
    for name in ("AudioDescriptionPartialCatalog", "AudioDescriptionChunkLayout",
                 "AudioDescriptionAnalysisProgress", "AudioDescriptionPartialCheckpoint",
                 "ScheduledDescription"):
        pieces.append(item(audio, "struct " + name, True))
    pieces.append(item(audio, "enum AudioDescriptionChunkMode", True))
    for name in ("AudioDescriptionChunkLayout", "AudioDescriptionAnalysisProgress",
                 "AudioDescriptionPartialCheckpoint"):
        pieces.append(item(audio, "impl " + name))
    for name in ("default_source_volume", "normalize_source_volume", "mix_sample",
                 "smooth_duck_fade", "merge_duck_intervals", "duck_gain_for", "render_mix"):
        pieces.append(item(audio, "fn " + name + "("))
    for name in ("MIX_SAMPLE_RATE", "MIX_CHANNELS", "DUCKING_DB", "FADE_MS", "PRE_DUCK_MS", "RELEASE_MS"):
        pieces.append(re.search(r"const " + name + r":.*?;", audio).group())
    pieces.append((ROOT / "scripts/audio_description_parity_tests.rs").read_text(encoding="utf-8"))
    harness = ROOT / "target/audio-description-parity"
    harness.mkdir(parents=True, exist_ok=True)
    (harness / "lib.rs").write_text("\n\n".join(pieces), encoding="utf-8")
    (harness / "Cargo.toml").write_text('''[package]
name = "sonarpad-audio-description-parity-tests"
version = "0.0.0"
edition = "2024"
[lib]
path = "lib.rs"
[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
hound = "3.5"
tempfile = "3"
''', encoding="utf-8")
    subprocess.run(["cargo", "test", "--manifest-path", str(harness / "Cargo.toml"),
                    "--target-dir", str(ROOT / "target"), "--lib"], check=True, cwd=ROOT)


if __name__ == "__main__":
    main()

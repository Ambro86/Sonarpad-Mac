"""Test the production podcast module without macOS GUI dependencies.

Usage: python scripts/test_podcast_search.py [--live]
"""
from pathlib import Path
import json
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]


def run_tests(live=False):
    harness = ROOT / "target/podcast-search-harness"
    harness.mkdir(parents=True, exist_ok=True)
    module = json.dumps(str(ROOT / "src/podcasts.rs").replace("\\", "/"))
    (harness / "lib.rs").write_text('''#![allow(dead_code)]
struct Settings { ui_language: String }
impl Settings {
    fn load() -> Self { Self { ui_language: "it".into() } }
}
fn append_podcast_log(message: &str) { eprintln!("{message}"); }
''' + f"#[path = {module}]\nmod podcasts;\n", encoding="utf-8")
    (harness / "Cargo.toml").write_text('''[package]
name = "sonarpad-podcast-search-tests"
version = "0.0.0"
edition = "2024"
[lib]
path = "lib.rs"
[dependencies]
tokio = { version = "1.49", features = ["macros", "rt-multi-thread", "time"] }
reqwest = { version = "0.12", features = ["blocking", "json", "rustls-tls"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
url = "2.5"
feed-rs = "2.3"
''', encoding="utf-8")
    command = ["cargo", "test", "--manifest-path", str(harness / "Cargo.toml"),
               "--target-dir", str(ROOT / "target"), "--lib"]
    if live:
        command += ["--", "--ignored", "--nocapture"]
    subprocess.run(command, cwd=ROOT, check=True, timeout=600)


if __name__ == "__main__":
    run_tests(live="--live" in sys.argv)

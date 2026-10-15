"""Run the production radio catalog tests without the macOS GUI dependencies.

Usage: python scripts/test_radio_catalog.py [--live]
"""
from pathlib import Path
import json
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]


def run_tests(live=False):
    source = (ROOT / "src/main.rs").read_text(encoding="utf-8")
    harness = ROOT / "target/radio-catalog-harness"
    harness.mkdir(parents=True, exist_ok=True)
    functions = []
    for name in ("normalized_radio_name", "canonical_radio_name", "radio_browser_language_name"):
        start = source.index("fn " + name + "(")
        # Some functions are followed by a derived type; only take the function.
        end = source.index("\n}", start) + 2
        functions.append(source[start:end])
    favorite = re.search(r"struct RadioFavorite \{[^}]+\}", source).group(0)
    page_size = re.search(r"const RADIO_RESULTS_PAGE_SIZE: usize = \d+;", source).group(0)
    module_path = json.dumps(str(ROOT / "src/radio_catalog.rs").replace("\\", "/"))
    library = "#[derive(Clone)]\n" + favorite + "\n" + page_size + "\n"
    library += "\n".join(functions)
    library += '\nfn append_podcast_log(message: &str) { eprintln!("{message}"); }\n'
    library += f"#[path = {module_path}]\nmod radio_catalog;\n"
    library += r'''
#[test]
#[ignore = "requires the live radio service"]
fn live_radio_catalog() {
    let query = radio_catalog::Query { text: "radio".into(), ..Default::default() };
    let mut session = radio_catalog::Session::new(query, Vec::new());
    session.load_page(0, &std::sync::atomic::AtomicBool::new(false));
    assert!(!session.results.is_empty(), "live catalog must return stations");
    assert!(session.results.iter().all(|s| s.stream_url.starts_with("http")));
    let first: Vec<_> = session.results.iter().take(25).map(|s| s.stream_url.clone()).collect();
    session.load_page(1, &std::sync::atomic::AtomicBool::new(false));
    assert_eq!(first, session.results.iter().take(25).map(|s| s.stream_url.clone()).collect::<Vec<_>>());
    println!("Live catalog returned {} unique stations after requesting two display pages", session.results.len());
}
'''
    (harness / "lib.rs").write_text(library, encoding="utf-8")
    (harness / "Cargo.toml").write_text('''
[package]
name = "sonarpad-radio-catalog-tests"
version = "0.0.0"
edition = "2024"
[lib]
path = "lib.rs"
[dependencies]
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
reqwest = { version = "0.12", features = ["blocking", "json", "rustls-tls"] }
url = "2.5"
''', encoding="utf-8")
    command = ["cargo", "test", "--manifest-path", str(harness / "Cargo.toml"),
               "--target-dir", str(ROOT / "target"), "--lib"]
    if live:
        command += ["--", "--ignored", "--nocapture"]
    subprocess.run(command, cwd=ROOT, check=True, timeout=240)


if __name__ == "__main__":
    run_tests(live="--live" in sys.argv)

"""Test diagnostic collection and HTTP delivery locally, without sending email."""
from pathlib import Path
import json
import subprocess

ROOT = Path(__file__).resolve().parents[1]
harness = ROOT / 'target/developer-report-tests'
harness.mkdir(parents=True, exist_ok=True)
(harness / 'Cargo.toml').write_text('''[package]
name = "sonarpad-report-tests"
version = "0.5.3"
edition = "2024"
[lib]
path = "lib.rs"
[dependencies]
serde_json = "1.0"
regex = "1.12"
reqwest = { version = "0.12", features = ["blocking", "json", "rustls-tls"] }
''', encoding='utf-8')
(harness / 'lib.rs').write_text('#[path = '+json.dumps((ROOT / 'src/developer_report.rs').as_posix())+']\nmod developer_report;\n', encoding='utf-8')
for lang in ['it','en','fr','es','pt','pl','cs']:
    data = json.loads((ROOT / f'i18n/ui_{lang}.json').read_text(encoding='utf-8'))
    assert len([key for key in data if key.startswith('report_')]) == 11
subprocess.run(['cargo','test','--manifest-path',str(harness / 'Cargo.toml')], check=True, cwd=ROOT)

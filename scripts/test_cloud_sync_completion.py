"""Run the production Mac sync completion with isolated storage/UI doubles."""
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def item(source, marker):
    start = source.index(marker)
    end = source.index("\n}", start) + 2
    return source[start:end]


def main():
    source = (ROOT / "src/cloud_sync_window.rs").read_text(encoding="utf-8")
    pieces = [(ROOT / "scripts/cloud_sync_completion_tests.rs").read_text(encoding="utf-8")]
    for marker in ["fn diagnostic(", "fn normalize_sync_settings(", "fn snapshot_for_sync(",
                   "pub(crate) fn snapshot(", "fn finish("]:
        pieces.append(item(source, marker))
    session_source = (ROOT / "src/cloud_sync.rs").read_text(encoding="utf-8")
    methods = []
    for marker in ["    pub fn metadata(", "    pub fn save("]:
        start = session_source.index(marker)
        end = session_source.index("\n    }", start) + len("\n    }")
        methods.append(session_source[start:end])
    pieces.append("mod metadata { use super::*; use std::{io::Write, path::{Path, PathBuf}}; "
                  "struct Session { path: PathBuf, meta: Value } impl Session {"
                  + "\n".join(methods) + "} " + r'''
    #[test]
    fn metadata_errors_and_roundtrip() {
        let dir = std::env::temp_dir().join(format!("sonarpad-sync-meta-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("cloud_sync.json");
        assert_eq!(Session::metadata(&dir).unwrap(), json!({}));
        std::fs::write(&path, "broken").unwrap();
        assert_eq!(Session::metadata(&dir).unwrap_err(), "sync_metadata_parse_error");
        std::fs::write(&path, "[]").unwrap();
        assert_eq!(Session::metadata(&dir).unwrap_err(), "sync_metadata_not_object");
        let session = Session { path: path.clone(), meta: json!({"sequence":3}) };
        session.save().unwrap();
        assert_eq!(Session::metadata(&dir).unwrap(), session.meta);
        let blocked = path.with_extension("json.tmp");
        std::fs::create_dir(&blocked).unwrap();
        assert!(session.save().unwrap_err().starts_with("sync_metadata_create_error:"));
        assert_eq!(Session::metadata(&dir).unwrap(), session.meta);
        std::fs::remove_dir(&blocked).unwrap();
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }
    }''')
    harness = ROOT / "target/cloud-sync-completion"
    harness.mkdir(parents=True, exist_ok=True)
    (harness / "lib.rs").write_text("\n\n".join(pieces), encoding="utf-8")
    (harness / "Cargo.toml").write_text('''[package]
name = "sonarpad-cloud-sync-completion-tests"
version = "0.0.0"
edition = "2024"
[lib]
path = "lib.rs"
[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
''', encoding="utf-8")
    subprocess.run(["cargo", "test", "--manifest-path", str(harness / "Cargo.toml"),
                    "--target-dir", str(ROOT / "target"), "--lib"], check=True, cwd=ROOT)


if __name__ == "__main__":
    main()

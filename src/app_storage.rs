use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

pub fn test_directory() -> PathBuf {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        let path = std::env::temp_dir().join(format!(
            "sonarpad-mac-tests-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&path).expect("isolated test directory");
        path
    })
    .clone()
}

pub fn write_atomically(path: &Path, data: &[u8]) -> std::io::Result<()> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let temporary = path.with_extension(format!(
        "{}.{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)?;
            file.write_all(data)?;
            file.sync_all()?;
        }
        // Same-volume rename replaces the complete file on macOS without truncating it.
        std::fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _cleanup = std::fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn concurrent_readers_never_see_truncated_settings() {
        let path = test_directory().join("concurrent.json");
        let old = vec![b'a'; 16384];
        let new = vec![b'b'; 32768];
        write_atomically(&path, &old).unwrap();
        std::thread::scope(|scope| {
            scope.spawn(|| {
                for _ in 0..200 {
                    let _result = write_atomically(&path, &new);
                    let _result = write_atomically(&path, &old);
                }
            });
            for _ in 0..1000 {
                let bytes = std::fs::read(&path).unwrap();
                assert!(bytes == old || bytes == new);
            }
        });
    }
    #[test]
    fn failed_replacement_preserves_destination() {
        let path = test_directory().join("protected-directory");
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(path.join("existing"), b"preserved").unwrap();
        assert!(write_atomically(&path, b"replacement").is_err());
        assert_eq!(std::fs::read(path.join("existing")).unwrap(), b"preserved");
    }
    #[test]
    fn test_directory_is_stable_and_temporary() {
        assert_eq!(test_directory(), test_directory());
        assert!(test_directory().starts_with(std::env::temp_dir()));
    }
}

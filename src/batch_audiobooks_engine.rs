//! A sequential document queue; the synthesis adapter is injected so the queue
//! and its file safety/cancellation rules can also be exercised without macOS.
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

pub const EXTENSIONS: &[&str] = &[
    "txt", "md", "pdf", "doc", "docx", "epub", "rtf", "html", "htm", "xlsx", "xls", "ods",
];
pub const FORMATS: &[&str] = &["mp3", "m4b", "m4a", "wav"];

pub fn add_files(queue: &mut Vec<PathBuf>, paths: impl IntoIterator<Item = PathBuf>) {
    for path in paths {
        if path.is_file()
            && path.extension().is_some_and(|e| {
                EXTENSIONS.contains(&e.to_string_lossy().to_ascii_lowercase().as_str())
            })
        {
            let path = std::fs::canonicalize(&path).unwrap_or(path);
            if !queue.contains(&path) {
                queue.push(path);
            }
        }
    }
}

pub fn folder_files(folder: &Path) -> Result<Vec<PathBuf>, String> {
    let mut paths = std::fs::read_dir(folder)
        .map_err(|e| e.to_string())?
        .map(|entry| entry.map(|e| e.path()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    paths.sort_by_key(|p| {
        p.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase()
    });
    let mut files = Vec::new();
    add_files(&mut files, paths);
    Ok(files)
}

#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    Pending,
    Running,
    Done,
    Failed,
    Cancelled,
}
#[derive(Clone, Debug)]
pub struct Item {
    pub input: PathBuf,
    pub status: Status,
    pub output: Option<PathBuf>,
    pub error: Option<String>,
}
#[derive(Clone, Debug)]
pub struct Progress {
    pub items: Vec<Item>,
    pub current: usize,
    pub percent: u32,
    pub finished: bool,
    pub cancelled: bool,
}
impl Progress {
    pub fn new(files: &[PathBuf]) -> Self {
        Self {
            items: files
                .iter()
                .map(|input| Item {
                    input: input.clone(),
                    status: Status::Pending,
                    output: None,
                    error: None,
                })
                .collect(),
            current: 0,
            percent: 0,
            finished: false,
            cancelled: false,
        }
    }
}

fn publish_unique(
    staged: &Path,
    folder: &Path,
    stem: &str,
    extension: &str,
) -> Result<PathBuf, String> {
    for number in 0..100_000 {
        let suffix = if number == 0 {
            String::new()
        } else {
            format!("_{number}")
        };
        let output = folder.join(format!("{stem}{suffix}.{extension}"));
        match std::fs::hard_link(staged, &output) {
            Ok(()) => return Ok(output),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => {
                // FAT/network destinations may not support hard links. Exclusive
                // creation still prevents replacing any existing/source file.
                let mut target = match std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&output)
                {
                    Ok(file) => file,
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(e) => return Err(e.to_string()),
                };
                let result = std::fs::File::open(staged)
                    .and_then(|mut input| std::io::copy(&mut input, &mut target))
                    .and_then(|_| target.sync_all());
                if let Err(e) = result {
                    drop(target);
                    let _ = std::fs::remove_file(&output);
                    return Err(e.to_string());
                }
                return Ok(output);
            }
        }
    }
    Err("Cannot allocate an output filename".into())
}

pub fn run(
    files: &[PathBuf],
    output: &Path,
    extension: &str,
    subfolders: bool,
    cancel: &AtomicBool,
    mut render: impl FnMut(&Path, &Path, &AtomicBool, &mut dyn FnMut(u32)) -> Result<(), String>,
    mut report: impl FnMut(&Progress),
) -> Progress {
    let mut state = Progress::new(files);
    report(&state);
    for (index, input) in files.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        state.current = index;
        state.items[index].status = Status::Running;
        report(&state);
        let result = (|| {
            if !FORMATS.contains(&extension) {
                return Err("Unsupported audiobook format".into());
            }
            let stem = input.file_stem().unwrap_or_default().to_string_lossy();
            let folder = if subfolders {
                output.join(stem.as_ref())
            } else {
                output.to_path_buf()
            };
            std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
            let work = folder.join(format!(".sonarpad-audiobook-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir(&work).map_err(|e| e.to_string())?;
            let staged = work.join(format!("audiobook.{extension}"));
            let result = (|| {
                render(input, &staged, cancel, &mut |pct| {
                    let pct = ((index * 100 + pct.min(99) as usize) / files.len()) as u32;
                    if pct > state.percent {
                        state.percent = pct;
                        report(&state);
                    }
                })?;
                if cancel.load(Ordering::Relaxed) {
                    return Err("__CANCELLED__".into());
                }
                if std::fs::metadata(&staged).map_err(|e| e.to_string())?.len() == 0 {
                    return Err("Empty audiobook output".into());
                }
                publish_unique(&staged, &folder, &stem, extension)
            })();
            let _ = std::fs::remove_dir_all(&work);
            result
        })();
        if cancel.load(Ordering::Relaxed) && result.is_err() {
            state.items[index].status = Status::Cancelled;
            break;
        }
        match result {
            Ok(path) => {
                state.items[index].status = Status::Done;
                state.items[index].output = Some(path);
            }
            Err(error) => {
                state.items[index].status = Status::Failed;
                state.items[index].error = Some(error);
            }
        }
        state.percent = ((index + 1) * 100 / files.len()) as u32;
        report(&state);
    }
    state.cancelled = cancel.load(Ordering::Relaxed);
    if state.cancelled {
        for item in &mut state.items {
            if matches!(item.status, Status::Pending | Status::Running) {
                item.status = Status::Cancelled;
            }
        }
    }
    state.finished = true;
    report(&state);
    state
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> PathBuf {
        let p = std::env::temp_dir().join(format!("sonarpad-batch-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&p).unwrap();
        p
    }
    #[test]
    fn folder_filters_sorts_and_deduplicates_documents() {
        let dir = fixture();
        for name in ["z.txt", "A.EPUB", "ignore.mp3"] {
            std::fs::write(dir.join(name), b"text").unwrap();
        }
        std::fs::create_dir(dir.join("subfolder.txt")).unwrap();
        let mut files = folder_files(&dir).unwrap();
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].file_name().unwrap(), "A.EPUB");
        add_files(&mut files, [dir.join("z.txt")]);
        assert_eq!(files.len(), 2);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn collisions_errors_and_progress_are_handled_per_document() {
        let dir = fixture();
        let files: Vec<_> = ["book.txt", "book.epub", "bad.txt", "last.txt"]
            .iter()
            .map(|n| dir.join(n))
            .collect();
        std::fs::write(dir.join("book.mp3"), b"existing").unwrap();
        let cancel = AtomicBool::new(false);
        let mut values = Vec::new();
        let result = run(
            &files,
            &dir,
            "mp3",
            false,
            &cancel,
            |input, output, _, progress| {
                if input.file_stem().unwrap() == "bad" {
                    return Err("bad document".into());
                }
                progress(60);
                std::fs::write(output, b"rendered").map_err(|e| e.to_string())
            },
            |p| values.push(p.percent),
        );
        assert_eq!(
            result
                .items
                .iter()
                .filter(|i| i.status == Status::Done)
                .count(),
            3
        );
        assert_eq!(result.items[2].status, Status::Failed);
        assert_eq!(result.percent, 100);
        assert!(values.windows(2).all(|p| p[0] <= p[1]));
        assert_eq!(std::fs::read(dir.join("book.mp3")).unwrap(), b"existing");
        assert!(dir.join("book_1.mp3").exists() && dir.join("book_2.mp3").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn cancellation_never_publishes_partial_or_starts_next_book() {
        let dir = fixture();
        let cancel = AtomicBool::new(false);
        let files = vec![dir.join("one.txt"), dir.join("two.txt")];
        let mut calls = 0;
        let result = run(
            &files,
            &dir,
            "wav",
            true,
            &cancel,
            |_, output, flag, progress| {
                calls += 1;
                std::fs::write(output, b"partial").unwrap();
                progress(40);
                flag.store(true, Ordering::Relaxed);
                Ok(())
            },
            |_| {},
        );
        assert_eq!(calls, 1);
        assert!(result.cancelled && result.finished);
        assert!(result.percent < 100);
        assert!(!dir.join("one/one.wav").exists());
        assert!(!dir.join("two").exists());
        assert_eq!(std::fs::read_dir(dir.join("one")).unwrap().count(), 0);
        std::fs::remove_dir_all(dir).unwrap();
    }
}

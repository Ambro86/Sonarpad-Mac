//! Non-destructive Sonarpad media editing projects and safe export replacement.
use super::{MIN_PART_SECONDS, MediaPart};

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const PROJECT_VERSION: u32 = 1;
const MAX_PROJECT_BYTES: u64 = 4 * 1024 * 1024;
static UNIQUE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectTrack {
    pub path: PathBuf,
    pub original_volume: f64,
    pub new_volume: f64,
    pub loop_track: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CutterProject {
    pub version: u32,
    pub input: PathBuf,
    pub duration: f64,
    pub has_video: bool,
    pub parts: Vec<MediaPart>,
    pub sources: Vec<serde_json::Value>,
    pub position: f64,
    pub rotation: usize,
    pub track: Option<ProjectTrack>,
    pub mode: usize,
    pub movement: usize,
    pub goldwave_keys: bool,
}

impl CutterProject {
    pub fn new(input: PathBuf, duration: f64, has_video: bool, parts: Vec<MediaPart>) -> Self {
        Self {
            version: PROJECT_VERSION,
            input,
            duration,
            has_video,
            parts,
            sources: Vec::new(),
            position: 0.0,
            rotation: 0,
            track: None,
            mode: 0,
            movement: 0,
            goldwave_keys: false,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.version != PROJECT_VERSION {
            return Err("Unsupported Sonarpad project version".into());
        }
        if !self.duration.is_finite()
            || self.duration < MIN_PART_SECONDS
            || !self.position.is_finite()
            || !(0.0..=self.duration).contains(&self.position)
            || self.rotation >= 4
            || self.mode >= 2
            || self.movement >= 8
            || self.parts.is_empty()
            || self.parts.len() > 100_000
        {
            return Err("Invalid media project values".into());
        }
        if !self.input.is_file() {
            return Err(format!(
                "Original media file missing: {}",
                self.input.display()
            ));
        }
        // Advanced editing may trim the beginning or end of a part, leaving gaps.
        // The export engine accepts ordered non-overlapping parts, not necessarily a
        // contiguous partition of the original file.
        let mut previous = 0.0;
        for p in &self.parts {
            if !p.start.is_finite()
                || !p.end.is_finite()
                || p.start < 0.0
                || p.start < previous - 0.000001
                || p.end - p.start < 0.000001
                || p.end > self.duration + 0.002
            {
                return Err("Invalid media project segments".into());
            }
            previous = p.end;
        }
        if !self.sources.is_empty() {
            return Err(
                "This project contains inserted media that this editor does not support".into(),
            );
        }
        if let Some(track) = &self.track
            && (!track.path.is_file()
                || !track.original_volume.is_finite()
                || !track.new_volume.is_finite()
                || !(0.0..=200.0).contains(&track.original_volume)
                || !(0.0..=200.0).contains(&track.new_volume))
        {
            return Err("Invalid or missing additional audio track".into());
        }
        Ok(())
    }
}

pub fn save(path: &Path, project: &CutterProject) -> Result<(), String> {
    project.validate()?;
    if !path
        .extension()
        .is_some_and(|v| v.to_string_lossy().eq_ignore_ascii_case("sonarcut"))
    {
        return Err("A Sonarpad project must have the .sonarcut extension".into());
    }
    let source_paths =
        std::iter::once(&project.input).chain(project.track.iter().map(|track| &track.path));
    if source_paths
        .into_iter()
        .any(|source| same_file(source, path))
    {
        return Err("A project must not overwrite any source media file".into());
    }
    let json = serde_json::to_vec_pretty(project).map_err(|e| e.to_string())?;
    if json.len() as u64 > MAX_PROJECT_BYTES {
        return Err("Project exceeds the supported size".into());
    }
    let temporary = unused_sibling(path, "project-tmp")?;
    fs::write(&temporary, json).map_err(|e| format!("Cannot write project: {e}"))?;
    let result = if path.exists() {
        backup_and_replace(path, &temporary).map(|_| ())
    } else {
        fs::rename(&temporary, path).map_err(|e| format!("Cannot save project: {e}"))
    };
    if result.is_err()
        && let Err(error) = fs::remove_file(&temporary)
    {
        crate::append_podcast_log(&format!("Failed to remove temporary cutter file: {error}"));
    }
    result
}

pub fn load(path: &Path) -> Result<CutterProject, String> {
    if fs::metadata(path)
        .map_err(|e| format!("Cannot open project: {e}"))?
        .len()
        > MAX_PROJECT_BYTES
    {
        return Err("Project file is too large".into());
    }
    let bytes = fs::read(path).map_err(|e| format!("Cannot read project: {e}"))?;
    let project: CutterProject =
        serde_json::from_slice(&bytes).map_err(|e| format!("Invalid Sonarpad project: {e}"))?;
    project.validate()?;
    Ok(project)
}

fn unused_sibling(path: &Path, tag: &str) -> Result<PathBuf, String> {
    let name = path
        .file_stem()
        .ok_or("Missing file name")?
        .to_string_lossy();
    let ext = path
        .extension()
        .map(|v| v.to_string_lossy().into_owned())
        .unwrap_or_default();
    for _ in 0..100 {
        let seq = UNIQUE.fetch_add(1, Ordering::Relaxed);
        let candidate = path.with_file_name(if ext.is_empty() {
            format!("{name}.{tag}-{}-{seq}", std::process::id())
        } else {
            format!("{name}.{tag}-{}-{seq}.{ext}", std::process::id())
        });
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err("Could not find an unused temporary filename".into())
}

pub fn backup_and_replace(destination: &Path, temporary: &Path) -> Result<PathBuf, String> {
    if !destination.is_file() || !temporary.is_file() {
        return Err("Missing destination or completed export".into());
    }
    let backup = unused_sibling(destination, "sonarpad-backup")?;
    fs::rename(destination, &backup)
        .map_err(|e| format!("Could not create original-file backup: {e}"))?;
    if let Err(err) = fs::rename(temporary, destination) {
        let restore = fs::rename(&backup, destination);
        return Err(match restore {
            Ok(()) => format!("Could not install exported file; original restored: {err}"),
            Err(restore_err) => format!(
                "Could not install exported file: {err}. CRITICAL: original remains in backup {}: {restore_err}",
                backup.display()
            ),
        });
    }
    Ok(backup)
}

pub fn same_file(a: &Path, b: &Path) -> bool {
    a == b
        || a.canonicalize()
            .ok()
            .zip(b.canonicalize().ok())
            .is_some_and(|(a, b)| a == b)
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path =
                std::env::temp_dir().join(format!("cutter-project-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
        fn project(&self) -> CutterProject {
            let input = self.0.join("original audio.wav");
            fs::write(&input, b"original").unwrap();
            CutterProject::new(
                input,
                6.0,
                false,
                vec![
                    MediaPart {
                        start: 0.0,
                        end: 2.0,
                        keep: true,
                    },
                    MediaPart {
                        start: 2.0,
                        end: 4.0,
                        keep: false,
                    },
                    MediaPart {
                        start: 4.0,
                        end: 6.0,
                        keep: true,
                    },
                ],
            )
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn project_roundtrip_keeps_edits_cursor_track_and_preferences() {
        let f = Fixture::new();
        let mut p = f.project();
        let track = f.0.join("new track.wav");
        fs::write(&track, b"track").unwrap();
        p.track = Some(ProjectTrack {
            path: track,
            original_volume: 80.0,
            new_volume: 25.0,
            loop_track: true,
        });
        p.position = 4.5;
        p.mode = 1;
        p.rotation = 3;
        p.movement = 7;
        p.goldwave_keys = true;
        let path = f.0.join("editing.sonarcut");
        save(&path, &p).unwrap();
        let loaded = load(&path).unwrap();
        assert_eq!(
            serde_json::to_value(&loaded).unwrap(),
            serde_json::to_value(&p).unwrap()
        );
        assert_eq!(fs::read(&p.input).unwrap(), b"original");
        assert!(!loaded.parts[1].keep);
    }
    #[test]
    fn invalid_and_missing_media_are_rejected_before_opening() {
        let f = Fixture::new();
        let mut p = f.project();
        p.parts[1].start = 1.0;
        assert!(p.validate().is_err());
        p.parts[1].start = 2.0;
        p.position = 10.0;
        assert!(p.validate().is_err());
        p.position = 0.0;
        p.version = 2;
        assert!(p.validate().is_err());
        p.version = 1;
        p.sources.push(serde_json::json!({"path":"inserted.wav"}));
        assert!(p.validate().is_err());
        p.sources.clear();
        fs::remove_file(&p.input).unwrap();
        assert!(p.validate().is_err());
    }
    #[test]
    fn project_cannot_overwrite_source_and_existing_save_is_backed_up() {
        let f = Fixture::new();
        let p = f.project();
        assert!(save(&p.input, &p).is_err());
        let path = f.0.join("editing.sonarcut");
        save(&path, &p).unwrap();
        let old = fs::read(&path).unwrap();
        let mut modified = p;
        modified.position = 2.0;
        save(&path, &modified).unwrap();
        let backup = fs::read_dir(&f.0)
            .unwrap()
            .flatten()
            .find(|e| e.file_name().to_string_lossy().contains("sonarpad-backup"))
            .unwrap();
        assert_eq!(fs::read(backup.path()).unwrap(), old);
        assert_eq!(load(&path).unwrap().position, 2.0);
    }
    #[test]
    fn replacement_preserves_previous_file_and_rejects_missing_export() {
        let f = Fixture::new();
        let original = f.0.join("original.wav");
        let pending = f.0.join("pending.wav");
        fs::write(&original, b"original").unwrap();
        assert!(backup_and_replace(&original, &pending).is_err());
        assert_eq!(fs::read(&original).unwrap(), b"original");
        fs::write(&pending, b"edited").unwrap();
        let backup = backup_and_replace(&original, &pending).unwrap();
        assert_eq!(fs::read(&original).unwrap(), b"edited");
        assert_eq!(fs::read(&backup).unwrap(), b"original");
        assert!(!pending.exists());
    }
}

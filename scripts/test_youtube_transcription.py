"""Exercise the actual YouTube transcription download lifecycle without a GUI/network."""
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]
WORK = ROOT / 'target/youtube-transcription-tests'

def main():
    source = (ROOT / 'src/main.rs').read_text(encoding='utf8')
    start = source.index('struct YoutubeTranscriptionDownload {')
    end = source.index('fn open_youtube_results_dialog(', start)
    WORK.mkdir(parents=True, exist_ok=True)
    (WORK / 'Cargo.toml').write_text('''[package]
name="youtube-transcription-tests"
version="0.0.0"
edition="2024"
[workspace]
[dependencies]
uuid={version="1.19",features=["v4"]}
''', encoding='utf8')
    (WORK / 'src').mkdir(exist_ok=True)
    harness = r'''
use std::path::PathBuf;
use std::sync::Mutex;
static LAST_DOWNLOAD: Mutex<Option<PathBuf>> = Mutex::new(None);
fn append_podcast_log(_message: &str) {}
fn sanitize_filename(title: &str) -> String { title.replace('/', "_") }
fn save_youtube_to_path(url: &str, format: &str, quality: &str, path: PathBuf) -> Result<PathBuf, String> {
    assert_eq!(format, "mp3");
    assert_eq!(quality, "best");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"audio fixture").unwrap();
    *LAST_DOWNLOAD.lock().unwrap() = Some(path.clone());
    if url == "failed" { Err("download failed".into()) } else { Ok(path) }
}
'''
    tests = r'''
#[test]
fn downloaded_audio_is_available_then_cleaned_on_close() {
    let path;
    {
        let download = download_youtube_for_transcription("fixture", "Video di prova").unwrap();
        path = download.path.clone();
        assert!(path.is_file());
        assert_eq!(path.file_name().unwrap(), "Video di prova.mp3");
    }
    assert!(!path.exists());
    assert!(!path.parent().unwrap().exists());
}
#[test]
fn identical_titles_do_not_overwrite_each_other() {
    let first = download_youtube_for_transcription("one", "Titolo").unwrap();
    let second = download_youtube_for_transcription("two", "Titolo").unwrap();
    assert_ne!(first.path, second.path);
    assert!(first.path.is_file() && second.path.is_file());
}
#[test]
fn failed_download_removes_partial_audio() {
    assert!(download_youtube_for_transcription("failed", "Parziale").is_err());
    let path = LAST_DOWNLOAD.lock().unwrap().clone().unwrap();
    assert!(!path.exists());
    assert!(!path.parent().unwrap().exists());
}
#[test]
fn cleanup_keeps_transcript_saved_beside_audio() {
    let transcript;
    {
        let download = download_youtube_for_transcription("fixture", "Audio").unwrap();
        transcript = download.path.with_extension("txt");
        std::fs::write(&transcript, "Testo trascritto").unwrap();
    }
    assert_eq!(std::fs::read_to_string(&transcript).unwrap(), "Testo trascritto");
    std::fs::remove_file(&transcript).unwrap();
    std::fs::remove_dir(transcript.parent().unwrap()).unwrap();
}
'''
    (WORK / 'src/lib.rs').write_text(harness + source[start:end] + tests, encoding='utf8')
    subprocess.run(['cargo', 'test', '--offline', '--manifest-path', str(WORK / 'Cargo.toml'), '--', '--test-threads=1'], check=True)

if __name__ == '__main__':
    main()

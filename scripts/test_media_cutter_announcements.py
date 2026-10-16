"""Exercise the production announcement queue with a blocked speech backend."""
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
source = (ROOT / 'src/media_cutter_accessibility.rs').read_text(encoding='utf-8')
start = source.index('pub(super) fn announce_voiceover_message(')
end = source.index('\n}', start) + 2
function = source[start:end].replace('pub(super) fn', 'fn', 1)
code = r'''
use std::sync::{Condvar, Mutex};
use std::time::Duration;
static DELIVERED: Mutex<Vec<String>> = Mutex::new(Vec::new());
static WAKE: Condvar = Condvar::new();
static RELEASE: (Mutex<bool>, Condvar) = (Mutex::new(false), Condvar::new());
fn deliver_voiceover_message(message: &str) {
    let first = {
        let mut delivered = DELIVERED.lock().unwrap();
        delivered.push(message.to_owned());
        WAKE.notify_all();
        delivered.len() == 1
    };
    if first {
        let (lock, wake) = &RELEASE;
        let state = wake.wait_timeout_while(lock.lock().unwrap(), Duration::from_secs(5), |done| !*done).unwrap();
        assert!(*state.0, "speech worker was never released");
    }
}
fn wait_for_count(count: usize) {
    let state = WAKE.wait_timeout_while(DELIVERED.lock().unwrap(), Duration::from_secs(5), |items| items.len() < count).unwrap();
    assert!(state.0.len() >= count, "missing announcement");
}
''' + function + r'''
fn main() {
    announce_voiceover_message("  ");
    announce_voiceover_message("  first  ");
    wait_for_count(1);
    // Calls must return while the first output is blocked. Only the newest
    // pending navigation message should survive, rather than speaking a backlog.
    announce_voiceover_message("second");
    announce_voiceover_message("third");
    announce_voiceover_message("last");
    assert_eq!(*DELIVERED.lock().unwrap(), vec!["first"]);
    *RELEASE.0.lock().unwrap() = true;
    RELEASE.1.notify_all();
    wait_for_count(2);
    assert_eq!(*DELIVERED.lock().unwrap(), vec!["first", "last"]);
}
'''
with tempfile.TemporaryDirectory(prefix='cutter-speech-') as folder:
    folder = Path(folder)
    rust = folder / 'queue.rs'
    exe = folder / 'queue-test.exe'
    rust.write_text(code, encoding='utf-8')
    subprocess.run(['rustc', '--edition=2024', str(rust), '-o', str(exe)], check=True)
    subprocess.run([str(exe)], check=True, timeout=15)
print('Speech queue: nonblocking, serialized, empty messages ignored, latest pending feedback retained.')

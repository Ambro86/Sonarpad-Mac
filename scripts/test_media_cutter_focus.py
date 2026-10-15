"""Run the cutter's initial-focus callback on repeated simulated timer events."""
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
source = (ROOT / "src/media_cutter.rs").read_text(encoding="utf-8")
dialog = source[source.index("pub fn open_dialog("):source.index("#[cfg(test)]")]
# wxdragon 0.9.18 delivers all timer events through the owner's callbacks.
assert dialog.count("Timer::new(&dialog)") == 1
start = dialog.index("    let initial_focus_pending =")
end = dialog.index("        if let Some(info) = probe_tick", start)
callback = dialog[start:end] + "    });\n"
code = r'''
use std::{cell::{Cell, RefCell}, rc::Rc};
struct Button(Rc<Cell<usize>>);
impl Button { fn set_focus(&self) { self.0.set(self.0.get() + 1); } }
struct Timer(RefCell<Option<Box<dyn FnMut(())>>>);
impl Timer {
    fn on_tick(&self, f: impl FnMut(()) + 'static) { *self.0.borrow_mut() = Some(Box::new(f)); }
    fn tick(&self) { self.0.borrow_mut().as_mut().unwrap()(()); }
}
fn append_podcast_log(_: &str) {}
fn main() {
    for _reopen in 0..3 {
        let focus_count = Rc::new(Cell::new(0));
        let open_button = Button(Rc::clone(&focus_count));
        let timer_tick = Timer(RefCell::new(None));
''' + callback + r'''
        assert_eq!(focus_count.get(), 0);
        timer_tick.tick();
        assert_eq!(focus_count.get(), 1);
        // Once the user navigates away, playback/export ticks must not refocus.
        for _ in 0..1000 { timer_tick.tick(); }
        assert_eq!(focus_count.get(), 1);
    }
}
'''
with tempfile.TemporaryDirectory(prefix="sonarpad-cutter-focus-") as directory:
    path = Path(directory)
    rust = path / "focus.rs"
    executable = path / "focus-test.exe"
    rust.write_text(code, encoding="utf-8")
    subprocess.run(["rustc", "--edition=2024", str(rust), "-o", str(executable)], check=True)
    subprocess.run([str(executable)], check=True)
print("Initial focus runs once per opening; 1000 subsequent ticks leave focus alone.")

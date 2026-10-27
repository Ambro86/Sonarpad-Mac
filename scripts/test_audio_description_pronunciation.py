"""Compile and test the production Rust pronunciation helper without a Mac build."""
from pathlib import Path
import subprocess
import tempfile

source = (Path(__file__).resolve().parents[1] / "src/audio_description.rs").read_text(encoding="utf-8")
marker = "fn normalize_audio_description_pronunciation("
start = source.index(marker)
function = source[start:source.index("\n}", start) + 2]
assert "let pronunciation_text = normalize_audio_description_pronunciation(text);" in source
code = function + r'''
fn main() {
    let cases = [
        ("Dr._Arinsu e Mr._Rossi.", "dr Arinsu e mr Rossi."),
        ("DR. Dr. dr. dR. MR. Mr. mr. mR.", "dr dr dr dr mr mr mr mr"),
        ("Dr.Dr._Mr.Mr.", "drdr mrmr"),
        ("(Dr. Arinsu), «Mr. Rossi»!", "(dr Arinsu), «mr Rossi»!"),
        ("Mrs. Ms. Prof. dr mr", "Mrs. Ms. Prof. dr mr"),
        ("adr. amr. 2dr. édr. 中mr.", "adr. amr. 2dr. édr. 中mr."),
        ("  Dr.\nArinsu\tMr. Rossi  ", "  dr\nArinsu\tmr Rossi  "),
        ("Jon_Snow entra. 3.14, fine!", "Jon Snow entra. 3.14, fine!"),
        ("", ""),
    ];
    for (input, expected) in cases {
        assert_eq!(normalize_audio_description_pronunciation(input), expected);
    }
}
'''
with tempfile.TemporaryDirectory(prefix="sonarpad-pronunciation-") as tmp:
    src = Path(tmp) / "pronunciation.rs"
    exe = Path(tmp) / "pronunciation.exe"
    src.write_text(code, encoding="utf-8")
    subprocess.run(["rustc", "--edition=2021", str(src), "-o", str(exe)], check=True)
    subprocess.run([str(exe)], check=True)
print("9 production Rust pronunciation cases passed.")

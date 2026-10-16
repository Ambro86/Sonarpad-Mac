"""Local batch tests: real document loaders, queue, Edge cache and FFmpeg output.

The Apple voice is replaced with a deterministic playable MP3 fixture: this
does not claim to exercise /usr/bin/say or VoiceOver without a Mac.
"""
import json
import math
from pathlib import Path
import shutil
import struct
import subprocess
import sys
import wave
import zipfile

ROOT = Path(__file__).resolve().parents[1]
WORK = ROOT / "target/batch-audiobook-tests"


def run(args, **kwargs):
    return subprocess.run([str(a) for a in args], check=True, **kwargs)


def main():
    ffmpeg = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else Path(shutil.which("ffmpeg")).resolve()
    WORK.mkdir(parents=True, exist_ok=True)
    with wave.open(str(WORK / "voice.wav"), "wb") as writer:
        writer.setparams((1, 2, 24000, 0, "NONE", ""))
        writer.writeframes(b"".join(struct.pack("<h", int(5000 * math.sin(2 * math.pi * 440 * n / 24000))) for n in range(24000)))
    run([ffmpeg, "-v", "error", "-y", "-i", WORK / "voice.wav", "-c:a", "libmp3lame", "-b:a", "128k", WORK / "voice.mp3"])
    (WORK / "book.txt").write_text("Test batch audiolibri. Questo è un documento di prova.", encoding="utf-8")
    (WORK / "book.html").write_text("<html><body><h1>Test batch</h1><p>Un documento HTML di prova.</p></body></html>", encoding="utf-8")
    (WORK / "book.rtf").write_text(r"{\rtf1\ansi Test batch. Documento RTF di prova.}", encoding="ascii")
    (WORK / "empty.txt").write_text(" \n ", encoding="utf-8")
    (WORK / "bad.epub").write_bytes(b"not an epub")
    with zipfile.ZipFile(WORK / "book.epub", "w") as epub:
        epub.writestr("mimetype", "application/epub+zip")
        epub.writestr("META-INF/container.xml", '<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>')
        epub.writestr("content.opf", '<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="id">batch</dc:identifier><dc:title>Test batch</dc:title><dc:language>it</dc:language></metadata><manifest><item id="chapter" href="chapter.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="chapter"/></spine></package>')
        epub.writestr("chapter.xhtml", '<html xmlns="http://www.w3.org/1999/xhtml"><body><p>Test batch. Documento EPUB di prova.</p></body></html>')
    dependencies = (ROOT / "Cargo.toml").read_text(encoding="utf-8").split("[dependencies]\n", 1)[1].split("[target.", 1)[0]
    needed = {"tokio", "reqwest", "tokio-tungstenite", "serde", "serde_json", "uuid", "chrono", "futures-util", "anyhow", "hex", "rand", "sha2", "url", "rodio", "hound", "docx-rs", "pdf-extract", "pdfium-render", "epub", "calamine", "encoding_rs", "cfb", "html2text", "regex", "quick-xml", "scraper"}
    dependencies = "\n".join(line for line in dependencies.splitlines() if line.split("=", 1)[0].strip() in needed)
    (WORK / "Cargo.toml").write_text('[package]\nname="batch-audiobook-tests"\nversion="0.0.0"\nedition="2024"\n[workspace]\n[[bin]]\nname="batch-audiobook-tests"\npath="main.rs"\n[dependencies]\n' + dependencies, encoding="utf-8")
    modules = ["file_loader", "edge_tts", "edge_tts_support", "media_join_engine", "batch_audiobooks_engine", "batch_audiobooks_render"]
    source = '#![allow(dead_code)]\n' + "\n".join(f'#[path={json.dumps((ROOT / "src" / (m+".rs")).as_posix())}] mod {m};' for m in modules)
    source += r'''
use std::{path::PathBuf,sync::atomic::{AtomicBool,Ordering}};
struct Settings {voice_engine:String,system_voice:String,voice:String,rate:i32,pitch:i32,volume:i32,disable_blank_line_pauses:bool}
fn normalized_voice_engine(s:&str)->String{s.into()}
fn is_system_voice_engine(s:&str)->bool{s=="system"}
fn append_podcast_log(s:&str){eprintln!("{s}")}
fn root()->PathBuf{PathBuf::from(std::env::args().nth(1).unwrap())}
fn ffmpeg_executable_path()->Option<PathBuf>{Some(PathBuf::from(std::env::args().nth(2).unwrap()))}
fn app_storage_path(s:&str)->PathBuf{root().join(s)}
fn prepare_text_for_tts(s:&str,_:bool)->String{s.into()}
fn synthesize_system_voice_to_mp3(text:&str,_:&str,_:i32,_:i32,_:i32)->Result<Vec<u8>,String>{
    assert!(text.to_lowercase().contains("batch"), "document text was not loaded: {text}");
    std::fs::read(root().join("voice.mp3")).map_err(|e|e.to_string())
}
fn main(){
    use batch_audiobooks_engine::{run,Status};
    use batch_audiobooks_render::{render,Voice};
    let dir=root();let rt=tokio::runtime::Runtime::new().unwrap();let flag=AtomicBool::new(false);
    let voice=Voice{engine:"system".into(),name:"Simulated Apple voice".into(),rate:0,pitch:0,volume:0,disable_blank_line_pauses:false};
    let inputs:Vec<_>=["book.txt","book.html","book.rtf","book.epub"].iter().map(|p|dir.join(p)).collect();
    for ext in ["mp3","m4b","m4a","wav"]{
        let destination=dir.join(format!("outputs-{ext}-{}",uuid::Uuid::new_v4()));
        let mut previous=0;
        let result=run(&inputs,&destination,ext,false,&flag,|i,o,c,p|render(i,o,&voice,&rt,c,p),|p|{assert!(p.percent>=previous);previous=p.percent;});
        assert!(result.finished && !result.cancelled);assert_eq!(result.percent,100);
        for item in &result.items{
            assert_eq!(item.status,Status::Done,"{:?}",item.error);let output=item.output.as_ref().unwrap();
            assert!(std::fs::metadata(output).unwrap().len()>1000);
            // Decode the actual exported file with FFmpeg, rather than trusting an extension.
            let status=std::process::Command::new(ffmpeg_executable_path().unwrap()).args(["-v","error","-i"]).arg(output).args(["-f","null","-"]).status().unwrap();assert!(status.success());
        }
        assert_eq!(std::fs::read_dir(&destination).unwrap().count(),4);
    }
    let out=dir.join(format!("errors-{}",uuid::Uuid::new_v4()));
    let mixed=vec![dir.join("empty.txt"),dir.join("bad.epub"),dir.join("book.txt")];
    let result=run(&mixed,&out,"mp3",true,&flag,|i,o,c,p|render(i,o,&voice,&rt,c,p),|_|{});
    assert_eq!(result.items[0].status,Status::Failed);assert_eq!(result.items[1].status,Status::Failed);assert_eq!(result.items[2].status,Status::Done);
    // Resume through the actual Edge cache, with the network deliberately unnecessary.
    let mut cached_voice=voice;cached_voice.engine="edge".into();cached_voice.name="it-IT-ElsaNeural".into();
    let text=file_loader::load_any_file(&inputs[0]).unwrap();let chunks=edge_tts::split_text_edge_audiobook(&text,&cached_voice.name);
    let cache=edge_tts_support::AudiobookCache::open(&app_storage_path("edge-audiobook-jobs"),&chunks,&cached_voice.name,0,0,0).unwrap();
    for index in 0..chunks.len(){cache.store(index,&std::fs::read(dir.join("voice.mp3")).unwrap()).unwrap();}
    let result=run(&inputs[..1],&out,"wav",false,&flag,|i,o,c,p|render(i,o,&cached_voice,&rt,c,p),|_|{});
    assert_eq!(result.items[0].status,Status::Done,"{:?}",result.items[0].error);
    let cancel_out=dir.join(format!("cancel-{}",uuid::Uuid::new_v4()));
    let result=run(&inputs,&cancel_out,"mp3",false,&flag,|i,o,c,p|render(i,o,&cached_voice,&rt,c,&mut|pct|{p(pct);flag.store(true,Ordering::Relaxed);}),|_|{});
    assert!(result.cancelled);assert!(result.items.iter().all(|i|i.status==Status::Cancelled));assert_eq!(std::fs::read_dir(cancel_out).unwrap().count(),0);
    println!("PASS: 4 document types x 4 audiobook formats decoded; empty/corrupt documents; continuation; cached Edge resume; cancellation without partial files. Apple synthesis simulated.");
}
'''
    (WORK / "main.rs").write_text(source, encoding="utf-8")
    shutil.copyfile(ROOT / "Cargo.lock", WORK / "Cargo.lock")
    target = ROOT / "target"
    run(["cargo", "test", "--manifest-path", WORK / "Cargo.toml", "--target-dir", target, "batch_audiobooks_engine::tests"])
    run(["cargo", "run", "--manifest-path", WORK / "Cargo.toml", "--target-dir", target, "--", WORK, ffmpeg])


if __name__ == "__main__":
    main()

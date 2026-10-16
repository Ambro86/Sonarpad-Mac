//! The batch adapter shares the document loaders, voice settings, retry policy
//! and resumable Edge cache of the ordinary Save audiobook action.
use rodio::Source;
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

pub struct Voice {
    pub engine: String,
    pub name: String,
    pub rate: i32,
    pub pitch: i32,
    pub volume: i32,
    pub disable_blank_line_pauses: bool,
}
impl Voice {
    pub fn from_settings(settings: &crate::Settings) -> Self {
        let engine = crate::normalized_voice_engine(&settings.voice_engine);
        let name = if crate::is_system_voice_engine(&engine) {
            settings.system_voice.clone()
        } else {
            settings.voice.clone()
        };
        Self {
            engine,
            name,
            rate: settings.rate,
            pitch: settings.pitch,
            volume: settings.volume,
            disable_blank_line_pauses: settings.disable_blank_line_pauses,
        }
    }
}

fn check(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Relaxed) {
        Err("__CANCELLED__".into())
    } else {
        Ok(())
    }
}

pub fn render(
    input: &Path,
    output: &Path,
    voice: &Voice,
    runtime: &tokio::runtime::Runtime,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(u32),
) -> Result<(), String> {
    check(cancel)?;
    let text = crate::file_loader::load_any_file(input).map_err(|e| e.to_string())?;
    let text = crate::prepare_text_for_tts(&text, voice.disable_blank_line_pauses);
    if text.trim().is_empty() {
        return Err("Document contains no readable text".into());
    }
    let system = crate::is_system_voice_engine(&voice.engine);
    let chunks = if system {
        crate::edge_tts::merge_short_tts_chunks(
            crate::edge_tts::split_text_lazy(&text),
            240,
            crate::edge_tts::EDGE_TTS_MAX_BYTES,
        )
    } else {
        crate::edge_tts::split_text_edge_audiobook(&text, &voice.name)
    };
    if chunks.is_empty() {
        return Err("Document contains no speakable text".into());
    }
    let cache = if system {
        None
    } else {
        match crate::edge_tts_support::AudiobookCache::open(
            &crate::app_storage_path("edge-audiobook-jobs"),
            &chunks,
            &voice.name,
            voice.rate,
            voice.pitch,
            voice.volume,
        ) {
            Ok(cache) => Some(cache),
            Err(e) => {
                crate::append_podcast_log(&format!("batch_audiobooks.cache: {e}"));
                None
            }
        }
    };
    let source = output.with_file_name("speech.mp3");
    let mut writer =
        std::io::BufWriter::new(std::fs::File::create(&source).map_err(|e| e.to_string())?);
    let mut duration = 0.0;
    for (index, chunk) in chunks.iter().enumerate() {
        check(cancel)?;
        let audio = if let Some(audio) = cache.as_ref().and_then(|c| c.load(index)) {
            audio
        } else {
            let audio = if system {
                crate::synthesize_system_voice_to_mp3(
                    chunk,
                    &voice.name,
                    voice.rate,
                    voice.pitch,
                    voice.volume,
                )?
            } else {
                runtime
                    .block_on(crate::edge_tts_support::synthesize_audiobook_chunk(
                        chunk,
                        &voice.name,
                        voice.rate,
                        voice.pitch,
                        voice.volume,
                        index,
                        || !cancel.load(Ordering::Relaxed),
                    ))
                    .map_err(|e| e.to_string())?
                    .ok_or("__CANCELLED__")?
            };
            if let Some(cache) = &cache
                && let Err(e) = cache.store(index, &audio)
            {
                crate::append_podcast_log(&format!("batch_audiobooks.cache_write: {e}"));
            }
            audio
        };
        check(cancel)?;
        let decoder = rodio::Decoder::new(Cursor::new(audio.clone())).map_err(|e| e.to_string())?;
        let rate = decoder.sample_rate() as f64 * decoder.channels() as f64;
        let mut samples = 0_u64;
        for _ in decoder {
            samples += 1;
            if samples.is_multiple_of(4096) {
                check(cancel)?;
            }
        }
        if samples == 0 {
            return Err("Empty synthesized audio".into());
        }
        duration += samples as f64 / rate;
        writer.write_all(&audio).map_err(|e| e.to_string())?;
        progress(((index + 1) * 85 / chunks.len()) as u32);
    }
    writer.flush().map_err(|e| e.to_string())?;
    drop(writer);
    check(cancel)?;
    let ext = output.extension().and_then(|e| e.to_str()).unwrap_or("mp3");
    let codec: &[&str] = match ext {
        "mp3" => &["-c:a", "libmp3lame", "-b:a", "192k"],
        "m4b" | "m4a" => &["-c:a", "aac", "-b:a", "192k"],
        "wav" => &["-c:a", "pcm_s16le"],
        _ => return Err("Unsupported audiobook format".into()),
    };
    let mut args = vec![
        "-i".into(),
        source.to_string_lossy().into_owned(),
        "-vn".into(),
    ];
    args.extend(codec.iter().map(|s| s.to_string()));
    args.push(output.to_string_lossy().into_owned());
    let ffmpeg = crate::ffmpeg_executable_path().unwrap_or_else(|| PathBuf::from("ffmpeg"));
    crate::media_join_engine::run(&ffmpeg, &args, duration, cancel, &mut |p| {
        progress(85 + (p * 14.0) as u32)
    })?;
    check(cancel)?;
    // Retain the cache until the batch publisher has safely created the final file;
    // it is content-addressed and can also resume the ordinary audiobook action.
    Ok(())
}

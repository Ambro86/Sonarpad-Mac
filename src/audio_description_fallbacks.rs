//! Recovery paths entered only after the ordinary audio-description attempt fails.
//! The ordinary Gemini/Pyannote/TTS/silence scheduler remains in the parent module.

use super::*;
use sha2::{Digest, Sha256};

pub(super) const RECOVERY_DECLINED: &str = "audio_description_recovery_declined";

pub(super) fn no_usable_descriptions(error: &str) -> bool {
    matches!(
        error,
        "Gemini non ha restituito descrizioni."
            | "Nessuna descrizione può essere inserita in sicurezza."
    )
}

pub(super) fn source_modified_ns(path: &Path) -> Option<u64> {
    fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|value| u64::try_from(value.as_nanos()).ok())
}

fn same_path(left: &Path, right: &Path) -> bool {
    fs::canonicalize(left).unwrap_or_else(|_| left.to_path_buf())
        == fs::canonicalize(right).unwrap_or_else(|_| right.to_path_buf())
}

// Bind the persistent copy to the original file AND the selected soundtrack.
// A different source revision or audio track must never reuse a previous copy.
fn recoded_source_path(job: &CreateJob) -> Result<PathBuf, String> {
    let canonical = fs::canonicalize(&job.input_path).map_err(|error| error.to_string())?;
    let metadata = fs::metadata(&canonical).map_err(|error| error.to_string())?;
    let modified = source_modified_ns(&canonical)
        .ok_or_else(|| "Impossibile verificare la versione del video sorgente.".to_string())?;
    let identity = serde_json::to_vec(&(
        "sonarpad-mac-timestamp-recovery-v1",
        &canonical,
        metadata.len(),
        modified,
        job.audio_stream_index,
    ))
    .map_err(|error| error.to_string())?;
    let signature = hex::encode(Sha256::digest(identity));
    let stem = job
        .input_path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("video");
    Ok(job
        .input_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(format!("{stem}_recoded_{}.mp4", &signature[..16])))
}

fn source_is_recoded(path: &Path) -> bool {
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    if stem.to_ascii_lowercase().ends_with("_recoded") {
        return true;
    }
    stem.rsplit_once("_recoded_").is_some_and(|(_, signature)| {
        signature.len() == 16 && signature.bytes().all(|value| value.is_ascii_hexdigit())
    })
}

fn validate_recoded_source(path: &Path, needs_audio: bool) -> Result<(), String> {
    if fs::metadata(path).map_err(|error| error.to_string())?.len() == 0 {
        return Err("La normalizzazione dei timestamp ha creato un file vuoto.".to_string());
    }
    let output = std::process::Command::new(ffprobe_path())
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=codec_type",
            "-of",
            "json",
        ])
        .arg(path)
        .output()
        .map_err(|error| error.to_string())?;
    let metadata: Value =
        serde_json::from_slice(&output.stdout).map_err(|error| error.to_string())?;
    let has_video = metadata
        .get("streams")
        .and_then(Value::as_array)
        .is_some_and(|streams| {
            streams
                .iter()
                .any(|stream| stream.get("codec_type").and_then(Value::as_str) == Some("video"))
        });
    if !output.status.success() || !has_video {
        return Err(
            "La normalizzazione dei timestamp non ha prodotto un video valido.".to_string(),
        );
    }
    let probe = probe_media(path)?;
    if !probe.duration_sec.is_finite()
        || probe.duration_sec <= 0.0
        || (needs_audio && !probe.has_audio)
    {
        return Err(
            "La copia normalizzata è priva della durata o della traccia audio richiesta."
                .to_string(),
        );
    }
    Ok(())
}

fn recode_source(job: &CreateJob, cancel: &Arc<AtomicBool>) -> Result<PathBuf, String> {
    let final_path = recoded_source_path(job)?;
    let tracks = list_audio_tracks(&job.input_path)?;
    let selected = if let Some(index) = job.audio_stream_index {
        Some(
            tracks
                .iter()
                .find(|track| track.stream_index == index)
                .ok_or_else(|| "La traccia audio scelta non è più disponibile.".to_string())?
                .stream_index,
        )
    } else {
        tracks
            .iter()
            .find(|track| track.is_default)
            .or_else(|| tracks.first())
            .map(|track| track.stream_index)
    };
    if cancel.load(Ordering::Relaxed) {
        return Err("cancelled".to_string());
    }
    if final_path.is_file() && validate_recoded_source(&final_path, selected.is_some()).is_ok() {
        append_podcast_log(&format!(
            "audio_description.timestamp_recode_reused path={} stream={selected:?}",
            final_path.display()
        ));
        return Ok(final_path);
    }
    let temporary = temporary_sibling_path(&final_path, "recode");
    let result = (|| {
        let mut errors = Vec::new();
        // The Mac equivalent of the Windows hardware-encoder fallback is
        // VideoToolbox, followed by a portable CPU encoder if it is unavailable.
        for backend in ["h264_videotoolbox", "libx264"] {
            if cancel.load(Ordering::Relaxed) {
                return Err("cancelled".to_string());
            }
            let _ = fs::remove_file(&temporary);
            let mut args = vec![
                "-y".into(),
                "-hide_banner".into(),
                "-loglevel".into(),
                "error".into(),
                "-fflags".into(),
                "+genpts".into(),
                "-i".into(),
                job.input_path.to_string_lossy().to_string(),
                "-map".into(),
                "0:v:0".into(),
                "-sn".into(),
                "-dn".into(),
                "-map_metadata".into(),
                "-1".into(),
                "-pix_fmt".into(),
                "yuv420p".into(),
                "-vf".into(),
                "setpts=PTS-STARTPTS".into(),
                "-c:v".into(),
                backend.into(),
            ];
            if backend == "libx264" {
                args.extend([
                    "-preset".into(),
                    "superfast".into(),
                    "-crf".into(),
                    "18".into(),
                ]);
            } else {
                args.extend(["-b:v".into(), "6000k".into()]);
            }
            if let Some(index) = selected {
                args.extend([
                    "-map".into(),
                    format!("0:{index}"),
                    "-c:a".into(),
                    "aac".into(),
                    "-b:a".into(),
                    "192k".into(),
                    "-af".into(),
                    "aresample=async=1:first_pts=0".into(),
                ]);
            } else {
                args.push("-an".into());
            }
            args.extend([
                "-movflags".into(),
                "+faststart".into(),
                "-avoid_negative_ts".into(),
                "make_zero".into(),
                temporary.to_string_lossy().to_string(),
            ]);
            append_podcast_log(&format!(
                "audio_description.timestamp_recode_attempt backend={backend} input={} selected_audio_stream={selected:?}",
                job.input_path.display()
            ));
            match run_ffmpeg(&args, cancel)
                .and_then(|()| validate_recoded_source(&temporary, selected.is_some()))
            {
                Ok(()) => {
                    if cancel.load(Ordering::Relaxed) {
                        return Err("cancelled".to_string());
                    }
                    fs::rename(&temporary, &final_path).map_err(|error| {
                        format!("Salvataggio della copia normalizzata fallito: {error}")
                    })?;
                    return Ok(final_path.clone());
                }
                Err(error) if error == "cancelled" => return Err(error),
                Err(error) => {
                    let lower = error.to_ascii_lowercase();
                    if lower.contains("permission denied")
                        || lower.contains("operation not permitted")
                        || lower.contains("no space left")
                        || lower.contains("read-only file system")
                    {
                        return Err(error);
                    }
                    errors.push(format!("{backend}: {error}"));
                }
            }
        }
        Err(format!(
            "Normalizzazione dei timestamp non riuscita: {}",
            errors.join(" | ")
        ))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

pub(super) fn create_with_timestamp_recovery(
    job: &CreateJob,
    rt: &Runtime,
    cancel: Arc<AtomicBool>,
    state: Arc<Mutex<ProgressState>>,
) -> Result<JobOutcome, String> {
    let normal = create_audio_description_attempt(job, rt, cancel.clone(), state.clone());
    if !matches!(&normal, Err(error) if error == "Timeline dei chunk Gemini non valida.")
        || !job.fixed_reanalysis_slots.is_empty()
        || source_is_recoded(&job.input_path)
        || cancel.load(Ordering::Relaxed)
    {
        return normal;
    }
    state.lock().unwrap().status = tr("audio_description.progress.timestamp_recode_start");
    let recoded = recode_source(job, &cancel)?;
    if cancel.load(Ordering::Relaxed) {
        return Err("cancelled".to_string());
    }
    let mut retry_job = job.clone();
    retry_job.input_path = recoded;
    retry_job.audio_stream_index = None;
    // One clean restart only. Never recurse through this wrapper or carry a
    // checkpoint belonging to the original source into the normalized timeline.
    // Keep an explicitly selected checkpoint destination. The attempt rejects
    // its old contents using source_path/audio_stream_index before any reuse.
    state.lock().unwrap().status = tr("audio_description.progress.timestamp_recode_done");
    append_podcast_log(&format!(
        "audio_description.timestamp_recode_restart path={}",
        retry_job.input_path.display()
    ));
    create_audio_description_attempt(&retry_job, rt, cancel, state)
}

pub(super) fn load_overlap_checkpoint(
    job: &CreateJob,
) -> Result<(CreateJob, AudioDescriptionPartialCheckpoint), String> {
    let path = job
        .resume_checkpoint_path
        .clone()
        .unwrap_or_else(|| partial_checkpoint_path(&job.output_path));
    let unavailable = || tr("audio_description.dialogue_overlap_unavailable");
    let checkpoint = load_partial_checkpoint(&path).map_err(|_| unavailable())?;
    let mut effective = job.clone();
    if !same_path(&checkpoint.source_path, &job.input_path) {
        let recoded = recoded_source_path(job).map_err(|_| unavailable())?;
        if !same_path(&checkpoint.source_path, &recoded) || checkpoint.audio_stream_index.is_some()
        {
            return Err(unavailable());
        }
        effective.input_path = recoded;
        effective.audio_stream_index = None;
    }
    if checkpoint.audio_stream_index != effective.audio_stream_index
        || !same_path(&checkpoint.output_mp3_path, &effective.output_path)
        || checkpoint.language_code != effective.language_code
        || checkpoint.verbosity != "short"
        || effective.verbosity != Verbosity::Brief
        || checkpoint.completed_chunks != checkpoint.total_chunks
        || checkpoint.source_modified_ns.is_none()
        || !checkpoint.source_duration_sec.is_finite()
        || checkpoint.source_duration_sec <= 0.0
        || checkpoint.descriptions.is_empty()
        || !checkpoint
            .descriptions
            .iter()
            .any(|description| crate::edge_tts::is_edge_text_usable(&description.text))
        || checkpoint.descriptions.iter().any(|description| {
            !description.start_sec.is_finite()
                || description
                    .visual_start_sec
                    .is_some_and(|value| !value.is_finite())
                || description
                    .visual_evidence_time_sec
                    .is_some_and(|value| !value.is_finite())
        })
    {
        return Err(unavailable());
    }
    effective.resume_checkpoint_path = Some(path);
    Ok((effective, checkpoint))
}

pub(super) fn schedule_dialogue_overlap(
    items: &[SynthesizedDescription],
    duration: f64,
) -> (Vec<ScheduledDescription>, Vec<DroppedDescription>) {
    let mut ordered = items.to_vec();
    ordered.sort_by(|left, right| left.visual_start_sec.total_cmp(&right.visual_start_sec));
    let mut scheduled = Vec::new();
    let mut dropped = Vec::new();
    let mut cursor = 0.0_f64;
    for item in ordered {
        let required = item.duration_sec;
        let visual = item.visual_start_sec.max(0.0).min(duration);
        let latest = (duration - required).max(0.0);
        let start = visual.min(latest).max(cursor);
        if !duration.is_finite()
            || duration <= 0.0
            || !required.is_finite()
            || required <= 0.0
            || required > duration
            || start > latest + f64::EPSILON
            || (start - visual).abs() > MAX_SHIFT_SEC + f64::EPSILON
        {
            dropped.push(DroppedDescription {
                original_index: item.original_index, text: item.text, desired_start_sec: visual,
                mandatory: false, slot_id: String::new(), duration_sec: required,
                reason: "dialogue-overlap fallback cannot preserve visual timing without overlapping another description".into(),
            });
            continue;
        }
        cursor = start + required;
        scheduled.push(ScheduledDescription {
            original_index: item.original_index,
            text: item.text,
            desired_start_sec: visual,
            visual_evidence_time_sec: item.visual_evidence_time_sec,
            mandatory: false,
            slot_id: String::new(),
            slot_start_sec: None,
            slot_end_sec: None,
            start_sec: start,
            pcm: item.pcm,
            duration_sec: required,
            extended_pause: false,
        });
    }
    (scheduled, dropped)
}

/// Called only by the explicit final-consent branch in the progress UI wrapper.
/// It reuses the completed Brief checkpoint and never calls Gemini again.
pub(super) fn create_dialogue_overlap(
    job: &CreateJob,
    rt: &Runtime,
    cancel: Arc<AtomicBool>,
    state: Arc<Mutex<ProgressState>>,
) -> Result<JobOutcome, String> {
    if cancel.load(Ordering::Relaxed) {
        return Err("cancelled".into());
    }
    let (job, checkpoint) = load_overlap_checkpoint(job)?;
    let work = cache_dir("dialogue_overlap")?;
    let mut temporary_files = Vec::new();
    let result = (|| {
        state.lock().unwrap().status = tr("audio_description.dialogue_overlap_retrying");
        let source_wav = work.join("source.wav");
        let probe = decode_source_audio(
            &job.input_path,
            &source_wav,
            job.audio_stream_index,
            &cancel,
        )?;
        if (probe.duration_sec - checkpoint.source_duration_sec).abs() > 0.5 {
            return Err(tr("audio_description.dialogue_overlap_unavailable"));
        }
        let tasks = checkpoint
            .descriptions
            .iter()
            .enumerate()
            .map(|(index, description)| AudioDescriptionSynthesisTask {
                original_index: index,
                synthesis_index: index,
                text: description.text.clone(),
                desired_start_sec: description.start_sec,
                visual_start_sec: description
                    .visual_start_sec
                    .unwrap_or(description.start_sec),
                visual_evidence_time_sec: description.visual_evidence_time_sec,
                mandatory: false,
                slot_id: String::new(),
                slot_start_sec: None,
                slot_end_sec: None,
            })
            .collect::<Vec<_>>();
        let synthesized = synthesize_description_tasks_parallel(
            &tasks,
            TtsParameters {
                engine: &job.tts_engine,
                voice: &job.tts_voice,
                rate: job.rate,
                pitch: job.pitch,
                volume: job.volume,
            },
            rt,
            &work,
            cancel.clone(),
            |completed, total| {
                state.lock().unwrap().progress =
                    55 + ((completed as i32 * 25) / total.max(1) as i32);
            },
        )?;
        let (scheduled, dropped) =
            schedule_dialogue_overlap(&synthesized, checkpoint.source_duration_sec);
        if scheduled.is_empty() {
            return Err(tr("audio_description.dialogue_overlap_unavailable"));
        }
        let mixed_wav = work.join("mix.wav");
        let output_duration = render_mix(&source_wav, &mixed_wav, &scheduled, &cancel)?;
        let mixed_mp3 = work.join("final.mp3");
        state.lock().unwrap().status = tr("audio_description.progress.export");
        encode_mp3(&mixed_wav, &mixed_mp3, &cancel)?;
        let staged = temporary_sibling_path(&job.output_path, "dialogue_overlap");
        temporary_files.push(staged.clone());
        let exported = export_audio_description_media(
            &job.input_path,
            &mixed_mp3,
            &staged,
            job.create_video_output,
            &cancel,
        )?;
        temporary_files.push(exported.clone());
        if fs::metadata(&exported)
            .map_err(|error| error.to_string())?
            .len()
            == 0
        {
            return Err("Il fallback finale ha prodotto un file vuoto.".into());
        }
        let final_output = if exported.extension() != staged.extension() {
            audio_description_mkv_fallback_path(&job.output_path)
        } else {
            job.output_path.clone()
        };
        if cancel.load(Ordering::Relaxed) {
            return Err("cancelled".into());
        }
        let project_file = if job.save_project {
            let path = project_path(&final_output);
            let temporary_project = temporary_sibling_path(&path, "dialogue_overlap");
            temporary_files.push(temporary_project.clone());
            let analysis = AudioDescriptionBridgeResult {
                ok: true,
                cancelled: false,
                error: String::new(),
                log_path: String::new(),
                duration_sec: checkpoint.source_duration_sec,
                chunk_duration_sec: CHUNK_SECONDS as u32,
                analysis_engine: "completed-brief-checkpoint".into(),
                protected_intervals: Vec::new(),
                descriptions: checkpoint.descriptions.clone(),
                character_glossary: checkpoint.character_glossary.clone(),
                gemini_model: checkpoint.gemini_model.clone(),
            };
            let mut project_job = job.clone();
            project_job.output_path = final_output.clone();
            let project = build_project(
                &project_job,
                &analysis,
                &scheduled,
                &dropped,
                output_duration,
            );
            save_project(&temporary_project, &project)?;
            if cancel.load(Ordering::Relaxed) {
                return Err("cancelled".into());
            }
            commit_project_pair(&exported, &final_output, &temporary_project, &path)?;
            Some(path)
        } else {
            fs::rename(&exported, &final_output).map_err(|error| error.to_string())?;
            None
        };
        let catalog_path = if job.keep_character_catalog {
            job.catalog
                .as_ref()
                .map(|catalog| save_catalog(catalog, &checkpoint.character_glossary))
                .transpose()?
        } else {
            None
        };
        append_podcast_log(&format!(
            "audio_description.dialogue_overlap_completed inserted={} dropped={} explicit_consent=true",
            scheduled.len(),
            dropped.len()
        ));
        if let Some(path) = job.resume_checkpoint_path.as_ref() {
            let _ = fs::remove_file(path);
        }
        Ok(JobOutcome {
            output_path: final_output,
            project_path: project_file,
            catalog_path,
            generated: checkpoint.descriptions.len(),
            inserted: scheduled.len(),
            extended: 0,
            dropped: dropped.len(),
            dropped_mandatory: 0,
        })
    })();
    let _ = fs::remove_dir_all(&work);
    for path in temporary_files {
        let _ = fs::remove_file(path);
    }
    result
}

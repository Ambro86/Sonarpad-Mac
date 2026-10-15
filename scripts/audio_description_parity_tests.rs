fn chunks(bounds: &[f64]) -> Vec<AudioDescriptionPreparedChunk> {
    bounds.windows(2).map(|p| AudioDescriptionPreparedChunk {
        path: "prepared.mkv".into(), start_sec: p[0], end_sec: p[1],
    }).collect()
}

fn progress() -> AudioDescriptionAnalysisProgress {
    AudioDescriptionAnalysisProgress {
        layout: AudioDescriptionChunkLayout::from_chunks(AudioDescriptionChunkMode::Normal,
            &chunks(&[0.0, 180.0, 360.0])),
        resume: AudioDescriptionBridgeResume {
            completed_chunks: 1,
            descriptions: vec![serde_json::from_value(serde_json::json!({
                "start_sec": 100.0, "end_sec": 105.0, "text": "A door opens.",
                "visual_evidence_time_sec": 102.0,
            })).unwrap()],
            character_glossary: vec![], verified_nonverbal_ids: vec!["NV-1000".into()],
        },
    }
}

#[test]
fn ordinary_resume_keeps_all_work_and_metadata() {
    let resumed = progress().resume_on(&chunks(&[0.0, 180.0, 360.0])).unwrap();
    assert_eq!(resumed.completed_chunks, 1);
    assert_eq!(resumed.descriptions[0].visual_evidence_time_sec, Some(102.0));
    assert_eq!(resumed.verified_nonverbal_ids, ["NV-1000"]);
}

#[test]
fn compatibility_resume_remaps_completed_index_without_losing_work() {
    let resumed = progress().resume_on(&chunks(&[0.0, 60.0, 120.0, 180.0, 240.0, 360.0])).unwrap();
    assert_eq!(resumed.completed_chunks, 3);
    assert_eq!(resumed.descriptions.len(), 1);
}

#[test]
fn incompatible_boundary_and_start_are_rejected() {
    assert!(progress().resume_on(&chunks(&[0.0, 90.0, 200.0, 360.0])).is_err());
    assert!(progress().resume_on(&chunks(&[1.0, 180.0, 360.0])).is_err());
    let mut bad = progress();
    bad.resume.completed_chunks = 3;
    assert!(bad.resume_on(&chunks(&[0.0, 180.0, 360.0])).is_err());
}

fn legacy_checkpoint() -> serde_json::Value {
    serde_json::json!({
        "format":"sonarpad-audio-description-partial", "version":1,
        "source_path":"film.mkv", "output_mp3_path":"film.mp3", "source_file_size":1000,
        "source_duration_sec":360.0, "language_code":"it", "verbosity":"standard",
        "allow_extended_pauses":true, "recognize_characters":true,
        "save_project":true, "keep_character_catalog":true,
        "tts_engine":"system", "tts_voice":"voice", "rate":0, "pitch":0, "volume":100,
        "gemini_model":"test", "character_catalog":null, "completed_chunks":1, "total_chunks":2
    })
}

#[test]
fn legacy_checkpoint_preserves_normal_defaults_and_refuses_unknown_layout() {
    let checkpoint: AudioDescriptionPartialCheckpoint = serde_json::from_value(legacy_checkpoint()).unwrap();
    assert!(!checkpoint.allow_nonverbal_vocals);
    assert_eq!(checkpoint.source_volume, 1.0);
    assert_eq!(checkpoint.resume_for_chunks(&chunks(&[0.0, 180.0, 360.0])).unwrap().completed_chunks, 1);
    assert!(checkpoint.resume_for_chunks(&chunks(&[0.0, 90.0, 180.0, 360.0])).is_err());
}

#[test]
fn checkpoint_round_trip_keeps_compatibility_mode_and_vocal_verifications() {
    let mut checkpoint: AudioDescriptionPartialCheckpoint = serde_json::from_value(legacy_checkpoint()).unwrap();
    checkpoint.chunk_layout = Some(AudioDescriptionChunkLayout::from_chunks(
        AudioDescriptionChunkMode::Mp4, &chunks(&[0.0, 180.0, 360.0])));
    checkpoint.allow_nonverbal_vocals = true;
    checkpoint.source_volume = 0.35;
    checkpoint.verified_nonverbal_ids = vec!["NV-1000".into()];
    let restored: AudioDescriptionPartialCheckpoint = serde_json::from_slice(&serde_json::to_vec(&checkpoint).unwrap()).unwrap();
    assert_eq!(restored.chunk_layout.as_ref().unwrap().mode, AudioDescriptionChunkMode::Mp4);
    assert_eq!(restored.source_volume, 0.35);
    assert!(restored.allow_nonverbal_vocals);
    assert_eq!(restored.resume_for_chunks(&chunks(&[0.0, 60.0, 120.0, 180.0, 360.0])).unwrap().completed_chunks, 3);
    assert_eq!(restored.verified_nonverbal_ids, ["NV-1000"]);
}

#[test]
fn actual_mix_adjusts_only_movie_volume_and_preserves_ducking() {
    let work = tempfile::tempdir().unwrap();
    let source = work.path().join("source.wav");
    let spec = WavSpec { channels: MIX_CHANNELS, sample_rate: MIX_SAMPLE_RATE,
        bits_per_sample: 16, sample_format: SampleFormat::Int };
    let mut writer = WavWriter::create(&source, spec).unwrap();
    for _ in 0..MIX_SAMPLE_RATE * 8 { writer.write_sample(4000i16).unwrap(); }
    writer.finalize().unwrap();
    let cue = ScheduledDescription {
        original_index: 0, text: "test".into(), desired_start_sec: 1.0,
        visual_evidence_time_sec: None, mandatory: false, slot_id: String::new(),
        slot_start_sec: None, slot_end_sec: None, start_sec: 1.0,
        pcm: vec![1000i16; MIX_SAMPLE_RATE as usize * 2].into(),
        duration_sec: 1.0, extended_pause: false,
    };
    let cancel = Arc::new(AtomicBool::new(false));
    for gain in [0.0, 0.5, 1.0, 2.0] {
        let output = work.path().join(format!("out-{gain}.wav"));
        assert_eq!(render_mix(&source, &output, &[cue.clone()], gain, &cancel).unwrap(), 4.0);
        let samples: Vec<i16> = WavReader::open(output).unwrap().samples().map(Result::unwrap).collect();
        assert_eq!(samples[0], (4000.0 * gain) as i16);
        let expected = (4000.0 * gain * 10f32.powf(DUCKING_DB / 20.0) + 1000.0) as i16;
        assert!((samples[MIX_SAMPLE_RATE as usize * 3] - expected).abs() <= 1);
    }
}

#[test]
fn source_gain_normalization_keeps_mute_and_clamps_bad_values() {
    for (value, expected) in [(0.0, 0.0), (-1.0, 0.0), (3.0, 2.0), (f32::NAN, 1.0), (f32::INFINITY, 1.0)] {
        assert_eq!(normalize_source_volume(value), expected);
    }
}

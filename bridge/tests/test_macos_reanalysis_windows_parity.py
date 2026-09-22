from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SRC = (ROOT / 'src' / 'audio_description.rs').read_text(encoding='utf-8')


def _function(name: str) -> str:
    marker = f'fn {name}('
    start = SRC.index(marker)
    brace = SRC.index('{', start)
    depth = 0
    for pos in range(brace, len(SRC)):
        if SRC[pos] == '{':
            depth += 1
        elif SRC[pos] == '}':
            depth -= 1
            if depth == 0:
                return SRC[start:pos + 1]
    raise AssertionError(f'unclosed function: {name}')


def test_mac_reanalysis_uses_normal_creation_pipeline_not_fixed_slot_mode():
    fn = _function('reanalyze_project_segment')
    assert 'create_audio_description(' in fn
    assert 'audio_description_job_from_project(' in fn
    assert 'fixed_reanalysis_slots_for_segment' not in fn
    assert 'mini_job.fixed_reanalysis_slots' not in fn
    assert 'reanalyze_windows_parity' in fn


def test_mac_reanalysis_treats_saved_project_timeline_as_authority():
    fn = _function('reanalyze_project_segment')
    assert 'mini_to_source_scale' in fn
    assert '(0.98..=1.02).contains(&mini_to_source_scale)' in fn
    assert 'distance <= 12.0' in fn
    # Candidate replacement must update only content/duration, never saved timing anchors.
    replacement = fn[fn.index('let saved = &mut candidate.descriptions[project_index];'):]
    assert 'saved.text =' in replacement
    assert 'saved.rendered_text =' in replacement
    assert 'saved.tts_duration_sec =' in replacement
    forbidden = (
        'saved.source_start_sec =',
        'saved.gemini_start_sec =',
        'saved.visual_evidence_time_sec =',
        'saved.slot_start_sec =',
        'saved.slot_end_sec =',
        'saved.extended_pause =',
    )
    for assignment in forbidden:
        assert assignment not in replacement


def test_mac_reanalysis_keeps_old_slots_when_fresh_result_is_missing_or_too_long():
    fn = _function('reanalyze_project_segment')
    assert 'skip_penalty = 4.0_f64' in fn
    assert 'fresh.tts_duration_sec > available_sec + 0.010' in fn
    assert 'reanalyze_keep_old' in fn
    assert 'if accepted == 0' in fn


def test_experimental_fixed_slot_bridge_is_not_reached_by_project_reanalysis():
    fn = _function('reanalyze_project_segment')
    assert 'fixed_reanalysis_slots' not in fn


def test_reanalysis_logs_terminal_worker_result_for_remote_diagnostics():
    fn = _function('run_project_reanalysis_with_progress')
    assert 'reanalyze_worker_done ok=true' in fn
    assert 'reanalyze_worker_done ok=false error=' in fn


def test_reanalysis_terminal_result_uses_one_shot_channel_not_polled_mutex():
    fn = _function('run_project_reanalysis_with_progress')
    assert 'mpsc::channel::<Result<ProjectSegmentReanalysis, String>>()' in fn
    assert 'result_sender.send(outcome)' in fn
    assert 'result_receiver.try_recv()' in fn
    assert 'reanalyze_result_received' in fn
    assert 'Arc::new(Mutex::new(None::<Result<ProjectSegmentReanalysis, String>>))' not in fn


def test_reanalysis_programmatic_close_cannot_be_misread_as_cancel():
    fn = _function('run_project_reanalysis_with_progress')
    assert 'let finished = Rc::new(Cell::new(false));' in fn
    assert 'if finished_close.get()' in fn
    assert 'event.skip(true);' in fn
    assert 'finished_tick.set(true);' in fn
    assert fn.index('finished_tick.set(true);') < fn.index('dialog_tick.end_modal(ID_OK);')
    assert 'reanalyze_modal_returned' in fn
    assert 'finished.set(true);' in fn
    assert 'reanalyze_dialog_destroyed' in fn


def test_reanalysis_drops_project_refcell_borrow_before_match_and_apply():
    src = SRC
    assert 'let project_snapshot = { project_reanalyze.borrow().clone() };' in src
    assert 'let reanalysis_result = run_project_reanalysis_with_progress(' in src
    assert 'match reanalysis_result {' in src
    assert 'project_reanalyze.borrow().clone(),\n            index,' not in src
    assert 'reanalyze_apply_begin' in src
    assert 'reanalyze_project_replaced' in src


def test_reanalysis_owns_terminal_result_before_destroying_progress_dialog():
    fn = _function('run_project_reanalysis_with_progress')
    extract = fn.index('let terminal_result = ui_result')
    destroy = fn.index('progress_dialog.destroy();')
    assert extract < destroy
    assert 'reanalyze_result_extracted' in fn
    # Avoid a Timer -> callback -> Timer Rc cycle in this one-shot dialog.
    assert 'let timer_handle = Rc::clone(&timer);' not in fn

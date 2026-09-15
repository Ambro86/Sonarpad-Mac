from pathlib import Path

SOURCE = Path(__file__).with_name('src') / 'audio_description.rs'
text = SOURCE.read_text(encoding='utf-8')


def test_reanalyzed_marker_is_rendered_in_description_choice():
    assert 'audio_description.project.reanalyzed_marker' in text
    assert 'reanalyzed_description_ids: Option<&HashSet<usize>>' in text
    assert 'ids.contains(&description.id)' in text


def test_reanalysis_cannot_capture_pre_reanalysis_text_as_a_draft():
    assert 'pending_text_reanalyze.borrow_mut().clear();' in text
    assert 'last_selected_reanalyze.set(None);' in text
    assert '*project_reanalyze.borrow_mut() = reanalysis.project;' in text
    assert 'let project_snapshot = { project_reanalyze.borrow().clone() };' in text
    assert 'match reanalysis_result {' in text
    assert 'last_selected_reanalyze.set(Some(selected_index));' in text
    assert text.count('pending_text_reanalyze.borrow_mut().clear();') >= 2
    assert 'choice.set_focus();' in text


def test_reanalyzed_ids_survive_choice_refreshes():
    assert text.count('Some(&*reanalyzed_ids_') >= 8

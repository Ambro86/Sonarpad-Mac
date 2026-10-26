use super::*;
use crate::document_deck::DocumentDeck;

pub const ID_NEW_DOCUMENT: i32 = 2012;
pub const ID_CLOSE_DOCUMENT: i32 = 2013;
pub const ID_NEXT_DOCUMENT: i32 = 2014;
pub const ID_PREVIOUS_DOCUMENT: i32 = 2015;

#[derive(Clone, Default)]
struct Document {
    text: String,
    state: CurrentDocumentState,
    selection: (i64, i64),
    modified: bool,
}

thread_local! {
    static DOCUMENTS: RefCell<DocumentDeck<Document>> = RefCell::new(DocumentDeck::default());
}

fn snapshot(text: &TextCtrl, state: &Arc<Mutex<CurrentDocumentState>>) -> Document {
    Document {
        text: text.get_value(),
        state: state.lock().unwrap().clone(),
        selection: text.get_selection(),
        modified: text.is_modified(),
    }
}

fn checkpoint(text: &TextCtrl, state: &Arc<Mutex<CurrentDocumentState>>) {
    let document = snapshot(text, state);
    DOCUMENTS.with(|documents| documents.borrow_mut().replace_current(document));
}

pub fn update_title(frame: &Frame, state: &Arc<Mutex<CurrentDocumentState>>) {
    let state = state.lock().unwrap();
    let title = state
        .opened_path
        .as_ref()
        .and_then(|path| path.file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| current_ui_strings().document_untitled.clone());
    let title = format!("{title} — Sonarpad");
    if frame.get_title() != title {
        frame.set_title(&title);
    }
}

fn restore(frame: &Frame, text: &TextCtrl, state: &Arc<Mutex<CurrentDocumentState>>) {
    let document = DOCUMENTS.with(|documents| {
        let documents = documents.borrow();
        documents.items[documents.active].clone()
    });
    text.set_value(&document.text);
    *state.lock().unwrap() = document.state;
    text.set_selection(document.selection.0, document.selection.1);
    text.set_modified(document.modified);
    update_title(frame, state);
    text.set_focus();
}

fn same_file(left: &Path, right: &Path) -> bool {
    if left == right {
        return true;
    }
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

pub fn activate_existing(
    frame: &Frame,
    text: &TextCtrl,
    state: &Arc<Mutex<CurrentDocumentState>>,
    path: &Path,
) -> bool {
    checkpoint(text, state);
    let (found, changed) = DOCUMENTS.with(|documents| {
        let mut documents = documents.borrow_mut();
        let index = documents.items.iter().position(|document| {
            document
                .state
                .opened_path
                .as_ref()
                .is_some_and(|open| same_file(open, path))
        });
        if let Some(index) = index {
            let changed = documents.active != index;
            documents.active = index;
            (true, changed)
        } else {
            (false, false)
        }
    });
    if changed {
        restore(frame, text, state);
    }
    found
}

/// Call only after a new document has loaded successfully, before replacing the editor.
pub fn begin_new(text: &TextCtrl, state: &Arc<Mutex<CurrentDocumentState>>) {
    checkpoint(text, state);
    DOCUMENTS.with(|documents| {
        let mut documents = documents.borrow_mut();
        let current = &documents.items[documents.active];
        if current.state.opened_path.is_some() || !current.text.is_empty() || current.modified {
            documents.append(Document::default());
        }
    });
}

pub fn confirm_all(
    frame: &Frame,
    settings: &Arc<Mutex<Settings>>,
    text: &TextCtrl,
    state: &Arc<Mutex<CurrentDocumentState>>,
    prefix: &str,
) -> bool {
    checkpoint(text, state);
    let (original, count) = DOCUMENTS.with(|documents| {
        let documents = documents.borrow();
        (documents.active, documents.items.len())
    });
    for index in 0..count {
        let modified = DOCUMENTS.with(|documents| documents.borrow().items[index].modified);
        if !modified {
            continue;
        }
        DOCUMENTS.with(|documents| documents.borrow_mut().active = index);
        restore(frame, text, state);
        if !confirm_unsaved_changes_before_close(frame, settings, text, state, prefix) {
            return false;
        }
        remember_text_bookmark(settings, text, state);
        checkpoint(text, state);
    }
    DOCUMENTS.with(|documents| documents.borrow_mut().active = original);
    restore(frame, text, state);
    true
}

#[allow(clippy::too_many_arguments)]
pub fn command(
    id: i32,
    frame: &Frame,
    settings: &Arc<Mutex<Settings>>,
    text: &TextCtrl,
    state: &Arc<Mutex<CurrentDocumentState>>,
    playback: &Arc<Mutex<GlobalPlayback>>,
    podcast: &Rc<RefCell<PodcastPlaybackState>>,
    cursor: &Rc<Cell<bool>>,
    article: &Rc<RefCell<Option<CurrentArticleState>>>,
) {
    let (before, count) = DOCUMENTS.with(|documents| {
        let documents = documents.borrow();
        (documents.active, documents.items.len())
    });
    append_podcast_log(&format!(
        "document.command.begin id={id} active={} count={count} modified={}",
        before + 1, text.is_modified(),
    ));
    if matches!(id, ID_NEXT_DOCUMENT | ID_PREVIOUS_DOCUMENT)
        && count < 2
    {
        append_podcast_log("document.command.skipped reason=only_one_document");
        return;
    }
    if id == ID_CLOSE_DOCUMENT
        && !confirm_unsaved_changes_before_close(frame, settings, text, state, "document.close")
    {
        append_podcast_log("document.command.skipped reason=close_cancelled");
        return;
    }
    remember_text_bookmark(settings, text, state);
    checkpoint(text, state);
    stop_tts_playback(playback);
    stop_podcast_playback(podcast);
    podcast.borrow_mut().selected_episode = None;
    *article.borrow_mut() = None;
    match id {
        ID_NEW_DOCUMENT => {
            begin_new(text, state);
            DOCUMENTS.with(|documents| documents.borrow_mut().replace_current(Document::default()));
        }
        ID_CLOSE_DOCUMENT => DOCUMENTS.with(|documents| documents.borrow_mut().remove_current()),
        ID_NEXT_DOCUMENT => DOCUMENTS.with(|documents| documents.borrow_mut().step(false)),
        ID_PREVIOUS_DOCUMENT => DOCUMENTS.with(|documents| documents.borrow_mut().step(true)),
        _ => return,
    }
    restore(frame, text, state);
    cursor.set(true);
    DOCUMENTS.with(|documents| {
        let documents = documents.borrow();
        append_podcast_log(&format!(
            "document.command.completed id={id} before={} active={} count={} changed={} selection={:?}",
            before + 1, documents.active + 1, documents.items.len(),
            before != documents.active, text.get_selection(),
        ));
    });
    let title = state
        .lock()
        .unwrap()
        .opened_path
        .as_ref()
        .and_then(|path| path.file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| current_ui_strings().document_untitled.clone());
    announce_voiceover_message(&title);
}

use super::*;
use crate::document_deck::DocumentDeck;

pub const ID_NEW_DOCUMENT: i32 = 2012;
pub const ID_CLOSE_DOCUMENT: i32 = 2013;
pub const ID_NEXT_DOCUMENT: i32 = 2014;
pub const ID_PREVIOUS_DOCUMENT: i32 = 2015;

#[derive(Clone, Default)]
struct Document {
    untitled_number: usize,
    text: String,
    state: CurrentDocumentState,
    selection: (i64, i64),
    modified: bool,
}

thread_local! {
    static DOCUMENTS: RefCell<DocumentDeck<Document>> = RefCell::new(DocumentDeck::default());
    static NEXT_UNTITLED_NUMBER: Cell<usize> = const { Cell::new(1) };
    static NO_OPEN_DOCUMENT_WINDOW: Cell<bool> = const { Cell::new(false) };
}

pub fn has_no_open_document_window() -> bool {
    NO_OPEN_DOCUMENT_WINDOW.with(Cell::get)
}

fn snapshot(text: &TextCtrl, state: &Arc<Mutex<CurrentDocumentState>>) -> Document {
    Document {
        untitled_number: DOCUMENTS.with(|documents| {
            let documents = documents.borrow();
            documents.items[documents.active].untitled_number
        }),
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

fn document_title(state: &CurrentDocumentState) -> String {
    state
        .opened_path
        .as_ref()
        .and_then(|path| path.file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| {
            let number = DOCUMENTS.with(|documents| {
                let mut documents = documents.borrow_mut();
                let active = documents.active;
                let document = &mut documents.items[active];
                if document.untitled_number == 0 {
                    document.untitled_number = NEXT_UNTITLED_NUMBER.with(|next| {
                        let number = next.get();
                        next.set(number + 1);
                        number
                    });
                }
                document.untitled_number
            });
            format!("{} {number}", current_ui_strings().document_untitled)
        })
}

pub fn update_title(frame: &Frame, state: &Arc<Mutex<CurrentDocumentState>>) {
    if has_no_open_document_window() {
        if frame.get_title() != "Sonarpad" {
            frame.set_title("Sonarpad");
        }
        return;
    }
    let title = document_title(&state.lock().unwrap());
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
    if has_no_open_document_window() {
        NO_OPEN_DOCUMENT_WINDOW.with(|closed| closed.set(false));
        DOCUMENTS.with(|documents| {
            documents.borrow_mut().replace_current(Document::default());
        });
        return;
    }
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
        before + 1,
        text.is_modified(),
    ));
    if has_no_open_document_window() && id != ID_NEW_DOCUMENT {
        append_podcast_log("document.command.skipped reason=no_open_window");
        return;
    }
    if matches!(id, ID_NEXT_DOCUMENT | ID_PREVIOUS_DOCUMENT) && count < 2 {
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
    let reopen_window = id == ID_NEW_DOCUMENT && has_no_open_document_window();
    if id == ID_NEW_DOCUMENT && !reopen_window {
        let _ = document_title(&state.lock().unwrap());
    }
    if !reopen_window {
        checkpoint(text, state);
    }
    stop_tts_playback(playback);
    stop_podcast_playback(podcast);
    podcast.borrow_mut().selected_episode = None;
    *article.borrow_mut() = None;
    match id {
        ID_NEW_DOCUMENT => {
            if reopen_window {
                NO_OPEN_DOCUMENT_WINDOW.with(|closed| closed.set(false));
                DOCUMENTS.with(|documents| {
                    documents.borrow_mut().replace_current(Document::default());
                });
            } else {
                DOCUMENTS.with(|documents| documents.borrow_mut().append(Document::default()));
            }
        }
        ID_CLOSE_DOCUMENT if count == 1 => {
            // Keep a blank internal backing document, but no visible editor window.
            // Unlike Quit, Cmd+W must leave the macOS application running.
            DOCUMENTS.with(|documents| {
                documents.borrow_mut().replace_current(Document::default());
            });
            *state.lock().unwrap() = CurrentDocumentState::default();
            text.set_value("");
            text.set_modified(false);
            NO_OPEN_DOCUMENT_WINDOW.with(|closed| closed.set(true));
            frame.set_title("Sonarpad");
            frame.show(false);
            append_podcast_log("document.command.last_closed window_hidden");
            return;
        }
        ID_CLOSE_DOCUMENT => DOCUMENTS.with(|documents| documents.borrow_mut().remove_current()),
        ID_NEXT_DOCUMENT => DOCUMENTS.with(|documents| documents.borrow_mut().step(false)),
        ID_PREVIOUS_DOCUMENT => DOCUMENTS.with(|documents| documents.borrow_mut().step(true)),
        _ => return,
    }
    if reopen_window {
        frame.show(true);
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
    let title = document_title(&state.lock().unwrap());
    announce_voiceover_message(&title);
}

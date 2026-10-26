//! Runs the production document controller with an in-memory editor on any OS.
#![allow(dead_code)]
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    path::{Path, PathBuf},
    rc::Rc,
    sync::{Arc, Mutex},
};
#[path = "../src/document_deck.rs"]
mod document_deck;
#[path = "../src/documents.rs"]
mod documents;

#[derive(Clone, Default)]
struct CurrentDocumentState {
    opened_path: Option<PathBuf>,
    direct_save_path: Option<PathBuf>,
    epub_toc: Vec<String>,
}
#[derive(Default)]
struct TextCtrl {
    text: RefCell<String>,
    selection: Cell<(i64, i64)>,
    modified: Cell<bool>,
}
impl TextCtrl {
    fn get_value(&self) -> String {
        self.text.borrow().clone()
    }
    fn set_value(&self, value: &str) {
        *self.text.borrow_mut() = value.into();
        self.modified.set(false);
    }
    fn get_selection(&self) -> (i64, i64) {
        self.selection.get()
    }
    fn set_selection(&self, start: i64, end: i64) {
        self.selection.set((start, end));
    }
    fn is_modified(&self) -> bool {
        self.modified.get()
    }
    fn set_modified(&self, value: bool) {
        self.modified.set(value);
    }
    fn set_focus(&self) {}
}
#[derive(Default)]
struct Frame(RefCell<String>);
impl Frame {
    fn get_title(&self) -> String {
        self.0.borrow().clone()
    }
    fn set_title(&self, title: &str) {
        *self.0.borrow_mut() = title.into();
    }
}
struct Settings;
struct GlobalPlayback;
#[derive(Default)]
struct PodcastPlaybackState {
    selected_episode: Option<String>,
}
struct CurrentArticleState;
struct UiStrings {
    document_untitled: String,
}
fn current_ui_strings() -> UiStrings {
    UiStrings {
        document_untitled: "Untitled".into(),
    }
}
fn announce_voiceover_message(_: &str) {}
fn append_podcast_log(_: &str) {}
fn stop_tts_playback(_: &Arc<Mutex<GlobalPlayback>>) {}
fn stop_podcast_playback(_: &Rc<RefCell<PodcastPlaybackState>>) {}
fn remember_text_bookmark(
    _: &Arc<Mutex<Settings>>,
    _: &TextCtrl,
    _: &Arc<Mutex<CurrentDocumentState>>,
) {
}
thread_local! {
    static RESPONSES: RefCell<VecDeque<bool>> = const { RefCell::new(VecDeque::new()) };
    static PROMPTS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}
fn confirm_unsaved_changes_before_close(
    _: &Frame,
    _: &Arc<Mutex<Settings>>,
    text: &TextCtrl,
    state: &Arc<Mutex<CurrentDocumentState>>,
    _: &str,
) -> bool {
    if !text.is_modified() {
        return true;
    }
    PROMPTS.with(|p| {
        p.borrow_mut().push(
            state
                .lock()
                .unwrap()
                .opened_path
                .as_ref()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
        )
    });
    let accepted = RESPONSES.with(|r| r.borrow_mut().pop_front().expect("unexpected save prompt"));
    if accepted {
        text.set_modified(false);
    }
    accepted
}
#[derive(Default)]
struct Session {
    frame: Frame,
    text: TextCtrl,
    state: Arc<Mutex<CurrentDocumentState>>,
}
impl Session {
    fn open(&self, name: &str, text: &str) {
        if documents::activate_existing(&self.frame, &self.text, &self.state, Path::new(name)) {
            return;
        }
        documents::begin_new(&self.text, &self.state);
        self.text.set_value(text);
        *self.state.lock().unwrap() = CurrentDocumentState {
            opened_path: Some(name.into()),
            direct_save_path: Some(name.into()),
            epub_toc: vec![format!("toc:{name}")],
        };
    }
    fn command(&self, id: i32) {
        documents::command(
            id,
            &self.frame,
            &Arc::new(Mutex::new(Settings)),
            &self.text,
            &self.state,
            &Arc::new(Mutex::new(GlobalPlayback)),
            &Rc::new(RefCell::new(PodcastPlaybackState::default())),
            &Rc::new(Cell::new(false)),
            &Rc::new(RefCell::new(None)),
        );
    }
    fn quit(&self) -> bool {
        documents::confirm_all(
            &self.frame,
            &Arc::new(Mutex::new(Settings)),
            &self.text,
            &self.state,
            "test",
        )
    }
}

#[test]
fn reopening_a_file_preserves_unsaved_text_selection_and_toc() {
    let s = Session::default();
    s.open("first.txt", "original");
    s.text.set_value("unsaved");
    s.text.set_modified(true);
    s.text.set_selection(2, 5);
    s.open("second.txt", "second");
    s.open("first.txt", "must not reload");
    assert_eq!(s.text.get_value(), "unsaved");
    assert_eq!(s.text.get_selection(), (2, 5));
    assert!(s.text.is_modified());
    assert_eq!(s.state.lock().unwrap().epub_toc, ["toc:first.txt"]);
    s.command(documents::ID_NEXT_DOCUMENT);
    assert_eq!(s.text.get_value(), "second");
    assert_eq!(
        s.state.lock().unwrap().direct_save_path,
        Some("second.txt".into())
    );
}

#[test]
fn cancelling_quit_keeps_the_pending_document_and_does_not_prompt_approved_docs_again() {
    let s = Session::default();
    s.open("first.txt", "one");
    s.text.set_modified(true);
    s.open("second.txt", "two");
    s.text.set_modified(true);
    s.open("third.txt", "three");
    RESPONSES.with(|r| *r.borrow_mut() = [true, false].into());
    assert!(!s.quit());
    assert_eq!(s.text.get_value(), "two");
    assert!(s.text.is_modified());
    RESPONSES.with(|r| *r.borrow_mut() = [true].into());
    assert!(s.quit());
    PROMPTS.with(|p| assert_eq!(*p.borrow(), ["first.txt", "second.txt", "second.txt"]));
}

#[test]
fn cancelling_close_keeps_document_and_closing_only_removes_selected_document() {
    let s = Session::default();
    s.open("first.txt", "one");
    s.open("second.txt", "two");
    s.text.set_modified(true);
    RESPONSES.with(|r| *r.borrow_mut() = [false, true].into());
    s.command(documents::ID_CLOSE_DOCUMENT);
    assert_eq!(s.text.get_value(), "two");
    s.command(documents::ID_CLOSE_DOCUMENT);
    assert_eq!(s.text.get_value(), "one");
    s.command(documents::ID_CLOSE_DOCUMENT);
    assert_eq!(s.text.get_value(), "");
    assert!(s.state.lock().unwrap().opened_path.is_none());
}

#[test]
fn new_document_does_not_discard_empty_but_modified_file() {
    let s = Session::default();
    s.open("empty.txt", "");
    s.text.set_modified(true);
    s.command(documents::ID_NEW_DOCUMENT);
    assert!(s.state.lock().unwrap().opened_path.is_none());
    s.command(documents::ID_PREVIOUS_DOCUMENT);
    assert_eq!(
        s.state.lock().unwrap().opened_path,
        Some("empty.txt".into())
    );
    assert!(s.text.is_modified());
}

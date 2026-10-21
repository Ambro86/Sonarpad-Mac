use crate::sonarmusic::{self as music, Client, Page, Request, TYPES};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::{Arc, Mutex, mpsc};
use wxdragon::prelude::*;
use wxdragon::timer::Timer;
#[derive(Clone, Default)]
struct View {
    request: Option<Request>,
    page: Page,
    number: usize,
    selection: usize,
}
enum Reply {
    Page(Request, Page, bool),
    Suggestions(Vec<String>),
}
#[derive(Clone, Copy)]
struct Controls {
    dialog: Dialog,
    panel: Panel,
    query: TextCtrl,
    kind: Choice,
    results: Choice,
    status: StaticText,
    search: Button,
    suggest: Button,
    open: Button,
    more: Button,
    back: Button,
}
impl Controls {
    fn update(self, busy: bool, view: &View, suggestions: &Option<Vec<String>>, history: bool) {
        self.query.enable(!busy);
        self.kind.enable(!busy);
        self.search.enable(!busy);
        self.suggest.enable(!busy);
        self.results.enable(!busy);
        let item = view
            .page
            .items
            .get(self.results.get_selection().unwrap_or(0) as usize);
        self.open.enable(
            !busy
                && (suggestions.as_ref().is_some_and(|s| !s.is_empty())
                    || item.is_some_and(|i| i.playable() || i.browse_request().is_some())),
        );
        self.more
            .enable(!busy && suggestions.is_none() && view.page.next.is_some());
        self.back
            .enable(!busy && (suggestions.is_some() || history));
    }
    fn render(self, view: &View, locale: &str) {
        self.results.clear();
        for item in &view.page.items {
            self.results.append(&item.display(locale));
        }
        if !view.page.items.is_empty() {
            self.results
                .set_selection(view.selection.min(view.page.items.len() - 1) as u32);
        }
    }
}
pub(crate) fn open(
    parent: &Frame,
    settings: &Arc<Mutex<crate::Settings>>,
    audio: &crate::AudioDescriptionLaunchContext,
) {
    let locale = crate::normalize_ui_language(&settings.lock().unwrap().ui_language);
    let text = |key| music::label(&locale, key);
    let client = match Client::new(&locale, crate::SONARPAD_ROUTE_CLIENT_TOKEN) {
        Ok(c) => c,
        Err(e) => {
            crate::show_message_dialog(parent, "SonarMusic", &e);
            return;
        }
    };
    let dialog = Dialog::builder(parent, "SonarMusic")
        .with_size(780, 450)
        .with_style(DialogStyle::DefaultDialogStyle | DialogStyle::ResizeBorder)
        .build();
    let panel = Panel::builder(&dialog).build();
    let root = BoxSizer::builder(Orientation::Vertical).build();
    root.add(
        &StaticText::builder(&panel)
            .with_label(&text("query"))
            .build(),
        0,
        SizerFlag::All,
        5,
    );
    let query = TextCtrl::builder(&panel)
        .with_style(TextCtrlStyle::ProcessEnter)
        .build();
    query.set_accessibility_label(&text("query"));
    root.add(&query, 0, SizerFlag::Expand | SizerFlag::All, 5);
    let row = BoxSizer::builder(Orientation::Horizontal).build();
    let search = Button::builder(&panel).with_label(&text("search")).build();
    let suggest = Button::builder(&panel).with_label(&text("suggest")).build();
    row.add(&search, 0, SizerFlag::All, 5);
    row.add(&suggest, 0, SizerFlag::All, 5);
    root.add_sizer(&row, 0, SizerFlag::Expand, 0);
    root.add(
        &StaticText::builder(&panel)
            .with_label(&text("type"))
            .build(),
        0,
        SizerFlag::All,
        5,
    );
    let kind = Choice::builder(&panel).build();
    kind.set_accessibility_label(&text("type"));
    for t in TYPES {
        kind.append(&text(t));
    }
    kind.set_selection(0);
    root.add(&kind, 0, SizerFlag::Expand | SizerFlag::All, 5);
    root.add(
        &StaticText::builder(&panel)
            .with_label(&text("results"))
            .build(),
        0,
        SizerFlag::All,
        5,
    );
    let results = Choice::builder(&panel).build();
    results.set_accessibility_label(&text("results"));
    root.add(&results, 0, SizerFlag::Expand | SizerFlag::All, 5);
    let status = StaticText::builder(&panel).build();
    root.add(&status, 0, SizerFlag::Expand | SizerFlag::All, 5);
    let row = BoxSizer::builder(Orientation::Horizontal).build();
    let open = Button::builder(&panel).with_label(&text("open")).build();
    let more = Button::builder(&panel).with_label(&text("more")).build();
    let back = Button::builder(&panel).with_label(&text("back")).build();
    let close = Button::builder(&panel)
        .with_id(ID_CANCEL)
        .with_label(&text("close"))
        .build();
    for button in [open, more, back, close] {
        row.add(&button, 0, SizerFlag::All, 5);
    }
    root.add_sizer(&row, 0, SizerFlag::Expand, 0);
    panel.set_sizer(root, true);
    let layout = BoxSizer::builder(Orientation::Vertical).build();
    layout.add(&panel, 1, SizerFlag::Expand, 0);
    dialog.set_sizer(layout, true);
    suggest.show(false);
    let c = Controls {
        dialog,
        panel,
        query,
        kind,
        results,
        status,
        search,
        suggest,
        open,
        more,
        back,
    };
    let view = Rc::new(RefCell::new(View::default()));
    let history = Rc::new(RefCell::new(Vec::<View>::new()));
    let suggestions = Rc::new(RefCell::new(None::<Vec<String>>));
    let busy = Rc::new(Cell::new(false));
    let (tx, rx) = mpsc::channel::<Result<Reply, String>>();
    let visible = Cell::new(false);
    query.on_text_updated(move |_| {
        let show = !query.get_value().trim().is_empty();
        if show != visible.get() {
            visible.set(show);
            suggest.show(show);
            c.panel.layout();
            c.dialog.layout();
        }
    });
    let run = {
        let tx = tx.clone();
        let client = client.clone();
        let view = view.clone();
        let suggestions = suggestions.clone();
        let history = history.clone();
        let busy = busy.clone();
        let locale = locale.clone();
        Rc::new(move |request: Request, more: bool| {
            if busy.replace(true) {
                return;
            }
            let token = if more {
                view.borrow().page.next.clone()
            } else {
                None
            };
            let number = if more { view.borrow().number + 1 } else { 1 };
            c.status.set_label(&music::label(&locale, "loading"));
            c.update(
                true,
                &view.borrow(),
                &suggestions.borrow(),
                !history.borrow().is_empty(),
            );
            let tx = tx.clone();
            let client = client.clone();
            std::thread::spawn(move || {
                let _ = tx.send(
                    client
                        .load(&request, token.as_deref(), number)
                        .map(|p| Reply::Page(request, p, more)),
                );
            });
        })
    };
    let do_search = {
        let run = run.clone();
        let history = history.clone();
        Rc::new(move || {
            if let Ok(request) = Request::search(
                &query.get_value(),
                TYPES[kind.get_selection().unwrap_or(0) as usize],
            ) {
                history.borrow_mut().clear();
                run(request, false);
            }
        })
    };
    let f = do_search.clone();
    search.on_click(move |_| f());
    query.on_text_enter(move |_| do_search());
    let b = busy.clone();
    let v = view.clone();
    let h = history.clone();
    let sg = suggestions.clone();
    let lang = locale.clone();
    suggest.on_click(move |_| {
        let query = query.get_value();
        if query.trim().is_empty() || b.replace(true) {
            return;
        }
        c.update(true, &v.borrow(), &sg.borrow(), !h.borrow().is_empty());
        status.set_label(&music::label(&lang, "loading"));
        let tx = tx.clone();
        let client = client.clone();
        std::thread::spawn(move || {
            let _ = tx.send(client.suggestions(&query).map(Reply::Suggestions));
        });
    });
    let run_more = run.clone();
    let v = view.clone();
    more.on_click(move |_| {
        if let Some(r) = v.borrow().request.clone() {
            run_more(r, true);
        }
    });
    let v = view.clone();
    let h = history.clone();
    let sg = suggestions.clone();
    let lang = locale.clone();
    back.on_click(move |_| {
        if sg.borrow_mut().take().is_none()
            && let Some(previous) = h.borrow_mut().pop()
        {
            *v.borrow_mut() = previous;
        }
        c.render(&v.borrow(), &lang);
        c.update(false, &v.borrow(), &sg.borrow(), !h.borrow().is_empty());
        results.set_focus();
    });
    let v = view.clone();
    let h = history.clone();
    let sg = suggestions.clone();
    let lang = locale.clone();
    let settings = Arc::clone(settings);
    let audio = audio.clone();
    open.on_click(move |_| {
        let index = results.get_selection().unwrap_or(0) as usize;
        let suggestion = sg.borrow().as_ref().and_then(|s| s.get(index)).cloned();
        if let Some(value) = suggestion {
            query.set_value(&value);
            sg.borrow_mut().take();
            c.render(&v.borrow(), &lang);
            c.update(false, &v.borrow(), &sg.borrow(), !h.borrow().is_empty());
            query.set_focus();
            return;
        }
        let item = v.borrow().page.items.get(index).cloned();
        let Some(item) = item else {
            return;
        };
        if let Some(url) = item.watch_url() {
            crate::open_youtube_results_dialog_with_title(
                &dialog,
                &settings,
                vec![crate::YoutubeSearchResult {
                    title: item.title,
                    url,
                    is_collection: false,
                    duration: (!item.duration.is_empty()).then_some(item.duration),
                }],
                None,
                &audio,
                false,
                "SonarMusic",
            );
            results.set_focus();
        } else if let Some(request) = item.browse_request() {
            let mut previous = v.borrow().clone();
            previous.selection = index;
            h.borrow_mut().push(previous);
            run(request, false);
        }
    });
    let v = view.clone();
    let h = history.clone();
    let sg = suggestions.clone();
    let b = busy.clone();
    results.on_selection_changed(move |_| {
        c.update(b.get(), &v.borrow(), &sg.borrow(), !h.borrow().is_empty())
    });
    let timer = Timer::new(&dialog);
    let v = view.clone();
    let h = history.clone();
    let sg = suggestions.clone();
    let b = busy.clone();
    timer.on_tick(move |_| {
        if let Ok(reply) = rx.try_recv() {
            b.set(false);
            match reply {
                Ok(Reply::Page(request, page, more)) => {
                    sg.borrow_mut().take();
                    let mut v = v.borrow_mut();
                    v.request = Some(request);
                    if more {
                        v.page.append(page);
                        v.number += 1;
                    } else {
                        v.page = page;
                        v.number = 1;
                        v.selection = 0;
                    }
                    c.render(&v, &locale);
                    status.set_label(&if v.page.items.is_empty() {
                        music::label(&locale, "empty")
                    } else {
                        format!(
                            "{}: {}",
                            music::label(&locale, "results"),
                            v.page.items.len()
                        )
                    });
                    results.set_focus();
                }
                Ok(Reply::Suggestions(values)) => {
                    results.clear();
                    for value in &values {
                        results.append(value);
                    }
                    if !values.is_empty() {
                        results.set_selection(0);
                    }
                    status.set_label(&music::label(
                        &locale,
                        if values.is_empty() {
                            "empty"
                        } else {
                            "suggest"
                        },
                    ));
                    *sg.borrow_mut() = Some(values);
                    results.set_focus();
                }
                Err(error) => {
                    crate::append_podcast_log(&error);
                    status.set_label(&error);
                    query.set_focus();
                }
            }
            c.update(false, &v.borrow(), &sg.borrow(), !h.borrow().is_empty());
        }
    });
    close.on_click(move |_| dialog.end_modal(ID_CANCEL));
    c.update(false, &view.borrow(), &None, false);
    query.set_focus();
    timer.start(100, false);
    dialog.show_modal();
    timer.stop();
    dialog.destroy();
}

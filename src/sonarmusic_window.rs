use crate::sonarmusic::{self as music, Client, Page, Pagination, Request, ResultRow, TYPES};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::{Arc, Mutex, mpsc};
use wxdragon::prelude::*;
use wxdragon::timer::Timer;
const ID_BACK: i32 = 31001;
#[derive(Clone, Default)]
struct View {
    request: Option<Request>,
    page: Page,
    number: usize,
    pagination: Pagination,
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
    back: Button,
    favorites: Choice,
    open_favorite: Button,
    remove_favorite: Button,
    add_favorite: Button,
}
impl Controls {
    fn update(self, busy: bool, view: &View, suggestions: &Option<Vec<String>>, history: bool) {
        self.query.enable(!busy);
        self.kind.enable(!busy);
        self.search.enable(!busy);
        self.suggest.enable(!busy);
        self.results.enable(!busy);
        self.favorites.enable(!busy);
        self.open_favorite
            .enable(!busy && self.favorites.get_selection().is_some());
        self.remove_favorite
            .enable(!busy && self.favorites.get_selection().is_some());
        let row = view.pagination.row(
            &view.page,
            self.results.get_selection().unwrap_or(0) as usize,
        );
        let available = match row {
            Some(ResultRow::Previous | ResultRow::Next) => true,
            Some(ResultRow::Item(index)) => view
                .page
                .items
                .get(index)
                .is_some_and(|i| i.playable() || i.browse_request().is_some()),
            None => false,
        };
        self.add_favorite.enable(
            !busy && suggestions.is_none() && matches!(row, Some(ResultRow::Item(_))) && available,
        );
        self.open
            .enable(!busy && (suggestions.as_ref().is_some_and(|s| !s.is_empty()) || available));
        let can_go_back = view.pagination.can_return(suggestions.is_some(), history);
        self.back.enable(!busy && can_go_back);
        self.dialog
            .set_escape_id(if can_go_back { ID_BACK } else { ID_CANCEL });
    }
    fn render(self, view: &View, locale: &str) {
        self.results.clear();
        let rows = view.pagination.labels(&view.page, locale);
        for row in &rows {
            self.results.append(row);
        }
        if !rows.is_empty() {
            self.results
                .set_selection(view.selection.min(rows.len() - 1) as u32);
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
        .with_size(780, 590)
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
    let suggest = Button::builder(&panel).with_label(&text("suggest")).build();
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
    let saved_kind = settings.lock().unwrap().sonarmusic_search_type.clone();
    kind.set_selection(music::search_type_index(&saved_kind) as u32);
    let settings_kind = Arc::clone(settings);
    kind.on_selection_changed(move |_| {
        let index = (kind.get_selection().unwrap_or(0) as usize).min(TYPES.len() - 1);
        let mut settings = settings_kind.lock().unwrap();
        settings.sonarmusic_search_type = TYPES[index].to_owned();
        settings.save();
    });
    root.add(&kind, 0, SizerFlag::Expand | SizerFlag::All, 5);
    let search = Button::builder(&panel).with_label(&text("search")).build();
    root.add(&search, 0, SizerFlag::All, 5);
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
    root.add(
        &StaticText::builder(&panel)
            .with_label(&text("favorites"))
            .build(),
        0,
        SizerFlag::All,
        5,
    );
    let favorites = Choice::builder(&panel).build();
    favorites.set_accessibility_label(&text("favorites"));
    root.add(&favorites, 0, SizerFlag::Expand | SizerFlag::All, 5);
    let favorite_row = BoxSizer::builder(Orientation::Horizontal).build();
    let open_favorite = Button::builder(&panel)
        .with_label(&text("open_favorite"))
        .build();
    let remove_favorite = Button::builder(&panel)
        .with_label(&text("remove_favorite"))
        .build();
    favorite_row.add(&open_favorite, 0, SizerFlag::All, 5);
    favorite_row.add(&remove_favorite, 0, SizerFlag::All, 5);
    root.add_sizer(&favorite_row, 0, SizerFlag::Expand, 0);
    let add_favorite = Button::builder(&panel)
        .with_label(&text("add_favorite"))
        .build();
    root.add(&add_favorite, 0, SizerFlag::All, 5);
    let status = StaticText::builder(&panel).build();
    root.add(&status, 0, SizerFlag::Expand | SizerFlag::All, 5);
    let row = BoxSizer::builder(Orientation::Horizontal).build();
    let open = Button::builder(&panel).with_label(&text("open")).build();
    let back = Button::builder(&panel)
        .with_id(ID_BACK)
        .with_label(&text("back"))
        .build();
    let close = Button::builder(&panel)
        .with_id(ID_CANCEL)
        .with_label(&text("close"))
        .build();
    for button in [open, back, close] {
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
        back,
        favorites,
        open_favorite,
        remove_favorite,
        add_favorite,
    };
    let favorite_settings = Arc::clone(settings);
    let saved_favorites = Rc::new(RefCell::new(music::favorite_items(
        &settings.lock().unwrap().sonarmusic_favorites,
    )));
    let refresh_favorites = {
        let saved = saved_favorites.clone();
        let locale = locale.clone();
        Rc::new(move || {
            let index = favorites.get_selection().unwrap_or(0) as usize;
            favorites.clear();
            let labels = crate::result_labels::accessible_result_labels(
                saved
                    .borrow()
                    .iter()
                    .map(|item| item.display(&locale))
                    .collect(),
            );
            for label in &labels {
                favorites.append(label);
            }
            if !labels.is_empty() {
                favorites.set_selection(index.min(labels.len() - 1) as u32);
            }
        })
    };
    refresh_favorites();
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
    let v = view.clone();
    let h = history.clone();
    let sg = suggestions.clone();
    let lang = locale.clone();
    back.on_click(move |_| {
        if sg.borrow_mut().take().is_none() {
            let mut current = v.borrow_mut();
            let View {
                pagination, page, ..
            } = &mut *current;
            if pagination.back(page) {
                current.number = current.number.saturating_sub(1);
                current.selection = 0;
            } else if let Some(previous) = h.borrow_mut().pop() {
                *current = previous;
            }
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
    let opening_busy = busy.clone();
    let activate = Rc::new(move || {
        if opening_busy.get() {
            return;
        }
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
        let row = {
            let current = v.borrow();
            current.pagination.row(&current.page, index)
        };
        let item_index = match row {
            Some(ResultRow::Next) => {
                let request = v.borrow().request.clone();
                if let Some(request) = request {
                    run(request, true);
                }
                return;
            }
            Some(ResultRow::Previous) => {
                {
                    let mut current = v.borrow_mut();
                    let View {
                        pagination, page, ..
                    } = &mut *current;
                    if pagination.back(page) {
                        current.number = current.number.saturating_sub(1);
                        current.selection = 0;
                    }
                }
                c.render(&v.borrow(), &lang);
                c.update(false, &v.borrow(), &sg.borrow(), !h.borrow().is_empty());
                results.set_focus();
                return;
            }
            Some(ResultRow::Item(index)) => index,
            None => return,
        };
        let item = v.borrow().page.items.get(item_index).cloned();
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
    let on_open = activate.clone();
    open.on_click(move |_| on_open());
    let activate_favorite = activate.clone();
    results.on_key_down(move |event| {
        if let WindowEventData::Keyboard(key) = &event
            && key.get_key_code() == Some(13)
        {
            activate();
            return;
        }
        event.skip(true);
    });
    let v = view.clone();
    let h = history.clone();
    let sg = suggestions.clone();
    let b = busy.clone();
    results.on_selection_changed(move |_| {
        c.update(b.get(), &v.borrow(), &sg.borrow(), !h.borrow().is_empty())
    });
    let add_action = {
        let saved = saved_favorites.clone();
        let settings = Arc::clone(&favorite_settings);
        let v = view.clone();
        let sg = suggestions.clone();
        let b = busy.clone();
        let h = history.clone();
        let refresh = refresh_favorites.clone();
        move || {
            if b.get() || sg.borrow().is_some() {
                return;
            }
            let current = v.borrow();
            let Some(ResultRow::Item(index)) = current
                .pagination
                .row(&current.page, results.get_selection().unwrap_or(0) as usize)
            else {
                return;
            };
            let Some(item) = current.page.items.get(index) else {
                return;
            };
            {
                let mut settings = settings.lock().unwrap();
                music::add_favorite(&mut settings.sonarmusic_favorites, item);
                *saved.borrow_mut() = music::favorite_items(&settings.sonarmusic_favorites);
                settings.save();
            }
            drop(current);
            refresh();
            c.update(false, &v.borrow(), &sg.borrow(), !h.borrow().is_empty());
        }
    };
    add_favorite.on_click(move |_| add_action());
    let saved = saved_favorites.clone();
    let settings_f = Arc::clone(&favorite_settings);
    let b = busy.clone();
    let v = view.clone();
    let sg = suggestions.clone();
    let h = history.clone();
    let refresh = refresh_favorites.clone();
    remove_favorite.on_click(move |_| {
        if b.get() {
            return;
        }
        let item = saved
            .borrow()
            .get(favorites.get_selection().unwrap_or(0) as usize)
            .cloned();
        if let Some(item) = item {
            {
                let mut settings = settings_f.lock().unwrap();
                music::remove_favorite(&mut settings.sonarmusic_favorites, &item);
                *saved.borrow_mut() = music::favorite_items(&settings.sonarmusic_favorites);
                settings.save();
            }
            refresh();
            c.update(false, &v.borrow(), &sg.borrow(), !h.borrow().is_empty());
            favorites.set_focus();
        }
    });
    let v = view.clone();
    let h = history.clone();
    let sg = suggestions.clone();
    let b = busy.clone();
    let saved = saved_favorites.clone();
    let lang = locale.clone();
    let open_saved = Rc::new(move || {
        if b.get() {
            return;
        }
        let index = favorites.get_selection().unwrap_or(0) as usize;
        if index >= saved.borrow().len() {
            return;
        }
        let mut previous = v.borrow().clone();
        previous.selection = results.get_selection().unwrap_or(0) as usize;
        h.borrow_mut().push(previous);
        *v.borrow_mut() = View {
            page: Page {
                items: saved.borrow().clone(),
                next: None,
            },
            number: 1,
            selection: index,
            ..View::default()
        };
        sg.borrow_mut().take();
        c.render(&v.borrow(), &lang);
        c.update(false, &v.borrow(), &None, !h.borrow().is_empty());
        activate_favorite();
    });
    let f = open_saved.clone();
    open_favorite.on_click(move |_| f());
    favorites.on_key_down(move |event| {
        if let WindowEventData::Keyboard(key) = &event
            && key.get_key_code() == Some(13)
        {
            open_saved();
            return;
        }
        event.skip(true);
    });
    let timer = Timer::new(&dialog);
    let v = view.clone();
    let h = history.clone();
    let sg = suggestions.clone();
    let b = busy.clone();
    timer.on_tick(move |_| {
        if let Ok(reply) = rx.try_recv() {
            b.set(false);
            let failed = reply.is_err();
            match reply {
                Ok(Reply::Page(request, page, more)) => {
                    sg.borrow_mut().take();
                    let mut v = v.borrow_mut();
                    v.request = Some(request);
                    if more {
                        let View {
                            pagination,
                            page: current,
                            ..
                        } = &mut *v;
                        pagination.advance(current, page);
                        v.number += 1;
                    } else {
                        v.page = page;
                        v.pagination = Pagination::default();
                        v.number = 1;
                        v.selection = 0;
                    }
                    v.selection = 0;
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
                }
                Err(error) => {
                    crate::append_podcast_log(&error);
                    status.set_label(&error);
                }
            }
            c.update(false, &v.borrow(), &sg.borrow(), !h.borrow().is_empty());
            // Native controls must be enabled before VoiceOver can focus them.
            if failed {
                query.set_focus();
            } else {
                results.set_focus();
                crate::append_podcast_log(&format!(
                    "SonarMusic results focused: page={} index={}",
                    v.borrow().number,
                    results.get_selection().unwrap_or(0)
                ));
            }
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

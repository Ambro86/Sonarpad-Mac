use crate::batch_audiobooks_engine::{self as engine, Progress, Status};
use crate::batch_audiobooks_render::{self as renderer, Voice};
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use wxdragon::prelude::*;
use wxdragon::timer::Timer;

fn strings() -> serde_json::Value {
    let lang = crate::normalize_ui_language(&crate::Settings::load().ui_language);
    let source = match lang.as_str() {
        "it" => include_str!("../i18n/ui_it.json"),
        "fr" => include_str!("../i18n/ui_fr.json"),
        "es" => include_str!("../i18n/ui_es.json"),
        "pt" => include_str!("../i18n/ui_pt.json"),
        "cs" => include_str!("../i18n/ui_cs.json"),
        "pl" => include_str!("../i18n/ui_pl.json"),
        _ => include_str!("../i18n/ui_en.json"),
    };
    serde_json::from_str(source).expect("valid UI translations")
}
fn tr(s: &serde_json::Value, key: &str) -> String {
    s[key].as_str().unwrap_or(key).to_owned()
}
pub fn menu_label() -> String {
    tr(&strings(), "batch_menu")
}
fn refresh(choice: Choice, files: &[PathBuf]) {
    choice.clear();
    for (i, file) in files.iter().enumerate() {
        choice.append(&format!("{}. {}", i + 1, file.display()));
    }
    if !files.is_empty() {
        choice.set_selection((files.len() - 1) as u32);
    }
}
fn folder(parent: &Dialog, title: &str) -> Option<PathBuf> {
    let picker = DirDialog::builder(parent, title, "").build();
    crate::set_mac_native_file_dialog_open(true);
    let result = picker.show_modal();
    crate::set_mac_native_file_dialog_open(false);
    if result == ID_OK {
        picker.get_path().map(PathBuf::from)
    } else {
        None
    }
}

pub fn open_dialog(
    parent: &Frame,
    settings: &Arc<Mutex<crate::Settings>>,
    runtime: &Arc<tokio::runtime::Runtime>,
) {
    let labels = Rc::new(strings());
    let title = tr(&labels, "batch_title");
    let dialog = Dialog::builder(parent, &title)
        .with_style(DialogStyle::DefaultDialogStyle | DialogStyle::ResizeBorder)
        .with_size(820, 530)
        .build();
    dialog.set_escape_id(wxdragon::id::ID_NONE);
    let panel = Panel::builder(&dialog).build();
    let progress_panel = Panel::builder(&dialog).build();
    let root = BoxSizer::builder(Orientation::Vertical).build();
    root.add(
        &StaticText::builder(&panel)
            .with_label(&tr(&labels, "batch_input"))
            .build(),
        0,
        SizerFlag::All,
        6,
    );
    let list = Choice::builder(&panel).build();
    list.set_accessibility_label(&tr(&labels, "batch_input"));
    root.add(&list, 0, SizerFlag::Expand | SizerFlag::All, 6);
    let add = Button::builder(&panel)
        .with_label(&tr(&labels, "batch_add_files"))
        .build();
    let add_folder = Button::builder(&panel)
        .with_label(&tr(&labels, "batch_add_folder"))
        .build();
    let row = BoxSizer::builder(Orientation::Horizontal).build();
    row.add(&add, 1, SizerFlag::All, 6);
    row.add(&add_folder, 1, SizerFlag::All, 6);
    root.add_sizer(&row, 0, SizerFlag::Expand, 0);
    let remove = Button::builder(&panel)
        .with_label(&tr(&labels, "batch_remove_selected"))
        .build();
    let clear = Button::builder(&panel)
        .with_label(&tr(&labels, "batch_clear"))
        .build();
    let row = BoxSizer::builder(Orientation::Horizontal).build();
    row.add(&remove, 1, SizerFlag::All, 6);
    row.add(&clear, 1, SizerFlag::All, 6);
    root.add_sizer(&row, 0, SizerFlag::Expand, 0);
    root.add(
        &StaticText::builder(&panel)
            .with_label(&tr(&labels, "batch_output_folder"))
            .build(),
        0,
        SizerFlag::All,
        6,
    );
    let output = TextCtrl::builder(&panel)
        .with_style(TextCtrlStyle::ReadOnly)
        .build();
    output.set_accessibility_label(&tr(&labels, "batch_output_folder"));
    let initial = settings.lock().unwrap().last_audiobook_save_dir.clone();
    output.set_value(&if initial.is_empty() {
        crate::default_audiobook_save_folder()
            .to_string_lossy()
            .into_owned()
    } else {
        initial
    });
    root.add(&output, 0, SizerFlag::Expand | SizerFlag::All, 6);
    let browse = Button::builder(&panel)
        .with_label(&tr(&labels, "batch_browse"))
        .build();
    root.add(&browse, 0, SizerFlag::All, 6);
    root.add(
        &StaticText::builder(&panel)
            .with_label(&tr(&labels, "convert_media_format"))
            .build(),
        0,
        SizerFlag::All,
        6,
    );
    let format = Choice::builder(&panel).build();
    format.set_accessibility_label(&tr(&labels, "convert_media_format"));
    for ext in engine::FORMATS {
        format.append(&ext.to_uppercase());
    }
    format.set_selection(0);
    root.add(&format, 0, SizerFlag::Expand | SizerFlag::All, 6);
    let subfolders = CheckBox::builder(&panel)
        .with_label(&tr(&labels, "batch_option_subfolder"))
        .build();
    root.add(&subfolders, 0, SizerFlag::All, 6);
    let voice = Voice::from_settings(&settings.lock().unwrap());
    let voice_name = if voice.name.is_empty() {
        tr(
            &labels,
            if crate::is_system_voice_engine(&voice.engine) {
                "voice_engine_system"
            } else {
                "voice_engine_microsoft"
            },
        )
    } else {
        voice.name.clone()
    };
    root.add(
        &StaticText::builder(&panel)
            .with_label(&tr(&labels, "batch_voice").replace("{voice}", &voice_name))
            .build(),
        0,
        SizerFlag::All,
        6,
    );
    let start = Button::builder(&panel)
        .with_label(&tr(&labels, "batch_start"))
        .build();
    let close = Button::builder(&panel)
        .with_label(&tr(&labels, "close"))
        .build();
    let row = BoxSizer::builder(Orientation::Horizontal).build();
    row.add(&start, 0, SizerFlag::All, 6);
    row.add(&close, 0, SizerFlag::All, 6);
    root.add_sizer(&row, 0, SizerFlag::Expand, 0);
    panel.set_sizer(root, true);
    let progress_root = BoxSizer::builder(Orientation::Vertical).build();
    let status = TextCtrl::builder(&progress_panel)
        .with_style(TextCtrlStyle::ReadOnly | TextCtrlStyle::MultiLine)
        .build();
    status.set_accessibility_label(&tr(&labels, "batch_status_running"));
    let gauge = Gauge::builder(&progress_panel).with_range(100).build();
    gauge.set_accessibility_label(&tr(&labels, "batch_status_running"));
    let cancel_button = Button::builder(&progress_panel)
        .with_label(&tr(&labels, "batch_cancel"))
        .build();
    progress_root.add(&status, 1, SizerFlag::Expand | SizerFlag::All, 12);
    progress_root.add(&gauge, 0, SizerFlag::Expand | SizerFlag::All, 12);
    progress_root.add(&cancel_button, 0, SizerFlag::All, 12);
    progress_panel.set_sizer(progress_root, true);
    let root = BoxSizer::builder(Orientation::Vertical).build();
    root.add(&panel, 1, SizerFlag::Expand, 0);
    root.add(&progress_panel, 1, SizerFlag::Expand, 0);
    dialog.set_sizer(root, true);
    progress_panel.show(false);
    let files = Rc::new(RefCell::new(Vec::<PathBuf>::new()));
    let busy = Rc::new(Cell::new(false));
    let close_after = Rc::new(Cell::new(false));
    let cancel = Arc::new(AtomicBool::new(false));
    let job = Arc::new(Mutex::new(Progress::new(&[])));
    let timer = Timer::new(&dialog);

    let f = files.clone();
    let l = labels.clone();
    add.on_click(move |_| {
        let wildcard = format!(
            "{}|{}",
            tr(&l, "batch_input"),
            engine::EXTENSIONS
                .iter()
                .map(|e| format!("*.{e}"))
                .collect::<Vec<_>>()
                .join(";")
        );
        let picker = FileDialog::builder(&dialog)
            .with_message(&tr(&l, "batch_add_files"))
            .with_wildcard(&wildcard)
            .with_style(
                FileDialogStyle::Open | FileDialogStyle::FileMustExist | FileDialogStyle::Multiple,
            )
            .build();
        crate::set_mac_native_file_dialog_open(true);
        let result = picker.show_modal();
        crate::set_mac_native_file_dialog_open(false);
        if result != ID_OK {
            return;
        }
        let mut selected: Vec<_> = picker
            .get_paths()
            .into_iter()
            .map(PathBuf::from)
            .filter(|p| p.is_file())
            .collect();
        if selected.is_empty()
            && let Some(path) = crate::resolve_file_dialog_path(&picker, true)
        {
            selected.push(path);
        }
        engine::add_files(&mut f.borrow_mut(), selected);
        refresh(list, &f.borrow());
        list.set_focus();
    });
    let f = files.clone();
    let l = labels.clone();
    add_folder.on_click(move |_| {
        if let Some(path) = folder(&dialog, &tr(&l, "batch_add_folder")) {
            match engine::folder_files(&path) {
                Ok(paths) => engine::add_files(&mut f.borrow_mut(), paths),
                Err(e) => crate::show_message_subdialog(&dialog, &tr(&l, "batch_title"), &e),
            }
            refresh(list, &f.borrow());
            list.set_focus();
        }
    });
    let f = files.clone();
    remove.on_click(move |_| {
        if let Some(i) = list.get_selection() {
            f.borrow_mut().remove(i as usize);
            refresh(list, &f.borrow());
            list.set_focus();
        }
    });
    let f = files.clone();
    clear.on_click(move |_| {
        f.borrow_mut().clear();
        refresh(list, &f.borrow());
        add.set_focus();
    });
    let l = labels.clone();
    browse.on_click(move |_| {
        if let Some(path) = folder(&dialog, &tr(&l, "batch_browse")) {
            output.set_value(&path.to_string_lossy());
            output.set_focus();
        }
    });
    let f = files.clone();
    let l = labels.clone();
    let b = busy.clone();
    let c = cancel.clone();
    let j = job.clone();
    let rt = runtime.clone();
    let settings = settings.clone();
    start.on_click(move |_| {
        if b.get() {
            return;
        }
        if f.borrow().is_empty() {
            crate::show_message_subdialog(
                &dialog,
                &tr(&l, "batch_title"),
                &tr(&l, "batch_error_empty_queue"),
            );
            return;
        }
        if output.get_value().trim().is_empty() {
            crate::show_message_subdialog(
                &dialog,
                &tr(&l, "batch_title"),
                &tr(&l, "batch_error_no_output_folder"),
            );
            return;
        }
        let inputs = f.borrow().clone();
        let destination = PathBuf::from(output.get_value());
        let extension = engine::FORMATS[format.get_selection().unwrap_or(0) as usize];
        let subfolders = subfolders.get_value();
        let voice = Voice::from_settings(&settings.lock().unwrap());
        c.store(false, Ordering::Relaxed);
        *j.lock().unwrap() = Progress::new(&inputs);
        b.set(true);
        gauge.set_value(0);
        status.set_value(&tr(&l, "batch_status_running"));
        panel.show(false);
        progress_panel.show(true);
        cancel_button.enable(true);
        dialog.layout();
        status.set_focus();
        let c = c.clone();
        let j = j.clone();
        let rt = rt.clone();
        std::thread::spawn(move || {
            engine::run(
                &inputs,
                &destination,
                extension,
                subfolders,
                &c,
                |input, output, cancel, progress| {
                    renderer::render(input, output, &voice, &rt, cancel, progress)
                },
                |p| *j.lock().unwrap() = p.clone(),
            );
        });
    });
    let c = cancel.clone();
    let l = labels.clone();
    cancel_button.on_click(move |_| {
        c.store(true, Ordering::Relaxed);
        cancel_button.enable(false);
        status.set_value(&tr(&l, "cancelling_audiobook"));
    });
    let b = busy.clone();
    let j = job.clone();
    let l = labels.clone();
    let closing = close_after.clone();
    let c = cancel.clone();
    let last = Cell::new(None);
    timer.on_tick(move |_| {
        if !b.get() {
            return;
        }
        let p = j.lock().unwrap().clone();
        let completed = p
            .items
            .iter()
            .filter(|item| item.status == Status::Done)
            .count();
        if last.get() != Some((p.current, p.percent, completed)) {
            last.set(Some((p.current, p.percent, completed)));
            gauge.set_value(p.percent as i32);
            let name = p
                .items
                .get(p.current)
                .map(|i| {
                    i.input
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned()
                })
                .unwrap_or_default();
            let message = tr(&l, "batch_progress")
                .replace("{completed}", &completed.to_string())
                .replace("{current}", &(p.current + 1).to_string())
                .replace("{total}", &p.items.len().to_string())
                .replace("{file}", &name)
                .replace("{percent}", &p.percent.to_string());
            if !c.load(Ordering::Relaxed) {
                status.set_value(&message);
            }
            gauge.set_accessibility_label(&message);
        }
        if p.finished {
            b.set(false);
            last.set(None);
            panel.show(true);
            progress_panel.show(false);
            dialog.layout();
            list.clear();
            let mut errors = Vec::new();
            for item in &p.items {
                let key = match item.status {
                    Status::Done => "done",
                    Status::Failed => "failed",
                    Status::Cancelled => "canceled",
                    Status::Running => "running",
                    Status::Pending => "pending",
                };
                let label = format!(
                    "{} — {}{}",
                    item.input.display(),
                    tr(&l, &format!("batch_status_{key}")),
                    item.output
                        .as_ref()
                        .map(|p| format!(" — {}", p.display()))
                        .unwrap_or_default()
                );
                list.append(&label);
                if let Some(error) = &item.error {
                    errors.push(format!("{}: {error}", item.input.display()));
                    crate::append_podcast_log(&format!(
                        "batch_audiobooks.error: {}: {error}",
                        item.input.display()
                    ));
                }
            }
            if !p.items.is_empty() {
                list.set_selection(0);
            }
            if closing.get() {
                dialog.end_modal(ID_CANCEL);
                return;
            }
            let count = |s| p.items.iter().filter(|i| i.status == s).count().to_string();
            let mut summary = tr(&l, "batch_summary")
                .replace("{success}", &count(Status::Done))
                .replace("{failed}", &count(Status::Failed))
                .replace("{cancelled}", &count(Status::Cancelled));
            if !errors.is_empty() {
                summary.push_str("\n\n");
                summary.push_str(&errors.join("\n"));
            }
            crate::show_message_subdialog(&dialog, &tr(&l, "batch_title"), &summary);
            list.set_focus();
        }
    });
    timer.start(150, false);
    close.on_click(move |_| {
        dialog.close(false);
    });
    let b = busy.clone();
    let c = cancel.clone();
    let l = labels.clone();
    dialog.on_close(move |event| {
        if b.get() {
            if crate::ask_yes_no_dialog(
                &dialog,
                &tr(&l, "batch_title"),
                &tr(&l, "batch_close_running"),
            ) {
                if !b.get() {
                    dialog.end_modal(ID_CANCEL);
                } else {
                    close_after.set(true);
                    c.store(true, Ordering::Relaxed);
                    cancel_button.enable(false);
                }
            }
            event.skip(false);
        } else {
            event.skip(true);
        }
    });
    // Escape and Command+W use the same localized running-job confirmation.
    macro_rules! close_keys {
        ($($control:expr),+ $(,)?) => { $(
            $control.on_key_down(move |event| {
                if let WindowEventData::Keyboard(key) = &event {
                    let code = key.get_key_code().unwrap_or_default();
                    if !key.alt_down() && (code == 27 || (key.cmd_down() && code == 87)) {
                        event.skip(false); dialog.close(false); return;
                    }
                }
                event.skip(true);
            });
        )+ };
    }
    close_keys!(
        dialog,
        panel,
        progress_panel,
        list,
        add,
        add_folder,
        remove,
        clear,
        output,
        browse,
        format,
        subfolders,
        start,
        close,
        status,
        gauge,
        cancel_button
    );
    add.set_focus();
    dialog.show_modal();
    timer.stop();
    dialog.destroy();
}

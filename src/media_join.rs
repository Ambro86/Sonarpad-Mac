use crate::media_join_engine;
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
    let text = match lang.as_str() {
        "it" => include_str!("../i18n/ui_it.json"),
        "fr" => include_str!("../i18n/ui_fr.json"),
        "es" => include_str!("../i18n/ui_es.json"),
        "pt" => include_str!("../i18n/ui_pt.json"),
        "cs" => include_str!("../i18n/ui_cs.json"),
        "pl" => include_str!("../i18n/ui_pl.json"),
        _ => include_str!("../i18n/ui_en.json"),
    };
    serde_json::from_str(text).expect("valid bundled UI translations")
}
fn text(strings: &serde_json::Value, key: &str) -> String {
    strings[key].as_str().unwrap_or(key).to_string()
}
pub fn menu_label() -> String {
    text(&strings(), "media_join_menu")
}

fn refresh(choice: Choice, files: &[PathBuf], selected: usize) {
    choice.clear();
    for (i, path) in files.iter().enumerate() {
        choice.append(&format!("{}. {}", i + 1, path.display()));
    }
    if !files.is_empty() {
        choice.set_selection(selected.min(files.len() - 1) as u32);
    }
}

#[derive(Default)]
struct Job {
    percent: Option<u32>,
    result: Option<Result<(), String>>,
}

pub fn open_dialog(parent: &Frame) {
    let strings = Rc::new(strings());
    let title = text(&strings, "media_join_title");
    let dialog = Dialog::builder(parent, &title)
        .with_style(DialogStyle::DefaultDialogStyle | DialogStyle::ResizeBorder)
        .with_size(800, 480)
        .build();
    dialog.set_escape_id(wxdragon::id::ID_NONE);
    let panel = Panel::builder(&dialog).build();
    let progress_panel = Panel::builder(&dialog).build();
    let root = BoxSizer::builder(Orientation::Vertical).build();
    root.add(
        &StaticText::builder(&panel)
            .with_label(&text(&strings, "media_join_input"))
            .build(),
        0,
        SizerFlag::All,
        6,
    );
    let files_choice = Choice::builder(&panel).build();
    files_choice.set_accessibility_label(&text(&strings, "media_join_input"));
    root.add(&files_choice, 0, SizerFlag::Expand | SizerFlag::All, 6);
    let add = Button::builder(&panel)
        .with_label(&text(&strings, "media_join_add"))
        .build();
    root.add(&add, 0, SizerFlag::All, 6);
    let order = BoxSizer::builder(Orientation::Horizontal).build();
    let remove = Button::builder(&panel)
        .with_label(&text(&strings, "media_join_remove"))
        .build();
    let up = Button::builder(&panel)
        .with_label(&text(&strings, "media_join_up"))
        .build();
    let down = Button::builder(&panel)
        .with_label(&text(&strings, "media_join_down"))
        .build();
    for button in [remove, up, down] {
        order.add(&button, 1, SizerFlag::All, 6);
    }
    root.add_sizer(&order, 0, SizerFlag::Expand, 0);
    root.add(
        &StaticText::builder(&panel)
            .with_label(&text(&strings, "media_join_output"))
            .build(),
        0,
        SizerFlag::All,
        6,
    );
    let output = TextCtrl::builder(&panel)
        .with_style(TextCtrlStyle::ReadOnly)
        .build();
    output.set_accessibility_label(&text(&strings, "media_join_output"));
    root.add(&output, 0, SizerFlag::Expand | SizerFlag::All, 6);
    let choose_output = Button::builder(&panel)
        .with_label(&text(&strings, "media_join_choose_output"))
        .build();
    root.add(&choose_output, 0, SizerFlag::All, 6);
    root.add(
        &StaticText::builder(&panel)
            .with_label(&text(&strings, "convert_media_format"))
            .build(),
        0,
        SizerFlag::All,
        6,
    );
    let format = Choice::builder(&panel).build();
    format.set_accessibility_label(&text(&strings, "convert_media_format"));
    for ext in media_join_engine::FORMATS {
        format.append(&ext.to_uppercase());
    }
    format.set_selection(0);
    root.add(&format, 0, SizerFlag::Expand | SizerFlag::All, 6);
    let join = Button::builder(&panel)
        .with_label(&text(&strings, "media_join_button"))
        .build();
    let close = Button::builder(&panel)
        .with_label(&text(&strings, "close"))
        .build();
    let bottom = BoxSizer::builder(Orientation::Horizontal).build();
    bottom.add(&join, 0, SizerFlag::All, 6);
    bottom.add(&close, 0, SizerFlag::All, 6);
    root.add_sizer(&bottom, 0, SizerFlag::Expand, 0);
    panel.set_sizer(root, true);
    let progress_root = BoxSizer::builder(Orientation::Vertical).build();
    let status = StaticText::builder(&progress_panel)
        .with_label(&text(&strings, "media_join_running"))
        .build();
    let gauge = Gauge::builder(&progress_panel).with_range(100).build();
    gauge.set_accessibility_label(&text(&strings, "media_join_running"));
    let cancel_button = Button::builder(&progress_panel)
        .with_label(&text(&strings, "cancel"))
        .build();
    progress_root.add(&status, 0, SizerFlag::Expand | SizerFlag::All, 12);
    progress_root.add(&gauge, 0, SizerFlag::Expand | SizerFlag::All, 12);
    progress_root.add(&cancel_button, 0, SizerFlag::All, 12);
    progress_panel.set_sizer(progress_root, true);
    let dialog_root = BoxSizer::builder(Orientation::Vertical).build();
    dialog_root.add(&panel, 1, SizerFlag::Expand, 0);
    dialog_root.add(&progress_panel, 1, SizerFlag::Expand, 0);
    dialog.set_sizer(dialog_root, true);
    progress_panel.show(false);
    let files = Rc::new(RefCell::new(Vec::<PathBuf>::new()));
    let busy = Rc::new(Cell::new(false));
    let cancel = Arc::new(AtomicBool::new(false));
    let job = Arc::new(Mutex::new(Job::default()));
    let timer = Timer::new(&dialog);

    let f = files.clone();
    let labels = strings.clone();
    add.on_click(move |_| {
        let picker = FileDialog::builder(&dialog).with_message(&text(&labels, "media_join_add"))
            .with_style(FileDialogStyle::Open | FileDialogStyle::FileMustExist | FileDialogStyle::Multiple)
            .with_wildcard("Media|*.mp3;*.m4a;*.mp4;*.mkv;*.avi;*.mov;*.wav;*.flac;*.ogg;*.opus;*.aac;*.aiff;*.webm;*.wma;*.m4b;*.mpg;*.mpeg;*.ts;*.m2ts;*.wmv;*.flv|*.*|*.*").build();
        crate::set_mac_native_file_dialog_open(true);
        let result = picker.show_modal();
        crate::set_mac_native_file_dialog_open(false);
        if result != ID_OK { return; }
        let mut selected: Vec<_> = picker.get_paths().into_iter().map(PathBuf::from).filter(|p| p.is_file()).collect();
        if selected.is_empty() && let Some(path) = crate::resolve_file_dialog_path(&picker, true) { selected.push(path); }
        let mut files = f.borrow_mut(); files.extend(selected);
        refresh(files_choice, &files, files.len().saturating_sub(1));
        if output.get_value().is_empty() && let Some(first) = files.first() {
            let ext = media_join_engine::FORMATS[format.get_selection().unwrap_or(0) as usize];
            output.set_value(&first.with_file_name(format!("{}_{}.{ext}", first.file_stem().unwrap_or_default().to_string_lossy(), text(&labels, "media_join_suffix"))).to_string_lossy());
        }
        files_choice.set_focus();
    });
    for (button, direction) in [(remove, 0i32), (up, -1), (down, 1)] {
        let f = files.clone();
        button.on_click(move |_| {
            let Some(selected) = files_choice.get_selection() else {
                return;
            };
            let mut files = f.borrow_mut();
            let mut index = selected as usize;
            if direction == 0 {
                files.remove(index);
                index = index.saturating_sub(1);
            } else {
                let next = (index as i32 + direction).clamp(0, files.len() as i32 - 1) as usize;
                files.swap(index, next);
                index = next;
            }
            refresh(files_choice, &files, index);
            files_choice.set_focus();
        });
    }
    let labels = strings.clone();
    choose_output.on_click(move |_| {
        let ext = media_join_engine::FORMATS[format.get_selection().unwrap_or(0) as usize];
        let path = PathBuf::from(output.get_value());
        let picker = FileDialog::builder(&dialog)
            .with_message(&text(&labels, "media_join_choose_output"))
            .with_style(FileDialogStyle::Save)
            .with_wildcard(&format!("{}|*.{ext}", ext.to_uppercase()))
            .with_default_file(&path.file_name().unwrap_or_default().to_string_lossy())
            .build();
        crate::set_mac_native_file_dialog_open(true);
        let result = picker.show_modal();
        crate::set_mac_native_file_dialog_open(false);
        if result == ID_OK
            && let Some(mut path) = crate::resolve_file_dialog_path(&picker, false)
        {
            path.set_extension(ext);
            output.set_value(&path.to_string_lossy());
        }
    });
    format.on_selection_changed(move |_| {
        if !output.get_value().is_empty() {
            let mut path = PathBuf::from(output.get_value());
            path.set_extension(
                media_join_engine::FORMATS[format.get_selection().unwrap_or(0) as usize],
            );
            output.set_value(&path.to_string_lossy());
        }
    });
    let f = files.clone();
    let b = busy.clone();
    let c = cancel.clone();
    let j = job.clone();
    let labels = strings.clone();
    join.on_click(move |_| {
        if b.get() {
            return;
        }
        if f.borrow().len() < 2 {
            crate::show_message_subdialog(
                &dialog,
                &text(&labels, "media_join_title"),
                &text(&labels, "media_join_no_input"),
            );
            return;
        }
        if output.get_value().is_empty() {
            choose_output.set_focus();
            return;
        }
        let path = PathBuf::from(output.get_value());
        if path.exists()
            && !crate::ask_yes_no_dialog(
                &dialog,
                &text(&labels, "media_join_title"),
                &text(&labels, "media_join_overwrite"),
            )
        {
            return;
        }
        let inputs = f.borrow().clone();
        c.store(false, Ordering::Relaxed);
        *j.lock().unwrap() = Job::default();
        b.set(true);
        panel.show(false);
        progress_panel.show(true);
        cancel_button.enable(true);
        status.set_label(&text(&labels, "media_join_running"));
        gauge.set_value(0);
        dialog.layout();
        cancel_button.set_focus();
        let c = c.clone();
        let j = j.clone();
        std::thread::spawn(move || {
            let ffmpeg = crate::ffmpeg_executable_path().unwrap_or_else(|| PathBuf::from("ffmpeg"));
            let result = media_join_engine::join(&ffmpeg, &inputs, &path, c, &mut |pct| {
                j.lock().unwrap().percent = Some(pct)
            });
            if let Err(e) = &result {
                crate::append_podcast_log(&format!("media_join.error: {e}"));
            }
            j.lock().unwrap().result = Some(result);
        });
    });
    let c = cancel.clone();
    let labels = strings.clone();
    cancel_button.on_click(move |_| {
        c.store(true, Ordering::Relaxed);
        cancel_button.enable(false);
        status.set_label(&text(&labels, "convert_folder_cancelling"));
    });
    let b = busy.clone();
    let j = job.clone();
    let labels = strings.clone();
    let last = Cell::new(None);
    timer.on_tick(move |_| {
        if !b.get() {
            return;
        }
        let (percent, result) = {
            let mut job = j.lock().unwrap();
            (job.percent, job.result.take())
        };
        if let Some(percent) = percent {
            if last.get() != Some(percent) {
                last.set(Some(percent));
                gauge.set_value(percent as i32);
                let message = format!("{} {percent}%", text(&labels, "media_join_running"));
                status.set_label(&message);
                gauge.set_accessibility_label(&message);
            }
        } else {
            last.set(None);
            gauge.set_value(0);
        }
        if let Some(result) = result {
            b.set(false);
            progress_panel.show(false);
            panel.show(true);
            dialog.layout();
            let message = match result {
                Ok(()) => text(&labels, "media_join_done"),
                Err(e) if e == "__CANCELLED__" => text(&labels, "media_join_cancelled"),
                Err(e) => format!("{}\n{e}", text(&labels, "media_join_failed")),
            };
            crate::show_message_subdialog(&dialog, &text(&labels, "media_join_title"), &message);
            join.set_focus();
        }
    });
    timer.start(150, false);
    let b = busy.clone();
    close.on_click(move |_| {
        if !b.get() {
            dialog.end_modal(ID_CANCEL);
        }
    });
    let b = busy.clone();
    dialog.on_close(move |event| {
        if b.get() {
            cancel_button.set_focus();
            event.skip(false);
        } else {
            event.skip(true);
        }
    });
    add.set_focus();
    dialog.show_modal();
    timer.stop();
    dialog.destroy();
}

use crate::podcast_recording_engine::{self as engine, Config, Control, Event};
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use wxdragon::prelude::*;
use wxdragon::timer::Timer;

fn strings() -> serde_json::Value {
    let lang = crate::normalize_ui_language(&crate::Settings::load().ui_language);
    serde_json::from_str(match lang.as_str() {
        "it" => include_str!("../i18n/ui_it.json"),
        "fr" => include_str!("../i18n/ui_fr.json"),
        "es" => include_str!("../i18n/ui_es.json"),
        "pt" => include_str!("../i18n/ui_pt.json"),
        "cs" => include_str!("../i18n/ui_cs.json"),
        "pl" => include_str!("../i18n/ui_pl.json"),
        _ => include_str!("../i18n/ui_en.json"),
    })
    .expect("valid bundled UI translations")
}
fn tr(s: &serde_json::Value, key: &str) -> String {
    s[key].as_str().unwrap_or(key).into()
}
pub fn menu_label() -> String {
    tr(&strings(), "record_menu")
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Idle,
    Starting,
    Recording,
    Paused,
    Stopping,
    Saving,
}

pub fn open_dialog(parent: &Frame) {
    let labels = Rc::new(strings());
    let dialog = Dialog::builder(parent, &tr(&labels, "record_title"))
        .with_size(720, 640)
        .with_style(DialogStyle::DefaultDialogStyle | DialogStyle::ResizeBorder)
        .build();
    dialog.set_escape_id(wxdragon::id::ID_NONE);
    let panel = Panel::builder(&dialog).build();
    let saving_panel = Panel::builder(&dialog).build();
    let root = BoxSizer::builder(Orientation::Vertical).build();
    let system = CheckBox::builder(&panel)
        .with_label(&tr(&labels, "record_system"))
        .build();
    let mic = CheckBox::builder(&panel)
        .with_label(&tr(&labels, "record_microphone"))
        .build();
    mic.set_value(true);
    system.enable(false);
    root.add(&system, 0, SizerFlag::All, 6);
    root.add(&mic, 0, SizerFlag::All, 6);
    let compatibility = StaticText::builder(&panel)
        .with_label(&tr(&labels, "record_compatibility"))
        .build();
    root.add(&compatibility, 0, SizerFlag::Expand | SizerFlag::All, 6);
    let device_label = StaticText::builder(&panel)
        .with_label(&tr(&labels, "record_device"))
        .build();
    let device = Choice::builder(&panel).build();
    device.set_accessibility_label(&tr(&labels, "record_device"));
    device.append(&tr(&labels, "record_default"));
    device.set_selection(0);
    root.add(&device_label, 0, SizerFlag::All, 6);
    root.add(&device, 0, SizerFlag::Expand | SizerFlag::All, 6);
    let refresh = Button::builder(&panel)
        .with_label(&tr(&labels, "record_refresh"))
        .build();
    refresh.enable(false);
    root.add(&refresh, 0, SizerFlag::All, 6);
    let gains = [
        "0%", "25%", "50%", "75%", "100%", "125%", "150%", "175%", "200%",
    ];
    let mic_label = StaticText::builder(&panel)
        .with_label(&tr(&labels, "record_mic_volume"))
        .build();
    let mic_gain = Choice::builder(&panel).build();
    mic_gain.set_accessibility_label(&tr(&labels, "record_mic_volume"));
    let system_label = StaticText::builder(&panel)
        .with_label(&tr(&labels, "record_system_volume"))
        .build();
    let system_gain = Choice::builder(&panel).build();
    system_gain.set_accessibility_label(&tr(&labels, "record_system_volume"));
    for gain in gains {
        mic_gain.append(gain);
        system_gain.append(gain);
    }
    mic_gain.set_selection(4);
    system_gain.set_selection(4);
    system_gain.enable(false);
    root.add(&mic_label, 0, SizerFlag::All, 6);
    root.add(&mic_gain, 0, SizerFlag::Expand | SizerFlag::All, 6);
    root.add(&system_label, 0, SizerFlag::All, 6);
    root.add(&system_gain, 0, SizerFlag::Expand | SizerFlag::All, 6);
    let output_label = StaticText::builder(&panel)
        .with_label(&tr(&labels, "record_output"))
        .build();
    let output = TextCtrl::builder(&panel)
        .with_style(TextCtrlStyle::ReadOnly)
        .build();
    output.set_accessibility_label(&tr(&labels, "record_output"));
    let choose = Button::builder(&panel)
        .with_label(&tr(&labels, "record_choose_output"))
        .build();
    root.add(&output_label, 0, SizerFlag::All, 6);
    root.add(&output, 0, SizerFlag::Expand | SizerFlag::All, 6);
    root.add(&choose, 0, SizerFlag::All, 6);
    let format_label = StaticText::builder(&panel)
        .with_label(&tr(&labels, "convert_media_format"))
        .build();
    let format = Choice::builder(&panel).build();
    format.set_accessibility_label(&tr(&labels, "convert_media_format"));
    for ext in engine::FORMATS {
        format.append(&ext.to_uppercase());
    }
    format.set_selection(0);
    root.add(&format_label, 0, SizerFlag::All, 6);
    root.add(&format, 0, SizerFlag::Expand | SizerFlag::All, 6);
    let status = TextCtrl::builder(&panel)
        .with_style(TextCtrlStyle::ReadOnly)
        .build();
    status.set_accessibility_label(&tr(&labels, "record_status"));
    status.set_value(&tr(&labels, "record_loading"));
    root.add(&status, 0, SizerFlag::Expand | SizerFlag::All, 6);
    let elapsed = TextCtrl::builder(&panel)
        .with_style(TextCtrlStyle::ReadOnly)
        .build();
    elapsed.set_accessibility_label(&tr(&labels, "record_elapsed"));
    elapsed.set_value("00:00:00");
    root.add(&elapsed, 0, SizerFlag::Expand | SizerFlag::All, 6);
    let start = Button::builder(&panel)
        .with_label(&tr(&labels, "record_start"))
        .build();
    start.enable(false);
    let pause = Button::builder(&panel)
        .with_label(&tr(&labels, "record_pause"))
        .build();
    pause.enable(false);
    let stop = Button::builder(&panel)
        .with_label(&tr(&labels, "record_stop"))
        .build();
    stop.enable(false);
    let close = Button::builder(&panel)
        .with_label(&tr(&labels, "close"))
        .build();
    let buttons = BoxSizer::builder(Orientation::Horizontal).build();
    for button in [start, pause, stop, close] {
        buttons.add(&button, 0, SizerFlag::All, 6);
    }
    root.add_sizer(&buttons, 0, SizerFlag::Expand, 0);
    panel.set_sizer(root, true);
    let saving_root = BoxSizer::builder(Orientation::Vertical).build();
    let save_status = TextCtrl::builder(&saving_panel)
        .with_style(TextCtrlStyle::ReadOnly)
        .build();
    save_status.set_accessibility_label(&tr(&labels, "record_saving"));
    let gauge = Gauge::builder(&saving_panel).with_range(100).build();
    let cancel_button = Button::builder(&saving_panel)
        .with_label(&tr(&labels, "cancel"))
        .build();
    saving_root.add(&save_status, 0, SizerFlag::Expand | SizerFlag::All, 12);
    saving_root.add(&gauge, 0, SizerFlag::Expand | SizerFlag::All, 12);
    saving_root.add(&cancel_button, 0, SizerFlag::All, 12);
    saving_panel.set_sizer(saving_root, true);
    let layout = BoxSizer::builder(Orientation::Vertical).build();
    layout.add(&panel, 1, SizerFlag::Expand, 0);
    layout.add(&saving_panel, 1, SizerFlag::Expand, 0);
    dialog.set_sizer(layout, true);
    saving_panel.show(false);

    let state = Rc::new(Cell::new(State::Idle));
    let system_available = Rc::new(Cell::new(false));
    let devices = Rc::new(RefCell::new(Vec::<engine::Device>::new()));
    let cancel = Arc::new(AtomicBool::new(false));
    let close_after = Rc::new(Cell::new(false));
    let command_tx = Rc::new(RefCell::new(None::<mpsc::Sender<Control>>));
    let (event_tx, event_rx) = mpsc::channel::<Event>();
    let (device_tx, device_rx) = mpsc::channel();
    let load_devices = Rc::new(move || {
        let tx = device_tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(engine::helper_path().and_then(|p| engine::devices(&p)));
        });
    });
    load_devices();
    let loader = load_devices.clone();
    let l = labels.clone();
    refresh.on_click(move |_| {
        refresh.enable(false);
        start.enable(false);
        status.set_value(&tr(&l, "record_loading"));
        loader();
    });
    mic.on_toggled(move |_| {
        let enabled = mic.get_value();
        device.enable(enabled);
        mic_gain.enable(enabled);
    });
    system.on_toggled(move |_| {
        system_gain.enable(system.get_value());
    });
    let l = labels.clone();
    choose.on_click(move |_| {
        let ext = engine::FORMATS[format.get_selection().unwrap_or(0) as usize];
        let path = PathBuf::from(output.get_value());
        let name = if path.as_os_str().is_empty() {
            format!(
                "{}-{}.{ext}",
                tr(&l, "record_filename"),
                chrono::Local::now().format("%Y-%m-%d_%H-%M-%S")
            )
        } else {
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        };
        let picker = FileDialog::builder(&dialog)
            .with_message(&tr(&l, "record_choose_output"))
            .with_style(FileDialogStyle::Save)
            .with_default_file(&name)
            .with_wildcard(&format!("{}|*.{ext}", ext.to_uppercase()))
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
            path.set_extension(engine::FORMATS[format.get_selection().unwrap_or(0) as usize]);
            output.set_value(&path.to_string_lossy());
        }
    });
    let s = state.clone();
    let l = labels.clone();
    let d = devices.clone();
    let c = cancel.clone();
    let commands = command_tx.clone();
    start.on_click(move |_| {
        if s.get() != State::Idle {
            return;
        }
        if !mic.get_value() && !system.get_value() {
            crate::show_message_subdialog(
                &dialog,
                &tr(&l, "record_title"),
                &tr(&l, "record_no_source"),
            );
            return;
        }
        if output.get_value().is_empty() {
            choose.set_focus();
            return;
        }
        let path = PathBuf::from(output.get_value());
        if path.exists() {
            crate::show_message_subdialog(
                &dialog,
                &tr(&l, "record_title"),
                &tr(&l, "record_exists"),
            );
            return;
        }
        let (helper, ffmpeg) = match (engine::helper_path(), crate::ffmpeg_executable_path()) {
            (Ok(helper), Some(ffmpeg)) => (helper, ffmpeg),
            _ => {
                crate::show_message_subdialog(
                    &dialog,
                    &tr(&l, "record_title"),
                    &tr(&l, "record_missing"),
                );
                return;
            }
        };
        let microphone = if mic.get_value() {
            Some(
                device
                    .get_selection()
                    .and_then(|i| i.checked_sub(1))
                    .and_then(|i| d.borrow().get(i as usize).map(|v| v.id.clone()))
                    .unwrap_or_else(|| "default".into()),
            )
        } else {
            None
        };
        let config = Config {
            microphone,
            system: system.get_value(),
            output: path,
            mic_gain: mic_gain.get_selection().unwrap_or(4) as f64 / 4.0,
            system_gain: system_gain.get_selection().unwrap_or(4) as f64 / 4.0,
        };
        let (tx, rx) = mpsc::channel();
        *commands.borrow_mut() = Some(tx);
        c.store(false, Ordering::Relaxed);
        s.set(State::Starting);
        for button in [start, refresh, choose] {
            button.enable(false);
        }
        for choice in [device, mic_gain, system_gain, format] {
            choice.enable(false);
        }
        mic.enable(false);
        system.enable(false);
        elapsed.set_value("00:00:00");
        status.set_value(&tr(&l, "record_starting"));
        crate::announce_voiceover_message(&tr(&l, "record_starting"));
        close.set_focus();
        let tx = event_tx.clone();
        let c = c.clone();
        std::thread::spawn(move || engine::record(&helper, &ffmpeg, config, rx, &c, &tx));
    });
    let commands = command_tx.clone();
    let s = state.clone();
    pause.on_click(move |_| {
        if let Some(tx) = commands.borrow().as_ref() {
            let command = if s.get() == State::Paused {
                Control::Resume
            } else {
                Control::Pause
            };
            if tx.send(command).is_ok() {
                pause.enable(false);
            }
        }
    });
    let commands = command_tx.clone();
    let s = state.clone();
    let l = labels.clone();
    stop.on_click(move |_| {
        if let Some(tx) = commands.borrow().as_ref()
            && tx.send(Control::Stop).is_ok()
        {
            s.set(State::Stopping);
            pause.enable(false);
            stop.enable(false);
            status.set_value(&tr(&l, "record_stopping"));
        }
    });
    let c = cancel.clone();
    let l = labels.clone();
    cancel_button.on_click(move |_| {
        if crate::ask_yes_no_dialog(
            &dialog,
            &tr(&l, "record_title"),
            &tr(&l, "record_cancel_save"),
        ) {
            c.store(true, Ordering::Relaxed);
            cancel_button.enable(false);
        }
    });
    let timer = Timer::new(&dialog);
    let s = state.clone();
    let available = system_available.clone();
    let d = devices.clone();
    let l = labels.clone();
    let closing = close_after.clone();
    let commands = command_tx.clone();
    let last_percent = Cell::new(None);
    let last_second = Cell::new(None);
    timer.on_tick(move |_| {
        for result in device_rx.try_iter() {
            refresh.enable(true);
            match result {
                Ok((supports_system, list)) => {
                    available.set(supports_system);
                    system.enable(supports_system);
                    compatibility.show(!supports_system);
                    device.clear();
                    device.append(&tr(&l, "record_default"));
                    for entry in &list {
                        device.append(&entry.name);
                    }
                    device.set_selection(0);
                    *d.borrow_mut() = list;
                    start.enable(true);
                    status.set_value(&tr(&l, "record_ready"));
                    dialog.layout();
                }
                Err(error) => {
                    crate::append_podcast_log(&format!("podcast_recording.devices: {error}"));
                    status.set_value(&tr(&l, "record_missing"));
                }
            }
        }
        for event in event_rx.try_iter() {
            match event {
                Event::Starting => {}
                Event::Recording | Event::Paused => {
                    if s.get() == State::Stopping {
                        continue;
                    }
                    let paused = matches!(event, Event::Paused);
                    s.set(if paused {
                        State::Paused
                    } else {
                        State::Recording
                    });
                    let message = tr(
                        &l,
                        if paused {
                            "record_paused"
                        } else {
                            "record_recording"
                        },
                    );
                    status.set_value(&message);
                    pause.set_label(&tr(
                        &l,
                        if paused {
                            "record_resume"
                        } else {
                            "record_pause"
                        },
                    ));
                    pause.enable(true);
                    stop.enable(true);
                    pause.set_focus();
                    crate::announce_voiceover_message(&message);
                }
                Event::Elapsed(seconds) => {
                    let secs = seconds.max(0.0) as u64;
                    if last_second.get() != Some(secs) {
                        last_second.set(Some(secs));
                        elapsed.set_value(&format!(
                            "{:02}:{:02}:{:02}",
                            secs / 3600,
                            secs / 60 % 60,
                            secs % 60
                        ));
                    }
                }
                Event::Saving(percent) => {
                    if s.get() != State::Saving {
                        s.set(State::Saving);
                        panel.show(false);
                        saving_panel.show(true);
                        cancel_button.enable(true);
                        dialog.layout();
                        save_status.set_focus();
                        crate::announce_voiceover_message(&tr(&l, "record_saving"));
                        last_percent.set(None);
                    }
                    if last_percent.get() != Some(percent) {
                        last_percent.set(Some(percent));
                        let message = format!("{} {percent}%", tr(&l, "record_saving"));
                        save_status.set_value(&message);
                        gauge.set_value(percent as i32);
                        gauge.set_accessibility_label(&message);
                    }
                }
                Event::Done(_) | Event::Failed(_, _) => {
                    let message = match event {
                        Event::Done(path) => {
                            format!("{}\n{}", tr(&l, "record_done"), path.display())
                        }
                        Event::Failed(error, recovery) => {
                            crate::append_podcast_log(&format!(
                                "podcast_recording.error: {error}; recovery={recovery:?}"
                            ));
                            let key = if error == "__CANCELLED__" {
                                "record_cancelled"
                            } else if error.starts_with("microphone_permission:") {
                                "record_mic_permission"
                            } else if error.starts_with("system:") {
                                "record_system_permission"
                            } else {
                                "record_failed"
                            };
                            let mut message = tr(&l, key);
                            if error != "__CANCELLED__" {
                                message.push_str(&format!("\n{error}"));
                            }
                            if let Some(path) = recovery {
                                message.push_str(&format!(
                                    "\n{}\n{}",
                                    tr(&l, "record_recovery"),
                                    path.display()
                                ));
                            }
                            closing.set(false);
                            message
                        }
                        _ => unreachable!(),
                    };
                    s.set(State::Idle);
                    *commands.borrow_mut() = None;
                    saving_panel.show(false);
                    panel.show(true);
                    dialog.layout();
                    for button in [start, refresh, choose] {
                        button.enable(true);
                    }
                    format.enable(true);
                    device.enable(mic.get_value());
                    mic_gain.enable(mic.get_value());
                    system_gain.enable(system.get_value());
                    system.enable(available.get());
                    mic.enable(true);
                    pause.enable(false);
                    stop.enable(false);
                    pause.set_label(&tr(&l, "record_pause"));
                    status.set_value(&tr(&l, "record_ready"));
                    last_second.set(None);
                    crate::announce_voiceover_message(&message);
                    crate::show_message_subdialog(&dialog, &tr(&l, "record_title"), &message);
                    if closing.get() {
                        dialog.end_modal(ID_OK);
                    } else {
                        start.set_focus();
                    }
                }
            }
        }
    });
    close.on_click(move |_| {
        dialog.close(false);
    });
    let s = state.clone();
    let c = cancel.clone();
    let l = labels.clone();
    dialog.on_close(move |event| {
        if s.get() == State::Idle {
            event.skip(true);
            return;
        }
        event.skip(false);
        let saving = matches!(s.get(), State::Saving | State::Starting);
        if crate::ask_yes_no_dialog(
            &dialog,
            &tr(&l, "record_title"),
            &tr(
                &l,
                if saving {
                    "record_cancel_save"
                } else {
                    "record_close_recording"
                },
            ),
        ) {
            if s.get() == State::Idle {
                dialog.end_modal(ID_CANCEL);
                return;
            }
            close_after.set(true);
            if saving {
                c.store(true, Ordering::Relaxed);
            } else if let Some(tx) = command_tx.borrow().as_ref() {
                let _ = tx.send(Control::Stop);
                s.set(State::Stopping);
                pause.enable(false);
                stop.enable(false);
            }
        }
    });
    macro_rules! close_keys { ($($control:expr),+ $(,)?) => { $(
        $control.on_key_down(move |event| {
            if let WindowEventData::Keyboard(key) = &event {
                let code = key.get_key_code().unwrap_or_default();
                if !key.alt_down() && (code == 27 || (key.cmd_down() && code == 87)) { event.skip(false); dialog.close(false); return; }
            } event.skip(true);
        });
    )+ }; }
    close_keys!(
        dialog,
        panel,
        saving_panel,
        mic,
        system,
        device,
        refresh,
        mic_gain,
        system_gain,
        output,
        choose,
        format,
        status,
        elapsed,
        start,
        pause,
        stop,
        close,
        save_status,
        gauge,
        cancel_button
    );
    timer.start(150, false);
    mic.set_focus();
    dialog.show_modal();
    timer.stop();
    dialog.destroy();
}

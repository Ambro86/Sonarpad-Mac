use super::*;

pub fn open(parent: &Frame) {
    let ui = current_ui_strings();
    let dialog = Dialog::builder(parent, &ui.report_title)
        .with_style(DialogStyle::DefaultDialogStyle | DialogStyle::ResizeBorder)
        .with_size(560, 420)
        .build();
    let panel = Panel::builder(&dialog).build();
    let root = BoxSizer::builder(Orientation::Vertical).build();
    let name = TextCtrl::builder(&panel).build();
    let email = TextCtrl::builder(&panel).build();
    let report = TextCtrl::builder(&panel)
        .with_style(TextCtrlStyle::MultiLine)
        .build();
    name.set_name(&ui.report_name);
    email.set_name(&ui.report_email);
    report.set_name(&ui.report_text);
    for (label, ctrl, proportion) in [
        (&ui.report_name, &name, 0),
        (&ui.report_email, &email, 0),
        (&ui.report_text, &report, 1),
    ] {
        root.add(
            &StaticText::builder(&panel).with_label(label).build(),
            0,
            SizerFlag::All,
            6,
        );
        root.add(
            ctrl,
            proportion,
            SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Bottom,
            6,
        );
    }
    root.add(
        &StaticText::builder(&panel)
            .with_label(&ui.report_notice)
            .build(),
        0,
        SizerFlag::All,
        6,
    );
    let buttons = BoxSizer::builder(Orientation::Horizontal).build();
    let send = Button::builder(&panel)
        .with_id(ID_OK)
        .with_label(&ui.report_send)
        .build();
    let cancel = Button::builder(&panel)
        .with_id(ID_CANCEL)
        .with_label(&ui.cancel)
        .build();
    buttons.add(&send, 0, SizerFlag::All, 8);
    buttons.add(&cancel, 0, SizerFlag::All, 8);
    root.add_sizer(&buttons, 0, SizerFlag::AlignRight, 0);
    panel.set_sizer(root, true);
    dialog.set_affirmative_id(ID_OK);
    dialog.set_escape_id(ID_CANCEL);
    dialog.centre();
    name.set_focus();
    loop {
        if dialog.show_modal() != ID_OK {
            break;
        }
        let name_value = name.get_value();
        let email_value = email.get_value();
        let report_value = report.get_value();
        if report_value.trim().is_empty()
            || report_value.chars().count() > 8000
            || name_value.chars().count() > 200
        {
            show_message_dialog(parent, &ui.report_title, &ui.report_invalid_text);
            report.set_focus();
            continue;
        }
        if !developer_report::valid_email(&email_value) {
            show_message_dialog(parent, &ui.report_title, &ui.report_invalid_email);
            email.set_focus();
            continue;
        }
        #[cfg(any(target_os = "macos", windows))]
        let path = primary_podcast_log_path();
        #[cfg(not(any(target_os = "macos", windows)))]
        let path: Option<PathBuf> = None;
        let path = path
            .filter(|p| p.exists())
            .unwrap_or_else(|| PathBuf::from("log.txt"));
        let (tx, rx) = std::sync::mpsc::channel();
        let worker = std::thread::Builder::new()
            .name("diagnostic-report".into())
            .spawn(move || {
                let result =
                    developer_report::send(&path, &name_value, &email_value, &report_value);
                if tx.send(result).is_err() {
                    append_podcast_log("diagnostics.report.receiver_closed");
                }
            });
        if worker.is_err() {
            show_message_dialog(parent, &ui.report_title, &ui.report_failure);
            continue;
        }
        let progress = ProgressDialog::builder(parent, &ui.report_title, &ui.report_sending, 100)
            .with_style(ProgressDialogStyle::AppModal)
            .build();
        let result = loop {
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(result) => break result,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    progress.pulse(None);
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    break Err("worker_disconnected".into());
                }
            }
        };
        progress.destroy();
        match result {
            Ok(()) => {
                append_podcast_log("diagnostics.report.sent");
                show_message_dialog(parent, &ui.report_title, &ui.report_success);
                break;
            }
            Err(err) => {
                append_podcast_log(&format!("diagnostics.report.failed {err}"));
                show_message_dialog(parent, &ui.report_title, &ui.report_failure);
            }
        }
    }
    dialog.destroy();
}

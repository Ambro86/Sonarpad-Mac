use crate::Settings;
use crate::faster_whisper_bridge::{
    BridgeModel, BridgeProgressCallbacks, transcribe_media,
};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use wxdragon::prelude::*;

const ID_TRANSCRIPTION_START: i32 = 7200;
const ID_TRANSCRIPTION_CLOSE: i32 = 7201;
const ID_TRANSCRIPTION_PROGRESS_CANCEL: i32 = 7202;

#[derive(Clone, Debug)]
struct ProgressState {
    progress: i32,
    stage: String,
    done: Option<Result<PathBuf, String>>,
}

fn tr_map() -> &'static HashMap<String, String> {
    static IT: OnceLock<HashMap<String, String>> = OnceLock::new();
    static EN: OnceLock<HashMap<String, String>> = OnceLock::new();
    static FR: OnceLock<HashMap<String, String>> = OnceLock::new();
    static ES: OnceLock<HashMap<String, String>> = OnceLock::new();
    static PT: OnceLock<HashMap<String, String>> = OnceLock::new();
    static CS: OnceLock<HashMap<String, String>> = OnceLock::new();
    static PL: OnceLock<HashMap<String, String>> = OnceLock::new();
    let lang = Settings::load().ui_language;
    let (slot, raw) = match lang.as_str() {
        "en" => (&EN, include_str!("../i18n/media_transcription_en.json")),
        "fr" => (&FR, include_str!("../i18n/media_transcription_fr.json")),
        "es" => (&ES, include_str!("../i18n/media_transcription_es.json")),
        "pt" => (&PT, include_str!("../i18n/media_transcription_pt.json")),
        "cs" => (&CS, include_str!("../i18n/media_transcription_cs.json")),
        "pl" => (&PL, include_str!("../i18n/media_transcription_pl.json")),
        _ => (&IT, include_str!("../i18n/media_transcription_it.json")),
    };
    slot.get_or_init(|| serde_json::from_str(raw).unwrap_or_default())
}

fn tr(key: &str) -> String {
    tr_map()
        .get(key)
        .cloned()
        .unwrap_or_else(|| key.to_string())
}

fn trf(key: &str, values: &[(&str, String)]) -> String {
    let mut text = tr(key);
    for (name, value) in values {
        text = text.replace(&format!("{{{name}}}"), value);
    }
    text
}

pub fn menu_label() -> String {
    tr("media_transcription.title")
}

fn media_wildcard() -> &'static str {
    "Media|*.mp3;*.wav;*.m4a;*.m4b;*.aac;*.flac;*.ogg;*.opus;*.mp4;*.mkv;*.mov;*.m4v;*.avi;*.webm;*.mpeg;*.mpg|Tutti i file|*.*"
}

fn choose_input(parent: &Dialog) -> Option<PathBuf> {
    let dialog = FileDialog::builder(parent)
        .with_message(&tr("media_transcription.open_title"))
        .with_wildcard(media_wildcard())
        .with_style(FileDialogStyle::Open | FileDialogStyle::FileMustExist)
        .build();
    if dialog.show_modal() == ID_OK {
        dialog.get_path().map(PathBuf::from)
    } else {
        None
    }
}

fn suggested_output(input: &Path) -> PathBuf {
    let stem = input
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("trascrizione");
    let file_name = format!("{stem}_trascrizione.txt");
    input
        .parent()
        .map(|parent| parent.join(&file_name))
        .unwrap_or_else(|| PathBuf::from(file_name))
}

fn choose_output(parent: &Dialog, input: Option<&Path>) -> Option<PathBuf> {
    let default_file = input
        .map(suggested_output)
        .and_then(|path| path.file_name().map(|name| name.to_string_lossy().to_string()))
        .unwrap_or_else(|| "trascrizione.txt".to_string());
    let dialog = FileDialog::builder(parent)
        .with_message(&tr("media_transcription.save_title"))
        .with_default_file(&default_file)
        .with_wildcard("Testo|*.txt")
        .with_style(FileDialogStyle::Save | FileDialogStyle::OverwritePrompt)
        .build();
    if dialog.show_modal() == ID_OK {
        dialog.get_path().map(PathBuf::from)
    } else {
        None
    }
}

fn selected_model(choice: &Choice) -> BridgeModel {
    match choice.get_selection().unwrap_or(0) {
        1 => BridgeModel::Medium,
        2 => BridgeModel::LargeV3,
        _ => BridgeModel::Small,
    }
}

fn show_message(parent: &dyn WxWidget, title: &str, message: &str, error: bool) {
    let style = if error {
        MessageDialogStyle::OK | MessageDialogStyle::IconError
    } else {
        MessageDialogStyle::OK | MessageDialogStyle::IconInformation
    };
    let dialog = MessageDialog::builder(parent, message, title)
        .with_style(style)
        .build();
    dialog.show_modal();
}

fn run_with_progress(
    parent: &Dialog,
    input: PathBuf,
    output: PathBuf,
    model: BridgeModel,
) -> Result<PathBuf, String> {
    let progress_dialog = Dialog::builder(parent, &tr("media_transcription.title"))
        .with_style(
            DialogStyle::Caption
                | DialogStyle::SystemMenu
                | DialogStyle::CloseBox
                | DialogStyle::StayOnTop,
        )
        .with_size(520, 180)
        .build();
    let panel = Panel::builder(&progress_dialog).build();
    let root = BoxSizer::builder(Orientation::Vertical).build();
    let status = StaticText::builder(&panel)
        .with_label(&tr("media_transcription.status.preparing_model"))
        .build();
    root.add(
        &status,
        0,
        SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Top,
        12,
    );
    let gauge = Gauge::builder(&panel).with_range(100).build();
    root.add(
        &gauge,
        0,
        SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Top,
        12,
    );
    let buttons = BoxSizer::builder(Orientation::Horizontal).build();
    let cancel_button = Button::builder(&panel)
        .with_id(ID_TRANSCRIPTION_PROGRESS_CANCEL)
        .with_label(&tr("media_transcription.cancel"))
        .build();
    buttons.add_spacer(1);
    buttons.add(&cancel_button, 0, SizerFlag::All, 10);
    root.add_sizer(&buttons, 0, SizerFlag::Expand | SizerFlag::Bottom, 0);
    panel.set_sizer(root, true);

    let state = Arc::new(Mutex::new(ProgressState {
        progress: 0,
        stage: "model".to_string(),
        done: None,
    }));
    let cancel = Arc::new(AtomicBool::new(false));
    let state_thread = Arc::clone(&state);
    let cancel_thread = Arc::clone(&cancel);
    thread::spawn(move || {
        let state_progress = Arc::clone(&state_thread);
        let state_stage = Arc::clone(&state_thread);
        let result = transcribe_media(
            &input,
            model,
            cancel_thread,
            BridgeProgressCallbacks {
                transcription: Some(Box::new(move |progress| {
                    state_progress.lock().unwrap().progress = progress;
                })),
                stage: Some(Box::new(move |stage| {
                    state_stage.lock().unwrap().stage = stage.to_string();
                })),
            },
        )
        .and_then(|result| {
            if result.text.trim().is_empty() {
                return Err(tr("media_transcription.error.empty"));
            }
            fs::write(&output, result.text.as_bytes())
                .map_err(|error| trf("media_transcription.error.save", &[("error", error.to_string())]))?;
            Ok(output)
        });
        state_thread.lock().unwrap().done = Some(result);
    });

    let result = Rc::new(RefCell::new(None::<Result<PathBuf, String>>));
    let finished = Rc::new(Cell::new(false));
    let cancel_pending = Rc::new(Cell::new(false));
    let cancel_click = Arc::clone(&cancel);
    let cancel_pending_click = Rc::clone(&cancel_pending);
    let status_cancel = status;
    cancel_button.on_click(move |_| {
        if !cancel_pending_click.replace(true) {
            cancel_click.store(true, Ordering::SeqCst);
            cancel_button.enable(false);
            status_cancel.set_label(&tr("media_transcription.status.canceling"));
        }
    });

    let cancel_close = Arc::clone(&cancel);
    let cancel_pending_close = Rc::clone(&cancel_pending);
    let finished_close = Rc::clone(&finished);
    let status_close = status;
    progress_dialog.on_close(move |event| {
        if finished_close.get() {
            event.skip(true);
            return;
        }
        if !cancel_pending_close.replace(true) {
            cancel_close.store(true, Ordering::SeqCst);
            cancel_button.enable(false);
            status_close.set_label(&tr("media_transcription.status.canceling"));
        }
        event.skip(false);
    });

    let timer = Rc::new(Timer::new(&progress_dialog));
    let timer_tick = Rc::clone(&timer);
    let timer_stop = Rc::clone(&timer);
    let state_tick = Arc::clone(&state);
    let result_tick = Rc::clone(&result);
    let finished_tick = Rc::clone(&finished);
    let cancel_pending_tick = Rc::clone(&cancel_pending);
    let dialog_tick = progress_dialog;
    let status_tick = status;
    let gauge_tick = gauge;
    timer_tick.on_tick(move |_| {
        let snapshot = state_tick.lock().unwrap().clone();
        if !cancel_pending_tick.get() {
            if snapshot.stage == "transcribing" {
                status_tick.set_label(&trf(
                    "media_transcription.status.transcribing",
                    &[("progress", snapshot.progress.clamp(0, 100).to_string())],
                ));
                gauge_tick.set_value(snapshot.progress.clamp(0, 99));
            } else {
                status_tick.set_label(&tr("media_transcription.status.preparing_model"));
                // wxDragon 0.9.9 does not expose wxGauge::Pulse. Keep the gauge
                // determinate while the bundled worker prepares/downloads the model.
                gauge_tick.set_value(snapshot.progress.clamp(0, 99));
            }
        }
        if let Some(done) = snapshot.done {
            timer_stop.stop();
            if done.is_ok() {
                gauge_tick.set_value(100);
            }
            *result_tick.borrow_mut() = Some(done);
            finished_tick.set(true);
            dialog_tick.end_modal(ID_OK);
        }
    });
    timer.start(100, false);
    progress_dialog.show_modal();
    timer.stop();
    progress_dialog.destroy();
    result
        .borrow_mut()
        .take()
        .unwrap_or_else(|| Err("cancelled".to_string()))
}

pub fn open_dialog(parent: &Frame) {
    let dialog = Dialog::builder(parent, &tr("media_transcription.title"))
        .with_style(DialogStyle::DefaultDialogStyle | DialogStyle::ResizeBorder)
        .with_size(760, 300)
        .build();
    let panel = Panel::builder(&dialog).build();
    let root = BoxSizer::builder(Orientation::Vertical).build();

    let input_row = BoxSizer::builder(Orientation::Horizontal).build();
    input_row.add(
        &StaticText::builder(&panel)
            .with_label(&tr("media_transcription.input"))
            .build(),
        0,
        SizerFlag::AlignCenterVertical | SizerFlag::All,
        5,
    );
    let input = TextCtrl::builder(&panel).build();
    input_row.add(&input, 1, SizerFlag::Expand | SizerFlag::All, 5);
    let input_button = Button::builder(&panel)
        .with_label(&tr("media_transcription.browse_input"))
        .build();
    input_row.add(&input_button, 0, SizerFlag::All, 5);
    root.add_sizer(&input_row, 0, SizerFlag::Expand, 0);

    let output_row = BoxSizer::builder(Orientation::Horizontal).build();
    output_row.add(
        &StaticText::builder(&panel)
            .with_label(&tr("media_transcription.output"))
            .build(),
        0,
        SizerFlag::AlignCenterVertical | SizerFlag::All,
        5,
    );
    let output = TextCtrl::builder(&panel).build();
    output_row.add(&output, 1, SizerFlag::Expand | SizerFlag::All, 5);
    let output_button = Button::builder(&panel)
        .with_label(&tr("media_transcription.browse_output"))
        .build();
    output_row.add(&output_button, 0, SizerFlag::All, 5);
    root.add_sizer(&output_row, 0, SizerFlag::Expand, 0);

    let model_row = BoxSizer::builder(Orientation::Horizontal).build();
    model_row.add(
        &StaticText::builder(&panel)
            .with_label(&tr("media_transcription.model"))
            .build(),
        0,
        SizerFlag::AlignCenterVertical | SizerFlag::All,
        5,
    );
    let model = Choice::builder(&panel).build();
    model.append(&tr("media_transcription.model.small"));
    model.append(&tr("media_transcription.model.medium"));
    model.append(&tr("media_transcription.model.large"));
    model.set_selection(0);
    model_row.add(&model, 1, SizerFlag::Expand | SizerFlag::All, 5);
    root.add_sizer(&model_row, 0, SizerFlag::Expand, 0);

    let buttons = BoxSizer::builder(Orientation::Horizontal).build();
    let start = Button::builder(&panel)
        .with_id(ID_TRANSCRIPTION_START)
        .with_label(&tr("media_transcription.start"))
        .build();
    let close = Button::builder(&panel)
        .with_id(ID_TRANSCRIPTION_CLOSE)
        .with_label(&tr("media_transcription.close"))
        .build();
    buttons.add_spacer(1);
    buttons.add(&start, 0, SizerFlag::All, 10);
    buttons.add(&close, 0, SizerFlag::All, 10);
    root.add_sizer(&buttons, 0, SizerFlag::Expand, 0);
    panel.set_sizer(root, true);

    let dialog_input = dialog;
    let output_input = output;
    input_button.on_click(move |_| {
        if let Some(path) = choose_input(&dialog_input) {
            input.set_value(&path.to_string_lossy());
            output_input.set_value(&suggested_output(&path).to_string_lossy());
        }
    });

    let dialog_output = dialog;
    let input_output = input;
    output_button.on_click(move |_| {
        let current = PathBuf::from(input_output.get_value());
        let input_path = current.is_file().then_some(current.as_path());
        if let Some(path) = choose_output(&dialog_output, input_path) {
            output.set_value(&path.to_string_lossy());
        }
    });

    let dialog_start = dialog;
    start.on_click(move |_| {
        let input_path = PathBuf::from(input.get_value());
        if !input_path.is_file() {
            show_message(
                &dialog_start,
                &tr("media_transcription.title"),
                &tr("media_transcription.error.input"),
                true,
            );
            input.set_focus();
            return;
        }
        let output_path = PathBuf::from(output.get_value());
        if output_path.as_os_str().is_empty() {
            show_message(
                &dialog_start,
                &tr("media_transcription.title"),
                &tr("media_transcription.error.output"),
                true,
            );
            output.set_focus();
            return;
        }
        if input_path == output_path {
            show_message(
                &dialog_start,
                &tr("media_transcription.title"),
                &tr("media_transcription.error.same_path"),
                true,
            );
            output.set_focus();
            return;
        }
        let chosen_model = selected_model(&model);
        match run_with_progress(&dialog_start, input_path, output_path, chosen_model) {
            Ok(path) => {
                show_message(
                    &dialog_start,
                    &tr("media_transcription.completed_title"),
                    &trf(
                        "media_transcription.completed",
                        &[("path", path.to_string_lossy().to_string())],
                    ),
                    false,
                );
                start.set_focus();
            }
            Err(error) if error == "cancelled" => {
                start.set_focus();
            }
            Err(error) => {
                show_message(
                    &dialog_start,
                    &tr("media_transcription.title"),
                    &trf("media_transcription.error.worker", &[("error", error)]),
                    true,
                );
                start.set_focus();
            }
        }
    });

    dialog.set_escape_id(ID_TRANSCRIPTION_CLOSE);
    let dialog_close = dialog;
    close.on_click(move |_| dialog_close.end_modal(ID_TRANSCRIPTION_CLOSE));
    let dialog_window_close = dialog;
    dialog.on_close(move |event| {
        dialog_window_close.end_modal(ID_TRANSCRIPTION_CLOSE);
        event.skip(false);
    });
    dialog.show_modal();
    dialog.destroy();
}

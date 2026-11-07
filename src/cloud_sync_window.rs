use crate::cloud_sync::{self, Session};
use crate::*;
use serde_json::{Value, json};
use std::sync::atomic::{AtomicBool, Ordering};
static BUSY: AtomicBool = AtomicBool::new(false);
static UI_OPEN: AtomicBool = AtomicBool::new(false);
static AUTO: std::sync::OnceLock<Mutex<cloud_sync::AutoSync>> = std::sync::OnceLock::new();
fn scheduler() -> &'static Mutex<cloud_sync::AutoSync> {
    AUTO.get_or_init(|| Mutex::new(cloud_sync::AutoSync::default()))
}
struct FlagGuard(&'static AtomicBool);
impl Drop for FlagGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}
type Outcome = (Session, Option<Value>, Option<String>);
pub struct Automatic {
    pub changed: bool,
    receiver: Option<std::sync::mpsc::Receiver<Result<Outcome, String>>>,
    checked: Option<std::time::Instant>,
}
impl Automatic {
    pub fn new() -> Self {
        Self {
            changed: false,
            receiver: None,
            checked: None,
        }
    }
    pub fn tick(&mut self, settings: &Arc<Mutex<Settings>>) {
        if let Some(receiver) = &self.receiver {
            let result = match receiver.try_recv() {
                Ok(result) => result,
                Err(std::sync::mpsc::TryRecvError::Empty) => return,
                Err(_) => Err("network_error".into()),
            };
            self.receiver = None;
            let result = result.and_then(|outcome| finish(settings, outcome));
            self.changed = result.is_ok();
            BUSY.store(false, Ordering::SeqCst);
            if let Ok(mut scheduler) = scheduler().lock() {
                scheduler.completed(result.is_ok(), cloud_sync::now_seconds());
            }
            if let Err(error) = result {
                append_podcast_log(&format!("CloudSync automatic completion error={error}"));
            }
        }
        if BUSY.load(Ordering::SeqCst) || UI_OPEN.load(Ordering::SeqCst) {
            return;
        }
        if self.checked.is_some_and(|t| t.elapsed().as_secs() < 10) {
            return;
        }
        self.checked = Some(std::time::Instant::now());
        let path = app_storage_path("cloud_sync.json");
        let Some(dir) = path.parent() else {
            return;
        };
        let Ok(meta) = Session::metadata(dir) else {
            return;
        };
        if !meta["group_id"].is_string() {
            return;
        }
        let Ok(before) = snapshot_for_sync() else {
            return;
        };
        let dirty = cloud_sync::local_changes(&meta, &before, true).unwrap_or(true);
        let due = scheduler()
            .lock()
            .map(|mut s| s.due(&meta, dirty, cloud_sync::now_seconds()))
            .unwrap_or(false);
        if !due || BUSY.swap(true, Ordering::SeqCst) {
            return;
        }
        let dir = dir.to_path_buf();
        let (sender, receiver) = std::sync::mpsc::channel();
        self.receiver = Some(receiver);
        std::thread::spawn(move || {
            let _ = sender.send(request(2, String::new(), before, dir));
        });
    }
}

fn label(it: &'static str, en: &'static str) -> &'static str {
    if Settings::load().ui_language == "it" {
        it
    } else {
        en
    }
}
fn snapshot_for_sync() -> Result<Value, String> {
    let data = read_app_storage_text("settings.json").ok_or("storage_error")?;
    let mut settings: Settings = serde_json::from_str(&data).map_err(|_| "storage_error")?;
    settings.ui_language = normalize_ui_language(&settings.ui_language);
    settings.news_language = normalize_news_language(&settings.news_language);
    normalize_settings_data(&mut settings);
    snapshot(&settings)
}

pub(crate) fn snapshot(settings: &Settings) -> Result<Value, String> {
    let mut value = serde_json::to_value(settings).map_err(|_| "storage_error")?;
    value["_cloud_default_urls"] = json!(
        articles::default_sources_for_news_language(&settings.news_language)
            .iter()
            .map(|s| s.url.clone())
            .collect::<Vec<_>>()
    );
    Ok(value)
}

pub fn open(parent: &Frame, settings: &Arc<Mutex<Settings>>) {
    let title = label("Sincronizzazione cloud", "Cloud sync");
    let Some(dir) = app_storage_path("cloud_sync.json")
        .parent()
        .map(Path::to_path_buf)
    else {
        return;
    };
    if UI_OPEN.swap(true, Ordering::SeqCst) {
        return;
    }
    let _guard = FlagGuard(&UI_OPEN);
    let mut page = 0;
    loop {
        let meta = match Session::metadata(&dir) {
            Ok(meta) => meta,
            Err(_) => {
                show_message_dialog(
                    parent,
                    title,
                    label(
                        "Non riesco a leggere lo stato della sincronizzazione. Riprova.",
                        "Unable to read sync status. Please try again.",
                    ),
                );
                return;
            }
        };
        let connected = meta["group_id"].is_string();
        let dirty = snapshot_for_sync()
            .and_then(|s| cloud_sync::local_changes(&meta, &s, true))
            .unwrap_or(true);
        let status = cloud_sync::status(
            &meta,
            dirty,
            BUSY.load(Ordering::SeqCst),
            scheduler().lock().map(|s| s.failed()).unwrap_or(false),
            Settings::load().ui_language == "it",
        );
        if BUSY.load(Ordering::SeqCst) {
            show_message_dialog(parent, title, &status);
            return;
        }
        if !connected && page == 1 {
            page = 0;
        }
        let (help, entries) = if page == 1 {
            (
                label(
                    "Qui puoi scegliere cosa sincronizzare o scollegare questo dispositivo. Normalmente non serve cambiare queste opzioni.",
                    "Choose what to sync or disconnect this device. Usually these options need no changes.",
                ),
                vec![
                    (8, label("Torna alla sincronizzazione", "Back to sync")),
                    (
                        5,
                        if meta["sync_preferences"] == json!(false) {
                            label(
                                "Attiva sincronizzazione preferenze",
                                "Enable preference sync",
                            )
                        } else {
                            label(
                                "Disattiva sincronizzazione preferenze",
                                "Disable preference sync",
                            )
                        },
                    ),
                    (
                        6,
                        if meta["sync_collections"] == json!(false) {
                            label(
                                "Attiva sincronizzazione preferiti e raccolte",
                                "Enable favorites and collections sync",
                            )
                        } else {
                            label(
                                "Disattiva sincronizzazione preferiti e raccolte",
                                "Disable favorites and collections sync",
                            )
                        },
                    ),
                    (
                        4,
                        label("Scollega questo dispositivo", "Disconnect this device"),
                    ),
                ],
            )
        } else if page == 2 {
            (
                label(
                    "Inserisci il codice di 8 cifre generato sull’altro dispositivo.",
                    "Enter the 8-digit code generated on your other device.",
                ),
                vec![
                    (
                        9,
                        label("Collega questo dispositivo", "Connect this device"),
                    ),
                    (8, label("Indietro", "Back")),
                ],
            )
        } else if connected {
            (
                label(
                    "Questo dispositivo è collegato. Scegli Sincronizza ora per aggiornare preferiti e preferenze. Per collegare un altro dispositivo, genera un codice.",
                    "This device is connected. Choose Sync now to update favorites and preferences. To connect another device, generate a code.",
                ),
                vec![
                    (2, label("Sincronizza ora", "Sync now")),
                    (
                        3,
                        label("Collega un altro dispositivo", "Connect another device"),
                    ),
                    (7, label("Opzioni di sincronizzazione", "Sync options")),
                ],
            )
        } else {
            (
                label(
                    "È il primo dispositivo? Scegli Inizia da questo dispositivo: riceverai un codice da inserire sugli altri. Se hai già un codice, scegli Ho già un codice.",
                    "Is this your first device? Choose Start on this device to receive a code for your other devices. If you already have a code, choose I already have a code.",
                ),
                vec![
                    (
                        0,
                        label("Inizia da questo dispositivo", "Start on this device"),
                    ),
                    (1, label("Ho già un codice", "I already have a code")),
                ],
            )
        };
        let help = if connected && page == 0 {
            status.as_str()
        } else {
            help
        };
        let dialog = Dialog::builder(parent, title).with_size(620, 340).build();
        let panel = Panel::builder(&dialog).build();
        let root = BoxSizer::builder(Orientation::Vertical).build();
        let description = StaticText::builder(&panel).with_label(help).build();
        description.wrap(570);
        root.add(&description, 0, SizerFlag::All, 10);
        let code = if page == 2 {
            let text = StaticText::builder(&panel)
                .with_label(label(
                    "Codice di collegamento (8 cifre)",
                    "Connection code (8 digits)",
                ))
                .build();
            root.add(&text, 0, SizerFlag::All, 5);
            let entry = TextCtrl::builder(&panel).build();
            entry.set_accessibility_label(label(
                "Codice di collegamento (8 cifre)",
                "Connection code (8 digits)",
            ));
            root.add(&entry, 0, SizerFlag::Expand | SizerFlag::All, 8);
            Some(entry)
        } else {
            None
        };
        for (index, (action, name)) in entries.into_iter().enumerate() {
            let button = Button::builder(&panel).with_label(name).build();
            root.add(&button, 0, SizerFlag::Expand | SizerFlag::All, 5);
            if index == 0 {
                button.set_default();
                button.set_focus();
            }
            let settings = Arc::clone(settings);
            button.on_click(move |_| {
                match action {1=>{dialog.end_modal(2002);return;},7=>{dialog.end_modal(2001);return;},8=>{dialog.end_modal(2000);return;},_=>{}}
                if action==4 && !ask_yes_no_dialog(&dialog,title,label("Scollegare? I preferiti rimangono sul dispositivo.","Disconnect? Favorites stay on this device.")){return;}
                let code=code.map(|v|v.get_value()).unwrap_or_default();
                let action=if action==9 {1} else {action};
                if action==1 && (code.trim().len()!=8 || !code.trim().bytes().all(|b|b.is_ascii_digit())) {
                    show_message_subdialog(&dialog,title,label("Inserisci un codice di 8 cifre.","Enter an 8-digit code."));return;
                }
                match run(&dialog,action,code,&settings) {
                    Ok(message)=>{
                        if action == 0 || action == 3 { show_text_preview_dialog(&dialog,title,&message); }
                        else { show_message_subdialog(&dialog,title,&message); }
                    },
                    Err(error)=>{
                        append_podcast_log(&format!("CloudSync completion error={error}"));
                        show_message_subdialog(&dialog,title,label("Operazione non riuscita. Controlla connessione e codice e riprova. I dati locali sono conservati.","Operation failed. Check your connection and code and retry. Local data is preserved."));
                    }
                }
                dialog.end_modal(if action>=5 {2001} else {2000});
            });
        }
        let close = Button::builder(&panel)
            .with_id(ID_CANCEL)
            .with_label(&current_ui_strings().cancel)
            .build();
        root.add(&close, 0, SizerFlag::All, 8);
        close.on_click(move |_| dialog.end_modal(ID_CANCEL));
        panel.set_sizer(root, true);
        dialog.set_escape_id(ID_CANCEL);
        dialog.centre();
        if let Some(code) = code {
            code.set_focus();
        }
        let result = dialog.show_modal();
        dialog.destroy();
        match result {
            2000 => page = 0,
            2001 => page = 1,
            2002 => page = 2,
            _ => break,
        }
    }
}

fn run(
    parent: &Dialog,
    action: usize,
    code: String,
    settings: &Arc<Mutex<Settings>>,
) -> Result<String, String> {
    if BUSY.swap(true, Ordering::SeqCst) {
        return Err("busy".into());
    }
    let _guard = FlagGuard(&BUSY);
    let before = snapshot_for_sync()?;
    let dir = app_storage_path("cloud_sync.json")
        .parent()
        .ok_or("storage_error")?
        .to_path_buf();
    let progress = Dialog::builder(parent, label("Sincronizzazione in corso…", "Syncing…"))
        .with_style(DialogStyle::Caption)
        .with_size(420, 120)
        .build();
    progress.on_close(|event| event.skip(false));
    let panel = Panel::builder(&progress).build();
    let label_widget = StaticText::builder(&panel)
        .with_label(label("Attendere…", "Please wait…"))
        .build();
    let root = BoxSizer::builder(Orientation::Vertical).build();
    root.add(&label_widget, 1, SizerFlag::All, 15);
    panel.set_sizer(root, true);
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result = request(action, code, before, dir);
        let _ = sender.send(result);
    });
    let result = Rc::new(RefCell::new(None));
    let completed = Rc::clone(&result);
    let timer = Timer::new(&progress);
    timer.on_tick(move |_| match receiver.try_recv() {
        Ok(value) => {
            *completed.borrow_mut() = Some(value);
            progress.end_modal(ID_OK);
        }
        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
            progress.end_modal(ID_CANCEL);
        }
        Err(std::sync::mpsc::TryRecvError::Empty) => {}
    });
    timer.start(100, false);
    progress.centre();
    progress.show_modal();
    timer.stop();
    progress.destroy();
    let outcome = result.borrow_mut().take().ok_or("network_error")?;
    let result = outcome.and_then(|outcome| finish(settings, outcome));
    if action <= 2 {
        if let Ok(mut scheduler) = scheduler().lock() {
            scheduler.completed(result.is_ok(), cloud_sync::now_seconds());
        }
    }
    if result
        .as_ref()
        .is_err_and(|error| error == "deletion_confirmation_required")
    {
        return Err(label("Rimozione inattesa bloccata. Nessuna eliminazione propagata senza un comando esplicito.", "Unexpected removal blocked. Deletions require an explicit removal command.").into());
    }
    let (session, invitation) = result?;
    Ok(if let Some(code) = invitation {
        format!(
            "{}\n{code}\n{}",
            label("Codice di collegamento:", "Connection code:"),
            label(
                "Sull’altro dispositivo apri Sincronizzazione cloud, scegli Ho già un codice e inserisci questo codice. È monouso e vale 10 minuti.",
                "On your other device, open Cloud sync, choose I already have a code and enter this code. Single-use, valid for 10 minutes."
            )
        )
    } else if action == 4 {
        label(
            "Dispositivo scollegato. I dati locali sono conservati.",
            "Device disconnected. Local data is preserved.",
        )
        .into()
    } else if action < 5 {
        label("Sincronizzazione completata.", "Sync completed.").into()
    } else {
        format!(
            "{}\n{}: {}\n{}: {}",
            label("Operazione completata.", "Operation completed."),
            label("Preferenze", "Preferences"),
            if session.meta["sync_preferences"] == json!(false) {
                label("Disattivate", "Off")
            } else {
                label("Attivate", "On")
            },
            label("Raccolte", "Collections"),
            if session.meta["sync_collections"] == json!(false) {
                label("Disattivate", "Off")
            } else {
                label("Attivate", "On")
            }
        )
    })
}

fn request(action: usize, code: String, before: Value, dir: PathBuf) -> Result<Outcome, String> {
    let mut session = Session::load(&dir)?;
    if action >= 5 {
        let enabled = session.toggle_scope(action == 5)?;
        append_podcast_log(if enabled {
            "CloudSync scope enabled"
        } else {
            "CloudSync scope disabled"
        });
        return Ok((session, None, None));
    }
    if action == 4 {
        session.disconnect(append_podcast_log)?;
        return Ok((session, None, None));
    }
    if action <= 1 {
        session.connect(
            if action == 1 {
                Some(code.as_str())
            } else {
                None
            },
            append_podcast_log,
        )?;
    }
    if action == 3 {
        let r = session.request("code", &json!({}), append_podcast_log)?;
        return Ok((
            session,
            None,
            Some(cloud_sync::text(&r, "code").to_string()),
        ));
    }
    let remote = session.sync(&before, true, append_podcast_log)?;
    let invitation = if action == 0 {
        Some(
            cloud_sync::text(
                &session.request("code", &json!({}), append_podcast_log)?,
                "code",
            )
            .to_string(),
        )
    } else {
        None
    };
    Ok((session, Some(remote), invitation))
}
fn finish(
    settings: &Arc<Mutex<Settings>>,
    outcome: Outcome,
) -> Result<(Session, Option<String>), String> {
    let (mut session, remote, invitation) = outcome;
    if let Some(mut remote) = remote {
        let mut locked = settings.lock().map_err(|_| "storage_error")?;
        let mut current = snapshot_for_sync()?;
        let merged = session.merge_response(
            &remote,
            &session.select(&cloud_sync::export(&current, true)),
        )?;
        let old_language = cloud_sync::language(&current, true);
        cloud_sync::apply(&mut current, &merged, true)?;
        let mut updated: Settings =
            serde_json::from_value(current).map_err(|_| "invalid_response")?;
        if old_language != updated.news_language {
            updated.article_sources = replace_default_article_sources_for_news_language(
                &updated.article_sources,
                &updated.news_language,
            );
            if session.meta["pending"]["state"]
                .get("news_sources")
                .is_some()
            {
                let mut current = snapshot(&updated)?;
                cloud_sync::reconcile_news_language(&session, &mut current, &mut remote, true)?;
                updated = serde_json::from_value(current).map_err(|_| "invalid_response")?;
            }
        }
        // Use the existing storage path and propagate write failures before the checkpoint.
        write_app_storage_text(
            "settings.json",
            &serde_json::to_string_pretty(&updated).map_err(|_| "storage_error")?,
        )
        .map_err(|_| "storage_error")?;
        let persisted = snapshot_for_sync()?;
        if cloud_sync::export(&persisted, true) != cloud_sync::export(&snapshot(&updated)?, true) {
            return Err("storage_error".into());
        }
        cloud_sync::log_collection_reductions(
            &session.select(&cloud_sync::export(&snapshot(&locked)?, true)),
            &session.select(&cloud_sync::export(&persisted, true)),
            "write_verified",
            crate::append_podcast_log,
        );
        *locked = updated;
        session.finish(remote)?;
    }
    Ok((session, invitation))
}

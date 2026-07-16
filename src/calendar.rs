use chrono::{Datelike, Duration, Local, NaiveDate, NaiveDateTime, Timelike, Weekday};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
#[cfg(target_os = "macos")]
use std::process::Command;
use std::sync::OnceLock;
use uuid::Uuid;

const REMINDERS_FILE: &str = "calendar_reminders.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CalendarReminder {
    pub(crate) id: String,
    pub(crate) date: String,
    pub(crate) text: String,
    #[serde(default)]
    pub(crate) has_time: bool,
    #[serde(default)]
    pub(crate) hour: u32,
    #[serde(default)]
    pub(crate) minute: u32,
    #[serde(default)]
    pub(crate) alert_minutes: u32,
    #[serde(default)]
    pub(crate) mac_calendar_uid: Option<String>,
    #[serde(default)]
    pub(crate) completed: bool,
    #[serde(default)]
    pub(crate) alerted: bool,
    #[serde(default)]
    pub(crate) snoozed_until: Option<String>,
}

#[derive(Deserialize)]
struct CalendarData {
    saints: HashMap<String, HashMap<String, String>>,
    quotes: HashMap<String, Vec<String>>,
}

#[derive(Clone, Copy)]
pub(crate) struct CalendarLabels {
    pub(crate) menu: &'static str,
    pub(crate) title: &'static str,
    pub(crate) day: &'static str,
    pub(crate) details: &'static str,
    pub(crate) today: &'static str,
    pub(crate) add_reminder: &'static str,
    pub(crate) open_system_calendar: &'static str,
    pub(crate) close: &'static str,
    pub(crate) reminder_title: &'static str,
    pub(crate) reminder_text: &'static str,
    pub(crate) time_mode: &'static str,
    pub(crate) no_time: &'static str,
    pub(crate) with_time: &'static str,
    pub(crate) hour: &'static str,
    pub(crate) minute: &'static str,
    pub(crate) alert: &'static str,
    pub(crate) alert_at_time: &'static str,
    pub(crate) alert_5_minutes: &'static str,
    pub(crate) alert_15_minutes: &'static str,
    pub(crate) alert_30_minutes: &'static str,
    pub(crate) alert_1_hour: &'static str,
    pub(crate) alert_1_day: &'static str,
    pub(crate) save: &'static str,
    pub(crate) cancel: &'static str,
    pub(crate) empty_text: &'static str,
    pub(crate) invalid_time: &'static str,
    pub(crate) saved: &'static str,
    pub(crate) saved_local_only: &'static str,
    pub(crate) holiday: &'static str,
    pub(crate) saint: &'static str,
    pub(crate) quote: &'static str,
    pub(crate) reminders: &'static str,
    pub(crate) no_reminders: &'static str,
    pub(crate) not_available: &'static str,
    pub(crate) system_calendar_error: &'static str,
}

pub(crate) fn labels(language: &str) -> CalendarLabels {
    match language {
        "it" => CalendarLabels {
            menu: "Calendario",
            title: "Calendario",
            day: "Giorno",
            details: "Dettagli del giorno",
            today: "Oggi",
            add_reminder: "Aggiungi promemoria",
            open_system_calendar: "Apri Calendario del Mac",
            close: "Chiudi",
            reminder_title: "Nuovo promemoria",
            reminder_text: "Testo del promemoria",
            time_mode: "Orario",
            no_time: "Senza orario",
            with_time: "Imposta un orario",
            hour: "Ora",
            minute: "Minuti",
            alert: "Avviso",
            alert_at_time: "All'orario dell'evento",
            alert_5_minutes: "5 minuti prima",
            alert_15_minutes: "15 minuti prima",
            alert_30_minutes: "30 minuti prima",
            alert_1_hour: "1 ora prima",
            alert_1_day: "1 giorno prima",
            save: "Salva",
            cancel: "Annulla",
            empty_text: "Scrivi il testo del promemoria.",
            invalid_time: "L'orario selezionato non è valido.",
            saved: "Promemoria salvato e aggiunto al Calendario del Mac.",
            saved_local_only: "Promemoria salvato in Sonarpad, ma non è stato possibile aggiungerlo al Calendario del Mac: {err}",
            holiday: "Festività",
            saint: "Santo del giorno",
            quote: "Citazione del giorno",
            reminders: "Promemoria",
            no_reminders: "Non ci sono promemoria",
            not_available: "Non disponibile",
            system_calendar_error: "Impossibile aprire il Calendario del Mac: {err}",
        },
        "fr" => CalendarLabels {
            menu: "Calendrier",
            title: "Calendrier",
            day: "Jour",
            details: "Détails du jour",
            today: "Aujourd'hui",
            add_reminder: "Ajouter un rappel",
            open_system_calendar: "Ouvrir Calendrier sur le Mac",
            close: "Fermer",
            reminder_title: "Nouveau rappel",
            reminder_text: "Texte du rappel",
            time_mode: "Heure",
            no_time: "Sans heure",
            with_time: "Définir une heure",
            hour: "Heure",
            minute: "Minutes",
            alert: "Alerte",
            alert_at_time: "À l'heure de l'événement",
            alert_5_minutes: "5 minutes avant",
            alert_15_minutes: "15 minutes avant",
            alert_30_minutes: "30 minutes avant",
            alert_1_hour: "1 heure avant",
            alert_1_day: "1 jour avant",
            save: "Enregistrer",
            cancel: "Annuler",
            empty_text: "Saisissez le texte du rappel.",
            invalid_time: "L'heure sélectionnée n'est pas valide.",
            saved: "Rappel enregistré et ajouté au Calendrier du Mac.",
            saved_local_only: "Rappel enregistré dans Sonarpad, mais impossible de l'ajouter au Calendrier du Mac : {err}",
            holiday: "Jour férié",
            saint: "Saint du jour",
            quote: "Citation du jour",
            reminders: "Rappels",
            no_reminders: "Aucun rappel",
            not_available: "Non disponible",
            system_calendar_error: "Impossible d'ouvrir Calendrier sur le Mac : {err}",
        },
        "es" => CalendarLabels {
            menu: "Calendario",
            title: "Calendario",
            day: "Día",
            details: "Detalles del día",
            today: "Hoy",
            add_reminder: "Añadir recordatorio",
            open_system_calendar: "Abrir Calendario del Mac",
            close: "Cerrar",
            reminder_title: "Nuevo recordatorio",
            reminder_text: "Texto del recordatorio",
            time_mode: "Hora",
            no_time: "Sin hora",
            with_time: "Establecer una hora",
            hour: "Hora",
            minute: "Minutos",
            alert: "Aviso",
            alert_at_time: "A la hora del evento",
            alert_5_minutes: "5 minutos antes",
            alert_15_minutes: "15 minutos antes",
            alert_30_minutes: "30 minutos antes",
            alert_1_hour: "1 hora antes",
            alert_1_day: "1 día antes",
            save: "Guardar",
            cancel: "Cancelar",
            empty_text: "Escribe el texto del recordatorio.",
            invalid_time: "La hora seleccionada no es válida.",
            saved: "Recordatorio guardado y añadido al Calendario del Mac.",
            saved_local_only: "Recordatorio guardado en Sonarpad, pero no se pudo añadir al Calendario del Mac: {err}",
            holiday: "Festivo",
            saint: "Santo del día",
            quote: "Cita del día",
            reminders: "Recordatorios",
            no_reminders: "No hay recordatorios",
            not_available: "No disponible",
            system_calendar_error: "No se pudo abrir Calendario del Mac: {err}",
        },
        "pt" => CalendarLabels {
            menu: "Calendário",
            title: "Calendário",
            day: "Dia",
            details: "Detalhes do dia",
            today: "Hoje",
            add_reminder: "Adicionar lembrete",
            open_system_calendar: "Abrir Calendário do Mac",
            close: "Fechar",
            reminder_title: "Novo lembrete",
            reminder_text: "Texto do lembrete",
            time_mode: "Hora",
            no_time: "Sem hora",
            with_time: "Definir uma hora",
            hour: "Hora",
            minute: "Minutos",
            alert: "Aviso",
            alert_at_time: "À hora do evento",
            alert_5_minutes: "5 minutos antes",
            alert_15_minutes: "15 minutos antes",
            alert_30_minutes: "30 minutos antes",
            alert_1_hour: "1 hora antes",
            alert_1_day: "1 dia antes",
            save: "Guardar",
            cancel: "Cancelar",
            empty_text: "Escreva o texto do lembrete.",
            invalid_time: "A hora selecionada não é válida.",
            saved: "Lembrete guardado e adicionado ao Calendário do Mac.",
            saved_local_only: "Lembrete guardado no Sonarpad, mas não foi possível adicioná-lo ao Calendário do Mac: {err}",
            holiday: "Feriado",
            saint: "Santo do dia",
            quote: "Citação do dia",
            reminders: "Lembretes",
            no_reminders: "Não há lembretes",
            not_available: "Não disponível",
            system_calendar_error: "Não foi possível abrir o Calendário do Mac: {err}",
        },
        "cs" => CalendarLabels {
            menu: "Kalendář",
            title: "Kalendář",
            day: "Den",
            details: "Podrobnosti dne",
            today: "Dnes",
            add_reminder: "Přidat připomínku",
            open_system_calendar: "Otevřít Kalendář na Macu",
            close: "Zavřít",
            reminder_title: "Nová připomínka",
            reminder_text: "Text připomínky",
            time_mode: "Čas",
            no_time: "Bez času",
            with_time: "Nastavit čas",
            hour: "Hodina",
            minute: "Minuty",
            alert: "Upozornění",
            alert_at_time: "V čase události",
            alert_5_minutes: "5 minut předem",
            alert_15_minutes: "15 minut předem",
            alert_30_minutes: "30 minut předem",
            alert_1_hour: "1 hodinu předem",
            alert_1_day: "1 den předem",
            save: "Uložit",
            cancel: "Zrušit",
            empty_text: "Zadejte text připomínky.",
            invalid_time: "Vybraný čas není platný.",
            saved: "Připomínka byla uložena a přidána do Kalendáře na Macu.",
            saved_local_only: "Připomínka byla uložena v Sonarpadu, ale nešlo ji přidat do Kalendáře na Macu: {err}",
            holiday: "Svátek",
            saint: "Světec dne",
            quote: "Citát dne",
            reminders: "Připomínky",
            no_reminders: "Žádné připomínky",
            not_available: "Není k dispozici",
            system_calendar_error: "Kalendář na Macu nelze otevřít: {err}",
        },
        "pl" => CalendarLabels {
            menu: "Kalendarz",
            title: "Kalendarz",
            day: "Dzień",
            details: "Szczegóły dnia",
            today: "Dzisiaj",
            add_reminder: "Dodaj przypomnienie",
            open_system_calendar: "Otwórz Kalendarz na Macu",
            close: "Zamknij",
            reminder_title: "Nowe przypomnienie",
            reminder_text: "Treść przypomnienia",
            time_mode: "Godzina",
            no_time: "Bez godziny",
            with_time: "Ustaw godzinę",
            hour: "Godzina",
            minute: "Minuty",
            alert: "Alert",
            alert_at_time: "O godzinie wydarzenia",
            alert_5_minutes: "5 minut wcześniej",
            alert_15_minutes: "15 minut wcześniej",
            alert_30_minutes: "30 minut wcześniej",
            alert_1_hour: "1 godzinę wcześniej",
            alert_1_day: "1 dzień wcześniej",
            save: "Zapisz",
            cancel: "Anuluj",
            empty_text: "Wpisz treść przypomnienia.",
            invalid_time: "Wybrana godzina jest nieprawidłowa.",
            saved: "Przypomnienie zapisano i dodano do Kalendarza na Macu.",
            saved_local_only: "Przypomnienie zapisano w Sonarpadzie, ale nie udało się dodać go do Kalendarza na Macu: {err}",
            holiday: "Święto",
            saint: "Święty dnia",
            quote: "Cytat dnia",
            reminders: "Przypomnienia",
            no_reminders: "Brak przypomnień",
            not_available: "Niedostępne",
            system_calendar_error: "Nie można otworzyć Kalendarza na Macu: {err}",
        },
        _ => CalendarLabels {
            menu: "Calendar",
            title: "Calendar",
            day: "Day",
            details: "Day details",
            today: "Today",
            add_reminder: "Add reminder",
            open_system_calendar: "Open Mac Calendar",
            close: "Close",
            reminder_title: "New reminder",
            reminder_text: "Reminder text",
            time_mode: "Time",
            no_time: "No time",
            with_time: "Set a time",
            hour: "Hour",
            minute: "Minutes",
            alert: "Alert",
            alert_at_time: "At event time",
            alert_5_minutes: "5 minutes before",
            alert_15_minutes: "15 minutes before",
            alert_30_minutes: "30 minutes before",
            alert_1_hour: "1 hour before",
            alert_1_day: "1 day before",
            save: "Save",
            cancel: "Cancel",
            empty_text: "Enter the reminder text.",
            invalid_time: "The selected time is invalid.",
            saved: "Reminder saved and added to Mac Calendar.",
            saved_local_only: "Reminder saved in Sonarpad, but it could not be added to Mac Calendar: {err}",
            holiday: "Holiday",
            saint: "Saint of the day",
            quote: "Quote of the day",
            reminders: "Reminders",
            no_reminders: "No reminders",
            not_available: "Not available",
            system_calendar_error: "Could not open Mac Calendar: {err}",
        },
    }
}

pub(crate) fn load_reminders() -> Vec<CalendarReminder> {
    let Some(data) = crate::read_app_storage_text(REMINDERS_FILE) else {
        return Vec::new();
    };
    serde_json::from_str::<Vec<CalendarReminder>>(&data).unwrap_or_default()
}

fn save_reminders(reminders: &[CalendarReminder]) -> Result<(), String> {
    let data = serde_json::to_string_pretty(reminders)
        .map_err(|error| format!("serializzazione promemoria fallita: {error}"))?;
    crate::write_app_storage_text(REMINDERS_FILE, &data)
}

pub(crate) fn add_reminder(
    date: NaiveDate,
    text: String,
    has_time: bool,
    hour: u32,
    minute: u32,
    alert_minutes: u32,
) -> Result<(CalendarReminder, Option<String>), String> {
    let mut reminder = CalendarReminder {
        id: Uuid::new_v4().to_string(),
        date: date.format("%Y-%m-%d").to_string(),
        text,
        has_time,
        hour,
        minute,
        alert_minutes,
        mac_calendar_uid: None,
        completed: false,
        alerted: false,
        snoozed_until: None,
    };

    let mut reminders = load_reminders();
    reminders.push(reminder.clone());
    save_reminders(&reminders)?;

    let internal_warning = schedule_internal_alert(&reminder).err();
    match add_to_macos_calendar(&reminder) {
        Ok(uid) => {
            reminder.mac_calendar_uid = uid;
            if let Some(saved) = reminders.iter_mut().find(|item| item.id == reminder.id) {
                saved.mac_calendar_uid = reminder.mac_calendar_uid.clone();
            }
            save_reminders(&reminders)?;
            Ok((reminder, internal_warning))
        }
        Err(error) => Ok((
            reminder,
            Some(match internal_warning {
                Some(internal) => format!("{error}; avviso Sonarpad: {internal}"),
                None => error,
            }),
        )),
    }
}

pub(crate) fn take_due_reminders() -> Vec<CalendarReminder> {
    let now = Local::now().naive_local();
    let grace_start = now - Duration::hours(24);
    let mut reminders = load_reminders();
    let mut due = Vec::new();

    for reminder in &mut reminders {
        if reminder.completed || reminder.alerted || !reminder.has_time {
            continue;
        }
        let Some(alert_at) = reminder_alert_datetime(reminder) else {
            continue;
        };
        if alert_at <= now && alert_at >= grace_start {
            reminder.alerted = true;
            reminder.snoozed_until = None;
            due.push(reminder.clone());
        }
    }

    if !due.is_empty() {
        if let Err(error) = save_reminders(&reminders) {
            crate::append_podcast_log(&format!(
                "calendar.reminder.save_alerted_failed err={error}"
            ));
        }
        for reminder in &due {
            remove_internal_alert(&reminder.id);
            crate::append_podcast_log(&format!(
                "calendar.reminder.due id={} text={} date={} time={:02}:{:02}",
                reminder.id, reminder.text, reminder.date, reminder.hour, reminder.minute
            ));
        }
    }
    due
}

pub(crate) fn initialize_internal_alerts() {
    let now = Local::now().naive_local();
    for reminder in load_reminders() {
        if reminder.completed || reminder.alerted || !reminder.has_time {
            continue;
        }
        if reminder_alert_datetime(&reminder).is_some_and(|alert_at| alert_at > now)
            && let Err(error) = schedule_internal_alert(&reminder)
        {
            crate::append_podcast_log(&format!(
                "calendar.reminder.initialize_failed id={} err={error}",
                reminder.id
            ));
        }
    }
}

pub(crate) fn complete_reminder(id: &str) -> Result<(), String> {
    let mut reminders = load_reminders();
    let reminder = reminders
        .iter_mut()
        .find(|reminder| reminder.id == id)
        .ok_or_else(|| "Promemoria non trovato.".to_string())?;
    reminder.completed = true;
    reminder.alerted = true;
    reminder.snoozed_until = None;
    save_reminders(&reminders)?;
    remove_internal_alert(id);
    crate::append_podcast_log(&format!("calendar.reminder.completed id={id}"));
    Ok(())
}

pub(crate) fn snooze_reminder(id: &str, minutes: i64) -> Result<(), String> {
    if minutes <= 0 {
        return Err("La durata del posticipo non è valida.".to_string());
    }
    let mut reminders = load_reminders();
    let reminder = reminders
        .iter_mut()
        .find(|reminder| reminder.id == id)
        .ok_or_else(|| "Promemoria non trovato.".to_string())?;
    let snoozed_until = Local::now().naive_local() + Duration::minutes(minutes);
    reminder.completed = false;
    reminder.alerted = false;
    reminder.snoozed_until = Some(snoozed_until.format("%Y-%m-%d %H:%M:%S").to_string());
    let reminder_to_schedule = reminder.clone();
    save_reminders(&reminders)?;
    schedule_internal_alert(&reminder_to_schedule)?;
    crate::append_podcast_log(&format!(
        "calendar.reminder.snoozed id={id} until={snoozed_until}"
    ));
    Ok(())
}

fn reminder_event_datetime(reminder: &CalendarReminder) -> Option<NaiveDateTime> {
    if !reminder.has_time {
        return None;
    }
    let date = NaiveDate::parse_from_str(&reminder.date, "%Y-%m-%d").ok()?;
    date.and_hms_opt(reminder.hour, reminder.minute, 0)
}

fn reminder_alert_datetime(reminder: &CalendarReminder) -> Option<NaiveDateTime> {
    if let Some(value) = reminder.snoozed_until.as_deref()
        && let Ok(value) = NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S")
    {
        return Some(value);
    }
    reminder_event_datetime(reminder)
        .map(|event_at| event_at - Duration::minutes(i64::from(reminder.alert_minutes)))
}

#[cfg(target_os = "macos")]
fn schedule_internal_alert(reminder: &CalendarReminder) -> Result<(), String> {
    if reminder.completed || reminder.alerted || !reminder.has_time {
        remove_internal_alert(&reminder.id);
        return Ok(());
    }
    let Some(alert_at) = reminder_alert_datetime(reminder) else {
        return Err("Scadenza del promemoria non valida.".to_string());
    };
    if alert_at <= Local::now().naive_local() {
        return Ok(());
    }
    let executable = std::env::current_exe()
        .map_err(|error| format!("eseguibile Sonarpad non disponibile: {error}"))?;
    let app_bundle = executable
        .parent()
        .and_then(|macos| macos.parent())
        .and_then(|contents| contents.parent())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("app"))
        .ok_or_else(|| "bundle Sonarpad.app non disponibile".to_string())?;
    let plist_path = internal_alert_plist_path(&reminder.id)?;
    if plist_path.is_file() {
        let _ = Command::new("/bin/launchctl")
            .arg("unload")
            .arg(&plist_path)
            .status();
    }
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>Label</key><string>com.sonarpad.calendar.{id}</string>
<key>ProgramArguments</key><array><string>/usr/bin/open</string><string>-a</string><string>{app}</string></array>
<key>StartCalendarInterval</key><dict>
<key>Year</key><integer>{year}</integer><key>Month</key><integer>{month}</integer>
<key>Day</key><integer>{day}</integer><key>Hour</key><integer>{hour}</integer>
<key>Minute</key><integer>{minute}</integer>
</dict><key>ProcessType</key><string>Background</string>
</dict></plist>
"#,
        id = xml_escape(&reminder.id),
        app = xml_escape(&app_bundle.to_string_lossy()),
        year = alert_at.year(),
        month = alert_at.month(),
        day = alert_at.day(),
        hour = alert_at.hour(),
        minute = alert_at.minute(),
    );
    std::fs::write(&plist_path, plist)
        .map_err(|error| format!("scrittura avviso programmato fallita: {error}"))?;
    let status = Command::new("/bin/launchctl")
        .arg("load")
        .arg(&plist_path)
        .status()
        .map_err(|error| format!("attivazione avviso programmato fallita: {error}"))?;
    if !status.success() {
        return Err(format!("launchctl ha restituito lo stato {status}"));
    }
    crate::append_podcast_log(&format!(
        "calendar.reminder.scheduled id={} alert_at={} plist={}",
        reminder.id,
        alert_at,
        plist_path.display()
    ));
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn schedule_internal_alert(_reminder: &CalendarReminder) -> Result<(), String> {
    Ok(())
}

#[cfg(target_os = "macos")]
fn internal_alert_plist_path(id: &str) -> Result<std::path::PathBuf, String> {
    let home =
        std::env::var_os("HOME").ok_or_else(|| "Cartella Home non disponibile".to_string())?;
    let directory = std::path::PathBuf::from(home)
        .join("Library")
        .join("LaunchAgents");
    std::fs::create_dir_all(&directory)
        .map_err(|error| format!("creazione cartella LaunchAgents fallita: {error}"))?;
    Ok(directory.join(format!("com.sonarpad.calendar.{id}.plist")))
}

#[cfg(target_os = "macos")]
fn remove_internal_alert(id: &str) {
    let Ok(path) = internal_alert_plist_path(id) else {
        return;
    };
    if path.is_file() {
        let _ = Command::new("/bin/launchctl")
            .arg("unload")
            .arg(&path)
            .status();
        if let Err(error) = std::fs::remove_file(&path) {
            crate::append_podcast_log(&format!(
                "calendar.reminder.remove_plist_failed id={id} path={} err={error}",
                path.display()
            ));
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn remove_internal_alert(_id: &str) {}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

pub(crate) fn reminders_for_date(
    reminders: &[CalendarReminder],
    date: NaiveDate,
) -> Vec<CalendarReminder> {
    let key = date.format("%Y-%m-%d").to_string();
    let mut result = reminders
        .iter()
        .filter(|reminder| reminder.date == key && !reminder.completed)
        .cloned()
        .collect::<Vec<_>>();
    result.sort_by_key(|reminder| {
        if reminder.has_time {
            (0_u8, reminder.hour, reminder.minute)
        } else {
            (1_u8, 0, 0)
        }
    });
    result
}

pub(crate) fn build_day_details(language: &str, date: NaiveDate) -> String {
    let labels = labels(language);
    let reminders = load_reminders();
    let mut lines = vec![localized_date(language, date)];
    if let Some(holiday) = holiday_for_date(language, date) {
        lines.push(format!("{}: {}", labels.holiday, holiday));
    }
    let saint = saint_for_date(language, date).unwrap_or_else(|| labels.not_available.to_string());
    lines.push(format!("{}: {}", labels.saint, saint));
    lines.push(format!(
        "{}: {}",
        labels.quote,
        quote_for_date(language, date, labels.not_available)
    ));
    let day_reminders = reminders_for_date(&reminders, date);
    if day_reminders.is_empty() {
        lines.push(labels.no_reminders.to_string());
    } else {
        lines.push(format!("{}: {}", labels.reminders, day_reminders.len()));
        for reminder in day_reminders {
            if reminder.has_time {
                lines.push(format!(
                    "{:02}:{:02} - {}",
                    reminder.hour, reminder.minute, reminder.text
                ));
            } else {
                lines.push(reminder.text);
            }
        }
    }
    lines.join("\n")
}

pub(crate) fn localized_date(language: &str, date: NaiveDate) -> String {
    let (weekdays, months) = localized_names(language);
    let weekday = match date.weekday() {
        Weekday::Mon => 0,
        Weekday::Tue => 1,
        Weekday::Wed => 2,
        Weekday::Thu => 3,
        Weekday::Fri => 4,
        Weekday::Sat => 5,
        Weekday::Sun => 6,
    };
    format!(
        "{} {} {} {}",
        weekdays[weekday],
        date.day(),
        months[date.month0() as usize],
        date.year()
    )
}

pub(crate) fn open_system_calendar() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let status = Command::new("/usr/bin/open")
            .args(["-a", "Calendar"])
            .status()
            .map_err(|error| format!("avvio Calendario fallito: {error}"))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("Calendario ha restituito lo stato {status}"))
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err("Calendario di macOS non disponibile".to_string())
    }
}

#[cfg(target_os = "macos")]
fn add_to_macos_calendar(reminder: &CalendarReminder) -> Result<Option<String>, String> {
    let date = NaiveDate::parse_from_str(&reminder.date, "%Y-%m-%d")
        .map_err(|error| format!("data promemoria non valida: {error}"))?;
    let script = r#"
on run argv
    set eventTitle to item 1 of argv
    set eventYear to (item 2 of argv) as integer
    set eventMonth to (item 3 of argv) as integer
    set eventDay to (item 4 of argv) as integer
    set eventHour to (item 5 of argv) as integer
    set eventMinute to (item 6 of argv) as integer
    set hasTime to item 7 of argv
    set alertMinutes to (item 8 of argv) as integer
    tell application "Calendar"
        set targetCalendar to first calendar whose writable is true
        set startDate to current date
        set year of startDate to eventYear
        set month of startDate to eventMonth
        set day of startDate to eventDay
        set seconds of startDate to 0
        if hasTime is "true" then
            set hours of startDate to eventHour
            set minutes of startDate to eventMinute
            set endDate to startDate + 3600
            set newEvent to make new event at end of events of targetCalendar with properties {summary:eventTitle, start date:startDate, end date:endDate}
            make new display alarm at end of display alarms of newEvent with properties {trigger interval:-(alertMinutes * 60)}
        else
            set hours of startDate to 0
            set minutes of startDate to 0
            set endDate to startDate + 86400
            set newEvent to make new event at end of events of targetCalendar with properties {summary:eventTitle, start date:startDate, end date:endDate, allday event:true}
        end if
        return uid of newEvent
    end tell
end run
"#;
    let output = Command::new("/usr/bin/osascript")
        .arg("-")
        .arg(&reminder.text)
        .arg(date.year().to_string())
        .arg(date.month().to_string())
        .arg(date.day().to_string())
        .arg(reminder.hour.to_string())
        .arg(reminder.minute.to_string())
        .arg(if reminder.has_time { "true" } else { "false" })
        .arg(reminder.alert_minutes.to_string())
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            if let Some(stdin) = child.stdin.as_mut() {
                stdin.write_all(script.as_bytes())?;
            }
            child.wait_with_output()
        })
        .map_err(|error| format!("esecuzione AppleScript fallita: {error}"))?;
    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if error.is_empty() {
            format!("AppleScript ha restituito lo stato {}", output.status)
        } else {
            error
        });
    }
    let uid = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok((!uid.is_empty()).then_some(uid))
}

#[cfg(not(target_os = "macos"))]
fn add_to_macos_calendar(_reminder: &CalendarReminder) -> Result<Option<String>, String> {
    Ok(None)
}

fn calendar_data() -> &'static CalendarData {
    static DATA: OnceLock<CalendarData> = OnceLock::new();
    DATA.get_or_init(|| {
        serde_json::from_str(include_str!("calendar_data.json")).unwrap_or_else(|error| {
            eprintln!("Calendar data parse failed: {error}");
            CalendarData {
                saints: HashMap::new(),
                quotes: HashMap::new(),
            }
        })
    })
}

fn saint_for_date(language: &str, date: NaiveDate) -> Option<String> {
    let key = format!("{}-{}", date.day(), date.month());
    calendar_data().saints.get(&key)?.get(language).cloned()
}

fn quote_for_date(language: &str, date: NaiveDate, fallback: &str) -> String {
    let list = calendar_data().quotes.get(language);
    let Some(list) = list else {
        return fallback.to_string();
    };
    if list.is_empty() {
        return fallback.to_string();
    }
    let Some(epoch) = NaiveDate::from_ymd_opt(1970, 1, 1) else {
        return list[0].clone();
    };
    let index = date
        .signed_duration_since(epoch)
        .num_days()
        .rem_euclid(list.len() as i64) as usize;
    list.get(index)
        .cloned()
        .unwrap_or_else(|| fallback.to_string())
}

fn holiday_for_date(language: &str, date: NaiveDate) -> Option<String> {
    let value = match language {
        "it" => match (date.day(), date.month()) {
            (1, 1) => "Capodanno",
            (6, 1) => "Epifania",
            (25, 4) => "Festa della Liberazione",
            (1, 5) => "Festa dei Lavoratori",
            (2, 6) => "Festa della Repubblica",
            (15, 8) => "Ferragosto",
            (1, 11) => "Tutti i Santi",
            (8, 12) => "Immacolata Concezione",
            (25, 12) => "Natale",
            (26, 12) => "Santo Stefano",
            _ => return None,
        },
        "pt" => match (date.day(), date.month()) {
            (1, 1) => "Ano Novo",
            (6, 1) => "Epifania",
            (25, 4) => "Dia da Liberdade",
            (1, 5) => "Dia do Trabalhador",
            (10, 6) => "Dia de Portugal",
            (15, 8) => "Assunção de Nossa Senhora",
            (1, 11) => "Todos os Santos",
            (8, 12) => "Imaculada Conceição",
            (25, 12) => "Natal",
            _ => return None,
        },
        "pl" => match (date.day(), date.month()) {
            (1, 1) => "Nowy Rok",
            (6, 1) => "Święto Trzech Króli",
            (1, 5) => "Święto Pracy",
            (3, 5) => "Święto Konstytucji 3 Maja",
            (15, 8) => "Wniebowzięcie Najświętszej Maryi Panny",
            (1, 11) => "Wszystkich Świętych",
            (11, 11) => "Narodowe Święto Niepodległości",
            (25, 12) => "Boże Narodzenie",
            (26, 12) => "Drugi dzień Świąt Bożego Narodzenia",
            _ => return None,
        },
        "cs" => match (date.day(), date.month()) {
            (1, 1) => "Nový rok",
            (1, 5) => "Svátek práce",
            (8, 5) => "Den vítězství",
            (5, 7) => "Den slovanských věrozvěstů Cyrila a Metoděje",
            (6, 7) => "Den upálení mistra Jana Husa",
            (28, 9) => "Den české státnosti",
            (28, 10) => "Den vzniku samostatného československého státu",
            (17, 11) => "Den boje za svobodu a demokracii",
            (24, 12) => "Štědrý den",
            (25, 12) => "1. svátek vánoční",
            (26, 12) => "2. svátek vánoční",
            _ => return None,
        },
        _ => return None,
    };
    Some(value.to_string())
}

fn localized_names(language: &str) -> (&'static [&'static str; 7], &'static [&'static str; 12]) {
    match language {
        "it" => (
            &[
                "lunedì",
                "martedì",
                "mercoledì",
                "giovedì",
                "venerdì",
                "sabato",
                "domenica",
            ],
            &[
                "gennaio",
                "febbraio",
                "marzo",
                "aprile",
                "maggio",
                "giugno",
                "luglio",
                "agosto",
                "settembre",
                "ottobre",
                "novembre",
                "dicembre",
            ],
        ),
        "fr" => (
            &[
                "lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi", "dimanche",
            ],
            &[
                "janvier",
                "février",
                "mars",
                "avril",
                "mai",
                "juin",
                "juillet",
                "août",
                "septembre",
                "octobre",
                "novembre",
                "décembre",
            ],
        ),
        "es" => (
            &[
                "lunes",
                "martes",
                "miércoles",
                "jueves",
                "viernes",
                "sábado",
                "domingo",
            ],
            &[
                "enero",
                "febrero",
                "marzo",
                "abril",
                "mayo",
                "junio",
                "julio",
                "agosto",
                "septiembre",
                "octubre",
                "noviembre",
                "diciembre",
            ],
        ),
        "pt" => (
            &[
                "segunda-feira",
                "terça-feira",
                "quarta-feira",
                "quinta-feira",
                "sexta-feira",
                "sábado",
                "domingo",
            ],
            &[
                "janeiro",
                "fevereiro",
                "março",
                "abril",
                "maio",
                "junho",
                "julho",
                "agosto",
                "setembro",
                "outubro",
                "novembro",
                "dezembro",
            ],
        ),
        "cs" => (
            &[
                "pondělí",
                "úterý",
                "středa",
                "čtvrtek",
                "pátek",
                "sobota",
                "neděle",
            ],
            &[
                "leden",
                "únor",
                "březen",
                "duben",
                "květen",
                "červen",
                "červenec",
                "srpen",
                "září",
                "říjen",
                "listopad",
                "prosinec",
            ],
        ),
        "pl" => (
            &[
                "poniedziałek",
                "wtorek",
                "środa",
                "czwartek",
                "piątek",
                "sobota",
                "niedziela",
            ],
            &[
                "stycznia",
                "lutego",
                "marca",
                "kwietnia",
                "maja",
                "czerwca",
                "lipca",
                "sierpnia",
                "września",
                "października",
                "listopada",
                "grudnia",
            ],
        ),
        _ => (
            &[
                "Monday",
                "Tuesday",
                "Wednesday",
                "Thursday",
                "Friday",
                "Saturday",
                "Sunday",
            ],
            &[
                "January",
                "February",
                "March",
                "April",
                "May",
                "June",
                "July",
                "August",
                "September",
                "October",
                "November",
                "December",
            ],
        ),
    }
}

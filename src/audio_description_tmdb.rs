//! Optional cast lookup. All requests use the existing Sonarpad Cinema proxy.
use super::*;
use crate::audio_description_bridge::{BridgeCharacter, TmdbCharacterReference};
use serde_json::Value;

fn number(value: &Value, key: &str) -> i64 {
    value[key].as_i64().unwrap_or(0)
}
fn text(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap_or("").trim().to_string()
}
fn entries(value: &Value, key: &str) -> Vec<Value> {
    value[key].as_array().cloned().unwrap_or_default()
}

fn lookup(
    title: &str,
    year: &str,
    mut choose: impl FnMut(&str, Vec<String>) -> Result<usize, String>,
) -> Result<Vec<BridgeCharacter>, String> {
    let client = CinemaClient;
    let parameters = vec![("query", title.to_string()), ("year", year.to_string())];
    let mut productions = Vec::new();
    // A failed search must not silently omit potentially ambiguous matches.
    for kind in ["movie", "tv"] {
        let response = client.character_lookup(&format!("search_{kind}"), &parameters)?;
        for production in entries(&response, "results") {
            if number(&production, "id") > 0 {
                productions.push((kind, production));
            }
        }
    }
    if productions.is_empty() {
        return Err(tr("audio_description.tmdb.no_results"));
    }
    let options = productions
        .iter()
        .map(|(kind, item)| {
            let (name, date) = if *kind == "movie" {
                ("title", "release_date")
            } else {
                ("name", "first_air_date")
            };
            format!(
                "{} ({}) — {}",
                text(item, name),
                text(item, date).chars().take(4).collect::<String>(),
                tr(&format!("audio_description.tmdb.{kind}"))
            )
        })
        .collect();
    let selected = choose("production", options)?;
    let (kind, production) = productions
        .get(selected)
        .ok_or("TMDB selection cancelled")?;
    let id = number(production, "id");
    let mut parameters = vec![(
        if *kind == "movie" {
            "movie_id"
        } else {
            "series_id"
        },
        id.to_string(),
    )];
    let details = client.character_lookup(&format!("{kind}_details"), &parameters)?;
    let animation = entries(&details, "genres")
        .iter()
        .any(|genre| number(genre, "id") == 16);
    let production_title = text(&details, if *kind == "movie" { "title" } else { "name" });
    let mut season_number = None;
    let mut episode_number = None;
    let mut action = format!("{kind}_credits");
    if *kind == "tv" {
        let seasons = entries(&details, "seasons");
        if !seasons.is_empty() {
            let options = seasons
                .iter()
                .map(|season| format!("{} ({})", text(season, "name"), text(season, "air_date")))
                .collect();
            let index = choose("season", options)?;
            let season = seasons.get(index).ok_or("TMDB selection cancelled")?;
            let season_id = number(season, "season_number");
            season_number = Some(season_id);
            parameters.push(("season_number", season_id.to_string()));
            action = "season_credits".into();
            let details = client.character_lookup("season_details", &parameters)?;
            let episodes = entries(&details, "episodes");
            if !episodes.is_empty() {
                let mut options = vec![tr("audio_description.tmdb.whole_season")];
                options.extend(episodes.iter().map(|episode| {
                    format!(
                        "{} — {}",
                        number(episode, "episode_number"),
                        text(episode, "name")
                    )
                }));
                let index = choose("episode", options)?;
                if index > 0 {
                    let episode = episodes.get(index - 1).ok_or("TMDB selection cancelled")?;
                    let episode_id = number(episode, "episode_number");
                    episode_number = Some(episode_id);
                    parameters.push(("episode_number", episode_id.to_string()));
                    action = "episode_credits".into();
                }
            }
        }
    }
    let credits = client.character_lookup(&action, &parameters)?;
    let mut cast = entries(&credits, "cast");
    cast.extend(entries(&credits, "guest_stars"));
    let mut characters = Vec::new();
    for actor in cast {
        let mut roles = entries(&actor, "roles");
        if roles.is_empty() {
            roles.push(actor.clone());
        }
        for role in roles {
            let name = text(&role, "character");
            if name.is_empty() || number(&actor, "id") <= 0 {
                continue;
            }
            let credit_id = text(&role, "credit_id");
            let person_id = number(&actor, "id");
            let profile = text(&actor, "profile_path");
            let reference = TmdbCharacterReference {
                actor_name: text(&actor, "name"),
                person_id,
                credit_id: credit_id.clone(),
                production_id: id,
                production_title: production_title.clone(),
                production_year: text(
                    &details,
                    if *kind == "movie" {
                        "release_date"
                    } else {
                        "first_air_date"
                    },
                )
                .get(..4)
                .and_then(|year| year.parse().ok()),
                media_type: kind.to_string(),
                season_number,
                episode_number,
                animation,
                profile_image_url: (!animation
                    && profile.starts_with('/')
                    && !profile.contains(".."))
                .then(|| format!("https://image.tmdb.org/t/p/w185{profile}")),
            };
            let identifier = format!("tmdb_{id}_{person_id}_{credit_id}");
            if characters
                .iter()
                .any(|item: &BridgeCharacter| item.id == identifier)
            {
                continue;
            }
            characters.push(BridgeCharacter {
                id: identifier,
                name,
                description: format!(
                    "TMDB cast candidate: {}. Identity in the video is NOT verified.",
                    reference.actor_name
                ),
                tmdb: Some(reference),
                provisional: true,
            });
        }
    }
    characters.truncate(96);
    if characters.is_empty() {
        return Err(tr("audio_description.tmdb.no_cast"));
    }
    Ok(characters)
}

struct CinemaClient;
impl CinemaClient {
    fn character_lookup(
        &self,
        action: &str,
        parameters: &[(&str, String)],
    ) -> Result<Value, String> {
        crate::cinema_client()?
            .get(crate::cinema_api_url(action)?)
            .header("X-Sonarpad-Route-Token", crate::SONARPAD_ROUTE_CLIENT_TOKEN)
            .header("Accept", "application/json")
            .query(parameters)
            .send()
            .and_then(|response| response.error_for_status())
            .and_then(|response| response.json())
            .map_err(|error| format!("TMDB: {error}"))
    }
}

enum LookupEvent {
    Choice(String, Vec<String>, mpsc::SyncSender<Option<usize>>),
    Done(Result<Vec<BridgeCharacter>, String>),
}

fn choose(parent: &Dialog, key: &str, options: &[String]) -> Option<usize> {
    let label = tr(&format!("audio_description.tmdb.{key}"));
    let dialog = Dialog::builder(parent, &label).with_size(580, 200).build();
    let panel = Panel::builder(&dialog).build();
    let root = BoxSizer::builder(Orientation::Vertical).build();
    let choice = Choice::builder(&panel).build();
    choice.set_accessibility_label(&label);
    for option in options {
        choice.append(option);
    }
    choice.set_selection(0);
    root.add(&choice, 0, SizerFlag::Expand | SizerFlag::All, 8);
    let ok = Button::builder(&panel)
        .with_id(ID_OK)
        .with_label("OK")
        .build();
    let cancel = Button::builder(&panel)
        .with_id(ID_CANCEL)
        .with_label(&tr("audio_description.cancel"))
        .build();
    root.add(&ok, 0, SizerFlag::All, 8);
    root.add(&cancel, 0, SizerFlag::All, 8);
    panel.set_sizer(root, true);
    dialog.set_affirmative_id(ID_OK);
    dialog.set_escape_id(ID_CANCEL);
    ok.on_click(move |_| dialog.end_modal(ID_OK));
    cancel.on_click(move |_| dialog.end_modal(ID_CANCEL));
    choice.set_focus();
    let result = if dialog.show_modal() == ID_OK {
        choice.get_selection().map(|index| index as usize)
    } else {
        None
    };
    dialog.destroy();
    result
}

pub(super) fn resolve(
    parent: &Dialog,
    title: String,
    year: String,
) -> Result<Option<Vec<BridgeCharacter>>, String> {
    let dialog = Dialog::builder(parent, &tr("audio_description.tmdb.searching"))
        .with_size(580, 160)
        .build();
    let panel = Panel::builder(&dialog).build();
    let root = BoxSizer::builder(Orientation::Vertical).build();
    let status = StaticText::builder(&panel)
        .with_label(&tr("audio_description.tmdb.searching"))
        .build();
    root.add(&status, 0, SizerFlag::Expand | SizerFlag::All, 8);
    let cancel_button = Button::builder(&panel)
        .with_id(ID_CANCEL)
        .with_label(&tr("audio_description.cancel"))
        .build();
    root.add(&cancel_button, 0, SizerFlag::All, 8);
    panel.set_sizer(root, true);
    dialog.set_escape_id(ID_CANCEL);
    let cancelled = Arc::new(AtomicBool::new(false));
    let cancel_worker = cancelled.clone();
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let result = lookup(&title, &year, |key, options| {
            if cancel_worker.load(Ordering::Relaxed) {
                return Err("cancelled".into());
            }
            let (response, selected) = mpsc::sync_channel(1);
            sender
                .send(LookupEvent::Choice(key.to_string(), options, response))
                .map_err(|_| "cancelled")?;
            loop {
                if cancel_worker.load(Ordering::Relaxed) {
                    return Err("cancelled".into());
                }
                match selected.recv_timeout(Duration::from_millis(200)) {
                    Ok(Some(index)) => return Ok(index),
                    Ok(None) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                        cancel_worker.store(true, Ordering::Relaxed);
                        return Err("cancelled".into());
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                }
            }
        });
        let _ = sender.send(LookupEvent::Done(result));
    });
    let result = Rc::new(RefCell::new(None));
    let result_tick = result.clone();
    let timer = Timer::new(&dialog);
    timer.on_tick(move |_| {
        while let Ok(event) = receiver.try_recv() {
            match event {
                LookupEvent::Choice(key, options, response) => {
                    let _ = response.send(choose(&dialog, &key, &options));
                }
                LookupEvent::Done(value) => {
                    *result_tick.borrow_mut() = Some(value);
                    dialog.end_modal(ID_OK);
                }
            }
        }
    });
    cancel_button.on_click(move |_| dialog.end_modal(ID_CANCEL));
    timer.start(150, false);
    dialog.show_modal();
    timer.stop();
    dialog.destroy();
    let was_cancelled = cancelled.swap(true, Ordering::Relaxed);
    if was_cancelled {
        return Ok(None);
    }
    let value = result.borrow_mut().take();
    match value {
        Some(value) => value.map(Some),
        None => Ok(None),
    }
}

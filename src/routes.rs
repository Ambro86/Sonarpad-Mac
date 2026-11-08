#[path = "route_history.rs"]
mod route_history;

use crate::{SONARPAD_ROUTE_CLIENT_TOKEN, Settings, current_ui_strings as ui_strings};
use serde::Deserialize;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use wxdragon::*;

#[derive(Clone, Deserialize)]
struct GeocodeCandidate {
    #[allow(dead_code)]
    label: Option<String>,
    #[allow(dead_code)]
    name: Option<String>,
    #[allow(dead_code)]
    country: Option<String>,
    #[allow(dead_code)]
    region: Option<String>,
    #[allow(dead_code)]
    locality: Option<String>,
    #[allow(dead_code)]
    postalcode: Option<String>,
    latitude: Option<f64>,
    longitude: Option<f64>,
}

impl GeocodeCandidate {
    fn display_label(&self) -> String {
        if let Some(lbl) = &self.label
            && !lbl.trim().is_empty()
        {
            return lbl.clone();
        }
        let mut parts = Vec::new();
        if let Some(n) = &self.name
            && !n.trim().is_empty()
        {
            parts.push(n.clone());
        }
        if let Some(l) = &self.locality
            && !l.trim().is_empty()
        {
            parts.push(l.clone());
        }
        if let Some(c) = &self.country
            && !c.trim().is_empty()
        {
            parts.push(c.clone());
        }
        if parts.is_empty() {
            "Unknown".to_string()
        } else {
            parts.join(", ")
        }
    }
}

#[derive(Clone, Deserialize)]
struct RouteStep {
    instruction: Option<String>,
    distance_meters: Option<f64>,
}

#[derive(Clone, Deserialize)]
struct RoutePath {
    distance_meters: Option<f64>,
    duration_seconds: Option<f64>,
    steps: Option<Vec<RouteStep>>,
}

#[derive(Deserialize)]
struct GeocodeResponse {
    ok: bool,
    error: Option<String>,
    results: Option<Vec<GeocodeCandidate>>,
}

#[derive(Deserialize)]
struct RouteResponse {
    ok: bool,
    error: Option<String>,
    distance_meters: Option<f64>,
    duration_seconds: Option<f64>,
    steps: Option<Vec<RouteStep>>,
    routes: Option<Vec<RoutePath>>,
}

#[derive(Clone)]
struct RouteResult {
    notices: Vec<String>,
    transit_details: Vec<String>,
    distance_text: Option<String>,
    duration_text: Option<String>,
    distance_meters: f64,
    duration_seconds: f64,
    steps: Vec<RouteStep>,
}

fn routes_client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(40))
        .user_agent(concat!("Sonarpad/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|err| err.to_string())
}

fn route_language_code() -> &'static str {
    match Settings::load().ui_language.as_str() {
        "it" => "it",
        "fr" => "fr",
        "es" => "es",
        "pt" => "pt",
        "cs" => "cs",
        "pl" => "pl",
        "de" => "de",
        _ => "en",
    }
}

fn route_country_alpha3() -> &'static str {
    match Settings::load().ui_language.as_str() {
        "it" => "ITA",
        "fr" => "FRA",
        "es" => "ESP",
        "pt" => "PRT",
        "cs" => "CZE",
        "pl" => "POL",
        "de" => "DEU",
        _ => "USA",
    }
}

fn geocode(query: &str) -> Result<Vec<GeocodeCandidate>, String> {
    let q = query.trim();
    if q.is_empty() {
        return Err(ui_strings().routes_invalid_address.clone());
    }

    let mut url = reqwest::Url::parse("https://sonarpad.com/api/ors_geocode.php")
        .map_err(|e| e.to_string())?;
    url.query_pairs_mut()
        .append_pair("q", q)
        .append_pair("size", "20")
        .append_pair("layers", "address,street,venue")
        .append_pair("sources", "osm,oa")
        .append_pair("boundary.country", route_country_alpha3())
        .append_pair("language", route_language_code());

    let resp = routes_client()?
        .get(url)
        .header("Accept", "application/json")
        .header("X-Sonarpad-Route-Token", SONARPAD_ROUTE_CLIENT_TOKEN)
        .send()
        .map_err(|err| err.to_string())?;

    let status = resp.status();
    if !status.is_success() {
        return Err(format!("HTTP {}", status.as_u16()));
    }

    let data = resp.json::<GeocodeResponse>().map_err(|e| e.to_string())?;
    if !data.ok {
        return Err(data.error.unwrap_or_else(|| "Server error".to_string()));
    }

    Ok(data.results.unwrap_or_default())
}

#[derive(Clone)]
struct RouteDeparture {
    time: String,
    time_zone: String,
}

fn default_departure_zone() -> &'static str {
    match route_country_alpha3() {
        "ITA" => "Europe/Rome",
        "FRA" => "Europe/Paris",
        "ESP" => "Europe/Madrid",
        "PRT" => "Europe/Lisbon",
        "CZE" => "Europe/Prague",
        "POL" => "Europe/Warsaw",
        "DEU" => "Europe/Berlin",
        _ => "America/New_York",
    }
}

fn calculate_route(
    from: &GeocodeCandidate,
    to: &GeocodeCandidate,
    profile: &str,
    preference: &str,
    include_municipalities: bool,
    departure: Option<&RouteDeparture>,
) -> Result<Vec<RouteResult>, String> {
    calculate_route_at(
        from,
        to,
        profile,
        preference,
        include_municipalities,
        "https://sonarpad.com/api",
        departure,
    )
}

fn calculate_route_at(
    from: &GeocodeCandidate,
    to: &GeocodeCandidate,
    profile: &str,
    preference: &str,
    include_municipalities: bool,
    base_url: &str,
    departure: Option<&RouteDeparture>,
) -> Result<Vec<RouteResult>, String> {
    if matches!(
        profile,
        "transit" | "driving-car" | "foot-walking" | "cycling-regular"
    ) {
        match primary_route(
            from,
            to,
            profile,
            preference,
            include_municipalities,
            base_url,
            departure,
        ) {
            Ok(routes) => {
                crate::append_podcast_log(&format!(
                    "route_result: provider=google fallback=false profile={profile} paths={}",
                    routes.len()
                ));
                return Ok(routes);
            }
            Err(error) if profile == "transit" => return Err(error),
            Err(error) => crate::append_podcast_log(&format!(
                "route_fallback: from=google to=ors reason={error}"
            )),
        }
    }
    let from_lat = from.latitude.unwrap_or(0.0).to_string();
    let from_lon = from.longitude.unwrap_or(0.0).to_string();
    let to_lat = to.latitude.unwrap_or(0.0).to_string();
    let to_lon = to.longitude.unwrap_or(0.0).to_string();

    let mut url =
        reqwest::Url::parse(&format!("{base_url}/ors_route.php")).map_err(|e| e.to_string())?;
    url.query_pairs_mut()
        .append_pair("from_lat", &from_lat)
        .append_pair("from_lon", &from_lon)
        .append_pair("to_lat", &to_lat)
        .append_pair("to_lon", &to_lon)
        .append_pair("profile", profile)
        .append_pair("preference", preference)
        .append_pair("avoid", "")
        .append_pair(
            "include_municipalities",
            if include_municipalities { "1" } else { "0" },
        )
        .append_pair("language", route_language_code())
        .append_pair("boundary.country", route_country_alpha3());

    let resp = routes_client()?
        .get(url)
        .header("Accept", "application/json")
        .header("X-Sonarpad-Route-Token", SONARPAD_ROUTE_CLIENT_TOKEN)
        .send()
        .map_err(|err| err.to_string())?;

    let status = resp.status();
    if !status.is_success() {
        return Err(format!("HTTP {}", status.as_u16()));
    }

    let data = resp.json::<RouteResponse>().map_err(|e| e.to_string())?;
    if !data.ok {
        return Err(data.error.unwrap_or_else(|| "Server error".to_string()));
    }

    if let Some(routes) = data.routes
        && !routes.is_empty()
    {
        let mut results = Vec::new();
        for path in routes {
            results.push(RouteResult {
                notices: Vec::new(),
                transit_details: Vec::new(),
                distance_text: None,
                duration_text: None,
                distance_meters: path.distance_meters.unwrap_or(0.0),
                duration_seconds: path.duration_seconds.unwrap_or(0.0),
                steps: path.steps.unwrap_or_default(),
            });
        }
        crate::append_podcast_log(&format!(
            "route_result: provider=ors fallback={} profile={profile} paths={}",
            profile != "wheelchair",
            results.len()
        ));
        return Ok(results);
    }

    crate::append_podcast_log(&format!(
        "route_result: provider=ors fallback={} profile={profile} paths=1",
        profile != "wheelchair"
    ));
    Ok(vec![RouteResult {
        notices: Vec::new(),
        transit_details: Vec::new(),
        distance_text: None,
        duration_text: None,
        distance_meters: data.distance_meters.unwrap_or(0.0),
        duration_seconds: data.duration_seconds.unwrap_or(0.0),
        steps: data.steps.unwrap_or_default(),
    }])
}

#[derive(Deserialize)]
struct PrimaryRouteResponse {
    ok: bool,
    data: Option<PrimaryRouteData>,
}
#[derive(Deserialize)]
struct PrimaryRouteData {
    includi_comuni: Option<bool>,
    preferenza: String,
    evita: String,
    percorsi: Vec<PrimaryRoutePath>,
}
#[derive(Deserialize)]
struct PrimaryRoutePath {
    distanza_metri: f64,
    durata_secondi: f64,
    distanza: String,
    durata: String,
    istruzioni: Vec<String>,
    #[serde(default)]
    avvisi: Vec<String>,
    #[serde(default)]
    mezzi: Vec<String>,
}

impl RouteResult {
    fn distance_label(&self) -> String {
        self.distance_text
            .clone()
            .unwrap_or_else(|| format_distance(self.distance_meters))
    }
    fn duration_label(&self) -> String {
        self.duration_text
            .clone()
            .unwrap_or_else(|| format_duration(self.duration_seconds))
    }

    fn selection_label(&self, name: &str, index: usize) -> String {
        let mut label = format!(
            "{} {} ({} - {})",
            name,
            index + 1,
            self.distance_label(),
            self.duration_label()
        );
        for transit in &self.transit_details {
            if !transit.trim().is_empty() {
                label.push_str("; ");
                label.push_str(transit);
            }
        }
        label
    }
}

fn primary_route(
    from: &GeocodeCandidate,
    to: &GeocodeCandidate,
    profile: &str,
    preference: &str,
    include_municipalities: bool,
    base_url: &str,
    departure: Option<&RouteDeparture>,
) -> Result<Vec<RouteResult>, String> {
    let mode = match profile {
        "driving-car" => "auto",
        "foot-walking" => "piedi",
        "cycling-regular" => "bici",
        _ => "pullman",
    };
    let preference = if profile == "transit" {
        "fastest"
    } else {
        preference
    };
    let country = match route_country_alpha3() {
        "ITA" => "it",
        "FRA" => "fr",
        "ESP" => "es",
        "PRT" => "pt",
        "CZE" => "cz",
        "POL" => "pl",
        "DEU" => "de",
        _ => "us",
    };
    let invalid = || ui_strings().routes_invalid_address.clone();
    let origin = format!(
        "{},{}",
        from.latitude.ok_or_else(invalid)?,
        from.longitude.ok_or_else(invalid)?
    );
    let destination = format!(
        "{},{}",
        to.latitude.ok_or_else(invalid)?,
        to.longitude.ok_or_else(invalid)?
    );
    let response: PrimaryRouteResponse = routes_client()?
        .post(format!("{base_url}/percorsi.php"))
        .timeout(Duration::from_secs(60))
        .header("X-Sonarpad-Route-Token", SONARPAD_ROUTE_CLIENT_TOKEN)
        .json(
            &serde_json::json!({"da": origin, "a": destination, "mezzo": mode,
            "paese": country, "lingua": route_language_code(),
            "partenza": if profile == "transit" { departure.map(|d| d.time.as_str()).unwrap_or("adesso") } else { "adesso" },
            "fuso": if profile == "transit" { departure.map(|d| d.time_zone.as_str()).unwrap_or("") } else { "" },
            "preferenza": preference, "evita": "", "includi_comuni": include_municipalities}),
        )
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|e| e.to_string())?
        .json()
        .map_err(|e| e.to_string())?;
    let data = response
        .data
        .filter(|d| {
            response.ok
                && !d.percorsi.is_empty()
                && d.preferenza == preference
                && d.evita.is_empty()
                && (!include_municipalities || d.includi_comuni == Some(true))
        })
        .ok_or_else(|| ui_strings().routes_error.clone())?;
    if data
        .percorsi
        .iter()
        .any(|p| p.istruzioni.is_empty() || p.istruzioni.iter().any(|s| s.trim().is_empty()))
    {
        return Err(ui_strings().routes_error.clone());
    }
    Ok(data
        .percorsi
        .into_iter()
        .map(|p| RouteResult {
            notices: p.avvisi,
            transit_details: p.mezzi,
            distance_meters: p.distanza_metri,
            duration_seconds: p.durata_secondi,
            distance_text: Some(p.distanza),
            duration_text: Some(p.durata),
            steps: p
                .istruzioni
                .into_iter()
                .map(|s| RouteStep {
                    instruction: Some(s),
                    distance_meters: None,
                })
                .collect(),
        })
        .collect())
}

fn select_candidate(
    parent: &Dialog,
    title: &str,
    candidates: Vec<GeocodeCandidate>,
) -> Option<GeocodeCandidate> {
    if candidates.is_empty() {
        return None;
    }
    if candidates.len() == 1 {
        return candidates.into_iter().next();
    }

    let ui = ui_strings();
    let d = Dialog::builder(parent, title).build();
    let p = Panel::builder(&d).build();
    let s = BoxSizer::builder(Orientation::Vertical).build();

    let choice = Choice::builder(&p).build();
    for c in &candidates {
        choice.append(&c.display_label());
    }
    choice.set_selection(0);
    s.add(&choice, 0, SizerFlag::Expand | SizerFlag::All, 10);

    let bs = BoxSizer::builder(Orientation::Horizontal).build();
    let ok = Button::builder(&p).with_label(&ui.ok).build();
    let cancel = Button::builder(&p).with_label(&ui.routes_cancel).build();
    bs.add(&ok, 0, SizerFlag::All, 5);
    bs.add(&cancel, 0, SizerFlag::All, 5);
    s.add_sizer(&bs, 0, SizerFlag::AlignCentre, 0);

    p.set_sizer(s, true);

    let d_c = d;
    ok.on_click(move |_| {
        d_c.end_modal(crate::ID_OK);
    });

    let d_c2 = d;
    cancel.on_click(move |_| {
        d_c2.end_modal(crate::ID_CANCEL);
    });

    let res = if d.show_modal() == crate::ID_OK {
        let idx = choice.get_selection().unwrap_or(0);
        Some(candidates[idx as usize].clone())
    } else {
        None
    };

    d.destroy();
    res
}

fn format_distance(meters: f64) -> String {
    if meters < 1000.0 {
        format!("{} m", meters.round() as i64)
    } else {
        format!("{:.1} km", meters / 1000.0)
    }
}

fn format_duration(seconds: f64) -> String {
    let minutes = (seconds / 60.0).round() as i64;
    if minutes < 60 {
        format!("{} min", minutes)
    } else {
        let hours = minutes / 60;
        let mins = minutes % 60;
        format!("{} h {} min", hours, mins)
    }
}

fn select_route(parent: &Dialog, title: &str, routes: Vec<RouteResult>) -> Option<RouteResult> {
    if routes.is_empty() {
        return None;
    }
    if routes.len() == 1 {
        return routes.into_iter().next();
    }

    let ui = ui_strings();
    let d = Dialog::builder(parent, title).build();
    let p = Panel::builder(&d).build();
    let s = BoxSizer::builder(Orientation::Vertical).build();

    let choice = Choice::builder(&p).build();
    for (i, r) in routes.iter().enumerate() {
        let label = r.selection_label(&ui.routes_route_name, i);
        choice.append(&label);
    }
    choice.set_selection(0);
    s.add(&choice, 0, SizerFlag::Expand | SizerFlag::All, 10);

    let bs = BoxSizer::builder(Orientation::Horizontal).build();
    let ok = Button::builder(&p).with_label(&ui.ok).build();
    let cancel = Button::builder(&p).with_label(&ui.routes_cancel).build();
    bs.add(&ok, 0, SizerFlag::All, 5);
    bs.add(&cancel, 0, SizerFlag::All, 5);
    s.add_sizer(&bs, 0, SizerFlag::AlignCentre, 0);

    p.set_sizer(s, true);

    let d_c = d;
    ok.on_click(move |_| {
        d_c.end_modal(crate::ID_OK);
    });

    let d_c2 = d;
    cancel.on_click(move |_| {
        d_c2.end_modal(crate::ID_CANCEL);
    });

    let res = if d.show_modal() == crate::ID_OK {
        let idx = choice.get_selection().unwrap_or(0);
        Some(routes[idx as usize].clone())
    } else {
        None
    };

    d.destroy();
    res
}

fn select_recent_route(parent: &Dialog) -> Option<route_history::RecentRoute> {
    let ui = ui_strings();
    let history = route_history::load();
    let dialog = Dialog::builder(parent, &ui.routes_recent_routes).build();
    let panel = Panel::builder(&dialog).build();
    let sizer = BoxSizer::builder(Orientation::Vertical).build();
    let choice = Choice::builder(&panel).build();
    for item in &history {
        let profile = match item.profile.as_str() {
            "foot-walking" => &ui.routes_profile_walking,
            "cycling-regular" => &ui.routes_profile_cycling,
            "wheelchair" => &ui.routes_profile_wheelchair,
            "transit" => &ui.routes_profile_transit,
            _ => &ui.routes_profile_driving,
        };
        choice.append(&format!("{} → {} ({})", item.from, item.to, profile));
    }
    if history.is_empty() {
        choice.append(&ui.routes_recent_empty);
    }
    choice.set_selection(0);
    sizer.add(&choice, 0, SizerFlag::All | SizerFlag::Expand, 10);
    let ok = Button::builder(&panel).with_label(&ui.ok).build();
    ok.enable(!history.is_empty());
    let clear = Button::builder(&panel)
        .with_label(&ui.routes_clear_history)
        .build();
    clear.enable(!history.is_empty());
    let cancel = Button::builder(&panel)
        .with_label(&ui.routes_cancel)
        .build();
    sizer.add(&ok, 0, SizerFlag::All, 5);
    sizer.add(&clear, 0, SizerFlag::All, 5);
    sizer.add(&cancel, 0, SizerFlag::All, 5);
    panel.set_sizer(sizer, true);
    ok.on_click(move |_| {
        dialog.end_modal(crate::ID_OK);
    });
    clear.on_click(move |_| {
        route_history::save(&[]);
        dialog.end_modal(crate::ID_CANCEL);
    });
    cancel.on_click(move |_| {
        dialog.end_modal(crate::ID_CANCEL);
    });
    let result = if dialog.show_modal() == crate::ID_OK {
        choice
            .get_selection()
            .and_then(|index| history.get(index as usize))
            .cloned()
    } else {
        None
    };
    dialog.destroy();
    result
}

fn label_route_control(control: &impl WxWidget, label: &StaticText) {
    control.set_accessibility_label(&label.get_label());
    #[cfg(target_os = "macos")]
    {
        use objc2_app_kit::{NSAccessibility, NSView};

        let native_label = label.get_handle();
        if !native_label.is_null() {
            // Keep the visible label, but let VoiceOver read it on the control
            // instead of visiting a separate static-text element first.
            // SAFETY: this live wxStaticText exposes its NSView on the UI thread.
            unsafe { (&*native_label.cast::<NSView>()).setAccessibilityElement(false) };
        }
    }
}

pub fn open_routes_dialog(parent: &Frame, editor: TextCtrl) {
    let ui = ui_strings();
    let dialog = Dialog::builder(parent, &ui.routes_title).build();
    let panel = Panel::builder(&dialog).build();
    let sizer = BoxSizer::builder(Orientation::Vertical).build();

    let from_label = StaticText::builder(&panel)
        .with_label(&ui.routes_from_label)
        .build();
    let from_ctrl = TextCtrl::builder(&panel).build();
    label_route_control(&from_ctrl, &from_label);

    let to_label = StaticText::builder(&panel)
        .with_label(&ui.routes_to_label)
        .build();
    let to_ctrl = TextCtrl::builder(&panel).build();
    label_route_control(&to_ctrl, &to_label);

    sizer.add(&from_label, 0, SizerFlag::All, 5);
    sizer.add(&from_ctrl, 0, SizerFlag::Expand | SizerFlag::All, 5);
    sizer.add(&to_label, 0, SizerFlag::All, 5);
    sizer.add(&to_ctrl, 0, SizerFlag::Expand | SizerFlag::All, 5);

    let profile_label = StaticText::builder(&panel)
        .with_label(&ui.routes_profile_label)
        .build();
    let profile_choice = Choice::builder(&panel).build();
    label_route_control(&profile_choice, &profile_label);
    profile_choice.append(&ui.routes_profile_driving);
    profile_choice.append(&ui.routes_profile_walking);
    profile_choice.append(&ui.routes_profile_cycling);
    profile_choice.append(&ui.routes_profile_wheelchair);
    profile_choice.append(&ui.routes_profile_transit);
    profile_choice.set_selection(0);

    let pref_label = StaticText::builder(&panel)
        .with_label(&ui.routes_preference_label)
        .build();
    let pref_choice = Choice::builder(&panel).build();
    label_route_control(&pref_choice, &pref_label);
    pref_choice.append(&ui.routes_preference_fastest);
    pref_choice.append(&ui.routes_preference_shortest);
    pref_choice.set_selection(0);

    let options_sizer = BoxSizer::builder(Orientation::Horizontal).build();
    options_sizer.add(
        &profile_label,
        0,
        SizerFlag::All | SizerFlag::AlignCenterVertical,
        5,
    );
    options_sizer.add(&profile_choice, 1, SizerFlag::All, 5);
    options_sizer.add(
        &pref_label,
        0,
        SizerFlag::All | SizerFlag::AlignCenterVertical,
        5,
    );
    options_sizer.add(&pref_choice, 1, SizerFlag::All, 5);
    sizer.add_sizer(&options_sizer, 0, SizerFlag::Expand, 0);

    let departure_label = StaticText::builder(&panel)
        .with_label(&ui.routes_departure_label)
        .build();
    let departure_choice = Choice::builder(&panel).build();
    label_route_control(&departure_choice, &departure_label);
    departure_choice.append(&ui.routes_departure_now);
    departure_choice.append(&ui.routes_departure_at);
    departure_choice.set_selection(0);
    let hour_label = StaticText::builder(&panel)
        .with_label(&ui.routes_departure_hour)
        .build();
    let hour_choice = Choice::builder(&panel).build();
    label_route_control(&hour_choice, &hour_label);
    for hour in 0..24 {
        hour_choice.append(&format!("{hour:02}"));
    }
    hour_choice.set_selection(0);
    let minute_label = StaticText::builder(&panel)
        .with_label(&ui.routes_departure_minute)
        .build();
    let minute_choice = Choice::builder(&panel).build();
    label_route_control(&minute_choice, &minute_label);
    for minute in 0..60 {
        minute_choice.append(&format!("{minute:02}"));
    }
    minute_choice.set_selection(0);
    let zone_label = StaticText::builder(&panel)
        .with_label(&ui.routes_departure_zone)
        .build();
    let zone_ctrl = TextCtrl::builder(&panel).build();
    label_route_control(&zone_ctrl, &zone_label);
    zone_ctrl.set_value(default_departure_zone());
    sizer.add(&departure_label, 0, SizerFlag::All, 5);
    sizer.add(&departure_choice, 0, SizerFlag::Expand | SizerFlag::All, 5);
    sizer.add(&hour_label, 0, SizerFlag::All, 5);
    sizer.add(&hour_choice, 0, SizerFlag::Expand | SizerFlag::All, 5);
    sizer.add(&minute_label, 0, SizerFlag::All, 5);
    sizer.add(&minute_choice, 0, SizerFlag::Expand | SizerFlag::All, 5);
    sizer.add(&zone_label, 0, SizerFlag::All, 5);
    sizer.add(&zone_ctrl, 0, SizerFlag::Expand | SizerFlag::All, 5);
    let refresh_departure = move || {
        let transit = profile_choice.get_selection() == Some(4);
        let scheduled = transit && departure_choice.get_selection() == Some(1);
        pref_choice.enable(!transit);
        pref_label.enable(!transit);
        departure_label.show(transit);
        departure_choice.show(transit);
        hour_label.show(scheduled);
        hour_choice.show(scheduled);
        minute_label.show(scheduled);
        minute_choice.show(scheduled);
        zone_label.show(scheduled);
        zone_ctrl.show(scheduled);
        dialog.set_size(Size::new(600, if scheduled { 760 } else { 600 }));
        panel.layout();
        dialog.layout();
    };
    profile_choice.on_selection_changed(move |_| refresh_departure());
    departure_choice.on_selection_changed(move |_| refresh_departure());
    refresh_departure();

    let municipalities = CheckBox::builder(&panel)
        .with_label(&ui.routes_include_municipalities)
        .build();
    sizer.add(&municipalities, 0, SizerFlag::All, 5);

    let calc_btn = Button::builder(&panel)
        .with_label(&ui.routes_calculate_button)
        .build();
    sizer.add(&calc_btn, 0, SizerFlag::All | SizerFlag::AlignCentre, 10);

    let recent_btn = Button::builder(&panel)
        .with_label(&ui.routes_recent_routes)
        .build();
    sizer.add(&recent_btn, 0, SizerFlag::All | SizerFlag::AlignCentre, 5);
    recent_btn.on_click(move |_| {
        if let Some(item) = select_recent_route(&dialog) {
            from_ctrl.set_value(&item.from);
            to_ctrl.set_value(&item.to);
            let index = match item.profile.as_str() {
                "foot-walking" => 1,
                "cycling-regular" => 2,
                "wheelchair" => 3,
                "transit" => 4,
                _ => 0,
            };
            profile_choice.set_selection(index);
            pref_choice.set_selection(if item.preference == "shortest" { 1 } else { 0 });
            pref_choice.enable(index != 4);
            pref_label.enable(index != 4);
            departure_choice.set_selection(0);
            refresh_departure();
            municipalities.set_value(item.municipalities);
            calc_btn.set_focus();
        } else {
            recent_btn.set_focus();
        }
    });
    panel.set_sizer(sizer, true);

    let dialog_clone = dialog;
    let editor_c = editor;
    let ui_loading = ui.routes_loading.clone();
    let ui_error = ui.routes_error.clone();
    let ui_dist = ui.routes_distance.clone();
    let ui_dur = ui.routes_duration.clone();
    let ui_instr = ui.routes_instructions.clone();

    calc_btn.on_click(move |_| {
        let from_str = from_ctrl.get_value().trim().to_string();
        let to_str = to_ctrl.get_value().trim().to_string();
        if from_str.is_empty() || to_str.is_empty() {
            return;
        }

        let profile_idx = profile_choice.get_selection().unwrap_or(0);
        let profile = match profile_idx {
            1 => "foot-walking",
            2 => "cycling-regular",
            3 => "wheelchair",
            4 => "transit",
            _ => "driving-car",
        }
        .to_string();

        let departure = if profile == "transit" && departure_choice.get_selection() == Some(1) {
            Some(RouteDeparture {
                time: format!(
                    "{:02}:{:02}",
                    hour_choice.get_selection().unwrap_or(0),
                    minute_choice.get_selection().unwrap_or(0)
                ),
                time_zone: zone_ctrl.get_value().trim().to_string(),
            })
        } else {
            None
        };

        let pref_idx = pref_choice.get_selection().unwrap_or(0);
        let pref = match pref_idx {
            1 => "shortest",
            _ => "fastest",
        }
        .to_string();

        let progress =
            ProgressDialog::builder(&dialog_clone, &ui_strings().routes_title, &ui_loading, 100)
                .with_style(ProgressDialogStyle::Smooth)
                .build();

        let geocode_state = Arc::new(Mutex::new(None));
        let gs_thread = Arc::clone(&geocode_state);

        let f_str = from_str.clone();
        let t_str = to_str.clone();
        std::thread::spawn(move || {
            let from_res = geocode(&f_str);
            let to_res = geocode(&t_str);
            *gs_thread.lock().unwrap() = Some((from_res, to_res));
        });

        let mut progress_value = 0;
        let error_fmt = ui_error.clone();
        let dist_label = ui_dist.clone();
        let dur_label = ui_dur.clone();
        let instr_label = ui_instr.clone();
        let ui_loading_c = ui_loading.clone();
        let ui_tit_c = ui_strings().routes_title.clone();
        let ui_cf = ui_strings().routes_choose_from.clone();
        let ui_ct = ui_strings().routes_choose_to.clone();
        let not_found = ui_strings().routes_address_not_found.clone();
        let d_c = dialog_clone;

        let (from_cand, to_cand) = loop {
            std::thread::sleep(Duration::from_millis(150));
            if let Some(res) = geocode_state.lock().unwrap().take() {
                progress.destroy();
                let (from_cands, to_cands) = match res {
                    (Ok(f), Ok(t)) => (f, t),
                    (Err(e), _) | (_, Err(e)) => {
                        let msg = error_fmt.replace("{err}", &e);
                        let err_dlg =
                            MessageDialog::builder(&dialog_clone, &msg, &ui_strings().routes_title)
                                .with_style(MessageDialogStyle::OK | MessageDialogStyle::IconError)
                                .build();
                        err_dlg.show_modal();
                        err_dlg.destroy();
                        return;
                    }
                };

                let d_c = dialog_clone;
                let f_c = select_candidate(&d_c, &ui_cf, from_cands);
                if f_c.is_none() {
                    let err_dlg =
                        MessageDialog::builder(&d_c, &not_found, &ui_strings().routes_title)
                            .with_style(
                                MessageDialogStyle::OK | MessageDialogStyle::IconInformation,
                            )
                            .build();
                    err_dlg.show_modal();
                    err_dlg.destroy();
                    return;
                }

                let t_c = select_candidate(&d_c, &ui_ct, to_cands);
                if t_c.is_none() {
                    let err_dlg =
                        MessageDialog::builder(&d_c, &not_found, &ui_strings().routes_title)
                            .with_style(
                                MessageDialogStyle::OK | MessageDialogStyle::IconInformation,
                            )
                            .build();
                    err_dlg.show_modal();
                    err_dlg.destroy();
                    return;
                }

                break (f_c.unwrap(), t_c.unwrap());
            }
            progress_value += 3;
            if progress_value >= 95 {
                progress_value = 10;
            }
            progress.update(progress_value, Some(&ui_loading_c));
        };

        let include_municipalities = municipalities.get_value();
        let recent = route_history::RecentRoute {
            from: from_cand.display_label(),
            to: to_cand.display_label(),
            profile: profile.clone(),
            preference: pref.clone(),
            avoid: String::new(),
            country: String::new(),
            municipalities: include_municipalities,
        };
        let route_state = Arc::new(Mutex::new(None));
        let rs_thread = Arc::clone(&route_state);
        let progress2 = ProgressDialog::builder(&d_c, &ui_tit_c, &ui_loading_c, 100)
            .with_style(ProgressDialogStyle::Smooth)
            .build();

        std::thread::spawn(move || {
            let res = calculate_route(
                &from_cand,
                &to_cand,
                &profile,
                &pref,
                include_municipalities,
                departure.as_ref(),
            );
            *rs_thread.lock().unwrap() = Some(res);
        });

        progress_value = 0;
        loop {
            std::thread::sleep(Duration::from_millis(150));
            if let Some(res) = route_state.lock().unwrap().take() {
                progress2.destroy();
                match res {
                    Ok(routes_list) => {
                        route_history::remember(recent.clone());
                        let ui_tit = ui_strings().routes_title.clone();
                        if let Some(route) = select_route(&d_c, &ui_tit, routes_list) {
                            let mut output = String::new();
                            output.push_str(&format!(
                                "{} {}\n",
                                dist_label,
                                route.distance_label()
                            ));
                            output.push_str(&format!(
                                "{} {}\n\n",
                                dur_label,
                                route.duration_label()
                            ));
                            output.push_str(&format!("{}\n", instr_label));

                            for (i, step) in route.steps.iter().enumerate() {
                                if let Some(instr) = &step.instruction {
                                    let d = step.distance_meters.unwrap_or(0.0);
                                    if d > 0.0 {
                                        output.push_str(&format!(
                                            "{}. {} ({})\n",
                                            i + 1,
                                            instr,
                                            format_distance(d)
                                        ));
                                    } else {
                                        output.push_str(&format!("{}. {}\n", i + 1, instr));
                                    }
                                }
                            }
                            if !route.notices.is_empty() {
                                output.push_str("\n");
                                output.push_str(&route.notices.join("\n"));
                                output.push_str("\n");
                            }
                            let text_to_import = format!("\n\n{}\n{}\n", ui_tit, output);
                            editor_c.append_text(&text_to_import);
                            d_c.end_modal(crate::ID_OK);
                        } else {
                            // User canceled route selection, no action needed
                        }
                    }
                    Err(e) => {
                        let msg = error_fmt.replace("{err}", &e);
                        let err_dlg =
                            MessageDialog::builder(&d_c, &msg, &ui_strings().routes_title)
                                .with_style(MessageDialogStyle::OK | MessageDialogStyle::IconError)
                                .build();
                        err_dlg.show_modal();
                        err_dlg.destroy();
                    }
                }
                break;
            }
            progress_value += 3;
            if progress_value >= 95 {
                progress_value = 10;
            }
            progress2.update(progress_value, Some(&ui_loading_c));
        }
    });

    dialog.set_size(Size::new(600, 600));
    from_ctrl.set_focus();
    dialog.show_modal();
    dialog.destroy();
}

#[cfg(test)]
mod primary_route_tests {
    use super::*;
    use std::io::{BufRead, Read, Write};
    use std::net::TcpListener;

    fn server(
        responses: Vec<(u16, &'static str)>,
    ) -> (String, std::thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            let mut requests = Vec::new();
            for (status, body) in responses {
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            assert!(
                                std::time::Instant::now() < deadline,
                                "Missing route request"
                            );
                            std::thread::sleep(std::time::Duration::from_millis(5));
                        }
                        Err(e) => panic!("{e}"),
                    }
                };
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                    .unwrap();
                let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
                let mut request = String::new();
                let mut length = 0;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" {
                        break;
                    }
                    if let Some(value) = line.to_lowercase().strip_prefix("content-length:") {
                        length = value.trim().parse().unwrap();
                    }
                    request.push_str(&line);
                }
                let mut bytes = vec![0; length];
                reader.read_exact(&mut bytes).unwrap();
                request.push_str(&String::from_utf8(bytes).unwrap());
                requests.push(request);
                write!(stream, "HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            }
            requests
        });
        (base, handle)
    }
    fn candidate() -> GeocodeCandidate {
        GeocodeCandidate {
            label: Some("Oxford Circus".into()),
            name: None,
            country: None,
            region: None,
            locality: None,
            postalcode: None,
            latitude: Some(51.5152),
            longitude: Some(-0.1419),
        }
    }
    const PRIMARY: &str = r#"{"ok":true,"data":{"percorsi":[{"distanza":"1.2 mi","durata":"20 min","istruzioni":["Take Bus 12 at 12:30"],"mezzi":["Take Bus 12 at 12:30"],"distanza_metri":1900,"durata_secondi":1200},{"distanza":"1.4 mi","durata":"24 min","istruzioni":["Take Underground Central"],"distanza_metri":2300,"durata_secondi":1440}],"preferenza":"fastest","evita":""}}"#;
    const LOCALITIES: &str = r#"{"ok":true,"data":{"percorsi":[{"distanza":"1.2 mi","durata":"20 min","istruzioni":["Locality detected along the route: London.","Take Bus 12 at 12:30"],"distanza_metri":1900,"durata_secondi":1200},{"distanza":"1.4 mi","durata":"24 min","istruzioni":["Take Underground Central"],"distanza_metri":2300,"durata_secondi":1440}],"preferenza":"fastest","evita":"","includi_comuni":true}}"#;
    const SHORTEST: &str = r#"{"ok":true,"data":{"percorsi":[{"distanza":"1.2 mi","durata":"20 min","istruzioni":["Take Bus 12 at 12:30"],"distanza_metri":1900,"durata_secondi":1200},{"distanza":"1.4 mi","durata":"24 min","istruzioni":["Take Underground Central"],"distanza_metri":2300,"durata_secondi":1440}],"preferenza":"shortest","evita":""}}"#;

    const ORS: &str = r#"{"ok":true,"distance_meters":1000,"duration_seconds":600,"steps":[]}"#;
    #[test]
    fn primary_success_preserves_modes_and_readable_alternatives() {
        for (profile, mode) in [
            ("foot-walking", "piedi"),
            ("driving-car", "auto"),
            ("transit", "pullman"),
        ] {
            let (base, server) = server(vec![(200, PRIMARY)]);
            let result = calculate_route_at(
                &candidate(),
                &candidate(),
                profile,
                "fastest",
                false,
                &base,
                None,
            )
            .unwrap();
            assert_eq!(result.len(), 2);
            assert_eq!(result[0].distance_label(), "1.2 mi");
            assert_eq!(result[0].duration_label(), "20 min");
            assert_eq!(result[0].transit_details, ["Take Bus 12 at 12:30"]);
            assert_eq!(
                result[0].selection_label("Route", 0),
                "Route 1 (1.2 mi - 20 min); Take Bus 12 at 12:30"
            );
            assert!(result[1].transit_details.is_empty());
            assert_eq!(
                result[1].selection_label("Route", 1),
                "Route 2 (1.4 mi - 24 min)"
            );
            assert_eq!(
                result[0].steps[0].instruction.as_deref(),
                Some("Take Bus 12 at 12:30")
            );
            let requests = server.join().unwrap();
            assert!(requests[0].starts_with("POST /percorsi.php "));
            assert!(requests[0].contains(&format!(r#""mezzo":"{mode}""#)));
        }
    }
    #[test]
    fn failures_fall_back_without_changing_travel_mode() {
        for failure in [
            (502, "down"),
            (200, "not JSON"),
            (200, r#"{"ok":true,"data":{"percorsi":[]}}"#),
        ] {
            for profile in ["foot-walking", "driving-car"] {
                let (base, server) = server(vec![failure, (200, ORS)]);
                let result = calculate_route_at(
                    &candidate(),
                    &candidate(),
                    profile,
                    "fastest",
                    false,
                    &base,
                    None,
                )
                .unwrap();
                assert_eq!(result[0].distance_meters, 1000.0);
                let requests = server.join().unwrap();
                assert!(requests[0].starts_with("POST /percorsi.php "));
                assert!(
                    requests[1].starts_with("GET /ors_route.php?") && requests[1].contains(profile)
                );
            }
        }
    }
    #[test]
    fn wheelchair_uses_ors_directly() {
        let (base, server) = server(vec![(200, ORS)]);
        calculate_route_at(
            &candidate(),
            &candidate(),
            "wheelchair",
            "fastest",
            false,
            &base,
            None,
        )
        .unwrap();
        assert!(server.join().unwrap()[0].starts_with("GET /ors_route.php?"));
    }
    #[test]
    fn shortest_and_cycling_use_primary() {
        for (profile, preference, response) in [
            ("driving-car", "shortest", SHORTEST),
            ("cycling-regular", "fastest", PRIMARY),
        ] {
            let (base, server) = server(vec![(200, response)]);
            let routes = calculate_route_at(
                &candidate(),
                &candidate(),
                profile,
                preference,
                false,
                &base,
                None,
            )
            .unwrap();
            assert_eq!(routes[0].distance_meters, 1900.0);
            assert!(server.join().unwrap()[0].contains(&format!(r#""preferenza":"{preference}""#)));
        }
        let (base, server) = server(vec![(200, PRIMARY), (200, ORS)]);
        calculate_route_at(
            &candidate(),
            &candidate(),
            "driving-car",
            "shortest",
            false,
            &base,
            None,
        )
        .unwrap();
        assert!(server.join().unwrap()[1].starts_with("GET /ors_route.php?"));
    }
    #[test]
    fn localities_enrich_the_google_route_without_ors() {
        let (base, server) = server(vec![(200, LOCALITIES)]);
        let routes = calculate_route_at(
            &candidate(),
            &candidate(),
            "driving-car",
            "fastest",
            true,
            &base,
            None,
        )
        .unwrap();
        assert_eq!(routes[0].distance_meters, 1900.0);
        assert!(
            routes[0].steps[0]
                .instruction
                .as_deref()
                .unwrap()
                .contains("London")
        );
        let requests = server.join().unwrap();
        assert_eq!(requests.len(), 1);
        assert!(requests[0].contains(r#""includi_comuni":true"#));
    }
    #[test]
    fn scheduled_transit_sends_selected_time_and_zone_only_for_transit() {
        for profile in ["transit", "driving-car"] {
            let departure = RouteDeparture {
                time: "18:35".into(),
                time_zone: "Europe/London".into(),
            };
            let (base, server) = server(vec![(200, PRIMARY)]);
            calculate_route_at(
                &candidate(),
                &candidate(),
                profile,
                "fastest",
                false,
                &base,
                Some(&departure),
            )
            .unwrap();
            let requests = server.join().unwrap();
            assert_eq!(requests.len(), 1);
            let expected = if profile == "transit" {
                "18:35"
            } else {
                "adesso"
            };
            assert!(requests[0].contains(&format!(r#""partenza":"{expected}""#)));
            assert_eq!(
                requests[0].contains(r#""fuso":"Europe/London""#),
                profile == "transit"
            );
        }
    }

    #[test]
    fn transit_failure_never_becomes_walking() {
        let (base, server) = server(vec![(502, "down")]);
        assert!(
            calculate_route_at(
                &candidate(),
                &candidate(),
                "transit",
                "fastest",
                false,
                &base,
                None,
            )
            .is_err()
        );
        assert_eq!(server.join().unwrap().len(), 1);
    }
}

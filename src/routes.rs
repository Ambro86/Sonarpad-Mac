use std::sync::{Arc, Mutex};
use std::time::Duration;
use wxdragon::*;
use crate::{Settings, current_ui_strings as ui_strings, SONARPAD_ROUTE_CLIENT_TOKEN};
use serde::Deserialize;

#[derive(Clone, Deserialize)]
struct GeocodeCandidate {
    latitude: Option<f64>,
    longitude: Option<f64>,
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

struct RouteResult {
    distance_meters: f64,
    duration_seconds: f64,
    steps: Vec<RouteStep>,
}

fn routes_client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent(concat!("Sonarpad/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|err| err.to_string())
}

fn language_code() -> &'static str {
    if Settings::load().ui_language == "it" { "it" } else { "en" }
}

fn country_alpha3() -> &'static str {
    if Settings::load().ui_language == "it" { "ITA" } else { "USA" }
}

fn geocode(query: &str) -> Result<Vec<GeocodeCandidate>, String> {
    let q = query.trim();
    if q.is_empty() {
        return Err(ui_strings().routes_invalid_address.clone());
    }
    
    let mut url = reqwest::Url::parse("https://sonarpad.com/api/ors_geocode.php").map_err(|e| e.to_string())?;
    url.query_pairs_mut()
        .append_pair("q", q)
        .append_pair("size", "20")
        .append_pair("layers", "address,street,venue")
        .append_pair("sources", "osm,oa")
        .append_pair("boundary.country", country_alpha3())
        .append_pair("language", language_code());
        
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

fn calculate_route(from: &GeocodeCandidate, to: &GeocodeCandidate, profile: &str, preference: &str) -> Result<RouteResult, String> {
    let from_lat = from.latitude.unwrap_or(0.0).to_string();
    let from_lon = from.longitude.unwrap_or(0.0).to_string();
    let to_lat = to.latitude.unwrap_or(0.0).to_string();
    let to_lon = to.longitude.unwrap_or(0.0).to_string();
    
    let mut url = reqwest::Url::parse("https://sonarpad.com/api/ors_route.php").map_err(|e| e.to_string())?;
    url.query_pairs_mut()
        .append_pair("from_lat", &from_lat)
        .append_pair("from_lon", &from_lon)
        .append_pair("to_lat", &to_lat)
        .append_pair("to_lon", &to_lon)
        .append_pair("profile", profile)
        .append_pair("preference", preference)
        .append_pair("avoid", "")
        .append_pair("include_municipalities", "0")
        .append_pair("language", language_code())
        .append_pair("boundary.country", country_alpha3());
        
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
    
    if let Some(routes) = data.routes {
        if let Some(path) = routes.into_iter().next() {
            return Ok(RouteResult {
                distance_meters: path.distance_meters.unwrap_or(0.0),
                duration_seconds: path.duration_seconds.unwrap_or(0.0),
                steps: path.steps.unwrap_or_default(),
            });
        }
    }
    
    Ok(RouteResult {
        distance_meters: data.distance_meters.unwrap_or(0.0),
        duration_seconds: data.duration_seconds.unwrap_or(0.0),
        steps: data.steps.unwrap_or_default(),
    })
}

fn do_route_search(from_str: String, to_str: String, profile: String, preference: String) -> Result<RouteResult, String> {
    let ui = ui_strings();
    let _ = &ui.routes_no_results; // suppress unused warning
    let from_cands = geocode(&from_str)?;
    let from_cand = from_cands.into_iter().next().ok_or_else(|| ui.routes_address_not_found.clone())?;
    
    let to_cands = geocode(&to_str)?;
    let to_cand = to_cands.into_iter().next().ok_or_else(|| ui.routes_address_not_found.clone())?;
    
    calculate_route(&from_cand, &to_cand, &profile, &preference)
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

pub fn open_routes_dialog(parent: &Frame) {
    let ui = ui_strings();
    let dialog = Dialog::builder(parent, &ui.routes_title).build();
    let panel = Panel::builder(&dialog).build();
    let sizer = BoxSizer::builder(Orientation::Vertical).build();
    
    let from_label = StaticText::builder(&panel).with_label(&ui.routes_from_label).build();
    let from_ctrl = TextCtrl::builder(&panel).build();
    
    let to_label = StaticText::builder(&panel).with_label(&ui.routes_to_label).build();
    let to_ctrl = TextCtrl::builder(&panel).build();
    
    sizer.add(&from_label, 0, SizerFlag::All, 5);
    sizer.add(&from_ctrl, 0, SizerFlag::Expand | SizerFlag::All, 5);
    sizer.add(&to_label, 0, SizerFlag::All, 5);
    sizer.add(&to_ctrl, 0, SizerFlag::Expand | SizerFlag::All, 5);
    
    let profile_label = StaticText::builder(&panel).with_label(&ui.routes_profile_label).build();
    let profile_choice = Choice::builder(&panel).build();
    profile_choice.append(&ui.routes_profile_driving);
    profile_choice.append(&ui.routes_profile_walking);
    profile_choice.append(&ui.routes_profile_cycling);
    profile_choice.append(&ui.routes_profile_wheelchair);
    profile_choice.set_selection(0);
    
    let pref_label = StaticText::builder(&panel).with_label(&ui.routes_preference_label).build();
    let pref_choice = Choice::builder(&panel).build();
    pref_choice.append(&ui.routes_preference_fastest);
    pref_choice.append(&ui.routes_preference_shortest);
    pref_choice.set_selection(0);
    
    let options_sizer = BoxSizer::builder(Orientation::Horizontal).build();
    options_sizer.add(&profile_label, 0, SizerFlag::All | SizerFlag::AlignCenterVertical, 5);
    options_sizer.add(&profile_choice, 1, SizerFlag::All, 5);
    options_sizer.add(&pref_label, 0, SizerFlag::All | SizerFlag::AlignCenterVertical, 5);
    options_sizer.add(&pref_choice, 1, SizerFlag::All, 5);
    sizer.add_sizer(&options_sizer, 0, SizerFlag::Expand, 0);
    
    let calc_btn = Button::builder(&panel).with_label(&ui.routes_calculate_button).build();
    sizer.add(&calc_btn, 0, SizerFlag::All | SizerFlag::AlignCentre, 10);
    
    let results_ctrl = TextCtrl::builder(&panel)
        .with_style(TextCtrlStyle::MultiLine | TextCtrlStyle::ReadOnly)
        .build();
    sizer.add(&results_ctrl, 1, SizerFlag::Expand | SizerFlag::All, 5);
    
    panel.set_sizer(sizer, true);
    
    let dialog_clone = dialog.clone();
    let results_c = results_ctrl.clone();
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
            _ => "driving-car",
        }.to_string();
        
        let pref_idx = pref_choice.get_selection().unwrap_or(0);
        let pref = match pref_idx {
            1 => "shortest",
            _ => "fastest",
        }.to_string();
        
        let progress = ProgressDialog::builder(&dialog_clone, &ui_strings().routes_title, &ui_loading, 100)
            .with_style(ProgressDialogStyle::Smooth)
            .build();
            
        let result_state = Arc::new(Mutex::new(None));
        let result_thread = Arc::clone(&result_state);
        
        std::thread::spawn(move || {
            let res = do_route_search(from_str, to_str, profile, pref);
            *result_thread.lock().unwrap() = Some(res);
        });
        
        let mut progress_value = 0;
        let results_inner = results_c.clone();
        let error_fmt = ui_error.clone();
        let dist_label = ui_dist.clone();
        let dur_label = ui_dur.clone();
        let instr_label = ui_instr.clone();
        
        loop {
            std::thread::sleep(Duration::from_millis(150));
            if let Some(res) = result_state.lock().unwrap().take() {
                progress.destroy();
                match res {
                    Ok(route) => {
                        let mut output = String::new();
                        output.push_str(&format!("{} {}\n", dist_label, format_distance(route.distance_meters)));
                        output.push_str(&format!("{} {}\n\n", dur_label, format_duration(route.duration_seconds)));
                        output.push_str(&format!("{}\n", instr_label));
                        
                        for (i, step) in route.steps.iter().enumerate() {
                            if let Some(instr) = &step.instruction {
                                let d = step.distance_meters.unwrap_or(0.0);
                                if d > 0.0 {
                                    output.push_str(&format!("{}. {} ({})\n", i + 1, instr, format_distance(d)));
                                } else {
                                    output.push_str(&format!("{}. {}\n", i + 1, instr));
                                }
                            }
                        }
                        results_inner.set_value(&output);
                    }
                    Err(e) => {
                        let msg = error_fmt.replace("{err}", &e);
                        results_inner.set_value(&msg);
                    }
                }
                break;
            }
            progress_value += 3;
            if progress_value >= 95 { progress_value = 10; }
            progress.update(progress_value, Some(&ui_loading));
        }
    });
    
    dialog.set_size(Size::new(600, 600));
    dialog.show_modal();
    dialog.destroy();
}

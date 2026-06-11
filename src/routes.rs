use std::sync::{Arc, Mutex};
use std::time::Duration;
use wxdragon::*;
use crate::{Settings, current_ui_strings as ui_strings, SONARPAD_ROUTE_CLIENT_TOKEN};
use serde::Deserialize;

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
        if let Some(lbl) = &self.label {
            if !lbl.trim().is_empty() {
                return lbl.clone();
            }
        }
        let mut parts = Vec::new();
        if let Some(n) = &self.name { if !n.trim().is_empty() { parts.push(n.clone()); } }
        if let Some(l) = &self.locality { if !l.trim().is_empty() { parts.push(l.clone()); } }
        if let Some(c) = &self.country { if !c.trim().is_empty() { parts.push(c.clone()); } }
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

fn calculate_route(from: &GeocodeCandidate, to: &GeocodeCandidate, profile: &str, preference: &str) -> Result<Vec<RouteResult>, String> {
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
        if !routes.is_empty() {
            let mut results = Vec::new();
            for path in routes {
                results.push(RouteResult {
                    distance_meters: path.distance_meters.unwrap_or(0.0),
                    duration_seconds: path.duration_seconds.unwrap_or(0.0),
                    steps: path.steps.unwrap_or_default(),
                });
            }
            return Ok(results);
        }
    }
    
    Ok(vec![RouteResult {
        distance_meters: data.distance_meters.unwrap_or(0.0),
        duration_seconds: data.duration_seconds.unwrap_or(0.0),
        steps: data.steps.unwrap_or_default(),
    }])
}

fn select_candidate(parent: &Dialog, title: &str, candidates: Vec<GeocodeCandidate>) -> Option<GeocodeCandidate> {
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
    
    let d_c = d.clone();
    ok.on_click(move |_| {
        d_c.end_modal(crate::ID_OK);
    });
    
    let d_c2 = d.clone();
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
    if routes.is_empty() { return None; }
    if routes.len() == 1 { return routes.into_iter().next(); }
    
    let ui = ui_strings();
    let d = Dialog::builder(parent, title).build();
    let p = Panel::builder(&d).build();
    let s = BoxSizer::builder(Orientation::Vertical).build();
    
    let choice = Choice::builder(&p).build();
    for (i, r) in routes.iter().enumerate() {
        let label = format!("{} {} ({} - {})", ui.routes_route_name, i + 1, format_distance(r.distance_meters), format_duration(r.duration_seconds));
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
    
    let d_c = d.clone();
    ok.on_click(move |_| {
        d_c.end_modal(crate::ID_OK);
    });
    
    let d_c2 = d.clone();
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

pub fn open_routes_dialog(parent: &Frame, editor: TextCtrl) {
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
    
    panel.set_sizer(sizer, true);
    
    let dialog_clone = dialog.clone();
    let editor_c = editor.clone();
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
        let d_c = dialog_clone.clone();
        
        let (from_cand, to_cand) = loop {
            std::thread::sleep(Duration::from_millis(150));
            if let Some(res) = geocode_state.lock().unwrap().take() {
                progress.destroy();
                let (from_cands, to_cands) = match res {
                    (Ok(f), Ok(t)) => (f, t),
                    (Err(e), _) | (_, Err(e)) => {
                        let msg = error_fmt.replace("{err}", &e);
                        let err_dlg = MessageDialog::builder(&dialog_clone, &msg, &ui_strings().routes_title)
                            .with_style(MessageDialogStyle::OK | MessageDialogStyle::IconError).build();
                        err_dlg.show_modal(); err_dlg.destroy();
                        return;
                    }
                };
                
                let d_c = dialog_clone.clone();
                let f_c = select_candidate(&d_c, &ui_cf, from_cands);
                if f_c.is_none() { 
                    let err_dlg = MessageDialog::builder(&d_c, &not_found, &ui_strings().routes_title)
                        .with_style(MessageDialogStyle::OK | MessageDialogStyle::IconInformation).build();
                    err_dlg.show_modal(); err_dlg.destroy();
                    return; 
                }
                
                let t_c = select_candidate(&d_c, &ui_ct, to_cands);
                if t_c.is_none() { 
                    let err_dlg = MessageDialog::builder(&d_c, &not_found, &ui_strings().routes_title)
                        .with_style(MessageDialogStyle::OK | MessageDialogStyle::IconInformation).build();
                    err_dlg.show_modal(); err_dlg.destroy();
                    return; 
                }
                
                break (f_c.unwrap(), t_c.unwrap());
            }
            progress_value += 3;
            if progress_value >= 95 { progress_value = 10; }
            progress.update(progress_value, Some(&ui_loading_c));
        };
        
        let route_state = Arc::new(Mutex::new(None));
        let rs_thread = Arc::clone(&route_state);
        let progress2 = ProgressDialog::builder(&d_c, &ui_tit_c, &ui_loading_c, 100)
            .with_style(ProgressDialogStyle::Smooth)
            .build();
            
        std::thread::spawn(move || {
            let res = calculate_route(&from_cand, &to_cand, &profile, &pref);
            *rs_thread.lock().unwrap() = Some(res);
        });
        
        progress_value = 0;
        loop {
            std::thread::sleep(Duration::from_millis(150));
            if let Some(res) = route_state.lock().unwrap().take() {
                progress2.destroy();
                match res {
                    Ok(routes_list) => {
                        let ui_tit = ui_strings().routes_title.clone();
                        if let Some(route) = select_route(&d_c, &ui_tit, routes_list) {
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
                            let text_to_import = format!("\n\n{}\n{}\n", ui_tit, output);
                            editor_c.append_text(&text_to_import);
                            d_c.end_modal(crate::ID_OK);
                        } else {
                            // User canceled route selection, no action needed
                        }
                    }
                    Err(e) => {
                        let msg = error_fmt.replace("{err}", &e);
                        let err_dlg = MessageDialog::builder(&d_c, &msg, &ui_strings().routes_title)
                            .with_style(MessageDialogStyle::OK | MessageDialogStyle::IconError).build();
                        err_dlg.show_modal(); err_dlg.destroy();
                    }
                }
                break;
            }
            progress_value += 3;
            if progress_value >= 95 { progress_value = 10; }
            progress2.update(progress_value, Some(&ui_loading_c));
        }
    });
    
    dialog.set_size(Size::new(600, 600));
    dialog.show_modal();
    dialog.destroy();
}

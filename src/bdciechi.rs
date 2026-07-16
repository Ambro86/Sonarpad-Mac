use crate::{ID_CANCEL, ID_OK};
use crate::{Settings, current_ui_strings as ui_strings, open_url_in_browser};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use wxdragon::*;

const ID_BDCIECHI_OPEN_WITH_SONARPAD: i32 = 4201;

type BdciechiSearchResultState = Arc<Mutex<Option<Result<Vec<String>, String>>>>;

fn bdciechi_is_italian_ui() -> bool {
    Settings::load().ui_language == "it"
}

fn bdciechi_error_title() -> &'static str {
    if bdciechi_is_italian_ui() {
        "Errore"
    } else {
        "Error"
    }
}

fn bdciechi_warning_title() -> &'static str {
    if bdciechi_is_italian_ui() {
        "Avviso"
    } else {
        "Warning"
    }
}

fn bdciechi_info_title() -> &'static str {
    if bdciechi_is_italian_ui() {
        "Informazione"
    } else {
        "Info"
    }
}

fn bdciechi_network_error(err: impl std::fmt::Display) -> String {
    if bdciechi_is_italian_ui() {
        format!("Errore di rete: {}", err)
    } else {
        format!("Network error: {}", err)
    }
}

fn bdciechi_read_error(err: impl std::fmt::Display) -> String {
    if bdciechi_is_italian_ui() {
        format!("Errore di lettura: {}", err)
    } else {
        format!("Read error: {}", err)
    }
}

fn show_bdciechi_message(
    parent: &impl WxWidget,
    message: &str,
    title: &str,
    style: MessageDialogStyle,
) {
    let dialog = MessageDialog::builder(parent, message, title)
        .with_style(style)
        .build();
    crate::localize_standard_dialog_buttons(&dialog);
    dialog.show_modal();
    dialog.destroy();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bdciechi_catalog_search_ignores_word_order() {
        let catalog = vec![
            "I promessi sposi - Alessandro Manzoni".to_string(),
            "Le avventure di Pinocchio - Carlo Collodi".to_string(),
        ];

        assert_eq!(
            bdciechi_filter_catalog(&catalog, "alessandro manzoni"),
            vec!["I promessi sposi - Alessandro Manzoni".to_string()]
        );
        assert_eq!(
            bdciechi_filter_catalog(&catalog, "manzoni alessandro"),
            vec!["I promessi sposi - Alessandro Manzoni".to_string()]
        );
    }

    #[test]
    fn bdciechi_catalog_search_normalizes_punctuation_and_accents() {
        let catalog = vec!["Citta e poesia - Autore".to_string()];

        assert_eq!(
            bdciechi_filter_catalog(&catalog, "città poesia"),
            vec!["Citta e poesia - Autore".to_string()]
        );
    }
}

fn ask_bdciechi_open_saved_book(parent: &impl WxWidget, message: &str) -> bool {
    let dialog = Dialog::builder(parent, bdciechi_info_title())
        .with_style(DialogStyle::Caption | DialogStyle::SystemMenu | DialogStyle::CloseBox)
        .with_size(500, 180)
        .build();
    let panel = Panel::builder(&dialog).build();
    let root = BoxSizer::builder(Orientation::Vertical).build();

    let text = StaticText::builder(&panel).with_label(message).build();
    root.add(&text, 1, SizerFlag::Expand | SizerFlag::All, 12);

    let buttons = BoxSizer::builder(Orientation::Horizontal).build();
    let ok_button = Button::builder(&panel)
        .with_id(ID_OK)
        .with_label("OK")
        .build();
    let open_button = Button::builder(&panel)
        .with_id(ID_BDCIECHI_OPEN_WITH_SONARPAD)
        .with_label("Apri con Sonarpad")
        .build();

    buttons.add_spacer(1);
    buttons.add(&ok_button, 0, SizerFlag::All, 10);
    buttons.add(&open_button, 0, SizerFlag::All, 10);
    root.add_sizer(&buttons, 0, SizerFlag::Expand, 0);

    panel.set_sizer(root, true);
    dialog.set_affirmative_id(ID_OK);
    dialog.set_escape_id(ID_OK);

    let dialog_ok = dialog;
    ok_button.on_click(move |_| {
        dialog_ok.end_modal(ID_OK);
    });

    let dialog_open = dialog;
    open_button.on_click(move |_| {
        dialog_open.end_modal(ID_BDCIECHI_OPEN_WITH_SONARPAD);
    });

    let should_open = dialog.show_modal() == ID_BDCIECHI_OPEN_WITH_SONARPAD;
    dialog.destroy();
    should_open
}

pub struct BdCiechiIdentifyResponse {
    pub nprov: String,
    pub quota: Option<BdCiechiQuota>,
}

pub struct BdCiechiQuota {
    pub remaining: String,
    pub monthly_total: String,
}

pub struct BdCiechiWorkResponse {
    pub _info: String,
    pub text_bytes: Vec<u8>,
}

fn bdciechi_cifra(input: &str) -> String {
    let chars: Vec<u16> = input.encode_utf16().collect();
    let len = chars.len();
    let mut v = vec![0u32; len + 1];
    for &c in &chars {
        v[0] = v[0].wrapping_add(c as u32);
    }
    v[0] %= 256;
    for i in 0..len {
        v[i + 1] = v[i] ^ (chars[i] as u32);
    }
    let mut out = String::with_capacity(v.len() * 2);
    for &n in &v {
        out.push_str(&format!("{:02X}", n & 0xFF));
    }
    out
}

fn bdciechi_decode_server_text(bytes: &[u8]) -> String {
    if let Ok(s) = std::str::from_utf8(bytes) {
        s.to_string()
    } else {
        bytes.iter().map(|&b| b as char).collect()
    }
}

fn bdciechi_rnd() -> String {
    use rand::Rng;
    let chars = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::thread_rng();
    (0..8)
        .map(|_| chars[rng.gen_range(0..chars.len())] as char)
        .collect()
}

fn bdciechi_iden_sp() -> &'static str {
    "SPMac"
}

pub fn bdciechi_identify(
    username: &str,
    password: &str,
) -> Result<BdCiechiIdentifyResponse, String> {
    let plain = format!(
        "{};{};{};*;{}",
        bdciechi_iden_sp(),
        username,
        password,
        bdciechi_rnd()
    );
    let enc = bdciechi_cifra(&plain);
    let url = format!("https://www.bdciechi.it/route.php?{}", enc);

    let resp = reqwest::blocking::get(&url)
        .map_err(bdciechi_network_error)?
        .bytes()
        .map_err(bdciechi_read_error)?;

    let body = bdciechi_decode_server_text(&resp);
    if body.trim_start().starts_with('!') {
        return Err(body);
    }

    let parts: Vec<&str> = body.trim().split(';').collect();
    if parts.is_empty() || parts[0].trim().is_empty() {
        return Err("Invalid response".to_string());
    }
    let nprov = parts[0].trim().to_string();
    let mut quota = None;
    if parts.len() > 1 && !parts[1].trim().is_empty() {
        let remaining = parts[1].trim().to_string();
        let total = if parts.len() > 2 && !parts[2].trim().is_empty() {
            parts[2].trim().to_string()
        } else {
            "60".to_string()
        };
        quota = Some(BdCiechiQuota {
            remaining,
            monthly_total: total,
        });
    }
    Ok(BdCiechiIdentifyResponse { nprov, quota })
}

pub fn bdciechi_fetch_list(nprov: &str, latest: bool) -> Result<Vec<String>, String> {
    let mode = if latest { "-ult" } else { "-ele" };
    let url = format!(
        "https://www.bdciechi.it/route.php?{};@{};{}",
        mode,
        nprov,
        bdciechi_rnd()
    );
    let resp = reqwest::blocking::get(&url)
        .map_err(bdciechi_network_error)?
        .bytes()
        .map_err(bdciechi_read_error)?;

    let body = bdciechi_decode_server_text(&resp);
    if body.trim_start().starts_with('!') {
        return Err(body);
    }
    Ok(body
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty() && !l.starts_with('['))
        .collect())
}

fn bdciechi_filter_catalog(catalog: &[String], query: &str) -> Vec<String> {
    let query_terms = bdciechi_search_terms(query);
    if query_terms.is_empty() {
        return catalog.to_vec();
    }

    catalog
        .iter()
        .filter(|item| {
            let search_key = bdciechi_search_key(item);
            query_terms.iter().all(|term| search_key.contains(term))
        })
        .cloned()
        .collect()
}

fn bdciechi_search_terms(query: &str) -> Vec<String> {
    bdciechi_search_key(query)
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

fn bdciechi_search_key(text: &str) -> String {
    text.chars()
        .flat_map(|ch| {
            let normalized = match ch {
                'à' | 'á' | 'â' | 'ä' | 'ã' | 'å' | 'À' | 'Á' | 'Â' | 'Ä' | 'Ã' | 'Å' => {
                    'a'
                }
                'è' | 'é' | 'ê' | 'ë' | 'È' | 'É' | 'Ê' | 'Ë' => 'e',
                'ì' | 'í' | 'î' | 'ï' | 'Ì' | 'Í' | 'Î' | 'Ï' => 'i',
                'ò' | 'ó' | 'ô' | 'ö' | 'õ' | 'Ò' | 'Ó' | 'Ô' | 'Ö' | 'Õ' => 'o',
                'ù' | 'ú' | 'û' | 'ü' | 'Ù' | 'Ú' | 'Û' | 'Ü' => 'u',
                'ç' | 'Ç' => 'c',
                other if other.is_alphanumeric() => other.to_ascii_lowercase(),
                _ => ' ',
            };
            normalized.to_lowercase()
        })
        .collect::<String>()
}

fn bdciechi_search_loading_message() -> &'static str {
    if bdciechi_is_italian_ui() {
        "Ricerca in corso..."
    } else {
        "Searching..."
    }
}

pub fn bdciechi_download_work(
    username: &str,
    password: &str,
    index: &str,
    preview: bool,
) -> Result<BdCiechiWorkResponse, String> {
    let utc = chrono::Utc::now().format("%Y-%m-%d %H.%M.%S").to_string();
    let sample = if preview { "+" } else { "" };
    let plain = format!(
        "{};{};{};{};{};{};150",
        bdciechi_iden_sp(),
        username,
        password,
        index,
        utc,
        sample
    );
    let enc = bdciechi_cifra(&plain);
    let url = format!("https://www.bdciechi.it/route.php?{}", enc);

    let resp = reqwest::blocking::get(&url)
        .map_err(bdciechi_network_error)?
        .bytes()
        .map_err(bdciechi_read_error)?;

    if resp.is_empty() {
        return Ok(BdCiechiWorkResponse {
            _info: String::new(),
            text_bytes: Vec::new(),
        });
    }

    if resp[0] == 33 {
        // '!'
        let body = bdciechi_decode_server_text(&resp);
        if body.trim_start().starts_with('!') {
            return Err(body);
        }
    }

    if let Some(pos) = resp.iter().position(|&x| x == 26) {
        let info_bytes = &resp[..pos];
        let text_bytes = &resp[pos + 1..];
        let info = bdciechi_decode_server_text(info_bytes);
        Ok(BdCiechiWorkResponse {
            _info: info,
            text_bytes: text_bytes.to_vec(),
        })
    } else {
        Ok(BdCiechiWorkResponse {
            _info: String::new(),
            text_bytes: resp.to_vec(),
        })
    }
}

pub fn open_bdciechi_dialog(parent: &Frame, settings: &Arc<Mutex<Settings>>, tc_main: TextCtrl) {
    let s = settings.lock().unwrap();
    let user = s.bdciechi_username.clone();
    let pass = s.bdciechi_password.clone();
    drop(s);

    if user.is_empty() || pass.is_empty() {
        show_bdciechi_login_dialog(parent, settings, tc_main);
    } else {
        let ui = ui_strings();
        let progress = ProgressDialog::builder(
            parent,
            &ui.bdciechi_title,
            &ui.bdciechi_catalog_loading,
            100,
        )
        .with_style(ProgressDialogStyle::Smooth)
        .build();

        let result_state = Arc::new(Mutex::new(None));
        let result_thread = Arc::clone(&result_state);

        let user_c = user.clone();
        let pass_c = pass.clone();

        std::thread::spawn(move || {
            let res = bdciechi_identify(&user_c, &pass_c);
            *result_thread.lock().unwrap() = Some(res);
        });

        let mut progress_value = 0;
        loop {
            std::thread::sleep(Duration::from_millis(150));
            if let Some(res) = result_state.lock().unwrap().take() {
                progress.destroy();
                match res {
                    Ok(identify) => {
                        show_bdciechi_dashboard(
                            parent,
                            Arc::clone(settings),
                            tc_main,
                            user,
                            pass,
                            identify,
                        );
                    }
                    Err(e) => {
                        show_bdciechi_message(
                            parent,
                            &e,
                            bdciechi_error_title(),
                            MessageDialogStyle::OK | MessageDialogStyle::IconError,
                        );
                        show_bdciechi_login_dialog(parent, settings, tc_main);
                    }
                }
                break;
            }
            progress_value += 3;
            if progress_value >= 95 {
                progress_value = 10;
            }
            progress.update(progress_value, Some(&ui.bdciechi_catalog_loading));
        }
    }
}

fn show_bdciechi_login_dialog(parent: &Frame, settings: &Arc<Mutex<Settings>>, tc_main: TextCtrl) {
    let ui = ui_strings();
    let dialog = Dialog::builder(parent, &ui.bdciechi_title).build();
    let panel = Panel::builder(&dialog).build();
    let sizer = BoxSizer::builder(Orientation::Vertical).build();

    let username_ctrl = TextCtrl::builder(&panel).build();
    let password_ctrl = TextCtrl::builder(&panel).build();

    let s = settings.lock().unwrap();
    username_ctrl.set_value(&s.bdciechi_username);
    password_ctrl.set_value(&s.bdciechi_password);
    drop(s);

    let label_user = StaticText::builder(&panel)
        .with_label(&ui.bdciechi_username_label)
        .build();
    sizer.add(&label_user, 0, SizerFlag::All, 5);
    sizer.add(&username_ctrl, 0, SizerFlag::Expand | SizerFlag::All, 5);

    let label_pass = StaticText::builder(&panel)
        .with_label(&ui.bdciechi_password_label)
        .build();
    sizer.add(&label_pass, 0, SizerFlag::All, 5);
    sizer.add(&password_ctrl, 0, SizerFlag::Expand | SizerFlag::All, 5);

    let btn_sizer = BoxSizer::builder(Orientation::Horizontal).build();
    let login_btn = Button::builder(&panel)
        .with_label(&ui.bdciechi_login_button)
        .build();
    let register_btn = Button::builder(&panel)
        .with_label(&ui.bdciechi_register_button)
        .build();
    btn_sizer.add(&login_btn, 0, SizerFlag::All, 5);
    btn_sizer.add(&register_btn, 0, SizerFlag::All, 5);
    sizer.add_sizer(&btn_sizer, 0, SizerFlag::AlignCentre, 0);

    panel.set_sizer(sizer, true);

    let user_ctrl = username_ctrl;
    let pass_ctrl = password_ctrl;
    let settings_clone = Arc::clone(settings);
    let dialog_close = dialog;
    let parent_clone = *parent;
    let tc_clone = tc_main;

    login_btn.on_click(move |_| {
        let u = user_ctrl.get_value().trim().to_string();
        let p = pass_ctrl.get_value().trim().to_string();
        if u.is_empty() || p.is_empty() {
            let uis = ui_strings();
            show_bdciechi_message(
                &dialog_close,
                &uis.bdciechi_missing_fields,
                bdciechi_warning_title(),
                MessageDialogStyle::OK | MessageDialogStyle::IconWarning,
            );
            return;
        }

        let mut s = settings_clone.lock().unwrap();
        s.bdciechi_username = u.clone();
        s.bdciechi_password = p.clone();
        s.save();
        drop(s);

        dialog_close.end_modal(ID_OK);

        let uis = ui_strings();
        let progress = ProgressDialog::builder(
            &parent_clone,
            &uis.bdciechi_title,
            &uis.bdciechi_catalog_loading,
            100,
        )
        .with_style(ProgressDialogStyle::Smooth)
        .build();

        let result_state = Arc::new(Mutex::new(None));
        let result_thread = Arc::clone(&result_state);

        let u_c = u.clone();
        let p_c = p.clone();

        std::thread::spawn(move || {
            let res = bdciechi_identify(&u_c, &p_c);
            *result_thread.lock().unwrap() = Some(res);
        });

        let mut progress_value = 0;
        loop {
            std::thread::sleep(Duration::from_millis(150));
            if let Some(res) = result_state.lock().unwrap().take() {
                progress.destroy();
                match res {
                    Ok(identify) => {
                        show_bdciechi_dashboard(
                            &parent_clone,
                            Arc::clone(&settings_clone),
                            tc_clone,
                            u.clone(),
                            p.clone(),
                            identify,
                        );
                    }
                    Err(e) => {
                        show_bdciechi_message(
                            &parent_clone,
                            &e,
                            bdciechi_error_title(),
                            MessageDialogStyle::OK | MessageDialogStyle::IconError,
                        );
                        show_bdciechi_login_dialog(&parent_clone, &settings_clone, tc_clone);
                    }
                }
                break;
            }
            progress_value += 3;
            if progress_value >= 95 {
                progress_value = 10;
            }
            progress.update(progress_value, Some(&uis.bdciechi_catalog_loading));
        }
    });

    register_btn.on_click(|_| {
        let _ = open_url_in_browser("https://www.bdciechi.it/iscrizione/");
    });

    dialog.show_modal();
    dialog.destroy();
}

fn show_bdciechi_dashboard(
    parent: &Frame,
    settings: Arc<Mutex<Settings>>,
    tc_main: TextCtrl,
    username: String,
    password: String,
    identify: BdCiechiIdentifyResponse,
) {
    let ui = ui_strings();
    let dialog = Dialog::builder(parent, &ui.bdciechi_title).build();
    let panel = Panel::builder(&dialog).build();
    let sizer = BoxSizer::builder(Orientation::Vertical).build();

    let quota_text = if let Some(q) = &identify.quota {
        ui.bdciechi_quota
            .replace("{remaining}", &q.remaining)
            .replace("{total}", &q.monthly_total)
    } else {
        String::new()
    };

    let quota_label = StaticText::builder(&panel).with_label(&quota_text).build();
    sizer.add(&quota_label, 0, SizerFlag::All, 10);

    let search_sizer = BoxSizer::builder(Orientation::Horizontal).build();
    let search_ctrl = TextCtrl::builder(&panel)
        .with_style(TextCtrlStyle::ProcessEnter)
        .build();
    let search_btn = Button::builder(&panel)
        .with_label(&ui.bdciechi_search_button)
        .build();
    let search_lbl = StaticText::builder(&panel)
        .with_label(&ui.bdciechi_search_label)
        .build();
    search_sizer.add(
        &search_lbl,
        0,
        SizerFlag::All | SizerFlag::AlignCenterVertical,
        5,
    );
    search_sizer.add(&search_ctrl, 1, SizerFlag::Expand | SizerFlag::All, 5);
    search_sizer.add(&search_btn, 0, SizerFlag::All, 5);
    sizer.add_sizer(&search_sizer, 0, SizerFlag::Expand | SizerFlag::All, 5);

    let btn_sizer1 = BoxSizer::builder(Orientation::Horizontal).build();
    let latest_btn = Button::builder(&panel)
        .with_label(&ui.bdciechi_latest_button)
        .build();
    let catalog_btn = Button::builder(&panel)
        .with_label(&ui.bdciechi_catalog_button)
        .build();
    btn_sizer1.add(&latest_btn, 0, SizerFlag::All, 5);
    btn_sizer1.add(&catalog_btn, 0, SizerFlag::All, 5);
    sizer.add_sizer(&btn_sizer1, 0, SizerFlag::AlignCentre, 0);

    let combo_sizer = BoxSizer::builder(Orientation::Horizontal).build();
    let results_combo = Choice::builder(&panel).build();
    let book_lbl = StaticText::builder(&panel)
        .with_label(&ui.bdciechi_book_label)
        .build();
    combo_sizer.add(
        &book_lbl,
        0,
        SizerFlag::All | SizerFlag::AlignCenterVertical,
        5,
    );
    combo_sizer.add(&results_combo, 1, SizerFlag::Expand | SizerFlag::All, 5);
    sizer.add_sizer(&combo_sizer, 0, SizerFlag::Expand | SizerFlag::All, 5);

    let pagination_sizer = BoxSizer::builder(Orientation::Horizontal).build();
    let prev_page_btn = Button::builder(&panel).with_label("<").build();
    let page_choice = Choice::builder(&panel).build();
    let goto_page_btn = Button::builder(&panel)
        .with_label(&ui.bdciechi_go_button)
        .build();
    let next_page_btn = Button::builder(&panel).with_label(">").build();
    let page_label = StaticText::builder(&panel).with_label("Pagina 1").build();
    pagination_sizer.add(
        &page_label,
        0,
        SizerFlag::All | SizerFlag::AlignCenterVertical,
        5,
    );
    pagination_sizer.add(&prev_page_btn, 0, SizerFlag::All, 5);
    pagination_sizer.add(&page_choice, 0, SizerFlag::All, 5);
    pagination_sizer.add(&goto_page_btn, 0, SizerFlag::All, 5);
    pagination_sizer.add(&next_page_btn, 0, SizerFlag::All, 5);
    sizer.add_sizer(&pagination_sizer, 0, SizerFlag::AlignCentre, 5);

    let action_sizer = BoxSizer::builder(Orientation::Horizontal).build();
    let preview_btn = Button::builder(&panel)
        .with_label(&ui.bdciechi_preview_button)
        .build();
    let import_btn = Button::builder(&panel)
        .with_label(&ui.bdciechi_import_button)
        .build();
    let back_btn = Button::builder(&panel)
        .with_label(&ui.bdciechi_back_button)
        .build();
    action_sizer.add(&preview_btn, 0, SizerFlag::All, 5);
    action_sizer.add(&import_btn, 0, SizerFlag::All, 5);
    action_sizer.add(&back_btn, 0, SizerFlag::All, 5);
    sizer.add_sizer(&action_sizer, 0, SizerFlag::AlignCentre, 5);

    let logout_btn = Button::builder(&panel)
        .with_label(&ui.bdciechi_logout_button)
        .build();
    sizer.add(&logout_btn, 0, SizerFlag::All | SizerFlag::AlignCentre, 10);

    panel.set_sizer(sizer, true);

    let set_view = {
        let pnl = panel;
        let dlg = dialog;
        let slbl = search_lbl;
        let sctrl = search_ctrl;
        let sbtn = search_btn;
        let lbtn = latest_btn;
        let cbtn = catalog_btn;
        let loutbtn = logout_btn;

        let blbl = book_lbl;
        let rcombo = results_combo;
        let pbtn = preview_btn;
        let ibtn = import_btn;
        let bbtn = back_btn;

        let plbl = page_label;
        let ppbtn = prev_page_btn;
        let pchoice = page_choice;
        let pgoto = goto_page_btn;
        let npbtn = next_page_btn;

        move |home: bool| {
            slbl.show(home);
            sctrl.show(home);
            sbtn.show(home);
            lbtn.show(home);
            cbtn.show(home);
            loutbtn.show(home);

            blbl.show(!home);
            rcombo.show(!home);
            pbtn.show(!home);
            ibtn.show(!home);
            bbtn.show(!home);
            plbl.show(!home);
            ppbtn.show(!home);
            pchoice.show(!home);
            pgoto.show(!home);
            npbtn.show(!home);

            pnl.layout();
            dlg.layout();
        }
    };

    set_view(true);

    let sv_back = set_view;
    back_btn.on_click(move |_| {
        sv_back(true);
    });
    let catalog: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let displayed_results: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let all_results: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let current_page = Arc::new(Mutex::new(0usize));

    let update_page = {
        let ar = Arc::clone(&all_results);
        let dr = Arc::clone(&displayed_results);
        let cp = Arc::clone(&current_page);
        let pc = page_choice;
        let pl = page_label;
        let pr_b = prev_page_btn;
        let n_b = next_page_btn;
        let combo = results_combo;
        let ui_no_res = ui.bdciechi_no_results.clone();

        move || {
            let res = ar.lock().unwrap();
            let mut page = cp.lock().unwrap();
            let total_pages = res.len().div_ceil(50).max(1);
            if *page >= total_pages {
                *page = total_pages.saturating_sub(1);
            }
            let start = *page * 50;
            let end = (start + 50).min(res.len());
            let page_res = res[start..end].to_vec();

            combo.clear();
            if page_res.is_empty() {
                combo.append(&ui_no_res);
            } else {
                for r in &page_res {
                    combo.append(r);
                }
            }
            combo.set_selection(0);
            *dr.lock().unwrap() = page_res;

            pl.set_label(&format!("Pagina {} di {}", *page + 1, total_pages));
            pc.clear();
            for i in 0..total_pages {
                pc.append(&format!("{}", i + 1));
            }
            pc.set_selection(*page as u32);

            pr_b.enable(*page > 0);
            n_b.enable(*page + 1 < total_pages);
        }
    };

    let up_prev = update_page.clone();
    let cp_prev = Arc::clone(&current_page);
    prev_page_btn.on_click(move |_| {
        let mut page = cp_prev.lock().unwrap();
        if *page > 0 {
            *page -= 1;
        }
        drop(page);
        up_prev();
    });

    let up_next = update_page.clone();
    let cp_next = Arc::clone(&current_page);
    let ar_next = Arc::clone(&all_results);
    next_page_btn.on_click(move |_| {
        let res = ar_next.lock().unwrap();
        let total_pages = res.len().div_ceil(50).max(1);
        drop(res);
        let mut page = cp_next.lock().unwrap();
        if *page + 1 < total_pages {
            *page += 1;
        }
        drop(page);
        up_next();
    });

    let up_choice = update_page.clone();
    let cp_choice = Arc::clone(&current_page);
    let pc_choice = page_choice;
    goto_page_btn.on_click(move |_| {
        if let Some(sel) = pc_choice.get_selection() {
            *cp_choice.lock().unwrap() = sel as usize;
            up_choice();
        }
    });

    let catalog_clone = Arc::clone(&catalog);
    let nprov = identify.nprov.clone();
    std::thread::spawn(move || {
        if let Ok(cat) = bdciechi_fetch_list(&nprov, false)
            && let Ok(mut c) = catalog_clone.lock()
        {
            *c = cat;
        }
    });

    let d_latest = dialog;
    let ui_title = ui.bdciechi_title.clone();
    let ui_loading = ui.bdciechi_catalog_loading.clone();
    let nprov_latest = identify.nprov.clone();
    let sv_latest = set_view;
    let ar_latest = Arc::clone(&all_results);
    let cp_latest = Arc::clone(&current_page);
    let up_latest = update_page.clone();
    latest_btn.on_click(move |_| {
        let progress = ProgressDialog::builder(&d_latest, &ui_title, &ui_loading, 100)
            .with_style(ProgressDialogStyle::Smooth)
            .build();

        let np = nprov_latest.clone();
        let result_state = Arc::new(Mutex::new(None));
        let result_thread = Arc::clone(&result_state);

        std::thread::spawn(move || {
            let res = bdciechi_fetch_list(&np, true).unwrap_or_default();
            *result_thread.lock().unwrap() = Some(res);
        });

        let mut progress_value = 0;
        loop {
            std::thread::sleep(Duration::from_millis(150));
            if let Some(res) = result_state.lock().unwrap().take() {
                progress.destroy();
                *ar_latest.lock().unwrap() = res;
                *cp_latest.lock().unwrap() = 0;
                up_latest();
                sv_latest(false);
                break;
            }
            progress_value += 3;
            if progress_value >= 95 {
                progress_value = 10;
            }
            progress.update(progress_value, Some(&ui_loading));
        }
    });

    let cat_ref = Arc::clone(&catalog);
    let sv_cat = set_view;
    let ar_cat = Arc::clone(&all_results);
    let cp_cat = Arc::clone(&current_page);
    let up_cat = update_page.clone();
    catalog_btn.on_click(move |_| {
        let cat = cat_ref.lock().unwrap().clone();
        *ar_cat.lock().unwrap() = cat;
        *cp_cat.lock().unwrap() = 0;
        up_cat();
        sv_cat(false);
    });

    let search_ref = search_ctrl;
    let cat_ref_s = Arc::clone(&catalog);
    let d_search = dialog;
    let ui_title_search = ui.bdciechi_title.clone();
    let ui_empty_search = ui.bdciechi_empty_search.clone();
    let sv_search = set_view;
    let ar_search = Arc::clone(&all_results);
    let cp_search = Arc::clone(&current_page);
    let up_search = update_page.clone();
    let nprov_search = identify.nprov.clone();
    let results_combo_search_focus = results_combo;
    let run_search = Rc::new(move || {
        let query = search_ref.get_value().trim().to_string();
        if query.is_empty() {
            show_bdciechi_message(
                &d_search,
                &ui_empty_search,
                bdciechi_warning_title(),
                MessageDialogStyle::OK | MessageDialogStyle::IconWarning,
            );
            return;
        }

        let loading_message = bdciechi_search_loading_message();
        let progress = ProgressDialog::builder(&d_search, &ui_title_search, loading_message, 100)
            .with_style(ProgressDialogStyle::Smooth)
            .build();

        let result_state: BdciechiSearchResultState = Arc::new(Mutex::new(None));
        let result_thread = Arc::clone(&result_state);
        let cat_thread = Arc::clone(&cat_ref_s);
        let nprov_thread = nprov_search.clone();
        let query_thread = query.clone();

        std::thread::spawn(move || {
            let res = (|| {
                let cached_catalog = cat_thread.lock().map(|cat| cat.clone()).unwrap_or_default();
                let catalog_to_search = if cached_catalog.is_empty() {
                    let fetched_catalog = bdciechi_fetch_list(&nprov_thread, false)?;
                    if let Ok(mut cat) = cat_thread.lock() {
                        *cat = fetched_catalog.clone();
                    }
                    fetched_catalog
                } else {
                    cached_catalog
                };
                Ok(bdciechi_filter_catalog(&catalog_to_search, &query_thread))
            })();
            *result_thread.lock().unwrap() = Some(res);
        });

        let mut progress_value = 0;
        loop {
            std::thread::sleep(Duration::from_millis(150));
            if let Some(res) = result_state.lock().unwrap().take() {
                progress.destroy();
                match res {
                    Ok(found) => {
                        *ar_search.lock().unwrap() = found;
                        *cp_search.lock().unwrap() = 0;
                        up_search();
                        sv_search(false);
                        results_combo_search_focus.set_focus();
                    }
                    Err(e) => {
                        show_bdciechi_message(
                            &d_search,
                            &e,
                            bdciechi_error_title(),
                            MessageDialogStyle::OK | MessageDialogStyle::IconError,
                        );
                    }
                }
                break;
            }
            progress_value += 3;
            if progress_value >= 95 {
                progress_value = 10;
            }
            progress.update(progress_value, Some(loading_message));
        }
    });

    let run_search_click = Rc::clone(&run_search);
    search_btn.on_click(move |_| {
        run_search_click();
    });

    let run_search_enter = Rc::clone(&run_search);
    search_ctrl.on_text_enter(move |_| {
        run_search_enter();
    });

    let do_action = move |preview: bool,
                          combo: &Choice,
                          disp_act: Arc<Mutex<Vec<String>>>,
                          cat_act: Arc<Mutex<Vec<String>>>,
                          u: String,
                          p: String,
                          d: Dialog,
                          ui_tit: String,
                          ui_load: String,
                          tc: TextCtrl,
                          e_msg: String,
                          i_msg: String,
                          p_tit: String| {
        let sel = combo.get_selection();
        if sel < Some(0) {
            return;
        }
        let disp = disp_act.lock().unwrap();
        if (sel.unwrap_or(0) as usize) >= disp.len() {
            return;
        }
        let record = disp[sel.unwrap() as usize].clone();
        drop(disp);

        let cat = cat_act.lock().unwrap();
        let index = if let Some(i) = cat.iter().position(|r| r == &record) {
            i.to_string()
        } else {
            "0".to_string()
        };
        drop(cat);

        let progress = ProgressDialog::builder(&d, &ui_tit, &ui_load, 100)
            .with_style(ProgressDialogStyle::Smooth)
            .build();

        let result_state = Arc::new(Mutex::new(None));
        let result_thread = Arc::clone(&result_state);

        std::thread::spawn(move || {
            let res = bdciechi_download_work(&u, &p, &index, preview);
            *result_thread.lock().unwrap() = Some(res);
        });

        let mut progress_value = 0;
        loop {
            std::thread::sleep(Duration::from_millis(150));
            if let Some(res) = result_state.lock().unwrap().take() {
                progress.destroy();
                match res {
                    Ok(work) => {
                        let text = bdciechi_decode_server_text(&work.text_bytes);
                        if preview {
                            show_bdciechi_message(
                                &d,
                                &text,
                                &p_tit,
                                MessageDialogStyle::OK | MessageDialogStyle::IconInformation,
                            );
                        } else {
                            let ui = crate::current_ui_strings();
                            let safe_name = crate::sanitize_filename(&record);
                            let default_filename = format!("{safe_name}.txt");
                            let text_wildcard = if bdciechi_is_italian_ui() {
                                "File di testo (*.txt)|*.txt"
                            } else {
                                "Text files (*.txt)|*.txt"
                            };
                            let fd = FileDialog::builder(&d)
                                .with_message(&ui.save_as)
                                .with_default_file(&default_filename)
                                .with_wildcard(text_wildcard)
                                .with_style(
                                    FileDialogStyle::Save | FileDialogStyle::OverwritePrompt,
                                )
                                .build();

                            if fd.show_modal() == crate::ID_OK
                                && let Some(path) = fd.get_path()
                            {
                                if let Err(e) = std::fs::write(&path, &text) {
                                    let msg = e_msg.replace("{err}", &e.to_string());
                                    show_bdciechi_message(
                                        &d,
                                        &msg,
                                        bdciechi_error_title(),
                                        MessageDialogStyle::OK | MessageDialogStyle::IconError,
                                    );
                                } else {
                                    let open_saved_book = ask_bdciechi_open_saved_book(&d, &i_msg);
                                    if open_saved_book {
                                        tc.set_value(&text);
                                    }
                                    d.end_modal(crate::ID_OK);
                                }
                            }
                            fd.destroy();
                        }
                    }
                    Err(e) => {
                        let msg = e_msg.replace("{err}", &e);
                        show_bdciechi_message(
                            &d,
                            &msg,
                            bdciechi_error_title(),
                            MessageDialogStyle::OK | MessageDialogStyle::IconError,
                        );
                    }
                }
                break;
            }
            progress_value += 3;
            if progress_value >= 95 {
                progress_value = 10;
            }
            progress.update(progress_value, Some(&ui_load));
        }
    };

    let combo_p = results_combo;
    let disp_act_p = Arc::clone(&displayed_results);
    let cat_act_p = Arc::clone(&catalog);
    let u_p = username.clone();
    let p_p = password.clone();
    let d_p = dialog;
    let ui_tit_p = ui.bdciechi_title.clone();
    let ui_load_p = ui.bdciechi_catalog_loading.clone();
    let tc_p = tc_main;
    let err_p = ui.bdciechi_download_error.clone();
    let imp_p = ui.bdciechi_imported.clone();
    let prev_p = ui.bdciechi_preview_title.clone();
    preview_btn.on_click(move |_| {
        do_action(
            true,
            &combo_p,
            Arc::clone(&disp_act_p),
            Arc::clone(&cat_act_p),
            u_p.clone(),
            p_p.clone(),
            d_p,
            ui_tit_p.clone(),
            ui_load_p.clone(),
            tc_p,
            err_p.clone(),
            imp_p.clone(),
            prev_p.clone(),
        );
    });

    let combo_i = results_combo;
    let disp_act_i = Arc::clone(&displayed_results);
    let cat_act_i = Arc::clone(&catalog);
    let u_i = username.clone();
    let p_i = password.clone();
    let d_i = dialog;
    let ui_tit_i = ui.bdciechi_title.clone();
    let ui_load_i = ui.bdciechi_catalog_loading.clone();
    let tc_i = tc_main;
    let err_i = ui.bdciechi_download_error.clone();
    let imp_i = ui.bdciechi_imported.clone();
    let prev_i = ui.bdciechi_preview_title.clone();
    import_btn.on_click(move |_| {
        do_action(
            false,
            &combo_i,
            Arc::clone(&disp_act_i),
            Arc::clone(&cat_act_i),
            u_i.clone(),
            p_i.clone(),
            d_i,
            ui_tit_i.clone(),
            ui_load_i.clone(),
            tc_i,
            err_i.clone(),
            imp_i.clone(),
            prev_i.clone(),
        );
    });

    let d_logout = dialog;
    let set_out = Arc::clone(&settings);
    logout_btn.on_click(move |_| {
        let mut s = set_out.lock().unwrap();
        s.bdciechi_username.clear();
        s.bdciechi_password.clear();
        s.save();
        drop(s);
        d_logout.end_modal(ID_CANCEL);
    });

    dialog.show_modal();
    dialog.destroy();
}

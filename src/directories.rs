#[path = "places.rs"]
mod places;

use crate::*;
use reqwest::blocking::Client;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use wxdragon::*;

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct SearchResult {
    pub id: String,
    pub name: String,
    pub address: Option<String>,
    pub city: Option<String>,
    pub province: Option<String>,
    pub category: Option<String>,
    pub phones: Vec<String>,
    pub detail: Option<DetailResponse>,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct SearchResponse {
    pub display_where: Option<String>,
    pub current_page: i32,
    pub is_last_page: bool,
    pub results: Vec<SearchResult>,
    pub actual_directory_index: Option<usize>,
}

#[derive(Clone, Debug)]
pub struct DetailResponse {
    pub body: String,
}

pub enum DirectoryKind {
    PagineBianche,
    PagineGialle,
}

const KEY: &[u8] = b"mK9!vP2xL8#qT4zN7@rW1sY6dF0hJ3uBzUrL1BirM@|\\";

// Translate the Directory/Yellow Pages dialogs independently of site data.
// Do not translate fetched business names, addresses or search queries.
fn directory_label(it: &'static str) -> &'static str {
    if crate::normalize_ui_language(&Settings::load().ui_language) != "de" {
        return it;
    }
    match it {
        "Nessun risultato trovato." => "Keine Ergebnisse gefunden.",
        "Ricerca Pagine Bianche/Gialle" => "Suche in den italienischen Telefonverzeichnissen",
        "Risultati Ricerca" => "Suchergebnisse",
        "Pagina Precedente" => "Vorherige Seite",
        "Pagina Successiva" => "Nächste Seite",
        "Apri" => "Öffnen",
        "Chiudi" => "Schließen",
        "Caricamento..." => "Wird geladen…",
        "Ricerca in corso..." => "Suche läuft…",
        "Recupero dettagli..." => "Details werden abgerufen…",
        "Errore" => "Fehler",
        "Categoria:" => "Kategorie:",
        "Indirizzo:" => "Adresse:",
        "Località:" => "Ort:",
        "Numeri di telefono:" => "Telefonnummern:",
        "Siti web:" => "Webseiten:",
        "Scheda web:" => "Webeintrag:",
        _ => it,
    }
}

fn decode(encoded: &str) -> String {
    use base64::{Engine as _, engine::general_purpose};
    let bytes = general_purpose::STANDARD
        .decode(encoded)
        .unwrap_or_default();
    let mut decoded = Vec::with_capacity(bytes.len());
    for (i, byte) in bytes.iter().enumerate() {
        decoded.push(byte ^ KEY[i % KEY.len()]);
    }
    String::from_utf8(decoded).unwrap_or_default()
}

pub(crate) fn normalize_phone(input: &str) -> Result<String, String> {
    let input = input.trim();
    if input.chars().enumerate().any(|(i, c)| {
        !c.is_ascii_digit() && !c.is_whitespace() && !"()-./".contains(c) && !(c == '+' && i == 0)
    }) {
        return Err("Inserisci un numero di telefono italiano valido.".into());
    }
    let compact: String = input
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '+')
        .collect();
    let number = if let Some(v) = compact
        .strip_prefix("+39")
        .or_else(|| compact.strip_prefix("0039"))
    {
        v
    } else if compact.starts_with('+') || compact.starts_with("00") {
        return Err("Inserisci un numero italiano, con prefisso +39 facoltativo.".into());
    } else {
        &compact
    };
    if !(5..=15).contains(&number.len()) {
        return Err("Inserisci un numero di telefono completo di prefisso.".into());
    }
    Ok(number.into())
}

pub fn search_directory(
    kind: DirectoryKind,
    what: &str,
    location: &str,
    page: i32,
    reverse: bool,
) -> Result<SearchResponse, String> {
    let settings = Settings::load();
    let language = crate::normalize_ui_language(&settings.ui_language);
    let country = if settings.places_country.is_empty() {
        crate::default_radio_country_code_for_ui_language(&settings.ui_language)
    } else {
        settings.places_country.as_str()
    };
    if language != "it"
        || (!reverse && matches!(kind, DirectoryKind::PagineGialle) && country != "it")
    {
        let primary = places::search(what, location, country, &language)?;
        return merge_activities(
            Ok(primary),
            Ok(SearchResponse {
                display_where: Some(location.into()),
                current_page: 1,
                is_last_page: true,
                results: Vec::new(),
                actual_directory_index: Some(1),
            }),
            location,
            1,
        );
    }
    if reverse || !matches!(kind, DirectoryKind::PagineGialle) || page > 1 {
        return search_directory_legacy(kind, what, location, page, reverse);
    }
    let (primary, legacy) = std::thread::scope(|scope| {
        let worker = scope.spawn(|| places::search(what, location, country, &language));
        let legacy = search_directory_legacy(kind, what, location, page, false);
        (
            worker
                .join()
                .unwrap_or_else(|_| Err("Ricerca luoghi interrotta.".into())),
            legacy,
        )
    });
    merge_activities(primary, legacy, location, page)
}

fn merge_activities(
    primary: Result<Vec<places::Place>, String>,
    legacy: Result<SearchResponse, String>,
    location: &str,
    page: i32,
) -> Result<SearchResponse, String> {
    let primary = match primary {
        Ok(items) if !items.is_empty() => items,
        _ => return legacy,
    };
    let mut response = legacy.unwrap_or_else(|_| SearchResponse {
        display_where: Some(location.into()),
        current_page: page,
        is_last_page: true,
        results: Vec::new(),
        actual_directory_index: Some(1),
    });
    // Keep the source for legacy details even if the existing PB fallback was used.
    let prefix = if response.actual_directory_index == Some(0) {
        "pb:"
    } else {
        "pg:"
    };
    response.results.retain(|item| {
        !primary.iter().any(|place| {
            places::same_place(
                place,
                &item.name,
                item.address.as_deref(),
                item.city.as_deref(),
                &item.phones,
            )
        })
    });
    for item in &mut response.results {
        item.id = format!("{prefix}{}", item.id);
    }
    let mut results: Vec<_> = primary
        .into_iter()
        .map(|place| {
            let detail = DetailResponse {
                body: place.body(&places::PlaceText::new(|key| {
                    let ui = current_ui_strings();
                    match key {
                        "category" => ui.places_category.clone(),
                        "address" => ui.places_address.clone(),
                        "phone" => ui.places_phone.clone(),
                        "website" => ui.places_website.clone(),
                        "rating" => ui.places_rating.clone(),
                        "reviews" => ui.places_reviews.clone(),
                        "unavailable" => ui.places_unavailable.clone(),
                        "status" => ui.places_status.clone(),
                        "hours" => ui.places_hours.clone(),
                        "hours_missing" => ui.places_hours_missing.clone(),
                        "today" => ui.places_today.clone(),
                        "tomorrow" => ui.places_tomorrow.clone(),
                        "range" => ui.places_range.clone(),
                        "next_day" => ui.places_next_day.clone(),
                        _ => key.into(),
                    }
                })),
            };
            SearchResult {
                id: format!("place:{}", place.id),
                name: place.nome,
                address: place.indirizzo,
                city: None,
                province: None,
                category: (!place.categorie.is_empty()).then(|| place.categorie.join(", ")),
                phones: place.telefoni,
                detail: Some(detail),
            }
        })
        .collect();
    results.extend(response.results);
    response.results = results;
    // Keep actual_directory_index for subsequent pages of the legacy provider.
    Ok(response)
}

fn search_directory_legacy(
    kind: DirectoryKind,
    what: &str,
    where_loc: &str,
    page: i32,
    reverse: bool,
) -> Result<SearchResponse, String> {
    let base_url = decode("BT9NUUx/HRUjWkodMRoTOlYsGzZeHTVfCiMeAT4c"); // http://mobile.italiaonline.it/
    let client_id = decode("HSlUThQ5Xh0="); // pbmobile
    let version = decode("XmUAD0M="); // 3.9.5

    let endpoint = match kind {
        DirectoryKind::PagineBianche => decode("Hi5YUxU4Qho="), // searchpb
        DirectoryKind::PagineGialle => decode("Hi5YUxU4Qh8="),  // searchpg
    };

    let mut mapped_what = if reverse {
        normalize_phone(what)?
    } else {
        what.to_string()
    };
    if let DirectoryKind::PagineGialle = kind
        && mapped_what.trim().eq_ignore_ascii_case("bar")
    {
        mapped_what = "bar caffetteria".to_string();
    }

    let mut url = format!("{base_url}{endpoint}?client={client_id}&version={version}");
    let what_enc = url::form_urlencoded::byte_serialize(mapped_what.as_bytes()).collect::<String>();
    url.push_str(&format!("&what={what_enc}"));
    if !reverse && !where_loc.trim().is_empty() {
        let where_enc =
            url::form_urlencoded::byte_serialize(where_loc.as_bytes()).collect::<String>();
        url.push_str(&format!("&where={where_enc}"));
    }
    if page > 1 {
        url.push_str(&format!("&page={page}"));
    }

    let client = Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client
        .get(&url)
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/136.0.0.0 Safari/537.36")
        .send()
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        if resp.status().as_u16() == 500 {
            if let DirectoryKind::PagineGialle = kind
                && let Ok(mut fb_resp) =
                    search_directory(DirectoryKind::PagineBianche, what, where_loc, page, reverse)
            {
                fb_resp.actual_directory_index = Some(0);
                return Ok(fb_resp);
            }
            if reverse {
                return Err("Servizio temporaneamente non disponibile. Riprova.".into());
            }
            return Ok(SearchResponse {
                display_where: None,
                results: vec![],
                current_page: 1,
                is_last_page: true,
                actual_directory_index: Some(if let DirectoryKind::PagineGialle = kind {
                    1
                } else {
                    0
                }),
            });
        }
        return Err(format!("Errore di rete: {}", resp.status()));
    }

    let text = resp.text().map_err(|e| e.to_string())?;
    let mut parsed = parse_search_response(&text)?;
    parsed.actual_directory_index = Some(if let DirectoryKind::PagineGialle = kind {
        1
    } else {
        0
    });
    if reverse {
        let number = normalize_phone(what)?;
        parsed.results.retain(|r| {
            r.phones
                .iter()
                .any(|p| normalize_phone(p).ok().as_deref() == Some(number.as_str()))
        });
    }
    Ok(parsed)
}

pub fn search_phone(what: &str, page: i32) -> Result<SearchResponse, String> {
    let number = normalize_phone(what)?;
    let mut combined = SearchResponse {
        display_where: None,
        current_page: page,
        is_last_page: true,
        results: Vec::new(),
        actual_directory_index: None,
    };
    let mut succeeded = false;
    let mut errors = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for kind in [DirectoryKind::PagineBianche, DirectoryKind::PagineGialle] {
        match search_directory(kind, &number, "", page, true) {
            Ok(response) => {
                succeeded = true;
                combined.is_last_page &= response.is_last_page;
                let prefix = if response.actual_directory_index == Some(1) {
                    "pg:"
                } else {
                    "pb:"
                };
                for mut item in response.results {
                    item.id = format!("{prefix}{}", item.id);
                    if seen.insert(item.id.clone()) {
                        combined.results.push(item);
                    }
                }
            }
            Err(error) => errors.push(error),
        }
    }
    if !succeeded || (combined.results.is_empty() && !errors.is_empty()) {
        return Err(errors.join("\n"));
    }
    Ok(combined)
}

pub fn detail_directory(
    kind: DirectoryKind,
    what: &str,
    where_loc: &str,
    id: &str,
) -> Result<DetailResponse, String> {
    for (prefix, source) in [
        ("pb:", DirectoryKind::PagineBianche),
        ("pg:", DirectoryKind::PagineGialle),
    ] {
        if let Some(id) = id.strip_prefix(prefix) {
            return detail_directory(source, what, "", id);
        }
    }
    let base_url = decode("BT9NUUx/HRUjWkodMRoTOlYsGzZeHTVfCiMeAT4c");
    let client_id = decode("HSlUThQ5Xh0=");
    let version = decode("XmUAD0M=");

    let endpoint = match kind {
        DirectoryKind::PagineBianche => decode("CS5NQB88Qho="), // detailpb
        DirectoryKind::PagineGialle => decode("CS5NQB88Qh8="),  // detailpg
    };

    let mut url = format!("{base_url}{endpoint}?client={client_id}&version={version}&id={id}");
    let what_enc = url::form_urlencoded::byte_serialize(what.as_bytes()).collect::<String>();
    url.push_str(&format!("&what={what_enc}"));
    if !where_loc.trim().is_empty() {
        let where_enc =
            url::form_urlencoded::byte_serialize(where_loc.as_bytes()).collect::<String>();
        url.push_str(&format!("&where={where_enc}"));
    }

    let client = Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client
        .get(&url)
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/136.0.0.0 Safari/537.36")
        .send()
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        return Err(format!("Errore di rete: {}", resp.status()));
    }

    let text = resp.text().map_err(|e| e.to_string())?;
    parse_detail_response(&text)
}

fn parse_search_response(xml_str: &str) -> Result<SearchResponse, String> {
    use quick_xml::Reader;
    use quick_xml::events::Event;

    let mut reader = Reader::from_str(xml_str);
    reader.config_mut().trim_text(true);

    let mut current_page = 1;
    let mut is_last_page = false;
    let mut results = Vec::new();
    let mut display_where = None;

    let mut in_result = false;
    let mut current_tag = String::new();

    let mut current_id = String::new();
    let mut current_name = String::new();
    let mut current_address = String::new();
    let mut current_city = String::new();
    let mut current_province = String::new();
    let mut current_category = String::new();
    let mut current_phones = Vec::new();
    let mut current_phone_number = String::new();

    let mut in_phone = false;

    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                current_tag = name.clone();
                if name == "result" {
                    in_result = true;
                    current_id.clear();
                    current_name.clear();
                    current_address.clear();
                    current_city.clear();
                    current_province.clear();
                    current_category.clear();
                    current_phones.clear();
                } else if name == "phone" {
                    in_phone = true;
                    current_phone_number.clear();
                }
            }
            Ok(Event::Text(e)) => {
                let raw_text = std::str::from_utf8(e.as_ref()).unwrap_or_default();
                let text = quick_xml::escape::unescape(raw_text)
                    .unwrap_or(std::borrow::Cow::Borrowed(raw_text))
                    .into_owned();
                if !in_result {
                    if current_tag == "current_page" {
                        if let Ok(p) = text.parse() {
                            current_page = p;
                        }
                    } else if current_tag == "isLastPage" {
                        is_last_page = text == "1";
                    } else if current_tag == "displayWhere" {
                        display_where = Some(text);
                    }
                } else {
                    if current_tag == "id" {
                        current_id = text;
                    } else if current_tag == "name" {
                        current_name = text;
                    } else if current_tag == "address" {
                        current_address = text;
                    } else if current_tag == "city" {
                        current_city = text;
                    } else if current_tag == "province" {
                        current_province = text;
                    } else if current_tag == "category" {
                        current_category = text;
                    } else if current_tag == "number" && in_phone {
                        current_phone_number = text;
                    }
                }
            }
            Ok(Event::End(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if name == "result" {
                    in_result = false;
                    if !current_id.is_empty() && !current_name.is_empty() {
                        results.push(SearchResult {
                            id: current_id.clone(),
                            name: current_name.clone(),
                            address: if current_address.is_empty() {
                                None
                            } else {
                                Some(current_address.clone())
                            },
                            city: if current_city.is_empty() {
                                None
                            } else {
                                Some(current_city.clone())
                            },
                            province: if current_province.is_empty() {
                                None
                            } else {
                                Some(current_province.clone())
                            },
                            category: if current_category.is_empty() {
                                None
                            } else {
                                Some(current_category.clone())
                            },
                            phones: current_phones.clone(),
                            detail: None,
                        });
                    }
                } else if name == "phone" {
                    in_phone = false;
                    if !current_phone_number.is_empty() {
                        current_phones.push(current_phone_number.clone());
                    }
                }
                current_tag.clear();
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => (),
        }
        buf.clear();
    }

    Ok(SearchResponse {
        display_where,
        results,
        current_page,
        is_last_page,
        actual_directory_index: None,
    })
}

fn parse_detail_response(xml_str: &str) -> Result<DetailResponse, String> {
    use quick_xml::Reader;
    use quick_xml::events::Event;

    let mut reader = Reader::from_str(xml_str);
    reader.config_mut().trim_text(true);

    let mut current_tag = String::new();
    let mut in_detail = false;
    let mut in_phone = false;
    let mut in_email = false;
    let mut in_website = false;

    let mut name = String::new();
    let mut description = String::new();
    let mut category = String::new();
    let mut address = String::new();
    let mut city = String::new();
    let mut province = String::new();
    let mut public_url = String::new();

    let mut phones = Vec::new();
    let mut current_phone = String::new();
    let mut emails = Vec::new();
    let mut current_email = String::new();
    let mut websites = Vec::new();
    let mut current_website = String::new();

    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                current_tag = tag.clone();
                if tag == "detail" {
                    in_detail = true;
                } else if tag == "phone" {
                    in_phone = true;
                    current_phone.clear();
                } else if tag == "email" {
                    in_email = true;
                    current_email.clear();
                } else if tag == "website" {
                    in_website = true;
                    current_website.clear();
                }
            }
            Ok(Event::Text(e)) => {
                let raw_text = std::str::from_utf8(e.as_ref()).unwrap_or_default();
                let text = quick_xml::escape::unescape(raw_text)
                    .unwrap_or(std::borrow::Cow::Borrowed(raw_text))
                    .into_owned();
                if in_detail {
                    match current_tag.as_str() {
                        "name" => name = text,
                        "description" => description = text,
                        "category" => category = text,
                        "address" if !in_email => address = text,
                        "city" => city = text,
                        "province" => province = text,
                        "public_url" => public_url = text,
                        "number" if in_phone => current_phone = text,
                        "address" if in_email => current_email = text,
                        "url" if in_website => current_website = text,
                        _ => {}
                    }
                }
            }
            Ok(Event::End(ref e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if tag == "detail" {
                    in_detail = false;
                } else if tag == "phone" {
                    in_phone = false;
                    if !current_phone.is_empty() {
                        phones.push(current_phone.clone());
                    }
                } else if tag == "email" {
                    in_email = false;
                    if !current_email.is_empty() {
                        emails.push(current_email.clone());
                    }
                } else if tag == "website" {
                    in_website = false;
                    if !current_website.is_empty() {
                        websites.push(current_website.clone());
                    }
                }
                current_tag.clear();
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => (),
        }
        buf.clear();
    }

    let mut body = String::new();
    body.push_str(if name.is_empty() { "Dettaglio" } else { &name });
    body.push_str("\r\n\r\n");

    let clean_desc = description
        .replace("<p>", "")
        .replace("</p>", "")
        .replace("/p", "")
        .replace("<br>", "\r\n")
        .replace("<br/>", "\r\n")
        .replace("<br />", "\r\n")
        .replace("<b>", "")
        .replace("</b>", "")
        .replace("<strong>", "")
        .replace("</strong>", "")
        .trim()
        .to_string();

    if !clean_desc.is_empty() {
        body.push_str(&clean_desc);
        body.push_str("\r\n\r\n");
    }
    if !category.is_empty() {
        body.push_str(&format!(
            "{} {}\r\n\r\n",
            directory_label("Categoria:"),
            category
        ));
    }
    let mut locality = String::new();
    if !city.is_empty() && !province.is_empty() {
        locality = format!("{} ({})", city, province);
    } else if !city.is_empty() {
        locality = city;
    } else if !province.is_empty() {
        locality = province;
    }

    if !address.is_empty() {
        body.push_str(directory_label("Indirizzo:"));
        body.push_str("\r\n");
        body.push_str(&address);
        body.push_str("\r\n");
        if !locality.is_empty() {
            body.push_str(&locality);
            body.push_str("\r\n");
        }
        body.push_str("\r\n");
    } else if !locality.is_empty() {
        body.push_str(directory_label("Località:"));
        body.push_str("\r\n");
        body.push_str(&locality);
        body.push_str("\r\n\r\n");
    }

    if !phones.is_empty() {
        body.push_str(directory_label("Numeri di telefono:"));
        body.push_str("\r\n");
        for p in &phones {
            body.push_str(p);
            body.push_str("\r\n");
        }
        body.push_str("\r\n");
    }

    if !emails.is_empty() {
        body.push_str("Email:\r\n");
        for e in &emails {
            body.push_str(e);
            body.push_str("\r\n");
        }
        body.push_str("\r\n");
    }

    if !websites.is_empty() {
        body.push_str(directory_label("Siti web:"));
        body.push_str("\r\n");
        for w in &websites {
            body.push_str(w);
            body.push_str("\r\n");
        }
        body.push_str("\r\n");
    }

    if !public_url.is_empty() {
        body.push_str(directory_label("Scheda web:"));
        body.push_str("\r\n");
        body.push_str(&public_url);
        body.push_str("\r\n");
    }

    Ok(DetailResponse {
        body: body.trim().to_string(),
    })
}

#[cfg(test)]
mod places_merge_tests {
    use super::*;

    fn place() -> places::Place {
        serde_json::from_value(serde_json::json!({"id":"new", "nome":"Bar Uno", "indirizzo":"Via Roma 12, Torino",
            "telefoni":[], "categorie":["Bar"], "descrizione":null, "stelle":4.5,"numero_recensioni":23})).unwrap()
    }

    fn legacy() -> SearchResponse {
        let mut response = parse_search_response("<response><status>200</status><current_page>1</current_page><isLastPage>0</isLastPage><results><result><id>one</id><name>Bar Uno</name><address>Via Roma 12</address><city>Torino</city></result><result><id>two</id><name>Bar Due</name></result></results></response>").unwrap();
        response.actual_directory_index = Some(1);
        response
    }

    #[test]
    fn new_results_precede_legacy_and_keep_detail_source_and_pagination() {
        let response = merge_activities(Ok(vec![place()]), Ok(legacy()), "Torino", 1).unwrap();
        assert_eq!(
            response
                .results
                .iter()
                .map(|r| r.id.as_str())
                .collect::<Vec<_>>(),
            ["place:new", "pg:two"]
        );
        assert!(!response.is_last_page);
        assert!(
            response.results[0]
                .detail
                .as_ref()
                .unwrap()
                .body
                .contains("23 recensioni")
        );
        assert!(response.results[1].detail.is_none());
    }

    #[test]
    fn source_failures_do_not_discard_available_results() {
        let response = merge_activities(Err("offline".into()), Ok(legacy()), "Torino", 1).unwrap();
        assert_eq!(response.results.len(), 2);
        let response =
            merge_activities(Ok(vec![place()]), Err("offline".into()), "Torino", 1).unwrap();
        assert_eq!(response.results[0].id, "place:new");
        assert!(
            merge_activities(Err("offline".into()), Err("offline".into()), "Torino", 1).is_err()
        );
    }

    #[test]
    fn old_white_pages_fallback_keeps_its_detail_source() {
        let mut old = legacy();
        old.actual_directory_index = Some(0);
        let response = merge_activities(Ok(vec![place()]), Ok(old), "Torino", 1).unwrap();
        assert_eq!(response.results[1].id, "pb:two");
        assert_eq!(response.actual_directory_index, Some(0));
    }
}

pub fn show_directory_results(
    parent: &Frame,
    tc_main: TextCtrl,
    mut directory_index: usize,
    query: &str,
    location: &str,
    response: SearchResponse,
    reverse: bool,
) {
    if let Some(idx) = response.actual_directory_index {
        directory_index = idx;
    }

    if response.results.is_empty() {
        let msg = MessageDialog::builder(
            parent,
            directory_label("Nessun risultato trovato."),
            directory_label("Cerca luoghi e attività"),
        )
        .with_style(MessageDialogStyle::OK | MessageDialogStyle::IconInformation)
        .build();
        msg.show_modal();
        msg.destroy();
        return;
    }

    let dialog = Dialog::builder(parent, directory_label("Risultati Ricerca"))
        .with_style(DialogStyle::DefaultDialogStyle | DialogStyle::ResizeBorder)
        .with_size(600, 300)
        .build();
    let panel = Panel::builder(&dialog).build();
    let root = BoxSizer::builder(Orientation::Vertical).build();

    let choice = Choice::builder(&panel).build();
    for res in &response.results {
        let mut label = res.name.clone();
        if let Some(ref addr) = res.address {
            label.push_str(&format!(" - {addr}"));
        }
        if let Some(ref city) = res.city {
            label.push_str(&format!(", {city}"));
        }
        choice.append(&label);
    }
    if !response.results.is_empty() {
        choice.set_selection(0);
    }

    root.add(&choice, 1, SizerFlag::Expand | SizerFlag::All, 10);

    let buttons = BoxSizer::builder(Orientation::Horizontal).build();

    let btn_prev = Button::builder(&panel)
        .with_label(directory_label("Pagina Precedente"))
        .build();
    let btn_next = Button::builder(&panel)
        .with_label(directory_label("Pagina Successiva"))
        .build();
    let btn_open = Button::builder(&panel)
        .with_label(directory_label("Apri"))
        .build();
    let btn_close = Button::builder(&panel)
        .with_label(directory_label("Chiudi"))
        .with_id(ID_CANCEL)
        .build();

    btn_prev.show(response.current_page > 1);
    btn_next.show(!response.is_last_page);
    btn_open.set_default();

    buttons.add(&btn_prev, 0, SizerFlag::All, 5);
    buttons.add(&btn_next, 0, SizerFlag::All, 5);
    buttons.add_spacer(1);
    buttons.add(&btn_open, 0, SizerFlag::All, 5);
    buttons.add(&btn_close, 0, SizerFlag::All, 5);

    root.add_sizer(&buttons, 0, SizerFlag::Expand, 0);

    panel.set_sizer(root, true);
    dialog.set_escape_id(ID_CANCEL);

    let query_c = query.to_string();
    let location_c = location.to_string();
    let current_page = response.current_page;
    let results_arc = Arc::new(response.results);

    let parent_c = *parent;
    let dialog_c = dialog;
    let choice_c = choice;
    let tc_main_c = tc_main;

    // Event handlers...

    let load_page = Rc::new({
        let p_clone = parent_c;
        let q_clone = query_c.clone();
        let l_clone = location_c.clone();
        let d_clone = dialog_c;
        let tc_c = tc_main_c;
        move |page: i32| {
            d_clone.end_modal(ID_OK);
            let kind = if directory_index == 1 {
                DirectoryKind::PagineGialle
            } else {
                DirectoryKind::PagineBianche
            };

            let progress = ProgressDialog::builder(
                &p_clone,
                directory_label("Caricamento..."),
                directory_label("Ricerca in corso..."),
                100,
            )
            .with_style(ProgressDialogStyle::Smooth)
            .build();

            let result_state = Arc::new(Mutex::new(None));
            let result_thread = Arc::clone(&result_state);
            let qc = q_clone.clone();
            let lc = l_clone.clone();

            std::thread::spawn(move || {
                let res = if reverse {
                    search_phone(&qc, page)
                } else {
                    search_directory(kind, &qc, &lc, page, false)
                };
                *result_thread.lock().unwrap() = Some(res);
            });

            let mut pv = 0;
            loop {
                std::thread::sleep(Duration::from_millis(150));
                if let Some(res) = result_state.lock().unwrap().take() {
                    progress.destroy();
                    match res {
                        Ok(resp) => {
                            show_directory_results(
                                &p_clone,
                                tc_c,
                                directory_index,
                                &q_clone,
                                &l_clone,
                                resp,
                                reverse,
                            );
                        }
                        Err(e) => {
                            show_message_dialog(&p_clone, directory_label("Errore"), &e);
                        }
                    }
                    break;
                }
                pv += 5;
                if pv >= 95 {
                    pv = 10;
                }
                progress.update(pv, Some(directory_label("Ricerca in corso...")));
            }
        }
    });

    let lp_prev = Rc::clone(&load_page);
    btn_prev.on_click(move |_| lp_prev(current_page - 1));
    let lp_next = Rc::clone(&load_page);
    btn_next.on_click(move |_| lp_next(current_page + 1));

    let results_for_open = Arc::clone(&results_arc);
    btn_open.on_click(move |_| {
        let sel = choice_c.get_selection();
        if sel.is_none() {
            return;
        }
        if let Some(res) = results_for_open.get(sel.unwrap() as usize) {
            let id = res.id.clone();
            let inline_detail = res.detail.clone();
            let d_clone = dialog_c;
            let kind = if directory_index == 1 {
                DirectoryKind::PagineGialle
            } else {
                DirectoryKind::PagineBianche
            };
            let qc = query_c.clone();
            let lc = location_c.clone();
            let tc_clone = tc_main_c;

            let progress = ProgressDialog::builder(
                &d_clone,
                directory_label("Caricamento..."),
                directory_label("Recupero dettagli..."),
                100,
            )
            .with_style(ProgressDialogStyle::Smooth)
            .build();

            let result_state = Arc::new(Mutex::new(None));
            let result_thread = Arc::clone(&result_state);

            std::thread::spawn(move || {
                let r = match inline_detail {
                    Some(detail) => Ok(detail),
                    None => detail_directory(kind, &qc, &lc, &id),
                };
                *result_thread.lock().unwrap() = Some(r);
            });

            let mut pv = 0;
            loop {
                std::thread::sleep(Duration::from_millis(150));
                if let Some(res) = result_state.lock().unwrap().take() {
                    progress.destroy();
                    match res {
                        Ok(detail) => {
                            tc_clone.set_value(&detail.body);
                            tc_clone.set_insertion_point(0);
                            d_clone.end_modal(ID_OK);
                        }
                        Err(e) => {
                            show_message_subdialog(&d_clone, directory_label("Errore"), &e);
                        }
                    }
                    break;
                }
                pv += 5;
                if pv >= 95 {
                    pv = 10;
                }
                progress.update(pv, Some(directory_label("Recupero dettagli...")));
            }
        }
    });

    let d_close = dialog;
    btn_close.on_click(move |_| d_close.end_modal(ID_CANCEL));

    dialog.show_modal();
    dialog.destroy();
}

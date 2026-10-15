use crate::{RadioFavorite, canonical_radio_name};
use serde::Deserialize;
use std::collections::HashSet;
use std::time::Duration;

#[derive(Clone, Default)]
pub(crate) struct Query {
    pub language: String,
    pub country: String,
    pub city: String,
    pub text: String,
}

impl Query {
    fn url(&self, page: usize) -> Result<url::Url, String> {
        let mut url =
            url::Url::parse("https://sonarpad.com/api/radioboss.php").map_err(|e| e.to_string())?;
        {
            let mut params = url.query_pairs_mut();
            params.append_pair("page", &page.to_string());
            // Match the search precedence used by the Windows catalog.
            if !self.text.trim().is_empty() {
                params.append_pair("q", self.text.trim());
            } else if !self.city.trim().is_empty() {
                params.append_pair("city", self.city.trim());
            } else if !self.country.trim().is_empty() {
                let country = match self.country.to_lowercase().as_str() {
                    "italy" => "italia".to_string(),
                    "united states" => "usa".to_string(),
                    "united kingdom" => "britain".to_string(),
                    "türkiye" => "turkey".to_string(),
                    _ => self.country.to_lowercase(),
                };
                params.append_pair("country", &country);
            } else if !self.language.is_empty() {
                params.append_pair(
                    "language",
                    crate::radio_browser_language_name(&self.language),
                );
            }
        }
        Ok(url)
    }
}

#[derive(Deserialize)]
struct Response {
    ok: bool,
    error: Option<String>,
    stations: Option<Vec<Station>>,
    #[serde(default)]
    has_more: bool,
}

#[derive(Deserialize)]
struct Station {
    #[serde(default)]
    name: String,
    stream: Option<String>,
    stream_direct: Option<String>,
}

fn parse(response: Response, language: &str) -> Result<(Vec<RadioFavorite>, bool), String> {
    if !response.ok {
        return Err(response
            .error
            .unwrap_or_else(|| "Radio catalog unavailable".into()));
    }
    let stations = response
        .stations
        .ok_or("Radio catalog response missing stations")?;
    let stations = stations
        .into_iter()
        .filter_map(|station| {
            let name = station.name.replace('&', "").trim().to_string();
            let stream = station
                .stream
                .filter(|s| !s.trim().is_empty())
                .or(station.stream_direct)?;
            let stream_url = stream.trim().to_string();
            if name.is_empty() || stream_url.is_empty() {
                return None;
            }
            Some(RadioFavorite {
                language_code: language.to_string(),
                name,
                stream_url,
            })
        })
        .collect();
    Ok((stations, response.has_more))
}

#[derive(Clone)]
pub(crate) struct Session {
    query: Query,
    pub results: Vec<RadioFavorite>,
    pub has_more: bool,
    cursor: usize,
    names: HashSet<String>,
    urls: HashSet<String>,
}

impl Session {
    pub fn new(query: Query, seeds: Vec<RadioFavorite>) -> Self {
        let mut session = Self {
            query,
            results: Vec::new(),
            has_more: true,
            cursor: 0,
            names: HashSet::new(),
            urls: HashSet::new(),
        };
        session.append(seeds);
        session
    }

    fn append(&mut self, stations: Vec<RadioFavorite>) {
        for station in stations {
            let name = canonical_radio_name(&station.name);
            let url = station.stream_url.trim().trim_end_matches('/').to_string();
            if name.is_empty()
                || url.is_empty()
                || self.names.contains(&name)
                || self.urls.contains(&url)
            {
                continue;
            }
            self.names.insert(name);
            self.urls.insert(url);
            self.results.push(station);
        }
    }

    pub fn needs_page(&self, page: usize) -> bool {
        self.has_more
            && (self.cursor == 0
                || self.results.len() < (page + 1) * crate::RADIO_RESULTS_PAGE_SIZE + 1)
    }

    pub fn load_page(&mut self, page: usize, cancelled: &std::sync::atomic::AtomicBool) {
        let result = self.load_with(page, |query, cursor| {
            if cancelled.load(std::sync::atomic::Ordering::SeqCst) {
                return Err("Radio search cancelled".into());
            }
            let response = reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(8))
                .user_agent("Sonarpad/1.0 (https://sonarpad.com)")
                .build()
                .map_err(|e| e.to_string())?
                .get(query.url(cursor)?)
                .send()
                .and_then(|r| r.error_for_status())
                .and_then(|r| r.json::<Response>())
                .map_err(|e| e.to_string())?;
            parse(response, &query.language)
        });
        if let Err(error) = result {
            crate::append_podcast_log(&format!(
                "Additional radio catalog page {}: {error}",
                self.cursor
            ));
        }
    }

    fn load_with(
        &mut self,
        page: usize,
        mut fetch: impl FnMut(&Query, usize) -> Result<(Vec<RadioFavorite>, bool), String>,
    ) -> Result<(), String> {
        let target = (page + 1) * crate::RADIO_RESULTS_PAGE_SIZE + 1;
        // Fetch the additional source once even when the existing catalog fills a page.
        // Cursor counts server pages, never filtered or deduplicated stations.
        while self.has_more && (self.cursor == 0 || self.results.len() < target) {
            match fetch(&self.query, self.cursor) {
                Ok((stations, more)) => {
                    self.cursor += 1;
                    self.has_more = more && self.cursor < 101;
                    self.append(stations);
                }
                Err(error) => {
                    self.has_more = false;
                    return Err(error);
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn station(i: usize) -> RadioFavorite {
        RadioFavorite {
            language_code: "it".into(),
            name: format!("Radio {i}"),
            stream_url: format!("https://example.com/{i}"),
        }
    }
    #[test]
    fn pages_are_stable_and_previous_does_not_fetch_again() {
        let mut session = Session::new(Query::default(), vec![station(0)]);
        let mut calls = Vec::new();
        let mut fetch = |_: &Query, page: usize| {
            calls.push(page);
            Ok((
                (page * 30..(page + 1) * 30).map(station).collect(),
                page < 2,
            ))
        };
        session.load_with(0, &mut fetch).unwrap();
        let first: Vec<_> = session
            .results
            .iter()
            .take(25)
            .map(|s| s.name.clone())
            .collect();
        session.load_with(1, &mut fetch).unwrap();
        session.load_with(0, &mut fetch).unwrap();
        session.load_with(3, &mut fetch).unwrap();
        assert_eq!(
            first,
            session
                .results
                .iter()
                .take(25)
                .map(|s| s.name.clone())
                .collect::<Vec<_>>()
        );
        assert_eq!(calls, [0, 1, 2]);
        assert_eq!(session.results.len(), 90);
        assert!(!session.has_more);
    }
    #[test]
    fn duplicate_and_empty_pages_do_not_end_search_early() {
        let mut session = Session::new(Query::default(), vec![station(0)]);
        session
            .load_with(0, |_, page| {
                Ok((
                    match page {
                        0 => vec![station(0)],
                        1 => vec![],
                        _ => vec![station(1)],
                    },
                    page < 2,
                ))
            })
            .unwrap();
        assert_eq!(session.cursor, 3);
        assert_eq!(session.results.len(), 2);
    }
    #[test]
    fn source_failure_preserves_existing_results() {
        let mut session = Session::new(Query::default(), vec![station(1)]);
        assert!(session.load_with(0, |_, _| Err("offline".into())).is_err());
        assert_eq!(session.results.len(), 1);
        assert!(!session.has_more);
    }
    #[test]
    fn parser_accepts_direct_stream_and_filters_invalid_stations() {
        let response = serde_json::from_str(r#"{"ok":true,"stations":[{"name":"Radio","stream":" ","stream_direct":"https://example.com/live"},{"name":"Empty"}],"has_more":true}"#).unwrap();
        let (stations, more) = parse(response, "it").unwrap();
        assert_eq!(stations.len(), 1);
        assert_eq!(stations[0].stream_url, "https://example.com/live");
        assert!(more);
    }
    #[test]
    fn cancelled_search_does_not_start_network_request() {
        let mut session = Session::new(Query::default(), vec![station(1)]);
        session.load_page(0, &std::sync::atomic::AtomicBool::new(true));
        assert_eq!(session.cursor, 0);
        assert_eq!(session.results.len(), 1);
    }

    #[test]
    fn duplicate_urls_and_names_are_removed_across_sources() {
        let mut alternate = station(1);
        alternate.name = "Different name, same stream".into();
        let mut same_name = station(2);
        same_name.name = station(1).name;
        let session = Session::new(Query::default(), vec![station(1), alternate, same_name]);
        assert_eq!(session.results.len(), 1);
    }

    #[test]
    fn query_encodes_keywords_and_maps_country() {
        let query = Query {
            text: "rock & roll".into(),
            ..Query::default()
        };
        let url = query.url(2).unwrap();
        let params: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(params["q"], "rock & roll");
        assert_eq!(params["page"], "2");
        let query = Query {
            country: "Italy".into(),
            ..Query::default()
        };
        assert!(query.url(0).unwrap().as_str().contains("country=italia"));
    }
}

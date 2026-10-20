use crate::Settings;
use feed_rs::parser;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::io::Cursor;

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct PodcastEpisode {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub link: String,
    #[serde(default)]
    pub audio_url: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub guid: String,
    #[serde(default)]
    pub published_date: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PodcastSource {
    pub title: String,
    pub url: String,
    #[serde(default)]
    pub episodes: Vec<PodcastEpisode>,
}

#[derive(Debug, Clone)]
pub struct PodcastSearchResult {
    pub title: String,
    pub artist: String,
    pub feed_url: String,
    pub collection_id: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct PodcastCategory {
    pub id: u32,
    pub name: String,
}

#[derive(Debug, Deserialize)]
struct ItunesSearchResponse {
    #[serde(default)]
    results: Vec<ItunesSearchItem>,
}

#[derive(Debug, Deserialize)]
struct ItunesSearchItem {
    #[serde(rename = "collectionId")]
    collection_id: Option<u64>,
    #[serde(rename = "collectionName")]
    collection_name: Option<String>,
    #[serde(rename = "artistName")]
    artist_name: Option<String>,
    #[serde(rename = "feedUrl")]
    feed_url: Option<String>,
    #[serde(rename = "primaryGenreId")]
    primary_genre_id: Option<u32>,
    #[serde(rename = "genreIds")]
    genre_ids: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct SpreakerSearchResponse {
    response: SpreakerSearchPayload,
}

#[derive(Debug, Deserialize)]
struct SpreakerSearchPayload {
    #[serde(default)]
    items: Vec<SpreakerShowItem>,
}

#[derive(Debug, Deserialize)]
struct SpreakerShowItem {
    show_id: u64,
    #[serde(default)]
    title: String,
    #[serde(default)]
    author_name: Option<String>,
    #[serde(default)]
    author: Option<SpreakerAuthor>,
}

#[derive(Debug, Deserialize)]
struct SpreakerAuthor {
    #[serde(default)]
    fullname: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SpreakerShowResponse {
    response: SpreakerShowPayload,
}

#[derive(Debug, Deserialize)]
struct SpreakerShowPayload {
    show: SpreakerShowItem,
}

pub fn normalize_url(input: &str) -> String {
    let s = input.trim();
    if s.is_empty() {
        return String::new();
    }
    if s.starts_with("//") {
        return format!("https:{s}");
    }
    if s.starts_with("http://") || s.starts_with("https://") {
        return s.to_string();
    }
    format!("https://{s}")
}

pub fn apple_categories_it() -> Vec<PodcastCategory> {
    vec![
        PodcastCategory {
            id: 1301,
            name: "Arti".to_string(),
        },
        PodcastCategory {
            id: 1321,
            name: "Affari".to_string(),
        },
        PodcastCategory {
            id: 1303,
            name: "Commedia".to_string(),
        },
        PodcastCategory {
            id: 1304,
            name: "Istruzione".to_string(),
        },
        PodcastCategory {
            id: 1483,
            name: "Narrativa".to_string(),
        },
        PodcastCategory {
            id: 1511,
            name: "Governo".to_string(),
        },
        PodcastCategory {
            id: 1512,
            name: "Salute e fitness".to_string(),
        },
        PodcastCategory {
            id: 1487,
            name: "Storia".to_string(),
        },
        PodcastCategory {
            id: 1305,
            name: "Bambini e famiglia".to_string(),
        },
        PodcastCategory {
            id: 1502,
            name: "Tempo libero".to_string(),
        },
        PodcastCategory {
            id: 1310,
            name: "Musica".to_string(),
        },
        PodcastCategory {
            id: 1489,
            name: "Notizie".to_string(),
        },
        PodcastCategory {
            id: 1314,
            name: "Religione e spiritualita".to_string(),
        },
        PodcastCategory {
            id: 1533,
            name: "Scienza".to_string(),
        },
        PodcastCategory {
            id: 1324,
            name: "Societa e cultura".to_string(),
        },
        PodcastCategory {
            id: 1545,
            name: "Sport".to_string(),
        },
        PodcastCategory {
            id: 1318,
            name: "Tecnologia".to_string(),
        },
        PodcastCategory {
            id: 1488,
            name: "True crime".to_string(),
        },
        PodcastCategory {
            id: 1309,
            name: "TV e film".to_string(),
        },
    ]
}

pub fn apple_categories(ui_language: &str) -> Vec<PodcastCategory> {
    if ui_language.eq_ignore_ascii_case("cs") {
        vec![
            PodcastCategory {
                id: 1301,
                name: "Umění".to_string(),
            },
            PodcastCategory {
                id: 1321,
                name: "Byznys".to_string(),
            },
            PodcastCategory {
                id: 1303,
                name: "Komedie".to_string(),
            },
            PodcastCategory {
                id: 1304,
                name: "Vzdělávání".to_string(),
            },
            PodcastCategory {
                id: 1483,
                name: "Fikce".to_string(),
            },
            PodcastCategory {
                id: 1511,
                name: "Vláda".to_string(),
            },
            PodcastCategory {
                id: 1512,
                name: "Zdraví a fitness".to_string(),
            },
            PodcastCategory {
                id: 1487,
                name: "Historie".to_string(),
            },
            PodcastCategory {
                id: 1305,
                name: "Děti a rodina".to_string(),
            },
            PodcastCategory {
                id: 1502,
                name: "Volný čas".to_string(),
            },
            PodcastCategory {
                id: 1310,
                name: "Hudba".to_string(),
            },
            PodcastCategory {
                id: 1489,
                name: "Zprávy".to_string(),
            },
            PodcastCategory {
                id: 1314,
                name: "Náboženství a spiritualita".to_string(),
            },
            PodcastCategory {
                id: 1533,
                name: "Věda".to_string(),
            },
            PodcastCategory {
                id: 1324,
                name: "Společnost a kultura".to_string(),
            },
            PodcastCategory {
                id: 1545,
                name: "Sport".to_string(),
            },
            PodcastCategory {
                id: 1318,
                name: "Technologie".to_string(),
            },
            PodcastCategory {
                id: 1488,
                name: "True crime".to_string(),
            },
            PodcastCategory {
                id: 1309,
                name: "TV a film".to_string(),
            },
        ]
    } else if ui_language.eq_ignore_ascii_case("fr") {
        vec![
            PodcastCategory {
                id: 1301,
                name: "Arts".to_string(),
            },
            PodcastCategory {
                id: 1321,
                name: "Affaires".to_string(),
            },
            PodcastCategory {
                id: 1303,
                name: "Comédie".to_string(),
            },
            PodcastCategory {
                id: 1304,
                name: "Éducation".to_string(),
            },
            PodcastCategory {
                id: 1483,
                name: "Fiction".to_string(),
            },
            PodcastCategory {
                id: 1511,
                name: "Gouvernement".to_string(),
            },
            PodcastCategory {
                id: 1512,
                name: "Santé et forme".to_string(),
            },
            PodcastCategory {
                id: 1487,
                name: "Histoire".to_string(),
            },
            PodcastCategory {
                id: 1305,
                name: "Enfants et famille".to_string(),
            },
            PodcastCategory {
                id: 1502,
                name: "Loisirs".to_string(),
            },
            PodcastCategory {
                id: 1310,
                name: "Musique".to_string(),
            },
            PodcastCategory {
                id: 1489,
                name: "Actualités".to_string(),
            },
            PodcastCategory {
                id: 1314,
                name: "Religion et spiritualité".to_string(),
            },
            PodcastCategory {
                id: 1533,
                name: "Sciences".to_string(),
            },
            PodcastCategory {
                id: 1324,
                name: "Société et culture".to_string(),
            },
            PodcastCategory {
                id: 1545,
                name: "Sports".to_string(),
            },
            PodcastCategory {
                id: 1318,
                name: "Technologie".to_string(),
            },
            PodcastCategory {
                id: 1488,
                name: "True crime".to_string(),
            },
            PodcastCategory {
                id: 1309,
                name: "TV et cinéma".to_string(),
            },
        ]
    } else if ui_language.eq_ignore_ascii_case("en") {
        vec![
            PodcastCategory {
                id: 1301,
                name: "Arts".to_string(),
            },
            PodcastCategory {
                id: 1321,
                name: "Business".to_string(),
            },
            PodcastCategory {
                id: 1303,
                name: "Comedy".to_string(),
            },
            PodcastCategory {
                id: 1304,
                name: "Education".to_string(),
            },
            PodcastCategory {
                id: 1483,
                name: "Fiction".to_string(),
            },
            PodcastCategory {
                id: 1511,
                name: "Government".to_string(),
            },
            PodcastCategory {
                id: 1512,
                name: "Health & Fitness".to_string(),
            },
            PodcastCategory {
                id: 1487,
                name: "History".to_string(),
            },
            PodcastCategory {
                id: 1305,
                name: "Kids & Family".to_string(),
            },
            PodcastCategory {
                id: 1502,
                name: "Leisure".to_string(),
            },
            PodcastCategory {
                id: 1310,
                name: "Music".to_string(),
            },
            PodcastCategory {
                id: 1489,
                name: "News".to_string(),
            },
            PodcastCategory {
                id: 1314,
                name: "Religion & Spirituality".to_string(),
            },
            PodcastCategory {
                id: 1533,
                name: "Science".to_string(),
            },
            PodcastCategory {
                id: 1324,
                name: "Society & Culture".to_string(),
            },
            PodcastCategory {
                id: 1545,
                name: "Sports".to_string(),
            },
            PodcastCategory {
                id: 1318,
                name: "Technology".to_string(),
            },
            PodcastCategory {
                id: 1488,
                name: "True Crime".to_string(),
            },
            PodcastCategory {
                id: 1309,
                name: "TV & Film".to_string(),
            },
        ]
    } else {
        apple_categories_it()
    }
}

fn podcast_country_code() -> &'static str {
    match Settings::load().ui_language.as_str() {
        "it" => "it",
        "fr" => "fr",
        "es" => "es",
        "pt" => "pt",
        "cs" => "cz",
        "pl" => "pl",
        _ => "us",
    }
}

pub async fn search_itunes_podcasts(keyword: &str) -> Result<Vec<PodcastSearchResult>, String> {
    let trimmed = keyword.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    let query: String = url::form_urlencoded::byte_serialize(trimmed.as_bytes()).collect();
    let country = podcast_country_code();
    let url = format!(
        "https://itunes.apple.com/search?media=podcast&entity=podcast&term={query}&country={country}&limit=20"
    );
    fetch_itunes_results(&url).await
}

pub async fn search_podcasts(keyword: &str) -> Result<Vec<PodcastSearchResult>, String> {
    let trimmed = keyword.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    let (itunes_results, spreaker_results) = tokio::join!(
        search_itunes_podcasts(trimmed),
        search_spreaker_podcasts(trimmed)
    );

    crate::append_podcast_log(&format!(
        "podcast.search.catalogs: apple={:?}, spreaker={:?}",
        itunes_results.as_ref().map(Vec::len),
        spreaker_results.as_ref().map(Vec::len)
    ));
    let mut combined = merge_catalog_results(itunes_results, spreaker_results)?;
    let exact = podcast_title_key(trimmed);
    combined.sort_by_key(|item| podcast_title_key(&item.title) != exact);
    Ok(combined)
}

fn merge_catalog_results(
    first: Result<Vec<PodcastSearchResult>, String>,
    second: Result<Vec<PodcastSearchResult>, String>,
) -> Result<Vec<PodcastSearchResult>, String> {
    let mut combined = match (first, second) {
        (Ok(mut first), Ok(second)) => {
            first.extend(second);
            first
        }
        (Ok(results), Err(error)) | (Err(error), Ok(results)) => {
            crate::append_podcast_log(&format!("podcast.search.partial_failure: {error}"));
            results
        }
        (Err(first), Err(second)) => return Err(format!("{first}; {second}")),
    };
    dedup_search_results(&mut combined);
    Ok(combined)
}

fn podcast_title_key(title: &str) -> String {
    title
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn spreaker_result(item: SpreakerShowItem) -> PodcastSearchResult {
    PodcastSearchResult {
        title: item.title,
        artist: item
            .author_name
            .filter(|name| !name.trim().is_empty())
            .or_else(|| item.author.and_then(|author| author.fullname))
            .unwrap_or_default(),
        feed_url: format!(
            "https://www.spreaker.com/show/{}/episodes/feed",
            item.show_id
        ),
        collection_id: None,
    }
}

async fn search_spreaker_podcasts(keyword: &str) -> Result<Vec<PodcastSearchResult>, String> {
    let query: String = url::form_urlencoded::byte_serialize(keyword.trim().as_bytes()).collect();
    let url = format!("https://api.spreaker.com/v2/search?q={query}&type=shows&limit=20");

    let client = reqwest::Client::builder()
        .user_agent(
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
             (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
        )
        .redirect(reqwest::redirect::Policy::limited(10))
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|err| err.to_string())?;

    let (search, exact) = tokio::join!(
        fetch_spreaker_search(&client, &url),
        fetch_spreaker_exact(&client, keyword)
    );
    crate::append_podcast_log(&format!(
        "podcast.search.spreaker: catalog={:?}, exact={:?}",
        search.as_ref().map(Vec::len),
        exact.as_ref().map(Vec::len)
    ));
    // Prefer the direct result: it also includes the author's display name.
    merge_catalog_results(exact, search)
}

async fn fetch_spreaker_search(
    client: &reqwest::Client,
    url: &str,
) -> Result<Vec<PodcastSearchResult>, String> {
    let parsed: SpreakerSearchResponse = client
        .get(url)
        .send()
        .await
        .map_err(|err| err.to_string())?
        .error_for_status()
        .map_err(|err| err.to_string())?
        .json()
        .await
        .map_err(|err| err.to_string())?;
    Ok(parsed
        .response
        .items
        .into_iter()
        .filter(|item| !item.title.trim().is_empty())
        .map(spreaker_result)
        .collect())
}

async fn fetch_spreaker_exact(
    client: &reqwest::Client,
    keyword: &str,
) -> Result<Vec<PodcastSearchResult>, String> {
    // Public shows can disappear from Spreaker's search index while their
    // permalink still resolves. Do not special-case any title or show ID.
    let key = podcast_title_key(keyword);
    if key.is_empty() {
        return Ok(Vec::new());
    }
    let encoded: String = url::form_urlencoded::byte_serialize(key.as_bytes()).collect();
    let response = client
        .get(format!("https://api.spreaker.com/v2/shows/{encoded}"))
        .send()
        .await
        .map_err(|err| err.to_string())?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(Vec::new());
    }
    let parsed: SpreakerShowResponse = response
        .error_for_status()
        .map_err(|err| err.to_string())?
        .json()
        .await
        .map_err(|err| err.to_string())?;
    Ok(exact_spreaker_result(parsed.response.show, &key))
}

fn exact_spreaker_result(item: SpreakerShowItem, key: &str) -> Vec<PodcastSearchResult> {
    // A historical permalink may now identify a renamed, unrelated show.
    if podcast_title_key(&item.title) != key {
        return Vec::new();
    }
    vec![spreaker_result(item)]
}

pub async fn search_itunes_category(category_id: u32) -> Result<Vec<PodcastSearchResult>, String> {
    let country = podcast_country_code();
    let top_url = format!(
        "https://itunes.apple.com/{country}/rss/toppodcasts/limit=20/genre={category_id}/json"
    );
    let top_ids = fetch_apple_top_ids(&top_url).await?;
    if !top_ids.is_empty()
        && let Some(lookup_url) = build_lookup_url(&top_ids)
    {
        let results = fetch_itunes_results_filtered(&lookup_url, Some(category_id)).await?;
        if !results.is_empty() {
            return Ok(order_results_by_ids(results, &top_ids));
        }
    }

    let url = format!(
        "https://itunes.apple.com/search?media=podcast&entity=podcast&genreId={category_id}&country={country}&limit=20"
    );
    let results = fetch_itunes_results_filtered(&url, Some(category_id)).await?;
    if results.is_empty() {
        fetch_itunes_results_filtered(&url, None).await
    } else {
        Ok(results)
    }
}

async fn fetch_itunes_results(url: &str) -> Result<Vec<PodcastSearchResult>, String> {
    fetch_itunes_results_filtered(url, None).await
}

async fn fetch_itunes_results_filtered(
    url: &str,
    genre_id: Option<u32>,
) -> Result<Vec<PodcastSearchResult>, String> {
    let client = reqwest::Client::builder()
        .user_agent(
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
             (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
        )
        .redirect(reqwest::redirect::Policy::limited(10))
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|err| err.to_string())?;

    let bytes = client
        .get(url)
        .send()
        .await
        .map_err(|err| err.to_string())?
        .error_for_status()
        .map_err(|err| err.to_string())?
        .bytes()
        .await
        .map_err(|err| err.to_string())?;

    let parsed: ItunesSearchResponse =
        serde_json::from_slice(&bytes).map_err(|err| err.to_string())?;
    Ok(itunes_items_to_results(parsed.results, genre_id))
}

pub async fn fetch_source(source: &PodcastSource) -> Result<PodcastSource, String> {
    let url = normalize_url(&source.url);
    if url.is_empty() {
        return Err("URL podcast vuoto".to_string());
    }

    let client = reqwest::Client::builder()
        .user_agent(
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
             (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
        )
        .redirect(reqwest::redirect::Policy::limited(10))
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|err| err.to_string())?;

    let bytes = client
        .get(&url)
        .send()
        .await
        .map_err(|err| err.to_string())?
        .error_for_status()
        .map_err(|err| err.to_string())?
        .bytes()
        .await
        .map_err(|err| err.to_string())?;

    let (title, episodes) = parse_feed(bytes.to_vec(), &source.title)?;
    Ok(PodcastSource {
        title,
        url,
        episodes,
    })
}

fn parse_feed(
    bytes: Vec<u8>,
    fallback_title: &str,
) -> Result<(String, Vec<PodcastEpisode>), String> {
    let cursor = Cursor::new(bytes);
    let feed = parser::parse(cursor).map_err(|err| err.to_string())?;
    let title = feed
        .title
        .map(|title| title.content)
        .unwrap_or_else(|| fallback_title.to_string());

    let mut episodes = Vec::new();
    for entry in feed.entries {
        let title = entry
            .title
            .as_ref()
            .map(|value| value.content.clone())
            .unwrap_or_else(|| "Episodio senza titolo".to_string());
        let link = select_entry_link(&entry).unwrap_or_default();
        let audio_url = select_enclosure_url(&entry).unwrap_or_default();
        let guid = if !entry.id.trim().is_empty() {
            entry.id.clone()
        } else if !link.trim().is_empty() {
            link.clone()
        } else if !audio_url.trim().is_empty() {
            audio_url.clone()
        } else {
            title.clone()
        };
        let description = entry
            .summary
            .as_ref()
            .map(|value| value.content.clone())
            .or_else(|| {
                entry
                    .content
                    .as_ref()
                    .and_then(|content| content.body.clone())
            })
            .unwrap_or_default();

        let published_date = entry
            .published
            .or(entry.updated)
            .map(|value| value.date_naive().format("%Y-%m-%d").to_string())
            .unwrap_or_default();

        episodes.push(PodcastEpisode {
            title,
            link,
            audio_url,
            description,
            guid,
            published_date,
        });
    }

    dedup_episodes(&mut episodes);
    Ok((title, episodes))
}

fn select_entry_link(entry: &feed_rs::model::Entry) -> Option<String> {
    for link in &entry.links {
        let href = link.href.trim();
        if href.is_empty() {
            continue;
        }
        let rel = link.rel.as_deref().unwrap_or("");
        if rel.is_empty() || rel.eq_ignore_ascii_case("alternate") {
            return Some(href.to_string());
        }
    }
    entry
        .links
        .iter()
        .find(|link| !link.href.trim().is_empty())
        .map(|link| link.href.trim().to_string())
}

fn select_enclosure_url(entry: &feed_rs::model::Entry) -> Option<String> {
    if let Some(content) = entry.content.as_ref()
        && let Some(src) = content.src.as_ref()
    {
        return Some(src.href.clone());
    }

    for link in &entry.links {
        let href = link.href.trim();
        if href.is_empty() {
            continue;
        }
        let rel = link.rel.as_deref().unwrap_or("");
        let media_type = link.media_type.as_deref().unwrap_or("");
        if rel.eq_ignore_ascii_case("enclosure")
            || media_type.to_ascii_lowercase().starts_with("audio/")
        {
            return Some(href.to_string());
        }
    }

    for link in &entry.links {
        let href = link.href.trim();
        if href.is_empty() {
            continue;
        }
        let media_type = link.media_type.as_deref().unwrap_or("");
        if media_type.to_ascii_lowercase().starts_with("video/") {
            return Some(href.to_string());
        }
    }

    for media in &entry.media {
        for content in &media.content {
            if let Some(url) = content.url.as_ref() {
                let url_str = url.as_str().to_ascii_lowercase();
                let has_media_ext = url_str.ends_with(".mp3")
                    || url_str.ends_with(".m4a")
                    || url_str.ends_with(".aac")
                    || url_str.ends_with(".ogg")
                    || url_str.ends_with(".opus")
                    || url_str.ends_with(".wav")
                    || url_str.contains("/audio.mp3")
                    || url_str.contains("/audio.");
                let is_embed = url_str.contains("/embed");
                if has_media_ext && !is_embed {
                    return Some(url.to_string());
                }
            }
        }
    }

    for media in &entry.media {
        for content in &media.content {
            if let Some(url) = content.url.as_ref() {
                let url_str = url.as_str().to_ascii_lowercase();
                let is_embed = url_str.contains("/embed");
                if let Some(media_type) = content.content_type.as_ref() {
                    let media_type = media_type.to_string().to_ascii_lowercase();
                    if (media_type.starts_with("audio/") || media_type.starts_with("video/"))
                        && !is_embed
                    {
                        return Some(url.to_string());
                    }
                }
            }
        }
    }

    for media in &entry.media {
        for content in &media.content {
            if let Some(url) = content.url.as_ref() {
                let url_str = url.as_str().to_ascii_lowercase();
                if !url_str.contains("/embed") {
                    return Some(url.to_string());
                }
            }
        }
    }

    None
}

fn dedup_episodes(episodes: &mut Vec<PodcastEpisode>) {
    let mut seen = std::collections::HashSet::new();
    episodes.retain(|episode| {
        let key = if !episode.guid.trim().is_empty() {
            format!("guid:{}", episode.guid.trim())
        } else {
            format!("link:{}", episode.link.trim())
        };
        seen.insert(key)
    });
}

async fn fetch_apple_top_ids(url: &str) -> Result<Vec<u64>, String> {
    let client = reqwest::Client::builder()
        .user_agent(
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
             (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
        )
        .redirect(reqwest::redirect::Policy::limited(10))
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|err| err.to_string())?;

    let bytes = client
        .get(url)
        .send()
        .await
        .map_err(|err| err.to_string())?
        .error_for_status()
        .map_err(|err| err.to_string())?
        .bytes()
        .await
        .map_err(|err| err.to_string())?;

    Ok(parse_apple_top_ids(&bytes))
}

fn parse_apple_top_ids(bytes: &[u8]) -> Vec<u64> {
    let mut ids = Vec::new();
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(bytes) else {
        return ids;
    };
    let Some(entries) = value.get("feed").and_then(|feed| feed.get("entry")) else {
        return ids;
    };
    let list = match entries {
        serde_json::Value::Array(items) => items.clone(),
        other => vec![other.clone()],
    };
    for entry in list {
        if let Some(id_value) = entry
            .get("id")
            .and_then(|id| id.get("attributes"))
            .and_then(|attrs| attrs.get("im:id"))
            .and_then(|value| value.as_str())
            && let Ok(id) = id_value.parse::<u64>()
        {
            ids.push(id);
        }
    }
    ids
}

fn build_lookup_url(ids: &[u64]) -> Option<String> {
    if ids.is_empty() {
        return None;
    }
    let joined = ids
        .iter()
        .map(|id| id.to_string())
        .collect::<Vec<String>>()
        .join(",");
    let country = podcast_country_code();
    Some(format!(
        "https://itunes.apple.com/lookup?id={joined}&country={country}"
    ))
}

fn order_results_by_ids(
    results: Vec<PodcastSearchResult>,
    ids: &[u64],
) -> Vec<PodcastSearchResult> {
    let mut by_id = std::collections::HashMap::new();
    for result in results {
        if let Some(id) = result.collection_id {
            by_id.insert(id, result);
        }
    }

    let mut ordered = Vec::new();
    for id in ids {
        if let Some(result) = by_id.remove(id) {
            ordered.push(result);
        }
    }
    ordered
}

fn itunes_items_to_results(
    items: Vec<ItunesSearchItem>,
    genre_id: Option<u32>,
) -> Vec<PodcastSearchResult> {
    let mut results = Vec::new();
    for item in items {
        if genre_id.is_some() && !itunes_item_matches_genre(&item, genre_id.unwrap_or_default()) {
            continue;
        }
        if let (Some(title), Some(feed_url)) = (item.collection_name, item.feed_url) {
            results.push(PodcastSearchResult {
                title,
                artist: item.artist_name.unwrap_or_default(),
                feed_url,
                collection_id: item.collection_id,
            });
        }
    }
    results
}

fn itunes_item_matches_genre(item: &ItunesSearchItem, genre_id: u32) -> bool {
    if let Some(ids) = item.genre_ids.as_ref() {
        return ids.iter().any(|id| id == &genre_id.to_string());
    }
    if let Some(primary) = item.primary_genre_id {
        return primary == genre_id;
    }
    false
}

fn dedup_search_results(results: &mut Vec<PodcastSearchResult>) {
    let mut seen_feed_urls = HashSet::new();
    results.retain(|result| {
        let feed_key = normalize_url(&result.feed_url).to_lowercase();

        if !feed_key.is_empty() && !seen_feed_urls.insert(feed_key) {
            return false;
        }

        true
    });
}

#[cfg(test)]
mod search_tests {
    use super::*;

    fn result(title: &str, feed: &str) -> PodcastSearchResult {
        PodcastSearchResult {
            title: title.into(),
            artist: String::new(),
            feed_url: feed.into(),
            collection_id: None,
        }
    }

    #[test]
    fn one_failed_catalog_keeps_the_other_results() {
        for reverse in [false, true] {
            let success = Ok(vec![result("A", "https://example.org/a")]);
            let failure = Err("HTTP 503".into());
            let merged = if reverse {
                merge_catalog_results(failure, success)
            } else {
                merge_catalog_results(success, failure)
            }
            .unwrap();
            assert_eq!(merged.len(), 1);
        }
    }

    #[test]
    fn both_failed_catalogs_remain_an_error() {
        let error = merge_catalog_results(
            Err("Apple unavailable".into()),
            Err("Spreaker unavailable".into()),
        )
        .unwrap_err();
        assert!(error.contains("Apple unavailable") && error.contains("Spreaker unavailable"));
    }

    #[test]
    fn empty_success_is_not_an_error() {
        assert!(
            merge_catalog_results(Ok(Vec::new()), Err("offline".into()))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn different_feeds_with_the_same_title_are_not_discarded() {
        let merged = merge_catalog_results(
            Ok(vec![result("Almanacco", "https://example.org/a")]),
            Ok(vec![result("Almanacco", "https://example.org/b")]),
        )
        .unwrap();
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn duplicate_feed_is_shown_once() {
        let merged = merge_catalog_results(
            Ok(vec![result("Almanacco", "https://example.org/a")]),
            Ok(vec![result("Almanacco renamed", "https://example.org/a")]),
        )
        .unwrap();
        assert_eq!(merged.len(), 1);
    }

    #[test]
    fn title_keys_handle_spaces_punctuation_and_case() {
        assert_eq!(podcast_title_key("  Almanacco  "), "almanacco");
        assert_eq!(
            podcast_title_key("L'Almanacco del GIORNO"),
            "l-almanacco-del-giorno"
        );
        assert!(podcast_title_key("?! ").is_empty());
    }

    #[test]
    fn spreaker_missing_or_null_author_is_supported() {
        for json in [
            r#"{"show_id":1,"title":"A"}"#,
            r#"{"show_id":1,"title":"A","author_name":null}"#,
        ] {
            let item = spreaker_result(serde_json::from_str(json).unwrap());
            assert!(item.artist.is_empty());
        }
        let item = spreaker_result(
            serde_json::from_str(
                r#"{
            "show_id":5587296,"title":"Almanacco","author_name":null,
            "author":{"fullname":"Radio Taormina"}}
        "#,
            )
            .unwrap(),
        );
        assert_eq!(item.artist, "Radio Taormina");
        assert_eq!(
            item.feed_url,
            "https://www.spreaker.com/show/5587296/episodes/feed"
        );
    }

    #[test]
    fn exact_lookup_rejects_a_renamed_or_unrelated_show() {
        let item = serde_json::from_str(r#"{"show_id":1,"title":"Another show"}"#).unwrap();
        assert!(exact_spreaker_result(item, "almanacco").is_empty());
        let item = serde_json::from_str(r#"{"show_id":1,"title":"ALMANACCO"}"#).unwrap();
        assert_eq!(exact_spreaker_result(item, "almanacco").len(), 1);
    }

    #[tokio::test]
    #[ignore = "requires live Apple and Spreaker services"]
    async fn live_almanacco_search_and_feed() {
        let results = search_podcasts("Almanacco").await.unwrap();
        let found = results
            .iter()
            .find(|r| r.feed_url == "https://www.spreaker.com/show/5587296/episodes/feed")
            .expect("Almanacco by Radio Taormina must be discoverable");
        assert_eq!(found.title, "Almanacco");
        assert_eq!(found.artist, "Radio Taormina");
        assert_eq!(results[0].title, "Almanacco");
        let feed = fetch_source(&PodcastSource {
            title: found.title.clone(),
            url: found.feed_url.clone(),
            episodes: Vec::new(),
        })
        .await
        .unwrap();
        assert!(!feed.episodes.is_empty());
        assert!(
            feed.episodes
                .iter()
                .any(|e| e.audio_url.starts_with("https://"))
        );
        println!(
            "Live: {} search results; {} episodes from {}",
            results.len(),
            feed.episodes.len(),
            found.title
        );
    }
}

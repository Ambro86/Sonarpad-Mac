use base64::Engine;
use quick_xml::Reader;
use quick_xml::events::Event;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

const RAIPLAY_BASE_URL_B64: &str = "IlQdF0ccDBsQTw8HExhZTxsOBBMeAw==";
const RAIPLAY_MENU_URL_B64: &str = "IlQdF0ccDBsQTw8HExhZTxsOBBMeA0wXGRwRSxATGxAHURoO";
const RAIPLAY_SEARCH_URL_B64: &str = "IlQdF0ccDBsQTw8HExhZTxsOBBMeA0wJEhYPDBwMFhZLFhMRHAEBARsYAA==";
const RAIPLAY_MENU_SECTION_SOURCE_PREFIX: &str = "raiplay-menu-section:";

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BrowseItemKind { Page, Media }

#[derive(Clone, Debug)]
pub struct BrowseItem {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub program_title: Option<String>,
    pub path_id: Option<String>,
    pub media_url: Option<String>,
    pub kind: BrowseItemKind,
}

#[derive(Clone, Debug)]
pub struct BrowsePage { pub title: String, pub items: Vec<BrowseItem> }

#[derive(Clone, Debug)]
pub struct PlaybackTarget { media_url: String, playback_url: String, audio_description_url: Option<String> }
impl PlaybackTarget {
    pub fn media_url(&self) -> &str { &self.media_url }
    pub fn playback_url(&self) -> &str { &self.playback_url }
    pub fn audio_description_url(&self) -> Option<&str> { self.audio_description_url.as_deref() }
}

pub fn load_root_page() -> Result<BrowsePage, String> { load_page(&raiplay_menu_url()?) }
pub fn load_page(source: &str) -> Result<BrowsePage, String> {
    if let Some(section_name) = source.strip_prefix(RAIPLAY_MENU_SECTION_SOURCE_PREFIX) { return load_menu_section_page(section_name); }
    load_page_from_url(source)
}
pub fn search(query: &str) -> Result<BrowsePage, String> {
    let query = query.split_whitespace().collect::<Vec<_>>().join(" ");
    let url = format!("{}?q={}", raiplay_search_url()?, percent_encode_query(&query));
    let mut page = load_page_from_url(&url)?;
    page.title = format!("RaiPlay - {query}");
    Ok(page)
}

pub fn resolve_playback_target(relinker_url: &str) -> Result<PlaybackTarget, String> {
    let (content_url, _is_live) = resolve_relinker_content_url(relinker_url)?;
    if is_drm_protected_raiplay_url(&content_url) { return Err("Questo contenuto RaiPlay usa DRM e non è supportato.".to_string()); }
    let audio_description_url = if is_hls_url(&content_url) { resolve_hls_audio_only_url(&content_url) } else { None };
    let playback_url = audio_description_url.clone().unwrap_or_else(|| content_url.clone());
    Ok(PlaybackTarget { media_url: content_url, playback_url, audio_description_url })
}

fn load_menu_section_page(section_name: &str) -> Result<BrowsePage, String> {
    let root = fetch_json(&raiplay_menu_url()?)?;
    let sections = root.get("menu").and_then(Value::as_array).or_else(|| root.get("items").and_then(Value::as_array)).ok_or_else(|| "Menu RaiPlay non disponibile.".to_string())?;
    let section = sections.iter().find(|section| string_field(section, "name").map(|v| v.eq_ignore_ascii_case(section_name)).unwrap_or(false)).ok_or_else(|| "Sezione RaiPlay non disponibile.".to_string())?;
    let title = string_field(section, "title").or_else(|| string_field(section, "name")).unwrap_or_else(|| "RaiPlay".to_string());
    let elements = section.get("elements").and_then(Value::as_array).ok_or_else(|| "Sezione RaiPlay senza elementi.".to_string())?;
    let mut items = Vec::new(); let mut seen = HashSet::new(); collect_cards(elements, &mut seen, &mut items)?;
    Ok(BrowsePage { title, items })
}
fn load_page_from_url(url: &str) -> Result<BrowsePage, String> {
    let root = fetch_json(url)?; let mut items=Vec::new(); let mut seen=HashSet::new(); collect_nested_items(&root,&mut seen,&mut items)?; if items.is_empty() && let Some(item)=parse_card(&root)? { items.push(item); }
    Ok(BrowsePage { title: page_title(&root), items })
}
fn collect_nested_items(value:&Value,seen:&mut HashSet<String>,items:&mut Vec<BrowseItem>)->Result<(),String>{match value{Value::Array(array)=>for entry in array{collect_entry(entry,seen,items)?},Value::Object(map)=>for key in ["items","contents","blocks","sets","elements"]{if let Some(array)=map.get(key).and_then(Value::as_array){for entry in array{collect_entry(entry,seen,items)?}}},_=>{}} Ok(())}
fn collect_entry(entry:&Value,seen:&mut HashSet<String>,items:&mut Vec<BrowseItem>)->Result<(),String>{if let Some(item)=parse_card(entry)? && seen.insert(item.id.clone()){items.push(item);} collect_nested_items(entry,seen,items)}
fn collect_cards(cards:&[Value],seen:&mut HashSet<String>,items:&mut Vec<BrowseItem>)->Result<(),String>{for card in cards{if let Some(item)=parse_card(card)? && seen.insert(item.id.clone()){items.push(item);}} Ok(())}
fn parse_card(card:&Value)->Result<Option<BrowseItem>,String>{
    if card.get("action").is_some(){return Ok(None);} if card.get("type").and_then(Value::as_str).map(|v|matches!(v,"label"|"placeholder")).unwrap_or(false){return Ok(None);} if string_field(card,"menu_type").map(|v|v.eq_ignore_ascii_case("RaiPlay Separatore Nav")).unwrap_or(false){return Ok(None);}
    let media_url=card.get("video").and_then(|video|string_field(video,"content_url")).or_else(||string_field(card,"video_url"));
    let path_id=string_field(card,"path_id").filter(|v|is_supported_internal_target(v)).or_else(||string_field(card,"url").and_then(|v|html_url_to_json_path(&v)));
    let kind=if media_url.is_some(){BrowseItemKind::Media}else if path_id.is_some(){BrowseItemKind::Page}else{return Ok(None)};
    let title=preferred_title(card)?; let description=preferred_description(card); let program_title=preferred_program_title(card);
    let id=match kind{BrowseItemKind::Media=>format!("media|{}|{}",media_url.clone().unwrap_or_default(),path_id.clone().unwrap_or_default()),BrowseItemKind::Page=>format!("page|{}",path_id.clone().unwrap_or_default())};
    Ok(Some(BrowseItem{id,title,description,program_title,path_id:path_id.map(|v|absolute_url(&v)).transpose()?,media_url,kind}))
}
fn preferred_title(card:&Value)->Result<String,String>{for key in ["titolo","episode_title","toptitle","title","name","label","programma","program_name"]{if let Some(v)=string_field(card,key).filter(|v|!v.is_empty()){return Ok(v)}} Err("Elemento RaiPlay senza titolo.".to_string())}
fn preferred_description(card:&Value)->Option<String>{for key in ["sommario","description","vanity","caption","subtitle","duration_in_minutes","menu_type"]{if let Some(v)=string_field(card,key).filter(|v|!v.is_empty()){return Some(v)}} None}
fn preferred_program_title(card:&Value)->Option<String>{for key in ["program_name","programma"]{if let Some(v)=string_field(card,key).filter(|v|!v.is_empty()){return Some(v)}} None}
fn page_title(root:&Value)->String{for key in ["name","title","label"]{if let Some(v)=string_field(root,key).filter(|v|!v.is_empty()){return v}} "RaiPlay".to_string()}
fn fetch_json(url:&str)->Result<Value,String>{let bytes=crate::curl_client::CurlClient::fetch_url_impersonated(url).map_err(|err|format!("Impossibile caricare RaiPlay: {err}"))?; serde_json::from_slice(&bytes).map_err(|err|format!("Risposta RaiPlay non valida: {err}"))}
fn string_field(value:&Value,key:&str)->Option<String>{value.get(key).and_then(Value::as_str).map(str::trim).filter(|v|!v.is_empty()).map(ToOwned::to_owned)}
fn is_supported_internal_target(path_or_url:&str)->bool{let t=path_or_url.trim(); if t.is_empty(){return false} if t.starts_with("http://")||t.starts_with("https://"){return t.contains("raiplay.it")&&(t.ends_with(".json")||t.contains(".json?"));} t.starts_with('/')&&t.ends_with(".json")}
fn html_url_to_json_path(path_or_url:&str)->Option<String>{let t=path_or_url.trim(); if t.is_empty(){return None} if t.ends_with(".json"){return Some(t.to_string())} if let Some(prefix)=t.strip_suffix(".html"){return Some(format!("{prefix}.json"))} if t.starts_with('/') {return Some(format!("{t}.json"))} if t.starts_with("http://")||t.starts_with("https://"){let replaced=t.replace(".html",".json"); if replaced!=t{return Some(replaced)} return Some(format!("{t}.json"))} None}
fn absolute_url(path_or_url:&str)->Result<String,String>{let t=path_or_url.trim(); if t.starts_with("http://")||t.starts_with("https://"){Ok(t.to_string())}else{Ok(format!("{}{}",raiplay_base_url()?,t))}}
fn resolve_relinker_content_url(relinker_url:&str)->Result<(String,bool),String>{let sep=if relinker_url.contains('?'){'&'}else{'?'}; let xml_url=format!("{relinker_url}{sep}output=45&pl=native"); let bytes=crate::curl_client::CurlClient::fetch_url_iphone_impersonated(&xml_url).map_err(|err|format!("Impossibile risolvere RaiPlay: {err}"))?; let xml=String::from_utf8(bytes).map_err(|err|format!("XML RaiPlay non UTF-8: {err}"))?; parse_relinker_content_url(&xml)}
fn parse_relinker_content_url(xml:&str)->Result<(String,bool),String>{if let Some(content_url)=extract_xml_tag_with_attribute(xml,"url","type","content"){let is_live=extract_xml_tag_text(xml,"is_live").map(|v|v.eq_ignore_ascii_case("Y")).unwrap_or(false);return Ok((content_url,is_live));} let mut reader=Reader::from_str(xml); let mut content_url=None; let mut is_live=false; loop{match reader.read_event(){Ok(Event::Start(event))=>{let tag=event.name().as_ref().to_vec(); if tag.as_slice()==b"is_live"{if let Ok(Event::Text(text))=reader.read_event(){is_live=text.decode().map(|v|v.trim().eq_ignore_ascii_case("Y")).unwrap_or(false);}}else if tag.as_slice()==b"url"{let mut is_content=false; for attr in event.attributes().flatten(){if attr.key.as_ref()==b"type"&&attr.decode_and_unescape_value(reader.decoder()).map(|v|v=="content").unwrap_or(false){is_content=true;break;}} if is_content&&let Ok(Event::Text(text))=reader.read_event(){let decoded=text.decode().map_err(|err|format!("URL RaiPlay non decodificabile: {err}"))?.into_owned(); if !decoded.trim().is_empty(){content_url=Some(decoded);}}}},Ok(Event::Eof)=>break,Ok(_)=>{},Err(err)=>return Err(format!("XML RaiPlay non valido: {err}"))}} let url=content_url.ok_or_else(||"URL contenuto RaiPlay non disponibile.".to_string())?; Ok((url,is_live))}
fn extract_xml_tag_text(xml:&str,tag:&str)->Option<String>{let start_tag=format!("<{tag}>"); let end_tag=format!("</{tag}>"); let start=xml.find(&start_tag)?+start_tag.len(); let end=xml[start..].find(&end_tag)?+start; let v=xml[start..end].trim(); if v.is_empty(){None}else{Some(v.to_string())}}
fn extract_xml_tag_with_attribute(xml:&str,tag_name:&str,attribute_name:&str,attribute_value:&str)->Option<String>{let start_tag_prefix=format!("<{tag_name}"); let end_tag=format!("</{tag_name}>"); let mut offset=0; while let Some(rel)=xml[offset..].find(&start_tag_prefix){let start=offset+rel; let tag_end=xml[start..].find('>')?+start; let tag_content=&xml[start..=tag_end]; let expected=format!(r#"{attribute_name}=\"{attribute_value}\""#); if tag_content.contains(&expected){let value_start=tag_end+1; let value_end=xml[value_start..].find(&end_tag)?+value_start; let v=xml[value_start..value_end].trim(); if !v.is_empty(){return Some(v.to_string())}} offset=tag_end.saturating_add(1);} None}
fn is_hls_url(url:&str)->bool{url.trim().to_ascii_lowercase().contains(".m3u8")}
fn is_drm_protected_raiplay_url(url:&str)->bool{let lower=url.trim().to_ascii_lowercase(); lower.contains("/drm_root/")||lower.contains("drmnagra")||lower.contains(".mpd")||lower.contains("manifest_mvnumber.mpd")}
fn resolve_hls_audio_only_url(master_url:&str)->Option<String>{resolve_hls_audio_track_urls(master_url).into_iter().find(|(attrs,_)|attrs.get("LANGUAGE").map(|v|v.eq_ignore_ascii_case("des")).unwrap_or(false)||attrs.get("NAME").map(|v|v.eq_ignore_ascii_case("Audiodescrizione")).unwrap_or(false)).or_else(||resolve_hls_audio_track_urls(master_url).into_iter().find(|(attrs,_)|attrs.get("LANGUAGE").map(|v|v.eq_ignore_ascii_case("ita")).unwrap_or(false))).map(|(_,url)|url)}
fn resolve_hls_audio_track_urls(master_url:&str)->Vec<(HashMap<String,String>,String)>{let Ok(bytes)=crate::curl_client::CurlClient::fetch_url_impersonated(master_url)else{return Vec::new()}; let Ok(playlist)=String::from_utf8(bytes)else{return Vec::new()}; let mut tracks=Vec::new(); for line in playlist.lines(){let t=line.trim(); if !t.starts_with("#EXT-X-MEDIA:")||!t.contains("TYPE=AUDIO"){continue} if let Some(uri)=parse_hls_attribute(t,"URI"){let mut attrs=HashMap::new(); for key in ["LANGUAGE","NAME","DEFAULT"]{if let Some(value)=parse_hls_attribute(t,key){attrs.insert(key.to_string(),value);}} tracks.push((attrs,resolve_hls_child_url(master_url,&uri)));}} tracks}
fn parse_hls_attribute(line:&str,key:&str)->Option<String>{let pattern=format!("{key}=\""); let start=line.find(&pattern)?+pattern.len(); let rest=&line[start..]; let end=rest.find('"')?; Some(rest[..end].to_string())}
fn resolve_hls_child_url(master_url:&str,child_uri:&str)->String{let t=child_uri.trim(); if t.starts_with("http://")||t.starts_with("https://"){return t.to_string()} let (base,query)=master_url.split_once('?').map(|(b,q)|(b,format!("?{q}"))).unwrap_or((master_url,String::new())); let mut parts=base.rsplitn(2,'/'); let _=parts.next(); let parent=parts.next().unwrap_or(base); if t.contains('?'){format!("{parent}/{t}")}else{format!("{parent}/{t}{query}")}}
fn raiplay_base_url()->Result<String,String>{decode_raiplay_url(RAIPLAY_BASE_URL_B64)} fn raiplay_menu_url()->Result<String,String>{decode_raiplay_url(RAIPLAY_MENU_URL_B64)} fn raiplay_search_url()->Result<String,String>{decode_raiplay_url(RAIPLAY_SEARCH_URL_B64)}
fn decode_raiplay_url(encoded:&str)->Result<String,String>{let key=resolve_raiplay_secret_key()?.into_bytes(); let bytes=base64::engine::general_purpose::STANDARD.decode(encoded).map_err(|err|format!("URL RaiPlay offuscato non valido: {err}"))?; let decoded:Vec<u8>=bytes.into_iter().enumerate().map(|(i,b)|b^key[i%key.len()]).collect(); String::from_utf8(decoded).map_err(|err|format!("URL RaiPlay decodificato non valido: {err}"))}
fn resolve_raiplay_secret_key()->Result<String,String>{crate::load_saved_rai_luce_code().ok_or_else(||"Chiave Luce mancante: inserisci il codice nelle impostazioni.".to_string())}
fn percent_encode_query(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => encoded.push(byte as char),
            b' ' => encoded.push_str("%20"),
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

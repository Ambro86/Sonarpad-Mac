//! Search categories shared by the search page and its continuations.
use serde_json::Value;
pub fn params(kind: u8) -> &'static str {
    match kind { 1 => "EgIQAQ==", 2 | 4 | 5 => "EgIQAw==", 3 => "EgIQAg==", _ => "" }
}

/// yt-dlp results have URLs and extractor metadata instead of renderer types.
/// Reuse the caller's collection classification, distinguishing channels from
/// playlists before allowing a result through a specific search category.
pub fn matches_fallback(kind: u8, url: &str, is_collection: bool) -> bool {
    if kind == 0 {
        return true;
    }
    let Ok(url) = url::Url::parse(url) else {
        return false;
    };
    if !matches!(url.host_str(), Some("youtube.com" | "www.youtube.com" | "m.youtube.com" | "youtu.be")) {
        return false;
    }
    let path = url.path().trim_end_matches('/');
    let channel = path.starts_with("/channel/")
        || path.starts_with("/user/")
        || path.starts_with("/c/")
        || path.starts_with("/@");
    match kind {
        1 => !is_collection && !channel,
        2 => is_collection && !channel,
        3 => channel,
        _ => false, // Courses/podcasts require metadata from the direct catalog.
    }
}

pub fn filter(value: &mut Value, kind: u8) {
    if kind == 0 { return; }
    match value {
        Value::Object(map) => {
            map.retain(|key, renderer| {
                let content = match key.as_str() {
                    "videoRenderer" | "playlistVideoRenderer" => "video",
                    "channelRenderer" => "channel",
                    "playlistRenderer" | "radioRenderer" => "playlist",
                    "lockupViewModel" => match renderer["contentType"].as_str().unwrap_or("") {
                        "LOCKUP_CONTENT_TYPE_VIDEO" => "video",
                        "LOCKUP_CONTENT_TYPE_PLAYLIST" => "playlist",
                        "LOCKUP_CONTENT_TYPE_COURSE" => "course",
                        "LOCKUP_CONTENT_TYPE_PODCAST" => "podcast",
                        _ => return true,
                    },
                    _ => return true,
                };
                match kind { 1 => content == "video", 2 => matches!(content, "playlist" | "course" | "podcast"),
                    3 => content == "channel", 4 => content == "course", 5 => content == "podcast", _ => true }
            });
            for child in map.values_mut() { filter(child, kind); }
        }
        Value::Array(items) => for item in items { filter(item, kind); },
        _ => {}
    }
}

#[cfg(test)]
mod tests {
 use super::*;
 #[test]
 fn fallback_keeps_only_the_requested_type_in_provider_order() {
  let rows = [
   ("https://www.youtube.com/watch?v=abcdefghijk", false),
   ("https://www.youtube.com/playlist?list=PLfirst", true),
   ("https://www.youtube.com/channel/UCchannel", true),
   ("https://www.youtube.com/playlist?list=PLsecond", true),
   ("https://www.youtube.com/@channel", true),
   ("https://www.youtube.com/shorts/123456789ab", false),
  ];
  for (kind, indices) in [(0, vec![0,1,2,3,4,5]), (1, vec![0,5]),
                         (2, vec![1,3]), (3, vec![2,4]), (4, vec![]), (5, vec![])] {
   let kept: Vec<usize> = rows.iter().enumerate()
       .filter(|(_, (url, collection))| matches_fallback(kind, url, *collection))
       .map(|(index, _)| index).collect();
   assert_eq!(kept, indices, "category {kind}");
  }
 }
 #[test]
 fn fallback_distinguishes_mix_playlists_from_videos_with_a_list_parameter() {
  let mix = "https://www.youtube.com/watch?v=abcdefghijk&list=RDabcdefghijk";
  assert!(matches_fallback(2, mix, true));
  assert!(!matches_fallback(1, mix, true));
  assert!(!matches_fallback(2, mix, false));
  assert!(matches_fallback(1, mix, false));
  for url in ["https://www.youtube.com/user/channel", "https://www.youtube.com/c/channel"] {
   assert!(!matches_fallback(2, url, true));
   assert!(matches_fallback(3, url, true));
  }
  assert!(!matches_fallback(2, "https://example.org/playlist?list=PLfake", true));
  assert!(matches_fallback(0, "https://example.org/playlist?list=PLfake", true));
 }
 #[test] fn mixed_results_and_continuation() {
  let original=serde_json::json!({"contents":[{"videoRenderer":{"videoId":"123"}},{"channelRenderer":{"channelId":"UC123"}},{"playlistRenderer":{"playlistId":"PL123"}},{"lockupViewModel":{"contentType":"LOCKUP_CONTENT_TYPE_COURSE"}},{"lockupViewModel":{"contentType":"LOCKUP_CONTENT_TYPE_PODCAST"}}],"continuationItemRenderer":{"token":"next"}});
  for (kind,count) in [(0,5),(1,1),(2,3),(3,1),(4,1),(5,1)] {
   let mut v=original.clone();filter(&mut v,kind);
   assert_eq!(v["contents"].as_array().unwrap().iter().filter(|v|!v.as_object().unwrap().is_empty()).count(),count);
   assert_eq!(v["continuationItemRenderer"]["token"],"next");
  }
 }
}

//! Search categories shared by the search page and its continuations.
use serde_json::Value;
pub fn params(kind: u8) -> &'static str {
    match kind { 1 => "EgIQAQ==", 2 | 4 | 5 => "EgIQAw==", 3 => "EgIQAg==", _ => "" }
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
 #[test] fn mixed_results_and_continuation() {
  let original=serde_json::json!({"contents":[{"videoRenderer":{"videoId":"123"}},{"channelRenderer":{"channelId":"UC123"}},{"playlistRenderer":{"playlistId":"PL123"}},{"lockupViewModel":{"contentType":"LOCKUP_CONTENT_TYPE_COURSE"}},{"lockupViewModel":{"contentType":"LOCKUP_CONTENT_TYPE_PODCAST"}}],"continuationItemRenderer":{"token":"next"}});
  for (kind,count) in [(0,5),(1,1),(2,3),(3,1),(4,1),(5,1)] {
   let mut v=original.clone();filter(&mut v,kind);
   assert_eq!(v["contents"].as_array().unwrap().iter().filter(|v|!v.as_object().unwrap().is_empty()).count(),count);
   assert_eq!(v["continuationItemRenderer"]["token"],"next");
  }
 }
}

use reqwest::blocking::Client;
use std::time::Duration;

const KEY: &[u8] = b"mK9!vP2xL8#qT4zN7@rW1sY6dF0hJ3uBzUrL1BirM@|\\";

fn decode(encoded: &str) -> String {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    let bytes = STANDARD.decode(encoded).unwrap_or_default();
    let mut decoded = Vec::new();
    for (i, byte) in bytes.iter().enumerate() {
        decoded.push(byte ^ KEY[i % KEY.len()]);
    }
    String::from_utf8(decoded).unwrap_or_default()
}

fn main() {
    let base_url = decode("BT9NUUx/HRUjWkodMRoTOlYsGzZeHTVfCiMeAT4c"); // http://mobile.italiaonline.it/
    let client_id = decode("HSlUThQ5Xh0="); // pbmobile
    let version = decode("XmUAD0M="); // 3.9.5
    let endpoint = decode("Hi5YUxU4Qh8="); // searchpg
    
    let what = "bar";
    let where_loc = "palermo";
    
    let mut url = format!("{base_url}{endpoint}?client={client_id}&version={version}");
    let what_enc = url::form_urlencoded::byte_serialize(what.as_bytes()).collect::<String>();
    url.push_str(&format!("&what={what_enc}"));
    if !where_loc.trim().is_empty() {
        let where_enc = url::form_urlencoded::byte_serialize(where_loc.as_bytes()).collect::<String>();
        url.push_str(&format!("&where={where_enc}"));
    }

    println!("URL: {}", url);

    let client = Client::builder().timeout(Duration::from_secs(15)).build().unwrap();
    let resp = client.get(&url).header("User-Agent", "Mozilla/5.0").send().unwrap();
    println!("Status: {}", resp.status());
    println!("Text: {}", resp.text().unwrap());
}

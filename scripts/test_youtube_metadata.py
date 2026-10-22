"""Compile and test the actual Mac YouTube parser on any host (no GUI build)."""
from pathlib import Path
import subprocess
ROOT = Path(__file__).resolve().parents[1]
source = (ROOT / 'src/main.rs').read_text(encoding='utf8')
work = ROOT / 'target/youtube-metadata-tests'
(work / 'src').mkdir(parents=True, exist_ok=True)
(work / 'Cargo.toml').write_text('[package]\nname="youtube-metadata-tests"\nversion="0.0.0"\nedition="2024"\n[workspace]\n[dependencies]\nserde_json="1"\n', encoding='utf8')
code = '#![allow(dead_code)]\nuse std::collections::HashSet;\n#[derive(Debug)] struct YoutubeSearchResult {title:String,url:String,is_collection:bool,duration:Option<String>}\n'
code += source[source.index('fn youtube_text_value('):source.index('fn is_youtube_url(')]
code += source[source.index('fn youtube_lockup_views('):source.index('fn youtube_find_continuation_token(')]
fixture = (ROOT/'src/test_fixtures/sonartube_channel_hype.json').as_posix()
code += '''
#[cfg(test)] mod tests {
use super::*;
fn fixture()->serde_json::Value {serde_json::from_str(include_str!("FIXTURE")).unwrap()}
#[test] fn channel_hype_members_and_collaborations() {
 let mut results=Vec::new();
 youtube_collect_direct_results(&fixture(),false,true,&mut results,&mut HashSet::new());
 assert_eq!(results.len(),3);
 assert!(results[0].url.ends_with("1o9lnx4-VrQ"));
 assert!(results[0].title.contains("PARTE 6"));
 assert!(results[1].url.ends_with("t0x0gxyqnFI"));
 assert!(results[2].url.ends_with("t4PepnB0X5s"));
}
#[test] fn deduplicates_across_pages() {
 let mut results=Vec::new(); let mut seen=HashSet::new();
 for _ in 0..2 {youtube_collect_direct_results(&fixture(),false,true,&mut results,&mut seen);}
 assert_eq!(results.len(),3);
}
#[test] fn videos_disabled() {
 let mut results=Vec::new();
 youtube_collect_direct_results(&fixture(),true,false,&mut results,&mut HashSet::new());
 assert!(results.is_empty());
}
#[test] fn legacy_video_still_works() {
 let mut results=Vec::new();
 let data=serde_json::json!({"videoRenderer":{"videoId":"legacy00123","title":{"simpleText":"Legacy"},"lengthText":{"simpleText":"12:34"}}});
 youtube_collect_direct_results(&data,false,true,&mut results,&mut HashSet::new());
 assert_eq!(results[0].title,"Legacy"); assert_eq!(results[0].duration.as_deref(),Some("12:34"));
}
}
'''.replace('FIXTURE', fixture)
(work/'src/lib.rs').write_text(code,encoding='utf8')
subprocess.run(['cargo','test','--offline','--manifest-path',str(work/'Cargo.toml')],check=True)

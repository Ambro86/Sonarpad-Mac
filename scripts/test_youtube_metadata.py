"""Compile and test the actual Mac YouTube parser on any host (no GUI build)."""
from pathlib import Path
import subprocess
ROOT = Path(__file__).resolve().parents[1]
source = (ROOT / 'src/main.rs').read_text(encoding='utf8')
work = ROOT / 'target/youtube-metadata-tests'
(work / 'src').mkdir(parents=True, exist_ok=True)
(work / 'Cargo.toml').write_text('[package]\nname="youtube-metadata-tests"\nversion="0.0.0"\nedition="2024"\n[workspace]\n[dependencies]\nserde_json="1"\nurl="2"\n', encoding='utf8')
code = '#![allow(dead_code)]\nuse std::collections::HashSet;\n#[derive(Debug)] struct YoutubeSearchResult {title:String,url:String,is_collection:bool,duration:Option<String>}\n'
code += '#[path="' + (ROOT/'src/youtube_search_types.rs').as_posix() + '"] mod youtube_search_types;\n'
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
#[test] fn mix_keeps_its_actual_seed_instead_of_a_playlist_page() {
 let data=serde_json::json!({"lockupViewModel":{
  "contentId":"RDEMDHx1mzcs_wPqWOntgHDScQ", "contentType":"LOCKUP_CONTENT_TYPE_PLAYLIST",
  "metadata":{"lockupMetadataViewModel":{"title":{"content":"Mix - Eros Ramazzotti"}}},
  "rendererContext":{"commandContext":{"onTap":{"innertubeCommand":{
   "commandMetadata":{"webCommandMetadata":{"url":"/watch?v=UojBaKX5Vz4&list=RDEMDHx1mzcs_wPqWOntgHDScQ&start_radio=1"}}
  }}}}
 }});
 let mut results=Vec::new();
 youtube_collect_direct_results(&data,true,true,&mut results,&mut HashSet::new());
 assert_eq!(results.len(),1);
 assert!(results[0].is_collection);
 assert_eq!(results[0].url,"https://www.youtube.com/watch?v=UojBaKX5Vz4&list=RDEMDHx1mzcs_wPqWOntgHDScQ&start_radio=1");
}
#[test] fn ordinary_playlist_without_endpoint_keeps_playlist_url() {
 let data=serde_json::json!({"lockupViewModel":{
  "contentId":"PLnormal", "contentType":"LOCKUP_CONTENT_TYPE_PLAYLIST",
  "metadata":{"lockupMetadataViewModel":{"title":{"content":"Playlist"}}}
 }});
 let mut results=Vec::new();
 youtube_collect_direct_results(&data,true,true,&mut results,&mut HashSet::new());
 assert_eq!(results.len(),1);
 assert_eq!(results[0].url,"https://www.youtube.com/playlist?list=PLnormal");
}
#[test] fn inline_mix_endpoint_is_preserved() {
 let renderer=serde_json::json!({"itemPlayback":{"inlinePlayerData":{"onSelect":{"innertubeCommand":{
  "commandMetadata":{"webCommandMetadata":{"url":"/watch?v=UojBaKX5Vz4&list=RDtest"}}
 }}}}});
 assert_eq!(youtube_renderer_url(&renderer,None).as_deref(),Some("https://www.youtube.com/watch?v=UojBaKX5Vz4&list=RDtest"));
}
#[test] fn legacy_video_still_works() {
 let mut results=Vec::new();
 let data=serde_json::json!({"videoRenderer":{"videoId":"legacy00123","title":{"simpleText":"Legacy"},"lengthText":{"simpleText":"12:34"}}});
 youtube_collect_direct_results(&data,false,true,&mut results,&mut HashSet::new());
 assert_eq!(results[0].title,"Legacy"); assert_eq!(results[0].duration.as_deref(),Some("12:34"));
}
}
'''.replace('FIXTURE', fixture)
# Optional network check: feed current YouTube responses into the actual Mac parser.
import sys
if '--live' in sys.argv:
    import json, urllib.request, urllib.parse
    for kind, query, param in [(0,'Pink Floyd',''),(1,'Pink Floyd','EgIQAQ=='),(2,'Pink Floyd','EgIQAw=='),(3,'Pink Floyd','EgIQAg=='),(4,'python course','EgIQAw=='),(5,'podcast italiano','EgIQAw==')]:
        url='https://www.youtube.com/results?'+urllib.parse.urlencode({'search_query':query,'sp':param,'hl':'it','gl':'IT'})
        request=urllib.request.Request(url,headers={'User-Agent':'Mozilla/5.0','Cookie':'SOCS=CAI'})
        html=urllib.request.urlopen(request,timeout=30).read().decode('utf8')
        marker='var ytInitialData = '
        value=json.JSONDecoder().raw_decode(html.split(marker,1)[1])[0]
        fixture=work/f'live-{kind}.json'
        fixture.write_text(json.dumps(value),encoding='utf8')
        code += f'''
#[test] fn live_category_{kind}() {{
          let mut data:serde_json::Value=serde_json::from_str(include_str!("{fixture.as_posix()}")).unwrap();
          youtube_search_types::filter(&mut data,{kind});
          let mut results=Vec::new();
          youtube_collect_direct_results(&data,true,true,&mut results,&mut HashSet::new());
          println!("Mac category {kind}: {{}} results",results.len());
          assert!(!results.is_empty());
          for r in results {{ match {kind} {{
           1 => assert!(!r.is_collection),
           2|4|5 => assert!(r.url.contains("list=")),
           3 => assert!(r.is_collection && (r.url.contains("/channel/") || r.url.contains("/@"))),
           _ => {{}}
          }} }}
        }}
'''
(work/'src/lib.rs').write_text(code,encoding='utf8')
subprocess.run(['cargo','test','--offline','--manifest-path',str(work/'Cargo.toml'),'--','--nocapture'],check=True)

"""Run real searches and continuations through the production Mac Rust functions."""
from pathlib import Path
import subprocess
ROOT = Path(__file__).resolve().parents[1]
source = (ROOT / 'src/main.rs').read_text(encoding='utf8')
work = ROOT / 'target/youtube-pagination-tests'
(work / 'src').mkdir(parents=True, exist_ok=True)
(work / 'Cargo.toml').write_text('''[package]
name="youtube-pagination-tests"
version="0.0.0"
edition="2024"
[workspace]
[dependencies]
serde_json="1"
url="2"
# Use rustls on this host: Windows Schannel fails the YouTube TLS handshake.
reqwest={version="0.12",default-features=false,features=["blocking","json","rustls-tls"]}
''', encoding='utf8')
def item(marker):
    start=source.index(marker)
    end=source.index('\n}',start)+2
    return source[start:end]+'\n'
code='''#![allow(dead_code)]
use std::collections::HashSet;
use std::time::Duration;
use url::Url;
#[derive(Debug)] struct YoutubeSearchResult {title:String,url:String,is_collection:bool,duration:Option<String>}
type YoutubeResultsPayload=(Vec<YoutubeSearchResult>,Option<YoutubeSearchContext>);
fn append_podcast_log(_: &str) {}
'''
code += '#[path="'+(ROOT/'src/youtube_search_types.rs').as_posix()+'"] mod youtube_search_types;\n'
for name in ['YoutubeSearchContext','YoutubeContinuation']:
    code += '#[derive(Clone)]\n'+item('struct '+name+' {')
code += '#[derive(Clone,Copy)]\n'+item('enum YoutubeContinuationEndpoint {')
code += item("struct YoutubeParseArgs<'a>")
for name in ['YOUTUBE_SEARCH_LIMIT','YOUTUBE_DIRECT_CLIENT_VERSION']:
    start=source.index('const '+name+':')
    code+=source[start:source.index('\n',start)]+'\n'
for name in ['percent_encode','youtube_result_dedup_key','youtube_seen_urls','youtube_filter_seen_results','youtube_direct_client','youtube_direct_fetch_page','find_json_object_after_marker','youtube_extract_initial_data','youtube_extract_quoted_config','youtube_text_value','youtube_renderer_url','youtube_absolute_url','youtube_lockup_views','youtube_collect_direct_results','youtube_find_continuation_token','youtube_direct_context','youtube_parse_direct_page','youtube_direct_search','youtube_direct_next_page']:
    code += item('fn '+name+'(')
code += '''
#[test]
fn continuation_ignores_search_filter_chips() {
 let data=serde_json::json!({"header":{"chipCloudChipRenderer":{"navigationEndpoint":{"continuationCommand":{"token":"wrong-filter"}}}},"onResponseReceivedCommands":[{"appendContinuationItemsAction":{"continuationItems":[{"continuationItemRenderer":{"continuationEndpoint":{"continuationCommand":{"token":"next-page"}}}}]}}]});
 assert_eq!(youtube_find_continuation_token(&data).as_deref(),Some("next-page"));
 let chips=serde_json::json!({"navigationEndpoint":{"continuationCommand":{"token":"wrong-filter"}}});
 assert!(youtube_find_continuation_token(&chips).is_none());
}
#[test]
fn live_search_pagination_all_categories() {
 for (kind,query) in [(0,"Pink Floyd"),(1,"Pink Floyd"),(2,"Pink Floyd"),(3,"Pink Floyd"),(4,"python course"),(5,"podcast italiano")] {
  let (first,context)=youtube_direct_search(query,kind).expect("first page");
  assert!(!first.is_empty(),"empty first page for {kind}");
  let mut context=context.expect("first continuation");
  let saved_context=context.clone();
  let mut seen=youtube_seen_urls(&first);
  println!("Mac category {kind}, page 1: {} results",first.len());
  for page in 1..=2 {
   assert_eq!(context.search_type,kind);
   let (results,next)=youtube_direct_next_page(&context).expect("next page");
   println!("Mac category {kind}, page {}: {} results",page+1,results.len());
   assert!(!results.is_empty());
   for result in &results {
    assert!(seen.insert(youtube_result_dedup_key(&result.url)),"duplicate across pages");
    match kind {
     1=>assert!(!result.is_collection),
     2|4|5=>assert!(result.url.contains("list=")),
     3=>assert!(result.is_collection && (result.url.contains("/@") || result.url.contains("/channel/"))),
     _=>{}
    }
   }
   if let Some(next)=next {
    assert_eq!(next.page,page);
    assert_eq!(next.search_type,kind);
    assert!(seen.is_subset(&next.seen_urls));
    context=next;
   } else {panic!("no continuation for category {kind}, page {page}");}
  }
  // Reopening a saved page context must retain its filter and independent seen set.
  assert_eq!(saved_context.page,0);
  assert_eq!(saved_context.search_type,kind);
  assert_eq!(saved_context.seen_urls,youtube_seen_urls(&first));
  let (replayed,_)=youtube_direct_next_page(&saved_context).expect("replay saved context");
  assert!(!replayed.is_empty());
 }
}
'''
# Include transport causes in test failures without logging successful URLs or tokens.
code = code.replace('richiesta YouTube fallita: {err}', 'richiesta YouTube fallita: {err:?}')
(work/'src/lib.rs').write_text(code,encoding='utf8')
subprocess.run(['cargo','test','--offline','--manifest-path',str(work/'Cargo.toml'),'--','--nocapture','--test-threads=1'],check=True)

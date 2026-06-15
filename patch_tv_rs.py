import os
import re

dir_path = r"C:\rustnotepad\Sonarpad minimal\src"

def replace_in_file(filename, replacements):
    path = os.path.join(dir_path, filename)
    if not os.path.exists(path):
        return
    with open(path, 'r', encoding='utf-8') as f:
        content = f.read()
    
    for old, new in replacements:
        content = content.replace(old, new)
        
    with open(path, 'w', encoding='utf-8') as f:
        f.write(content)

# In tv.rs:
# Change pub(crate) category: TvChannelCategory -> String
# Remove enum TvChannelCategory

tv_replacements = [
    ("pub(crate) category: TvChannelCategory,", "pub(crate) category: String,"),
    ("pub(crate) enum TvChannelCategory {\n    Rai,\n    Mediaset,\n    Other,\n    Regional,\n}", ""),
    ("category: TvChannelCategory::Regional,", 'category: "Regionali".to_string(),'),
    ("category: tv_channel_category(&name),", 'category: tv_channel_category_string(&name),'),
    ("category: tv_channel_category_from_group(\n                    channel.group_title.as_deref().unwrap_or_default(),\n                    &name,\n                ),", 'category: channel.group_title.unwrap_or_else(|| "Altri".to_string()),'),
    ("category: tv_channel_category_from_group(channel.group_title.as_deref().unwrap_or_default(), &name),", 'category: channel.group_title.unwrap_or_else(|| "Altri".to_string()),')
]

# We also need to add tv_channel_category_string and remove tv_channel_category
with open(os.path.join(dir_path, 'tv.rs'), 'r', encoding='utf-8') as f:
    tv_content = f.read()

tv_content = tv_content.replace("pub(crate) category: TvChannelCategory,", "pub(crate) category: String,")
tv_content = re.sub(r"pub\(crate\) enum TvChannelCategory\s*\{[^}]*\}", "", tv_content)

# Replace local category parsing
tv_content = tv_content.replace("category: tv_channel_category(&name),", "category: tv_channel_category_string(&name),")
tv_content = tv_content.replace("category: TvChannelCategory::Regional,", 'category: "Regionali".to_string(),')

# For the remote channels, just use the group_title
tv_content = re.sub(r"category:\s*tv_channel_category_from_group\s*\([^)]*\),", 'category: channel.group_title.clone().unwrap_or_else(|| "Altri".to_string()),', tv_content, flags=re.DOTALL)

# Delete tv_channel_category and tv_channel_category_from_group
tv_content = re.sub(r"fn tv_channel_category\([^)]*\)\s*->\s*TvChannelCategory\s*\{.*?(?=\nfn|\n#\[|\n$)", "", tv_content, flags=re.DOTALL)
tv_content = re.sub(r"fn tv_channel_category_from_group\([^)]*\)\s*->\s*TvChannelCategory\s*\{.*?(?=\nfn|\n#\[|\n$)", "", tv_content, flags=re.DOTALL)

# Inject tv_channel_category_string
tv_content += """
fn tv_channel_category_string(name: &str) -> String {
    let lower = name.to_lowercase();
    if lower.contains("rai") {
        "Rai".to_string()
    } else if lower.contains("italia 1") || lower.contains("canale 5") || lower.contains("rete 4") || lower.contains("mediaset") || lower.contains("tgcom24") {
        "Mediaset".to_string()
    } else {
        "Altri".to_string()
    }
}
"""

# Now the Autoresolver logic in `pub(crate) fn resolve_tv_stream_url`
resolver_injection = """
    let mut eff_resolver = channel.stream_resolver.as_deref();
    let mut eff_channel_id = channel.resolver_channel_id.as_deref();
    
    let norm_name = channel.name.to_lowercase().replace(" ", "");
    match norm_name.as_str() {
        "realtime" => { eff_resolver = Some("aurora_channel"); eff_channel_id = Some("2"); },
        "nove" | "la9" | "9" => { eff_resolver = Some("aurora_channel"); eff_channel_id = Some("3"); },
        "dmax" => { eff_resolver = Some("aurora_channel"); eff_channel_id = Some("4"); },
        "foodnetwork" => { eff_resolver = Some("aurora_channel"); eff_channel_id = Some("6"); },
        "motortrend" => { eff_resolver = Some("aurora_channel"); eff_channel_id = Some("11"); },
        "discoverychannel" => { eff_resolver = Some("aurora_channel"); eff_channel_id = Some("12"); },
        "hgtv" => { eff_resolver = Some("aurora_channel"); eff_channel_id = Some("13"); },
        _ => {}
    }

    if let Some(resolver) = eff_resolver {
        if resolver == "aurora_channel" {
"""

tv_content = tv_content.replace(
    'if let Some(resolver) = &channel.stream_resolver {\n        if resolver == "aurora_channel" {',
    resolver_injection
)
tv_content = tv_content.replace(
    'let endpoint = channel.resolver_endpoint.as_deref()',
    'let endpoint = channel.resolver_endpoint.as_deref()'
)
tv_content = tv_content.replace(
    'let channel_id = channel.resolver_channel_id.as_deref().unwrap_or(&channel.name);',
    'let channel_id = eff_channel_id.unwrap_or(&channel.name);'
)

with open(os.path.join(dir_path, 'tv.rs'), 'w', encoding='utf-8') as f:
    f.write(tv_content)

print("Updated tv.rs")

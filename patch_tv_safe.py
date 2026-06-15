import os
import re

dir_path = r"C:\rustnotepad\Sonarpad minimal\src"

def patch_tv_resolver():
    path = os.path.join(dir_path, 'tv.rs')
    if not os.path.exists(path):
        return
    with open(path, 'r', encoding='utf-8') as f:
        content = f.read()

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

    content = content.replace(
        'if let Some(resolver) = &channel.stream_resolver {\n        if resolver == "aurora_channel" {',
        resolver_injection
    )
    content = content.replace(
        'let channel_id = channel.resolver_channel_id.as_deref().unwrap_or(&channel.name);',
        'let channel_id = eff_channel_id.unwrap_or(&channel.name);'
    )

    with open(path, 'w', encoding='utf-8') as f:
        f.write(content)

    print("Updated tv.rs with Autoresolver logic ONLY")

if __name__ == '__main__':
    patch_tv_resolver()

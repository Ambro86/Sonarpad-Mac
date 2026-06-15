import json
import re

with open(r'C:\rustnotepad\sonarpad_mobile_starter\tv_channels_resolver.php', 'r', encoding='utf-8') as f:
    content = f.read()

# Extract the JSON block
match = re.search(r'\$json\s*=\s*<<<\'JSON\'\n(.*?)\nJSON;', content, re.DOTALL)
if match:
    json_data = match.group(1)
    data = json.loads(json_data)
    
    pluto_channels = []
    la9_channels = []
    
    for channel in data:
        name = channel.get('name', '').lower()
        if 'pluto' in name:
            pluto_channels.append(channel)
        if 'la9' in name or 'la 9' in name:
            la9_channels.append(channel)
    
    print("Total channels fetched locally:", len(data))
    print("\nPluto TV Channels:")
    for ch in pluto_channels:
        print(f"- {ch.get('name')}: {ch.get('url')} (Group: {ch.get('group_title')})")
        
    print("\nLa9 Channels:")
    for ch in la9_channels:
        print(f"- {ch.get('name')}: {ch.get('url')} (Group: {ch.get('group_title')})")
else:
    print("Failed to find JSON block in PHP file.")

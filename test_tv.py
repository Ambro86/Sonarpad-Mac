import urllib.request
import json
import os

url = "https://sonarpad.com/api/tv_channels_resolver.php"
route_token = os.environ.get("SONARPAD_ROUTE_CLIENT_TOKEN", "")
tv_token = os.environ.get("SONARPAD_TV_CLIENT_TOKEN", "")

req = urllib.request.Request(url, headers={
    'Accept': 'application/json',
    'X-Sonarpad-Route-Token': route_token,
    'X-Sonarpad-TV-Token': tv_token,
    'User-Agent': 'Sonarpad TV/1.0'
})

try:
    with urllib.request.urlopen(req) as response:
        data = json.loads(response.read().decode())
        
        pluto_channels = []
        la9_channels = []
        
        for channel in data.get('channels', []):
            name = channel.get('name', '').lower()
            if 'pluto' in name:
                pluto_channels.append(channel)
            if 'la9' in name or 'la 9' in name:
                la9_channels.append(channel)
        
        print("Total channels:", len(data.get('channels', [])))
        print("\nPluto TV Channels:")
        for ch in pluto_channels:
            print(f"- {ch.get('name')}: {ch.get('url')} (Group: {ch.get('group_title')})")
            
        print("\nLa9 Channels:")
        for ch in la9_channels:
            print(f"- {ch.get('name')}: {ch.get('url')} (Group: {ch.get('group_title')})")

except Exception as e:
    print("Error:", e)

import urllib.request
import urllib.parse
import json
import subprocess
import re
import sys

def resolve_aurora_channel():
    endpoint = "https://public.aurora.enhanced.live"
    realm = "it"
    channel_id = "3"

    base_headers = {
        'Accept': 'application/json,text/plain,*/*',
        'Content-Type': 'application/json',
        'Origin': 'https://nove.tv',
        'Referer': 'https://nove.tv/live-streaming-nove',
        'User-Agent': 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/136.0.0.0 Safari/537.36',
        'X-disco-client': 'WEB:UNKNOWN:wbdatv:2.1.9',
        'X-disco-params': 'realm=' + realm,
        'X-Device-Info': 'STONEJS/1 (Unknown/Unknown; Windows/10; Unknown)'
    }

    token_url = f"{endpoint}/token?realm={urllib.parse.quote(realm)}"
    req_token = urllib.request.Request(token_url, headers=base_headers)
    
    try:
        with urllib.request.urlopen(req_token) as response:
            res_json = json.loads(response.read().decode('utf-8'))
            token = res_json.get('data', {}).get('attributes', {}).get('token')
            if not token:
                return None
    except Exception as e:
        return None

    playback_url = f"{endpoint}/playback/v3/channelPlaybackInfo"
    payload = {
        'channelId': channel_id,
        'deviceInfo': {
            'adBlocker': False,
            'drmSupported': True,
            'hdrCapabilities': ['SDR'],
            'hwDecodingCapabilities': [],
            'soundCapabilities': ['STEREO'],
        },
        'wisteriaProperties': {
            'device': {
                'browser': {
                    'name': 'chrome',
                    'version': '136',
                },
                'type': 'desktop',
            },
            'platform': 'desktop',
        },
    }
    
    playback_headers = base_headers.copy()
    playback_headers['Authorization'] = f"Bearer {token}"
    
    data = json.dumps(payload).encode('utf-8')
    req_playback = urllib.request.Request(playback_url, data=data, headers=playback_headers, method='POST')
    
    try:
        with urllib.request.urlopen(req_playback) as response:
            playback_resp = response.read().decode('utf-8')
            match = re.search(r'https?://[^\s"\'<>\\\\]+?\.m3u8[^\s"\'<>\\\\]*', playback_resp)
            if not match:
                return None
            m3u8_url = match.group(0).replace('\\/', '/')
            return m3u8_url
    except Exception as e:
        return None

if __name__ == '__main__':
    resolved_url = resolve_aurora_channel()
    if resolved_url:
        print("URL ottenuto:", resolved_url)
        print("\nTesto l'apertura con ffplay (senza video, solo audio per 5 secondi)...")
        cmd = [
            'ffplay',
            '-v', 'warning',
            '-nodisp',
            '-autoexit',
            '-t', '5',
            resolved_url
        ]
        
        result = subprocess.run(cmd)
        
        if result.returncode == 0:
            print("\nSUCCESSO! ffplay ha riprodotto l'audio correttamente per 5 secondi.")
        else:
            print("\nERRORE! ffplay non e riuscito a riprodurre l'audio.")

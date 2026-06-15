import urllib.request
import urllib.parse
import json
import subprocess
import re

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

    print("1. Richiedo il token ad Aurora...")
    token_url = f"{endpoint}/token?realm={urllib.parse.quote(realm)}"
    req_token = urllib.request.Request(token_url, headers=base_headers)
    
    try:
        with urllib.request.urlopen(req_token) as response:
            res_json = json.loads(response.read().decode('utf-8'))
            token = res_json.get('data', {}).get('attributes', {}).get('token')
            if not token:
                print("Errore: token non trovato nella risposta.")
                return None
            print("   Token ottenuto con successo.")
    except Exception as e:
        print(f"Errore durante la richiesta del token: {e}")
        return None

    print("2. Richiedo l'URL di playback...")
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
            
            # Find the m3u8 url using regex (similar to the PHP script)
            match = re.search(r'https?://[^\s"\'<>\\\\]+?\.m3u8[^\s"\'<>\\\\]*', playback_resp)
            if not match:
                print("Errore: URL m3u8 non trovato nel JSON.")
                return None
            
            m3u8_url = match.group(0).replace('\\/', '/')
            print(f"   URL m3u8 risolto: {m3u8_url}")
            return m3u8_url
    except Exception as e:
        print(f"Errore durante la richiesta di playback: {e}")
        return None

def test_stream_with_ffprobe(url):
    print("\n3. Testo lo stream con ffprobe...")
    cmd = [
        'ffprobe',
        '-v', 'error',
        '-headers', 'Origin: https://nove.tv\r\nReferer: https://nove.tv/live-streaming-nove\r\nUser-Agent: Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/136.0.0.0 Safari/537.36\r\n',
        '-show_entries', 'stream=codec_name,codec_type',
        '-of', 'default=noprint_wrappers=1:nokey=1',
        url
    ]
    
    result = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    
    if result.returncode == 0:
        codecs = result.stdout.strip().split('\n')
        print("Lo streaming funziona perfettamente! Codec rilevati nel flusso:")
        for i, codec in enumerate(codecs):
            if codec:
                print(f"  - Traccia {i+1}: {codec.upper()}")
    else:
        print("Errore durante il test dello stream:")
        print(result.stderr)

if __name__ == '__main__':
    resolved_url = resolve_aurora_channel()
    if resolved_url:
        test_stream_with_ffprobe(resolved_url)

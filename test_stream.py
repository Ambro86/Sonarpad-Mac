import urllib.request
import json
import subprocess
import sys

url = "https://sonarpad.com/api/tv_channels_resolver.php"
token = ""

req = urllib.request.Request(url, headers={
    'Accept': 'application/json',
    'X-Sonarpad-Route-Token': token,
    'User-Agent': 'Sonarpad TV/1.0'
})

try:
    print("Recupero l'elenco dei canali da sonarpad.com...")
    with urllib.request.urlopen(req) as response:
        data = json.loads(response.read().decode())
        
        la9_url = None
        for channel in data.get('channels', []):
            name = channel.get('name', '').lower()
            if 'la9' in name or 'la 9' in name:
                la9_url = channel.get('url')
                break
        
        if la9_url:
            print(f"\nTrovato La9. Sto testando lo streaming con ffprobe...\nURL: {la9_url}\n")
            
            # Use ffprobe to check if the stream is readable and what codecs it contains
            cmd = [
                'ffprobe',
                '-v', 'error',
                '-headers', 'Origin: https://nove.tv\r\nReferer: https://nove.tv/live-streaming-nove\r\nUser-Agent: Mozilla/5.0 (Windows NT 10.0; Win64; x64)\r\n',
                '-show_entries', 'stream=codec_name,codec_type',
                '-of', 'default=noprint_wrappers=1:nokey=1',
                la9_url
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
        else:
            print("Canale La9 non trovato nell'elenco.")

except Exception as e:
    print("Error:", e)

import gzip
import base64
import json

def decrypt(payload_json, secret_key):
    static_parts = [b"sonar", b"pad-", b"SonarSecure-"]
    key = secret_key.encode('utf-8')
    for part in static_parts:
        key += part
        
    data = json.loads(payload_json)
    encrypted = base64.b64decode(data['payload_b64'])
    
    decrypted = bytearray()
    for i, byte in enumerate(encrypted):
        decrypted.append(byte ^ key[i % len(key)])
        
    return gzip.decompress(decrypted).decode('utf-8')

tv_channels = r'''{"payload_b64":"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","algorithm":"gzip-xor-base64-v1"}'''

regional_channels = r'''{"payload_b64":"csAxIXZQMnhMMo4nOUegdie+WUmPr85Uj4uCpYb+eOdb7HZeI0qNCD7T7Xlq+7Z34/VojdnssHYJrF8MsIN5yUmAdYLumBrJ3Y1fD74xbk4UyD4bsx3g1GAH6rS84/ksoTk2E5536VjBOx7IYFz//hI2DPNEaUMkNk3znFwDXVNEy6+Gs3OSPhzVG0YQg3Bp3eSj7+Wkpd4LYzxx8+P6woCB2UIxKrXej36b/GQGUDa4cAtnmqla69uoehBDRXsxpp4RqRyyEb2tNmQ3RJW5O0OAP5bLMLL7atRd6lJbLxaATCMxAitjohs7CnWm8GxiW/cpCuMxbLffbLf1U/FkojcCWoVMxjem+MWbDOpyuO3x5muELmrcBfSFOTGK+6DNBm7b8XfmwsQz3QmoQRHIYcfwje/j6a/9frT35X3LSJAB0+UwSVGeMvCSSyeQcTJY3NtQUPd59Qt0svtvFwdksay9OjgKmvZiPQ5k+4hsR/DPixpoUFDgJboB3rR9+kw2i1mzYuAt7dEazKqaDh9C4uZTFVroYKOYNLxcfPoDvLE4NM0LJo6uRuYotBkMQS6GKep2TCJciQi9d+cbBLcfoVRw3Yajra9G/ZLbihg22u50J3iLTuUkSfsUHWepqHC90J//JfXmqFYzNKCZe5KO5vtS6z6+F1P51ZkPdA1En4a3jokxcqsPFN38rlbpsXG4IuphfOeNxneAac0Cjxlm1p/E7BdAnZkWb0g/i8gTcTmVjP5/4OICFuz5mn3DHcnXD61/vpIBJVvcgr8JnPx3OuYCfkoBgt3XSOoVjd2rqOKYfZyj64RqZrXV2olVFmKSBjU+9aVCugpQaYSeOgOcaO3qhFh7lqmb1blGphnTTe0eV4EtT068jcyJlzeGZ9n9I7ZqIVodQZXSJwJnRFNQLXnn6d3g86kkH5uyX2+/GiEWGGMlNIHm4g/wnzFkRgr9R/OSo2Fn9slLaF5vcpsYVy7GQK+O7p3wN0eSRb3Y39Qbbz2LB58SAEU432sE7yI6wA4cHgit33noxPQWM2kGNWfSFvJILtsEpWMAJYJnmwxGTIQgLwJRjGnZ8nnBdaUpLNf7ap9EHcW3PdAjH2Hjs+EG9SGj6f6D/x9RGzuowx7Am1t4odnr+KqBC34Z7jhe3ixIhkEE5pqfiPnqheU6X5gG/OZMKymJwo2+uOSPdWsu7A2nUaHJyUuv2sKbPjxELlIfjs8cWK21XEvL5fk9wfo3KdGjQkDrRKo9dpJbEzDKsexZqrT9c+A3ELAhv6gIMI0TRLCApOyH3E9/hFv7uzzcd4W10GEzWaOz1BZ+pVZy0+NCEyrQeFJDS8FxorM/2VPCGIPgejoFJ/UcaJeYyEILf1ZwcHb0G/SmyQTBY7F4eLnhwbvnVehk6clOMa09b9Lk+vxkvSAGuAbTnN+ZQGRWuk77GmFgTyho/0tmf+5y0XUG7zm8FXEjH0yiGtHoO7jrCkXH+pRGTPi9gA3RKZkhlfOiwa2bVrk/6/VtUloaCv0hrS0mvPojNLr6SPFEQDayGWwEluc+TWqkJO9VdvTNXcVIDDBvPnm6WcvsyUkcPX4PnbDVm+w5mzvNd4e8B7jHWHJM","algorithm":"gzip-xor-base64-v1"}'''

key1 = "mK9!vP2xL8#qT4zN7@rW1sY6dF0hJ3uBzUrL1BirM@|"
key2 = "mK9!vP2xL8#qT4zN7@rW1sY6dF0hJ3uBzUrL1BirM@|\\"

print("Decoding local with key1")
try:
    print(decrypt(tv_channels, key1)[:200])
except Exception as e:
    print("Failed key1:", e)
    
print("\nDecoding local with key2")
try:
    print(decrypt(tv_channels, key2)[:200])
except Exception as e:
    print("Failed key2:", e)

try:
    parsed = json.loads(decrypt(tv_channels, key1))
    parsed_reg = json.loads(decrypt(regional_channels, key1))
    
    pluto = [c['name'] for c in parsed['channels'] if 'pluto' in c['name'].lower()]
    la9 = [c['name'] for c in parsed['channels'] if 'la9' in c['name'].lower() or 'la 9' in c['name'].lower()]
    
    print("Local Pluto:", pluto)
    print("Local La9:", la9)
    
    pluto_reg = [c['name'] for c in parsed_reg['channels'] if 'pluto' in c['name'].lower()]
    la9_reg = [c['name'] for c in parsed_reg['channels'] if 'la9' in c['name'].lower() or 'la 9' in c['name'].lower()]
    
    print("Regional Pluto:", pluto_reg)
    print("Regional La9:", la9_reg)

except Exception as e:
    print(e)

"""Isolated real Metal smoke test; never changes the production CPU bridge."""
import json
import os
from pathlib import Path
import platform
import re
import subprocess
import sys
import urllib.request
import wave

import av
import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'bridge'))
from faster_whisper_bridge import decode_audio_mono_16k
from whisper_cpp_backend import download_cpp_model, parse_segments

OUT = Path('metal-test-results').resolve()
OUT.mkdir(exist_ok=True)
assert platform.machine() == 'arm64', platform.machine()
model = download_cpp_model('small', str(Path(os.environ['RUNNER_TEMP']) / 'models' / 'faster-whisper'))
binary = OUT / 'whisper-cli'
urllib.request.urlretrieve(
    'https://raw.githubusercontent.com/ggml-org/whisper.cpp/2eeeba56e9edd762b4b38467bab96c2517163158/samples/jfk.wav',
    OUT / 'english.wav',
)
with av.open(str(OUT / 'english.wav')) as source, av.open(str(OUT / 'english.mkv'), 'w') as dest:
    video = dest.add_stream('mpeg4', rate=1)
    video.width, video.height, video.pix_fmt = 160, 90, 'yuv420p'
    audio = dest.add_stream('pcm_s16le', rate=16000)
    audio.layout = 'mono'
    for index in range(12):
        frame = av.VideoFrame.from_ndarray(np.zeros((90, 160, 3), dtype=np.uint8), format='rgb24')
        frame.pts = index
        for packet in video.encode(frame):
            dest.mux(packet)
    for packet in video.encode(None):
        dest.mux(packet)
    for frame in source.decode(audio=0):
        for packet in audio.encode(frame):
            dest.mux(packet)
    for packet in audio.encode(None):
        dest.mux(packet)

subprocess.run(['say', '-v', 'Alice', '-r', '150', '-o', str(OUT / 'italian.aiff'),
                'Oggi proviamo la trascrizione in italiano. Questo programma converte la voce in testo '
                'e permette di lavorare con file audio e video.'], check=True, timeout=60)

checks = []
for filename, language, expected in [('english.mkv', 'en', ['americans', 'country']),
                                      ('italian.aiff', 'it', ['italiano', 'audio', 'video'])]:
    # Same PyAV decoder, model, beam size and result parser as Sonarpad.
    samples = decode_audio_mono_16k(str(OUT / filename))
    wav_path = OUT / (language + '-decoded.wav')
    with wave.open(str(wav_path), 'wb') as wav:
        wav.setnchannels(1)
        wav.setsampwidth(2)
        wav.setframerate(16000)
        wav.writeframes(np.clip(samples * 32768, -32768, 32767).astype('<i2').tobytes())
    output = OUT / (language + '-transcript')
    command = [str(binary), '-m', str(model), '-f', str(wav_path), '-l', language,
               '-t', '3', '-bs', '5', '-bo', '5', '-mc', '0', '-pp', '-oj', '-of', str(output)]
    # GPU enabled by default: deliberately omit the production -ng switch.
    with (OUT / (language + '.log')).open('w', encoding='utf-8') as log:
        process = subprocess.run(command, stdout=log, stderr=subprocess.STDOUT, timeout=240)
    log = (OUT / (language + '.log')).read_text(encoding='utf-8')
    assert process.returncode == 0, log
    # Merely discovering a GPU is insufficient: require its backend to be used.
    assert 'whisper_backend_init_gpu: using Metal backend' in log, log
    payload = json.loads(output.with_suffix('.json').read_text(encoding='utf-8'))
    segments = parse_segments(payload, language, len(samples) / 16000)
    transcript = ' '.join(segment.text for segment in segments)
    assert all(word in transcript.lower() for word in expected), transcript
    progress = [int(value) for value in re.findall(r'progress\s*=\s*(\d+)%', log)]
    assert progress and progress == sorted(progress), progress
    device = re.search(r'GPU name:\s*(.+)', log)
    checks.append({'input': filename, 'language': language, 'metal_used': True,
                   'device': device.group(1).strip() if device else 'see log',
                   'text': transcript, 'progress': progress})
    print(json.dumps(checks[-1], ensure_ascii=False), flush=True)
(OUT / 'summary.json').write_text(json.dumps({'ok': True, 'checks': checks,
    'scope': 'Functional CLI test on GitHub GPU; not a physical Mac performance measurement.'},
    ensure_ascii=False, indent=2), encoding='utf-8')

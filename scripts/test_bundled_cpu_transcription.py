"""Run short integration checks against the actual PyInstaller macOS worker."""
import json
import os
import platform
from pathlib import Path
import re
import signal
import subprocess
import sys
import tempfile
import time
import urllib.request

import av
import numpy as np

OUT = Path('transcription-test-results')
OUT.mkdir(exist_ok=True)
WORKER = Path(sys.argv[1]).resolve()
CACHE = Path(os.environ['RUNNER_TEMP']) / 'sonarpad-transcription-test-models' / 'faster-whisper'
CACHE.mkdir(parents=True, exist_ok=True)


def command(path, language):
    return [str(WORKER), '--model', 'small', '--input', str(path), '--language', language,
            '--download-root', str(CACHE), '--timestamps']


def transcribe(path, language, label, expected_backend=None):
    start = time.perf_counter()
    process = subprocess.run(command(path, language), capture_output=True, text=True, timeout=240)
    elapsed = time.perf_counter() - start
    (OUT / (label + '.log')).write_text(process.stdout + process.stderr, encoding='utf-8')
    assert process.returncode == 0, process.stderr + process.stdout
    results = [json.loads(line) for line in process.stdout.splitlines() if line.startswith('{')]
    result = results[-1]
    expected_backend = expected_backend or ('whisper.cpp-metal' if platform.machine() == 'arm64' else 'whisper.cpp-cpu')
    assert result['ok'] and result['backend'] == expected_backend, result
    assert result['language'] == language and result['text'].strip(), result
    assert re.search(r'\[\d+:\d+\]', result['text']), result
    progress = [int(line.split(':')[1]) for line in process.stdout.splitlines() if line.startswith('PROGRESS:')]
    assert progress and progress == sorted(set(progress)) and max(progress) <= 99, progress
    result['wall_seconds_including_download_if_uncached'] = elapsed
    result['progress'] = progress
    (OUT / (label + '.json')).write_text(json.dumps(result, indent=2, ensure_ascii=False), encoding='utf-8')
    print(label, elapsed, result['text'], flush=True)
    return result


urllib.request.urlretrieve(
    'https://raw.githubusercontent.com/ggml-org/whisper.cpp/2eeeba56e9edd762b4b38467bab96c2517163158/samples/jfk.wav',
    OUT / 'jfk.wav',
)
# Wrap the public test speech in a real video container, without changing audio.
with av.open(str(OUT / 'jfk.wav')) as source, av.open(str(OUT / 'sample.mkv'), 'w') as dest:
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

english = transcribe(OUT / 'sample.mkv', 'en', 'video-english')
assert 'country' in english['text'].lower() and 'americans' in english['text'].lower()

# Use an installed native Italian voice; do not call a paid or private service.
italian_text = ('Oggi proviamo la trascrizione in italiano. Questo programma converte la voce in testo '
                'e permette di lavorare con file audio e video.')
subprocess.run(['say', '-v', 'Alice', '-r', '150', '-o', str(OUT / 'italian.aiff'), italian_text],
               check=True, timeout=60)
italian = transcribe(OUT / 'italian.aiff', 'it', 'audio-italian')
assert all(word in italian['text'].lower() for word in ['italiano', 'audio', 'video']), italian

# Exercise the same process-group cancellation used by the Rust host. There
# must be no retry, surviving child, or new temporary WAV after cancellation.
temp_root = Path(tempfile.gettempdir())
before = set(temp_root.glob('sonarpad-transcription-*'))
process = subprocess.Popen(command(OUT / 'sample.mkv', 'en'), stdout=subprocess.PIPE,
                           stderr=subprocess.DEVNULL, text=True, start_new_session=True)
try:
    for line in process.stdout:
        if line.startswith('STAGE:transcribing'):
            time.sleep(0.5)
            os.killpg(process.pid, signal.SIGTERM)
            break
    process.wait(timeout=15)
    assert process.returncode != 0
    for _ in range(30):
        children = subprocess.run(['pgrep', '-g', str(process.pid)], capture_output=True, text=True)
        if not children.stdout.strip():
            break
        time.sleep(0.1)
    assert not children.stdout.strip(), children.stdout
    assert set(temp_root.glob('sonarpad-transcription-*')) <= before
finally:
    if process.poll() is None:
        os.killpg(process.pid, signal.SIGKILL)
        process.wait()
    process.stdout.close()
(OUT / 'cancellation.json').write_text(json.dumps({'ok': True, 'no_surviving_processes': True, 'temporary_audio_cleaned': True}))
print('All bundled CPU transcription checks passed.', flush=True)

if platform.machine() == 'arm64':
    # A real child-process failure must recover through the independent CPU
    # executable using the already downloaded model, without duplicate text.
    metal = WORKER.with_name('whisper-metal')
    saved = metal.with_name('whisper-metal.saved')
    metal.rename(saved)
    try:
        metal.write_text('#!/bin/sh\necho "simulated Metal failure" >&2\nexit 42\n', encoding='utf-8')
        metal.chmod(0o755)
        recovered = transcribe(OUT / 'sample.mkv', 'en', 'metal-failure-cpu-recovery', 'whisper.cpp-cpu')
        assert recovered['text'] == english['text'], recovered
        log = (OUT / 'metal-failure-cpu-recovery.log').read_text(encoding='utf-8')
        assert 'BACKEND_FALLBACK:whisper.cpp-metal->whisper.cpp-cpu:' in log
    finally:
        metal.unlink(missing_ok=True)
        saved.rename(metal)
print('All bundled Metal/CPU transcription and recovery checks passed.', flush=True)

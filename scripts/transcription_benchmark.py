"""Isolated macOS experiment. Never changes the application's transcription code."""
import argparse
import hashlib
import importlib.metadata
import json
import os
from pathlib import Path
import platform
import re
import resource
import subprocess
import sys
import time
import wave

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / 'benchmark-results'
CPP = ROOT / 'bench-whisper'
SECONDS = 30
REFERENCE = (
    'And so my fellow Americans ask not what your country can do for you '
    'ask what you can do for your country'
)
VARIANTS = ['baseline', 'native_threads', 'float32', 'greedy']


def save(name, value):
    (OUT / name).write_text(json.dumps(value, indent=2, ensure_ascii=False), encoding='utf-8')


def word_error_rate(reference, text):
    ref = re.findall(r'\w+', reference.lower())
    hyp = re.findall(r'\w+', text.lower())
    previous = list(range(len(hyp) + 1))
    for i, word in enumerate(ref, 1):
        current = [i]
        for j, candidate in enumerate(hyp, 1):
            current.append(min(current[-1] + 1, previous[j] + 1,
                               previous[j - 1] + (word != candidate)))
        previous = current
    return previous[-1] / max(1, len(ref))


def prepare_clip():
    import av
    import numpy as np
    from fractions import Fraction
    with wave.open(str(CPP / 'samples/jfk.wav'), 'rb') as source:
        assert source.getframerate() == 16000 and source.getnchannels() == 1
        assert source.getsampwidth() == 2
        speech = np.frombuffer(source.readframes(source.getnframes()), dtype='<i2')
    assert speech.size * 2 < SECONDS * 16000
    audio = np.zeros(SECONDS * 16000, dtype=np.int16)
    audio[:speech.size] = speech
    second = speech.size + 16000
    audio[second:second + speech.size] = speech
    with wave.open(str(OUT / 'sample.wav'), 'wb') as dest:
        dest.setnchannels(1)
        dest.setsampwidth(2)
        dest.setframerate(16000)
        dest.writeframes(audio.tobytes())
    # Tiny video with identical lossless audio: compare video decoding separately
    # from inference, rather than measuring network downloads as transcription.
    with av.open(str(OUT / 'sample.mkv'), 'w') as container:
        video = container.add_stream('mpeg4', rate=1)
        video.width, video.height, video.pix_fmt = 160, 90, 'yuv420p'
        sound = container.add_stream('pcm_s16le', rate=16000)
        sound.layout = 'mono'
        for index in range(SECONDS):
            frame = av.VideoFrame.from_ndarray(np.zeros((90, 160, 3), dtype=np.uint8), format='rgb24')
            frame.pts, frame.time_base = index, Fraction(1, 1)
            for packet in video.encode(frame):
                container.mux(packet)
        for packet in video.encode(None):
            container.mux(packet)
        for offset in range(0, audio.size, 16000):
            frame = av.AudioFrame.from_ndarray(audio[offset:offset + 16000].reshape(1, -1), format='s16', layout='mono')
            frame.sample_rate, frame.pts, frame.time_base = 16000, offset, Fraction(1, 16000)
            for packet in sound.encode(frame):
                container.mux(packet)
        for packet in sound.encode(None):
            container.mux(packet)
    return hashlib.sha256((OUT / 'sample.wav').read_bytes()).hexdigest()


def metal_probe():
    source = OUT / 'metal_probe.swift'
    source.write_text('import Metal\nimport Foundation\n'
                      'let devices = MTLCopyAllDevices().map { $0.name }\n'
                      'let data = try JSONSerialization.data(withJSONObject: devices)\n'
                      'print(String(data: data, encoding: .utf8)!)\n', encoding='utf-8')
    result = subprocess.run(['swift', str(source)], capture_output=True, text=True, timeout=90)
    (OUT / 'metal_probe.log').write_text(result.stdout + result.stderr, encoding='utf-8')
    return json.loads(result.stdout) if result.returncode == 0 else []


def faster_whisper_case(case):
    import ctranslate2
    from faster_whisper import WhisperModel
    sys.path.insert(0, str(ROOT / 'bridge'))
    import faster_whisper_bridge as bridge
    model_path = json.loads((OUT / 'models.json').read_text())['faster_whisper']
    threads = 4 if case == 'baseline' else min(4, os.cpu_count() or 1)
    compute = 'float32' if case == 'float32' else 'int8'
    start = time.perf_counter()
    if case == 'baseline':
        model = bridge.create_transcription_model(model_path, None)
    else:
        model = WhisperModel(model_path, device='cpu', compute_type=compute, cpu_threads=threads)
    load_seconds = time.perf_counter() - start
    engine = model
    trials = []
    for repeat in range(2):
        start = time.perf_counter()
        audio = bridge.decode_audio_mono_16k(str(OUT / 'sample.mkv'))
        decode = time.perf_counter() - start
        options = dict(language='en', task='transcribe', vad_filter=False,
                       beam_size=1 if case == 'greedy' else 5,
                       condition_on_previous_text=False)
        started = time.perf_counter()
        segments, _ = engine.transcribe(audio, **options)
        text = ' '.join(segment.text.strip() for segment in segments)
        inference = time.perf_counter() - started
        trials.append(dict(repeat=repeat, decode_seconds=decode, inference_seconds=inference,
                           total_seconds=decode + inference, rtf=(decode + inference) / SECONDS,
                           text=text, wer=word_error_rate(REFERENCE + ' ' + REFERENCE, text)))
    save(case + '.json', dict(case=case, engine='faster-whisper', compute=compute,
                             threads=threads, load_seconds=load_seconds, trials=trials,
                             peak_rss_bytes=resource.getrusage(resource.RUSAGE_SELF).ru_maxrss,
                             supported_compute=sorted(ctranslate2.get_supported_compute_types('cpu'))))


def cpp_case(gpu):
    name = 'cpp_metal' if gpu else 'cpp_cpu'
    trials = []
    for repeat in range(2):
        prefix = OUT / f'{name}-{repeat}'
        command = [str(CPP / 'build/bin/whisper-cli'), '-m', str(CPP / 'models/ggml-small.bin'),
                   '-f', str(OUT / 'sample.wav'), '-l', 'en', '-t', str(min(4, os.cpu_count() or 1)),
                   '-bs', '5', '-bo', '5', '-mc', '0', '-oj', '-of', str(prefix)]
        if not gpu:
            command.append('-ng')
        started = time.perf_counter()
        process = subprocess.run(command, capture_output=True, text=True, timeout=180)
        elapsed = time.perf_counter() - started
        log = process.stdout + process.stderr
        prefix.with_suffix('.log').write_text(log, encoding='utf-8')
        if process.returncode != 0:
            raise RuntimeError(log[-3000:])
        data = json.loads(prefix.with_suffix('.json').read_text())
        text = ' '.join(segment['text'].strip() for segment in data['transcription'])
        load = re.search(r'load time\s*=\s*([\d.]+) ms', log)
        trials.append(dict(repeat=repeat, process_seconds=elapsed, rtf=elapsed / SECONDS,
                           load_seconds=float(load[1]) / 1000 if load else None,
                           text=text, wer=word_error_rate(REFERENCE + ' ' + REFERENCE, text)))
    # CLI wall time includes loading the model each time; FW trials keep it loaded.
    save(name + '.json', dict(case=name, engine='whisper.cpp', trials=trials,
                             timing='process including model load; PCM input excludes video decode'))


def run_experiments():
    from faster_whisper.utils import download_model
    OUT.mkdir(exist_ok=True)
    devices = metal_probe()
    sample_hash = prepare_clip()
    packages = ['faster-whisper', 'ctranslate2', 'av', 'numpy', 'onnxruntime']
    save('environment.json', dict(platform=platform.platform(), machine=platform.machine(),
                                 cpu_count=os.cpu_count(), metal_devices=devices,
                                 packages={name: importlib.metadata.version(name) for name in packages},
                                 sample_sha256=sample_hash, seconds=SECONDS,
                                 note='Modern hosted macOS, NOT Catalina. One short English sample; not a general accuracy benchmark.'))
    started = time.perf_counter()
    model = download_model('small')
    save('models.json', dict(faster_whisper=model, download_seconds=time.perf_counter() - started,
                            cpp_commit='2eeeba56e9edd762b4b38467bab96c2517163158',
                            cpp_sha256=hashlib.sha256((CPP / 'models/ggml-small.bin').read_bytes()).hexdigest()))
    cases = VARIANTS + ['cpp_cpu'] + (['cpp_metal'] if devices else [])
    if not devices:
        save('cpp_metal.json', dict(case='cpp_metal', unavailable='Runner exposes no Metal device; no GPU speed claim possible.'))
    for case in cases:
        print('BENCHMARK ' + case, flush=True)
        try:
            result = subprocess.run([sys.executable, __file__, '--case', case],
                                    capture_output=True, text=True, timeout=390)
            (OUT / (case + '.log')).write_text(result.stdout + result.stderr, encoding='utf-8')
            if result.returncode:
                save(case + '.json', dict(case=case, error=result.stderr[-3000:]))
        except subprocess.TimeoutExpired:
            save(case + '.json', dict(case=case, error='Timed out at 390 seconds'))
    rows = ['| Variant | First run (s) | Second run (s) | WER second run |',
            '|---|---:|---:|---:|']
    failures = []
    for case in cases:
        data = json.loads((OUT / (case + '.json')).read_text())
        if 'error' in data:
            failures.append(case)
            rows.append(f'| {case} | ERROR | | |')
        else:
            times = [trial.get('total_seconds', trial.get('process_seconds')) for trial in data['trials']]
            rows.append(f"| {case} | {times[0]:.2f} | {times[1]:.2f} | {data['trials'][1]['wer']:.1%} |")
    summary = '\n'.join(rows) + '\n\n30s multilingual small model; known English JFK excerpt twice with silence. Downloads excluded.\nFW: model load excluded (recorded separately). CPP: process/model load included, video decode excluded.\nCPU/GPU backend changes and greedy decoding may change accuracy. No actual Catalina runtime test.\nMetal devices: ' + repr(devices)
    (OUT / 'summary.txt').write_text(summary, encoding='utf-8')
    if os.environ.get('GITHUB_STEP_SUMMARY'):
        Path(os.environ['GITHUB_STEP_SUMMARY']).write_text(summary, encoding='utf-8')
    print(summary, flush=True)
    if failures:
        raise SystemExit('Benchmark cases failed: ' + ', '.join(failures))


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--case', choices=VARIANTS + ['cpp_cpu', 'cpp_metal'])
    args = parser.parse_args()
    if not args.case:
        run_experiments()
    elif args.case.startswith('cpp_'):
        cpp_case(args.case == 'cpp_metal')
    else:
        faster_whisper_case(args.case)

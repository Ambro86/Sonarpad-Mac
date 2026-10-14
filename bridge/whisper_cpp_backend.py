"""CPU-only whisper.cpp adapter for the existing transcription bridge protocol."""
import contextlib
import json
import os
from pathlib import Path
import platform
import re
import signal
import subprocess
import sys
import tempfile
import threading
from types import SimpleNamespace
import wave

MODEL_REVISION = "5359861c739e955e79d9a303bcbc70fb988958b1"
SUPPORTED_MODELS = {"small", "medium", "large-v3"}


@contextlib.contextmanager
def cleanup_on_termination():
    # Rust cancels the entire process group with SIGTERM. Unwind Python's
    # temporary-file context as well; never interpret cancellation as fallback.
    previous = None
    if threading.current_thread() is threading.main_thread():
        def terminate(signum, _frame):
            raise SystemExit(128 + signum)
        previous = signal.signal(signal.SIGTERM, terminate)
    try:
        yield
    finally:
        if previous is not None:
            signal.signal(signal.SIGTERM, previous)


def supports_cpp_runtime():
    if sys.platform != "darwin":
        return False
    try:
        version = tuple(int(part) for part in platform.mac_ver()[0].split(".")[:2])
    except ValueError:
        return False
    # The new Accelerate BLAS interface requires macOS 13.3. Preserve the
    # existing compatible backend on Catalina and older supported Intel Macs.
    return version >= (13, 3)


def bundled_cpp_binary():
    root = Path(sys.executable).resolve().parent if getattr(sys, "frozen", False) else Path(__file__).resolve().parent
    binary = root / "whisper-cli"
    return binary if binary.is_file() else None


def validate_cpp_binary(binary):
    result = subprocess.run([str(binary), "--help"], capture_output=True, text=True, timeout=15)
    if result.returncode != 0 or "--no-gpu" not in result.stdout + result.stderr:
        raise RuntimeError("bundled whisper.cpp CPU executable is unavailable")


def download_cpp_model(model_name, download_root):
    from huggingface_hub import hf_hub_download
    if model_name not in SUPPORTED_MODELS:
        raise ValueError("unsupported whisper.cpp model")
    cache = Path(download_root).parent / "whisper-cpp" if download_root else None
    path = hf_hub_download(
        repo_id="ggerganov/whisper.cpp", filename=f"ggml-{model_name}.bin",
        revision=MODEL_REVISION, cache_dir=str(cache) if cache else None,
    )
    with open(path, "rb") as source:
        if source.read(4) != b"lmgg":
            raise ValueError("invalid whisper.cpp model header")
    return Path(path)


def parse_segments(payload, language, duration):
    if payload.get("result", {}).get("language") != language:
        raise ValueError("whisper.cpp returned an unexpected language")
    raw_segments = payload.get("transcription")
    if not isinstance(raw_segments, list):
        raise ValueError("whisper.cpp response is missing transcription segments")
    segments = []
    for item in raw_segments:
        start = float(item["offsets"]["from"]) / 1000.0
        end = float(item["offsets"]["to"]) / 1000.0
        if not (0 <= start <= end) or not isinstance(item.get("text"), str):
            raise ValueError("invalid whisper.cpp transcription segment")
        segments.append(SimpleNamespace(text=item["text"], start=min(start, duration), end=min(end, duration)))
    return segments


class CppThenCpuModel:
    def __init__(self, binary, model_path, fallback_factory):
        self.binary = Path(binary)
        self.model_path = Path(model_path)
        self.fallback_factory = fallback_factory
        self.fallback = None
        self.progress_callback = None

    @property
    def backend(self):
        return "cpu" if self.fallback is not None else "whisper.cpp-cpu"

    @property
    def compute_type(self):
        return "int8" if self.fallback is not None else "float16-weights"

    def transcribe(self, audio, **options):
        if self.fallback is not None:
            return self.fallback.transcribe(audio, **options)
        try:
            segments = self._transcribe_cpp(audio, options)
            return iter(segments), SimpleNamespace(language=options["language"])
        except Exception as error:
            # No segments escape before the complete result is validated, so a
            # retry cannot duplicate part of the transcription. Cancellation
            # signals/KeyboardInterrupt are not caught as recoverable failures.
            print(f"BACKEND_FALLBACK:whisper.cpp-cpu:{type(error).__name__}: {error}", file=sys.stderr, flush=True)
            self.fallback = self.fallback_factory()
            return self.fallback.transcribe(audio, **options)

    def _transcribe_cpp(self, audio, options):
        import numpy as np
        language = str(options.get("language", "")).strip().lower()
        if not language or language == "auto" or options.get("task") != "transcribe":
            raise ValueError("explicit transcription language is required")
        samples = np.asarray(audio, dtype=np.float32).reshape(-1)
        if samples.size == 0:
            raise ValueError("input audio is empty")
        with cleanup_on_termination(), tempfile.TemporaryDirectory(prefix="sonarpad-transcription-") as folder:
            root = Path(folder)
            wav_path, output = root / "audio.wav", root / "transcript"
            pcm = np.clip(samples * 32768.0, -32768, 32767).astype("<i2")
            with wave.open(str(wav_path), "wb") as wav:
                wav.setnchannels(1)
                wav.setsampwidth(2)
                wav.setframerate(16000)
                wav.writeframes(pcm.tobytes())
            command = [str(self.binary), "-m", str(self.model_path), "-f", str(wav_path),
                       "-l", language, "-ng", "-t", str(min(4, os.cpu_count() or 1)),
                       "-bs", str(options.get("beam_size", 5)), "-bo", "5", "-mc", "0",
                       "-pp", "-oj", "-of", str(output)]
            # Inherit the worker's process group: the Rust host cancels the
            # worker and this child together. Never create a detached session.
            process = subprocess.Popen(command, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE,
                                       text=True, encoding="utf-8", errors="replace")
            tail = []
            try:
                for line in process.stderr:
                    tail.append(line.rstrip())
                    tail = tail[-30:]
                    match = re.search(r"progress\s*=\s*(\d+)%", line)
                    if match and self.progress_callback:
                        self.progress_callback(min(100, int(match[1])))
                if process.wait() != 0:
                    raise RuntimeError("whisper.cpp failed: " + " | ".join(tail))
            finally:
                if process.poll() is None:
                    process.terminate()
                    try:
                        process.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        process.wait()
                if process.stderr:
                    process.stderr.close()
            payload = json.loads(output.with_suffix(".json").read_text(encoding="utf-8"))
            return parse_segments(payload, language, samples.size / 16000)


def prefer_cpp_model(model_name, download_root, fallback_factory):
    if not supports_cpp_runtime():
        return fallback_factory()
    binary = bundled_cpp_binary()
    if binary is None:
        return fallback_factory()
    try:
        validate_cpp_binary(binary)
        model_path = download_cpp_model(model_name, download_root)
        return CppThenCpuModel(binary, model_path, fallback_factory)
    except Exception as error:
        print(f"BACKEND_FALLBACK:whisper.cpp-cpu:{type(error).__name__}: {error}", file=sys.stderr, flush=True)
        return fallback_factory()

#!/usr/bin/env python3
import argparse
import json
import os
import sys
import wave


BRIDGE_PROTOCOL_VERSION = 2
LONG_AUDIO_THRESHOLD_SECONDS = 30 * 60
LONG_AUDIO_CHUNK_SECONDS = 15 * 60
WHISPER_SAMPLE_RATE = 16000
QUIET_SEARCH_RADIUS_SECONDS = 10
QUIET_WINDOW_SECONDS = 0.2
QUIET_RMS_THRESHOLD = 0.02
FALLBACK_OVERLAP_SECONDS = 1


def print_json(payload):
    print(json.dumps(payload, ensure_ascii=False), flush=True)


def configure_stdio_utf8():
    try:
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    except Exception:
        pass
    try:
        sys.stderr.reconfigure(encoding="utf-8", errors="replace")
    except Exception:
        pass


def first_audio_stream(container):
    """Return the first actual audio Stream without PyAV's typed index lookup."""
    for stream in container.streams:
        if getattr(stream, "type", "") == "audio":
            return stream
    return None


def decode_audio_mono_16k(path):
    """Decode to mono float32/16 kHz before handing audio to faster-whisper.

    Passing an ndarray makes faster-whisper skip its own PyAV decode path.
    Selecting the concrete Stream object also avoids PyAV's audio=0 tuple lookup,
    which can raise IndexError on some otherwise playable media files.
    """
    import av  # type: ignore
    import numpy as np  # type: ignore

    chunks = []
    with av.open(path, metadata_errors="ignore") as container:
        audio_stream = first_audio_stream(container)
        if audio_stream is None:
            raise RuntimeError("input file has no audio stream")
        resampler = av.AudioResampler(
            format="s16",
            layout="mono",
            rate=WHISPER_SAMPLE_RATE,
        )

        def append_frame(frame):
            samples = frame.to_ndarray().reshape(-1)
            if samples.size:
                chunks.append(np.ascontiguousarray(samples, dtype=np.int16))

        for frame in container.decode(audio_stream):
            for resampled in resampler.resample(frame):
                append_frame(resampled)
        for resampled in resampler.resample(None):
            append_frame(resampled)

    if not chunks:
        raise RuntimeError("input audio stream contains no decodable samples")
    samples = np.concatenate(chunks)
    return samples.astype(np.float32) / 32768.0


def audio_duration_seconds(path):
    try:
        import av  # type: ignore

        with av.open(path) as container:
            if container.duration is not None and container.duration > 0:
                return float(container.duration) / float(av.time_base)
            for stream in container.streams:
                if getattr(stream, "type", "") != "audio":
                    continue
                if (
                    stream.duration is not None
                    and stream.duration > 0
                    and stream.time_base is not None
                ):
                    return float(stream.duration * stream.time_base)
                if (
                    stream.frames is not None
                    and stream.frames > 0
                    and stream.rate is not None
                    and float(stream.rate) > 0.0
                ):
                    return float(stream.frames) / float(stream.rate)
    except Exception:
        pass

    try:
        with wave.open(path, "rb") as wav_file:
            rate = wav_file.getframerate()
            frames = wav_file.getnframes()
            if rate <= 0:
                return 0.0
            return float(frames) / float(rate)
    except Exception:
        return 0.0


def create_transcription_model(model_name, download_root):
    """Use the same stable faster-whisper CPU backend on every macOS architecture."""
    from faster_whisper import WhisperModel  # type: ignore

    model_kwargs = {"device": "cpu", "compute_type": "int8"}
    if download_root:
        model_kwargs["download_root"] = download_root
    return WhisperModel(model_name, **model_kwargs)



def detect_source_language(model, audio, requested_language=""):
    """Detect one source language for the whole job and keep it fixed.

    Whisper's transcribe task should preserve the source language.  Detecting
    once up front prevents independent/chunk-level auto-detection from
    switching language part-way through a file.
    """
    requested = str(requested_language or "").strip().lower()
    if requested:
        return requested, 1.0

    language, probability, _all_probs = model.detect_language(
        audio=audio,
        vad_filter=True,
        language_detection_segments=3,
        language_detection_threshold=0.5,
    )
    language = str(language or "").strip().lower()
    if not language:
        raise RuntimeError("could not detect source audio language")
    return language, float(probability or 0.0)

def active_backend(model):
    return str(getattr(model, "backend", "cpu") or "cpu")


def active_compute_type(model):
    return str(getattr(model, "compute_type", "int8") or "int8")


def format_timestamp(seconds):
    total = max(0, int(seconds))
    hours = total // 3600
    minutes = (total % 3600) // 60
    secs = total % 60
    if hours > 0:
        return f"{hours:02d}:{minutes:02d}:{secs:02d}"
    return f"{minutes:02d}:{secs:02d}"


def choose_quiet_split(samples, target_samples):
    """Return a nearby quiet split and whether it is a real pause."""
    import numpy as np  # type: ignore

    radius = min(
        int(QUIET_SEARCH_RADIUS_SECONDS * WHISPER_SAMPLE_RATE),
        max(1, target_samples // 4),
    )
    window = max(1, int(QUIET_WINDOW_SECONDS * WHISPER_SAMPLE_RATE))
    search_start = max(0, target_samples - radius - window // 2)
    search_end = min(samples.size, target_samples + radius + window // 2)
    region = samples[search_start:search_end]
    if region.size < window:
        return min(target_samples, samples.size), False

    squared = region.astype(np.float64) ** 2
    cumulative = np.concatenate(([0.0], np.cumsum(squared)))
    step = max(1, window // 4)
    best_start = 0
    best_mean_square = float("inf")
    for start in range(0, region.size - window + 1, step):
        mean_square = (cumulative[start + window] - cumulative[start]) / window
        if mean_square < best_mean_square:
            best_mean_square = mean_square
            best_start = start

    split = search_start + best_start + window // 2
    rms_normalized = (best_mean_square**0.5) / 32768.0
    return max(1, min(split, samples.size)), rms_normalized <= QUIET_RMS_THRESHOLD


def iter_resampled_audio_chunks(path, chunk_seconds=LONG_AUDIO_CHUNK_SECONDS):
    """Decode bounded mono/16 kHz blocks, preferring pauses as boundaries."""
    import av  # type: ignore
    import numpy as np  # type: ignore

    chunk_samples = max(1, int(chunk_seconds * WHISPER_SAMPLE_RATE))
    search_samples = min(
        int(QUIET_SEARCH_RADIUS_SECONDS * WHISPER_SAMPLE_RATE),
        max(1, chunk_samples // 4),
    )
    overlap_samples = int(FALLBACK_OVERLAP_SECONDS * WHISPER_SAMPLE_RATE)
    ready_samples = chunk_samples + search_samples
    pending = []
    pending_samples = 0
    buffer_start_samples = 0

    with av.open(path, metadata_errors="ignore") as container:
        audio_stream = first_audio_stream(container)
        if audio_stream is None:
            raise RuntimeError("input file has no audio stream")
        resampler = av.AudioResampler(
            format="s16",
            layout="mono",
            rate=WHISPER_SAMPLE_RATE,
        )

        def append_frame(frame):
            nonlocal pending_samples
            samples = frame.to_ndarray().reshape(-1)
            if samples.size:
                samples = np.ascontiguousarray(samples, dtype=np.int16)
                pending.append(samples)
                pending_samples += int(samples.size)

        for frame in container.decode(audio_stream):
            for resampled in resampler.resample(frame):
                append_frame(resampled)
            if pending_samples >= ready_samples:
                combined = np.concatenate(pending)
                while combined.size >= ready_samples:
                    split, found_quiet = choose_quiet_split(combined, chunk_samples)
                    chunk = combined[:split]
                    yield (
                        chunk.astype(np.float32) / 32768.0,
                        float(buffer_start_samples) / WHISPER_SAMPLE_RATE,
                    )
                    consumed = split
                    if not found_quiet:
                        consumed = max(1, split - min(overlap_samples, split // 4))
                    combined = combined[consumed:]
                    buffer_start_samples += consumed
                pending = [combined.copy()] if combined.size else []
                pending_samples = int(combined.size)

        for resampled in resampler.resample(None):
            append_frame(resampled)

    if pending_samples:
        combined = np.concatenate(pending)
        while combined.size > chunk_samples:
            split, found_quiet = choose_quiet_split(combined, chunk_samples)
            chunk = combined[:split]
            yield (
                chunk.astype(np.float32) / 32768.0,
                float(buffer_start_samples) / WHISPER_SAMPLE_RATE,
            )
            consumed = split
            if not found_quiet:
                consumed = max(1, split - min(overlap_samples, split // 4))
            combined = combined[consumed:]
            buffer_start_samples += consumed
        if combined.size:
            yield (
                combined.astype(np.float32) / 32768.0,
                float(buffer_start_samples) / WHISPER_SAMPLE_RATE,
            )


def append_transcribed_segments(
    segments,
    parts,
    timestamps,
    offset_seconds,
    total_duration,
    last_progress,
):
    for segment in segments:
        text = (segment.text or "").strip()
        if text:
            if timestamps:
                start = offset_seconds + (getattr(segment, "start", 0.0) or 0.0)
                parts.append(f"[{format_timestamp(start)}] {text}")
            else:
                parts.append(text)
        if total_duration > 0 and segment.end is not None:
            end = offset_seconds + float(segment.end)
            pct = int((end / total_duration) * 100.0)
            next_progress = max(0, min(99, pct))
            if next_progress > last_progress:
                last_progress = next_progress
                print(f"PROGRESS:{last_progress}", flush=True)
    return last_progress


def transcribe_long_input(model, input_path, language, timestamps, total_duration):
    parts = []
    last_progress = 0
    selected_language = str(language or "").strip().lower()
    detected_language = selected_language

    for audio_chunk, offset_seconds in iter_resampled_audio_chunks(input_path):
        chunk_duration = float(audio_chunk.shape[0]) / WHISPER_SAMPLE_RATE
        if not selected_language:
            selected_language, probability = detect_source_language(model, audio_chunk)
            detected_language = selected_language
            print(
                f"LANGUAGE:{selected_language}:{probability:.4f}",
                flush=True,
            )
        segments, _info = model.transcribe(
            audio_chunk,
            language=selected_language,
            task="transcribe",
            vad_filter=False,
            beam_size=5,
            condition_on_previous_text=False,
        )
        last_progress = append_transcribed_segments(
            segments,
            parts,
            timestamps,
            offset_seconds,
            total_duration,
            last_progress,
        )
        if total_duration > 0:
            chunk_end = offset_seconds + chunk_duration
            pct = int((chunk_end / total_duration) * 100.0)
            next_progress = max(0, min(99, pct))
            if next_progress > last_progress:
                last_progress = next_progress
                print(f"PROGRESS:{last_progress}", flush=True)

    transcript = ("\n".join(parts) if timestamps else " ".join(parts)).strip()
    return {"ok": True, "text": transcript, "language": detected_language}


def transcribe_input(model, input_path, language, timestamps):
    if not os.path.isfile(input_path):
        return {"ok": False, "error": f"input file not found: {input_path}"}

    total_duration = audio_duration_seconds(input_path)
    if total_duration >= LONG_AUDIO_THRESHOLD_SECONDS:
        return transcribe_long_input(
            model,
            input_path,
            language,
            timestamps,
            total_duration,
        )

    last_progress = 0
    audio = decode_audio_mono_16k(input_path)
    if total_duration <= 0:
        total_duration = float(audio.shape[0]) / WHISPER_SAMPLE_RATE
    selected_language, language_probability = detect_source_language(
        model, audio, language
    )
    print(
        f"LANGUAGE:{selected_language}:{language_probability:.4f}",
        flush=True,
    )
    segments, _info = model.transcribe(
        audio,
        language=selected_language,
        task="transcribe",
        vad_filter=False,
        beam_size=5,
        condition_on_previous_text=False,
    )

    parts = []
    append_transcribed_segments(
        segments,
        parts,
        timestamps,
        0.0,
        total_duration,
        last_progress,
    )

    transcript = ("\n".join(parts) if timestamps else " ".join(parts)).strip()
    return {"ok": True, "text": transcript, "language": selected_language}


def worker_loop(model):
    print_json(
        {
            "ready": True,
            "bridge_version": BRIDGE_PROTOCOL_VERSION,
            "backend": active_backend(model),
            "compute_type": active_compute_type(model),
        }
    )

    for raw_line in sys.stdin:
        line = raw_line.strip()
        if not line:
            continue
        try:
            request = json.loads(line)
        except Exception as exc:
            print_json({"ok": False, "error": f"invalid worker request: {exc}"})
            continue

        command = str(request.get("command", "") or "").strip().lower()
        if command == "shutdown":
            return 0
        if command != "transcribe":
            print_json({"ok": False, "error": f"unsupported worker command: {command}"})
            continue

        try:
            result = transcribe_input(
                model,
                str(request.get("input", "") or ""),
                str(request.get("language", "") or ""),
                bool(request.get("timestamps", False)),
            )
            result["backend"] = active_backend(model)
            result["compute_type"] = active_compute_type(model)
            print_json(result)
        except Exception as exc:
            print_json({"ok": False, "error": f"{type(exc).__name__}: {exc}"})
    return 0


def main():
    configure_stdio_utf8()
    parser = argparse.ArgumentParser()
    parser.add_argument("--bridge-version", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--input", default="", help="Path to audio file")
    parser.add_argument("--model", default="", help="small | medium | large-v3")
    parser.add_argument("--language", default="", help="Language code, e.g. it")
    parser.add_argument("--download-root", default="", help="Model cache directory")
    parser.add_argument("--timestamps", action="store_true", help="Include segment timestamps")
    parser.add_argument("--worker", action="store_true", help="Keep model loaded for multiple requests")
    args = parser.parse_args()

    if args.bridge_version:
        print_json({"bridge_version": BRIDGE_PROTOCOL_VERSION})
        return 0
    if args.self_test:
        try:
            import av  # type: ignore
            import ctranslate2  # type: ignore
            import faster_whisper  # type: ignore
            import onnxruntime  # type: ignore

            payload = {
                "ok": True,
                "bridge_version": BRIDGE_PROTOCOL_VERSION,
                "backend": "cpu",
                "faster_whisper": getattr(faster_whisper, "__version__", ""),
                "ctranslate2": getattr(ctranslate2, "__version__", ""),
                "onnxruntime": getattr(onnxruntime, "__version__", ""),
                "av": getattr(av, "__version__", ""),
            }
            print_json(payload)
            return 0
        except Exception as exc:
            print_json({"ok": False, "error": f"self-test failed: {exc}"})
            return 1
    if not args.model:
        print_json({"ok": False, "error": "model is required"})
        return 1

    if not args.worker and not os.path.isfile(args.input):
        print_json({"ok": False, "error": f"input file not found: {args.input}"})
        return 1

    try:
        print("STAGE:model", flush=True)
        model = create_transcription_model(args.model, args.download_root)
        print(f"BACKEND:{active_backend(model)}", flush=True)
        if args.worker:
            return worker_loop(model)

        print("STAGE:transcribing", flush=True)
        result = transcribe_input(model, args.input, args.language, args.timestamps)
        result["backend"] = active_backend(model)
        result["compute_type"] = active_compute_type(model)
        print_json(result)
        return 0
    except Exception as exc:
        print_json({"ok": False, "error": f"{type(exc).__name__}: {exc}"})
        return 1


if __name__ == "__main__":
    sys.exit(main())

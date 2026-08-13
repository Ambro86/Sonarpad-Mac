#!/usr/bin/env bash
set -euo pipefail

PYTHON_BIN="${PYTHON_BIN:-python3}"
OUT_ROOT="${1:-../dist/faster-whisper-worker}"
FASTER_WHISPER_VERSION="${FASTER_WHISPER_VERSION:-1.2.1}"
CTRANSLATE2_VERSION="${CTRANSLATE2_VERSION:-4.3.1}"
ONNXRUNTIME_VERSION="${ONNXRUNTIME_VERSION:-1.15.0}"
PYAV_VERSION="${PYAV_VERSION:-12.3.0}"
PYINSTALLER_VERSION="${PYINSTALLER_VERSION:-6.20.0}"
TOKENIZERS_VERSION="${TOKENIZERS_VERSION:-0.22.1}"
NUMPY_SPEC="${NUMPY_SPEC:-numpy>=1.24.2,<2}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

"$PYTHON_BIN" -m pip install --upgrade pip
"$PYTHON_BIN" -m pip install \
  "${NUMPY_SPEC}" \
  "ctranslate2==${CTRANSLATE2_VERSION}" \
  "onnxruntime==${ONNXRUNTIME_VERSION}" \
  "av==${PYAV_VERSION}" \
  "tokenizers==${TOKENIZERS_VERSION}" \
  "faster-whisper==${FASTER_WHISPER_VERSION}" \
  "pyinstaller==${PYINSTALLER_VERSION}"

rm -rf build/faster_whisper_bridge dist/faster_whisper_bridge
"$PYTHON_BIN" -m PyInstaller \
  --noconfirm \
  --clean \
  --onedir \
  --collect-data faster_whisper \
  --name faster_whisper_bridge \
  faster_whisper_bridge.py

rm -rf "$OUT_ROOT/faster_whisper_bridge"
mkdir -p "$OUT_ROOT"
cp -R dist/faster_whisper_bridge "$OUT_ROOT/faster_whisper_bridge"
chmod +x "$OUT_ROOT/faster_whisper_bridge/faster_whisper_bridge"

"$OUT_ROOT/faster_whisper_bridge/faster_whisper_bridge" --self-test

echo "Built $OUT_ROOT/faster_whisper_bridge/faster_whisper_bridge"
file "$OUT_ROOT/faster_whisper_bridge/faster_whisper_bridge"

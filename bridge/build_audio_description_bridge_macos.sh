#!/usr/bin/env bash
set -euo pipefail

PYTHON_BIN="${PYTHON_BIN:-python3}"
OUT_ROOT="${1:-../dist/audio-description-worker}"
ONNXRUNTIME_VERSION="${ONNXRUNTIME_VERSION:-1.19.2}"
NUMPY_SPEC="${NUMPY_SPEC:-numpy>=1.26,<2}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

"$PYTHON_BIN" -m pip install --upgrade pip
"$PYTHON_BIN" -m pip install \
  'google-genai' \
  'google-api-core' \
  "onnxruntime==${ONNXRUNTIME_VERSION}" \
  "${NUMPY_SPEC}" \
  'pyinstaller>=6.10,<7'

rm -rf build/audio_description_bridge dist/audio_description_bridge
"$PYTHON_BIN" -m PyInstaller --noconfirm --clean audio_description_bridge_macos.spec

rm -rf "$OUT_ROOT/audio_description_bridge"
mkdir -p "$OUT_ROOT"
cp -R dist/audio_description_bridge "$OUT_ROOT/audio_description_bridge"
chmod +x "$OUT_ROOT/audio_description_bridge/audio_description_bridge"

echo "Built $OUT_ROOT/audio_description_bridge/audio_description_bridge"
file "$OUT_ROOT/audio_description_bridge/audio_description_bridge"

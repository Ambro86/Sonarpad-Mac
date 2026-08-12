#!/usr/bin/env bash
set -euo pipefail

PYTHON_BIN="${PYTHON_BIN:-python3}"
OUT_ROOT="${1:-../dist/audio-description-worker}"
ONNXRUNTIME_VERSION="${ONNXRUNTIME_VERSION:-1.19.2}"
NUMPY_SPEC="${NUMPY_SPEC:-numpy>=1.26,<2}"
# Match the SDK versions embedded in the known-good Windows worker. Newer
# google-genai 2.17.0 can remain blocked in an SSL write on macOS while sending
# inline video, even though the same bridge code completes normally on Windows.
GOOGLE_GENAI_VERSION="${GOOGLE_GENAI_VERSION:-2.12.1}"
GOOGLE_API_CORE_VERSION="${GOOGLE_API_CORE_VERSION:-2.32.0}"
CRYPTOGRAPHY_VERSION="${CRYPTOGRAPHY_VERSION:-48.0.1}"
PYINSTALLER_VERSION="${PYINSTALLER_VERSION:-6.20.0}"
PROTOBUF_PURE_PYTHON="${PROTOBUF_PURE_PYTHON:-0}"
PROTOBUF_VERSION="${PROTOBUF_VERSION:-7.35.1}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

"$PYTHON_BIN" -m pip install --upgrade pip

# The protobuf 7.35.1 macOS native wheel advertises an old platform tag but
# its bundled _upb extension currently declares macOS 12.0. Catalina builds
# opt into the official pure-Python wheel instead; normal macOS builds keep
# the native implementation.
if [[ "$PROTOBUF_PURE_PYTHON" == "1" ]]; then
  PROTOBUF_WHEEL_DIR="$(mktemp -d)"
  trap 'rm -rf "$PROTOBUF_WHEEL_DIR"' EXIT
  "$PYTHON_BIN" -m pip download \
    --no-deps \
    --only-binary=:all: \
    --platform any \
    --dest "$PROTOBUF_WHEEL_DIR" \
    "protobuf==${PROTOBUF_VERSION}"
  PROTOBUF_WHEEL="$(find "$PROTOBUF_WHEEL_DIR" -maxdepth 1 -type f -name 'protobuf-*-py3-none-any.whl' -print -quit)"
  test -n "$PROTOBUF_WHEEL"
  "$PYTHON_BIN" -m pip install "$PROTOBUF_WHEEL"
fi

"$PYTHON_BIN" -m pip install --only-binary=cryptography \
  "google-genai==${GOOGLE_GENAI_VERSION}" \
  "google-api-core==${GOOGLE_API_CORE_VERSION}" \
  "cryptography==${CRYPTOGRAPHY_VERSION}" \
  "onnxruntime==${ONNXRUNTIME_VERSION}" \
  "${NUMPY_SPEC}" \
  "pyinstaller==${PYINSTALLER_VERSION}"

rm -rf build/audio_description_bridge dist/audio_description_bridge
"$PYTHON_BIN" -m PyInstaller --noconfirm --clean audio_description_bridge_macos.spec

rm -rf "$OUT_ROOT/audio_description_bridge"
mkdir -p "$OUT_ROOT"
cp -R dist/audio_description_bridge "$OUT_ROOT/audio_description_bridge"
chmod +x "$OUT_ROOT/audio_description_bridge/audio_description_bridge"

"$OUT_ROOT/audio_description_bridge/audio_description_bridge" --self-test

echo "Built $OUT_ROOT/audio_description_bridge/audio_description_bridge"
file "$OUT_ROOT/audio_description_bridge/audio_description_bridge"

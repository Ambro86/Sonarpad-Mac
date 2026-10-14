#!/usr/bin/env bash
set -euo pipefail

OUT_DIR="${1:?Usage: build_whisper_cpp_macos.sh output-directory}"
mkdir -p "$OUT_DIR"
OUT_DIR="$(cd "$OUT_DIR" && pwd)"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SOURCE_DIR="$SCRIPT_DIR/build/whisper-cpp-src"
BUILD_DIR="$SCRIPT_DIR/build/whisper-cpp-cpu"
COMMIT=2eeeba56e9edd762b4b38467bab96c2517163158
mkdir -p "$SCRIPT_DIR/build"

if [ ! -d "$SOURCE_DIR/.git" ]; then
  git clone --depth 1 --branch v1.8.3 https://github.com/ggml-org/whisper.cpp.git "$SOURCE_DIR"
fi
test "$(git -C "$SOURCE_DIR" rev-parse HEAD)" = "$COMMIT"

# Static CPU backend: no Metal/CoreML, OpenMP or Homebrew dylibs in the app.
# AVX2/FMA/F16C are disabled to keep older Intel CPUs supported. Accelerate
# selects appropriate BLAS kernels at runtime. Never use -march=native.
MACOSX_DEPLOYMENT_TARGET=13.3 cmake -S "$SOURCE_DIR" -B "$BUILD_DIR" \
  -DCMAKE_BUILD_TYPE=Release -DCMAKE_OSX_DEPLOYMENT_TARGET=13.3 \
  -DBUILD_SHARED_LIBS=OFF -DGGML_BACKEND_DL=OFF -DGGML_NATIVE=OFF \
  -DGGML_METAL=OFF -DWHISPER_COREML=OFF -DGGML_OPENMP=OFF \
  -DGGML_BLAS=ON -DGGML_BLAS_VENDOR=Apple -DGGML_ACCELERATE=ON \
  -DGGML_AVX2=OFF -DGGML_AVX512=OFF -DGGML_FMA=OFF -DGGML_F16C=OFF \
  -DWHISPER_BUILD_TESTS=OFF
cmake --build "$BUILD_DIR" --config Release --target whisper-cli -j 3
cp "$BUILD_DIR/bin/whisper-cli" "$OUT_DIR/whisper-cli"
chmod +x "$OUT_DIR/whisper-cli"
cp "$SOURCE_DIR/LICENSE" "$OUT_DIR/whisper-cpp-LICENSE.txt"
"$OUT_DIR/whisper-cli" --help
otool -L "$OUT_DIR/whisper-cli"
if otool -L "$OUT_DIR/whisper-cli" | tail -n +2 | grep -E '/(opt/homebrew|usr/local|Users)/|lib(ggml|whisper|omp).*dylib|/(Metal|MetalPerformanceShaders|CoreML)\.framework/'; then
  echo 'Unexpected external dependency in whisper.cpp CPU executable' >&2
  exit 1
fi

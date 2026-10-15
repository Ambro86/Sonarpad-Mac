#!/usr/bin/env bash
set -euo pipefail

# Pinned upstream source; static linking keeps the app independent of Homebrew.
revision=0480cb05fa188d37ae87e8f4fd8f1aea3711f7ee
archive="$PWD/dist/x264-src/x264-${revision}.tar.gz"
source_dir="$PWD/dist/x264-build/x264-${revision}"
prefix="$PWD/dist/x264-install"
mkdir -p "$(dirname "$archive")" "$source_dir" "$prefix"
if [ ! -f "$archive" ]; then
  curl --fail --location --retry 3 \
    "https://code.videolan.org/videolan/x264/-/archive/${revision}/x264-${revision}.tar.gz" \
    -o "$archive"
fi
echo "d0967a1348c85dfde363bb52610403be898171493100561efa0dd05d5fd1ae50  $archive" | shasum -a 256 -c -
tar -xzf "$archive" --strip-components=1 -C "$source_dir"
pushd "$source_dir"
./configure --prefix="$prefix" --enable-static --enable-pic --disable-cli \
  --disable-opencl --bit-depth=8
make -j"$(sysctl -n hw.ncpu)"
make install
popd

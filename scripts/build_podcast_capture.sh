#!/bin/bash
# Build the same native audio helper for Intel/Catalina and Apple Silicon.
set -euo pipefail
arch="${1:-$(uname -m)}"
minimum="${2:-10.15}"
output="${3:-dist/podcast-capture}"
mkdir -p "$(dirname "$output")"
plist="$(dirname "$output")/podcast-capture-Info.plist"
cat > "$plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>com.sonarpad.minimal.podcast-capture</string>
<key>CFBundleName</key><string>Sonarpad</string>
<key>NSMicrophoneUsageDescription</key><string>Sonarpad records your microphone only when you start recording a podcast.</string>
</dict></plist>
PLIST
# Weak linking is essential: Catalina must launch even though ScreenCaptureKit is absent.
xcrun swiftc -swift-version 5 -O -target "${arch}-apple-macosx${minimum}" \
  -Xfrontend -disable-autolink-framework -Xfrontend ScreenCaptureKit \
  -Xlinker -weak_framework -Xlinker ScreenCaptureKit \
  -Xlinker -sectcreate -Xlinker __TEXT -Xlinker __info_plist -Xlinker "$plist" \
  native/podcast_capture.swift -o "$output"
# -O removes assert, so use -Onone for the actual assertions in the native smoke test.
xcrun swiftc -swift-version 5 -Onone -target "${arch}-apple-macosx${minimum}" \
  -Xfrontend -disable-autolink-framework -Xfrontend ScreenCaptureKit \
  -Xlinker -weak_framework -Xlinker ScreenCaptureKit \
  native/podcast_capture.swift -o "${output}-test"
"${output}-test" --self-test
rm "${output}-test"

# Fail the build if the optional framework was accidentally linked strongly.
otool -l "$output" > "${output}-load-commands.txt"
python3 - "${output}-load-commands.txt" <<'PYCODE'
import sys
from pathlib import Path
commands = Path(sys.argv[1]).read_text().split('Load command ')
found = [c for c in commands if 'ScreenCaptureKit.framework' in c]
assert len(found) == 1 and 'LC_LOAD_WEAK_DYLIB' in found[0], found
PYCODE

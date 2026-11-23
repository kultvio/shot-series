#!/bin/bash
set -euo pipefail

cd -- "$(dirname -- "$0")"
bundle="$PWD/build/ShotSeries.app"
arch="$(uname -m)"
sdk="$(xcrun --sdk macosx --show-sdk-path)"

mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources"
cp Info.plist "$bundle/Contents/Info.plist"
cp media/ShotSeries.icns "$bundle/Contents/Resources/ShotSeries.icns"
xcrun swiftc -swift-version 6 -O -target "$arch-apple-macosx14.0" -sdk "$sdk" \
    -module-cache-path "$PWD/build/module-cache" \
    Sources/*.swift -o "$bundle/Contents/MacOS/ShotSeries"
codesign --force --sign - "$bundle"
printf '%s\n' "$bundle"

#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")"
VERSION="${1:?Usage: bash build-release.sh VERSION (for example 0.1.0)}"
if [[ ! "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "Version must have the form MAJOR.MINOR.PATCH" >&2
  exit 1
fi

# Build each architecture separately so the script also works with Command Line Tools.
swift build -c release --triple arm64-apple-macosx13.0 --product LyricsOverlay
ARM_DIR="$(swift build -c release --triple arm64-apple-macosx13.0 --product LyricsOverlay --show-bin-path)"
swift build -c release --triple x86_64-apple-macosx13.0 --product LyricsOverlay
INTEL_DIR="$(swift build -c release --triple x86_64-apple-macosx13.0 --product LyricsOverlay --show-bin-path)"

bash build-app.sh
APP="$PWD/Lyrics Overlay.app"
lipo -create "$ARM_DIR/LyricsOverlay" "$INTEL_DIR/LyricsOverlay" -output "$APP/Contents/MacOS/LyricsOverlay"
/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $VERSION" "$APP/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleVersion $VERSION" "$APP/Contents/Info.plist"
# Local ad-hoc signature only: this is NOT a Developer ID signature or notarization.
codesign --force --deep --sign - "$APP"
codesign --verify --verbose=2 "$APP"
lipo "$APP/Contents/MacOS/LyricsOverlay" -verify_arch arm64 x86_64
mkdir -p dist
ARCHIVE="$PWD/dist/Lyrics-Overlay-$VERSION-universal.zip"
rm -f "$ARCHIVE"
ditto -c -k --sequesterRsrc --keepParent "$APP" "$ARCHIVE"
echo "Created $ARCHIVE (ad-hoc signed, not notarized)"

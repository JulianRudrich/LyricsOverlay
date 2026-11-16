#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")"
VERSION="${1:-$(cat VERSION)}"
if [[ ! "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "Version must have the form MAJOR.MINOR.PATCH" >&2
  exit 1
fi
swift build -c release --product LyricsOverlay
BIN_DIR="$(swift build -c release --show-bin-path)"
APP="$PWD/Lyrics Overlay.app"
mkdir -p "$APP/Contents/MacOS"
mkdir -p "$APP/Contents/Resources"
cp "$BIN_DIR/LyricsOverlay" "$APP/Contents/MacOS/LyricsOverlay"
cp LICENSE "$APP/Contents/Resources/LICENSE"
cp assets/brand/AppIcon.icns "$APP/Contents/Resources/AppIcon.icns"
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleExecutable</key><string>LyricsOverlay</string>
  <key>CFBundleIdentifier</key><string>de.local.lyricsoverlay</string>
  <key>CFBundleName</key><string>Lyrics Overlay</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>LSMinimumSystemVersion</key><string>13.0</string>
  <key>LSUIElement</key><true/>
  <key>NSAppleEventsUsageDescription</key><string>Lyrics Overlay liest den aktuellen Song und die Wiedergabeposition aus Spotify oder Apple Music, um synchronisierte Lyrics anzuzeigen.</string>
</dict></plist>
PLIST
codesign --force --deep --sign - "$APP"
echo "Fertig: $APP"

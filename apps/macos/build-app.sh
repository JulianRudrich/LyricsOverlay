#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")"
VERSION="$(tr -d '[:space:]' < VERSION)"
BUILD_PATH="$PWD/.build/local-app"
swift build -c release --scratch-path "$BUILD_PATH"
BIN_DIR="$(swift build -c release --scratch-path "$BUILD_PATH" --show-bin-path)"
APP="$BUILD_PATH/bundle/Lyrics Overlay.app"
DESTINATION="${LYRICS_OVERLAY_APP_DIR:-$HOME/Applications}/Lyrics Overlay.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN_DIR/LyricsOverlay" "$APP/Contents/MacOS/LyricsOverlay"
cp assets/AppIcon.icns "$APP/Contents/Resources/AppIcon.icns"
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleExecutable</key><string>LyricsOverlay</string>
  <key>CFBundleIdentifier</key><string>de.local.lyricsoverlay</string>
  <key>CFBundleName</key><string>Lyrics Overlay</string>
  <key>CFBundleDisplayName</key><string>Lyrics Overlay</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>LSMinimumSystemVersion</key><string>13.0</string>
  <key>LSUIElement</key><true/>
  <key>NSAppleEventsUsageDescription</key><string>Lyrics Overlay reads the current track from Spotify or Music to show synchronized lyrics, and starts your True Shuffle playlist in Spotify.</string>
</dict></plist>
PLIST
codesign --force --deep --sign - "$APP"
mkdir -p "$(dirname "$DESTINATION")"
ditto "$APP" "$DESTINATION"
codesign --verify --strict "$DESTINATION"
echo "Done: $DESTINATION"
# A copy that is still running keeps the old code; restart it so the new build is what you see.
if pgrep -xq LyricsOverlay; then pkill -x LyricsOverlay; sleep 0.5; open "$DESTINATION"; fi

#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")"
swift build -c release
BIN_DIR="$(swift build -c release --show-bin-path)"
APP="$PWD/Lyrics Overlay.app"
mkdir -p "$APP/Contents/MacOS"
cp "$BIN_DIR/LyricsOverlay" "$APP/Contents/MacOS/LyricsOverlay"
cat > "$APP/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleExecutable</key><string>LyricsOverlay</string>
  <key>CFBundleIdentifier</key><string>de.local.lyricsoverlay</string>
  <key>CFBundleName</key><string>Lyrics Overlay</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>1.0</string>
  <key>CFBundleVersion</key><string>1</string>
  <key>LSMinimumSystemVersion</key><string>13.0</string>
  <key>LSUIElement</key><true/>
  <key>NSAppleEventsUsageDescription</key><string>Lyrics Overlay liest den aktuellen Spotify-Song und die Wiedergabeposition, um synchronisierte Lyrics anzuzeigen.</string>
</dict></plist>
PLIST
codesign --force --deep --sign - "$APP"
echo "Fertig: $APP"

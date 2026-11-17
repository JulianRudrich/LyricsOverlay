# Lyrics Overlay — Unreleased

## New

- **Apple Music support.** Choose the source under Settings → General or in the menu-bar menu: Auto, Spotify or Apple Music. Auto follows the playing app. Players that are not running are never launched.
- **Mini-player.** Hover over the overlay for ♥, previous, play/pause and next. The progress line becomes a scrubber with elapsed and remaining time. Click a lyric line to jump there, or a chorus dot to jump to the chorus. Click the cover or title of a Spotify track to open it in Spotify.
- **New overlay and menu.** Scrolling lyrics with word-by-word highlighting, smooth song changes, a glass menu-bar panel and a new app icon. The menu opens on any menu-bar click; ⌘W closes a window and ⌘Q quits.
- **True Shuffle (Spotify, optional).** Shuffle Liked Songs or your own playlists, alone or mixed, with no repeats until everything has played. Artists are kept apart, Keep playing adds the next round before the music stops, and Up next shows what follows. Remove, restore and keep a shuffle as its own playlist. Needs Spotify Premium and your own free Spotify developer app.
- **Your music.** Listening time, plays, streak, skips, a listening clock and your most played songs and artists, recorded locally while the app runs. With Spotify connected, also Spotify's own top lists.
- **Sleep timer.** Pause after 15 to 60 minutes or at the end of the track.
- **Exact sync.** Position readings are timed from when the player reported them, small drift is smoothed out, and the lyrics hold after a seek until playback resumes.
- **English and German interface**, following the macOS language, with an override in Settings.
- **More reliable lyrics lookup.** Exact LRCLIB lookup by title, artist, album and duration, then a search. Results are cached locally, so replaying a song works offline.
- **Plain-lyrics fallback.** Songs without synced lyrics show their plain lyrics spread over the song, marked "~ not synced". Instrumental tracks are labeled.
- **Launch at login**, **auto-hide when nothing is playing** (off by default), and **click-through is remembered** across relaunches.

## Download

Download `Lyrics-Overlay-0.2.0-universal.zip`, extract it and open `Lyrics Overlay.app`. **This preview is ad-hoc signed, not signed with an Apple Developer ID, and not notarized.** Gatekeeper may prevent it from opening. Only run it if you trust the source; consult [Apple's instructions](https://support.apple.com/guide/mac-help/open-a-mac-app-from-an-unidentified-developer-mh40616/mac) if macOS blocks it. Do not disable Gatekeeper globally.

Song title, artist, album and duration are sent to LRCLIB to find lyrics. Listening history stays on your Mac. See [Privacy](docs/PRIVACY.md) for details.

---

# Lyrics Overlay v0.1.0 — early preview

A native macOS overlay for synced Spotify lyrics, with a global show/hide shortcut, adjustable layout and opacity, and lyrics from LRCLIB.

## Download

Download `Lyrics-Overlay-0.1.0-universal.zip`, extract it and open `Lyrics Overlay.app`. The binary contains both Apple Silicon and Intel architectures, but it has not yet been run on an Intel Mac or tested on a second Mac. macOS 13 is the build target, not a verified minimum on all systems.

**This preview is ad-hoc signed, not signed with an Apple Developer ID, and not notarized.** Gatekeeper may prevent it from opening. Only run it if you trust the source; consult [Apple's instructions](https://support.apple.com/guide/mac-help/open-a-mac-app-from-an-unidentified-developer-mh40616/mac) if macOS blocks it. Do not disable Gatekeeper globally.

Requirements: Spotify desktop app, internet access, and permission for Lyrics Overlay to control Spotify. Song titles and artists are sent to LRCLIB to find lyrics. Synchronized lyrics are not available for every track. See the README for details and privacy information.

This is an early preview; please report problems in GitHub Issues.

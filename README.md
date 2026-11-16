# Lyrics Overlay

Follow the words while you work. Adjust the timing when a line arrives early. Add the lyrics yourself when a track doesn't have them.

Lyrics Overlay is a native macOS app that keeps synchronized lyrics above your other windows. This repository contains the original project by Julian Rudrich.

> **Looking at the 0.2 preview.** The features below are available on the [preview branch](../../tree/preview/lyrics-overlay-0.2). The default branch and latest published release currently contain the earlier Spotify-only 0.1.0. The preview is still awaiting real-world Mac playback testing.

## An overlay you can adjust

Lyrics Overlay keeps the current line in a small window above your other apps. Pick how much you want to see, place it where it fits, and keep listening.

### Choose how much you see

Show one line, the current and next line, or the lines around it. Adjust the type size, make the background subtle or transparent, and move the overlay to a corner.

Let clicks pass through to the window underneath. Show or hide lyrics with **⌥ ⇧ L**. Switch between Spotify and Apple Music from the menu bar, or let the app follow the active player.

### Fix timing once

A song a little ahead or behind? Adjust it in quarter-second steps. Your correction stays with that song for the next listen.

### Bring your own lyrics

Import an LRC file, or paste a song and add timestamps as you listen. Your own lyrics take precedence and remain available offline.

## Try it on macOS

### Download the preview

1. Open the latest successful run for this branch in [GitHub Actions](../../actions/workflows/ci.yml).
2. Download **Lyrics-Overlay-0.2.0-universal** from its Artifacts section. GitHub requires sign-in.
3. Extract the archive and open **Lyrics Overlay.app**.
4. Play a song in Spotify or Music and allow macOS Automation access when prompted.

The preview is ad-hoc signed and has not been notarized with an Apple Developer ID. Gatekeeper may block the download. If you trust the build, follow [Apple's instructions for opening an app from an unidentified developer](https://support.apple.com/guide/mac-help/open-a-mac-app-from-an-unidentified-developer-mh40616/mac).

### Build from source or try the demo

Requires macOS 13 or later, the Apple Command Line Tools, and the desktop version of Spotify or Apple's Music app for live playback. The example below launches a built-in demo without a player or network requests.

```sh
git clone --branch preview/lyrics-overlay-0.2 https://github.com/JulianRudrich/LyricsOverlay.git
cd LyricsOverlay
bash build-app.sh
open "Lyrics Overlay.app" --args --demo
```

Quit the demo and open `Lyrics Overlay.app` again for live playback. Play a song and approve the macOS Automation prompt for the player. Spotify and Apple Music are read locally. Lyrics Overlay uses neither player API keys nor a separate login.

- **Choose a player** in **Song & Timing**. Automatic mode follows an active player. You can also lock the source to Spotify or Apple Music.
- **Open settings** with the waveform button in the overlay or a left-click on the menu-bar icon.
- **Move or hide the overlay** from the menu-bar icon's context menu. **⌥ ⇧ L** toggles visibility from any app.
- **Switch players** from the menu bar. Click-through is remembered, and can be turned off from the same menu. Reopening the app also restores mouse interaction.
- **Launch at login** can be enabled in the appearance settings. macOS may ask for approval in Login Items.

The [0.1.0 release](../../releases/tag/v0.1.0) is the earlier Spotify-only preview.

## Use your own lyrics

1. Play the song you want to edit and open **Song & Timing**.
2. Choose **LRC importieren** for an existing file, or **Lyrics bearbeiten** to paste or edit the text.
3. In the editor, use **Zeitstempel setzen** or **⇧⌘T** on the current line as you listen. Save once the text has timestamps.
4. Use the **±0.25 s** buttons if the whole song needs a small timing correction. Positive values bring the text forward.

Own lyrics take precedence for the current recording. You can remove them, reset timing independently, or disable online search and use only your files. Imports support UTF-8 LRC up to 512 KB per track. An empty timed line marks an instrumental break.

## Need a hand?

- **The player is open, but the overlay cannot read it.** Check **System Settings → Privacy & Security → Automation** and enable the player for Lyrics Overlay.
- **Apple Music is playing, but the track is unavailable.** On some macOS versions, Music can fail to expose playback metadata for songs outside your library. Add the track to your library and play it from there. [Apple Developer Forums](https://developer.apple.com/forums/thread/798267)
- **A song has no synced lyrics.** Try again from the overlay, or import your own LRC. Online results depend on availability in LRCLIB and a match to the recording.
- **Clicks go through the overlay.** Right-click the menu-bar icon and disable **Klicks durchlassen** before moving it or opening settings from it.

## Privacy

Online lookup sends track title, artist, album when available, and duration to LRCLIB over HTTPS. The results cache stays in memory. Song offsets are stored in macOS preferences, and your own lyrics stay in `~/Library/Application Support/Lyrics Overlay/LRC`. The app does not upload your edits. [LRCLIB](https://lrclib.net/)

## Development

```sh
swift build
swift run LyricsCoreChecks
bash build-release.sh "$(cat VERSION)"
```

The [macOS workflow](../../actions/workflows/ci.yml) builds the app, runs the core checks, and verifies the universal arm64 and x86_64 preview archive. Test playback, Automation permission, and overlay behavior with both players on a Mac before distributing a build. See the [contribution guide](CONTRIBUTING.md).

Created by [Julian Rudrich](https://github.com/JulianRudrich). The code is licensed under [MIT](LICENSE). Song lyrics belong to their respective rights holders.
